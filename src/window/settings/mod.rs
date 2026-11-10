use crate::platform::{MonitorRef, WindowRef};
use crate::plugin::inventory::InstalledPlugin;
use crate::utils::settings_ui::items::{POPUP_MENU_R, SettingsItem};
use crate::utils::settings_ui::{
    SwitchAnimator, WidgetDropAnimation, WidgetEditorHover, WidgetEditorMode, WidgetEditorSlot,
    WidgetSource,
};
use std::rc::Rc;
use std::time::Instant;
use winisland_core::anim::AnimPool;
use winisland_core::config::{AppConfig, AppConfigField};
use winisland_core::plugin_settings::PluginSettingsPage;
use winisland_core::widgets::PluginWidget;
use winisland_platform::{WindowPoint, WindowSize};
use winisland_plugin_host::host::PluginHost;
use winisland_plugin_package::marketplace::MarketplacePlugin;
use winisland_render::RendererTargetId;
use winisland_utils::Educe;
use winisland_utils::poll::Job;

mod events;
pub mod input;
pub mod items;
mod lifecycle;
mod page_order;
pub mod pages;
mod plugins;
mod popup;
mod refresh;
pub mod renderer;
mod resource_editor;
mod scrollbar;
mod search;
pub mod sidebar;

pub(crate) use popup::PopupState;

pub(crate) const WIN_W: f32 = 780.0;
pub(crate) const WIN_H: f32 = 680.0;
pub(crate) const SIDEBAR_W: f32 = 216.0;
pub(crate) const SIDEBAR_ROW_H: f32 = 30.0;
pub(crate) const SIDEBAR_ROW_GAP: f32 = 2.0;
pub(crate) const SIDEBAR_START_Y: f32 = 84.0;
pub(crate) const BUILTIN_SIDEBAR_PAGE_COUNT: usize = BuiltinPage::ALL.len();
pub(crate) const GENERAL_PAGE_INDEX: usize = BuiltinPage::General as usize;
pub(crate) const MUSIC_PAGE_INDEX: usize = BuiltinPage::Music as usize;
pub(crate) const WIDGETS_PAGE_INDEX: usize = BuiltinPage::Widgets as usize;
pub(crate) const PLUGINS_PAGE_INDEX: usize = BuiltinPage::Plugins as usize;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum BuiltinPage {
    General,
    Appearance,
    Layout,
    Behavior,
    Alerts,
    Music,
    Widgets,
    Plugins,
    #[cfg(feature = "lingosoul")]
    LingoSoul,
    About,
}

impl BuiltinPage {
    pub(crate) const ALL: &'static [Self] = &[
        Self::General,
        Self::Appearance,
        Self::Layout,
        Self::Behavior,
        Self::Alerts,
        Self::Music,
        Self::Widgets,
        Self::Plugins,
        #[cfg(feature = "lingosoul")]
        Self::LingoSoul,
        Self::About,
    ];

    pub(crate) fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }

    pub(crate) fn title_key(self) -> &'static str {
        match self {
            Self::General => "tab_general",
            Self::Appearance => "tab_appearance",
            Self::Layout => "tab_layout",
            Self::Behavior => "tab_behavior",
            Self::Alerts => "tab_alerts",
            Self::Music => "tab_music",
            Self::Widgets => "tab_widgets",
            Self::Plugins => "tab_plugins",
            #[cfg(feature = "lingosoul")]
            Self::LingoSoul => "tab_lingosoul",
            Self::About => "tab_about",
        }
    }
}
pub(crate) const PAGE_NAV_X: f32 = SIDEBAR_W + 8.0;
pub(crate) const PAGE_NAV_Y: f32 = 11.0;
pub(crate) const PAGE_NAV_WIDTH: f32 = 32.0;
pub(crate) const PAGE_NAV_HEIGHT: f32 = 32.0;
pub(crate) const PAGE_NAV_GAP: f32 = 0.0;
pub(crate) const SETTINGS_HEADER_H: f32 = 54.0;
const POPUP_CLOSE_SPEED: f32 = 0.3;

pub(crate) use crate::utils::settings_ui::chrome::{
    traffic_light_at as window_control_at, traffic_lights_hovered as window_controls_hovered,
};

#[derive(Clone, Copy)]
pub(crate) enum PageNavigation {
    Back,
    Forward,
}

