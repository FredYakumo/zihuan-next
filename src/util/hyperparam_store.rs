use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use log::warn;
use serde_json::Value;

use crate::error::Result;

/// Returns the central directory that stores per-graph hyperparameter value files.
///
/// Windows : `%APPDATA%\zihuan-next_aibot\hyperparams\`
/// Linux/macOS : `$XDG_CONFIG_HOME/zihuan-next_aibot/hyperparams/` (or `~/.config/…`)
fn hp_data_dir() -> Option<PathBuf> {
    let base = if cfg!(target_os = "windows") {
        std::env::var("APPDATA")
            .or_else(|_| std::env::var("LOCALAPPDATA"))
            .ok()
            .map(PathBuf::from)
    } else {
        std::env::var("XDG_CONFIG_HOME")
            .ok()
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var("HOME")
                    .ok()
                    .map(|h| PathBuf::from(h).join(".config"))
            })
    }?;

    Some(base.join("zihuan-next_aibot").join("hyperparams"))
}

/// Derive a stable YAML filename for a given graph file path.
///
/// We use the file stem plus an 8-hex-char hash of the canonicalized path so
/// that two graphs with the same stem but different directories don't collide.
fn hyperparam_yaml_path(graph_path: &Path) -> Option<PathBuf> {
    let dir = hp_data_dir()?;

    let stem = graph_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "graph".to_string());

    // Use the canonical path if available, fall back to the raw path string.
    let canonical_str = graph_path
        .canonicalize()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| graph_path.to_string_lossy().to_string());

    let hash = simple_hash8(&canonical_str);
    let filename = format!("{}_{}.yaml", stem, hash);
    Some(dir.join(filename))
}

/// Simple deterministic 32-bit hash encoded as 8 lowercase hex chars,
/// based on FNV-1a (no external dependency needed).
fn simple_hash8(s: &str) -> String {
    let mut h: u32 = 0x811c9dc5;
    for b in s.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x01000193);
    }
    format!("{:08x}", h)
}

/// Load hyperparameter values for a given graph file from the central YAML store.
///
/// Returns an empty map when the YAML file does not yet exist or cannot be parsed.
pub fn load_hyperparameter_values(graph_path: &Path) -> HashMap<String, Value> {
    let yaml_path = match hyperparam_yaml_path(graph_path) {
        Some(p) => p,
        None => return HashMap::new(),
    };

    let content = match fs::read_to_string(&yaml_path) {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };

    match serde_yaml::from_str::<HashMap<String, serde_yaml::Value>>(&content) {
        Ok(map) => map
            .into_iter()
            .filter_map(|(k, v)| yaml_to_json_value(v).map(|jv| (k, jv)))
            .collect(),
        Err(e) => {
            warn!(
                "[HyperParamStore] Failed to parse {}: {}",
                yaml_path.display(),
                e
            );
            HashMap::new()
        }
    }
}

/// Persist hyperparameter values for a given graph file to the central YAML store.
pub fn save_hyperparameter_values(
    graph_path: &Path,
    values: &HashMap<String, Value>,
) -> Result<()> {
    let yaml_path = match hyperparam_yaml_path(graph_path) {
        Some(p) => p,
        None => return Ok(()), // nowhere to save, silently ok
    };

    if let Some(parent) = yaml_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let yaml_map: HashMap<String, serde_yaml::Value> = values
        .iter()
        .filter_map(|(k, v)| json_to_yaml_value(v).map(|yv| (k.clone(), yv)))
        .collect();

    let content = serde_yaml::to_string(&yaml_map)
        .map_err(|e| crate::error::Error::StringError(format!("yaml serialize error: {e}")))?;
    fs::write(&yaml_path, content)?;
    Ok(())
}

fn yaml_to_json_value(v: serde_yaml::Value) -> Option<Value> {
    match v {
        serde_yaml::Value::Null => None,
        serde_yaml::Value::Bool(b) => Some(Value::Bool(b)),
        serde_yaml::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(Value::Number(i.into()))
            } else if let Some(f) = n.as_f64() {
                serde_json::Number::from_f64(f).map(Value::Number)
            } else {
                None
            }
        }
        serde_yaml::Value::String(s) => Some(Value::String(s)),
        _ => None, // arrays/maps not supported as hp values
    }
}

fn json_to_yaml_value(v: &Value) -> Option<serde_yaml::Value> {
    match v {
        Value::Bool(b) => Some(serde_yaml::Value::Bool(*b)),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(serde_yaml::Value::Number(i.into()))
            } else if let Some(f) = n.as_f64() {
                // serde_yaml accepts f64 directly via From<f64>
                Some(serde_yaml::Value::Number(serde_yaml::Number::from(f)))
            } else {
                None
            }
        }
        Value::String(s) => Some(serde_yaml::Value::String(s.clone())),
        _ => None,
    }
}
