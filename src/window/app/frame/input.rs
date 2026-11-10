use std::time::{Duration, Instant};

use crate::platform::WindowRef;
use crate::ui::expanded::music_view::{
    get_progress_bar_rect, set_progress_dragging, set_progress_hover,
};
use crate::ui::expanded::pager::ExpandedPage;
use crate::utils::mouse::is_left_button_pressed;
use winisland_core::config::MIN_HIDDEN_WIDTH;

use super::super::{App, DragAxis, RIGHT_DRAG_THRESHOLD};
use super::{SETTLE_PIXEL_VELOCITY, SETTLE_PIXELS};

impl App {
    pub(super) fn input_pressed(&self) -> bool {
        if self.plugin_ui.pressed_buttons & (1 << 1) != 0 || self.plugin_ui.touch_id.is_some() {
            return false;
        }
        self.touch_id.is_some()
            || (self
                .last_touch_at
                .is_none_or(|time| time.elapsed() >= Duration::from_millis(250))
                && is_left_button_pressed())
    }

    pub(super) fn update_hit_regions(&self, window: &WindowRef, plugin_captured: bool) {
        use winisland_platform::{HitRegion, Rect};
        use winisland_render::{Path, Point};

        if self.fullscreen_hide_active() && !self.hide.overlay_reveal {
            window.set_hit_regions(&[]);
            return;
        }
        if self.touch_id.is_some()
            || self.is_dragging
            || self.is_right_dragging
            || self.seek.active
            || self.volume_dragging
            || crate::ui::expanded::music_view::output_scroll_dragging()
            || self.compact_overlay.is_volume_dragging()
            || self.compact_overlay.is_brightness_dragging()
            || plugin_captured
        {
            window.set_hit_regions(&[HitRegion::WholeWindow(true)]);
            return;
        }
        let layout = self.compute_island_layout();
        let island = winisland_render::Rect::from_xywh(
            layout.current_island_x as f32,
            layout.current_island_y as f32,
            self.springs.w.value,
            self.springs.h.value,
        );
        let path = Path::continuous_rounded_rect(island, self.springs.r.value);
        let mut regions = vec![HitRegion::Path(path.clip_segments(Point::new(0.0, 0.0)))];
        let rect_region = |rect: winisland_render::Rect| {
            HitRegion::Rect(Rect {
                left: rect.left.floor() as i32,
                top: rect.top.floor() as i32,
                right: rect.right.ceil() as i32,
                bottom: rect.bottom.ceil() as i32,
            })
        };
        if self.expanded {
            regions.extend(
                crate::ui::expanded::pager::hit_regions(
                    island,
                    self.expanded_pages().len(),
                    self.config.expanded_scale,
                    layout.dock_bottom,
                )
                .into_iter()
                .flatten()
                .map(rect_region),
            );
        }
        if !self.expanded
            && let Some(orb) = self.status_orb_rect()
        {
            regions.push(rect_region(orb));
        }
        if self.is_hidden()
            && self.config.hidden_width <= MIN_HIDDEN_WIDTH
            && self.springs.hide.value >= 0.999
        {
            regions.push(rect_region(winisland_render::Rect::from_xywh(
                layout.hidden_reveal_x as f32,
                layout.hidden_reveal_y as f32,
                layout.hidden_reveal_w as f32,
                layout.hidden_reveal_h as f32,
            )));
        }
        window.set_hit_regions(&regions);
    }

