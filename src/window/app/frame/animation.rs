use crate::platform::WindowRef;

use super::super::App;
use super::{SETTLE_PIXEL_VELOCITY, SETTLE_PIXELS};

impl App {
    pub(super) fn update_spring_targets(
        &mut self,
        window: &WindowRef,
        music_active: bool,
        is_paused: bool,
        dt: f32,
    ) {
        let was_animating = self.springs.any_animating();
        let compact_components_visible = self.expanded || !self.components_hidden;
        let music_active = music_active && compact_components_visible;
        let lyric_target_w = self.compute_lyric_target_width(window, music_active, is_paused, dt);
        let preserve_compact_widget_width = !self.is_width_hiding()
            || !crate::ui::widget::compact::hide_fade_complete(self.springs.hide.value);
        let compact_widget_target_w = if !self.expanded
            && !self.compact_overlay.is_visible()
            && preserve_compact_widget_width
        {
            let scale = self.config.compact_scale.max(f32::EPSILON);
            let has_mini_content = !self.components_hidden && self.activity_mgr.current().is_some();
            let center_content_width = has_mini_content.then_some(lyric_target_w / scale);
            crate::ui::widget::compact::target_width(
                if self.components_hidden {
                    &[]
                } else {
                    &self.config.compact_widget_layout
                },
                self.config.base_width,
                center_content_width,
            ) * scale
        } else {
            lyric_target_w
        };
        let plugin_widths = crate::ui::plugin::compact_widths(self.plugin_host.as_deref());
        let compact_widget_target_w = if compact_components_visible
            && preserve_compact_widget_width
            && !self.compact_overlay.is_visible()
        {
            compact_widget_target_w
                + (plugin_widths.0 + plugin_widths.1) * self.config.compact_scale
        } else {
            compact_widget_target_w
        };
        let compact_content_h = self.compact_content_height();
        let default_target_h = if self.expanded {
            self.config.expanded_height * self.config.expanded_scale
        } else {
            compact_content_h
        };
        let default_target_r = if self.expanded {
            crate::utils::shape::expanded_island_radius(
                self.config.expanded_width * self.config.expanded_scale,
                default_target_h,
                self.config.expanded_scale,
            )
        } else {
            compact_content_h / 2.0
        };
        let lingosoul_target = (!self.expanded)
            .then(|| self.lingosoul.target_size(&self.config, compact_content_h))
            .flatten();
        let (target_w, target_h, target_r) = if let Some(size) = lingosoul_target {
            size
        } else if let Some(size) = self.compact_overlay.target_size(
            self.config.base_width,
            self.config.base_height,
            self.config.compact_scale,
        ) {
            (size.width, size.height, size.height / 2.0)
        } else if self.expanded {
            (compact_widget_target_w, default_target_h, default_target_r)
        } else {
            self.lingosoul.stretch(
                &self.config,
                (compact_widget_target_w, default_target_h, default_target_r),
            )
        };
        let (stiffness, damping) = if let Some(motion) = self.lingosoul.panel_motion() {
            motion
        } else if lingosoul_target.is_some() || self.lingosoul.wants_glass() {
            (0.15, 0.72)
        } else {
            (0.10, 0.68)
        };
        let target_view = self.target_page_position();
        self.springs
            .retarget_expansion(self.expanded, target_w, target_h, target_r, target_view);
        let width_hiding = self.is_width_hiding();
        if self.width_hiding_last_frame && !width_hiding {
            self.restoring_hide_width = true;
        } else if width_hiding {
            self.restoring_hide_width = false;
        }
        self.width_hiding_last_frame = width_hiding;

        if self.restoring_hide_width {
            let progress = 1.0 - 0.78_f32.powf(dt);
            self.springs.w.value += (target_w - self.springs.w.value) * progress;
            self.springs.w.velocity = 0.0;
            if (target_w - self.springs.w.value).abs() <= 0.25 * self.config.compact_scale {
                self.springs.w.value = target_w;
                self.restoring_hide_width = false;
            }
        } else {
            self.springs.w.update_dt(target_w, stiffness, damping, dt);
            self.springs
                .w
                .settle(target_w, SETTLE_PIXELS, SETTLE_PIXEL_VELOCITY);
        }
        self.springs.h.update_dt(target_h, stiffness, damping, dt);
        self.springs
            .h
            .settle(target_h, SETTLE_PIXELS, SETTLE_PIXEL_VELOCITY);
        self.springs.r.update_dt(target_r, stiffness, damping, dt);
        self.springs
            .r
            .settle(target_r, SETTLE_PIXELS, SETTLE_PIXEL_VELOCITY);
        self.springs.multitask_x.update_dt(0.0, 0.15, 0.71, dt);
        self.springs.multitask_x.settle(
            0.0,
            0.02 * self.config.compact_scale,
            0.005 * self.config.compact_scale,
        );
        self.springs.view.update_dt(target_view, 0.12, 0.68, dt);
        let page_width = self.springs.w.value.max(1.0);
        self.springs.view.settle(
            target_view,
            SETTLE_PIXELS / page_width,
            SETTLE_PIXEL_VELOCITY / page_width,
        );
        if was_animating && !self.springs.any_animating() {
            window.request_redraw();
        }
    }
}
