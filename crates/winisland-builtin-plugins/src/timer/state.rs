use std::time::{Duration, Instant};

use crate::geometry::{Point, Rect};

use super::view::Rolling;

pub(crate) const PRESETS: [u64; 6] = [1, 5, 10, 15, 25, 60];
pub(crate) const DEFAULT_SECONDS: u64 = 5 * 60;
pub(crate) const FINISHED_PULSE_SECS: f32 = 8.0;
pub(crate) const CONTROL_COUNT: usize = PRESETS.len() + 3 + TimeField::ALL.len();
const COLUMN_GAP: f32 = 18.0;
const CHIP_GAP: f32 = 6.0;
const PRESS_SECS: f32 = 0.26;
const HOVER_RATE: f32 = 18.0;
const EASE_RATE: f32 = 9.0;
const LABEL_SECS: f32 = 0.22;
const SETTLE_EPSILON: f32 = 0.002;
const DIAL_STRIPS: usize = 16;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum TimeField {
    Hours,
    Minutes,
    Seconds,
}

impl TimeField {
    pub(crate) const ALL: [Self; 3] = [Self::Hours, Self::Minutes, Self::Seconds];

    pub(crate) fn unit(self) -> u64 {
        match self {
            Self::Hours => 3600,
            Self::Minutes => 60,
            Self::Seconds => 1,
        }
    }

    pub(crate) fn max(self) -> u64 {
        match self {
            Self::Hours => 23,
            Self::Minutes | Self::Seconds => 59,
        }
    }

