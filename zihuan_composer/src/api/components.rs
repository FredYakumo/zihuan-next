use std::path::PathBuf;
use std::sync::Arc;

use salvo::http::StatusCode;
use salvo::prelude::*;
use salvo::writing::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::state::AppState;
use crate::installation::{generate_detailed_install_command, DetailedSetupConfig};
use crate::orchestrator::{emit_progress, SetupOrchestrator};
use crate::storage;
use zihuan_core::storage::ConnectionConfig;

mod special_installers;
pub mod wechat_login;

const COMPONENTS_FILE_NAME: &str = "components.yaml";

#[derive(Clone, Deserialize, Serialize)]
pub struct ComponentRecord {
    #[serde(default = "default_component_id")]
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

fn default_component_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn components_file_path() -> PathBuf {
    crate::config::root().join(COMPONENTS_FILE_NAME)
}

fn load_components() -> Result<Vec<ComponentRecord>, String> {
    crate::config::read_yaml(&components_file_path())
}

pub(crate) fn recover_interrupted_installs() -> Result<(), String> {
    let mut components = load_components()?;
    let mut changed = false;
    for component in &mut components {
        if component.status == "installing" {
            component.status = "failed".into();
            if !component.extra_install_metadata.is_object() {
                component.extra_install_metadata = serde_json::json!({});
            }
            component.extra_install_metadata["last_error"] = Value::String(
                "上次安装在 Composer 退出时中断；可使用相同名称重试，或卸载已创建的资源".into(),
            );
            changed = true;
        }
    }
    if changed {
        save_components(&components)?;
    }
    Ok(())
}

fn save_components(components: &[ComponentRecord]) -> Result<(), String> {
    crate::config::write_yaml(&components_file_path(), components)
}

#[derive(Clone, Deserialize, Serialize)]
pub struct InstallComponentRequest {
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

pub(crate) fn component_from_request(
    request: &InstallComponentRequest,
    status: &str,
) -> ComponentRecord {
    let now = chrono::Utc::now().to_rfc3339();
    let id = reusable_component_id(request.name.trim()).unwrap_or_else(default_component_id);
    let mut metadata = request.extra_install_metadata.clone();
    if !metadata.is_object() {
        metadata = serde_json::json!({});
    }
    if let Some(config) = &request.detailed_config {
        metadata["detailed_config"] = serde_json::to_value(config).unwrap_or(Value::Null);
        metadata["install_method"] = Value::String(request.install_method.clone());
        metadata["compose_path"] = Value::String(
            crate::config::root()
                .join("components")
                .join(&id)
                .join("detailed-compose.yaml")
                .to_string_lossy()
                .to_string(),
        );
    }
    ComponentRecord {
        id,
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

/// A retry of a failed or still pending installation must update the existing
/// record instead of appending a second component with the same name.
fn reusable_component_id(name: &str) -> Option<String> {
    load_components().ok()?.into_iter().find_map(|item| {
        if item.name == name && matches!(item.status.as_str(), "failed" | "installing") {
            Some(item.id)
        } else {
            None
        }
    })
}

/// Finds the record owning a component name. A usable record wins over a leftover
/// `failed`/`installing` one so a retry cannot extend a legacy duplicate name.
fn find_component_by_name(name: &str) -> Result<Option<ComponentRecord>, String> {
    let mut fallback = None;
    for item in load_components()? {
        if item.name != name {
            continue;
        }
        if !matches!(item.status.as_str(), "failed" | "installing") {
            return Ok(Some(item));
        }
        fallback.get_or_insert(item);
    }
    Ok(fallback)
}

/// Reports whether the background task recorded on a component is still running.
/// A record left in `installing` by a crashed process has no live task and may
/// be retried, while a retry during a real install is rejected.
fn has_live_install_task(depot: &Depot, component: &ComponentRecord) -> bool {
    let Some(task_id) =
        component.extra_install_metadata.get("install_task_id").and_then(Value::as_str)
    else {
        return false;
    };
    depot
        .obtain::<Arc<AppState>>()
        .map(|state| state.setup_tasks.lock().unwrap().contains_key(task_id))
        .unwrap_or(false)
}

pub(crate) fn save_component_record(component: &ComponentRecord) -> Result<(), String> {
    let _guard = crate::config::WRITE_LOCK.lock().map_err(|error| error.to_string())?;
    let mut components = load_components()?;
    if let Some(existing) = components.iter_mut().find(|item| item.id == component.id) {
        *existing = component.clone();
    } else {
        components.push(component.clone());
    }
    crate::config::write_yaml_unlocked(&components_file_path(), &components)
}

pub(crate) fn find_component(id: &str) -> Result<ComponentRecord, String> {
    load_components()?
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| format!("Component '{id}' was not found"))
}

pub(crate) fn component_config(component: &ComponentRecord) -> Result<DetailedSetupConfig, String> {
    serde_json::from_value(
        component
            .extra_install_metadata
            .get("detailed_config")
            .cloned()
            .ok_or("Component install configuration is missing")?,
    )
    .map_err(|err| format!("Invalid component install configuration: {err}"))
}

fn component_connections(component: &ComponentRecord) -> Result<Vec<ConnectionConfig>, String> {
    Ok(storage::load_connections()
        .map_err(|err| err.to_string())?
        .into_iter()
        .filter(|c| component.connection_ids.contains(&c.config_id))
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

pub(crate) fn render_error(res: &mut Response, status: StatusCode, error: String) {
    res.status_code(status);
    res.render(Json(serde_json::json!({ "error": error })));
}

#[handler]
pub async fn list_components(_req: &mut Request, res: &mut Response) {
    let components = match load_components() {
        Ok(components) => components,
        Err(error) => return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, error),
    };
    res.render(Json(components));
}

#[handler]
pub async fn install_component(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let request: InstallComponentRequest = match req.parse_json().await {
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
            "Component name, version and component type are required".to_string(),
        );
    }
    match find_component_by_name(request.name.trim()) {
        Ok(Some(existing))
            if existing.status == "installing" && has_live_install_task(depot, &existing) =>
        {
            return render_error(
                res,
                StatusCode::CONFLICT,
                format!("Component '{}' is still being installed", existing.name),
            )
        }
        Ok(Some(existing)) if !matches!(existing.status.as_str(), "failed" | "installing") => {
            return render_error(
                res,
                StatusCode::CONFLICT,
                format!("Component '{}' already exists", existing.name),
            )
        }
        Ok(_) => {}
        Err(error) => return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, error),
    }
    if request.component_type == "zihuan" {
        return crate::self_install::install(request, depot, res).await;
    }
    if let Some(installer) = special_installers::installer_for(&request.component_type) {
        if special_installers::uses_background_install(&request) {
            return install_special_in_background(depot, res, request).await;
        }
        return match installer.install(&request).await {
            Ok(component) => {
                let connections = component_connections(&component).unwrap_or_default();
                // Command-mode installers record the copyable script; the success dialog
                // shows it next to the saved connections.
                let install_command = component
                    .extra_install_metadata
                    .get("install_command")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
                res.render(Json(serde_json::json!({
                    "component": component,
                    "install_command": install_command,
                    "connections": connections
                })))
            }
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
            "Unsupported component installation method".to_string(),
        );
    }
    let mut detailed_config = match request.detailed_config.clone() {
        Some(config) => config,
        None => {
            return render_error(
                res,
                StatusCode::BAD_REQUEST,
                "Detailed component configuration is required".to_string(),
            )
        }
    };
    detailed_config.install_method = if request.install_method.ends_with("docker") {
        crate::installation::DetailedInstallMethod::Docker
    } else {
        crate::installation::DetailedInstallMethod::Binary
    };
    let mut component = component_from_request(
        &request,
        if request.install_method.starts_with("command_") {
            "command_generated"
        } else {
            "installing"
        },
    );
    if request.install_method == "binary" {
        if let Some(command) = binary_uninstall_command(&request.component_type) {
            component.extra_install_metadata["uninstall_command"] = Value::String(command);
        }
    }
    if let Err(error) = save_component_record(&component) {
        return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, error);
    }

