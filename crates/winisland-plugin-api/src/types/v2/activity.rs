use super::WidgetId;
pub use super::context::{PRIORITY_HIGH, PRIORITY_LOW, PRIORITY_MEDIUM};

pub const ACTIVITY_ENABLED: u32 = 1 << 0;
pub const ACTIVITY_KEEP_VISIBLE: u32 = 1 << 1;
/// Yields the compact island to any other activity or compact content. While it yields, the host
/// draws the plugin's indicator from [`ActivityStatusV2`] instead of showing this activity.
pub const ACTIVITY_SECONDARY: u32 = 1 << 2;
/// The published countdown is paused; the host draws the indicator dimmed.
pub const ACTIVITY_STATUS_PAUSED: u32 = 1 << 0;

#[repr(C)]
#[derive(Clone, Copy, PartialEq)]
pub struct ActivitySpecV2 {
    pub struct_size: u32,
    pub flags: u32,
    pub priority: u32,
    pub timeout_ms: u32,
    pub compact_surface: WidgetId,
    pub expanded_page: WidgetId,
    pub preferred_width: f32,
    pub preferred_height: f32,
}

impl Default for ActivitySpecV2 {
    fn default() -> Self {
        Self {
            struct_size: std::mem::size_of::<Self>() as u32,
            flags: ACTIVITY_ENABLED,
            priority: PRIORITY_MEDIUM,
            timeout_ms: 0,
            compact_surface: WidgetId::INVALID,
            expanded_page: WidgetId::INVALID,
            preferred_width: 0.0,
            preferred_height: 0.0,
        }
    }
}

/// The countdown a secondary activity publishes for the host-drawn compact indicator. The host
/// records the moment the status arrives and advances `remaining_ms` itself while it runs.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ActivityStatusV2 {
    pub struct_size: u32,
    pub flags: u32,
    pub remaining_ms: u64,
    pub total_ms: u64,
}

impl Default for ActivityStatusV2 {
    fn default() -> Self {
        Self {
            struct_size: std::mem::size_of::<Self>() as u32,
            flags: 0,
            remaining_ms: 0,
            total_ms: 0,
        }
    }
}
