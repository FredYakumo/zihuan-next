use std::io::{Cursor, Write};
use std::path::Path;

use chrono::Utc;
use chrono::{DateTime, SecondsFormat};
use salvo::http::StatusCode;
use salvo::prelude::*;
use salvo::writing::Json;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::task;
use zihuan_core::config::ConfigCenter;
use zihuan_core::python_runtime::PythonRuntimeConfig;
use zihuan_core::system_config::{GlobalSettingsSection, ModelHttpApiKey, ModelHttpServiceSettings};
use zihuan_core::config::llm_refs::load_llm_refs;
use zihuan_core::model_inference::model_config::ModelRefSpec;
use uuid::Uuid;

use zihuan_core::python_runtime_resolver::check_python_runtime;
use zip::write::SimpleFileOptions;
use zip::ZipArchive;

const CHAT_HISTORY_DIR: &str = "chat_history";
const UPLOADED_IMAGES_DIR: &str = "uploaded_images";
const TEXT_EMBEDDING_MODEL_DIR: &str = "models/text_embedding";

#[derive(Serialize)]
pub struct StorageEntry {
    pub label: String,
    pub path: String,
    pub exists: bool,
}

#[derive(Serialize)]
pub struct ModelEntry {
    pub name: String,
    pub path: String,
    pub valid: bool,
    pub size_bytes: Option<u64>,
}

#[derive(Serialize)]
pub struct ModelGroup {
    pub label: String,
    pub dir: String,
    pub models: Vec<ModelEntry>,
}

#[derive(Serialize)]
pub struct StorageInfoResponse {
    pub data_dir: String,
    pub storage_entries: Vec<StorageEntry>,
    pub model_groups: Vec<ModelGroup>,
}

#[derive(Serialize)]
pub struct PythonRuntimeResponse {
    pub config: PythonRuntimeConfig,
    pub available: bool,
    pub command: Option<String>,
    pub executable_path: Option<String>,
    pub version: Option<String>,
    pub diagnostic: Option<String>,
}

#[derive(Serialize)]
pub struct PythonRuntimeSelectionResponse {
    pub cancelled: bool,
    pub runtime: Option<PythonRuntimeResponse>,
}

#[derive(Serialize)]
pub struct ModelHttpSettingsResponse {
    pub enabled: bool,
    pub endpoint: String,
    pub public_model_config_ids: Vec<String>,
    pub api_keys: Vec<ModelHttpApiKeyResponse>,
}

#[derive(Serialize)]
pub struct ModelHttpApiKeyResponse {
    pub id: String,
    pub name: String,
    pub secret_prefix: String,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub group: Option<String>,
    pub enabled: bool,
}

#[derive(Deserialize)]
pub struct UpdateModelHttpSettingsRequest {
    pub enabled: bool,
    #[serde(default)]
    pub public_model_config_ids: Vec<String>,
}

