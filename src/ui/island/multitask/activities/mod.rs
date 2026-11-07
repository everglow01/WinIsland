mod device;

pub(crate) use device::StatusIndicators;

use winisland_render::{Painter, Rect};

/// The countdown a plugin activity publishes for the host-drawn compact indicator.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Indicator {
    pub progress: f32,
    pub paused: bool,
    pub page: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Activity {
    Status {
        camera: f32,
        microphone: f32,
        indicator: f32,
        plugin: f32,
        shared: f32,
        indicator_progress: f32,
        indicator_paused: bool,
        indicator_page: Option<u64>,
        plugin_surface: Option<u64>,
    },
}

impl Activity {
    pub(crate) fn width(self, height: f32, _scale: f32) -> f32 {
        match self {
            Self::Status { shared, .. } => device::width(height, shared),
        }
    }

    pub(crate) fn has_indicator(self) -> bool {
        match self {
            Self::Status { indicator, .. } => indicator > 0.5,
        }
    }

    pub(crate) fn indicator_page(self) -> Option<u64> {
        match self {
            Self::Status { indicator_page, .. } => indicator_page,
        }
    }

    /// The plugin status surface occupying the leading circle, with its visibility.
    pub(crate) fn plugin_surface(self) -> Option<(u64, f32)> {
        match self {
            Self::Status {
                plugin,
                plugin_surface,
                ..
            } => plugin_surface
                .filter(|_| plugin > 0.0)
                .map(|id| (id, plugin)),
        }
    }

    pub(crate) fn draw(self, painter: Painter<'_>, rect: Rect, scale: f32, opacity: f32) {
        match self {
            Self::Status {
                camera,
                microphone,
                indicator,
                indicator_progress,
                indicator_paused,
                ..
            } => device::draw(
                painter,
                rect,
                scale,
                opacity,
                device::Indicators {
                    camera,
                    microphone,
                    indicator,
                    indicator_progress,
                    indicator_paused,
                },
            ),
        }
    }
}
