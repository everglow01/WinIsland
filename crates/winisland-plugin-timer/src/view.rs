use std::time::{Duration, Instant};

use winisland_plugin_api::TextMetricsV2;
use winisland_plugin_api::sdk::{
    Deg12, DrawListBuilder, Error, Host, Rect as DrawRect, Rgba, Size, TextStyle,
};

use crate::geometry::{Point, Rect, Vec2};
use crate::locale::Text;
use crate::state::{
    self, Action, Phase, PrimaryStyle, State, TimeField, advance, layout, presets_enabled,
    progress, remaining, ring_stroke, time_center, time_size,
};

const ORANGE: Rgba = Rgba::from_argb(0xffff_9f0a);
const GREEN: Rgba = Rgba::from_argb(0xff30_d158);
const YELLOW: Rgba = Rgba::from_argb(0xffff_cc00);
const DIGIT_WEIGHT: u16 = 600;
const MEDIUM: u16 = 500;
const SEMIBOLD: u16 = 600;
const DIGIT_DURATION: Duration = Duration::from_millis(300);
const MAX_DIGITS: usize = 8;
const WIDTH_RATE: f32 = 14.0;
const ICON_SIZE: f32 = 18.0;
const EDGE_INSET: f32 = 9.0;
pub(crate) const COMPACT_FONT_SIZE: f32 = 13.0;

fn with_alpha(color: Rgba, factor: f32) -> Rgba {
    color.with_alpha((factor.clamp(0.0, 1.0) * 255.0).round() as u8)
}

fn alpha_of(color: Rgba) -> f32 {
    f32::from(((color.argb() >> 24) & 0xff) as u8) / 255.0
}

fn blend(from: Rgba, to: Rgba, amount: f32) -> Rgba {
    let (a, b) = (from.argb(), to.argb());
    let channel = |shift: u32| {
        let start = ((a >> shift) & 0xff) as f32;
        let end = ((b >> shift) & 0xff) as f32;
        ((start + (end - start) * amount).round().clamp(0.0, 255.0)) as u8
    };
    Rgba::from_argb(
        (u32::from(channel(24)) << 24)
            | (u32::from(channel(16)) << 16)
            | (u32::from(channel(8)) << 8)
            | u32::from(channel(0)),
    )
}

pub(crate) struct View<'a> {
    host: &'a Host,
    list: DrawListBuilder,
    clips: usize,
    error: Option<Error>,
}

impl<'a> View<'a> {
    pub(crate) fn new(host: &'a Host, width: f32, height: f32) -> Self {
        Self {
            host,
            list: DrawListBuilder::new(Size::new(width.max(1.0), height.max(1.0))),
            clips: 0,
            error: None,
        }
    }

    pub(crate) fn finish(self) -> Result<DrawListBuilder, Error> {
        match self.error {
            Some(error) => Err(error),
            None => Ok(self.list),
        }
    }

    fn measure(&mut self, text: &str, size: f32, weight: u16) -> TextMetricsV2 {
        let mut style = TextStyle::default_at(size);
        style.weight = weight;
        match self
            .host
            .text()
            .and_then(|api| api.measure_style(text, &style))
        {
            Ok(metrics) => metrics,
            Err(error) => {
                self.error = Some(error);
                TextMetricsV2::default()
            }
        }
    }

    fn width(&mut self, text: &str, size: f32, weight: u16) -> f32 {
        self.measure(text, size, weight).width
    }

    /// Time text width with fixed digit cells, matching how the digits are drawn.
    fn time_width(&mut self, text: &str, size: f32) -> f32 {
        let digit = self.width("0", size, DIGIT_WEIGHT);
        let mut total = 0.0;
        for character in text.chars() {
            total += if character.is_ascii_digit() {
                digit
            } else {
                self.width(&character.to_string(), size, DIGIT_WEIGHT)
                    .max(size * 0.25)
            };
        }
        total
    }

    fn text_at(&mut self, text: &str, x: f32, top: f32, size: f32, weight: u16, color: Rgba) {
        let mut style = TextStyle::default_at(size);
        style.weight = weight;
        let metrics = self.measure(text, size, weight);
        // The host lays text out glyph by glyph and stops at the first glyph that would exceed
        // the rect, so a rect that exactly fits the measured width can drop the last glyph to
        // rounding. Leave room and let the surrounding clip decide.
        let height = metrics.height.max(size) + size * 0.5;
        let width = metrics.width.max(1.0) + size * 2.0;
        self.list
            .text(text, DrawRect::new(x, top, width, height), &style, color);
    }

