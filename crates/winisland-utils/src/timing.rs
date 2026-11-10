use std::time::{Duration, Instant};

/// Returns the elapsed time and advances the timestamp to the same clock sample.
pub fn take_elapsed(last: &mut Instant, now: Instant) -> Duration {
    now.saturating_duration_since(std::mem::replace(last, now))
}
