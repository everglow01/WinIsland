use std::time::{Duration, Instant};

use winisland_core::context::{
    Activity, ActivityContent, ActivityProgress, ActivityText, Priority,
};
use winisland_plugin_api::types::v2::context::CONTEXT_FLAG_SHOW_COMPACT;
use winisland_plugin_api::{
    ACTIVITY_ENABLED, ACTIVITY_KEEP_VISIBLE, ACTIVITY_SECONDARY, SURFACE_COMPACT_MAIN,
    SURFACE_ENABLED, SURFACE_PAGE,
};

use super::{PluginHost, fixed_text};

impl PluginHost {
    pub fn activities_snapshot(&self, after_revision: u64) -> Option<(u64, Vec<Activity>)> {
        let state = self.runtime.state.lock().ok()?;
        if state.activity_revision == after_revision {
            return None;
        }
        let mut activities: Vec<_> = state
            .contexts
            .iter()
            .map(|(id, record)| {
                let data = &record.data;
                Activity {
                    id: *id,
                    priority: match data.priority {
                        0 => Priority::Low,
                        2 => Priority::High,
                        _ => Priority::Medium,
                    },
                    content: ActivityContent::Text(ActivityText {
                        title: fixed_text(&data.title),
                        body: fixed_text(&data.body),
                        compact_text: fixed_text(&data.compact_text),
                    }),
                    keep_visible: false,
                    secondary: false,
                    progress: None,
                    preferred_width: 0.0,
                    preferred_height: 0.0,
                    page: None,
                    show_compact: data.flags & CONTEXT_FLAG_SHOW_COMPACT != 0,
                    expires_at: (data.timeout_ms != 0)
                        .then(|| record.updated_at + Duration::from_millis(data.timeout_ms as u64)),
                    updated_at: record.updated_at,
                }
            })
            .collect();
        let surface = |id: u64, kind| {
            state
                .widgets
                .get(&id)
                .filter(|record| !record.disabled)
                .and_then(|record| record.surface)
                .filter(|spec| spec.kind == kind && spec.flags & SURFACE_ENABLED != 0)
        };
        for (id, record) in &state.activities {
            let spec = record.spec;
            let Some(compact) = surface(spec.compact_surface.get(), SURFACE_COMPACT_MAIN) else {
                continue;
            };
            activities.push(Activity {
                id: *id,
                priority: match spec.priority {
                    0 => Priority::Low,
                    2 => Priority::High,
                    _ => Priority::Medium,
                },
                content: ActivityContent::Surface(spec.compact_surface.get()),
                show_compact: spec.flags & ACTIVITY_ENABLED != 0,
                keep_visible: spec.flags & ACTIVITY_KEEP_VISIBLE != 0,
                secondary: spec.flags & ACTIVITY_SECONDARY != 0,
                progress: record.status.map(|status| ActivityProgress {
                    remaining_ms: status.remaining_ms,
                    total_ms: status.total_ms,
                    paused: status.paused,
                    sampled_at: status.sampled_at,
                }),
                preferred_width: if spec.preferred_width > 0.0 {
                    spec.preferred_width
                } else {
                    compact.width
                },
                preferred_height: if spec.preferred_height > 0.0 {
                    spec.preferred_height
                } else {
                    compact.height
                },
                page: surface(spec.expanded_page.get(), SURFACE_PAGE)
                    .map(|_| spec.expanded_page.get()),
                expires_at: (spec.timeout_ms != 0)
                    .then(|| record.updated_at + Duration::from_millis(spec.timeout_ms as u64)),
                updated_at: record.updated_at,
            });
        }
        let now = Instant::now();
        activities.retain(|item| item.expires_at.is_none_or(|deadline| deadline > now));
        Some((state.activity_revision, activities))
    }
}
