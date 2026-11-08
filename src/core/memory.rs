use std::time::{Duration, Instant};

use pollkit::Cooldown;

const RECLAIM_INTERVAL: Duration = Duration::from_secs(5);
const IDLE_SETTLE: Duration = Duration::from_millis(500);

#[derive(Default)]
pub(crate) struct MemoryActivity {
    pub animating: bool,
    pub interacting: bool,
    pub resource_updates: bool,
    pub expanded: bool,
    pub playback: bool,
    pub dynamic_background: bool,
}

#[derive(Default)]
pub(crate) struct MemoryReclaimer {
    cooldown: Cooldown,
    activity: MemoryActivity,
    idle_since: Option<Instant>,
}

impl MemoryReclaimer {
    pub(crate) fn set_activity(&mut self, activity: MemoryActivity) {
        self.activity = activity;
    }

    pub(crate) fn update(&mut self, now: Instant, auxiliary_active: bool) {
        let idle = !auxiliary_active
            && !self.activity.animating
            && !self.activity.interacting
            && !self.activity.resource_updates
            && (!self.activity.expanded
                || (!self.activity.playback && !self.activity.dynamic_background));
        if !idle {
            self.idle_since = None;
            return;
        }
        let idle_since = *self.idle_since.get_or_insert(now);
        if now.saturating_duration_since(idle_since) >= IDLE_SETTLE && self.cooldown.is_ready(now) {
            self.reclaim(now);
        }
    }

    fn reclaim(&mut self, now: Instant) {
        match crate::platform::metrics().trim_working_set() {
            Ok(()) => log::info!("Process working set trimmed"),
            Err(error) => log::warn!("Working set trim failed: {error}"),
        }
        self.cooldown.start(now, RECLAIM_INTERVAL);
    }
}
