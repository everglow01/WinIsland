use std::time::{Duration, Instant};

use crate::utils::settings_ui::WidgetEditorHover;
use winisland_utils::timing::take_elapsed;

use super::{PLUGIN_DETAIL_KEY, SettingsApp, WIDGETS_PAGE_INDEX};

const WIDGET_HOVER_RATE: f32 = 18.0;
const WIDGET_DRAG_LIFT_RATE: f32 = 22.0;
const WIDGET_DROP_DURATION: f32 = 0.28;

pub(super) fn animate_towards(value: &mut f32, target: f32, rate: f32, dt: f32) -> bool {
    let previous = *value;
    *value += (target - *value) * (1.0 - (-rate * dt).exp());
    if (target - *value).abs() < 0.001 {
        *value = target;
    }
    (*value - previous).abs() > f32::EPSILON
}

impl SettingsApp {
    pub(crate) fn update_detected_apps(&mut self) {
        let mut changed = false;
        for app in &self.config.smtc_known_apps {
            if !self.detected_apps.contains(app) {
                self.detected_apps.push(app.clone());
                changed = true;
            }
        }
        if changed {
            self.items_dirty = true;
        }
        if !self.detected_apps_scan.is_running() {
            self.detected_apps_scan = crate::core::smtc::detect_active_apps_async();
        }
    }

    pub(super) fn poll_detected_apps(&mut self) {
        let Some(apps) = self.detected_apps_scan.poll_ok() else {
            return;
        };
        let mut changed = false;
        for app in apps {
            if !self.detected_apps.contains(&app) {
                self.detected_apps.push(app);
                changed = true;
            }
        }
        if changed {
            self.items_dirty = true;
            self.request_redraw();
        }
    }

    pub(super) fn poll_plugin_inventory(&mut self) {
        if let Some(plugins) = self.plugin_inventory_scan.poll_ok() {
            self.set_plugins(plugins);
        }
    }

    pub(super) fn update_widget_hover(&mut self) -> bool {
        if self.resource_editor_open {
            return self.set_widget_hover_target(None);
        }
        if self.widget_drag_active() {
            self.set_widget_hover_target(None);
            let new_slot = self.widget_preview_slot_at_mouse();
            let current_slot = self.active_widget_drag_hover_slot();
            if new_slot != current_slot {
                self.set_active_widget_drag_hover_slot(new_slot);
            }
            return true;
        }
        if self.active_page != WIDGETS_PAGE_INDEX {
            return self.set_widget_hover_target(None);
        }
        let new_hover = self.widget_editor_hover_at_mouse();
        let new_slot = self.widget_preview_slot_at_mouse();
        let current_slot = self.active_widget_preview_hover_slot();
        if new_slot != current_slot {
            self.set_active_widget_preview_hover_slot(new_slot);
        }
        self.set_widget_hover_target(new_hover) || new_slot != current_slot
    }

    pub(super) fn set_widget_hover_target(&mut self, target: Option<WidgetEditorHover>) -> bool {
        if self.widget_hover_target == target {
            return false;
        }
        self.widget_hover_target = target.clone();
        if target.is_some() {
            self.widget_hover_visual = target;
            self.widget_hover_progress = 0.0;
        }
        true
    }

    pub(super) fn widget_interaction_animating(&self) -> bool {
        let hover_target = f32::from(self.widget_hover_target.is_some());
        let drag_target = f32::from(self.widget_drag_active());
        (self.widget_hover_progress - hover_target).abs() > 0.001
            || (self.widget_drag_lift_progress - drag_target).abs() > 0.001
            || self.widget_drop_animation.is_some()
    }

    pub(super) fn update_widget_interaction_animations(&mut self, dt: f32) -> bool {
        let hover_target = f32::from(self.widget_hover_target.is_some());
        let drag_target = f32::from(self.widget_drag_active());
        let mut changed = animate_towards(
            &mut self.widget_hover_progress,
            hover_target,
            WIDGET_HOVER_RATE,
            dt,
        );
        changed |= animate_towards(
            &mut self.widget_drag_lift_progress,
            drag_target,
            WIDGET_DRAG_LIFT_RATE,
            dt,
        );
        if hover_target == 0.0 && self.widget_hover_progress == 0.0 {
            self.widget_hover_visual = None;
        }
        if let Some(animation) = &mut self.widget_drop_animation {
            animation.progress = (animation.progress + dt / WIDGET_DROP_DURATION).min(1.0);
            changed = true;
            if animation.progress >= 1.0 {
                self.widget_drop_animation = None;
            }
        }
        changed
    }

    pub(crate) fn update(&mut self) -> Option<Instant> {
        let window = self.window.as_ref()?;
        if window.is_minimized() == Some(true) {
            return None;
        }

        self.frame_count += 1;
        self.poll_detected_apps();
        self.poll_plugin_inventory();
        if self.frame_count.is_multiple_of(120) {
            self.update_detected_apps();
        }

        let has_anim = self.switch_anim.is_animating()
            || self.anim.is_animating()
            || self.widget_interaction_animating()
            || self.page_order_animating();
        let has_popup = self.popup.is_some();
        let is_scrolling = (self.target_scroll_y - self.scroll_y).abs() > 0.1;
        let is_widget_dragging = self.widget_drag_active();
        let is_number_input_active =
            self.number_input.is_some() || self.search.focused || self.search.flash_active();

        if !(has_anim || has_popup || is_scrolling || is_widget_dragging || is_number_input_active)
        {
            return None;
        }

        let now = Instant::now();
        if now < self.next_frame_deadline {
            return Some(self.next_frame_deadline);
        }

        let mut redraw = is_widget_dragging || is_number_input_active || self.switch_anim.tick();
        if self.anim.tick() {
            redraw = true;
        }
        if self.plugin_detail_closing && self.anim.get(PLUGIN_DETAIL_KEY) <= 0.005 {
            self.plugin_detail_closing = false;
            self.selected_plugin_id = None;
            self.plugin_detail_scroll = 0.0;
        }

        self.ensure_items_cache();
        let max_scroll = self.cached_max_scroll;
        self.target_scroll_y = self.target_scroll_y.clamp(0.0, max_scroll);

        let dt = take_elapsed(&mut self.last_frame_time, now)
            .as_secs_f32()
            .clamp(0.001, 0.05);
        redraw |= self.update_widget_interaction_animations(dt);
        redraw |= self.update_page_order_animation(dt);

        if (self.scroll_y - self.target_scroll_y).abs() > f32::EPSILON {
            self.scroll_y = self.target_scroll_y;
            redraw = true;
        }

        if redraw {
            self.request_redraw();
            self.next_frame_deadline = now + Duration::from_millis(16);
            Some(self.next_frame_deadline)
        } else {
            None
        }
    }
}
