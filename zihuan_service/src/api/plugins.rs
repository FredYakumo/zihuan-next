use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use salvo::http::StatusCode;
use salvo::prelude::*;
use salvo::writing::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::state::AppState;
use crate::setup_orchestrator::{
    generate_detailed_install_command, DetailedSetupConfig, SetupOrchestrator,
};
use zihuan_core::storage::{self, ConnectionConfig};

mod special_installers;

const PLUGINS_FILE_NAME: &str = "plugins.json";

#[derive(Clone, Deserialize, Serialize)]
pub struct PluginRecord {
    #[serde(default = "default_plugin_id")]
    pub id: String,
    pub name: String,
    pub version: String,
    pub installed_at: String,
    pub installation_method: String,
    pub extra_install_metadata: Value,
    #[serde(default)]
    pub component_type: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub connection_ids: Vec<String>,
    #[serde(default)]
    pub updated_at: String,
}

fn default_plugin_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn plugins_file_path() -> PathBuf {
    zihuan_core::system_config::application_data_dir().join(PLUGINS_FILE_NAME)
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
    fs::create_dir_all(parent).map_err(|error| {
        format!("Failed to create plugin data directory '{}': {error}", parent.display())
    })?;
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

#[derive(Deserialize)]
pub struct InstallPluginRequest {
    pub name: String,
    pub version: String,
    pub component_type: String,
    #[serde(default)]
    pub install_method: String,
    #[serde(default)]
    pub detailed_config: Option<DetailedSetupConfig>,
    #[serde(default)]
    pub extra_install_metadata: Value,
}

pub(crate) fn plugin_from_request(request: &InstallPluginRequest, status: &str) -> PluginRecord {
    let now = chrono::Utc::now().to_rfc3339();
    let mut metadata = request.extra_install_metadata.clone();
    if !metadata.is_object() {
        metadata = serde_json::json!({});
    }
    if let Some(config) = &request.detailed_config {
        metadata["detailed_config"] = serde_json::to_value(config).unwrap_or(Value::Null);
        metadata["install_method"] = Value::String(request.install_method.clone());
        metadata["compose_path"] = Value::String(
            zihuan_core::system_config::application_data_dir()
                .join(format!("plugin-{}.yaml", request.name.trim().replace(' ', "-")))
                .to_string_lossy()
                .to_string(),
        );
    }
    PluginRecord {
        id: default_plugin_id(),
        name: request.name.trim().to_string(),
        version: request.version.trim().to_string(),
        installed_at: now.clone(),
        installation_method: request.install_method.clone(),
        extra_install_metadata: metadata,
        component_type: request.component_type.clone(),
        status: status.to_string(),
        connection_ids: Vec::new(),
        updated_at: now,
    }
}

pub(crate) fn save_plugin_record(plugin: &PluginRecord) -> Result<(), String> {
    let mut plugins = load_plugins()?;
    if let Some(existing) = plugins.iter_mut().find(|item| item.id == plugin.id) {
        *existing = plugin.clone();
    } else {
        plugins.push(plugin.clone());
    }
    save_plugins(&plugins)
}

fn find_plugin(id: &str) -> Result<PluginRecord, String> {
    load_plugins()?
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| format!("Plugin '{id}' was not found"))
}

fn plugin_config(plugin: &PluginRecord) -> Result<DetailedSetupConfig, String> {
    serde_json::from_value(
        plugin
            .extra_install_metadata
            .get("detailed_config")
            .cloned()
            .ok_or("Plugin install configuration is missing")?,
    )
    .map_err(|err| format!("Invalid plugin install configuration: {err}"))
}

fn plugin_connections(plugin: &PluginRecord) -> Result<Vec<ConnectionConfig>, String> {
    Ok(storage::load_connections()
        .map_err(|err| err.to_string())?
        .into_iter()
        .filter(|c| plugin.connection_ids.contains(&c.config_id))
        .collect())
}

