use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use log::warn;
use serde_json::Value;

use crate::error::Result;
use zihuan_node::graph_io::NodeGraphDefinition;

/// Returns the central directory that stores hyperparameter value files.
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

fn shared_hyperparam_yaml_path() -> Option<PathBuf> {
    Some(hp_data_dir()?.join("shared_hyperparameters.yaml"))
}

/// Derive the legacy per-graph YAML filename for a given graph file path.
fn legacy_hyperparam_yaml_path(graph_path: &Path) -> Option<PathBuf> {
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

fn load_yaml_map(yaml_path: &Path) -> HashMap<String, Value> {
    let content = match fs::read_to_string(yaml_path) {
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

fn save_yaml_map(yaml_path: &Path, values: &HashMap<String, Value>) -> Result<()> {
    if let Some(parent) = yaml_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let yaml_map: HashMap<String, serde_yaml::Value> = values
        .iter()
        .filter_map(|(k, v)| json_to_yaml_value(v).map(|yv| (k.clone(), yv)))
        .collect();

    let content = serde_yaml::to_string(&yaml_map)
        .map_err(|e| crate::error::Error::StringError(format!("yaml serialize error: {e}")))?;
    fs::write(yaml_path, content)?;
    Ok(())
}

fn global_store_key(group: &str, name: &str) -> String {
    format!("{group}::{name}")
}

fn normalize_group(group: &str) -> String {
    let trimmed = group.trim();
    if trimmed.is_empty() {
        "default".to_string()
    } else {
        trimmed.to_string()
    }
}

fn migrate_legacy_values_into_shared_store(
    graph_path: &Path,
    graph: &NodeGraphDefinition,
    shared_store: &mut HashMap<String, Value>,
) -> bool {
    let dir = match hp_data_dir() {
        Some(dir) => dir,
        None => return false,
    };

    let mut changed = false;

    if let Some(legacy_path) = legacy_hyperparam_yaml_path(graph_path) {
        let legacy_values = load_yaml_map(&legacy_path);
        for hp in &graph.hyperparameters {
            let key = global_store_key(&normalize_group(&hp.group), &hp.name);
            if shared_store.contains_key(&key) {
                continue;
            }
            if let Some(value) = legacy_values.get(&hp.name) {
                shared_store.insert(key, value.clone());
                changed = true;
            }
        }
    }

    let legacy_files: Vec<(PathBuf, std::time::SystemTime)> = match fs::read_dir(&dir) {
        Ok(entries) => entries
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("yaml") {
                    return None;
                }
                if path.file_name().and_then(|name| name.to_str())
                    == Some("shared_hyperparameters.yaml")
                {
                    return None;
                }
                let modified = entry
                    .metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                Some((path, modified))
            })
            .collect(),
        Err(_) => return changed,
    };

    for hp in &graph.hyperparameters {
        let key = global_store_key(&normalize_group(&hp.group), &hp.name);
        if shared_store.contains_key(&key) {
            continue;
        }

        let mut best_match: Option<(std::time::SystemTime, Value)> = None;
        for (path, modified) in &legacy_files {
            let legacy_values = load_yaml_map(&path);
            let Some(value) = legacy_values.get(&hp.name).cloned() else {
                continue;
            };

            let replace = match &best_match {
                Some((best_modified, _)) => *modified > *best_modified,
                None => true,
            };
            if replace {
                best_match = Some((*modified, value));
            }
        }

        if let Some((_, value)) = best_match {
            shared_store.insert(key, value);
            changed = true;
        }
    }

    changed
}

/// Load hyperparameter values for the current graph from the global shared YAML store.
///
/// Values are shared across graphs by `(group, name)`. Legacy per-graph YAML files are
/// imported automatically when matching values are found.
pub fn load_hyperparameter_values(
    graph_path: &Path,
    graph: &NodeGraphDefinition,
) -> HashMap<String, Value> {
    let shared_path = match shared_hyperparam_yaml_path() {
        Some(p) => p,
        None => return HashMap::new(),
    };

    let mut shared_store = load_yaml_map(&shared_path);
    let migrated = migrate_legacy_values_into_shared_store(graph_path, graph, &mut shared_store);
    if migrated {
        if let Err(e) = save_yaml_map(&shared_path, &shared_store) {
            warn!(
                "[HyperParamStore] Failed to persist shared store migration: {}",
                e
            );
        }
    }

    graph
        .hyperparameters
        .iter()
        .filter_map(|hp| {
            shared_store
                .get(&global_store_key(&normalize_group(&hp.group), &hp.name))
                .cloned()
                .map(|value| (hp.name.clone(), value))
        })
        .collect()
}

/// Persist hyperparameter values for the current graph into the global shared YAML store.
pub fn save_hyperparameter_values(
    graph_path: &Path,
    graph: &NodeGraphDefinition,
    values: &HashMap<String, Value>,
) -> Result<()> {
    let yaml_path = match shared_hyperparam_yaml_path() {
        Some(p) => p,
        None => return Ok(()), // nowhere to save, silently ok
    };

    let mut shared_store = load_yaml_map(&yaml_path);
    if migrate_legacy_values_into_shared_store(graph_path, graph, &mut shared_store) {
        // keep imported values before applying the latest graph-local updates
    }

    for hp in &graph.hyperparameters {
        let key = global_store_key(&normalize_group(&hp.group), &hp.name);
        if let Some(value) = values.get(&hp.name) {
            shared_store.insert(key, value.clone());
        } else {
            shared_store.remove(&key);
        }
    }

    save_yaml_map(&yaml_path, &shared_store)
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
