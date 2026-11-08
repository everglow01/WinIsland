use std::collections::HashMap;

use skia_safe::{Font, FontStyle as SkFontStyle, Typeface};

use super::{
    FONT_MGR, FontManager, fallback_typeface, text_paint, typeface_supports_char, user_typeface,
};
use crate::painter::Painter;
use crate::types::{Rect, Rgba};

#[derive(Clone, Copy)]
pub struct PluginTextMetrics {
    pub width: f32,
    pub height: f32,
    pub ascent: f32,
    pub descent: f32,
}

#[derive(Clone, Copy)]
pub struct PluginTextParams<'a> {
    pub painter: Painter<'a>,
    pub rect: Rect,
    pub size: f32,
    pub italic: bool,
    pub family: &'a str,
    pub align: u8,
    pub wrap: bool,
    pub ellipsis: bool,
}

#[derive(Clone, Copy)]
pub struct PluginTextRun<'a> {
    pub text: &'a str,
    pub weight: u16,
    pub color: Rgba,
}

#[derive(Clone, Copy)]
struct Glyph {
    character: char,
    weight: u16,
    color: Rgba,
    width: f32,
    /// `Typeface::unique_id` of the face that has this character.
    face: u32,
}

fn slanted(italic: bool) -> skia_safe::font_style::Slant {
    if italic {
        skia_safe::font_style::Slant::Italic
    } else {
        skia_safe::font_style::Slant::Upright
    }
}

impl FontManager {
    fn plugin_typeface(&self, style: SkFontStyle, family: &str) -> Option<Typeface> {
        if family.is_empty()
            && let Some(typeface) = user_typeface()
        {
            return Some(typeface);
        }
        FONT_MGR.with(|manager| {
            (!family.is_empty())
                .then(|| manager.match_family_style(family, style))
                .flatten()
                .or_else(|| manager.match_family_style("Microsoft YaHei", style))
                .or_else(|| manager.match_family_style("Segoe UI", style))
                .or_else(|| manager.legacy_make_typeface(None, style))
        })
    }

    fn plugin_font(&self, typeface: &Typeface, size: f32, weight: u16, italic: bool) -> Font {
        let mut font = Font::from_typeface(typeface.clone(), size);
        font.set_subpixel(true);
        if weight >= 600 && *typeface.font_style().weight() < 600 {
            font.set_embolden(true);
        }
        if italic && typeface.font_style().slant() == skia_safe::font_style::Slant::Upright {
            font.set_skew_x(-0.25);
        }
        font
    }

    /// The face that can draw `character`, matching the fallback the host text path uses so a
    /// plugin does not show missing-glyph boxes for scripts its interface font lacks.
    fn plugin_char_typeface(&self, primary: &Typeface, character: char) -> Typeface {
        if typeface_supports_char(primary, character) {
            primary.clone()
        } else {
            fallback_typeface(character)
        }
    }

    pub fn measure_plugin_text(
        &self,
        text: &str,
        size: f32,
        weight: u16,
        italic: bool,
        family: &str,
    ) -> Option<PluginTextMetrics> {
        let style = SkFontStyle::new(i32::from(weight).into(), 5.into(), slanted(italic));
        let primary = self.plugin_typeface(style, family)?;
        let mut width = 0.0;
        let mut buffer = [0_u8; 4];
        for character in text.chars() {
            let typeface = self.plugin_char_typeface(&primary, character);
            let font = self.plugin_font(&typeface, size, weight, italic);
            width += font.measure_str(character.encode_utf8(&mut buffer), None).0;
        }
        let (_, metrics) = self.plugin_font(&primary, size, weight, italic).metrics();
        Some(PluginTextMetrics {
            width,
            height: metrics.descent - metrics.ascent,
            ascent: -metrics.ascent,
            descent: metrics.descent,
        })
    }

    pub fn plugin_font_family(&self, index: u32) -> Option<String> {
        FONT_MGR.with(|manager| {
            ((index as usize) < manager.count_families())
                .then(|| manager.family_name(index as usize))
        })
    }

    pub fn draw_plugin_text(
        &self,
        params: PluginTextParams<'_>,
        text: &str,
        weight: u16,
        color: Rgba,
    ) -> bool {
        self.draw_plugin_runs(
            params,
            &[PluginTextRun {
                text,
                weight,
                color,
            }],
        )
    }