    pub(super) fn update_right_drag(&mut self, window: &WindowRef, px: i32, py: i32) {
        let (Some((start_cx, start_cy)), Some((start_ox, start_oy))) =
            (self.right_press_cursor, self.right_drag_start_offset)
        else {
            return;
        };
        let dx = px - start_cx;
        let dy = py - start_cy;
        if !self.is_right_dragging
            && (dx.abs() >= RIGHT_DRAG_THRESHOLD || dy.abs() >= RIGHT_DRAG_THRESHOLD)
        {
            self.is_right_dragging = true;
            log::info!("Right click drag started at offsets: ({start_ox}, {start_oy})");
        }
        if !self.is_right_dragging {
            return;
        }
        self.config.position_x_offset = start_ox + dx;
        self.config.position_y_offset = start_oy + dy;
        if let Some(monitor) = Self::get_target_monitor(window, self.config.monitor_index) {
            let mon_size = monitor.size();
            let mon_pos = monitor.position();
            let (new_x, new_y) = self.compute_window_position(mon_pos, mon_size);
            if new_x != self.geom.configured_x || new_y != self.geom.configured_y {
                self.set_configured_window_position(window, new_x, new_y);
            }
        }
        window.request_redraw();
    }

    pub(super) fn update_seeking_input(&mut self, window: &WindowRef, rel_x: i32) {
        if self.seek.active && self.input_pressed() {
            let click_x = rel_x as f32 - self.page_translation(ExpandedPage::Music);
            self.seek.preview_at(click_x);
            window.request_redraw();
        } else if self.finish_seek() {
            window.request_redraw();
        }
    }

    pub(super) fn update_audio_controls(
        &mut self,
        window: &WindowRef,
        rel_x: i32,
        rel_y: i32,
        offset_x: f64,
        island_y: f64,
    ) {
        use crate::ui::expanded::music_view;
        let revision = self.audio_outputs.revision();
        let devices = (revision != self.audio_outputs_revision).then(|| {
            self.audio_outputs_revision = revision;
            self.audio_outputs.devices()
        });
        music_view::set_audio_state(devices, self.compact_overlay.volume_level());
        let on_music = self.expanded && self.page_focused(ExpandedPage::Music);
        if on_music
            && self
                .audio_outputs_refreshed
                .is_none_or(|refreshed| refreshed.elapsed() >= Duration::from_secs(3))
        {
            self.audio_outputs_refreshed = Some(Instant::now());
            self.audio_outputs.refresh();
        }
        let cx = rel_x as f32 - self.page_translation(ExpandedPage::Music);
        music_view::set_audio_pointer(
            on_music.then(|| winisland_render::Point::new(cx, rel_y as f32)),
        );
        if self.volume_dragging {
            if on_music && self.input_pressed() {
                let level = music_view::volume_level_at(
                    offset_x as f32,
                    island_y as f32,
                    self.springs.w.value,
                    self.config.expanded_scale,
                    cx,
                );
                self.compact_overlay.set_volume_level(level);
            } else {
                self.volume_dragging = false;
                music_view::set_volume_dragging(false);
            }
            window.request_redraw();
        }
        if music_view::output_scroll_dragging() {
            if on_music && self.input_pressed() {
                music_view::drag_output_scroll(
                    offset_x as f32,
                    island_y as f32,
                    self.springs.w.value,
                    self.config.expanded_scale,
                    rel_y as f32,
                );
            } else {
                music_view::end_output_scroll_drag();
            }
            window.request_redraw();
        }
    }

    pub(super) fn update_progress_hover(
        &self,
        rel_x: i32,
        rel_y: i32,
        offset_x: f64,
        island_y: f64,
        music_active: bool,
    ) {
        let progress_hover_active = if self.seek.active {
            true
        } else if self.expanded
            && self.page_focused(ExpandedPage::Music)
            && self.media_control_available(
                winisland_plugin_api::types::v2::context::MEDIA_CONTROL_SEEK,
            )
        {
            if let Some((bar_left, bar_right, bar_top, bar_hit_h)) = get_progress_bar_rect(
                offset_x as f32,
                island_y as f32,
                self.springs.w.value,
                music_active,
                self.config.expanded_scale,
            ) {
                let cx = rel_x as f32 - self.page_translation(ExpandedPage::Music);
                let cy = rel_y as f32;
                let margin = 4.0 * self.config.expanded_scale;
                cx >= bar_left - margin
                    && cx <= bar_right + margin
                    && cy >= bar_top - margin
                    && cy <= bar_top + bar_hit_h + margin
            } else {
                false
            }
        } else {
            false
        };
        set_progress_hover(progress_hover_active);
        set_progress_dragging(self.seek.active);
    }

