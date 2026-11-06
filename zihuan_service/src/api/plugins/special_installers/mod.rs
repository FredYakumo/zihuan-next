use std::future::Future;
use std::pin::Pin;

use super::{InstallPluginRequest, PluginRecord};

pub mod sqlite;
pub mod wechat_pad;

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

static SQLITE_INSTALLER: sqlite::SqliteSpecialPluginInstaller =
    sqlite::SqliteSpecialPluginInstaller;
static WECHAT_PAD_INSTALLER: wechat_pad::WeChatPadProSpecialPluginInstaller =
    wechat_pad::WeChatPadProSpecialPluginInstaller;

static INSTALLERS: [&dyn SpecialPluginInstaller; 2] = [&SQLITE_INSTALLER, &WECHAT_PAD_INSTALLER];

pub fn installer_for(component_type: &str) -> Option<&'static dyn SpecialPluginInstaller> {
    INSTALLERS
        .iter()
        .copied()
        .find(|installer| installer.component_type() == component_type)
}
