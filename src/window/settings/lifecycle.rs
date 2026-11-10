use std::time::Instant;

use crate::platform::{MonitorRef, WindowRef, window};
use crate::utils::color::{SettingsTheme, dark_settings_theme, light_settings_theme};
use winisland_platform::{
    LogicalWindowSize, SettingsSpec, Theme, WindowId, WindowPosition, WindowSize,
};
use winisland_render::{Renderer, RendererTargetId};

use super::{SIDEBAR_W, SettingsApp, WIN_H, WIN_W, pages, sidebar};

impl SettingsApp {
    pub(crate) fn window_scale(&self) -> f32 {
        self.window
            .as_ref()
            .map(|window| window.scale_factor() as f32)
            .unwrap_or(1.0)
    }

    pub(crate) fn logical_window_size(&self) -> (f32, f32) {
        let scale = self.window_scale();
        (self.win_w / scale, self.win_h / scale)
    }

    pub(crate) fn content_width(&self) -> f32 {
        self.logical_window_size().0 - SIDEBAR_W
    }

    pub(crate) fn theme(&self) -> SettingsTheme {
        if self.is_light {
            light_settings_theme()
        } else {
            dark_settings_theme()
        }
    }

    pub(crate) fn update_theme(&mut self) {
        self.is_light = match self.config.settings_theme.as_str() {
            "light" => true,
            "dark" => false,
            _ => {
                if let Some(win) = &self.window {
                    window().theme(win.id()) == Some(Theme::Light)
                } else {
                    false
                }
            }
        };
        if let Some(win) = &self.window {
            Self::apply_titlebar_theme(*win, self.is_light);
            win.request_redraw();
        }
    }

    pub(crate) fn apply_titlebar_theme(window_ref: WindowRef, is_light: bool) {
        window().set_titlebar_theme(window_ref.id(), is_light);
    }

    pub(crate) fn get_monitor_list() -> Vec<String> {
        use windows::Win32::Graphics::Gdi::{
            DISPLAY_DEVICE_ACTIVE, DISPLAY_DEVICE_STATE_FLAGS, DISPLAY_DEVICEW, EnumDisplayDevicesW,
        };
        let mut monitors: Vec<String> = Vec::new();
        // SAFETY: Enumeration is synchronous; outputs have valid sizes and device-name buffers stay live.
        unsafe {
            let mut idx = 0u32;
            let mut active_count = 0;
            loop {
                let mut dd: DISPLAY_DEVICEW = std::mem::zeroed();
                dd.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;
                if EnumDisplayDevicesW(None, idx, &mut dd, 0).as_bool() {
                    if (dd.StateFlags & DISPLAY_DEVICE_ACTIVE) != DISPLAY_DEVICE_STATE_FLAGS(0) {
                        active_count += 1;
                        let name = String::from_utf16_lossy(&dd.DeviceName)
                            .trim_end_matches('\0')
                            .to_string();
                        let mut dm: DISPLAY_DEVICEW = std::mem::zeroed();
                        dm.cb = std::mem::size_of::<DISPLAY_DEVICEW>() as u32;
                        let mut label = if EnumDisplayDevicesW(
                            windows::core::PCWSTR(dd.DeviceName.as_ptr()),
                            0,
                            &mut dm,
                            0,
                        )
                        .as_bool()
                        {
                            let friendly = String::from_utf16_lossy(&dm.DeviceString)
                                .trim_end_matches('\0')
                                .to_string();
                            if friendly.is_empty() {
                                name.clone()
                            } else {
                                friendly
                            }
                        } else {
                            name.clone()
                        };
                        label = format!("Display {active_count}: {label}");
                        monitors.push(label);
                    }
                    idx += 1;
                } else {
                    break;
                }
            }
        }
        if monitors.is_empty() {
            monitors.push("Primary".to_string());
        }
        monitors
    }

    pub(crate) fn create_window(
        &mut self,
        renderer: &mut Renderer,
        target_monitor: Option<MonitorRef>,
    ) {
        self.target_monitor = target_monitor;
        let id = window()
            .create_settings(SettingsSpec {
                title: "WinIsland Settings",
                logical_size: LogicalWindowSize::new(WIN_W as f64, WIN_H as f64),
                monitor: self.target_monitor.as_ref().map(MonitorRef::id),
                resizable: true,
            })
            .expect("Settings window creation failed");
        let window_ref = WindowRef(id);
        self.window = Some(window_ref);
        self.window_backdrop = match window().start_window_backdrop(id) {
            Ok(()) => true,
            Err(error) => {
                log::info!("Settings sidebar blur unavailable: {error}");
                false
            }
        };
        self.keep_window_on_island_monitor(true);
        let size = window_ref.inner_size();
        self.win_w = size.width as f32;
        self.win_h = size.height as f32;
        let scale = window_ref.scale_factor();
        self.logical_win_w = size.width as f64 / scale;
        self.logical_win_h = size.height as f64 / scale;
        self.renderer_target = match window()
            .native_surface(id)
            .ok_or_else(|| "Settings window has no native surface".to_string())
            .and_then(|surface| {
                renderer
                    .create_target(surface, size.width, size.height)
                    .map_err(|error| error.to_string())
            }) {
            Ok(target) => Some(target),
            Err(error) => {
                log::error!("Settings renderer initialization failed: {error}");
                self.close_requested = true;
                return;
            }
        };
        self.close_requested = false;
        self.next_frame_deadline = Instant::now();
        self.update_theme();
        self.update_detected_apps();
    }

