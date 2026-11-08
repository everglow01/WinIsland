use std::time::{Duration, Instant};

use winisland_utils::poll::Cooldown;

const RECLAIM_INTERVAL: Duration = Duration::from_secs(5);
const IDLE_SETTLE: Duration = Duration::from_millis(500);
const ACTIVE_FRAME_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Default)]
pub(crate) struct MemoryActivity {
    pub animating: bool,
    pub interacting: bool,
    pub resource_updates: bool,
    pub playback: bool,
    pub dynamic_background: bool,
}

#[derive(Default)]
pub(crate) struct MemoryReclaimer {
    cooldown: Cooldown,
    activity: MemoryActivity,
    idle_since: Option<Instant>,
    last_frame: Option<Instant>,
    rendering_until: Option<Instant>,
    last_error: Option<String>,
}

impl MemoryReclaimer {
    pub(crate) fn set_activity(&mut self, activity: MemoryActivity) {
        self.activity = activity;
    }

    pub(crate) fn frame_presented(&mut self, now: Instant) {
        if self.last_frame.replace(now).is_some_and(|previous| {
            now.saturating_duration_since(previous) <= ACTIVE_FRAME_INTERVAL
        }) {
            self.rendering_until = Some(now + IDLE_SETTLE);
        }
    }

    pub(crate) fn update(&mut self, now: Instant, auxiliary_active: bool) {
        let idle = !auxiliary_active
            && !self.activity.animating
            && !self.activity.interacting
            && !self.activity.resource_updates
            && !self.activity.playback
            && !self.activity.dynamic_background
            && self.rendering_until.is_none_or(|until| now >= until);
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
