use std::time::{Duration, Instant};

use crate::core::memory::MemoryActivity;
use crate::platform::WindowRef;
use crate::ui::expanded::pager::ExpandedPage;

use super::super::App;
use super::HIDDEN_FRAME_INTERVAL;

const ACTIVE_FRAME_INTERVAL: Duration = Duration::from_millis(20);
const IDLE_FRAME_INTERVAL: Duration = Duration::from_millis(50);

impl App {
    pub(super) fn update_compact_widget_refresh(&mut self, window: &WindowRef, now: Instant) {
        if self.expanded
            || self.components_hidden
            || self.is_hidden()
            || self.compact_overlay.is_visible()
        {
            self.compact_widget_refresh.clear();
            return;
        }
        let Some(delay) =
            crate::ui::widget::compact::next_refresh_delay(&self.config.compact_widget_layout)
        else {
            self.compact_widget_refresh.clear();
            return;
        };
        if self.compact_widget_refresh.is_ready(now) {
            window.request_redraw();
            self.compact_widget_refresh.start(now, delay);
        }
    }

    fn periodic_effect_redraw_due(&mut self) -> bool {
        let is_dynamic = self.config.island_style == "dynamic";
        !self.is_hidden() && (is_dynamic || self.expanded) && self.effect_refresh.due_now()
    }

    pub(super) fn schedule_next_frame(
        &mut self,
        window: &WindowRef,
        now: Instant,
        pacing: FramePacing,
    ) {
        let should_periodic_redraw = self.periodic_effect_redraw_due();
        let transition_active = self.lingosoul.is_animating()
            || self.lingosoul.has_pending_requests()
            || self.springs.any_animating()
            || self.pager_hover_animating()
            || self.compact_overlay.is_alert_animating()
            || self.multitask.is_animating()
            || self.status_indicators.is_animating()
            || self
                .attention_pulse_started
                .is_some_and(|started| now.duration_since(started) < Duration::from_millis(1600))
            || self.compact_overlay.is_volume_dragging()
            || self.compact_overlay.is_brightness_dragging()
            || self.lyrics.transition < 1.0
            || self.volume_dragging
            || crate::ui::expanded::music_view::audio_controls_animating()
            || self.is_dragging
            || self.seek.active
            || self.is_right_dragging;
        let alert_visible = self.compact_overlay.is_alert_visible();
        let resource_usage_active = !alert_visible && self.resource_usage_animating();
        let compact_components_visible = self.expanded || !self.components_hidden;
        let playback_active = !alert_visible
            && !self.is_hidden()
            && compact_components_visible
            && pacing.media_is_playing
            && (!self.expanded || self.page_visible(ExpandedPage::Music));
        let dynamic_effect_active = !self.is_hidden()
            && compact_components_visible
            && self.config.island_style == "dynamic"
            && self.current_media_info().thumbnail.is_some();
        let interactive_active = (pacing.is_hovering_visible
            && (!self.expanded || self.page_visible(ExpandedPage::Music)))
            || (pacing.compact_overlay_visible && !alert_visible)
            || pacing.passive_reveal_active
            || self.right_press_cursor.is_some()
            || self
                .secondary_indicator
                .is_some_and(|indicator| !indicator.progress.paused);
        self.memory.set_activity(MemoryActivity {
            animating: transition_active,
            interacting: self.input_pressed()
                || self.right_press_cursor.is_some()
                || self.plugin_ui.pressed_buttons != 0
                || self.plugin_ui.touch_id.is_some(),
            resource_updates: resource_usage_active,
        });
        self.precise_frame_pacing = transition_active
            || playback_active
            || dynamic_effect_active
            || interactive_active
            || resource_usage_active;
        let frame_interval = if transition_active {
            self.display_frame_interval
        } else if self.precise_frame_pacing {
            self.aligned_frame_interval(ACTIVE_FRAME_INTERVAL)
        } else if self.is_hidden() {
            HIDDEN_FRAME_INTERVAL
        } else {
            IDLE_FRAME_INTERVAL
        };
        self.next_frame_deadline = now + frame_interval;
        if self.precise_frame_pacing || should_periodic_redraw {
            window.request_redraw();
        }
    }

    fn aligned_frame_interval(&self, desired: Duration) -> Duration {
        let refresh = self.display_frame_interval.as_nanos().max(1);
        let refreshes = ((desired.as_nanos() + refresh / 2) / refresh).max(1) as u32;
        self.display_frame_interval.saturating_mul(refreshes)
    }

    fn resource_usage_animating(&self) -> bool {
        use winisland_core::config::{CompactWidgetKind, WidgetKind};

        if self.is_hidden() || self.compact_overlay.is_visible() {
            return false;
        }
        let visible = if self.expanded {
            self.page_visible(ExpandedPage::Widgets)
                && self
                    .config
                    .widget_layout
                    .iter()
                    .any(|slot| slot.widget == Some(WidgetKind::ResourceUsage))
        } else {
            self.config
                .compact_widget_layout
                .iter()
                .any(|slot| slot.widget == Some(CompactWidgetKind::ResourceUsage))
        };
        visible
            && if self.expanded {
                crate::ui::widget::resource_usage::is_animating(&self.config.resource_metrics)
            } else {
                crate::ui::widget::resource_usage::is_animating(
                    &self.config.compact_resource_metrics,
                )
            }
    }
}

pub(super) struct FramePacing {
    pub(super) media_is_playing: bool,
    pub(super) is_hovering_visible: bool,
    pub(super) compact_overlay_visible: bool,
    pub(super) passive_reveal_active: bool,
}
