use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::storage::{
    is_memory_expired, AgentMemoryRecord, AgentMemorySearchHit, AgentMemoryUpsert,
};
use crate::system_config::application_data_dir;

#[derive(Debug, Default, Deserialize, Serialize)]
struct LocalMemoryMetadata {
    #[serde(flatten)]
    input: AgentMemoryUpsert,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    updated_at: String,
}

#[derive(Debug)]
pub struct LocalMemoryStore {
    directory: PathBuf,
    write_lock: Mutex<()>,
}

impl LocalMemoryStore {
    pub fn in_app_data_dir() -> Self {
        Self::new(application_data_dir().join("memory"))
    }
    pub fn new(directory: PathBuf) -> Self {
        Self { directory, write_lock: Mutex::new(()) }
    }

    pub fn create_or_update(&self, input: &AgentMemoryUpsert) -> Result<AgentMemoryRecord> {
        let normalized = super::memory_types::normalize_memory_input(input)?;
        let input = &normalized;
        validate_memory_key(&input.key)?;
        validate_expires_at(input.expires_at.as_deref())?;
        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| crate::string_error!("local memory write lock poisoned"))?;
        let category = input.attributes.category;
        let directory = self.directory.join(category.schema().as_str());
        fs::create_dir_all(&directory)?;
        let object_id = super::memory_object_id(input);
        let path = directory.join(format!("{object_id}.md"));
        let meta_path = path.with_extension("meta.json");
        let existing = fs::read_to_string(&meta_path)
            .ok()
            .and_then(|text| serde_json::from_str::<LocalMemoryMetadata>(&text).ok());
        let now = Utc::now().to_rfc3339();
        let metadata = LocalMemoryMetadata {
            input: input.clone(),
            created_at: existing.map(|meta| meta.created_at).unwrap_or_else(|| now.clone()),
            updated_at: now,
        };
        let temporary = path.with_extension("md.tmp");
        let meta_temporary = path.with_extension("meta.json.tmp");
        fs::write(&temporary, input.value.as_bytes())?;
        fs::write(&meta_temporary, serde_json::to_vec(&metadata)?)?;
        fs::rename(&temporary, &path)?;
        fs::rename(&meta_temporary, &meta_path)?;
        record_from_path(&path, &object_id, input.value.clone())
    }

    pub fn migrate_legacy(&self) -> Result<()> {
        static MIGRATION_LOCK: Mutex<()> = Mutex::new(());
        let _guard = MIGRATION_LOCK
            .lock()
            .map_err(|_| crate::string_error!("local memory migration lock poisoned"))?;
        if !self.directory.exists() {
            return Ok(());
        }
        for entry in fs::read_dir(&self.directory)? {
            let path = entry?.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
                continue;
            }
            let Some(key) = path.file_stem().and_then(|key| key.to_str()) else {
                continue;
            };
            let input = super::memory_migration::converted_legacy_record(record_from_path(
                &path,
                key,
                fs::read_to_string(&path)?,
            )?);
            if self.get(&input)?.is_none() {
                self.create_or_update(&input)?;
            }
            let archive = self.directory.join("legacy_archive");
            fs::create_dir_all(&archive)?;
            fs::rename(&path, archive.join(path.file_name().unwrap()))?;
            let meta = path.with_extension("meta.json");
            if meta.exists() {
                fs::rename(&meta, archive.join(meta.file_name().unwrap()))?;
            }
        }
        Ok(())
    }

    pub fn get(&self, input: &AgentMemoryUpsert) -> Result<Option<AgentMemoryRecord>> {
        let id = super::memory_object_id(input);
        let path = self
            .directory
            .join(input.attributes.category.schema().as_str())
            .join(format!("{id}.md"));
        match fs::read_to_string(&path) {
            Ok(value) => record_from_path(&path, &id, value).map(Some),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub fn list(&self, query: Option<&str>, top_n: usize) -> Result<Vec<AgentMemorySearchHit>> {
        let mut hits = Vec::new();
        for category in super::MemoryCategory::ALL {
            hits.extend(self.list_category(category, query, top_n)?);
        }
        hits.sort_by(|a, b| b.record.updated_at.cmp(&a.record.updated_at));
        hits.truncate(top_n);
        Ok(hits)
    }

    pub fn list_category(
        &self,
        category: super::MemoryCategory,
        query: Option<&str>,
        top_n: usize,
    ) -> Result<Vec<AgentMemorySearchHit>> {
        let mut directories = vec![self.directory.join(category.schema().as_str())];
        // Read legacy markdown in place until it has been explicitly rewritten.
        if category == super::MemoryCategory::Concept {
            directories.push(self.directory.clone());
        }
        let query = query.map(normalize_search_query).filter(|query| !query.is_empty());
        let mut records = Vec::new();
        for directory in directories {
            if !directory.exists() {
                continue;
            }
            for entry in fs::read_dir(directory)? {
                let path = entry?.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
                    continue;
                }
                let Some(key) = path.file_stem().and_then(|name| name.to_str()) else {
                    continue;
                };
                let value = fs::read_to_string(&path)?;
                let record = record_from_path(&path, key, value)?;
                if is_memory_expired(&record) {
                    continue;
                }
                let searchable = format!("{} {}", record.key, serde_json::to_string(&record)?);
                if query
                    .as_ref()
                    .is_some_and(|query| !matches_query(&record.key, &searchable, query))
                {
                    continue;
                }
                records.push(AgentMemorySearchHit { record, distance: None });
            }
        }
        records.sort_by(|a, b| {
            b.record
                .attributes
                .starts_at
                .as_ref()
                .unwrap_or(&b.record.updated_at)
                .cmp(a.record.attributes.starts_at.as_ref().unwrap_or(&a.record.updated_at))
        });
        records.truncate(top_n);
        Ok(records)
    }
}

