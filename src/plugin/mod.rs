pub(crate) use winisland_builtin_plugins::{BuiltinPlugin, builtin_plugins};
pub use winisland_builtin_plugins::{calendar, timer};
pub mod inventory;
pub(crate) mod system;

pub(crate) fn is_builtin(id: &str) -> bool {
    builtin_plugin(id).is_some()
}

pub(crate) fn builtin_plugin(id: &str) -> Option<BuiltinPlugin> {
    builtin_plugins().into_iter().find(|plugin| {
        winisland_plugin_host::loader::PluginMetadata::from(&plugin.descriptor.metadata).id == id
    })
}
