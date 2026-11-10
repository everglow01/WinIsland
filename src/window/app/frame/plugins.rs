use super::super::App;

impl App {
    pub(super) fn poll_pending_plugin_install(&mut self) {
        match self
            .pending_install
            .poll_result(|| "Installation thread crashed".to_string())
        {
            Some(Ok((manifest, staging))) => {
                let activation = self
                    .plugin_host
                    .as_ref()
                    .ok_or_else(|| "ABI v2 plugin host is unavailable".to_string())
                    .and_then(|host| {
                        host.activate_staged_plugin(&manifest, &staging)
                            .map_err(|error| error.to_string())
                    });
                if let Err(error) = activation {
                    let _ = std::fs::remove_dir_all(staging);
                    Self::show_toast("Plugin Error", &error);
                    log::error!("Failed to activate installed plugin: {error}");
                    self.set_plugin_install_error(winisland_core::i18n::tr_args(
                        "plugin_install_failed",
                        &[&error],
                    ));
                    return;
                }
                Self::show_toast(
                    "Plugin Installed",
                    &format!("{} loaded successfully!", manifest.name),
                );
                if let Some(settings) = self.settings.as_mut() {
                    settings.finish_marketplace_install();
                    settings.set_plugin_inventory_scan(self.plugin_mgr.installed_plugins_async());
                    settings.set_plugin_status(
                        winisland_core::i18n::tr("plugin_install_success"),
                        false,
                    );
                }
                log::info!("Plugin '{}' installed via drop", manifest.name);
            }
            Some(Err(e)) => {
                Self::show_toast("Plugin Error", &e);
                self.set_plugin_install_error(winisland_core::i18n::tr_args(
                    "plugin_install_failed",
                    &[&e],
                ));
                log::error!("Failed to install plugin from drop: {e}");
            }
            None => {}
        }
    }

    pub(super) fn poll_pending_plugin_marketplace(&mut self) {
        match self
            .pending_marketplace_catalog
            .poll_result(|| "The marketplace task stopped unexpectedly".to_string())
        {
            Some(Ok(catalog)) => {
                self.marketplace_catalog = Some(catalog.clone());
                if let Some(settings) = self.settings.as_mut() {
                    settings.set_marketplace_catalog(catalog);
                }
            }
            Some(Err(error)) => {
                log::error!("Failed to load plugin marketplace: {error}");
                if self.marketplace_catalog.is_none()
                    && let Some(settings) = self.settings.as_mut()
                {
                    settings.set_marketplace_error(error);
                }
            }
            None => {}
        }

        match self
            .pending_marketplace_download
            .poll_result(|| "the marketplace download task stopped unexpectedly".to_string())
        {
            Some(Ok(path)) => {
                if let Some(settings) = self.settings.as_mut() {
                    settings
                        .set_plugin_status(winisland_core::i18n::tr("plugin_installing"), false);
                }
                self.install_zip_drop(&path);
            }
            Some(Err(error)) => {
                log::error!("Failed to download marketplace plugin: {error}");
                self.set_plugin_install_error(winisland_core::i18n::tr_args(
                    "plugin_install_failed",
                    &[&error],
                ));
            }
            None => {}
        }
    }

    fn set_plugin_install_error(&mut self, message: String) {
        if let Some(settings) = self.settings.as_mut() {
            settings.finish_marketplace_install();
            settings.set_plugin_status(message, false);
        }
    }
}
