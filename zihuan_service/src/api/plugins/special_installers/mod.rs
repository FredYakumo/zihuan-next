use std::future::Future;
use std::net::TcpListener;
use std::path::Path;
use std::pin::Pin;

use super::{InstallPluginRequest, PluginRecord};
use crate::setup_orchestrator::SetupProgressEvent;

pub mod napcat;
pub mod sqlite;
pub mod wechat_pad;

pub trait SpecialPluginInstaller: Sync {
    fn component_type(&self) -> &'static str;

    fn install<'a>(
        &'a self,
        request: &'a InstallPluginRequest,
    ) -> Pin<Box<dyn Future<Output = Result<PluginRecord, String>> + Send + 'a>>;

    /// Same install, but reporting each step (including the commands being run) to the
    /// SSE progress channel, like the standard compose/binary pipeline. The default
    /// ignores the channel; long-running installers override it.
    fn install_with_progress<'a>(
        &'a self,
        request: &'a InstallPluginRequest,
        _progress: &'a tokio::sync::broadcast::Sender<SetupProgressEvent>,
    ) -> Pin<Box<dyn Future<Output = Result<PluginRecord, String>> + Send + 'a>> {
        Box::pin(async move { self.install(request).await })
    }

    /// Same uninstall, reporting each step (including the commands being run) to the
    /// SSE progress channel. The default ignores the channel.
    fn uninstall_with_progress<'a>(
        &'a self,
        plugin: &'a PluginRecord,
        _progress: &'a tokio::sync::broadcast::Sender<SetupProgressEvent>,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>> {
        Box::pin(async move { self.uninstall(plugin).await })
    }

    fn uninstall<'a>(
        &'a self,
        plugin: &'a PluginRecord,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + 'a>>;
}

/// Long-running special installs (NapCat Docker/Native) run in a background task with
/// SSE progress; the instant ones (command generation, embedded SQLite, WeChatPadPro)
/// stay synchronous.
pub fn uses_background_install(request: &InstallPluginRequest) -> bool {
    request.component_type == "napcat"
        && !matches!(
            request.install_method.as_str(),
            "command_docker" | "command_native" | "command_binary" | "binary"
        )
}

static SQLITE_INSTALLER: sqlite::SqliteSpecialPluginInstaller =
    sqlite::SqliteSpecialPluginInstaller;
static WECHAT_PAD_INSTALLER: wechat_pad::WeChatPadProSpecialPluginInstaller =
    wechat_pad::WeChatPadProSpecialPluginInstaller;
static NAPCAT_INSTALLER: napcat::NapCatSpecialPluginInstaller = napcat::NapCatSpecialPluginInstaller;

static INSTALLERS: [&dyn SpecialPluginInstaller; 3] =
    [&SQLITE_INSTALLER, &WECHAT_PAD_INSTALLER, &NAPCAT_INSTALLER];

pub fn installer_for(component_type: &str) -> Option<&'static dyn SpecialPluginInstaller> {
    INSTALLERS
        .iter()
        .copied()
        .find(|installer| installer.component_type() == component_type)
}

/// Shared helpers for the Docker-based special installers below. They deliberately
/// shell out to the Docker CLI instead of any SDK, like the rest of the codebase.

/// Last 8 alphanumeric characters of a plugin id, used for compose project names and
/// per-plugin data directories.
pub(super) fn short_id(value: &str) -> String {
    value.chars().filter(|c| c.is_ascii_alphanumeric()).take(8).collect()
}

/// First free port at or above `start`, falling back to `start` when nothing in the
/// scan range is bindable.
pub(super) fn pick_free_port(start: u16) -> u16 {
    (start..start.saturating_add(50))
        .find(|port| TcpListener::bind(("127.0.0.1", *port)).is_ok())
        .unwrap_or(start)
}

pub(super) async fn docker_compose_available() -> bool {
    matches!(
        tokio::process::Command::new("docker").args(["compose", "version"]).output().await,
        Ok(output) if output.status.success()
    )
}

pub(super) async fn run_compose(
    compose_path: &Path,
    project: &str,
    args: &[&str],
) -> Result<(), String> {
    let output = tokio::process::Command::new("docker")
        .arg("compose")
        .arg("-f")
        .arg(compose_path)
        .arg("-p")
        .arg(project)
        .args(args)
        .output()
        .await
        .map_err(|error| format!("执行 docker compose 失败: {error}"))?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "docker compose {} 失败: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr).trim()
    ))
}

/// The standard detailed config only feeds the expose/target flags to the special
/// installers; its component sections are unrelated defaults that would only pollute
/// the record.
pub(super) fn strip_unrelated_metadata(plugin: &mut PluginRecord) {
    if let Some(metadata) = plugin.extra_install_metadata.as_object_mut() {
        metadata.remove("detailed_config");
    }
}