    fn text_center(&mut self, text: &str, bounds: Rect, size: f32, weight: u16, color: Rgba) {
        let metrics = self.measure(text, size, weight);
        let height = metrics.height.max(size);
        self.text_at(
            text,
            bounds.center_x() - metrics.width / 2.0,
            bounds.center_y() - height / 2.0,
            size,
            weight,
            color,
        );
    }

    fn fill_round_rect(&mut self, rect: Rect, radius: f32, color: Rgba) {
        self.list.fill_round_rect(to_draw(rect), radius, color);
    }

    fn stroke_round_rect(&mut self, rect: Rect, radius: f32, width: f32, color: Rgba) {
        self.list
            .stroke_round_rect(to_draw(rect), radius, width, color);
    }

    fn fill_circle(&mut self, center: Point, radius: f32, color: Rgba) {
        self.list.fill_circle(center.x, center.y, radius, color);
    }

    fn stroke_arc(&mut self, rect: Rect, start: f32, sweep: f32, width: f32, color: Rgba) {
        self.list
            .stroke_arc(to_draw(rect), Deg12(start), Deg12(sweep), width, color);
    }

    /// Strokes an arc and caps both ends, which the draw-list arc cannot do on its own.
    fn stroke_progress(
        &mut self,
        center: Point,
        radius: f32,
        fraction: f32,
        width: f32,
        color: Rgba,
    ) {
        if radius <= 0.0 || width <= 0.0 {
            return;
        }
        let rect = circle_rect(center, radius);
        if fraction >= 1.0 {
            self.stroke_arc(rect, 0.0, 360.0, width, color);
            return;
        }
        if fraction <= 0.0 {
            return;
        }
        self.stroke_arc(rect, 0.0, fraction * 360.0, width, color);
        let cap = width / 2.0;
        let angle = fraction * std::f32::consts::TAU;
        self.fill_circle(Point::new(center.x, center.y - radius), cap, color);
        self.fill_circle(
            Point::new(
                center.x + angle.sin() * radius,
                center.y - angle.cos() * radius,
            ),
            cap,
            color,
        );
    }

    fn stroke_line(&mut self, from: Point, to: Point, width: f32, color: Rgba) {
        self.list
            .stroke_line((from.x, from.y), (to.x, to.y), width, color);
    }

    fn clip_rect(&mut self, rect: Rect) {
        self.list.clip_rect(to_draw(rect));
        self.clips += 1;
    }

    fn save(&self) -> usize {
        self.clips
    }

    fn restore_to(&mut self, count: usize) {
        while self.clips > count {
            self.list.pop_clip();
            self.clips -= 1;
        }
    }
}

fn to_draw(rect: Rect) -> DrawRect {
    DrawRect::new(rect.left, rect.top, rect.width(), rect.height())
}

fn circle_rect(center: Point, radius: f32) -> Rect {
    Rect::from_xywh(
        center.x - radius,
        center.y - radius,
        radius * 2.0,
        radius * 2.0,
    )
}

#[derive(Clone, Copy)]
struct DigitChange {
    previous: char,
    started: Instant,
}

#[derive(Clone, Copy)]
pub(crate) enum Anchor {
    Center(Point),
    Right(Point),
}

pub(crate) struct Rolling {
    digits: [char; MAX_DIGITS],
    digit_count: usize,
    changes: [Option<DigitChange>; MAX_DIGITS],
    departing: [Option<DigitChange>; MAX_DIGITS],
    width: f32,
    target_width: f32,
    last_frame: Option<Instant>,
}

impl Rolling {
    pub(crate) fn new() -> Self {
        Self {
            digits: [' '; MAX_DIGITS],
            digit_count: 0,
            changes: [None; MAX_DIGITS],
            departing: [None; MAX_DIGITS],
            width: 0.0,
            target_width: 0.0,
            last_frame: None,
        }
    }

