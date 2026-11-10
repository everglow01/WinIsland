use std::time::{Duration, Instant};

use winisland_render::text::FontManager;
use winisland_render::{FontWeight, Painter, Path, Point, Rect, Rgba, StrokeCap, Vec2};

use super::CompactSize;
use super::notification::NotificationIndicator;

const AMBER: Rgba = Rgba::from_rgb(255, 179, 64);
const ENTER_DURATION: Duration = Duration::from_millis(220);
const CLOSE_DURATION: Duration = Duration::from_millis(180);
const ICON_VIEWPORT: f32 = 32.0;

#[derive(Default)]
pub(super) struct AlertIndicator {
    title: String,
    icon: Option<Path>,
    shown_at: Option<Instant>,
    closing_at: Option<Instant>,
    entered: bool,
}

impl AlertIndicator {
    pub(super) fn show(&mut self, title: String, icon: Option<Path>) {
        self.title = title;
        self.icon = icon;
        self.shown_at = Some(Instant::now());
        self.closing_at = None;
        self.entered = false;
    }

    pub(super) fn is_visible(&self) -> bool {
        self.shown_at.is_some()
            && self
                .closing_at
                .is_none_or(|started| started.elapsed() < CLOSE_DURATION)
    }

    pub(super) fn is_animating(&self) -> bool {
        self.shown_at
            .is_some_and(|started| started.elapsed() < ENTER_DURATION)
            || self
                .closing_at
                .is_some_and(|started| started.elapsed() < CLOSE_DURATION)
    }

    pub(super) fn close(&mut self) -> bool {
        if !self.is_visible() || self.closing_at.is_some() {
            return false;
        }
        self.closing_at = Some(Instant::now());
        true
    }

    pub(super) fn clear(&mut self) {
        self.shown_at = None;
        self.closing_at = None;
        self.entered = false;
        self.title.clear();
        self.icon = None;
    }

    pub(super) fn update(&mut self) -> bool {
        if self.shown_at.is_some()
            && self
                .closing_at
                .is_some_and(|started| started.elapsed() >= CLOSE_DURATION)
        {
            self.clear();
            return true;
        }
        if !self.entered
            && self
                .shown_at
                .is_some_and(|started| started.elapsed() >= ENTER_DURATION)
        {
            self.entered = true;
            return true;
        }
        false
    }

    pub(super) fn target_size(base_width: f32, base_height: f32, scale: f32) -> CompactSize {
        NotificationIndicator::target_size(base_width, base_height, scale)
    }

    fn close_rect(rect: Rect, scale: f32) -> Rect {
        let size = 52.0 * scale;
        let inset = ((rect.height() - size) / 2.0).max(12.0 * scale);
        Rect::from_xywh(
            rect.right - inset - size,
            rect.center_y() - size / 2.0,
            size,
            size,
        )
    }

    pub(super) fn hit_close(&self, point: Point, rect: Rect, scale: f32) -> bool {
        self.is_visible()
            && self.closing_at.is_none()
            && Self::close_rect(self.presentation(rect, scale).0, scale)
                .inset(-3.0 * scale)
                .contains(point)
    }

    fn presentation(&self, rect: Rect, scale: f32) -> (Rect, f32) {
        let Some(started) = self.shown_at else {
            return (rect, 0.0);
        };
        let enter =
            (started.elapsed().as_secs_f32() / ENTER_DURATION.as_secs_f32()).clamp(0.0, 1.0);
        let enter = 1.0 - (1.0 - enter).powi(3);
        let exit = self.closing_at.map_or(1.0, |started| {
            1.0 - (started.elapsed().as_secs_f32() / CLOSE_DURATION.as_secs_f32()).clamp(0.0, 1.0)
        });
        (
            rect.offset(Vec2::new(0.0, (1.0 - enter) * 4.0 * scale)),
            enter * exit,
        )
    }

    pub(super) fn draw(&self, painter: Painter<'_>, rect: Rect, scale: f32, alpha: f32) {
        let (rect, opacity) = self.presentation(rect, scale);
        let opacity = alpha * opacity;
        if opacity <= 0.0 {
            return;
        }
        let color = AMBER.with_alpha_f(opacity);
        let button = Self::close_rect(rect, scale);
        let icon_center = self
            .icon
            .as_ref()
            .map(|_| Point::new(rect.left + 46.0 * scale, rect.center_y()));
        if let (Some(path), Some(center)) = (&self.icon, icon_center) {
            let saved = painter.save();
            painter.translate(Vec2::new(center.x, center.y));
            painter.scale(Vec2::new(1.7 * scale, 1.7 * scale));
            painter.translate(Vec2::new(-ICON_VIEWPORT / 2.0, -ICON_VIEWPORT / 2.0));
            painter.fill_path(path, color);
            painter.restore_to(saved);
        }
        let text_left =
            icon_center.map_or(rect.left + 24.0 * scale, |center| center.x + 31.0 * scale);
        draw_text(
            painter,
            &self.title,
            Rect::from_xywh(
                text_left,
                rect.center_y() + 4.0 * scale,
                (button.left - text_left - 16.0 * scale).max(0.0),
                22.0 * scale,
            ),
            18.0 * scale,
            FontWeight::SEMIBOLD,
            color,
        );
        let center = Point::new(button.center_x(), button.center_y());
        painter.fill_circle(
            center,
            button.width() / 2.0,
            Rgba::WHITE.with_alpha_f(opacity * 0.12),
        );
        let arm = 8.0 * scale;
        for direction in [-1.0, 1.0] {
            painter.stroke_line(
                Point::new(center.x - arm, center.y - direction * arm),
                Point::new(center.x + arm, center.y + direction * arm),
                2.2 * scale,
                Rgba::WHITE.with_alpha_f(opacity * 0.65),
                StrokeCap::Round,
            );
        }
    }
}

fn draw_text(
    painter: Painter<'_>,
    text: &str,
    rect: Rect,
    size: f32,
    weight: FontWeight,
    color: Rgba,
) {
    let fonts = FontManager::global();
    let width = fonts.measure_str(text, size, weight).width();
    let size = if width > rect.width() && width > 0.0 {
        size * rect.width().max(0.0) / width
    } else {
        size
    };
    let bounds = fonts.measure_str(text, size, weight);
    fonts.draw_str(
        painter,
        text,
        Point::new(
            rect.left - bounds.left,
            rect.center_y() - bounds.height() / 2.0 - bounds.top,
        ),
        size,
        weight,
        color,
    );
}
