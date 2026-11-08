use std::time::{Duration, Instant};

use crate::core::memory::MemoryActivity;
use crate::utils::mouse::{
    double_click_interval, get_global_cursor_pos, is_left_button_pressed,
    is_point_in_continuous_rounded_rect, is_point_in_rect,
};
use winisland_core::config::MIN_HIDDEN_WIDTH;
use winisland_utils::timing::take_elapsed;

use super::App;
use pacing::FramePacing;

mod animation;
mod input;
mod media;
mod pacing;
mod plugins;
mod visibility;

const SETTLE_PIXELS: f32 = 0.1;
const SETTLE_PIXEL_VELOCITY: f32 = 0.05;
const HIDDEN_FRAME_INTERVAL: Duration = Duration::from_millis(100);
const RENDERER_RECOVERY_INTERVAL: Duration = Duration::from_secs(1);

impl App {
    pub(super) fn on_about_to_wait(&mut self) {
        if !is_left_button_pressed() {
            self.plugin_ui.pressed_buttons &= !(1 << 1);
        }
        let window = match self.window {
            Some(w) => w,
            None => return,
        };
        let now = Instant::now();
        if let Some(error) = self
            .renderer
            .as_mut()
            .and_then(winisland_render::Renderer::take_failure)
        {
            self.invalidate_renderer(&error, now);
        }
        self.recover_renderer(&window, now, RENDERER_RECOVERY_INTERVAL);
        if now < self.next_frame_deadline {
            return;
        }
        self.update_plugin_services();
        if !self.display_transition
            && self
                .geom
                .position_restore_after
                .is_some_and(|restore_after| now >= restore_after)
        {
            self.geom.position_restore_after = None;
            self.set_configured_window_position(
                &window,
                self.geom.configured_x,
                self.geom.configured_y,
            );
        }
        if self.topmost_check.due(now) && !self.update_display_transition(&window, now) {
            Self::enforce_overlay_window(&window);
        }
        self.handle_tray_events(&window);
        self.handle_hide_hotkey(&window, now);
        self.poll_clipboard_link(&window);
        self.reload_config_if_changed(&window);
        if self.is_hidden() && !self.can_hide() {
            self.reveal_island();
        }

        self.poll_pending_plugin_install();
        self.poll_pending_plugin_marketplace();
        if let Some(pages) = self.next_v2_settings_pages()
            && let Some(settings) = self.settings.as_mut()
        {
            settings.set_plugin_settings_pages(pages);
        }
        let activities_changed = self.refresh_v2_activities();
        let secondary_changed = self.update_secondary_activities();
        if self.activity_mgr.tick() || activities_changed || secondary_changed {
            self.resize_activity_window();
            window.request_redraw();
        }

        let dt =
            (take_elapsed(&mut self.last_update_time, now).as_secs_f32() * 60.0).clamp(0.1, 6.0);

        if !self.visible {
            self.memory.set_activity(MemoryActivity::default());
            self.compact_overlay.finish_volume_drag();
            self.compact_overlay.finish_brightness_drag();
            self.audio.set_gate_override(false);
            self.next_frame_deadline = now + HIDDEN_FRAME_INTERVAL;
            self.precise_frame_pacing = false;
            return;
        }
        let (px, py) = if self.touch_id.is_some() {
            (
                (self.touch_pos.x + self.geom.win_x as f64) as i32,
                (self.touch_pos.y + self.geom.win_y as f64) as i32,
            )
        } else {
            get_global_cursor_pos()
        };
        self.update_right_drag(&window, px, py);

        let (music_active, media_is_playing) = self.poll_media_info(&window);
        if self.compact_overlay.update_alert() {
            window.request_redraw();
        }
        let activities_changed = self.update_local_activities(music_active, media_is_playing);
        if activities_changed {
            window.request_redraw();
        }

        if self.fullscreen_check.due(now) {
            self.update_fullscreen_suppression(&window, now);
        }

        let rel_x = px - self.geom.win_x;
        let rel_y = py - self.geom.win_y;
        let layout = self.compute_island_layout();
        let island_y = layout.island_y;
        let offset_x = layout.offset_x;
        let current_island_x = layout.current_island_x;
        let current_island_y = layout.current_island_y;
        let is_hovering_island = is_point_in_continuous_rounded_rect(
            rel_x as f64,
            rel_y as f64,
            current_island_x,
            current_island_y,
            self.springs.w.value as f64,
            self.springs.h.value as f64,
            self.springs.r.value as f64,
        ) || self.pager_contains(rel_x, rel_y, &layout);
        let is_over_hidden_reveal = self.is_hidden()
            && (is_hovering_island
                || (self.config.hidden_width <= MIN_HIDDEN_WIDTH
                    && self.springs.hide.value >= 0.999
                    && is_point_in_rect(
                        rel_x as f64,
                        rel_y as f64,
                        layout.hidden_reveal_x,
                        layout.hidden_reveal_y,
                        layout.hidden_reveal_w,
                        layout.hidden_reveal_h,
                    )));
        let cursor_interaction_suppressed = self.is_cursor_suppressed && self.touch_id.is_none();
        let interaction_suppressed = cursor_interaction_suppressed
            || (self.fullscreen_hide_active() && !self.hide.overlay_reveal);
        let is_hovering_visible = !interaction_suppressed && is_hovering_island;
        let passive_reveal_active = self.is_fullscreen_suppressed
            && !self.fullscreen_hide_active()
            && is_over_hidden_reveal;
        if self.hidden_reveal_click.update(
            passive_reveal_active,
            self.input_pressed(),
            (px as f32, py as f32),
            now,
            double_click_interval(),
        ) {
            self.reveal_island();
            window.request_redraw();
            log::info!("Island revealed by fullscreen edge double-click");
        }

        let plugin_input = self
            .plugin_host
            .as_ref()
            .map(|host| host.runtime().extensions.input_state())
            .unwrap_or_default();
        let compact_overlay_visible = self.update_compact_and_auto_hide(
            &window,
            is_hovering_visible || plugin_input.captured || plugin_input.keyboard,
        );

        if self.compact_overlay.is_volume_dragging() {
            if self.input_pressed() && !interaction_suppressed {
                self.update_volume_drag_position(rel_x, &layout);
            } else {
                self.compact_overlay.finish_volume_drag();
            }
            window.request_redraw();
        }
        if self.compact_overlay.is_brightness_dragging() {
            if self.input_pressed() && !interaction_suppressed {
                self.update_brightness_drag_position(rel_x, &layout);
            } else {
                self.compact_overlay.finish_brightness_drag();
            }
            window.request_redraw();
        }

        self.update_seeking_input(&window, rel_x);
        self.update_audio_controls(&window, rel_x, rel_y, offset_x, island_y);
        self.update_progress_hover(rel_x, rel_y, offset_x, island_y, music_active);
        self.update_pager_hover(&window, rel_x, rel_y, &layout, !interaction_suppressed, dt);
        self.update_hide_drag(&window, px, py, dt);
        self.update_expand_collapse_click(&window, is_hovering_visible);

        let is_paused = music_active && !media_is_playing;
        self.update_multitask(&window, now);
        self.lingosoul
            .set_screen_height(self.geom.monitor_size.1 as f32);
        self.lingosoul.set_expanded(self.expanded);
        if let Some(seconds) = self.lingosoul.update(dt) {
            let data = seconds.to_string();
            if let Some(host) = &self.plugin_host {
                let _ = host
                    .runtime()
                    .extensions
                    .invoke(crate::plugin::timer::START_COMMAND, data.as_bytes());
            }
        }
        self.update_assistant_reveal(&window);
        self.serve_assistant_requests();
        self.sync_chat_focus();
        self.update_lyrics(&window, music_active, dt);
        self.update_spring_targets(&window, music_active, is_paused, dt);
        self.update_compact_widget_refresh(&window, now);
        self.update_hit_regions(&window, plugin_input.captured);

        self.schedule_next_frame(
            &window,
            now,
            FramePacing {
                media_is_playing,
                is_hovering_visible,
                compact_overlay_visible,
                passive_reveal_active,
            },
        );
    }
}