fn command_uninstall_command() -> String {
    "cd ~/zihuan-next-install && docker compose -f docker-compose.yaml down --volumes --remove-orphans".to_string()
}

fn binary_uninstall_command(component_type: &str) -> Option<String> {
    let (brew_package, apt_package, dnf_package, pacman_package) = match component_type {
        "mysql" => ("mysql", "mysql-server", "mysql-server", "mysql-server"),
        "redis" => ("redis", "redis", "redis", "redis"),
        "elasticsearch" => ("elasticsearch", "elasticsearch", "elasticsearch", "elasticsearch"),
        _ => return None,
    };
    Some(format!(
        "# Remove the locally installed {component_type} package\n\
if command -v brew >/dev/null 2>&1; then\n  brew uninstall {brew_package}\n\
elif command -v apt-get >/dev/null 2>&1; then\n  sudo apt-get purge -y {apt_package}\n\
elif command -v dnf >/dev/null 2>&1; then\n  sudo dnf remove -y {dnf_package}\n\
elif command -v pacman >/dev/null 2>&1; then\n  sudo pacman -Rns --noconfirm {pacman_package}\n\
else\n  echo 'No supported package manager was found. Remove {component_type} manually.'\n\
fi"
    ))
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
        return render_error(
            res,
            StatusCode::CONFLICT,
            format!("Plugin '{}' already exists", plugin.name),
        );
    }
    plugins.push(plugin.clone());
    if let Err(error) = save_plugins(&plugins) {
        return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, error);
    }
    res.render(Json(plugin));
}