    pub(super) fn update_hide_drag(&mut self, window: &WindowRef, px: i32, py: i32, dt: f32) {
        if self.is_dragging && !self.dismissing_notification && !self.is_hidden() {
            let upward_distance = self.drag_start_py - py;
            let horizontal_distance = px - self.drag_start_px;
            if upward_distance.abs() > 3 || horizontal_distance.abs() > 3 {
                self.drag_has_moved = true;
            }
            if self.drag_axis.is_none() {
                let horizontal = horizontal_distance.abs();
                let vertical = upward_distance.abs();
                if horizontal >= 10 && horizontal as f32 > vertical as f32 * 1.4 {
                    self.drag_axis = Some(DragAxis::Horizontal);
                } else if vertical >= 24 && vertical as f32 > horizontal as f32 * 1.4 {
                    self.drag_axis = Some(DragAxis::Vertical);
                }
            }
            if self.drag_axis == Some(DragAxis::Vertical)
                && upward_distance > 3
                && self.hide.origin.is_none()
            {
                self.prepare_hide(window);
            }
            if self.drag_axis == Some(DragAxis::Vertical) && self.hide.origin.is_some() {
                let drag_layout = self.compute_island_layout();
                if drag_layout.hide_distance > 0.0 {
                    let mut new_val = self.drag_start_hide_val
                        + (upward_distance as f32 / drag_layout.hide_distance as f32);
                    new_val = new_val.clamp(0.0, 1.0);
                    self.springs.hide.value = new_val;
                    self.springs.hide.velocity = 0.0;
                    window.request_redraw();
                }
            }
        } else {
            let hide_target = if self.is_hidden() { 1.0 } else { 0.0 };
            let (stiffness, damping) = if self.is_hidden() {
                (0.12, 0.70)
            } else {
                (0.08, 0.78)
            };
            self.springs
                .hide
                .update_dt(hide_target, stiffness, damping, dt);
            let hide_span = self.compute_island_layout().hide_distance.max(50.0) as f32;
            self.springs.hide.settle(
                hide_target,
                SETTLE_PIXELS / hide_span,
                SETTLE_PIXEL_VELOCITY / hide_span,
            );
        }
        if !(self.is_hidden() || self.hide.fullscreen && self.hide.overlay_reveal) {
            self.restore_hide_origin(window);
        }
        let pulling_chat = self.is_dragging
            && !self.dismissing_notification
            && !self.expanded
            && self.drag_axis == Some(DragAxis::Vertical)
            && self.springs.hide.value <= 0.3
            && self.lingosoul.is_connected();
        self.lingosoul.set_pull(if pulling_chat {
            (py - self.drag_start_py) as f32
        } else {
            0.0
        });
        if self.springs.hide.velocity.abs() > 0.001
            || (self.springs.hide.value > 0.0 && self.springs.hide.value < 1.0)
        {
            window.request_redraw();
        }
    }

    pub(super) fn update_expand_collapse_click(
        &mut self,
        window: &WindowRef,
        is_hovering_visible: bool,
    ) {
        if self.fullscreen_hide_active() {
            return;
        }
        if self.expanded && self.expanded_pages().is_empty() {
            self.expanded = false;
            self.reset_page();
            window.request_redraw();
        }
        let pressing = self.input_pressed();
        if !pressing {
            self.expanded_press_started_inside = false;
            self.expanded_header_press = None;
        }
        if self.expanded && !is_hovering_visible && pressing && !self.expanded_press_started_inside
        {
            self.expanded = false;
            self.reset_page();
            window.request_redraw();
        }
        if !self.expanded && is_hovering_visible && pressing {
            self.idle_timer = Instant::now();
        }
    }
}