    fn advance(&mut self, text: &str, now: Instant) {
        for change in self.changes.iter_mut().chain(self.departing.iter_mut()) {
            if change.is_some_and(|change| now.duration_since(change.started) >= DIGIT_DURATION) {
                *change = None;
            }
        }
        let chars: Vec<char> = text.chars().take(MAX_DIGITS).collect();
        let old_count = self.digit_count;
        let new_count = chars.len();
        if old_count > 0 && old_count != new_count {
            let mut shifted = [None; MAX_DIGITS];
            for (index, slot) in shifted.iter_mut().enumerate().take(new_count) {
                let old_index = (index + old_count).checked_sub(new_count);
                *slot = old_index
                    .filter(|old_index| *old_index < old_count)
                    .and_then(|old_index| self.changes[old_index]);
            }
            self.changes = shifted;
            if new_count < old_count {
                self.departing = [None; MAX_DIGITS];
                for index in 0..old_count - new_count {
                    self.departing[index] = Some(DigitChange {
                        previous: self.digits[index],
                        started: now,
                    });
                }
            }
        }
        for (index, character) in chars.iter().enumerate() {
            let old_index = (index + old_count).checked_sub(new_count);
            let previous = match old_index {
                Some(old_index) if old_index < old_count => self.digits[old_index],
                _ if old_count > 0 => ' ',
                _ => *character,
            };
            if previous != *character {
                self.changes[index] = Some(DigitChange {
                    previous,
                    started: now,
                });
            }
        }
        self.digit_count = new_count;
        self.digits[..new_count].copy_from_slice(&chars);
    }

    fn draw(
        &mut self,
        view: &mut View<'_>,
        text: &str,
        anchor: Anchor,
        size: f32,
        colors: &[Rgba],
        fallback: Rgba,
    ) {
        let now = Instant::now();
        let dt = self
            .last_frame
            .map_or(0.0, |last| now.duration_since(last).as_secs_f32())
            .min(0.1);
        self.last_frame = Some(now);
        self.advance(text, now);
        let digit_width = view.width("0", size, DIGIT_WEIGHT);
        let advance = |character: char, view: &mut View<'_>| {
            if character.is_ascii_digit() {
                digit_width
            } else {
                view.width(&character.to_string(), size, DIGIT_WEIGHT)
                    .max(size * 0.25)
            }
        };
        let chars = self.digits[..self.digit_count].to_vec();
        let mut total = 0.0;
        for character in &chars {
            total += advance(*character, view);
        }
        self.target_width = total;
        if self.width <= f32::EPSILON {
            self.width = total;
        } else {
            self.width += (total - self.width) * (1.0 - (-dt * WIDTH_RATE).exp());
            if (total - self.width).abs() <= 0.25 {
                self.width = total;
            }
        }
        let right = match anchor {
            Anchor::Center(center) => Point::new(center.x + self.width / 2.0, center.y),
            Anchor::Right(right) => right,
        };
        let mut x = right.x - total;
        let travel = size * 0.55;
        let save = view.save();
        view.clip_rect(Rect::from_xywh(
            right.x - total.max(self.width) * 1.5,
            right.y - size * 0.62,
            total.max(self.width) * 2.0,
            size * 1.24,
        ));
        let eased = |change: DigitChange| {
            let t = (now.duration_since(change.started).as_secs_f32()
                / DIGIT_DURATION.as_secs_f32())
            .clamp(0.0, 1.0);
            1.0 - (1.0 - t).powi(3)
        };
        let color_at = |index: usize| colors.get(index).copied().unwrap_or(fallback);
        let mut buffer = [0_u8; 4];
        let mut departing_x = x;
        let departing: Vec<DigitChange> = self.departing.iter().flatten().copied().collect();
        for change in departing.iter().rev() {
            let width = advance(change.previous, view);
            departing_x -= width;
            let amount = eased(*change);
            let color = color_at(0);
            view.text_center(
                change.previous.encode_utf8(&mut buffer),
                Rect::from_xywh(
                    departing_x,
                    right.y - size / 2.0 - travel * amount,
                    width,
                    size,
                ),
                size,
                DIGIT_WEIGHT,
                with_alpha(color, alpha_of(color) * (1.0 - amount)),
            );
        }
        for (index, character) in chars.iter().enumerate() {
            let width = advance(*character, view);
            let cell = Rect::from_xywh(x, right.y - size / 2.0, width, size);
            let color = color_at(index);
            if let Some(change) = self.changes[index] {
                let amount = eased(change);
                view.text_center(
                    change.previous.encode_utf8(&mut buffer),
                    cell.offset(Vec2::new(0.0, -travel * amount)),
                    size,
                    DIGIT_WEIGHT,
                    with_alpha(color, alpha_of(color) * (1.0 - amount)),
                );
                view.text_center(
                    character.encode_utf8(&mut buffer),
                    cell.offset(Vec2::new(0.0, travel * (1.0 - amount))),
                    size,
                    DIGIT_WEIGHT,
                    with_alpha(color, alpha_of(color) * amount),
                );
            } else {
                view.text_center(
                    character.encode_utf8(&mut buffer),
                    cell,
                    size,
                    DIGIT_WEIGHT,
                    color,
                );
            }
            x += width;
        }
        view.restore_to(save);
    }
}

