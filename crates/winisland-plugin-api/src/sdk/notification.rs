use crate::{AlertSpecV2, AlertStateV2, IFACE_NOTIFICATION, NotificationApiV2, Utf8Slice};

use super::{Error, Host, copy_fixed, success};

#[derive(Clone)]
pub struct NotificationApi(Host);

impl Host {
    pub fn notification(&self) -> Result<NotificationApi, Error> {
        self.query::<NotificationApiV2>(IFACE_NOTIFICATION)?;
        Ok(NotificationApi(self.clone()))
    }
}

impl NotificationApi {
    /// Shows a system toast even while the island is hidden.
    pub fn toast(&self, title: &str, body: &str) -> Result<(), Error> {
        let table = self.0.query::<NotificationApiV2>(IFACE_NOTIFICATION)?;
        let function = table
            .toast
            .ok_or(Error::MissingFunction("notification.toast"))?;
        // SAFETY: Both borrowed strings remain readable until this call returns.
        success(unsafe {
            function(
                table.prefix.context,
                self.0.token,
                Utf8Slice::borrowed(title),
                Utf8Slice::borrowed(body),
            )
        })
    }

    /// Raises a compact alert card. `icon` is SVG path data; an empty string omits the icon.
    pub fn alert(&self, title: &str, body: &str, icon: &str) -> Result<(), Error> {
        let table = self.0.query::<NotificationApiV2>(IFACE_NOTIFICATION)?;
        let function = table
            .alert
            .ok_or(Error::MissingFunction("notification.alert"))?;
        let mut spec = AlertSpecV2::default();
        copy_fixed(&mut spec.title, title);
        copy_fixed(&mut spec.body, body);
        spec.icon = Utf8Slice::borrowed(icon);
        // SAFETY: The specification and borrowed icon stay readable until this call returns.
        success(unsafe { function(table.prefix.context, self.0.token, &spec) })
    }

    pub fn dismiss_alert(&self) -> Result<(), Error> {
        let table = self.0.query::<NotificationApiV2>(IFACE_NOTIFICATION)?;
        let function = table
            .dismiss_alert
            .ok_or(Error::MissingFunction("notification.dismiss_alert"))?;
        // SAFETY: The host validates that the caller owns the alert.
        success(unsafe { function(table.prefix.context, self.0.token) })
    }

    /// Reports whether the caller's alert is still on screen.
    pub fn alert_visible(&self) -> Result<bool, Error> {
        let table = self.0.query::<NotificationApiV2>(IFACE_NOTIFICATION)?;
        let function = table
            .alert_state
            .ok_or(Error::MissingFunction("notification.alert_state"))?;
        let mut state = AlertStateV2::default();
        // SAFETY: The output is a complete writable state value during this call.
        success(unsafe { function(table.prefix.context, self.0.token, &mut state) })?;
        Ok(state.visible != 0)
    }
}
