use std::ffi::c_void;
use std::time::Instant;

use winisland_plugin_api::abi::{CAP_ACTIVITY, PluginStatus};
use winisland_plugin_api::types::v2::{
    ACTIVITY_STATUS_PAUSED, ActivityStatusV2, PluginToken, ResourceId,
};

use super::{read_struct, runtime};
use crate::resources::ResourceKind;
use crate::runtime::ActivityStatusRecord;

fn status(value: &ActivityStatusV2) -> Result<ActivityStatusRecord, PluginStatus> {
    if value.flags & !ACTIVITY_STATUS_PAUSED != 0
        || value.total_ms == 0
        || value.remaining_ms > value.total_ms
    {
        return Err(PluginStatus::InvalidArgument);
    }
    Ok(ActivityStatusRecord {
        remaining_ms: value.remaining_ms,
        total_ms: value.total_ms,
        paused: value.flags & ACTIVITY_STATUS_PAUSED != 0,
        sampled_at: Instant::now(),
    })
}

pub unsafe extern "C" fn set(
    context: *mut c_void,
    token: PluginToken,
    activity: ResourceId,
    value: *const ActivityStatusV2,
) -> PluginStatus {
    let result = (|| {
        // SAFETY: The ABI caller supplies a live context and a readable sized value.
        let (host, value) = unsafe { (runtime(context)?, read_struct(value)?) };
        host.registry.require(token, CAP_ACTIVITY)?;
        let status = status(&value)?;
        host.resources
            .require(token, ResourceKind::Activity, activity.get())?;
        let mut state = host.state.lock().map_err(|_| PluginStatus::Internal)?;
        let record = state
            .activities
            .get_mut(&activity.get())
            .ok_or(PluginStatus::StaleHandle)?;
        record.status = Some(status);
        state.activity_revision = state.activity_revision.wrapping_add(1);
        drop(state);
        host.extensions.changed();
        Ok(())
    })();
    result.err().unwrap_or(PluginStatus::Ok)
}

pub unsafe extern "C" fn clear(
    context: *mut c_void,
    token: PluginToken,
    activity: ResourceId,
) -> PluginStatus {
    let result = (|| {
        // SAFETY: The ABI caller supplies a live instance context.
        let host = unsafe { runtime(context)? };
        host.registry.require(token, CAP_ACTIVITY)?;
        host.resources
            .require(token, ResourceKind::Activity, activity.get())?;
        let mut state = host.state.lock().map_err(|_| PluginStatus::Internal)?;
        let record = state
            .activities
            .get_mut(&activity.get())
            .ok_or(PluginStatus::StaleHandle)?;
        if record.status.take().is_some() {
            state.activity_revision = state.activity_revision.wrapping_add(1);
            drop(state);
            host.extensions.changed();
        }
        Ok(())
    })();
    result.err().unwrap_or(PluginStatus::Ok)
}