fn fitted_size(view: &mut View<'_>, text: &str, size: f32, weight: u16, max_width: f32) -> f32 {
    let width = view.width(text, size, weight);
    if width <= max_width || width <= f32::EPSILON {
        size
    } else {
        size * max_width / width
    }
}

fn primary_color(style: PrimaryStyle) -> Rgba {
    match style {
        PrimaryStyle::Start | PrimaryStyle::Resume => GREEN,
        PrimaryStyle::Pause => ORANGE,
    }
}

fn primary_label(style: PrimaryStyle, text: &Text) -> &'static str {
    match style {
        PrimaryStyle::Start => text.start,
        PrimaryStyle::Pause => text.pause,
        PrimaryStyle::Resume => text.resume,
    }
}

pub(crate) fn draw_page(
    view: &mut View<'_>,
    state: &mut State,
    width: f32,
    height: f32,
    opacity: f32,
    text_color: Rgba,
    clock: (u16, u16, u16),
) {
    if opacity <= 0.01 {
        return;
    }
    let layout = layout(width, height);
    let now = Instant::now();
    let frame = advance(state, now);
    let left = remaining(frame.phase, frame.seconds, now);
    let time_text = state::time_text(frame.phase, frame.seconds, now);
    let editable = presets_enabled(frame.phase);
    let pressed = |action: Action| {
        frame
            .press
            .filter(|(pressed, _)| *pressed == action)
            .map_or(0.0, |(_, amount)| amount)
    };
    let field_hover = TimeField::ALL
        .into_iter()
        .map(|field| frame.hover[Action::Field(field).index()])
        .fold(0.0, f32::max);
    let dial_hover = frame.hover[Action::Dial.index()].max(field_hover);

    let center = layout.ring_center;
    let radius = layout.ring_radius;
    let stroke = ring_stroke(radius);
    let ring_radius = (radius - stroke / 2.0).max(0.0);
    view.stroke_progress(
        center,
        ring_radius,
        1.0,
        stroke,
        with_alpha(text_color, (0.12 + 0.08 * dial_hover) * opacity),
    );
    let pulse = match frame.phase {
        Phase::Finished { at } => progress(at, now, state::FINISHED_PULSE_SECS).map_or(1.0, |t| {
            0.6 + 0.4 * (t * state::FINISHED_PULSE_SECS * std::f32::consts::TAU / 1.2).cos()
        }),
        _ => 1.0,
    };
    if frame.ring_fraction > 0.001 {
        view.stroke_progress(
            center,
            ring_radius,
            frame.ring_fraction,
            stroke,
            with_alpha(ORANGE, frame.ring_alpha * pulse * opacity),
        );
    }

    let inner_width = (ring_radius - stroke) * 1.7;
    let pop = match frame.phase {
        Phase::Finished { at } => {
            progress(at, now, 0.5).map_or(1.0, |t| 0.9 + 0.1 * state::ease_out_back(t))
        }
        _ => 1.0,
    };
    let size = time_size(&layout, &time_text, &mut |text: &str, size: f32| {
        view.time_width(text, size)
    }) * pop;
    let anchor = time_center(&layout, size);
    let base_color = with_alpha(text_color, opacity);
    let mut colors: Vec<Rgba> = Vec::new();
    if editable {
        let colon = view.time_width(":", size);
        let mut x = anchor.x - view.time_width(&time_text, size) / 2.0;
        let corner = size * 0.2;
        let mut parts = time_text.split(':');
        for (field, selection) in TimeField::ALL.into_iter().zip(frame.selection) {
            let part = parts.next().unwrap_or_default();
            let width = view.time_width(part, size);
            let rect = Rect::from_xywh(x - colon / 2.0, anchor.y - size / 2.0, width + colon, size);
            let pill = Rect::from_xywh(
                rect.left + colon * 0.25,
                rect.top + size * 0.08,
                (rect.width() - colon * 0.5).max(0.0),
                (rect.height() - size * 0.16).max(0.0),
            );
            let hover_amount = frame.hover[Action::Field(field).index()] * (1.0 - selection);
            if hover_amount > 0.002 {
                view.fill_round_rect(
                    pill,
                    corner,
                    with_alpha(text_color, 0.1 * hover_amount * opacity),
                );
            }
            if selection > 0.002 {
                view.fill_round_rect(pill, corner, with_alpha(ORANGE, 0.22 * selection * opacity));
            }
            x += width + colon;
        }
        let mut field = 0;
        colors = time_text
            .chars()
            .map(|character| {
                if character == ':' {
                    field += 1;
                    return base_color;
                }
                let selection = frame.selection[field.min(TimeField::ALL.len() - 1)];
                with_alpha(blend(text_color, ORANGE, selection), opacity)
            })
            .collect();
    }
    state.digits.draw(
        view,
        &time_text,
        Anchor::Center(anchor),
        size,
        &colors,
        base_color,
    );
    let text = crate::locale::text(&state.language);
    let (caption, caption_color) = match frame.phase {
        Phase::Idle | Phase::Finished { .. } if frame.selected.is_some() => (
            text.fine_hint.to_owned(),
            with_alpha(ORANGE, 0.85 * opacity),
        ),
        Phase::Idle | Phase::Finished { .. } if dial_hover > 0.5 => (
            text.scroll_hint.to_owned(),
            with_alpha(ORANGE, 0.85 * opacity),
        ),
        Phase::Idle => (
            text.title.to_owned(),
            with_alpha(text_color, 0.55 * opacity),
        ),
        Phase::Running { .. } => (
            crate::locale::fill(text.ends_at, &state::end_time_text(left, clock)),
            with_alpha(text_color, 0.55 * opacity),
        ),
        Phase::Paused { .. } => (text.pause.to_owned(), with_alpha(ORANGE, 0.85 * opacity)),
        Phase::Finished { .. } => (text.done.to_owned(), with_alpha(ORANGE, opacity)),
    };
    let caption_size = fitted_size(
        view,
        &caption,
        (radius * 0.16).max(8.0),
        MEDIUM,
        inner_width,
    );
    view.text_center(
        &caption,
        Rect::from_xywh(
            center.x - radius,
            center.y + size * 0.48,
            radius * 2.0,
            caption_size * 1.4,
        ),
        caption_size,
        MEDIUM,
        caption_color,
    );

    for (index, chip) in layout.chips.iter().enumerate() {
        if chip.height() <= 1.0 {
            continue;
        }
        let action = Action::Preset(index);
        let selected = state::PRESETS[index] * 60 == frame.seconds;
        let enabled_factor = frame.chips_enabled;
        let hover_amount = frame.hover[action.index()];
        let fill = if selected {
            with_alpha(
                ORANGE,
                (0.24 + 0.08 * hover_amount) * enabled_factor * opacity,
            )
        } else {
            with_alpha(
                text_color,
                (0.08 + 0.06 * hover_amount + 0.12 * pressed(action)) * enabled_factor * opacity,
            )
        };
        view.fill_round_rect(*chip, chip.height() / 2.0, fill);
        let label = crate::locale::fill(text.minutes, &state::PRESETS[index].to_string());
        let label_size = fitted_size(
            view,
            &label,
            chip.height() * 0.5,
            MEDIUM,
            chip.width() - chip.height() * 0.5,
        );
        view.text_center(
            &label,
            *chip,
            label_size,
            MEDIUM,
            if selected {
                with_alpha(ORANGE, enabled_factor * opacity)
            } else {
                with_alpha(text_color, 0.85 * enabled_factor * opacity)
            },
        );
    }

    let cancel_enabled = if matches!(frame.phase, Phase::Idle) {
        0.4
    } else {
        1.0
    };
    let primary_enabled = if editable && frame.seconds == 0 {
        0.4
    } else {
        1.0
    };
    let (primary_tint, primary_labels) = match frame.primary_change {
        Some((previous, eased)) => (
            blend(primary_color(previous), primary_color(frame.primary), eased),
            vec![
                (primary_label(previous, text), 1.0 - eased),
                (primary_label(frame.primary, text), eased),
            ],
        ),
        None => (
            primary_color(frame.primary),
            vec![(primary_label(frame.primary, text), 1.0)],
        ),
    };
    let buttons = [
        (
            Action::Cancel,
            layout.cancel,
            vec![(text.cancel, 1.0)],
            text_color,
            cancel_enabled,
        ),
        (
            Action::Primary,
            layout.primary,
            primary_labels,
            primary_tint,
            primary_enabled,
        ),
    ];
    for (action, rect, labels, tint, enabled_factor) in buttons {
        if rect.width() <= 1.0 {
            continue;
        }
        let hover_amount = frame.hover[action.index()];
        let press_amount = pressed(action);
        let inset = rect.height().min(rect.width()) * 0.03 * press_amount;
        let body = rect.inset(inset);
        let corner = body.height().min(body.width()) / 2.0;
        let fill_alpha = if action == Action::Cancel {
            0.16 + 0.06 * hover_amount + 0.12 * press_amount
        } else {
            0.24 + 0.08 * hover_amount + 0.14 * press_amount
        };
        view.fill_round_rect(
            body,
            corner,
            with_alpha(tint, fill_alpha * enabled_factor * opacity),
        );
        view.stroke_round_rect(
            body.inset(0.5),
            corner,
            1.0,
            with_alpha(tint, 0.16 * enabled_factor * opacity),
        );
        let label_alpha = if action == Action::Cancel { 0.9 } else { 1.0 };
        // These buttons are taller than they are wide, and a capsule is full width where the
        // label sits, so reserve a small margin instead of half the height; otherwise four
        // characters such as キャンセル shrink to an unreadable size.
        let label_width = (body.width() - 10.0).max(1.0);
        for (label, weight) in labels {
            let label_size = fitted_size(
                view,
                label,
                (body.height() * 0.36).min(15.0),
                SEMIBOLD,
                label_width,
            );
            view.text_center(
                label,
                rect,
                label_size,
                SEMIBOLD,
                with_alpha(tint, label_alpha * weight * enabled_factor * opacity),
            );
        }
    }
}

