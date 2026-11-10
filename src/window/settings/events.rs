use std::time::{Duration, Instant};

use winisland_platform::{
    CursorKind, InputState, Key, MouseButton, MouseWheelDelta, PlatformEvent, TouchPhase,
    WindowPoint,
};
use winisland_render::Renderer;

use super::{
    MUSIC_PAGE_INDEX, PLUGINS_PAGE_INDEX, POPUP_CLOSE_SPEED, POPUP_OPACITY_KEY, PageNavigation,
    PluginSettingsRequest, PopupState, SETTINGS_HEADER_H, SIDEBAR_KEY_BASE, SIDEBAR_W, SettingsApp,
    WIDGETS_PAGE_INDEX, window_control_at, window_controls_hovered,
};

const CURSOR_MOVE_THRESHOLD: f32 = 0.5;
const MOUSE_WHEEL_LINE_HEIGHT: f32 = 40.0;
const SIDEBAR_TITLE_HEIGHT: f32 = 78.0;
const SIDEBAR_HOVER_SLOTS: usize = 64;
const WINDOW_CONTROL_CLOSE: usize = 0;
const WINDOW_CONTROL_MINIMIZE: usize = 1;

fn scroll_delta(delta: MouseWheelDelta) -> f32 {
    match delta {
        MouseWheelDelta::Lines { y, .. } => y * MOUSE_WHEEL_LINE_HEIGHT,
        MouseWheelDelta::Pixels { y, .. } => y as f32,
    }
}

