use super::{SETTINGS_HEADER_H, SettingsApp};

const SCROLLBAR_BOTTOM_INSET: f32 = 8.0;
const SCROLLBAR_RIGHT_INSET: f32 = 5.0;
const SCROLLBAR_THUMB_MIN_H: f32 = 32.0;
const SCROLLBAR_TRACK_TOP_INSET: f32 = 8.0;
const SCROLLBAR_W: f32 = 4.0;
const SCROLLBAR_HIT_W: f32 = 16.0;

#[derive(Clone, Copy)]
pub(crate) struct ScrollbarGeometry {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
    track_y: f32,
    track_height: f32,
}

impl ScrollbarGeometry {
    pub(crate) fn hit_test(&self, x: f32, y: f32) -> bool {
        x >= self.x + self.width - SCROLLBAR_HIT_W
            && x <= self.x + self.width + SCROLLBAR_RIGHT_INSET
            && y >= self.y
            && y <= self.y + self.height
    }
}

impl SettingsApp {
    pub(crate) fn scrollbar_geometry(&self) -> Option<ScrollbarGeometry> {
        if self.cached_max_scroll <= 0.0 {
            return None;
        }
        let (win_w, win_h) = self.logical_window_size();
        let track_y = SETTINGS_HEADER_H + SCROLLBAR_TRACK_TOP_INSET;
        let track_height = win_h - track_y - SCROLLBAR_BOTTOM_INSET;
        let viewport_height = win_h - SETTINGS_HEADER_H;
        if track_height <= 0.0 || viewport_height <= 0.0 {
            return None;
        }
        let content_height = viewport_height + self.cached_max_scroll;
        let height = (track_height * viewport_height / content_height)
            .clamp(SCROLLBAR_THUMB_MIN_H.min(track_height), track_height);
        let travel = track_height - height;
        let y = track_y + self.scroll_y / self.cached_max_scroll * travel;
        Some(ScrollbarGeometry {
            x: win_w - SCROLLBAR_RIGHT_INSET - SCROLLBAR_W,
            y,
            width: SCROLLBAR_W,
            height,
            track_y,
            track_height,
        })
    }

    pub(super) fn begin_scroll_drag(&mut self, mouse_x: f32, mouse_y: f32) -> bool {
        let Some(scrollbar) = self.scrollbar_geometry() else {
            return false;
        };
        if !scrollbar.hit_test(mouse_x, mouse_y) {
            return false;
        }
        self.scroll_dragging = true;
        self.scroll_drag_offset = mouse_y - scrollbar.y;
        true
    }

    pub(super) fn update_scroll_drag(&mut self, mouse_y: f32) {
        if !self.scroll_dragging {
            return;
        }
        let Some(scrollbar) = self.scrollbar_geometry() else {
            self.scroll_dragging = false;
            return;
        };
        let travel = scrollbar.track_height - scrollbar.height;
        if travel <= 0.0 {
            return;
        }
        let thumb_y = (mouse_y - self.scroll_drag_offset)
            .clamp(scrollbar.track_y, scrollbar.track_y + travel);
        let scroll = (thumb_y - scrollbar.track_y) / travel * self.cached_max_scroll;
        self.target_scroll_y = scroll;
        self.scroll_y = scroll;
        self.request_redraw();
    }
}