#[derive(Deserialize)]
pub struct CreateModelHttpApiKeyRequest {
    pub name: String,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub group: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateModelHttpApiKeyRequest {
    pub name: String,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub group: Option<String>,
    pub enabled: bool,
}

#[derive(Serialize)]
pub struct CreateModelHttpApiKeyResponse {
    #[serde(flatten)]
    pub key: ModelHttpApiKeyResponse,
    pub secret: String,
}

fn model_http_key_response(key: &ModelHttpApiKey) -> ModelHttpApiKeyResponse {
    ModelHttpApiKeyResponse {
        id: key.id.clone(),
        name: key.name.clone(),
        secret_prefix: key.secret_prefix.clone(),
        created_at: key.created_at.to_rfc3339_opts(SecondsFormat::Secs, true),
        expires_at: key.expires_at.map(|value| value.to_rfc3339_opts(SecondsFormat::Secs, true)),
        group: key.group.clone(),
        enabled: key.enabled,
    }
}

fn model_http_settings_response(settings: ModelHttpServiceSettings, endpoint: String) -> ModelHttpSettingsResponse {
    ModelHttpSettingsResponse {
        enabled: settings.enabled,
        endpoint,
        public_model_config_ids: settings.public_model_config_ids,
        api_keys: settings.api_keys.iter().map(model_http_key_response).collect(),
    }
}

#[handler]
pub async fn get_model_http_settings(_req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let endpoint = depot.obtain::<String>().cloned().unwrap_or_default();
    match zihuan_core::system_config::load_section::<GlobalSettingsSection>() {
        Ok(settings) => res.render(Json(model_http_settings_response(settings.model_http_service, endpoint))),
        Err(error) => render_settings_error(res, StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    }
}

#[handler]
pub async fn update_model_http_settings(req: &mut Request, res: &mut Response, depot: &mut Depot) {
    let endpoint = depot.obtain::<String>().cloned().unwrap_or_default();
    let body: UpdateModelHttpSettingsRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(error) => return render_settings_error(res, StatusCode::BAD_REQUEST, error.to_string()),
    };
    let llm_refs = match load_llm_refs() {
        Ok(items) => items,
        Err(error) => return render_settings_error(res, StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    };
    let mut public_model_names = std::collections::HashSet::new();
    for config_id in &body.public_model_config_ids {
        let Some(llm_ref) = llm_refs.iter().find(|item| item.id == *config_id && item.enabled) else {
            return render_settings_error(res, StatusCode::BAD_REQUEST, format!("public model '{config_id}' is not an enabled chat model"));
        };
        let ModelRefSpec::ChatLlm { llm } = &llm_ref.model else {
            return render_settings_error(res, StatusCode::BAD_REQUEST, format!("public model '{}' is not a chat model", llm_ref.name));
        };
        if !public_model_names.insert(llm_ref.name.clone()) {
            return render_settings_error(res, StatusCode::BAD_REQUEST, format!("public models must use unique configuration names: {}", llm_ref.name));
        }
    }
    let mut settings = match zihuan_core::system_config::load_section::<GlobalSettingsSection>() {
        Ok(settings) => settings,
        Err(error) => return render_settings_error(res, StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    };
    settings.model_http_service.enabled = body.enabled;
    settings.model_http_service.public_model_config_ids = body.public_model_config_ids;
    match zihuan_core::system_config::save_section::<GlobalSettingsSection>(&settings) {
        Ok(()) => res.render(Json(model_http_settings_response(settings.model_http_service, endpoint))),
        Err(error) => render_settings_error(res, StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    }
}

#[handler]
pub async fn create_model_http_api_key(req: &mut Request, res: &mut Response) {
    let body: CreateModelHttpApiKeyRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(error) => return render_settings_error(res, StatusCode::BAD_REQUEST, error.to_string()),
    };
    if body.name.trim().is_empty() {
        return render_settings_error(res, StatusCode::BAD_REQUEST, "API Key name must not be empty".to_string());
    }
    let mut settings = match zihuan_core::system_config::load_section::<GlobalSettingsSection>() {
        Ok(settings) => settings,
        Err(error) => return render_settings_error(res, StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    };
    let secret = format!("zhk_{}", Uuid::new_v4().simple());
    let secret_hash = hex::encode(Sha256::digest(secret.as_bytes()));
    let key = ModelHttpApiKey {
        id: Uuid::new_v4().to_string(),
        name: body.name.trim().to_string(),
        secret_hash,
        secret_prefix: secret.chars().take(12).collect(),
        created_at: Utc::now(),
        expires_at: body.expires_at,
        group: body.group.filter(|value| !value.trim().is_empty()),
        enabled: true,
    };
    settings.model_http_service.api_keys.push(key.clone());
    match zihuan_core::system_config::save_section::<GlobalSettingsSection>(&settings) {
        Ok(()) => res.render(Json(CreateModelHttpApiKeyResponse { key: model_http_key_response(&key), secret })),
        Err(error) => render_settings_error(res, StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    }
}

#[handler]
pub async fn update_model_http_api_key(req: &mut Request, res: &mut Response) {
    let id = req.param::<String>("id").unwrap_or_default();
    let body: UpdateModelHttpApiKeyRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(error) => return render_settings_error(res, StatusCode::BAD_REQUEST, error.to_string()),
    };
    let mut settings = match zihuan_core::system_config::load_section::<GlobalSettingsSection>() {
        Ok(settings) => settings,
        Err(error) => return render_settings_error(res, StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    };
    let Some(key) = settings.model_http_service.api_keys.iter_mut().find(|key| key.id == id) else {
        return render_settings_error(res, StatusCode::NOT_FOUND, "API Key not found".to_string());
    };
    key.name = body.name.trim().to_string();
    key.expires_at = body.expires_at;
    key.group = body.group.filter(|value| !value.trim().is_empty());
    key.enabled = body.enabled;
    let response = model_http_key_response(key);
    match zihuan_core::system_config::save_section::<GlobalSettingsSection>(&settings) {
        Ok(()) => res.render(Json(response)),
        Err(error) => render_settings_error(res, StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    }
}

#[handler]
pub async fn delete_model_http_api_key(req: &mut Request, res: &mut Response) {
    let id = req.param::<String>("id").unwrap_or_default();
    let mut settings = match zihuan_core::system_config::load_section::<GlobalSettingsSection>() {
        Ok(settings) => settings,
        Err(error) => return render_settings_error(res, StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    };
    let before = settings.model_http_service.api_keys.len();
    settings.model_http_service.api_keys.retain(|key| key.id != id);
    if before == settings.model_http_service.api_keys.len() {
        return render_settings_error(res, StatusCode::NOT_FOUND, "API Key not found".to_string());
    }
    match zihuan_core::system_config::save_section::<GlobalSettingsSection>(&settings) {
        Ok(()) => res.render(Json(serde_json::json!({ "ok": true }))),
        Err(error) => render_settings_error(res, StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    }
}

fn render_settings_error(res: &mut Response, status: StatusCode, error: String) {
    res.status_code(status);
    res.render(Json(serde_json::json!({ "error": error })));
}

async fn python_runtime_response(config: PythonRuntimeConfig) -> PythonRuntimeResponse {
    let workspace_root = match std::env::current_dir() {
        Ok(path) => path,
        Err(error) => {
            return PythonRuntimeResponse {
                config,
                available: false,
                command: None,
                executable_path: None,
                version: None,
                diagnostic: Some(format!("无法获取当前工作目录: {error}")),
            };
        }
    };

    match check_python_runtime(&workspace_root, &config).await {
        Ok((command, version, executable_path)) => PythonRuntimeResponse {
            config,
            available: true,
            command: Some(command.display()),
            executable_path: Some(executable_path),
            version: Some(version),
            diagnostic: None,
        },
        Err(error) => PythonRuntimeResponse {
            config,
            available: false,
            command: None,
            executable_path: None,
            version: None,
            diagnostic: Some(error.to_string()),
        },
    }
}

#[handler]
pub async fn get_python_runtime(_req: &mut Request, res: &mut Response) {
    let config = match ConfigCenter::shared().load_root() {
        Ok(root) => root.python_runtime,
        Err(error) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(Json(serde_json::json!({ "error": error.to_string() })));
            return;
        }
    };
    res.render(Json(python_runtime_response(config).await));
}

#[handler]
pub async fn update_python_runtime(req: &mut Request, res: &mut Response) {
    let config: PythonRuntimeConfig = match req.parse_json().await {
        Ok(config) => config,
        Err(error) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(serde_json::json!({ "error": error.to_string() })));
            return;
        }
    };

    match save_python_runtime(config).await {
        Ok(runtime) => res.render(Json(runtime)),
        Err((status, error)) => {
            res.status_code(status);
            res.render(Json(serde_json::json!({ "error": error })));
        }
    }
}

#[handler]
pub async fn select_python_runtime(_req: &mut Request, res: &mut Response) {
    let path = task::spawn_blocking(|| {
        tinyfiledialogs::open_file_dialog("选择 Python 可执行文件", "", Some((&["*.exe"], "Python executable")))
    })
    .await
    .unwrap_or(None);

    let Some(path) = path else {
        res.render(Json(PythonRuntimeSelectionResponse { cancelled: true, runtime: None }));
        return;
    };

    let config = PythonRuntimeConfig {
        kind: zihuan_core::python_runtime::PythonRuntimeKind::CustomExecutable,
        executable_path: Some(path),
    };
    match save_python_runtime(config).await {
        Ok(runtime) => res.render(Json(PythonRuntimeSelectionResponse {
            cancelled: false,
            runtime: Some(runtime),
        })),
        Err((status, error)) => {
            res.status_code(status);
            res.render(Json(serde_json::json!({ "error": error })));
        }
    }
}

async fn save_python_runtime(
    config: PythonRuntimeConfig,
) -> std::result::Result<PythonRuntimeResponse, (StatusCode, String)> {
    let workspace_root =
        std::env::current_dir().map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    let (command, version, executable_path) = check_python_runtime(&workspace_root, &config)
        .await
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;

    let mut root = ConfigCenter::shared()
        .load_root()
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    root.python_runtime = config;
    ConfigCenter::shared()
        .save_root(&root)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;

    Ok(PythonRuntimeResponse {
        config: root.python_runtime,
        available: true,
        command: Some(command.display()),
        executable_path: Some(executable_path),
        version: Some(version),
        diagnostic: None,
    })
}

fn dir_size(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries
        .filter_map(|e| e.ok())
        .map(|e| {
            let p = e.path();
            if p.is_file() {
                p.metadata().map(|m| m.len()).unwrap_or(0)
            } else if p.is_dir() {
                dir_size(&p)
            } else {
                0
            }
        })
        .sum()
}

fn abs_path_str(path: &Path) -> String {
    let s = path
        .canonicalize()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.display().to_string());
    // Windows canonicalize() prepends \\?\ (extended-length path prefix) — strip it.
    s.strip_prefix(r"\\?\").unwrap_or(&s).to_string()
}

#[handler]
pub async fn get_storage_info(_req: &mut Request, res: &mut Response) {
    let data_dir = zihuan_core::system_config::application_data_dir();

    let chat_history_path = data_dir.join(CHAT_HISTORY_DIR);
    let uploaded_images_path = Path::new(UPLOADED_IMAGES_DIR);

    let storage_entries = vec![
        StorageEntry {
            label: "聊天记录".to_string(),
            path: chat_history_path.display().to_string(),
            exists: chat_history_path.exists(),
        },
        StorageEntry {
            label: "上传的图片".to_string(),
            path: abs_path_str(uploaded_images_path),
            exists: uploaded_images_path.exists(),
        },
    ];

    let te_dir = Path::new(TEXT_EMBEDDING_MODEL_DIR);
    let te_abs = abs_path_str(te_dir);

    let te_models = match std::fs::read_dir(te_dir) {
        Ok(entries) => {
            let mut models: Vec<ModelEntry> = entries
                .filter_map(|e| e.ok())
                .filter_map(|entry| {
                    let path = entry.path();
                    if !path.is_dir() {
                        return None;
                    }
                    let name = path.file_name()?.to_str()?.to_string();
                    let valid = ["config.json", "tokenizer.json", "model.safetensors"]
                        .iter()
                        .all(|f| path.join(f).is_file());
                    let size_bytes = if valid { Some(dir_size(&path)) } else { None };
                    let abs = abs_path_str(&path);
                    Some(ModelEntry {
                        name,
                        path: abs,
                        valid,
                        size_bytes,
                    })
                })
                .collect();
            models.sort_by(|a, b| a.name.cmp(&b.name));
            models
        }
        Err(_) => Vec::new(),
    };

    let model_groups = vec![ModelGroup {
        label: "文本嵌入模型".to_string(),
        dir: te_abs,
        models: te_models,
    }];

    res.render(Json(StorageInfoResponse {
        data_dir: data_dir.display().to_string(),
        storage_entries,
        model_groups,
    }));
}

// ─── Config export / restore ──────────────────────────────────────────────────

fn machine_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}

#[handler]
pub async fn export_config(_req: &mut Request, res: &mut Response) {
    let config_path = zihuan_core::system_config::system_config_file_path();

    let config_bytes = if config_path.exists() {
        match std::fs::read(&config_path) {
            Ok(b) => b,
            Err(e) => {
                res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
                res.render(Json(serde_json::json!({ "error": e.to_string() })));
                return;
            }
        }
    } else {
        // Config not yet written — export an empty default.
        b"{}".to_vec()
    };

    let mut zip_buf = Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut zip_buf);
        let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        if let Err(e) = zip.start_file("system_config.json", options) {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(Json(serde_json::json!({ "error": e.to_string() })));
            return;
        }
        if let Err(e) = zip.write_all(&config_bytes) {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(Json(serde_json::json!({ "error": e.to_string() })));
            return;
        }
        if let Err(e) = zip.finish() {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(Json(serde_json::json!({ "error": e.to_string() })));
            return;
        }
    }

    let machine = machine_name();
    let datetime = Utc::now().format("%Y-%m-%d_%H-%M-%S").to_string();
    // Sanitize machine name for use in a filename.
    let safe_machine: String = machine
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '_' })
        .collect();
    let filename = format!("zihuan-config_{safe_machine}_{datetime}.zip");

