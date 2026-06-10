use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use zihuan_core::error::{Error, Result};

pub const LOCAL_LLM_MODEL_ROOT: &str = "models/llm";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalLlmModelKind {
    Text,
    VisionLanguage,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocalLlmModelLayout {
    Gguf,
    Hf,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalLlmModelInfo {
    pub model_name: String,
    pub kind: LocalLlmModelKind,
    pub layout: LocalLlmModelLayout,
    pub available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight_file: Option<String>,
    pub supports_multimodal_input: bool,
}

pub fn list_local_llm_models() -> Vec<LocalLlmModelInfo> {
    let root = Path::new(LOCAL_LLM_MODEL_ROOT);
    let mut models = match fs::read_dir(root) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .filter_map(|path| inspect_model_dir(&path).ok())
            .collect::<Vec<_>>(),
        Err(_) => Vec::new(),
    };
    models.sort_by(|left, right| left.model_name.cmp(&right.model_name));
    models
}

pub fn resolve_model_dir(model_name: &str) -> Result<PathBuf> {
    let trimmed = model_name.trim();
    if trimmed.is_empty() {
        return Err(Error::ValidationError(
            "model_name is required for local llm models".to_string(),
        ));
    }
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains("..") {
        return Err(Error::ValidationError(format!(
            "model_name must be a direct child directory under {LOCAL_LLM_MODEL_ROOT}"
        )));
    }

    let model_dir = Path::new(LOCAL_LLM_MODEL_ROOT).join(trimmed);
    if !model_dir.is_dir() {
        return Err(Error::ValidationError(format!(
            "local llm model '{}' was not found under {}",
            trimmed, LOCAL_LLM_MODEL_ROOT
        )));
    }
    Ok(model_dir)
}

pub fn get_local_llm_model_info(model_name: &str) -> Result<LocalLlmModelInfo> {
    let model_dir = resolve_model_dir(model_name)?;
    inspect_model_dir(&model_dir)
}

fn inspect_model_dir(model_dir: &Path) -> Result<LocalLlmModelInfo> {
    let model_name = model_dir
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| Error::StringError(format!("invalid model directory '{}'", model_dir.display())))?
        .to_string();
    let tokenizer_present = model_dir.join("tokenizer.json").is_file();
    let gguf_weight = find_first_weight_file(model_dir, "gguf");
    let safetensors_weight = find_first_weight_file(model_dir, "safetensors");
    let config_path = model_dir.join("config.json");

    if !tokenizer_present {
        return Ok(LocalLlmModelInfo {
            model_name,
            kind: LocalLlmModelKind::Text,
            layout: LocalLlmModelLayout::Unknown,
            available: false,
            reason: Some("missing tokenizer.json".to_string()),
            weight_file: None,
            supports_multimodal_input: false,
        });
    }

    if let Some(gguf_weight) = gguf_weight {
        return Ok(LocalLlmModelInfo {
            model_name,
            kind: LocalLlmModelKind::Text,
            layout: LocalLlmModelLayout::Gguf,
            available: true,
            reason: None,
            weight_file: Some(file_name_string(&gguf_weight)),
            supports_multimodal_input: false,
        });
    }

    if !config_path.is_file() {
        return Ok(LocalLlmModelInfo {
            model_name,
            kind: LocalLlmModelKind::Text,
            layout: LocalLlmModelLayout::Unknown,
            available: false,
            reason: Some("missing config.json".to_string()),
            weight_file: None,
            supports_multimodal_input: false,
        });
    }

    let config_text = fs::read_to_string(&config_path).map_err(|err| {
        Error::StringError(format!(
            "failed to read local llm config '{}' for '{}': {}",
            config_path.display(),
            model_name,
            err
        ))
    })?;
    let config_json = serde_json::from_str::<Value>(&config_text).map_err(|err| {
        Error::StringError(format!(
            "failed to parse local llm config '{}' for '{}': {}",
            config_path.display(),
            model_name,
            err
        ))
    })?;

    let is_vl = config_json.get("vision_config").is_some()
        && config_json.get("image_token_id").is_some()
        && config_json.get("vision_start_token_id").is_some()
        && config_json.get("vision_end_token_id").is_some();
    let kind = if is_vl {
        LocalLlmModelKind::VisionLanguage
    } else {
        LocalLlmModelKind::Text
    };
    let supports_multimodal_input = matches!(kind, LocalLlmModelKind::VisionLanguage);
    let weight_file = safetensors_weight.as_ref().map(|path| file_name_string(path.as_path()));

    if weight_file.is_none() {
        return Ok(LocalLlmModelInfo {
            model_name,
            kind,
            layout: LocalLlmModelLayout::Hf,
            available: false,
            reason: Some("missing *.safetensors weight file".to_string()),
            weight_file: None,
            supports_multimodal_input,
        });
    }

    let (available, reason) = match kind {
        LocalLlmModelKind::Text => (
            false,
            Some("HF text runtime is not implemented yet; use api_style=candle_gguf with a GGUF model".to_string()),
        ),
        LocalLlmModelKind::VisionLanguage => (
            false,
            Some("Qwen3-VL local runtime is not implemented yet; model is discoverable but unavailable".to_string()),
        ),
    };

    Ok(LocalLlmModelInfo {
        model_name,
        kind,
        layout: LocalLlmModelLayout::Hf,
        available,
        reason,
        weight_file,
        supports_multimodal_input,
    })
}

fn file_name_string(path: &Path) -> String {
    path.file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string()
}

fn find_first_weight_file(model_dir: &Path, extension: &str) -> Option<PathBuf> {
    let mut weights = fs::read_dir(model_dir)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| {
            path.extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case(extension))
        })
        .collect::<Vec<_>>();
    weights.sort();
    weights.into_iter().next()
}