fn validate_memory_key(key: &str) -> Result<&str> {
    let key = key.trim();
    if key.is_empty()
        || key == "."
        || key == ".."
        || key.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|'])
        || key.ends_with('.')
        || key.ends_with(' ')
    {
        return Err(Error::ValidationError("memory key must be a safe, descriptive file name without path separators or reserved characters".to_string()));
    }
    let reserved = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if reserved.iter().any(|name| key.eq_ignore_ascii_case(name)) {
        return Err(Error::ValidationError(
            "memory key must not use a reserved file name".to_string(),
        ));
    }
    Ok(key)
}

fn record_from_path(path: &Path, key: &str, value: String) -> Result<AgentMemoryRecord> {
    let file_metadata = fs::metadata(path)?;
    let modified = system_time_to_rfc3339(file_metadata.modified().ok())
        .unwrap_or_else(|| Utc::now().to_rfc3339());
    let metadata = match fs::read_to_string(path.with_extension("meta.json")) {
        Ok(text) => serde_json::from_str::<LocalMemoryMetadata>(&text)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            LocalMemoryMetadata::default()
        }
        Err(error) => return Err(error.into()),
    };
    let input = metadata.input;
    Ok(AgentMemoryRecord {
        attributes: input.attributes,
        object_id: key.to_string(),
        key: if input.key.is_empty() {
            key.to_string()
        } else {
            input.key
        },
        value,
        expires_at: input.expires_at,
        sender_id_list: input.sender_id_list,
        group_id_list: input.group_id_list,
        created_at: if metadata.created_at.is_empty() {
            modified.clone()
        } else {
            metadata.created_at
        },
        updated_at: if metadata.updated_at.is_empty() {
            modified
        } else {
            metadata.updated_at
        },
    })
}

fn validate_expires_at(expires_at: Option<&str>) -> Result<()> {
    let Some(expires_at) = expires_at else {
        return Ok(());
    };
    DateTime::parse_from_rfc3339(expires_at).map(|_| ()).map_err(|error| {
        Error::ValidationError(format!("invalid expires_at '{expires_at}': {error}"))
    })
}

fn system_time_to_rfc3339(value: Option<std::time::SystemTime>) -> Option<String> {
    value.map(|value| DateTime::<Utc>::from(value).to_rfc3339())
}

fn normalize_search_query(query: &str) -> String {
    query.trim().to_lowercase()
}
fn matches_query(key: &str, value: &str, query: &str) -> bool {
    key.to_lowercase().contains(query) || value.to_lowercase().contains(query)
}