    res.add_header("Content-Type", "application/zip", true).ok();
    res.add_header("Content-Disposition", format!("attachment; filename=\"{filename}\""), true)
        .ok();
    res.write_body(zip_buf.into_inner()).ok();
}

const RESTORE_MAX_BYTES: usize = 50 * 1024 * 1024; // 50 MB

#[handler]
pub async fn restore_config(req: &mut Request, res: &mut Response) {
    let bytes = match req.payload_with_max_size(RESTORE_MAX_BYTES).await {
        Ok(b) => b.clone(),
        Err(e) => {
            res.status_code(StatusCode::PAYLOAD_TOO_LARGE);
            res.render(Json(serde_json::json!({ "error": e.to_string() })));
            return;
        }
    };

    if bytes.is_empty() {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(serde_json::json!({ "error": "empty request body" })));
        return;
    }

    let cursor = Cursor::new(&bytes[..]);
    let mut archive = match ZipArchive::new(cursor) {
        Ok(a) => a,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(serde_json::json!({ "error": format!("invalid zip: {e}") })));
            return;
        }
    };

    let config_bytes = match archive.by_name("system_config.json") {
        Ok(mut entry) => {
            let mut buf = Vec::new();
            if let Err(e) = std::io::Read::read_to_end(&mut entry, &mut buf) {
                res.status_code(StatusCode::BAD_REQUEST);
                res.render(Json(serde_json::json!({ "error": format!("failed to read zip entry: {e}") })));
                return;
            }
            buf
        }
        Err(_) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(serde_json::json!({
                "error": "zip does not contain system_config.json"
            })));
            return;
        }
    };

    let root: serde_json::Value = match serde_json::from_slice(&config_bytes) {
        Ok(v) => v,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(
                serde_json::json!({ "error": format!("system_config.json is not valid JSON: {e}") }),
            ));
            return;
        }
    };

    // Backup the current config before overwriting.
    let config_path = zihuan_core::system_config::system_config_file_path();
    if config_path.exists() {
        let bak_path = config_path.with_extension("json.bak");
        let _ = std::fs::copy(&config_path, &bak_path);
    }

    if let Err(e) = zihuan_core::system_config::save_system_config_root(&root) {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(serde_json::json!({ "error": format!("failed to save config: {e}") })));
        return;
    }

    res.render(Json(serde_json::json!({ "ok": true })));
}