#[handler]
pub async fn install_plugin(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let request: InstallPluginRequest = match req.parse_json().await {
        Ok(value) => value,
        Err(err) => return render_error(res, StatusCode::BAD_REQUEST, err.to_string()),
    };
    if request.name.trim().is_empty()
        || request.version.trim().is_empty()
        || request.component_type.trim().is_empty()
    {
        return render_error(
            res,
            StatusCode::BAD_REQUEST,
            "Plugin name, version and component type are required".to_string(),
        );
    }
    if let Some(installer) = special_installers::installer_for(&request.component_type) {
        return match installer.install(&request).await {
            Ok(plugin) => res.render(Json(serde_json::json!({ "plugin": plugin }))),
            Err(error) => render_error(res, StatusCode::INTERNAL_SERVER_ERROR, error),
        };
    }
    if !matches!(
        request.install_method.as_str(),
        "docker" | "binary" | "command_docker" | "command_binary"
    ) {
        return render_error(
            res,
            StatusCode::BAD_REQUEST,
            "Unsupported plugin installation method".to_string(),
        );
    }
    let detailed_config = match request.detailed_config.clone() {
        Some(config) => config,
        None => {
            return render_error(
                res,
                StatusCode::BAD_REQUEST,
                "Detailed plugin configuration is required".to_string(),
            )
        }
    };
    let mut plugin = plugin_from_request(
        &request,
        if request.install_method.starts_with("command_") {
            "command_generated"
        } else {
            "installing"
        },
    );
    if request.install_method == "binary" {
        if let Some(command) = binary_uninstall_command(&request.component_type) {
            plugin.extra_install_metadata["uninstall_command"] = Value::String(command);
        }
    }
    if let Err(error) = save_plugin_record(&plugin) {
        return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, error);
    }

    if request.install_method.starts_with("command_") {
        let mut config = detailed_config;
        config.install_method = if request.install_method.ends_with("docker") {
            crate::setup_orchestrator::DetailedInstallMethod::Docker
        } else {
            crate::setup_orchestrator::DetailedInstallMethod::Binary
        };
        match generate_detailed_install_command(&config) {
            Ok(result) => {
                let connections = result.connections;
                for connection in &connections {
                    if let Err(error) = storage::upsert_connection(connection.clone()) {
                        return render_error(
                            res,
                            StatusCode::INTERNAL_SERVER_ERROR,
                            error.to_string(),
                        );
                    }
                }
                plugin.connection_ids = connections.iter().map(|c| c.config_id.clone()).collect();
                plugin.extra_install_metadata["install_command"] =
                    Value::String(result.install_command.clone());
                plugin.extra_install_metadata["connection_config"] =
                    serde_json::to_value(&connections).unwrap_or(Value::Null);
                plugin.extra_install_metadata["uninstall_command"] =
                    Value::String(command_uninstall_command());
                plugin.status = "command_generated".to_string();
                plugin.updated_at = chrono::Utc::now().to_rfc3339();
                if let Err(error) = save_plugin_record(&plugin) {
                    return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, error);
                }
                return res.render(Json(serde_json::json!({ "plugin": plugin, "install_command": result.install_command, "connections": connections })));
            }
            Err(error) => return render_error(res, StatusCode::BAD_REQUEST, error),
        }
    }

    let state = match depot.obtain::<Arc<AppState>>() {
        Ok(state) => state.clone(),
        Err(_) => {
            return render_error(
                res,
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to obtain app state".to_string(),
            )
        }
    };
    let task_id = uuid::Uuid::new_v4().to_string();
    let task_id_for_task = task_id.clone();
    let (progress_tx, _rx) = tokio::sync::broadcast::channel(256);
    state.setup_tasks.lock().unwrap().insert(task_id.clone(), progress_tx.clone());
    let orchestrator = SetupOrchestrator::new(task_id.clone(), progress_tx);
    let plugin_id = plugin.id.clone();
    tokio::spawn(async move {
        let result = orchestrator.run_detailed(detailed_config).await;
        let mut current = match find_plugin(&plugin_id) {
            Ok(value) => value,
            Err(_) => return,
        };
        match result {
            Ok(()) => {
                if let Some(path) =
                    current.extra_install_metadata.get("compose_path").and_then(Value::as_str)
                {
                    let source = zihuan_core::system_config::application_data_dir()
                        .join("detailed-compose.yaml");
                    let _ = tokio::fs::copy(source, path).await;
                }
                if let Ok(connections) = storage::load_connections() {
                    current.connection_ids = connections
                        .iter()
                        .filter(|c| c.name.starts_with("setup-detailed-"))
                        .map(|c| c.config_id.clone())
                        .collect();
                }
                current.status = "installed".to_string();
                current.updated_at = chrono::Utc::now().to_rfc3339();
                let _ = save_plugin_record(&current);
            }
            Err(error) => {
                current.status = "failed".to_string();
                current.extra_install_metadata["last_error"] = Value::String(error);
                current.updated_at = chrono::Utc::now().to_rfc3339();
                let _ = save_plugin_record(&current);
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        state.setup_tasks.lock().unwrap().remove(&task_id_for_task);
    });
    res.render(Json(
        serde_json::json!({ "accepted": true, "task_id": task_id, "plugin": plugin }),
    ))
}

#[handler]
pub async fn enable_plugin(req: &mut Request, res: &mut Response) {
    lifecycle_plugin(req, res, true).await;
}
#[handler]
pub async fn disable_plugin(req: &mut Request, res: &mut Response) {
    lifecycle_plugin(req, res, false).await;
}

async fn lifecycle_plugin(req: &mut Request, res: &mut Response, enabled: bool) {
    let id = req.param::<String>("id").unwrap_or_default();
    let mut plugin = match find_plugin(&id) {
        Ok(value) => value,
        Err(error) => return render_error(res, StatusCode::NOT_FOUND, error),
    };
    let method = plugin.installation_method.clone();
    let command = if method.starts_with("command_") {
        let action = if enabled { "up -d" } else { "stop" };
        Some(format!(
            "docker compose -f {} {}",
            plugin
                .extra_install_metadata
                .get("compose_path")
                .and_then(Value::as_str)
                .unwrap_or("docker-compose.yaml"),
            action
        ))
    } else {
        let action = if enabled { "up" } else { "stop" };
        if method == "docker" {
            if let Some(path) =
                plugin.extra_install_metadata.get("compose_path").and_then(Value::as_str)
            {
                let mut command_process = tokio::process::Command::new("docker");
                command_process.args(["compose", "-f", path, action]);
                if enabled {
                    command_process.arg("-d");
                }
                let output = command_process.output().await;
                if let Ok(output) = output {
                    if !output.status.success() {
                        return render_error(
                            res,
                            StatusCode::BAD_REQUEST,
                            String::from_utf8_lossy(&output.stderr).trim().to_string(),
                        );
                    }
                }
            }
        }
        None
    };
    plugin.status = if enabled { "installed" } else { "disabled" }.to_string();
    plugin.updated_at = chrono::Utc::now().to_rfc3339();
    if let Err(error) = save_plugin_record(&plugin) {
        return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, error);
    }
    if let Ok(mut connections) = storage::load_connections() {
        for connection in &mut connections {
            if plugin.connection_ids.contains(&connection.config_id) {
                connection.enabled = enabled;
            }
        }
        let _ = storage::save_connections(connections);
    }
    res.render(Json(serde_json::json!({ "plugin": plugin, "command": command })))
}