    pub fn draw_plugin_runs(
        &self,
        params: PluginTextParams<'_>,
        runs: &[PluginTextRun<'_>],
    ) -> bool {
        if params.rect.width() <= 0.0 || params.rect.height() <= 0.0 {
            return true;
        }
        let base_style = SkFontStyle::new(400.into(), 5.into(), slanted(params.italic));
        let Some(primary) = self.plugin_typeface(base_style, params.family) else {
            return false;
        };
        let base_font = self.plugin_font(&primary, params.size, 400, params.italic);
        let (_, metrics) = base_font.metrics();
        let line_height = (metrics.descent - metrics.ascent).max(params.size * 1.1);
        let max_lines = ((params.rect.height() / line_height).ceil() as usize).clamp(1, 256);
        let mut fonts: HashMap<(u32, u16), Font> = HashMap::new();
        let mut lines: Vec<Vec<Glyph>> = vec![Vec::new()];
        let mut widths = vec![0.0_f32];
        let mut truncated = false;
        'runs: for run in runs {
            for character in run.text.chars() {
                if character == '\n' {
                    if lines.len() >= max_lines {
                        truncated = true;
                        break 'runs;
                    }
                    lines.push(Vec::new());
                    widths.push(0.0);
                    continue;
                }
                let typeface = self.plugin_char_typeface(&primary, character);
                let key = (typeface.unique_id(), run.weight);
                let font = fonts.entry(key).or_insert_with(|| {
                    self.plugin_font(&typeface, params.size, run.weight, params.italic)
                });
                let mut buffer = [0_u8; 4];
                let width = font.measure_str(character.encode_utf8(&mut buffer), None).0;
                let index = lines.len() - 1;
                if widths[index] + width > params.rect.width() && !lines[index].is_empty() {
                    if !params.wrap || lines.len() >= max_lines {
                        truncated = true;
                        break 'runs;
                    }
                    lines.push(Vec::new());
                    widths.push(0.0);
                }
                let index = lines.len() - 1;
                widths[index] += width;
                lines[index].push(Glyph {
                    character,
                    weight: run.weight,
                    color: run.color,
                    width,
                    face: key.0,
                });
            }
        }
        if truncated && params.ellipsis {
            let index = lines.len() - 1;
            let weight = lines[index].last().map_or(400, |glyph| glyph.weight);
            let color = lines[index].last().map_or(Rgba::WHITE, |glyph| glyph.color);
            let face = lines[index]
                .last()
                .map_or_else(|| primary.unique_id(), |glyph| glyph.face);
            let ellipsis_width = fonts
                .entry((face, weight))
                .or_insert_with(|| {
                    let typeface = self.plugin_char_typeface(&primary, '…');
                    self.plugin_font(&typeface, params.size, weight, params.italic)
                })
                .measure_str("…", None)
                .0;
            while widths[index] + ellipsis_width > params.rect.width() {
                let Some(glyph) = lines[index].pop() else {
                    break;
                };
                widths[index] -= glyph.width;
            }
            if ellipsis_width <= params.rect.width() {
                lines[index].push(Glyph {
                    character: '…',
                    weight,
                    color,
                    width: ellipsis_width,
                    face,
                });
                widths[index] += ellipsis_width;
            }
        }
        params.painter.save();
        params.painter.clip_rect(params.rect);
        for (index, line) in lines.iter().enumerate() {
            let mut x = match params.align {
                1 => params.rect.left + (params.rect.width() - widths[index]) * 0.5,
                2 => params.rect.right - widths[index],
                _ => params.rect.left,
            };
            let baseline = params.rect.top - metrics.ascent + index as f32 * line_height;
            let mut start = 0;
            while start < line.len() {
                let weight = line[start].weight;
                let color = line[start].color;
                let face = line[start].face;
                let mut end = start + 1;
                while end < line.len()
                    && line[end].weight == weight
                    && line[end].color == color
                    && line[end].face == face
                {
                    end += 1;
                }
                let value: String = line[start..end]
                    .iter()
                    .map(|glyph| glyph.character)
                    .collect();
                let Some(font) = fonts.get(&(face, weight)) else {
                    start = end;
                    continue;
                };
                params.painter.canvas().draw_str(
                    &value,
                    (x, baseline),
                    font,
                    &text_paint(color, None),
                );
                x += font.measure_str(&value, None).0;
                start = end;
            }
        }
        params.painter.restore();
        true
    }
}