pub(crate) fn draw_compact(
    view: &mut View<'_>,
    state: &mut State,
    width: f32,
    height: f32,
    opacity: f32,
) {
    if opacity <= 0.01 {
        return;
    }
    let now = Instant::now();
    let phase = state.phase;
    if !matches!(phase, Phase::Running { .. } | Phase::Paused { .. }) {
        return;
    }
    let frame = advance(state, now);
    let paused = matches!(frame.phase, Phase::Paused { .. });
    let color = with_alpha(YELLOW, opacity * if paused { 0.65 } else { 1.0 });
    let total = Duration::from_secs(frame.seconds);
    let fraction = if total.is_zero() {
        0.0
    } else {
        (1.0 - remaining(frame.phase, frame.seconds, now).as_secs_f32() / total.as_secs_f32())
            .clamp(0.0, 1.0)
    };
    let center = Point::new(EDGE_INSET + ICON_SIZE / 2.0, height / 2.0);
    draw_progress(view, center, 1.0, fraction, color);
    let text = state::format_remaining(remaining(frame.phase, frame.seconds, now));
    state.compact_digits.draw(
        view,
        &text,
        Anchor::Right(Point::new(width - EDGE_INSET, height / 2.0)),
        COMPACT_FONT_SIZE,
        &[],
        color,
    );
}

fn draw_progress(view: &mut View<'_>, center: Point, scale: f32, fraction: f32, color: Rgba) {
    let radius = (ICON_SIZE / 2.0 - 1.6) * scale;
    let ring_width = 1.8 * scale;
    view.stroke_progress(
        center,
        radius,
        1.0,
        ring_width,
        with_alpha(color, alpha_of(color) * 0.22),
    );
    view.stroke_progress(center, radius, fraction, ring_width, color);
    let angle = fraction * std::f32::consts::TAU;
    let direction = Vec2::new(angle.sin(), -angle.cos());
    let marker_width = 1.5 * scale;
    let outer = radius - ring_width / 2.0 - marker_width / 2.0 - 0.8 * scale;
    let inner = outer - 2.2 * scale;
    view.stroke_line(
        Point::new(
            center.x + direction.x * inner,
            center.y + direction.y * inner,
        ),
        Point::new(
            center.x + direction.x * outer,
            center.y + direction.y * outer,
        ),
        marker_width,
        color,
    );
    view.fill_circle(
        Point::new(
            center.x + direction.x * outer,
            center.y + direction.y * outer,
        ),
        marker_width / 2.0,
        color,
    );
}