impl SettingsApp {
    pub(crate) fn handle_window_event(&mut self, event: PlatformEvent, renderer: &mut Renderer) {
        match event {
            PlatformEvent::CloseRequested { .. } | PlatformEvent::Destroyed { .. } => {
                self.close_requested = true;
            }
            PlatformEvent::Focused { focused, .. } => self.handle_focus_changed(focused),
            PlatformEvent::ThemeChanged { theme, .. } if self.config.settings_theme == "system" => {
                self.handle_system_theme_changed(theme);
            }
            PlatformEvent::Resized { .. }
                if self
                    .window
                    .as_ref()
                    .is_some_and(|window| window.is_maximized()) =>
            {
                if let Some(window) = &self.window {
                    window.set_maximized(false);
                }
            }
            PlatformEvent::Resized { size, .. } => self.handle_resized(renderer, size),
            PlatformEvent::ScaleFactorChanged { scale, .. } => self.handle_scale_changed(scale),
            PlatformEvent::Moved { .. } => self.keep_window_on_island_monitor(false),
            PlatformEvent::KeyInput {
                key,
                state: InputState::Pressed,
                text,
                ..
            } => {
                if !self.handle_popup_key(&key, text.as_deref())
                    && !self.handle_search_key(&key, text.as_deref())
                {
                    self.handle_pressed_key(&key);
                }
            }
            PlatformEvent::ModifiersChanged { modifiers, .. } => {
                self.ctrl_pressed = modifiers & winisland_plugin_api::MOD_CONTROL != 0;
            }
            PlatformEvent::ImeCommit { text, .. } => {
                if !self.insert_input_text(&text) {
                    self.insert_search_text(&text);
                }
            }
            PlatformEvent::ImeComposition { text, .. } => self.set_search_composition(&text),
            PlatformEvent::CursorMoved { position, .. } => self.handle_cursor_moved(position),
            PlatformEvent::CursorLeft { .. } => self.handle_cursor_left(),
            PlatformEvent::DroppedFile { path, .. }
                if self.active_page == PLUGINS_PAGE_INDEX
                    && path
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("zip")) =>
            {
                self.handle_plugin_file_drop(path);
            }
            PlatformEvent::MouseWheel { delta, .. } if !self.suppress_synthetic_mouse() => {
                self.handle_mouse_wheel(delta);
            }
            PlatformEvent::MouseInput {
                state: InputState::Pressed,
                button: MouseButton::Left,
                ..
            } if !self.suppress_synthetic_mouse() => self.handle_left_mouse_pressed(),
            PlatformEvent::MouseInput {
                state: InputState::Released,
                button: MouseButton::Left,
                ..
            } if !self.suppress_synthetic_mouse() => self.handle_left_mouse_released(),
            PlatformEvent::Touch {
                touch_id,
                phase,
                position,
                ..
            } => self.handle_touch(touch_id, phase, position),
            PlatformEvent::RedrawRequested { .. } => self.draw(renderer),
            _ => (),
        }
    }

    pub(super) fn handle_focus_changed(&mut self, focused: bool) {
        self.focused = focused;
        if !focused {
            self.cancel_page_order_drag();
            self.commit_number_input();
            self.dots_hovered = false;
            self.scroll_dragging = false;
        }
        self.request_redraw();
    }

    pub(super) fn handle_pressed_key(&mut self, key: &Key) {
        if matches!(key, Key::Escape) && self.cancel_page_order_drag() {
            return;
        }
        if self.resource_editor_open {
            if matches!(key, Key::Escape) {
                if self.popup.take().is_some() {
                    self.anim.set_with_speed(POPUP_OPACITY_KEY, 0.0, 0.3);
                    self.request_redraw();
                } else {
                    self.handle_resource_editor_escape();
                }
            }
            return;
        }
        if self.handle_number_input_key(key) {
            return;
        }
        match key {
            Key::ArrowLeft => {
                self.navigate_page_history(PageNavigation::Back);
            }
            Key::ArrowRight => {
                self.navigate_page_history(PageNavigation::Forward);
            }
            _ => {}
        }
    }

    pub(super) fn handle_cursor_moved(&mut self, position: WindowPoint) {
        let scale = self.window_scale();
        let new_position = (position.x as f32 / scale, position.y as f32 / scale);
        let mouse_moved = (new_position.0 - self.last_hover_mouse_pos.0).abs()
            > CURSOR_MOVE_THRESHOLD
            || (new_position.1 - self.last_hover_mouse_pos.1).abs() > CURSOR_MOVE_THRESHOLD;
        self.logical_mouse_pos = new_position;
        self.update_scroll_drag(new_position.1);
        if let Some(popup) = self.popup.as_mut()
            && popup.drag_scroll(new_position.1)
        {
            self.request_redraw();
        }

        let mut redraw = mouse_moved;
        let dots_hovered = self.focused && window_controls_hovered(new_position.0, new_position.1);
        if dots_hovered != self.dots_hovered {
            self.dots_hovered = dots_hovered;
            redraw = true;
        }
        redraw |= self.update_widget_hover();
        redraw |= self.update_page_order_drag();
        redraw |= self.update_resource_editor_drag();
        redraw |= self.update_popup_hover();
        if mouse_moved {
            self.last_hover_mouse_pos = new_position;
            redraw |= self.update_sidebar_hover();
        }
        if redraw {
            self.request_redraw();
        }

        let cursor = if self.get_hover_state() {
            CursorKind::Pointer
        } else {
            CursorKind::Default
        };
        if let Some(window) = &self.window {
            crate::platform::window().set_cursor(window.id(), cursor);
        }
    }

    pub(super) fn handle_touch(&mut self, touch_id: u64, phase: TouchPhase, location: WindowPoint) {
        self.last_touch_at = Some(Instant::now());
        match phase {
            TouchPhase::Started if self.touch_id.is_none() => {
                self.touch_id = Some(touch_id);
                self.touch_start = location;
                self.touch_last = location;
                self.touch_scrolling = false;
                self.touch_pressed = false;
                self.handle_cursor_moved(location);
                let (x, y) = self.logical_mouse_pos;
                if self.begin_scroll_drag(x, y) {
                    self.touch_pressed = true;
                    self.request_redraw();
                } else if self.active_page == WIDGETS_PAGE_INDEX
                    && !self.resource_editor_open
                    && self.handle_widget_drag_press()
                {
                    self.touch_pressed = true;
                    self.widget_drag_lift_progress = 0.0;
                    self.widget_drop_animation = None;
                    self.set_widget_hover_target(None);
                    self.request_redraw();
                } else if self.music_notice_button_hovered()
                    || window_control_at(x, y).is_some()
                    || (self.is_window_drag_region(x, y) && self.popup.is_none())
                {
                    self.touch_pressed = true;
                    self.handle_left_mouse_pressed();
                }
            }
            TouchPhase::Moved if self.touch_id == Some(touch_id) => {
                self.handle_cursor_moved(location);
                if !self.touch_pressed {
                    let scale = self.window_scale() as f64;
                    let dx = location.x - self.touch_start.x;
                    let dy = location.y - self.touch_start.y;
                    if dx.hypot(dy) > 8.0 * scale {
                        self.touch_scrolling = true;
                    }
                    if self.touch_scrolling
                        && self.popup.is_none()
                        && !self.resource_editor_open
                        && self.logical_mouse_pos.0 >= SIDEBAR_W
                        && self.logical_mouse_pos.1 >= SETTINGS_HEADER_H
                    {
                        let delta = (location.y - self.touch_last.y) as f32 / self.window_scale();
                        if self.active_page == PLUGINS_PAGE_INDEX
                            && self.plugin_detail_contains(self.logical_mouse_pos.0)
                        {
                            self.plugin_detail_scroll = (self.plugin_detail_scroll - delta)
                                .clamp(0.0, self.plugin_detail_max_scroll);
                        } else {
                            self.ensure_items_cache();
                            self.target_scroll_y =
                                (self.target_scroll_y - delta).clamp(0.0, self.cached_max_scroll);
                            self.scroll_y = self.target_scroll_y;
                        }
                        self.request_redraw();
                    }
                }
                self.touch_last = location;
            }
            TouchPhase::Ended if self.touch_id == Some(touch_id) => {
                self.handle_cursor_moved(location);
                if self.touch_pressed {
                    self.handle_left_mouse_released();
                } else {
                    let scale = self.window_scale() as f64;
                    let dx = location.x - self.touch_start.x;
                    let dy = location.y - self.touch_start.y;
                    if !self.touch_scrolling && dx.hypot(dy) <= 8.0 * scale {
                        self.handle_left_mouse_pressed();
                        self.handle_left_mouse_released();
                    }
                }
                self.touch_pressed = false;
                self.touch_scrolling = false;
                self.touch_id = None;
            }
            TouchPhase::Cancelled if self.touch_id == Some(touch_id) => {
                self.music_notice_pressed = false;
                self.scroll_dragging = false;
                self.widget_dragging = None;
                self.compact_widget_dragging = None;
                self.widget_drag_hover_slot = None;
                self.touch_pressed = false;
                self.touch_scrolling = false;
                self.touch_id = None;
                self.request_redraw();
            }
            _ => {}
        }
    }

    pub(super) fn suppress_synthetic_mouse(&self) -> bool {
        self.touch_id.is_some()
            || self
                .last_touch_at
                .is_some_and(|time| time.elapsed() < Duration::from_millis(250))
    }

    pub(super) fn update_popup_hover(&mut self) -> bool {
        let Some(popup) = &mut self.popup else {
            return false;
        };
        if popup.keyboard_anchor == Some(self.logical_mouse_pos) {
            return false;
        }
        popup.keyboard_anchor = None;
        let (mouse_x, mouse_y) = self.logical_mouse_pos;
        let hover_index = popup.hit_test_item(mouse_x, mouse_y);
        if hover_index == popup.hover_idx {
            return false;
        }
        popup.hover_idx = hover_index;
        true
    }

    pub(super) fn update_sidebar_hover(&mut self) -> bool {
        let (mouse_x, mouse_y) = self.logical_mouse_pos;
        let point = winisland_render::Point::new(mouse_x, mouse_y);
        let rows = self.sidebar_rows();
        let hover_index = (mouse_x < SIDEBAR_W)
            .then(|| rows.iter().position(|(_, rect)| rect.contains(point)))
            .flatten();
        let sidebar_hover = hover_index.map_or(-1, |index| index as i32);
        if sidebar_hover == self.sidebar_hover {
            return false;
        }
        self.sidebar_hover = sidebar_hover;
        for index in 0..rows.len().max(SIDEBAR_HOVER_SLOTS) {
            self.anim.set(
                SIDEBAR_KEY_BASE + index as u64,
                if hover_index == Some(index) { 1.0 } else { 0.0 },
            );
        }
        true
    }

    pub(super) fn handle_cursor_left(&mut self) {
        if self.active_page == MUSIC_PAGE_INDEX && self.show_music_notice() {
            self.logical_mouse_pos = (-1.0, -1.0);
            self.request_redraw();
        }
        let hover_changed = self.set_widget_hover_target(None);
        let slot_changed = self.active_widget_preview_hover_slot().is_some();
        self.set_active_widget_preview_hover_slot(None);
        if self.dots_hovered || hover_changed || slot_changed {
            self.dots_hovered = false;
            self.request_redraw();
        }
    }

    pub(super) fn handle_plugin_file_drop(&mut self, path: std::path::PathBuf) {
        self.plugin_status = Some((winisland_core::i18n::tr("plugin_installing"), false));
        self.plugin_request = Some(PluginSettingsRequest::Install(path));
        self.mark_items_dirty();
        self.request_redraw();
    }

    pub(super) fn handle_mouse_wheel(&mut self, delta: MouseWheelDelta) {
        if self.resource_editor_open {
            return;
        }
        let (mouse_x, mouse_y) = self.logical_mouse_pos;
        if let Some(popup) = self.popup.as_mut()
            && popup.max_scroll() > 0.0
            && popup
                .menu_rect()
                .contains(winisland_render::Point::new(mouse_x, mouse_y))
        {
            if popup.scroll_by(-scroll_delta(delta)) {
                self.update_popup_hover();
                self.request_redraw();
            }
            return;
        }
        if self.popup.is_some() {
            self.popup = None;
            self.pending_plugin_setting = None;
            self.anim
                .set_with_speed(POPUP_OPACITY_KEY, 0.0, POPUP_CLOSE_SPEED);
            self.request_redraw();
            return;
        }
        let delta = scroll_delta(delta);
        if self.active_page == PLUGINS_PAGE_INDEX && self.plugin_detail_contains(mouse_x) {
            self.plugin_detail_scroll =
                (self.plugin_detail_scroll - delta).clamp(0.0, self.plugin_detail_max_scroll);
            self.request_redraw();
        } else if mouse_x >= SIDEBAR_W {
            self.target_scroll_y =
                (self.target_scroll_y - delta).clamp(0.0, self.cached_max_scroll);
            self.scroll_y = self.target_scroll_y;
            self.request_redraw();
        }
    }

    pub(super) fn handle_left_mouse_pressed(&mut self) {
        let (mouse_x, mouse_y) = self.logical_mouse_pos;
        if let Some(popup) = self.popup.as_mut()
            && popup.begin_scroll_drag(mouse_x, mouse_y)
        {
            self.request_redraw();
            return;
        }
        if self.resource_editor_open {
            self.handle_click();
            return;
        }
        if self.begin_scroll_drag(mouse_x, mouse_y) {
            self.request_redraw();
            return;
        }
        match window_control_at(mouse_x, mouse_y) {
            Some(WINDOW_CONTROL_CLOSE) => self.close_requested = true,
            Some(WINDOW_CONTROL_MINIMIZE) => {
                if let Some(window) = &self.window {
                    window.set_minimized(true);
                }
            }
            Some(_) => {}
            None if self.is_window_drag_region(mouse_x, mouse_y) && self.popup.is_none() => {
                if let Some(window) = &self.window {
                    crate::platform::window().begin_drag(window.id());
                }
            }
            None if self.handle_page_order_press() => {}
            None if self.handle_widget_drag_press() => {
                self.widget_drag_lift_progress = 0.0;
                self.widget_drop_animation = None;
                self.set_widget_hover_target(None);
                self.request_redraw();
            }
            None => self.handle_click(),
        }
    }

    pub(super) fn is_window_drag_region(&self, mouse_x: f32, mouse_y: f32) -> bool {
        if self.resource_editor_open {
            return false;
        }
        let in_sidebar_title = mouse_x < SIDEBAR_W
            && mouse_y < SIDEBAR_TITLE_HEIGHT
            && !self
                .search_rect()
                .contains(winisland_render::Point::new(mouse_x, mouse_y));
        let in_content_title = mouse_x >= SIDEBAR_W
            && mouse_y < SETTINGS_HEADER_H
            && Self::page_navigation_at(mouse_x, mouse_y).is_none()
            && self.widget_mode_at(mouse_x, mouse_y).is_none();
        in_sidebar_title || in_content_title
    }

    pub(super) fn handle_left_mouse_released(&mut self) {
        if self.popup.as_mut().is_some_and(PopupState::end_scroll_drag) {
            self.request_redraw();
            return;
        }
        if std::mem::take(&mut self.music_notice_pressed) {
            if self.music_notice_button_hovered() {
                self.config.music_notice_acknowledged = true;
                self.persist_settings_change();
            } else {
                self.request_redraw();
            }
            return;
        }
        let scroll_released = std::mem::take(&mut self.scroll_dragging);
        if scroll_released
            || self.handle_widget_drag_release()
            || self.handle_page_order_release()
            || self.handle_resource_editor_release()
        {
            self.request_redraw();
        }
    }
}
