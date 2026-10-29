//! Registry of installed dynamic script plugins.
//!
//! Plugins live in `<program dir>/plugins/<name>/` with a `plugin.py` (or `plugin.mjs` /
//! `plugin.ts` / `plugin.js`) entry declaring `PLUGIN_MANIFEST`. The catalog is built once
//! at service startup and kept in memory; installing or editing a plugin requires a
//! restart, the same as a node script does.
//!
//! A plugin's `plugin_type` decides which host subsystem picks it up. This module owns the
//! catalog itself and the lookups; feature-specific adapters (for example the web search
//! engine) live next to the subsystem they plug into.

use std::sync::{LazyLock, RwLock};
use std::time::Duration;

use dynamic_script_engine::PluginCatalogEntry;
use serde_json::Value;

use crate::error::{Error, Result};

/// The plugin_type that registers a dynamic script as a web search engine.
pub const PLUGIN_TYPE_WEB_SEARCH_ENGINE: &str = "web_search_engine";

/// The plugin_type that registers a dynamic script as a Brain tool.
pub const PLUGIN_TYPE_BRAIN_TOOL: &str = "brain_tool";

static SCRIPT_PLUGINS: LazyLock<RwLock<Vec<PluginCatalogEntry>>> =
    LazyLock::new(|| RwLock::new(Vec::new()));

/// Scans the plugins directory and replaces the installed-plugin catalog.
///
/// Called once at service startup, next to the DAG node catalog load. Diagnostics from
/// broken plugins are logged, not fatal: one broken plugin must not take down the runtime
/// or hide the others.
pub fn reload_script_plugin_catalog() -> Result<Vec<PluginCatalogEntry>> {
    reload_script_plugin_catalog_with_diagnostics().map(|(plugins, _)| plugins)
}

/// Same scan as [`reload_script_plugin_catalog`], also returning per-plugin failures.
///
/// Installing a plugin reports those diagnostics to the caller, because a plugin whose
/// manifest or dependencies fail is exactly what the operator needs to see.
pub fn reload_script_plugin_catalog_with_diagnostics(
) -> Result<(Vec<PluginCatalogEntry>, Vec<dynamic_script_engine::ScriptDiagnostic>)> {
    let workspace =
        std::env::current_dir().map_err(|error| Error::ValidationError(error.to_string()))?;
    let runtime = crate::config::ConfigCenter::shared().load_root()?;
    let catalog = dynamic_script_engine::load_plugin_catalog(
        &workspace,
        &dynamic_script_engine::plugin_dir(&workspace),
        &runtime.node_runtime,
        &runtime.python_runtime,
    )
    .map_err(|error| Error::ValidationError(error.to_string()))?;
    for diagnostic in &catalog.diagnostics {
        log::warn!("dynamic script plugin {:?}: {}", diagnostic.language, diagnostic.message);
    }
    *SCRIPT_PLUGINS
        .write()
        .map_err(|_| Error::ValidationError("插件注册表互斥锁不可用".to_string()))? =
        catalog.plugins.clone();
    Ok((catalog.plugins, catalog.diagnostics))
}

/// Drops one plugin from the in-memory catalog after its directory was removed.
///
/// The on-disk catalog is rebuilt on the next start anyway; this only keeps the running UI
/// truthful until then, without rescanning every other plugin.
pub fn forget_script_plugin(name: &str) {
    if let Ok(mut plugins) = SCRIPT_PLUGINS.write() {
        plugins.retain(|plugin| plugin.name != name);
    }
}

/// Every installed plugin, in directory order.
pub fn script_plugin_catalog() -> Vec<PluginCatalogEntry> {
    SCRIPT_PLUGINS.read().map(|plugins| plugins.clone()).unwrap_or_default()
}

/// Installed plugins whose `plugin_type` matches, for one subsystem's registry.
pub fn plugins_of_type(plugin_type: &str) -> Vec<PluginCatalogEntry> {
    script_plugin_catalog()
        .into_iter()
        .filter(|plugin| plugin.plugin_type == plugin_type)
        .collect()
}

pub fn find_script_plugin(name: &str) -> Option<PluginCatalogEntry> {
    script_plugin_catalog().into_iter().find(|plugin| plugin.name == name)
}

/// The provider names installed for one plugin type, for error messages.
pub fn installed_plugin_names(plugin_type: &str) -> Vec<String> {
    plugins_of_type(plugin_type).into_iter().map(|plugin| plugin.name).collect()
}

/// Runs one method of an installed plugin in the plugin's own dependency environment.
///
/// `config` is handed to the plugin as its connection/config object verbatim; callers that
/// own credentials build it first. `limit` is the runner watchdog budget, which should
/// exceed whatever timeout the plugin itself applies so a slow upstream surfaces as the
/// plugin's own error instead of a killed process.
pub fn call_plugin_method(
    plugin: &PluginCatalogEntry,
    method: &str,
    args: Value,
    config: Value,
    limit: Duration,
) -> Result<Value> {
    let workspace =
        std::env::current_dir().map_err(|error| Error::ValidationError(error.to_string()))?;
    let runtime = crate::config::ConfigCenter::shared().load_root()?;
    let plugin_root = dynamic_script_engine::plugin_dir(&workspace);
    let plugin_path = plugin_root.join(&plugin.directory);
    let script_path = plugin_root.join(&plugin.script);
    dynamic_script_engine::execute_plugin_call(
        &workspace,
        &plugin_path,
        plugin.language,
        &runtime.node_runtime,
        &runtime.python_runtime,
        &script_path,
        method,
        args,
        config,
        Some(limit),
    )
    .map_err(|error| Error::ValidationError(error.to_string()))
}
