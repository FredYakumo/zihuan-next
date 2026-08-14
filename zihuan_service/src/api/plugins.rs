use std::fs;
use std::path::PathBuf;

use salvo::http::StatusCode;
use salvo::prelude::*;
use salvo::writing::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const APP_DIR_NAME: &str = "zihuan-next_aibot";
const PLUGINS_FILE_NAME: &str = "plugins.json";

#[derive(Clone, Deserialize, Serialize)]
pub struct PluginRecord {
    pub name: String,
    pub version: String,
    pub installed_at: String,
    pub installation_method: String,
    pub extra_install_metadata: Value,
}

fn plugins_file_path() -> PathBuf {
    zihuan_core::system_config::app_data_dir()
        .join(APP_DIR_NAME)
        .join(PLUGINS_FILE_NAME)
}

fn load_plugins() -> Result<Vec<PluginRecord>, String> {
    let path = plugins_file_path();
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&path)
        .map_err(|error| format!("Failed to read plugin list '{}': {error}", path.display()))?;
    serde_json::from_str(&content)
        .map_err(|error| format!("Failed to parse plugin list '{}': {error}", path.display()))
}

fn save_plugins(plugins: &[PluginRecord]) -> Result<(), String> {
    let path = plugins_file_path();
    let parent = path
        .parent()
        .ok_or_else(|| format!("Plugin data path '{}' has no parent directory", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Failed to create plugin data directory '{}': {error}", parent.display()))?;
    let content = serde_json::to_string_pretty(plugins)
        .map_err(|error| format!("Failed to serialize plugin list: {error}"))?;
    fs::write(&path, content)
        .map_err(|error| format!("Failed to save plugin list '{}': {error}", path.display()))
}

fn validate_plugin(plugin: &PluginRecord) -> Result<(), String> {
    if plugin.name.trim().is_empty() {
        return Err("Plugin name must not be empty".to_string());
    }
    if plugin.version.trim().is_empty() {
        return Err("Plugin version must not be empty".to_string());
    }
    if plugin.installed_at.trim().is_empty() {
        return Err("Plugin install date must not be empty".to_string());
    }
    if plugin.installation_method.trim().is_empty() {
        return Err("Plugin installation method must not be empty".to_string());
    }
    Ok(())
}

fn render_error(res: &mut Response, status: StatusCode, error: String) {
    res.status_code(status);
    res.render(Json(serde_json::json!({ "error": error })));
}

#[handler]
pub async fn list_plugins(_req: &mut Request, res: &mut Response) {
    match load_plugins() {
        Ok(plugins) => res.render(Json(plugins)),
        Err(error) => render_error(res, StatusCode::INTERNAL_SERVER_ERROR, error),
    }
}

#[handler]
pub async fn create_plugin(req: &mut Request, res: &mut Response) {
    let plugin: PluginRecord = match req.parse_json().await {
        Ok(plugin) => plugin,
        Err(error) => return render_error(res, StatusCode::BAD_REQUEST, error.to_string()),
    };
    if let Err(error) = validate_plugin(&plugin) {
        return render_error(res, StatusCode::BAD_REQUEST, error);
    }

    let mut plugins = match load_plugins() {
        Ok(plugins) => plugins,
        Err(error) => return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, error),
    };
    if plugins.iter().any(|item| item.name == plugin.name) {
        return render_error(res, StatusCode::CONFLICT, format!("Plugin '{}' already exists", plugin.name));
    }
    plugins.push(plugin.clone());
    if let Err(error) = save_plugins(&plugins) {
        return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, error);
    }
    res.render(Json(plugin));
}

#[handler]
pub async fn update_plugin(req: &mut Request, res: &mut Response) {
    let name = req.param::<String>("name").unwrap_or_default();
    let plugin: PluginRecord = match req.parse_json().await {
        Ok(plugin) => plugin,
        Err(error) => return render_error(res, StatusCode::BAD_REQUEST, error.to_string()),
    };
    if let Err(error) = validate_plugin(&plugin) {
        return render_error(res, StatusCode::BAD_REQUEST, error);
    }

    let mut plugins = match load_plugins() {
        Ok(plugins) => plugins,
        Err(error) => return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, error),
    };
    let Some(index) = plugins.iter().position(|item| item.name == name) else {
        return render_error(res, StatusCode::NOT_FOUND, format!("Plugin '{name}' was not found"));
    };
    if plugin.name != name && plugins.iter().any(|item| item.name == plugin.name) {
        return render_error(res, StatusCode::CONFLICT, format!("Plugin '{}' already exists", plugin.name));
    }
    plugins[index] = plugin.clone();
    if let Err(error) = save_plugins(&plugins) {
        return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, error);
    }
    res.render(Json(plugin));
}

#[handler]
pub async fn delete_plugin(req: &mut Request, res: &mut Response) {
    let name = req.param::<String>("name").unwrap_or_default();
    let mut plugins = match load_plugins() {
        Ok(plugins) => plugins,
        Err(error) => return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, error),
    };
    let previous_count = plugins.len();
    plugins.retain(|item| item.name != name);
    if plugins.len() == previous_count {
        return render_error(res, StatusCode::NOT_FOUND, format!("Plugin '{name}' was not found"));
    }
    if let Err(error) = save_plugins(&plugins) {
        return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, error);
    }
    res.render(Json(serde_json::json!({ "ok": true })));
}