    pub(crate) fn invalidate_renderer_target(&mut self) {
        self.renderer_target = None;
        sidebar::clear_sidebar_icon_cache();
        pages::plugins::clear_plugin_icon_cache();
    }

    pub(crate) fn recreate_renderer_target(
        &mut self,
        renderer: &mut Renderer,
    ) -> Result<(), String> {
        let Some(window) = self.window.as_ref() else {
            return Ok(());
        };
        let size = window.inner_size();
        let surface = crate::platform::window()
            .native_surface(window.id())
            .ok_or_else(|| "Settings window has no native surface".to_string())?;
        self.renderer_target = Some(
            renderer
                .create_target(surface, size.width, size.height)
                .map_err(|error| error.to_string())?,
        );
        window.request_redraw();
        Ok(())
    }

    pub(super) fn handle_system_theme_changed(&mut self, theme: Theme) {
        self.is_light = theme == Theme::Light;
        if let Some(window) = &self.window {
            Self::apply_titlebar_theme(*window, self.is_light);
        }
        self.request_redraw();
    }

    pub(super) fn handle_resized(&mut self, renderer: &mut Renderer, size: WindowSize) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        if window.is_minimized() == Some(true) || size.width == 0 || size.height == 0 {
            return;
        }
        if let Some(expected) = self.pending_dpi_size.take() {
            if (expected.width.abs_diff(size.width) > 2
                || expected.height.abs_diff(size.height) > 2)
                && !std::mem::replace(&mut self.pending_dpi_retried, true)
            {
                self.pending_dpi_size = Some(expected);
                let _ = window.request_inner_size(expected);
            }
        } else if size.width > 0 && size.height > 0 {
            let scale = window.scale_factor();
            self.logical_win_w = size.width as f64 / scale;
            self.logical_win_h = size.height as f64 / scale;
        }
        self.win_w = size.width as f32;
        self.win_h = size.height as f32;
        self.mark_items_dirty();
        self.resize_renderer_target(renderer, size);
        self.keep_window_on_island_monitor(false);
        self.request_redraw();
    }

    pub(super) fn handle_scale_changed(&mut self, scale_factor: f64) {
        let size = WindowSize::new(
            (self.logical_win_w * scale_factor).round() as u32,
            (self.logical_win_h * scale_factor).round() as u32,
        );
        if self
            .window
            .is_some_and(|window_ref| window_ref.request_inner_size(size))
        {
            self.pending_dpi_size = Some(size);
            self.pending_dpi_retried = false;
            self.dpi_size_check = true;
        }
    }

    pub(crate) fn set_target_monitor(&mut self, monitor: MonitorRef) {
        let changed = self.target_monitor.as_ref().is_none_or(|current| {
            current.position() != monitor.position() || current.size() != monitor.size()
        });
        if changed {
            self.target_monitor = Some(monitor);
            self.keep_window_on_island_monitor(true);
        }
    }

    pub(super) fn keep_window_on_island_monitor(&self, center_if_outside: bool) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        if window.is_minimized() == Some(true) {
            return;
        }
        let Some(target) = self.target_monitor.as_ref() else {
            return;
        };
        let Some(position) = window.outer_position() else {
            return;
        };
        let Some(size) = window.outer_size() else {
            return;
        };
        let work = target.0.work_area;
        let max_x = (work.right - size.width as i32).max(work.left);
        let max_y = (work.bottom - size.height as i32).max(work.top);
        let outside = position.x < work.left
            || position.x > max_x
            || position.y < work.top
            || position.y > max_y;
        let (x, y) = if center_if_outside && outside {
            ((work.left + max_x) / 2, (work.top + max_y) / 2)
        } else {
            (
                position.x.clamp(work.left, max_x),
                position.y.clamp(work.top, max_y),
            )
        };
        if position.x != x || position.y != y {
            window.set_outer_position(WindowPosition::new(x, y));
        }
    }

    pub(super) fn resize_renderer_target(&mut self, renderer: &mut Renderer, size: WindowSize) {
        let Some(target) = self.renderer_target else {
            return;
        };
        if let Err(error) = renderer.resize(target, size.width, size.height) {
            log::error!("Settings renderer resize failed: {error}");
        }
    }

    pub(crate) fn request_redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    pub(crate) fn window_id(&self) -> Option<WindowId> {
        self.window.as_ref().map(|window| window.id())
    }

    pub(crate) fn bring_to_front(&self) {
        if let Some(window) = &self.window {
            window.set_minimized(false);
            let _ = crate::platform::display().bring_foreign_window_to_front("WinIsland Settings");
            window.request_redraw();
        }
    }

    pub(crate) fn close_requested(&self) -> bool {
        self.close_requested
    }

    pub(crate) fn close(&mut self) -> Option<RendererTargetId> {
        self.commit_number_input();
        self.popup = None;
        self.widget_dragging = None;
        self.compact_widget_dragging = None;
        self.widget_hover_target = None;
        self.widget_hover_visual = None;
        self.widget_drop_animation = None;
        sidebar::clear_sidebar_icon_cache();
        sidebar::clear_plugin_settings_icon_cache();
        pages::plugins::clear_plugin_icon_cache();
        let renderer_target = self.renderer_target.take();
        if let Some(window_ref) = self.window.take() {
            window().destroy_window(window_ref.id());
        }
        renderer_target
    }
}