pub(crate) const POPUP_OPACITY_KEY: u64 = 1;
pub(crate) const SIDEBAR_KEY_BASE: u64 = 1_000;
pub(crate) const PLUGIN_DETAIL_KEY: u64 = 2_000;

pub(crate) type NumberInputHandler = fn(&mut SettingsApp, &str);

pub(crate) struct PendingPluginSetting {
    resource_id: u64,
    key: String,
    minimum: Option<f64>,
    maximum: Option<f64>,
}

pub(crate) struct NumberInput {
    pub(crate) rect: winisland_render::Rect,
    pub(crate) text: String,
    pub(crate) on_commit: NumberInputHandler,
    /// Accepts any text, including IME input and paste, instead of numbers only.
    pub(crate) free_text: bool,
}

pub(crate) enum PluginSettingsRequest {
    HideIsland,
    Exit,
    Install(std::path::PathBuf),
    LoadMarketplace,
    InstallMarketplace(Box<MarketplacePlugin>),
    SetEnabled {
        id: String,
        enabled: bool,
    },
    Uninstall {
        id: String,
    },
    Restart,
    ShowGuide,
    #[cfg(feature = "lingosoul")]
    ClearLingoSoulHistory,
    #[cfg(feature = "lingosoul")]
    ReloadLingoSoul,
    #[cfg(feature = "lingosoul")]
    DownloadLingoSoulModels,
    #[cfg(feature = "lingosoul")]
    CancelLingoSoulDownload,
    #[cfg(feature = "lingosoul")]
    OpenLingoSoulChat,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PluginPageTab {
    Installed,
    Marketplace,
}

pub(crate) enum MarketplaceViewState {
    NotLoaded,
    Loading,
    Loaded(Vec<MarketplacePlugin>),
    Failed(String),
}

#[derive(Educe)]
#[educe(Default)]
pub struct SettingsApp {
    pub(crate) window: Option<WindowRef>,
    pub(crate) renderer_target: Option<RendererTargetId>,
    pub(crate) window_backdrop: bool,
    pub(crate) config: AppConfig,
    #[educe(Default(expression = GENERAL_PAGE_INDEX))]
    pub(crate) active_page: usize,
    #[educe(Default(expression = vec![0]))]
    pub(crate) page_history: Vec<usize>,
    pub(crate) page_history_index: usize,
    pub(crate) switch_anim: SwitchAnimator,
    #[educe(Default(expression = (usize::MAX, usize::MAX)))]
    pub(crate) switch_anim_context: (usize, usize),
    pub(crate) anim: AnimPool,
    pub(crate) logical_mouse_pos: (f32, f32),
    #[educe(Default(expression = (-1.0, -1.0)))]
    pub(crate) last_hover_mouse_pos: (f32, f32),
    touch_id: Option<u64>,
    touch_start: WindowPoint,
    touch_last: WindowPoint,
    touch_scrolling: bool,
    touch_pressed: bool,
    last_touch_at: Option<Instant>,
    pub(crate) frame_count: u64,
    pub(crate) scroll_y: f32,
    pub(crate) target_scroll_y: f32,
    #[educe(Default(expression = Instant::now()))]
    pub(crate) last_frame_time: Instant,
    #[educe(Default(expression = Instant::now()))]
    pub(crate) next_frame_deadline: Instant,
    pub(crate) detected_apps: Vec<String>,
    detected_apps_scan: Job<Vec<String>>,
    #[educe(Default(expression = -1))]
    pub(crate) sidebar_hover: i32,
    pub(crate) popup: Option<PopupState>,
    pub(crate) number_input: Option<NumberInput>,
    pub(crate) ctrl_pressed: bool,
    pub(crate) search: search::SearchState,
    pub(crate) is_light: bool,
    pub(crate) cached_items: Vec<SettingsItem>,
    #[educe(Default(expression = true))]
    pub(crate) items_dirty: bool,
    pub(crate) cached_content_height: f32,
    pub(crate) cached_max_scroll: f32,
    #[educe(Default(expression = WIN_W))]
    pub(crate) win_w: f32,
    #[educe(Default(expression = WIN_H))]
    pub(crate) win_h: f32,
    #[educe(Default(expression = f64::from(WIN_W)))]
    logical_win_w: f64,
    #[educe(Default(expression = f64::from(WIN_H)))]
    logical_win_h: f64,
    pending_dpi_size: Option<WindowSize>,
    pending_dpi_retried: bool,
    dpi_size_check: bool,
    target_monitor: Option<MonitorRef>,
    #[educe(Default(expression = true))]
    pub(crate) focused: bool,
    pub(crate) dots_hovered: bool,
    pub(crate) music_notice_pressed: bool,
    pub(crate) scroll_dragging: bool,
    scroll_drag_offset: f32,
    pub(crate) widget_dragging: Option<WidgetSource>,
    pub(crate) widget_drag_hover_slot: Option<WidgetEditorSlot>,
    pub(crate) widget_preview_hover_slot: Option<WidgetEditorSlot>,
    #[educe(Default(expression = WidgetEditorMode::Expanded))]
    pub(crate) widget_editor_mode: WidgetEditorMode,
    pub(crate) compact_widget_dragging: Option<winisland_core::config::CompactWidgetKind>,
    pub(crate) widget_hover_target: Option<WidgetEditorHover>,
    pub(crate) widget_hover_visual: Option<WidgetEditorHover>,
    pub(crate) widget_hover_progress: f32,
    pub(crate) widget_drag_lift_progress: f32,
    pub(crate) widget_drop_animation: Option<WidgetDropAnimation>,
    pub(crate) page_order: page_order::PageOrderState,
    pub(crate) resource_editor_open: bool,
    pub(crate) resource_editor: resource_editor::ResourceEditorState,
    pub(crate) plugin_widgets: Vec<PluginWidget>,
    pub(crate) plugin_host: Option<Rc<PluginHost>>,
    pub(crate) plugins: Vec<InstalledPlugin>,
    plugin_inventory_scan: Job<Vec<InstalledPlugin>>,
    #[educe(Default(expression = PluginPageTab::Installed))]
    pub(crate) plugin_page_tab: PluginPageTab,
    #[educe(Default(expression = MarketplaceViewState::NotLoaded))]
    pub(crate) marketplace_state: MarketplaceViewState,
    pub(crate) marketplace_installing_id: Option<String>,
    pub(crate) pending_plugin_uninstall_id: Option<String>,
    pub(crate) selected_plugin_id: Option<String>,
    pub(crate) plugin_detail_closing: bool,
    pub(crate) plugin_detail_scroll: f32,
    pub(crate) plugin_detail_max_scroll: f32,
    pub(crate) plugin_status: Option<(String, bool)>,
    pub(crate) plugin_settings_pages: Vec<PluginSettingsPage>,
    pub(crate) plugin_settings_error: Option<(u64, String)>,
    pub(crate) pending_plugin_setting: Option<PendingPluginSetting>,
    pub(crate) pending_setting: Option<AppConfigField>,
    #[cfg(feature = "lingosoul")]
    pub(crate) lingosoul_field: Option<pages::lingosoul::LingoSoulField>,
    #[cfg(feature = "lingosoul")]
    pub(crate) lingosoul_error: Option<String>,
    plugin_request: Option<PluginSettingsRequest>,
    close_requested: bool,
}

impl SettingsApp {
    pub(crate) fn sidebar_page_count(&self) -> usize {
        BUILTIN_SIDEBAR_PAGE_COUNT + self.plugin_settings_pages.len()
    }

