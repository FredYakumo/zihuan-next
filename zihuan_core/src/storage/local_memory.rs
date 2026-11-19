use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::Utc;
use log::warn;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::storage::memory::{
    BehavioralMemoryPrepared, BehavioralMemoryRecord, ConceptMemoryPrepared, ConceptMemoryRecord,
    MemoryRecord, MemorySearchHit, PersonMemoryRecord, PersonMemoryWrite,
};
use crate::storage::{is_memory_expired, memory_object_id, MemoryCategory, PersonMemory};
use crate::system_config::application_data_dir;

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

    fn category_directory(&self, category: MemoryCategory) -> PathBuf {
        self.directory.join(category.schema().as_str())
    }

    fn record_paths(&self, category: MemoryCategory, object_id: &str) -> (PathBuf, PathBuf) {
        let path = self.category_directory(category).join(format!("{object_id}.md"));
        let meta_path = path.with_extension("meta.json");
        (path, meta_path)
    }

    /// Writes one concept record; an existing record under the same identity keeps
    /// its creation time.
    pub(crate) fn save_concept(
        &self,
        prepared: &ConceptMemoryPrepared,
    ) -> Result<ConceptMemoryRecord> {
        let _guard = self.lock()?;
        let (path, meta_path) = self.record_paths(MemoryCategory::Concept, &prepared.object_id);
        fs::create_dir_all(path.parent().expect("category directory has a parent"))?;
        let existing_meta: LocalConceptMeta = read_meta(&meta_path)?;
        let now = Utc::now().to_rfc3339();
        let meta = LocalConceptMeta {
            key: prepared.key.clone(),
            access_level: prepared.access_level,
            starts_at: prepared.starts_at.clone(),
            sender_id_list: prepared.sender_id_list.clone(),
            group_id_list: prepared.group_id_list.clone(),
            created_at: non_empty_or(existing_meta.created_at, &now),
            updated_at: now,
        };
        write_record_files(&path, &meta_path, &prepared.content, &meta)?;
        Ok(ConceptMemoryRecord {
            key: meta.key,
            content: prepared.content.clone(),
            access_level: meta.access_level,
            starts_at: meta.starts_at,
            sender_id_list: meta.sender_id_list,
            group_id_list: meta.group_id_list,
            object_id: prepared.object_id.clone(),
            created_at: meta.created_at,
            updated_at: meta.updated_at,
        })
    }

    /// Writes one behavioral record; an existing record under the same identity keeps
    /// its creation time.
    pub(crate) fn save_behavioral(
        &self,
        prepared: &BehavioralMemoryPrepared,
    ) -> Result<BehavioralMemoryRecord> {
        let _guard = self.lock()?;
        let (path, meta_path) = self.record_paths(MemoryCategory::Behavioral, &prepared.object_id);
        fs::create_dir_all(path.parent().expect("category directory has a parent"))?;
        let existing_meta: LocalBehavioralMeta = read_meta(&meta_path)?;
        let now = Utc::now().to_rfc3339();
        let meta = LocalBehavioralMeta {
            key: prepared.key.clone(),
            access_level: prepared.access_level,
            expires_at: prepared.expires_at.clone(),
            sender_id_list: prepared.sender_id_list.clone(),
            group_id_list: prepared.group_id_list.clone(),
            created_at: non_empty_or(existing_meta.created_at, &now),
            updated_at: now,
        };
        write_record_files(&path, &meta_path, &prepared.content, &meta)?;
        Ok(BehavioralMemoryRecord {
            key: meta.key,
            content: prepared.content.clone(),
            access_level: meta.access_level,
            expires_at: meta.expires_at,
            sender_id_list: meta.sender_id_list,
            group_id_list: meta.group_id_list,
            object_id: prepared.object_id.clone(),
            created_at: meta.created_at,
            updated_at: meta.updated_at,
        })
    }

    /// Writes one person profile; the QQ ID is the identity, so saving overwrites it.
    pub(crate) fn save_person(&self, write: &PersonMemoryWrite) -> Result<PersonMemoryRecord> {
        let _guard = self.lock()?;
        let object_id = self
            .get_person_record(&write.profile.qq_id)?
            .map(|record| record.object_id)
            .unwrap_or_else(|| {
                memory_object_id(MemoryCategory::Person, &write.profile.qq_id, &[], &[])
            });
        let (path, meta_path) = self.record_paths(MemoryCategory::Person, &object_id);
        fs::create_dir_all(path.parent().expect("category directory has a parent"))?;
        let existing_meta: LocalPersonMeta = read_meta(&meta_path)?;
        let now = Utc::now().to_rfc3339();
        let meta = LocalPersonMeta {
            profile: write.profile.clone(),
            access_level: write.access_level,
            created_at: non_empty_or(existing_meta.created_at, &now),
            updated_at: now,
        };
        let body = serde_json::to_string(&meta.profile)?;
        write_record_files(&path, &meta_path, &body, &meta)?;
        Ok(PersonMemoryRecord {
            profile: meta.profile,
            access_level: meta.access_level,
            object_id,
            created_at: meta.created_at,
            updated_at: meta.updated_at,
        })
    }

    /// Exact identity read of one concept record under this scope.
    pub fn get_concept(
        &self,
        key: &str,
        sender_id_list: &[String],
        group_id_list: &[String],
    ) -> Result<Option<ConceptMemoryRecord>> {
        let object_id =
            memory_object_id(MemoryCategory::Concept, key.trim(), sender_id_list, group_id_list);
        let (path, meta_path) = self.record_paths(MemoryCategory::Concept, &object_id);
        Ok(read_concept_record(&path, &meta_path, &object_id).ok().flatten())
    }

    /// Exact identity read of one behavioral record under this scope.
    pub fn get_behavioral(
        &self,
        key: &str,
        sender_id_list: &[String],
        group_id_list: &[String],
    ) -> Result<Option<BehavioralMemoryRecord>> {
        let object_id =
            memory_object_id(MemoryCategory::Behavioral, key.trim(), sender_id_list, group_id_list);
        let (path, meta_path) = self.record_paths(MemoryCategory::Behavioral, &object_id);
        Ok(read_behavioral_record(&path, &meta_path, &object_id).ok().flatten())
    }

    /// Exact identity read of one person profile by QQ ID.
    pub fn get_person_record(&self, qq_id: &str) -> Result<Option<PersonMemoryRecord>> {
        Ok(self
            .list_category(MemoryCategory::Person, None, usize::MAX)?
            .into_iter()
            .filter_map(|hit| match hit.record {
                MemoryRecord::Person(record) => Some(record),
                _ => None,
            })
            .find(|record| record.profile.qq_id == qq_id))
    }

    /// All categories most-recently-updated first.
    pub fn list(&self, query: Option<&str>, top_n: usize) -> Result<Vec<MemorySearchHit>> {
        let mut hits = Vec::new();
        for category in MemoryCategory::ALL {
            hits.extend(self.list_category(category, query, top_n)?);
        }
        hits.sort_by(|a, b| b.record.updated_at().cmp(a.record.updated_at()));
        hits.truncate(top_n);
        Ok(hits)
    }

    /// One category's records, optionally filtered by a case-insensitive substring
    /// query over the searchable text, newest effective record first.
    pub fn list_category(
        &self,
        category: MemoryCategory,
        query: Option<&str>,
        top_n: usize,
    ) -> Result<Vec<MemorySearchHit>> {
        let directory = self.category_directory(category);
        let query = query.map(normalize_search_query).filter(|query| !query.is_empty());
        let mut records = Vec::new();
        if !directory.exists() {
            return Ok(records);
        }
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
                continue;
            }
            let Some(object_id) = path.file_stem().and_then(|name| name.to_str()) else {
                continue;
            };
            let meta_path = path.with_extension("meta.json");
            // A corrupt or unreadable record is skipped, not fatal to the listing.
            let hit = match category {
                MemoryCategory::Concept => read_concept_record(&path, &meta_path, object_id)
                    .ok()
                    .flatten()
                    .map(MemoryRecord::Concept),
                MemoryCategory::Behavioral => read_behavioral_record(&path, &meta_path, object_id)
                    .ok()
                    .flatten()
                    .map(MemoryRecord::Behavioral),
                MemoryCategory::Person => read_person_record(&meta_path, object_id)
                    .ok()
                    .flatten()
                    .map(MemoryRecord::Person),
            };
            let Some(record) = hit else {
                continue;
            };
            if is_memory_expired(&record) {
                continue;
            }
            if let Some(query) = &query {
                if !matches_query(&record_key(&record), &record_search_text(&record), query) {
                    continue;
                }
            }
            records.push(MemorySearchHit { record, distance: None });
        }
        records
            .sort_by(|a, b| record_effective_time(&b.record).cmp(record_effective_time(&a.record)));
        records.truncate(top_n);
        Ok(records)
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, ()>> {
        self.write_lock
            .lock()
            .map_err(|_| crate::string_error!("local memory write lock poisoned"))
    }
}

