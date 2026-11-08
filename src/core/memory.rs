use std::time::{Duration, Instant};

use winisland_utils::poll::Cooldown;

const RECLAIM_INTERVAL: Duration = Duration::from_secs(5);
const IDLE_SETTLE: Duration = Duration::from_millis(500);
const MAX_RECLAIM_DEFERRAL: Duration = Duration::from_secs(1);

#[derive(Default)]
pub(crate) struct MemoryActivity {
    pub animating: bool,
    pub interacting: bool,
    pub resource_updates: bool,
}

#[derive(Default)]
pub(crate) struct MemoryReclaimer {
    cooldown: Cooldown,
    activity: MemoryActivity,
    idle_since: Option<Instant>,
    reclaim_pending_since: Option<Instant>,
    last_error: Option<String>,
}

impl MemoryReclaimer {
    pub(crate) fn set_activity(&mut self, activity: MemoryActivity) {
        self.activity = activity;
    }

    pub(crate) fn update(&mut self, now: Instant, auxiliary_active: bool) {
        let idle = !auxiliary_active
            && !self.activity.animating
            && !self.activity.interacting
            && !self.activity.resource_updates;
        if idle {
            self.idle_since.get_or_insert(now);
        } else {
            self.idle_since = None;
        }
        if !self.cooldown.is_ready(now) {
            return;
        }
        let pending_since = *self.reclaim_pending_since.get_or_insert(now);
        let settled = self
            .idle_since
            .is_some_and(|since| now.saturating_duration_since(since) >= IDLE_SETTLE);
        if settled || now.saturating_duration_since(pending_since) >= MAX_RECLAIM_DEFERRAL {
            self.reclaim(now);
            self.reclaim_pending_since = None;
        }
    }

    fn reclaim(&mut self, now: Instant) {
        match crate::platform::metrics().trim_working_set() {
            Ok(()) => {
                if self.last_error.take().is_some() {
                    log::info!("Working set reclamation recovered");
                }
                log::debug!("Process working set trimmed");
            }
            Err(error) => {
                let message = error.to_string();
                if self.last_error.as_ref() != Some(&message) {
                    log::warn!("Working set trim failed: {message}");
                    self.last_error = Some(message);
                }
            }
        }
        self.cooldown.start(now, RECLAIM_INTERVAL);
    }
}
