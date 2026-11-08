use std::cell::LazyCell;
use std::time::{Duration, Instant};

use winisland_core::anim::{Curve, Tween};
use winisland_platform::DeviceActivity;
use winisland_render::{
    Angle, Image, ImageOptions, Painter, Point, Rect, Rgba, Sampling, StrokeCap,
};

use crate::ui::compact::timer::TimerContent;

use super::{Activity, AssistantActivity};

const PILL_EXTENSION: f32 = 0.75;
const FADE_DURATION: Duration = Duration::from_millis(100);
const SHARE_DURATION: Duration = Duration::from_millis(420);
const TIMER_ICON_DIAMETER: f32 = 26.0;

thread_local! {
    static CHECKMARK: LazyCell<Option<(Image, f32)>> = LazyCell::new(|| {
        crate::icons::load_tinted_glyph(
            include_bytes!("../../../../../resources/in_app/icons/checkmark.png"),
            Rgba::WHITE,
        )
    });
}

pub(crate) struct StatusIndicators {
    camera: Tween,
    microphone: Tween,
    timer: Tween,
    plugin: Tween,
    assistant: Tween,
    assistant_activity: AssistantActivity,
    shared: Tween,
    timer_progress: f32,
    timer_paused: bool,
    plugin_surface: Option<u64>,
}

impl Default for StatusIndicators {
    fn default() -> Self {
        Self {
            camera: Tween::new(0.0),
            microphone: Tween::new(0.0),
            timer: Tween::new(0.0),
            plugin: Tween::new(0.0),
            assistant: Tween::new(0.0),
            assistant_activity: AssistantActivity::Available,
            shared: Tween::new(0.0),
            timer_progress: 0.0,
            timer_paused: false,
            plugin_surface: None,
        }
    }
}

impl StatusIndicators {
    pub(crate) fn update(
        &mut self,
        state: DeviceActivity,
        timer: Option<TimerContent>,
        plugin_surface: Option<u64>,
        assistant_activity: Option<AssistantActivity>,
        now: Instant,
    ) -> bool {
        let before = self.activity();
        let camera = state.camera == Some(true);
        let microphone = state.microphone == Some(true);
        let timing = timer.is_some();
        let plugin = plugin_surface.is_some() && !timing;
        let devices = camera || microphone;
        let occupied = timing || plugin;
        let assistant = assistant_activity.is_some();
        let groups = u8::from(devices) + u8::from(occupied) + u8::from(assistant);
        let pace = |other: bool| if other { SHARE_DURATION } else { FADE_DURATION };
        for (animation, active, duration) in [
            (&mut self.camera, camera, pace(occupied)),
            (&mut self.microphone, microphone, pace(occupied)),
            (&mut self.timer, timing, pace(devices)),
            (&mut self.plugin, plugin, pace(devices)),
            (&mut self.assistant, assistant, pace(devices || occupied)),
        ] {
            animation.retarget(if active { 1.0 } else { 0.0 }, duration, Curve::Ease, now);
            animation.update(now);
        }
        self.shared.retarget(
            f32::from(groups.saturating_sub(1)),
            SHARE_DURATION,
            Curve::Ease,
            now,
        );
        self.shared.update(now);
        if let Some(activity) = assistant_activity {
            self.assistant_activity = activity;
        }
        if let Some(timer) = timer {
            self.timer_progress = (timer.progress() * 720.0).round() / 720.0;
            self.timer_paused = timer.paused;
        }
        if plugin {
            self.plugin_surface = plugin_surface;
        } else if !self.plugin.is_animating() {
            self.plugin_surface = None;
        }
        before != self.activity()
    }

