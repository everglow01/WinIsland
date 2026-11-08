pub mod calendar;
mod geometry;
pub mod timer;

use winisland_plugin_api::PluginDescriptorV2;

pub struct BuiltinPlugin {
    pub descriptor: PluginDescriptorV2,
    pub name_key: &'static str,
    pub description_key: &'static str,
}

pub fn builtin_plugins() -> [BuiltinPlugin; 2] {
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
