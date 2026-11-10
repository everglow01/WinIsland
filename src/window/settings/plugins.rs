use std::rc::Rc;

use crate::plugin::inventory::InstalledPlugin;
use crate::utils::settings_ui::WidgetSource;
use winisland_core::plugin_settings::PluginSettingsPage;
use winisland_core::widgets::PluginWidget;
use winisland_plugin_host::host::PluginHost;
use winisland_plugin_package::marketplace::MarketplaceCatalog;
use winisland_utils::poll::Job;

use super::{
    BUILTIN_SIDEBAR_PAGE_COUNT, MarketplaceViewState, PLUGIN_DETAIL_KEY, PLUGINS_PAGE_INDEX,
    POPUP_CLOSE_SPEED, POPUP_OPACITY_KEY, PluginSettingsRequest, SettingsApp, pages, sidebar,
};

impl SettingsApp {
    pub(crate) fn take_plugin_request(&mut self) -> Option<PluginSettingsRequest> {
        self.plugin_request.take()
    }

    pub(crate) fn set_plugins(&mut self, plugins: Vec<InstalledPlugin>) {
        pages::plugins::clear_plugin_icon_cache();
        self.plugins = plugins;
        if self
            .selected_plugin_id
            .as_ref()
            .is_some_and(|id| !self.plugins.iter().any(|plugin| &plugin.id == id))
        {
            self.selected_plugin_id = None;
            self.pending_plugin_uninstall_id = None;
            self.plugin_detail_closing = true;
            self.anim.set_with_speed(PLUGIN_DETAIL_KEY, 0.0, 0.28);
        }
        self.mark_items_dirty();
        self.request_redraw();
    }

    pub(crate) fn set_plugin_inventory_scan(&mut self, scan: Job<Vec<InstalledPlugin>>) {
        self.plugin_inventory_scan = scan;
    }

    pub(crate) fn set_plugin_widgets(&mut self, plugin_widgets: Vec<PluginWidget>) {
        self.plugin_widgets = plugin_widgets;
        let layout_changed = winisland_core::config::normalize_active_plugin_widget_layout(
            &self.config.widget_layout,
            &mut self.config.plugin_widget_layout,
            &self.plugin_widgets,
        );
        if self.widget_dragging.as_ref().is_some_and(|source| {
            matches!(source, WidgetSource::Plugin(id) if !self.plugin_widgets.iter().any(|widget| widget.layout_id().as_ref() == Some(id)))
        }) {
            self.widget_dragging = None;
            self.widget_drag_hover_slot = None;
        }
        self.mark_items_dirty();
        if layout_changed {
            crate::core::persistence::save_config(&self.config);
        }
        self.request_redraw();
    }

    pub(crate) fn set_plugin_host(&mut self, host: Option<Rc<PluginHost>>) {
        self.plugin_host = host;
    }

    pub(crate) fn set_plugin_settings_pages(&mut self, pages: Vec<PluginSettingsPage>) {
        let previous_active_page = self.active_page;
        let icons_changed = self.plugin_settings_pages.len() != pages.len()
            || self.plugin_settings_pages.iter().any(|current| {
                pages
                    .iter()
                    .find(|page| page.resource_id == current.resource_id)
                    .is_none_or(|page| page.icon != current.icon)
            });
        let structure_changed = self
            .plugin_settings_pages
            .iter()
            .map(|page| page.resource_id)
            .ne(pages.iter().map(|page| page.resource_id));
        let active_resource = self
            .active_plugin_settings_page()
            .map(|page| page.resource_id);
        self.plugin_settings_pages = pages;
        if let Some(resource_id) = active_resource {
            self.active_page = self
                .plugin_settings_pages
                .iter()
                .position(|page| page.resource_id == resource_id)
                .map_or(PLUGINS_PAGE_INDEX, |index| {
                    BUILTIN_SIDEBAR_PAGE_COUNT + index
                });
        } else if self.active_page >= self.sidebar_page_count() {
            self.active_page = PLUGINS_PAGE_INDEX;
        }
        if structure_changed {
            self.page_history.clear();
            self.page_history.push(self.active_page);
            self.page_history_index = 0;
        }
        if self.active_page != previous_active_page {
            self.scroll_y = 0.0;
            self.target_scroll_y = 0.0;
        }
        let pending_is_current = self.pending_plugin_setting.as_ref().is_some_and(|pending| {
            self.plugin_settings_pages
                .iter()
                .any(|page| page.resource_id == pending.resource_id)
        });
        if !pending_is_current {
            if self.pending_plugin_setting.is_some() {
                self.popup = None;
                self.number_input = None;
                self.anim
                    .set_with_speed(POPUP_OPACITY_KEY, 0.0, POPUP_CLOSE_SPEED);
            }
            self.pending_plugin_setting = None;
        }
        let error_is_current =
            self.plugin_settings_error
                .as_ref()
                .is_some_and(|(resource_id, _)| {
                    self.plugin_settings_pages
                        .iter()
                        .any(|page| page.resource_id == *resource_id)
                });
        if !error_is_current {
            self.plugin_settings_error = None;
        }
        if icons_changed {
            sidebar::clear_plugin_settings_icon_cache();
        }
        self.mark_items_dirty();
        self.request_redraw();
    }

    pub(crate) fn set_plugin_status(&mut self, message: String, restart: bool) {
        self.plugin_status = Some((message, restart));
        self.mark_items_dirty();
        self.request_redraw();
    }

    pub(crate) fn set_marketplace_loading(&mut self) {
        if !matches!(self.marketplace_state, MarketplaceViewState::Loaded(_)) {
            self.marketplace_state = MarketplaceViewState::Loading;
        }
        self.mark_items_dirty();
        self.request_redraw();
    }

    pub(crate) fn set_marketplace_catalog(&mut self, catalog: MarketplaceCatalog) {
        pages::plugins::clear_plugin_icon_cache();
        self.marketplace_state = MarketplaceViewState::Loaded(catalog.plugins);
        self.mark_items_dirty();
        self.request_redraw();
    }

    pub(crate) fn set_marketplace_error(&mut self, error: String) {
        self.marketplace_state = MarketplaceViewState::Failed(error);
        self.mark_items_dirty();
        self.request_redraw();
    }

    pub(crate) fn finish_marketplace_install(&mut self) {
        self.marketplace_installing_id = None;
        self.mark_items_dirty();
        self.request_redraw();
    }
}