    pub(crate) fn coarse_step(self) -> u64 {
        match self {
            Self::Hours => 1,
            Self::Minutes => 5,
            Self::Seconds => 10,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Action {
    Preset(usize),
    Primary,
    Cancel,
    Dial,
    Field(TimeField),
}

impl Action {
    pub(crate) fn index(self) -> usize {
        match self {
            Self::Preset(index) => index,
            Self::Primary => PRESETS.len(),
            Self::Cancel => PRESETS.len() + 1,
            Self::Dial => PRESETS.len() + 2,
            Self::Field(field) => PRESETS.len() + 3 + field as usize,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Phase {
    Idle,
    Running { ends_at: Instant },
    Paused { remaining: Duration },
    Finished { at: Instant },
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum PrimaryStyle {
    Start,
    Pause,
    Resume,
}

impl PrimaryStyle {
    fn of(phase: Phase) -> Self {
        match phase {
            Phase::Idle | Phase::Finished { .. } => Self::Start,
            Phase::Running { .. } => Self::Pause,
            Phase::Paused { .. } => Self::Resume,
        }
    }
}

pub(crate) struct State {
    pub(crate) seconds: u64,
    pub(crate) phase: Phase,
    pub(crate) selected: Option<TimeField>,
    pub(crate) selection: [f32; TimeField::ALL.len()],
    pub(crate) hover_target: Option<Action>,
    pub(crate) hover: [f32; CONTROL_COUNT],
    pub(crate) press: Option<(Action, Instant)>,
    pub(crate) last_frame: Option<Instant>,
    pub(crate) ring_fraction: f32,
    pub(crate) ring_alpha: f32,
    pub(crate) chips_enabled: f32,
    pub(crate) primary: PrimaryStyle,
    pub(crate) primary_change: Option<(PrimaryStyle, Instant)>,
    pub(crate) digits: Rolling,
    pub(crate) compact_digits: Rolling,
    pub(crate) alert_open: bool,
    pub(crate) language: String,
    pub(crate) preferred_width: f32,
    pub(crate) wheel: f32,
    pub(crate) regions: Vec<Action>,
}

impl State {
    pub(crate) fn new() -> Self {
        Self {
            seconds: DEFAULT_SECONDS,
            phase: Phase::Idle,
            selected: None,
            selection: [0.0; TimeField::ALL.len()],
            hover_target: None,
            hover: [0.0; CONTROL_COUNT],
            press: None,
            last_frame: None,
            ring_fraction: 1.0,
            ring_alpha: 0.35,
            chips_enabled: 1.0,
            primary: PrimaryStyle::Start,
            primary_change: None,
            digits: Rolling::new(),
            compact_digits: Rolling::new(),
            alert_open: false,
            language: String::from("en_us"),
            preferred_width: 0.0,
            wheel: 0.0,
            regions: Vec::new(),
        }
    }
}

pub(crate) struct Layout {
    pub(crate) ring_center: Point,
    pub(crate) ring_radius: f32,
    pub(crate) chips: [Rect; PRESETS.len()],
    pub(crate) primary: Rect,
    pub(crate) cancel: Rect,
}

pub(crate) fn layout(w: f32, h: f32) -> Layout {
    let radius = 48.0_f32.min(w / 2.0).min(h / 2.0).max(0.0);
    let inset = (radius * (1.0 - std::f32::consts::FRAC_1_SQRT_2) + 8.0).max(24.0);
    let inner = Rect::from_xywh(
        inset,
        inset,
        (w - inset * 2.0).max(0.0),
        (h - inset * 2.0).max(0.0),
    );
    let diameter = inner.height().min(inner.width() * 0.48);
    let ring_center = Point::new(inner.left + diameter / 2.0, inner.center_y());
    let column_top = ring_center.y - diameter / 2.0;
    let column_bottom = ring_center.y + diameter / 2.0;
    let column_left = inner.left + diameter + COLUMN_GAP;
    let column_w = (inner.right - column_left).max(0.0);
    let chip_h = (diameter * 0.2).clamp(14.0, 24.0);
    let chip_w = ((column_w - CHIP_GAP * 2.0) / 3.0).max(0.0);
    let chips = std::array::from_fn(|index| {
        let column = (index % 3) as f32;
        let row = (index / 3) as f32;
        Rect::from_xywh(
            column_left + column * (chip_w + CHIP_GAP),
            column_top + row * (chip_h + CHIP_GAP),
            chip_w,
            chip_h,
        )
    });
    let buttons_top = column_top + (chip_h + CHIP_GAP) * 2.0;
    let button_h = (column_bottom - buttons_top).max(0.0);
    let button_w = ((column_w - CHIP_GAP) / 2.0).max(0.0);
    Layout {
        ring_center,
        ring_radius: diameter / 2.0,
        chips,
        cancel: Rect::from_xywh(column_left, buttons_top, button_w, button_h),
        primary: Rect::from_xywh(
            column_left + button_w + CHIP_GAP,
            buttons_top,
            button_w,
            button_h,
        ),
    }
}

pub(crate) fn ring_stroke(radius: f32) -> f32 {
    (radius * 0.12).max(3.0)
}

pub(crate) fn progress(started: Instant, now: Instant, duration: f32) -> Option<f32> {
    let t = now.saturating_duration_since(started).as_secs_f32() / duration;
    (t < 1.0).then_some(t.max(0.0))
}

pub(crate) fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

pub(crate) fn ease_out_back(t: f32) -> f32 {
    const OVERSHOOT: f32 = 1.70158;
    let shifted = t - 1.0;
    1.0 + (OVERSHOOT + 1.0) * shifted.powi(3) + OVERSHOOT * shifted.powi(2)
}

fn approach(value: &mut f32, target: f32, blend: f32) {
    *value += (target - *value) * blend;
    if (target - *value).abs() <= SETTLE_EPSILON {
        *value = target;
    }
}

pub(crate) fn presets_enabled(phase: Phase) -> bool {
    matches!(phase, Phase::Idle | Phase::Finished { .. })
}

pub(crate) fn remaining(phase: Phase, seconds: u64, now: Instant) -> Duration {
    match phase {
        Phase::Idle => Duration::from_secs(seconds),
        Phase::Running { ends_at } => ends_at.saturating_duration_since(now),
        Phase::Paused { remaining } => remaining,
        Phase::Finished { .. } => Duration::ZERO,
    }
}

fn ring_targets(phase: Phase, seconds: u64, now: Instant) -> (f32, f32) {
    let total = Duration::from_secs(seconds);
    let fraction = if total.is_zero() {
        0.0
    } else {
        (remaining(phase, seconds, now).as_secs_f32() / total.as_secs_f32()).clamp(0.0, 1.0)
    };
    match phase {
        Phase::Idle => (1.0, 0.35),
        Phase::Running { .. } => (fraction, 1.0),
        Phase::Paused { .. } => (fraction, 0.55),
        Phase::Finished { .. } => (1.0, 1.0),
    }
}

pub(crate) fn apply_action(state: &mut State, action: Action) {
    let now = Instant::now();
    state.press = Some((action, now));
    let total = Duration::from_secs(state.seconds);
    state.phase = match (action, state.phase) {
        (Action::Preset(index), phase) if presets_enabled(phase) => {
            state.seconds = PRESETS[index.min(PRESETS.len() - 1)] * 60;
            Phase::Idle
        }
        (Action::Field(field), phase) if presets_enabled(phase) => {
            state.selected = Some(field);
            Phase::Idle
        }
        (Action::Primary, Phase::Idle | Phase::Finished { .. }) if !total.is_zero() => {
            Phase::Running {
                ends_at: now + total,
            }
        }
        (Action::Primary, Phase::Running { ends_at }) => Phase::Paused {
            remaining: ends_at.saturating_duration_since(now),
        },
        (Action::Primary, Phase::Paused { remaining }) => Phase::Running {
            ends_at: now + remaining,
        },
        (Action::Cancel, _) => Phase::Idle,
        (_, phase) => phase,
    };
    if !presets_enabled(state.phase) {
        state.selected = None;
    }
}

/// Starts a countdown of `seconds` right away; zero cancels the timer.
pub(crate) fn start_for(state: &mut State, seconds: u64) {
    state.selected = None;
    if seconds == 0 {
        state.phase = Phase::Idle;
        return;
    }
    state.seconds = seconds;
    state.phase = Phase::Running {
        ends_at: Instant::now() + Duration::from_secs(seconds),
    };
}

pub(crate) fn is_running_or_paused(phase: Phase) -> bool {
    matches!(phase, Phase::Running { .. } | Phase::Paused { .. })
}

pub(crate) fn scroll(state: &mut State, target: Action, steps: i32) -> bool {
    if steps == 0 || !presets_enabled(state.phase) {
        return false;
    }
    let (field, step) = match (state.selected, target) {
        (Some(field), _) => (field, 1),
        (None, Action::Field(field)) => (field, field.coarse_step()),
        (None, _) => (TimeField::Minutes, TimeField::Minutes.coarse_step()),
    };
    let unit = field.unit();
    let value = state.seconds / unit % (field.max() + 1);
    let mut next = value;
    for _ in 0..steps.unsigned_abs() {
        next = if steps > 0 {
            ((next / step + 1) * step).min(field.max())
        } else {
            next.saturating_sub(1) / step * step
        };
    }
    if next == value {
        return false;
    }
    state.seconds = state.seconds - value * unit + next * unit;
    state.phase = Phase::Idle;
    true
}

pub(crate) fn poll_finished(state: &mut State) -> Option<u64> {
    let now = Instant::now();
    let finished = match state.phase {
        Phase::Running { ends_at } => now >= ends_at,
        Phase::Paused { remaining } => remaining.is_zero(),
        Phase::Idle | Phase::Finished { .. } => false,
    };
    if !finished {
        return None;
    }
    state.phase = Phase::Finished { at: now };
    Some(state.seconds)
}

pub(crate) struct Frame {
    pub(crate) seconds: u64,
    pub(crate) phase: Phase,
    pub(crate) selected: Option<TimeField>,
    pub(crate) selection: [f32; TimeField::ALL.len()],
    pub(crate) hover: [f32; CONTROL_COUNT],
    pub(crate) press: Option<(Action, f32)>,
    pub(crate) ring_fraction: f32,
    pub(crate) ring_alpha: f32,
    pub(crate) chips_enabled: f32,
    pub(crate) primary: PrimaryStyle,
    pub(crate) primary_change: Option<(PrimaryStyle, f32)>,
}

pub(crate) fn advance(state: &mut State, now: Instant) -> Frame {
    let dt = state
        .last_frame
        .map_or(0.0, |last| {
            now.saturating_duration_since(last).as_secs_f32()
        })
        .min(0.1);
    state.last_frame = Some(now);
    let hover_blend = 1.0 - (-dt * HOVER_RATE).exp();
    let ease_blend = 1.0 - (-dt * EASE_RATE).exp();
    let hover_target = state.hover_target;
    for (index, value) in state.hover.iter_mut().enumerate() {
        let target = f32::from(hover_target.is_some_and(|action| action.index() == index));
        approach(value, target, hover_blend);
    }
    let selected = state.selected;
    for (field, value) in TimeField::ALL.into_iter().zip(state.selection.iter_mut()) {
        approach(value, f32::from(selected == Some(field)), hover_blend);
    }
    let (ring_fraction, ring_alpha) = ring_targets(state.phase, state.seconds, now);
    let running = matches!(state.phase, Phase::Running { .. });
    let fraction_blend = if running && (state.ring_fraction - ring_fraction).abs() < 0.02 {
        1.0
    } else {
        ease_blend
    };
    approach(&mut state.ring_fraction, ring_fraction, fraction_blend);
    approach(&mut state.ring_alpha, ring_alpha, ease_blend);
    let chips_target = if presets_enabled(state.phase) {
        1.0
    } else {
        0.35
    };
    approach(&mut state.chips_enabled, chips_target, ease_blend);

    let style = PrimaryStyle::of(state.phase);
    if style != state.primary {
        state.primary_change = Some((state.primary, now));
        state.primary = style;
    }

    let press = state.press.and_then(|(action, started)| {
        progress(started, now, PRESS_SECS).map(|t| (action, 1.0 - t))
    });
    if press.is_none() {
        state.press = None;
    }
    let primary_change = state.primary_change.and_then(|(previous, started)| {
        progress(started, now, LABEL_SECS).map(|t| (previous, ease_out_cubic(t)))
    });
    if primary_change.is_none() {
        state.primary_change = None;
    }
    Frame {
        seconds: state.seconds,
        phase: state.phase,
        selected,
        selection: state.selection,
        hover: state.hover,
        press,
        ring_fraction: state.ring_fraction,
        ring_alpha: state.ring_alpha,
        chips_enabled: state.chips_enabled,
        primary: state.primary,
        primary_change,
    }
}

pub(crate) fn time_text(phase: Phase, seconds: u64, now: Instant) -> String {
    let left = remaining(phase, seconds, now);
    if presets_enabled(phase) {
        let total = left.as_secs();
        format!(
            "{}:{:02}:{:02}",
            total / 3600,
            total % 3600 / 60,
            total % 60
        )
    } else {
        format_remaining(left)
    }
}

pub(crate) fn format_remaining(duration: Duration) -> String {
    let total = duration.as_secs() + u64::from(duration.subsec_nanos() > 0);
    let (hours, minutes, seconds) = (total / 3600, total % 3600 / 60, total % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

pub(crate) fn duration_label(seconds: u64, text: &super::locale::Text) -> String {
    if seconds.is_multiple_of(60) {
        super::locale::fill(text.minutes, &(seconds / 60).to_string())
    } else {
        format_remaining(Duration::from_secs(seconds))
    }
}

pub(crate) fn end_time_text(remaining: Duration, now: (u16, u16, u16)) -> String {
    let (hour, minute, second) = now;
    let seconds_today =
        u64::from(hour) * 3600 + u64::from(minute) * 60 + u64::from(second) + remaining.as_secs();
    let minutes = (seconds_today / 60) % (24 * 60);
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

pub(crate) fn time_size(
    layout: &Layout,
    text: &str,
    mut measure: impl FnMut(&str, f32) -> f32,
) -> f32 {
    let stroke = ring_stroke(layout.ring_radius);
    let inner_width = ((layout.ring_radius - stroke / 2.0).max(0.0) - stroke) * 1.7;
    let size = layout.ring_radius * 0.42;
    let width = measure(text, size);
    if width <= inner_width || width <= f32::EPSILON {
        size
    } else {
        size * inner_width / width
    }
}

pub(crate) fn time_center(layout: &Layout, size: f32) -> Point {
    Point::new(layout.ring_center.x, layout.ring_center.y - size * 0.12)
}

pub(crate) fn field_rects(
    text: &str,
    size: f32,
    center: Point,
    mut measure: impl FnMut(&str, f32) -> f32,
) -> [Rect; TimeField::ALL.len()] {
    let colon = measure(":", size);
    let mut x = center.x - measure(text, size) / 2.0;
    let mut parts = text.split(':');
    std::array::from_fn(|_| {
        let width = measure(parts.next().unwrap_or_default(), size);
        let rect = Rect::from_xywh(x - colon / 2.0, center.y - size / 2.0, width + colon, size);
        x += width + colon;
        rect
    })
}

/// Horizontal strips covering the dial disc, largest first so later regions keep priority.
pub(crate) fn dial_strips(layout: &Layout) -> Vec<Rect> {
    let radius = layout.ring_radius.max(0.0);
    let center = layout.ring_center;
    let step = radius * 2.0 / DIAL_STRIPS as f32;
    (0..DIAL_STRIPS)
        .map(|index| {
            let top = center.y - radius + step * index as f32;
            let bottom = (top + step).min(center.y + radius);
            let edge = (top - center.y).abs().max((bottom - center.y).abs());
            let half = (radius * radius - edge * edge).max(0.0).sqrt();
            Rect::from_xywh(center.x - half, top, half * 2.0, (bottom - top).max(0.0))
        })
        .collect()
}
