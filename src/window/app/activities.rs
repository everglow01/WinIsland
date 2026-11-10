use std::time::Instant;

use winisland_core::context::{Activity, ActivityContent, ActivityProgress, Priority};

use crate::ui::expanded::pager::ExpandedPage;
use crate::ui::island::MiniContent;

use super::App;

const MUSIC_ACTIVITY: u64 = 1;

/// A secondary plugin activity that yields the compact island to other content.
#[derive(Clone, Copy)]
pub(super) struct SecondaryIndicator {
    pub page: Option<u64>,
    pub progress: ActivityProgress,
}

impl App {
    pub(super) fn resize_activity_window(&self) {
        if let Some(window) = self.window {
            let size = self.required_window_size();
            if window.inner_size() != size {
                let _ = window.request_inner_size(size);
            }
        }
    }

    pub(super) fn update_local_activities(&mut self, music_active: bool, playing: bool) -> bool {
        Self::sync_local_activity(
            &mut self.activity_mgr,
            MUSIC_ACTIVITY,
            music_active,
            if playing {
                Priority::High
            } else {
                Priority::Low
            },
            playing,
            0.0,
        )
    }

    /// Whether another activity, compact widget, or compact surface occupies the island.
    pub(super) fn compact_island_shared(&self, excluding: u64) -> bool {
        self.activity_mgr.current_excluding(excluding).is_some()
            || self
                .config
                .compact_widget_layout
                .iter()
                .any(|slot| slot.widget.is_some())
            || {
                let (left, right) = crate::ui::plugin::compact_widths(self.plugin_host.as_deref());
                left + right > 0.0
            }
    }

    fn sync_local_activity(
        manager: &mut winisland_core::context::ActivityManager,
        id: u64,
        enabled: bool,
        priority: Priority,
        keep_visible: bool,
        preferred_width: f32,
    ) -> bool {
        if !enabled {
            return manager.remove(id);
        }
        let updated_at = manager
            .get(id)
            .filter(|item| item.priority == priority)
            .map_or_else(Instant::now, |item| item.updated_at);
        manager.upsert(Activity {
            id,
            priority,
            content: ActivityContent::Local,
            show_compact: true,
            keep_visible,
            secondary: false,
            progress: None,
            preferred_width,
            preferred_height: 0.0,
            page: None,
            expires_at: None,
            updated_at,
        })
    }

    pub(super) fn mini_content(&self) -> Option<MiniContent<'_>> {
        Self::mini_content_for(&self.activity_mgr)
    }

    pub(super) fn mini_content_for(
        manager: &winisland_core::context::ActivityManager,
    ) -> Option<MiniContent<'_>> {
        let activity = manager.current()?;
        match &activity.content {
            ActivityContent::Local if activity.id == MUSIC_ACTIVITY => Some(MiniContent::Music),
            ActivityContent::Local => None,
            ActivityContent::Text(text) => Some(MiniContent::Text(text)),
            ActivityContent::Surface(id) => Some(MiniContent::Surface(*id)),
        }
    }

    pub(super) fn activity_page(&self) -> Option<ExpandedPage> {
        let activity = self.activity_mgr.current()?;
        let page = match activity.content {
            ActivityContent::Local if activity.id == MUSIC_ACTIVITY => ExpandedPage::Music,
            _ => ExpandedPage::Plugin(activity.page?),
        };
        self.expanded_pages().contains(&page).then_some(page)
    }
}
