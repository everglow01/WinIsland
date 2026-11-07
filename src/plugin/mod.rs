pub use winisland_plugin_calendar as calendar;
pub use winisland_plugin_timer as timer;
pub mod inventory;
pub(crate) mod system;

pub(crate) struct BuiltinPlugin {
    pub descriptor: winisland_plugin_api::PluginDescriptorV2,
    pub name_key: &'static str,
    pub description_key: &'static str,
}

pub(crate) fn builtin_plugins() -> [BuiltinPlugin; 2] {
    [
        BuiltinPlugin {
            descriptor: calendar::descriptor(),
            name_key: "page_calendar",
            description_key: "plugin_calendar_description",
        },
        BuiltinPlugin {
            descriptor: timer::descriptor(),
            name_key: "page_timer",
            description_key: "plugin_timer_description",
        },
    ]
}

pub(crate) fn is_builtin(id: &str) -> bool {
    builtin_plugin(id).is_some()
}

pub(crate) fn builtin_plugin(id: &str) -> Option<BuiltinPlugin> {
    builtin_plugins().into_iter().find(|plugin| {
        winisland_plugin_host::loader::PluginMetadata::from(&plugin.descriptor.metadata).id == id
    })
}
