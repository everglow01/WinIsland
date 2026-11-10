use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Low = 0,
    Medium = 1,
    High = 2,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityText {
    pub title: String,
    pub body: String,
    pub compact_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivityContent {
    Local,
    Text(ActivityText),
    Surface(u64),
}

/// A countdown a plugin publishes for the host-drawn compact indicator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActivityProgress {
    pub remaining_ms: u64,
    pub total_ms: u64,
    pub paused: bool,
    pub sampled_at: Instant,
}

impl ActivityProgress {
    /// The remaining share of the countdown, advancing in real time while it runs.
    pub fn fraction(&self, now: Instant) -> f32 {
        if self.total_ms == 0 {
            return 0.0;
        }
        let elapsed = if self.paused {
            0
        } else {
            now.saturating_duration_since(self.sampled_at).as_millis() as u64
        };
        let remaining = self.remaining_ms.saturating_sub(elapsed);
        (remaining as f32 / self.total_ms as f32).clamp(0.0, 1.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Activity {
    pub id: u64,
    pub priority: Priority,
    pub content: ActivityContent,
    pub show_compact: bool,
    pub keep_visible: bool,
    /// Yields the compact island to any other activity or compact content.
    pub secondary: bool,
    /// Countdown shown by the host's compact indicator while this activity is not the island content.
    pub progress: Option<ActivityProgress>,
    pub preferred_width: f32,
    pub preferred_height: f32,
    pub page: Option<u64>,
    pub expires_at: Option<Instant>,
    pub updated_at: Instant,
}

impl Activity {
    pub fn visible_at(&self, now: Instant) -> bool {
        self.show_compact && self.expires_at.is_none_or(|deadline| deadline > now)
    }
}
