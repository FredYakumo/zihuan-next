use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use log::warn;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::storage::{is_memory_expired, AgentMemoryRecord, AgentMemorySearchHit, AgentMemoryUpsert};
use crate::system_config::app_data_dir;

#[derive(Debug, Default, Deserialize, Serialize)]
struct LocalMemoryMetadata {
    #[serde(default)]
    expires_at: Option<String>,
}

#[derive(Debug)]
pub struct LocalMemoryStore {
    directory: PathBuf,
    write_lock: Mutex<()>,
}

impl LocalMemoryStore {
    pub fn in_app_data_dir() -> Self {
        Self::new(app_data_dir().join("zihuan-next_aibot").join("memory"))
    }

    pub fn new(directory: PathBuf) -> Self {
        Self { directory, write_lock: Mutex::new(()) }
    }

    pub fn create_or_update(&self, input: &AgentMemoryUpsert) -> Result<AgentMemoryRecord> {
        let key = validate_memory_key(&input.key)?;
        validate_expires_at(input.expires_at.as_deref())?;
        let _guard = self.write_lock.lock().map_err(|_| crate::string_error!("local memory write lock poisoned"))?;
        fs::create_dir_all(&self.directory)?;
        let path = self.path_for_key(key);
        let metadata_path = self.metadata_path_for_key(key);
        let temporary = path.with_extension("md.tmp");
        let metadata_temporary = metadata_path.with_extension("meta.json.tmp");
        fs::write(&temporary, input.value.as_bytes())?;
        fs::write(&metadata_temporary, serde_json::to_vec(&LocalMemoryMetadata { expires_at: input.expires_at.clone() })?)?;
        fs::rename(&temporary, &path)?;
        fs::rename(&metadata_temporary, &metadata_path)?;
        record_from_path(&path, key, input.value.clone())
    }

    pub fn list(&self, query: Option<&str>, top_n: usize) -> Result<Vec<AgentMemorySearchHit>> {
        if !self.directory.exists() {
            return Ok(Vec::new());
        }
        let query = query.map(normalize_search_query).filter(|value| !value.is_empty());
        let mut records = Vec::new();
        for entry in fs::read_dir(&self.directory)? {
            let entry = match entry { Ok(entry) => entry, Err(_) => continue };
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("md") {
                continue;
            }
            let Some(key) = path.file_stem().and_then(|stem| stem.to_str()) else { continue };
            let value = match fs::read_to_string(&path) { Ok(value) => value, Err(_) => continue };
            if query.as_ref().is_some_and(|query| !matches_query(key, &value, query)) {
                continue;
            }
            if let Ok(record) = record_from_path(&path, key, value) {
                if is_memory_expired(&record) {
                    remove_expired_memory(&path);
                    continue;
                }
                records.push(AgentMemorySearchHit { record, distance: None });
            }
        }
        records.sort_by(|left, right| right.record.updated_at.cmp(&left.record.updated_at));
        records.truncate(top_n);
        Ok(records)
    }

    fn path_for_key(&self, key: &str) -> PathBuf { self.directory.join(format!("{key}.md")) }

    fn metadata_path_for_key(&self, key: &str) -> PathBuf { self.directory.join(format!("{key}.meta.json")) }
}

fn validate_memory_key(key: &str) -> Result<&str> {
    let key = key.trim();
    if key.is_empty() || key == "." || key == ".." || key.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) || key.ends_with('.') || key.ends_with(' ') {
        return Err(Error::ValidationError("memory key must be a safe, descriptive file name without path separators or reserved characters".to_string()));
    }
    let reserved = ["CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9"];
    if reserved.iter().any(|name| key.eq_ignore_ascii_case(name)) {
        return Err(Error::ValidationError("memory key must not use a reserved file name".to_string()));
    }
    Ok(key)
}

fn record_from_path(path: &Path, key: &str, value: String) -> Result<AgentMemoryRecord> {
    let metadata = fs::metadata(path)?;
    let updated_at = system_time_to_rfc3339(metadata.modified().ok()).unwrap_or_else(|| Utc::now().to_rfc3339());
    let created_at = system_time_to_rfc3339(metadata.created().ok()).unwrap_or_else(|| updated_at.clone());
    let expires_at = read_expires_at(path);
    Ok(AgentMemoryRecord { object_id: key.to_string(), key: key.to_string(), value, expires_at, sender_id_list: Vec::new(), group_id_list: Vec::new(), created_at, updated_at })
}

fn read_expires_at(path: &Path) -> Option<String> {
    let metadata_path = path.with_extension("meta.json");
    let content = match fs::read_to_string(&metadata_path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => {
            warn!("Failed to read local memory metadata {}: {error}", metadata_path.display());
            return None;
        }
    };
    let metadata = match serde_json::from_str::<LocalMemoryMetadata>(&content) {
        Ok(metadata) => metadata,
        Err(error) => {
            warn!("Failed to parse local memory metadata {}: {error}", metadata_path.display());
            return None;
        }
    };
    if let Err(error) = validate_expires_at(metadata.expires_at.as_deref()) {
        warn!("Ignoring invalid local memory expiry in {}: {error}", metadata_path.display());
        return None;
    }
    metadata.expires_at
}

fn validate_expires_at(expires_at: Option<&str>) -> Result<()> {
    let Some(expires_at) = expires_at else {
        return Ok(());
    };
    DateTime::parse_from_rfc3339(expires_at)
        .map(|_| ())
        .map_err(|error| Error::ValidationError(format!("invalid expires_at '{expires_at}': {error}")))
}

fn remove_expired_memory(path: &Path) {
    if let Err(error) = fs::remove_file(path) {
        warn!("Failed to remove expired local memory {}: {error}", path.display());
        return;
    }

    let metadata_path = path.with_extension("meta.json");
    if let Err(error) = fs::remove_file(&metadata_path) {
        if error.kind() != std::io::ErrorKind::NotFound {
            warn!("Failed to remove expired local memory metadata {}: {error}", metadata_path.display());
        }
    }
}

fn system_time_to_rfc3339(value: Option<std::time::SystemTime>) -> Option<String> {
    value.map(|value| DateTime::<Utc>::from(value).to_rfc3339())
}

fn normalize_search_query(query: &str) -> String { query.trim().to_lowercase() }
fn matches_query(key: &str, value: &str, query: &str) -> bool { key.to_lowercase().contains(query) || value.to_lowercase().contains(query) }
