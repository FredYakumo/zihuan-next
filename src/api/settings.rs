use std::path::Path;

use salvo::prelude::*;
use salvo::writing::Json;
use serde::Serialize;

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
    path.canonicalize()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
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
