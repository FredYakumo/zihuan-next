use std::io::{Cursor, Write};
use std::path::Path;

use chrono::Utc;
use salvo::http::StatusCode;
use salvo::prelude::*;
use salvo::writing::Json;
use serde::Serialize;
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
        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
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
    res.add_header(
        "Content-Disposition",
        format!("attachment; filename=\"{filename}\""),
        true,
    )
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
            res.render(Json(
                serde_json::json!({ "error": format!("invalid zip: {e}") }),
            ));
            return;
        }
    };

    let config_bytes = match archive.by_name("system_config.json") {
        Ok(mut entry) => {
            let mut buf = Vec::new();
            if let Err(e) = std::io::Read::read_to_end(&mut entry, &mut buf) {
                res.status_code(StatusCode::BAD_REQUEST);
                res.render(Json(
                    serde_json::json!({ "error": format!("failed to read zip entry: {e}") }),
                ));
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
        res.render(Json(
            serde_json::json!({ "error": format!("failed to save config: {e}") }),
        ));
        return;
    }

    res.render(Json(serde_json::json!({ "ok": true })));
}