    pub(crate) fn active_plugin_settings_page(&self) -> Option<&PluginSettingsPage> {
        self.active_page
            .checked_sub(BUILTIN_SIDEBAR_PAGE_COUNT)
            .and_then(|index| self.plugin_settings_pages.get(index))
    }

    pub(crate) fn page_title(&self) -> String {
        match BuiltinPage::from_index(self.active_page) {
            Some(page) => winisland_core::i18n::tr(page.title_key()),
            None => self
                .active_plugin_settings_page()
                .map(|page| page.title.clone())
                .unwrap_or_default(),
        }
    }

    pub fn new(
        config: AppConfig,
        plugins: Vec<InstalledPlugin>,
        plugin_widgets: Vec<PluginWidget>,
        plugin_settings_pages: Vec<PluginSettingsPage>,
    ) -> Self {
        crate::ui::widget::resource_usage::set_configs(
            &config.resource_metrics,
            &config.compact_resource_metrics,
        );
        winisland_core::config::set_resource_widget_span(
            config.resource_widget_columns,
            config.resource_widget_rows,
        );
        let detected_apps = config.smtc_known_apps.clone();
        Self {
            config,
            detected_apps,
            plugins,
            plugin_widgets,
            plugin_settings_pages,
            ..Self::default()
        }
    }
}