    if request.install_method.starts_with("command_") {
        let mut config = detailed_config;
        config.install_method = if request.install_method.ends_with("docker") {
            crate::installation::DetailedInstallMethod::Docker
        } else {
            crate::installation::DetailedInstallMethod::Binary
        };
        let command = if config.search.enabled
            && !config.relational.enabled
            && !config.redis.enabled
            && !config.rustfs.enabled
        {
            crate::installation::generate_retrieval_install_command(
                &config.search,
                &config.install_method,
            )
        } else {
            generate_detailed_install_command(&config)
        };
        match command {
            Ok(result) => {
                let mut connections = result.connections;
                for connection in &mut connections {
                    connection.id = format!("{}-{}", component.id, connection.id);
                    connection.config_id = connection.id.clone();
                }
                for connection in &connections {
                    if let Err(error) = storage::upsert_connection(connection.clone()) {
                        return render_error(
                            res,
                            StatusCode::INTERNAL_SERVER_ERROR,
                            error.to_string(),
                        );
                    }
                }
                component.connection_ids =
                    connections.iter().map(|c| c.config_id.clone()).collect();
                component.extra_install_metadata["install_command"] =
                    Value::String(result.install_command.clone());
                component.extra_install_metadata["connection_config"] =
                    serde_json::to_value(&connections).unwrap_or(Value::Null);
                component.extra_install_metadata["uninstall_command"] =
                    Value::String(command_uninstall_command());
                component.status = "command_generated".to_string();
                component.updated_at = chrono::Utc::now().to_rfc3339();
                if let Err(error) = save_component_record(&component) {
                    return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, error);
                }
                return res.render(Json(serde_json::json!({ "component": component, "install_command": result.install_command, "connections": connections })));
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
    state.register_task(task_id.clone(), progress_tx.clone());
    let orchestrator = SetupOrchestrator::new(task_id.clone(), progress_tx);
    let component_id = component.id.clone();
    component.extra_install_metadata["install_task_id"] = Value::String(task_id.clone());
    if let Err(error) = save_component_record(&component) {
        return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, error);
    }
    tokio::spawn(async move {
        let directory = crate::config::root().join("components").join(&component_id);
        let result = crate::config::COMPONENT_SCOPE
            .scope((component_id.clone(), directory), async {
                if detailed_config.search.enabled
                    && !detailed_config.relational.enabled
                    && !detailed_config.redis.enabled
                    && !detailed_config.rustfs.enabled
                {
                    let method = detailed_config.install_method.clone();
                    orchestrator
                        .run_retrieval_database_install(detailed_config.search, method)
                        .await
                        .map(|_| ())
                } else {
                    orchestrator.run_detailed(detailed_config).await
                }
            })
            .await;
        // A retry reuses the record, so a stale task must not overwrite the
        // status of the attempt that currently owns it.
        if let Ok(mut current) = find_component(&component_id) {
            let owns_record =
                current.extra_install_metadata.get("install_task_id").and_then(Value::as_str)
                    == Some(task_id_for_task.as_str());
            if owns_record {
                match result {
                    Ok(()) => {
                        if let Ok(connections) = storage::load_connections() {
                            current.connection_ids = connections
                                .iter()
                                .filter(|c| c.config_id.starts_with(&format!("{component_id}-")))
                                .map(|c| c.config_id.clone())
                                .collect();
                        }
                        current.status = "installed".to_string();
                        current.updated_at = chrono::Utc::now().to_rfc3339();
                        match save_component_record(&current) {
                            Ok(()) => {
                                orchestrator.emit("finished", "success", "组件安装完成", Some(100))
                            }
                            Err(error) => orchestrator.emit("failed", "error", &error, None),
                        }
                    }
                    Err(error) => {
                        current.status = "failed".to_string();
                        current.extra_install_metadata["last_error"] = Value::String(error.clone());
                        current.updated_at = chrono::Utc::now().to_rfc3339();
                        let _ = save_component_record(&current);
                        orchestrator.emit("failed", "error", &error, None);
                    }
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        state.setup_tasks.lock().unwrap().remove(&task_id_for_task);
    });
    res.render(Json(
        serde_json::json!({ "accepted": true, "task_id": task_id, "component": component }),
    ))
}

/// Runs a long-running special install (NapCat Docker/Native) in a background task and
/// streams its progress over the shared SSE endpoint, mirroring the standard pipeline.
/// The installer owns the success record; this wrapper only persists the failed record
/// and reports the error event when the installer gives up.
async fn install_special_in_background(
    depot: &Depot,
    res: &mut Response,
    request: InstallComponentRequest,
) {
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
    let installer = match special_installers::installer_for(&request.component_type) {
        Some(installer) => installer,
        None => {
            return render_error(
                res,
                StatusCode::BAD_REQUEST,
                "unknown special installer".to_string(),
            )
        }
    };
    let task_id = uuid::Uuid::new_v4().to_string();
    let (progress_tx, _rx) = tokio::sync::broadcast::channel(256);
    state.register_task(task_id.clone(), progress_tx.clone());

    // Placeholder record: duplicate submissions are rejected while it is `installing`
    // with a live task, and a retry reuses its id via the name lookup.
    let mut component = component_from_request(&request, "installing");
    component.extra_install_metadata["install_task_id"] = Value::String(task_id.clone());
    if let Err(error) = save_component_record(&component) {
        state.setup_tasks.lock().unwrap().remove(&task_id);
        return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, error);
    }

    let task_id_for_task = task_id.clone();
    let request_for_task = request.clone();
    let progress_for_task = progress_tx.clone();
    tokio::spawn(async move {
        // Give the SSE subscriber a moment to attach so the first steps (including the
        // command lines) are not missed; the broadcast channel has no replay.
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let result = installer.install_with_progress(&request_for_task, &progress_for_task).await;
        if let Err(error) = result {
            let mut failed = component_from_request(&request_for_task, "installing");
            failed.status = "failed".to_string();
            failed.extra_install_metadata["install_task_id"] =
                Value::String(task_id_for_task.clone());
            failed.extra_install_metadata["last_error"] = Value::String(error.clone());
            failed.updated_at = chrono::Utc::now().to_rfc3339();
            let _ = save_component_record(&failed);
            emit_progress(Some(&progress_for_task), "failed", "error", error.clone());
        } else {
            emit_progress(Some(&progress_for_task), "finished", "success", "组件安装完成".into());
        }
        // Keep the sender registered briefly so late SSE subscribers can still attach.
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        state.setup_tasks.lock().unwrap().remove(&task_id_for_task);
    });

    res.render(Json(
        serde_json::json!({ "accepted": true, "task_id": task_id, "component": component }),
    ))
}

#[handler]
pub async fn enable_component(req: &mut Request, res: &mut Response) {
    lifecycle_component(req, res, true).await;
}
#[handler]
pub async fn disable_component(req: &mut Request, res: &mut Response) {
    lifecycle_component(req, res, false).await;
}

async fn lifecycle_component(req: &mut Request, res: &mut Response, enabled: bool) {
    let id = req.param::<String>("id").unwrap_or_default();
    let mut component = match find_component(&id) {
        Ok(value) => value,
        Err(error) => return render_error(res, StatusCode::NOT_FOUND, error),
    };
    let method = component.installation_method.clone();
    if component.component_type == "zihuan" && method == "native" {
        if let Err(error) = crate::self_install::set_running(&component, enabled).await {
            return render_error(res, StatusCode::BAD_REQUEST, error);
        }
    } else if matches!(method.as_str(), "native" | "binary") {
        if let Err(error) = crate::native::set_running(&component, enabled).await {
            return render_error(res, StatusCode::BAD_REQUEST, error);
        }
    }
    let command = if method.starts_with("command_") {
        let action = if enabled { "up -d" } else { "stop" };
        Some(format!(
            "docker compose -f {} {}",
            component
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
                component.extra_install_metadata.get("compose_path").and_then(Value::as_str)
            {
                let mut command_process = tokio::process::Command::new("docker");
                command_process.args(["compose", "-f", path]);
                if let Some(project) =
                    component.extra_install_metadata.get("project").and_then(Value::as_str)
                {
                    command_process.args(["-p", project]);
                }
                command_process.arg(action);
                if enabled {
                    command_process.arg("-d");
                }
                match command_process.output().await {
                    Ok(output) if output.status.success() => {}
                    Ok(output) => {
                        return render_error(
                            res,
                            StatusCode::BAD_REQUEST,
                            String::from_utf8_lossy(&output.stderr).trim().to_string(),
                        );
                    }
                    Err(error) => {
                        return render_error(res, StatusCode::BAD_REQUEST, error.to_string())
                    }
                }
            }
        }
        None
    };
    component.status = if enabled { "installed" } else { "disabled" }.to_string();
    component.updated_at = chrono::Utc::now().to_rfc3339();
    if let Err(error) = save_component_record(&component) {
        return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, error);
    }
    if let Ok(mut connections) = storage::load_connections() {
        for connection in &mut connections {
            if component.connection_ids.contains(&connection.config_id) {
                connection.enabled = enabled;
            }
        }
        let _ = storage::save_connections(connections);
    }
    res.render(Json(serde_json::json!({ "component": component, "command": command })))
}

#[handler]
pub async fn delete_component(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let id = req
        .param::<String>("id")
        .or_else(|| req.param::<String>("name"))
        .unwrap_or_default();
    let components = match load_components() {
        Ok(components) => components,
        Err(error) => return render_error(res, StatusCode::INTERNAL_SERVER_ERROR, error),
    };
    let target = components.iter().find(|item| item.id == id || item.name == id).cloned();
    let Some(target) = target else {
        return render_error(res, StatusCode::NOT_FOUND, format!("Component '{id}' was not found"));
    };
    if target.status == "installing" && has_live_install_task(depot, &target) {
        return render_error(res, StatusCode::CONFLICT, "组件仍在安装中，请等待安装结束".into());
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
    // The teardown streams its commands over SSE like an install; the task reports the
    // final event and keeps the sender around briefly for late subscribers.
    let task_id = uuid::Uuid::new_v4().to_string();
    let (progress_tx, _rx) = tokio::sync::broadcast::channel(256);
    state.register_task(task_id.clone(), progress_tx.clone());
    let target_for_task = target.clone();
    let task_id_for_task = task_id.clone();
    let progress_for_task = progress_tx.clone();
    tokio::spawn(async move {
        // Give the SSE subscriber a moment to attach so the first steps are not missed.
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let mut failure: Option<String> = None;
        if target_for_task.component_type == "zihuan"
            && target_for_task.installation_method == "native"
        {
            if let Err(error) = crate::self_install::uninstall(&target_for_task).await {
                failure = Some(error);
            }
        } else if target_for_task.installation_method == "binary" {
            if let Err(error) = crate::native::uninstall(&target_for_task).await {
                failure = Some(error);
            }
        }
        if let Some(installer) = special_installers::installer_for(&target_for_task.component_type)
        {
            if let Err(error) =
                installer.uninstall_with_progress(&target_for_task, &progress_for_task).await
            {
                failure = Some(error);
            }
        }
        if failure.is_none() {
            if target_for_task.extra_install_metadata.get("detailed_config").is_some()
                || target_for_task.component_type == "zihuan"
            {
                if target_for_task.installation_method == "docker" {
                    if let Some(path) = target_for_task
                        .extra_install_metadata
                        .get("compose_path")
                        .and_then(Value::as_str)
                    {
                        emit_progress(
                            Some(&progress_for_task),
                            "uninstall",
                            "running",
                            format!(
                                "执行: docker compose -f {path} down --volumes --remove-orphans"
                            ),
                        );
                        match tokio::process::Command::new("docker")
                            .args(["compose", "-f", path, "down", "--volumes", "--remove-orphans"])
                            .output()
                            .await
                        {
                            Ok(output) if output.status.success() => {
                                emit_progress(
                                    Some(&progress_for_task),
                                    "uninstall",
                                    "success",
                                    "容器已停止并移除".to_string(),
                                );
                            }
                            Ok(output) => {
                                failure = Some(format!(
                                    "docker compose down 失败: {}",
                                    String::from_utf8_lossy(&output.stderr).trim()
                                ))
                            }
                            Err(error) => {
                                failure = Some(format!("执行 docker compose 失败: {error}"))
                            }
                        }
                    }
                }
            }
        }
        match failure {
            None => {
                for connection_id in &target_for_task.connection_ids {
                    let _ = storage::delete_connection(connection_id);
                }
                let mut components = match load_components() {
                    Ok(components) => components,
                    Err(error) => {
                        emit_progress(
                            Some(&progress_for_task),
                            "uninstall_failed",
                            "error",
                            error.clone(),
                        );
                        return;
                    }
                };
                components.retain(|item| {
                    item.id != target_for_task.id && item.name != target_for_task.name
                });
                if let Err(error) = save_components(&components) {
                    emit_progress(
                        Some(&progress_for_task),
                        "uninstall_failed",
                        "error",
                        error.clone(),
                    );
                    return;
                }
                emit_progress(
                    Some(&progress_for_task),
                    "finished",
                    "success",
                    format!("卸载完成: {}", target_for_task.name),
                );
            }
            Some(error) => {
                // The record and its connections survive so the operator can run the
                // command manually (the frontend shows the copy box) and retry.
                emit_progress(Some(&progress_for_task), "uninstall_failed", "error", error);
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        state.setup_tasks.lock().unwrap().remove(&task_id_for_task);
    });
    res.render(Json(serde_json::json!({ "accepted": true, "task_id": task_id })));
}