#[handler]
pub async fn update_plugin(req: &mut Request, res: &mut Response) {
    let name = req
        .param::<String>("id")
        .or_else(|| req.param::<String>("name"))
        .unwrap_or_default();
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
    let Some(index) = plugins.iter().position(|item| item.id == name || item.name == name) else {
        return render_error(res, StatusCode::NOT_FOUND, format!("Plugin '{name}' was not found"));
    };
    if plugin.name != name && plugins.iter().any(|item| item.name == plugin.name) {
        return render_error(
            res,
            StatusCode::CONFLICT,
            format!("Plugin '{}' already exists", plugin.name),
        );
    }
    plugins[index] = plugin.clone();
    if let Err(error) = save_plugins(&plugins) {
        return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, error);
    }
    res.render(Json(plugin));
}

#[handler]
pub async fn delete_plugin(req: &mut Request, res: &mut Response) {
    let id = req
        .param::<String>("id")
        .or_else(|| req.param::<String>("name"))
        .unwrap_or_default();
    let mut plugins = match load_plugins() {
        Ok(plugins) => plugins,
        Err(error) => return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, error),
    };
    let previous_count = plugins.len();
    let target = plugins.iter().find(|item| item.id == id || item.name == id).cloned();
    let Some(target) = target else {
        return render_error(res, StatusCode::NOT_FOUND, format!("Plugin '{id}' was not found"));
    };
    if let Some(installer) = special_installers::installer_for(&target.component_type) {
        if let Err(error) = installer.uninstall(&target).await {
            return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, error);
        }
    }
    if let Some(config) = target.extra_install_metadata.get("detailed_config") {
        if target.installation_method == "docker" {
            if let Some(path) =
                target.extra_install_metadata.get("compose_path").and_then(Value::as_str)
            {
                let _ = tokio::process::Command::new("docker")
                    .args(["compose", "-f", path, "down", "--volumes", "--remove-orphans"])
                    .output()
                    .await;
            }
        }
        let _ = config;
    }
    for connection_id in &target.connection_ids {
        let _ = storage::delete_connection(connection_id);
    }
    plugins.retain(|item| item.id != target.id && item.name != target.name);
    if plugins.len() == previous_count {
        return render_error(res, StatusCode::NOT_FOUND, format!("Plugin '{id}' was not found"));
    }
    if let Err(error) = save_plugins(&plugins) {
        return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, error);
    }
    let uninstall_command = target
        .extra_install_metadata
        .get("uninstall_command")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    res.render(Json(serde_json::json!({ "ok": true, "uninstall_command": uninstall_command })));
}