// ── Per-category on-disk metadata ─────────────────────────────────────────────

#[derive(Debug, Default, Serialize, Deserialize)]
struct LocalConceptMeta {
    key: String,
    #[serde(default = "default_access_level")]
    access_level: u32,
    starts_at: String,
    #[serde(default)]
    sender_id_list: Vec<String>,
    #[serde(default)]
    group_id_list: Vec<String>,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    updated_at: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct LocalBehavioralMeta {
    key: String,
    #[serde(default = "default_access_level")]
    access_level: u32,
    expires_at: String,
    #[serde(default)]
    sender_id_list: Vec<String>,
    #[serde(default)]
    group_id_list: Vec<String>,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    updated_at: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct LocalPersonMeta {
    #[serde(default)]
    profile: PersonMemory,
    #[serde(default = "default_access_level")]
    access_level: u32,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    updated_at: String,
}

fn default_access_level() -> u32 {
    crate::storage::user_permission::ROLE_SELF_ACCESS_LEVEL
}

// ── Record file helpers ───────────────────────────────────────────────────────

/// Reads the metadata sidecar of one record; a missing sidecar is `None`, a corrupt
/// one is a validation error.
fn read_meta_strict<T: serde::de::DeserializeOwned>(meta_path: &Path) -> Result<Option<T>> {
    match fs::read_to_string(meta_path) {
        Ok(text) => serde_json::from_str(&text).map(Some).map_err(|error| {
            warn!("Failed to parse local memory metadata {}: {error}", meta_path.display());
            Error::ValidationError("invalid local memory metadata".to_string())
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Reads metadata for a write: an absent sidecar starts fresh.
fn read_meta<T: serde::de::DeserializeOwned + Default>(meta_path: &Path) -> Result<T> {
    Ok(read_meta_strict(meta_path)?.unwrap_or_default())
}

fn write_record_files(
    path: &Path,
    meta_path: &Path,
    body: &str,
    meta: &impl Serialize,
) -> Result<()> {
    let temporary = path.with_extension("md.tmp");
    let meta_temporary = path.with_extension("meta.json.tmp");
    fs::write(&temporary, body)?;
    fs::write(&meta_temporary, serde_json::to_vec(meta)?)?;
    fs::rename(&temporary, path)?;
    fs::rename(&meta_temporary, meta_path)?;
    Ok(())
}

fn read_concept_record(
    path: &Path,
    meta_path: &Path,
    object_id: &str,
) -> Result<Option<ConceptMemoryRecord>> {
    let meta: LocalConceptMeta = match read_meta_strict(meta_path)? {
        Some(meta) => meta,
        None => return Ok(None),
    };
    let content = fs::read_to_string(path)?;
    Ok(Some(ConceptMemoryRecord {
        key: meta.key,
        content,
        access_level: meta.access_level,
        starts_at: meta.starts_at,
        sender_id_list: meta.sender_id_list,
        group_id_list: meta.group_id_list,
        object_id: object_id.to_string(),
        created_at: meta.created_at,
        updated_at: meta.updated_at,
    }))
}

fn read_behavioral_record(
    path: &Path,
    meta_path: &Path,
    object_id: &str,
) -> Result<Option<BehavioralMemoryRecord>> {
    let meta: LocalBehavioralMeta = match read_meta_strict(meta_path)? {
        Some(meta) => meta,
        None => return Ok(None),
    };
    let content = fs::read_to_string(path)?;
    Ok(Some(BehavioralMemoryRecord {
        key: meta.key,
        content,
        access_level: meta.access_level,
        expires_at: meta.expires_at,
        sender_id_list: meta.sender_id_list,
        group_id_list: meta.group_id_list,
        object_id: object_id.to_string(),
        created_at: meta.created_at,
        updated_at: meta.updated_at,
    }))
}

fn read_person_record(meta_path: &Path, object_id: &str) -> Result<Option<PersonMemoryRecord>> {
    let meta: LocalPersonMeta = match read_meta_strict(meta_path)? {
        Some(meta) => meta,
        None => return Ok(None),
    };
    Ok(Some(PersonMemoryRecord {
        profile: meta.profile,
        access_level: meta.access_level,
        object_id: object_id.to_string(),
        created_at: meta.created_at,
        updated_at: meta.updated_at,
    }))
}

fn non_empty_or(value: String, fallback: &str) -> String {
    if value.is_empty() {
        fallback.to_string()
    } else {
        value
    }
}

fn record_key(record: &MemoryRecord) -> &str {
    match record {
        MemoryRecord::Concept(record) => &record.key,
        MemoryRecord::Behavioral(record) => &record.key,
        // Person identities never match by title; their searchable text is the profile.
        MemoryRecord::Person(_) => "",
    }
}

fn record_search_text(record: &MemoryRecord) -> String {
    match record {
        MemoryRecord::Concept(record) => format!("{} {}", record.key, record.content),
        MemoryRecord::Behavioral(record) => format!("{} {}", record.key, record.content),
        MemoryRecord::Person(record) => crate::storage::memory::person_search_text(&record.profile),
    }
}

fn record_effective_time(record: &MemoryRecord) -> &str {
    match record {
        // Concepts sort by when they became effective; the rest by recency.
        MemoryRecord::Concept(record) => &record.starts_at,
        MemoryRecord::Person(record) => &record.updated_at,
        MemoryRecord::Behavioral(record) => &record.updated_at,
    }
}

fn normalize_search_query(query: &str) -> String {
    query.trim().to_lowercase()
}
fn matches_query(key: &str, value: &str, query: &str) -> bool {
    key.to_lowercase().contains(query) || value.to_lowercase().contains(query)
}