    pub(crate) fn activity(&self) -> Option<Activity> {
        let camera = self.camera.value();
        let microphone = self.microphone.value();
        let timer = self.timer.value();
        let plugin = self.plugin_surface.map_or(0.0, |_| self.plugin.value());
        let assistant = self.assistant.value();
        (camera > 0.0
            || microphone > 0.0
            || timer > 0.0
            || plugin > 0.0
            || assistant > 0.0
            || self.is_animating())
        .then_some(Activity::Status {
            camera,
            microphone,
            timer,
            plugin,
            assistant,
            assistant_activity: self.assistant_activity,
            shared: self.shared.value(),
            timer_progress: self.timer_progress,
            timer_paused: self.timer_paused,
            plugin_surface: self.plugin_surface,
        })
    }

    pub(crate) fn is_animating(&self) -> bool {
        self.camera.is_animating()
            || self.microphone.is_animating()
            || self.timer.is_animating()
            || self.plugin.is_animating()
            || self.assistant.is_animating()
            || (self.assistant.value() > 0.0
                && matches!(self.assistant_activity, AssistantActivity::Working(_)))
            || self.shared.is_animating()
    }
}

pub(super) struct Indicators {
    pub(super) camera: f32,
    pub(super) microphone: f32,
    pub(super) timer: f32,
    pub(super) timer_progress: f32,
    pub(super) timer_paused: bool,
    pub(super) assistant: f32,
    pub(super) assistant_activity: AssistantActivity,
}

pub(super) fn width(height: f32, shared: f32) -> f32 {
    height * (1.0 + PILL_EXTENSION * shared)
}

pub(super) fn draw(
    painter: Painter<'_>,
    rect: Rect,
    scale: f32,
    opacity: f32,
    indicators: Indicators,
) {
    let diameter = rect.width().min(rect.height());
    if indicators.timer > 0.0 {
        crate::ui::compact::timer::draw_timer_icon(
            painter,
            Point::new(rect.left + diameter / 2.0, rect.center_y()),
            diameter / TIMER_ICON_DIAMETER,
            indicators.timer_progress,
            indicators.timer_paused,
            opacity * indicators.timer * indicators.timer,
        );
    }
    let dot_radius = diameter / 9.0;
    let dot_spacing = diameter * 0.35;
    let dots_x = rect.right - diameter / 2.0 - diameter * PILL_EXTENSION * indicators.assistant;
    let dots = [
        (
            indicators.camera,
            -indicators.microphone,
            Rgba::from_rgb(48, 209, 88),
        ),
        (
            indicators.microphone,
            indicators.camera,
            Rgba::from_rgb(255, 159, 10),
        ),
    ];
    for (visibility, offset, color) in dots {
        let x = dots_x + offset * 0.5 * dot_spacing;
        painter.fill_circle(
            Point::new(x, rect.center_y()),
            dot_radius,
            color.with_alpha_f(opacity * visibility * visibility),
        );
    }
    if indicators.assistant > 0.0 {
        let radius = diameter * 0.23;
        let center = Point::new(rect.right - diameter / 2.0, rect.center_y());
        let alpha = opacity * indicators.assistant;
        match indicators.assistant_activity {
            AssistantActivity::Working(phase) => painter.stroke_arc(
                Rect::from_xywh(
                    center.x - radius,
                    center.y - radius,
                    radius * 2.0,
                    radius * 2.0,
                ),
                Angle::from_degrees(phase),
                Angle::from_degrees(265.0),
                (1.8 * scale).max(1.0),
                Rgba::WHITE.with_alpha_f(alpha),
                StrokeCap::Round,
            ),
            AssistantActivity::Available => CHECKMARK.with(|icon| {
                if let Some((image, aspect)) = icon.as_ref() {
                    let width = radius * 2.0;
                    let height = width / aspect;
                    painter.draw_image(
                        image,
                        Rect::from_xywh(
                            center.x - width / 2.0,
                            center.y - height / 2.0,
                            width,
                            height,
                        ),
                        &ImageOptions::default()
                            .with_sampling(Sampling::LinearLinear)
                            .with_alpha_f(alpha),
                    );
                }
            }),
        }
    }
}
