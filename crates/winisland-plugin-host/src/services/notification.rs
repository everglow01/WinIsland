use std::ffi::c_void;

use winisland_plugin_api::abi::{CAP_NOTIFY, PluginStatus};
use winisland_plugin_api::types::v2::{AlertSpecV2, AlertStateV2, PluginToken, Utf8Slice};

use super::extensions::text;
use super::{read_struct, read_utf8, runtime};
use crate::extensions::HostRequest;
use crate::runtime::AlertRecord;

pub unsafe extern "C" fn toast(
    context: *mut c_void,
    token: PluginToken,
    title: Utf8Slice,
    body: Utf8Slice,
) -> PluginStatus {
    let result = (|| {
        // SAFETY: The ABI caller supplies a live instance context.
        let host = unsafe { runtime(context)? };
        host.registry.require(token, CAP_NOTIFY)?;
        // SAFETY: Both borrowed strings stay readable through this synchronous call.
        let (title, body) = unsafe { (read_utf8(title, 256)?, read_utf8(body, 1024)?) };
        if title.is_empty() {
            return Err(PluginStatus::InvalidArgument);
        }
        let mut state = host
            .extensions
            .state
            .lock()
            .map_err(|_| PluginStatus::Internal)?;
        state.request(token, HostRequest::Toast { title, body })?;
        drop(state);
        host.extensions.changed();
        Ok(())
    })();
    result.err().unwrap_or(PluginStatus::Ok)
}

pub unsafe extern "C" fn alert(
    context: *mut c_void,
    token: PluginToken,
    spec: *const AlertSpecV2,
) -> PluginStatus {
    let result = (|| {
        // SAFETY: The ABI caller supplies a live context and a readable sized descriptor.
        let (host, spec) = unsafe { (runtime(context)?, read_struct(spec)?) };
        host.registry.require(token, CAP_NOTIFY)?;
        if spec.flags != 0 {
            return Err(PluginStatus::InvalidArgument);
        }
        let title = text(&spec.title)?;
        if title.is_empty() {
            return Err(PluginStatus::InvalidArgument);
        }
        let body = text(&spec.body)?;
        // SAFETY: The borrowed icon path stays readable through this synchronous call.
        let icon = unsafe { read_utf8(spec.icon, 16 * 1024)? };
        let mut state = host.state.lock().map_err(|_| PluginStatus::Internal)?;
        state.alert = Some(AlertRecord {
            token,
            title: title.to_owned(),
            body: body.to_owned(),
            icon,
            dismissed: false,
        });
        state.alert_revision = state.alert_revision.wrapping_add(1);
        drop(state);
        host.extensions.changed();
        Ok(())
    })();
    result.err().unwrap_or(PluginStatus::Ok)
}

pub unsafe extern "C" fn dismiss_alert(context: *mut c_void, token: PluginToken) -> PluginStatus {
    let result = (|| {
        // SAFETY: The ABI caller supplies a live instance context.
        let host = unsafe { runtime(context)? };
        host.registry.require(token, CAP_NOTIFY)?;
        let mut state = host.state.lock().map_err(|_| PluginStatus::Internal)?;
        let dismissed = state.alert.as_mut().is_some_and(|alert| {
            if alert.token != token || alert.dismissed {
                return false;
            }
            alert.dismissed = true;
            true
        });
        if dismissed {
            state.alert_revision = state.alert_revision.wrapping_add(1);
            drop(state);
            host.extensions.changed();
        }
        Ok(())
    })();
    result.err().unwrap_or(PluginStatus::Ok)
}

pub unsafe extern "C" fn alert_state(
    context: *mut c_void,
    token: PluginToken,
    out: *mut AlertStateV2,
) -> PluginStatus {
    let result = (|| {
        if out.is_null() {
            return Err(PluginStatus::InvalidArgument);
        }
        // SAFETY: The ABI caller supplies a live instance context.
        let host = unsafe { runtime(context)? };
        host.registry.require(token, CAP_NOTIFY)?;
        // SAFETY: The ABI requires a writable, initialized sized output.
        unsafe {
            read_struct(out)?;
        }
        let state = host.state.lock().map_err(|_| PluginStatus::Internal)?;
        let visible = state
            .alert
            .as_ref()
            .is_some_and(|alert| alert.token == token && !alert.dismissed);
        drop(state);
        // SAFETY: The checked output remains exclusively writable.
        unsafe {
            (*out).visible = u8::from(visible);
        }
        Ok(())
    })();
    result.err().unwrap_or(PluginStatus::Ok)
}
