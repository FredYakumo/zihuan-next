use std::io::{Cursor, Write};
use std::path::Path;

use chrono::Utc;
use salvo::http::StatusCode;
use salvo::prelude::*;
use salvo::writing::Json;
use serde::Serialize;
use tokio::task;
use zihuan_core::config::ConfigCenter;
use zihuan_core::python_runtime::PythonRuntimeConfig;

use zihuan_service::python_runtime::check_python_runtime;
use zip::write::SimpleFileOptions;
use zip::ZipArchive;

const APP_DIR_NAME: &str = "zihuan-next_aibot";
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

fn python_runtime_response(config: PythonRuntimeConfig) -> PythonRuntimeResponse {
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

    match check_python_runtime(&workspace_root, &config) {
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
    res.render(Json(python_runtime_response(config)));
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

    match save_python_runtime(config) {
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
    match save_python_runtime(config) {
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

fn save_python_runtime(
    config: PythonRuntimeConfig,
) -> std::result::Result<PythonRuntimeResponse, (StatusCode, String)> {
    let workspace_root =
        std::env::current_dir().map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    check_python_runtime(&workspace_root, &config).map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;

    let mut root = ConfigCenter::shared()
        .load_root()
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    root.python_runtime = config;
    ConfigCenter::shared()
        .save_root(&root)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;

    Ok(python_runtime_response(root.python_runtime))
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
    let data_dir = zihuan_core::system_config::app_data_dir().join(APP_DIR_NAME);

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
