use std::cell::RefCell;
use std::collections::HashMap;

use harfrust::{Buffer, Script, ShapeOptions, ShaperFont, Tag};
use skia_safe::{Canvas, Font, GlyphId, Paint, Path, PathBuilder, Point, Rect, Typeface};

const FONT_CACHE_LIMIT: usize = 4;

thread_local! {
    static FONTS: RefCell<HashMap<u32, Option<harfrust::Font>>> = RefCell::new(HashMap::new());
}

pub(super) fn clear_font_cache() {
    FONTS.with(|fonts| fonts.borrow_mut().clear());
}

pub(super) fn script(character: char) -> Option<Script> {
    let script = harfrust::unicode::script(u32::from(character));
    (!matches!(script, Script::COMMON | Script::INHERITED | Script::UNKNOWN)).then_some(script)
}

fn font_data(typeface: &Typeface) -> Option<harfrust::Font> {
    let id = typeface.unique_id();
    FONTS.with(|fonts| {
        let mut fonts = fonts.borrow_mut();
        if let Some(font) = fonts.get(&id) {
            return font.clone();
        }
        let font = typeface.to_font_data().and_then(|(data, index)| {
            let font = harfrust::Font::new(data, u32::try_from(index).ok()?)?;
            let variations = typeface.variation_design_position().unwrap_or_default();
            Some(
                font.instance_builder()
                    .variations(variations.into_iter().map(|coordinate| {
                        harfrust::font::Variation::new(
                            Tag::from_be_bytes(coordinate.axis.to_be_bytes()),
                            coordinate.value,
                        )
                    }))
                    .build(),
            )
        });
        super::evict_one_if_full(&mut fonts, FONT_CACHE_LIMIT);
        fonts.insert(id, font.clone());
        font
    })
}

pub(super) struct ShapedRun {
    glyphs: Vec<GlyphId>,
    positions: Vec<Point>,
    clusters: Vec<(usize, f32)>,
    pub(super) width: f32,
}

impl ShapedRun {
    pub(super) fn new(text: &str, typeface: &Typeface, size: f32) -> Option<Self> {
        if text.is_ascii()
            || !text
                .chars()
                .any(|character| script(character) == Some(Script::BENGALI))
        {
            return None;
        }
        let font = font_data(typeface)?;
        let units = font.units_per_em();
        if units == 0 || !size.is_finite() || size <= 0.0 {
            return None;
        }
        let mut buffer = Buffer::new();
        buffer.push_str(text);
        buffer.guess_segment_properties();
        if let Err(error) = harfrust::shape(
            &ShaperFont::new(&font),
            &mut buffer,
            ShapeOptions::default(),
        ) {
            log::warn!("Cannot shape lyric text: {error}");
            return None;
        }
        let scale = size / f32::from(units);
        let mut run = Self {
            glyphs: Vec::with_capacity(buffer.glyph_infos().len()),
            positions: Vec::with_capacity(buffer.glyph_infos().len()),
            clusters: Vec::new(),
            width: 0.0,
        };
        let mut y = 0.0;
        for (info, position) in buffer.glyph_infos().iter().zip(buffer.glyph_positions()) {
            run.glyphs.push(u16::try_from(info.glyph_id).ok()?);
            run.positions.push(Point::new(
                run.width + position.x_offset as f32 * scale,
                -y - position.y_offset as f32 * scale,
            ));
            run.width += position.x_advance as f32 * scale;
            y += position.y_advance as f32 * scale;
            let cluster = usize::try_from(info.cluster).ok()?;
            match run.clusters.last_mut() {
                Some((last, width)) if *last == cluster => *width = run.width,
                _ => run.clusters.push((cluster, run.width)),
            }
        }
        Some(run)
    }

    pub(super) fn prefix_width(&self, end: usize) -> f32 {
        self.clusters
            .iter()
            .take_while(|(cluster, _)| *cluster < end)
            .last()
            .map_or(0.0, |(_, width)| *width)
    }

    pub(super) fn draw(&self, canvas: &Canvas, origin: (f32, f32), font: &Font, paint: &Paint) {
        canvas.draw_glyphs_at(&self.glyphs, self.positions.as_slice(), origin, font, paint);
    }

    pub(super) fn bounds(&self, font: &Font, paint: Option<&Paint>) -> Rect {
        let mut bounds = vec![Rect::default(); self.glyphs.len()];
        font.get_bounds(&self.glyphs, &mut bounds, paint);
        bounds
            .into_iter()
            .zip(&self.positions)
            .map(|(bounds, position)| bounds.with_offset((position.x, position.y)))
            .reduce(Rect::join2)
            .unwrap_or_default()
    }

    pub(super) fn path(&self, font: &Font, x: f32) -> Path {
        let mut path = PathBuilder::new();
        for (&glyph, position) in self.glyphs.iter().zip(&self.positions) {
            if let Some(glyph) = font.get_path(glyph) {
                path.add_path_with_offset(&glyph, (x + position.x, position.y), None);
            }
        }
        path.snapshot()
    }
}
