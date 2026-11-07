use crate::{ACTIVITY_STATUS_PAUSED, ActivityStatusV2, IFACE_INDICATOR, IndicatorApiV2};

use super::{Activity, Error, Host, success};

#[derive(Clone)]
pub struct IndicatorApi(Host);

impl Host {
    pub fn indicator(&self) -> Result<IndicatorApi, Error> {
        self.query::<IndicatorApiV2>(IFACE_INDICATOR)?;
        Ok(IndicatorApi(self.clone()))
    }
}

impl IndicatorApi {
    /// Publishes the countdown the host draws in its compact activity indicator.
    pub fn set(
        &self,
        activity: &Activity,
        remaining_ms: u64,
        total_ms: u64,
        paused: bool,
    ) -> Result<(), Error> {
        let table = self.0.query::<IndicatorApiV2>(IFACE_INDICATOR)?;
        let function = table.set.ok_or(Error::MissingFunction("indicator.set"))?;
        let status = ActivityStatusV2 {
            flags: if paused { ACTIVITY_STATUS_PAUSED } else { 0 },
            remaining_ms,
            total_ms,
            ..Default::default()
        };
        // SAFETY: The status and the owned activity id remain valid for this synchronous call.
        success(unsafe { function(table.prefix.context, self.0.token, activity.id(), &status) })
    }

    /// Stops drawing the indicator for that activity.
    pub fn clear(&self, activity: &Activity) -> Result<(), Error> {
        let table = self.0.query::<IndicatorApiV2>(IFACE_INDICATOR)?;
        let function = table
            .clear
            .ok_or(Error::MissingFunction("indicator.clear"))?;
        // SAFETY: The host validates that the activity belongs to the caller.
        success(unsafe { function(table.prefix.context, self.0.token, activity.id()) })
    }
}
