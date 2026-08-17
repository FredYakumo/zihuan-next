use std::future::Future;
use std::pin::Pin;

use super::{InstallPluginRequest, PluginRecord};

pub mod sqlite;

pub trait SpecialPluginInstaller: Sync {
    fn component_type(&self) -> &'static str;

    fn install<'a>(
        &'a self,
        request: &'a InstallPluginRequest,
    ) -> Pin<Box<dyn Future<Output = Result<PluginRecord, String>> + Send + 'a>>;

    fn uninstall<'a>(
        &'a self,
        plugin: &'a PluginRecord,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>>;
}

static SQLITE_INSTALLER: sqlite::SqliteSpecialPluginInstaller = sqlite::SqliteSpecialPluginInstaller;

pub fn installer_for(component_type: &str) -> Option<&'static dyn SpecialPluginInstaller> {
    let installer: &'static dyn SpecialPluginInstaller = &SQLITE_INSTALLER;
    (installer.component_type() == component_type).then_some(installer)
}
