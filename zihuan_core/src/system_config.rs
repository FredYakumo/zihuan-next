use std::fs;
use std::path::PathBuf;

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{Map, Value};
use chrono::{DateTime, Utc};

use crate::error::Result;

/// Names the application-owned directory beneath the platform configuration root,
/// shared by plugins, chat history, setup state, local memory, hyperparameters, and install artifacts.
const APP_DIR_NAME: &str = "zihuan-next_aibot";
/// Separates the shared system-configuration document from other application data.
const SYSTEM_CONFIG_DIR: &str = "system_config";
/// Identifies the JSON document that stores versioned system configuration sections.
const SYSTEM_CONFIG_FILE: &str = "system_config.json";
/// Stores the schema-version field required in every persisted system-configuration root.
const VERSION_KEY: &str = "version";
/// Supplies the version written for newly created configuration documents.
const DEFAULT_VERSION: u32 = 1;

/// Purpose: Defines one typed section stored within the shared versioned system-configuration JSON document.
pub trait SystemConfigSection {
    ///  Selects this section's stable key in the shared configuration document.
    const SECTION_KEY: &'static str;
    ///  Defines the serializable, defaultable Rust value stored under [`Self::SECTION_KEY`].
    type Value: DeserializeOwned + Serialize + Default;

    /// Purpose: Deserializes this section from a loaded configuration root, using the section default
    /// representation when the key is absent.
    fn read_from_root(root: &Value) -> Result<Self::Value> {
        let value = root.get(Self::SECTION_KEY).cloned().unwrap_or_else(|| Value::Array(Vec::new()));
        serde_json::from_value(value).map_err(|err| {
            crate::string_error!("failed to parse system config section '{}': {err}", Self::SECTION_KEY)
        })
    }

    /// Serializes this section into a configuration root while preserving the document version.
    fn write_to_root(root: &mut Value, value: &Self::Value) -> Result<()> {
        let object = root
            .as_object_mut()
            .ok_or_else(|| crate::string_error!("system config root must be a JSON object"))?;
        object.insert(
            Self::SECTION_KEY.to_string(),
            serde_json::to_value(value).map_err(|err| {
                crate::string_error!("failed to serialize system config section '{}': {err}", Self::SECTION_KEY)
            })?,
        );
        ensure_version(object);
        Ok(())
    }
}

/// Loads a typed configuration section for settings and runtime services without exposing
/// callers to shared-file paths, JSON parsing, or version normalization.
pub fn load_section<S: SystemConfigSection>() -> Result<S::Value> {
    let root = load_system_config_root()?;
    S::read_from_root(&root)
}

/// Persists one typed configuration section for settings and runtime services while
/// retaining unrelated sections in the shared document.
pub fn save_section<S: SystemConfigSection>(value: &S::Value) -> Result<()> {
    let mut root = load_system_config_root()?;
    S::write_to_root(&mut root, value)?;
    save_system_config_root(&root)
}

/// Reads the complete versioned configuration document used by configuration import,
/// export, and typed section helpers.
pub fn load_system_config_root() -> Result<Value> {
    let path = system_config_file_path();
    if !path.exists() {
        return Ok(default_system_config_root());
    }

    let content = fs::read_to_string(&path)?;
    let mut root: Value = serde_json::from_str(&content)
        .map_err(|err| crate::string_error!("failed to parse {}: {err}", path.display()))?;
    normalize_root(&mut root)?;
    Ok(root)
}

/// Writes the complete versioned configuration document after normalizing its root and
/// ensuring the configuration directory exists.
pub fn save_system_config_root(root: &Value) -> Result<()> {
    let mut normalized = root.clone();
    normalize_root(&mut normalized)?;

    let path = system_config_file_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let content = serde_json::to_string_pretty(&normalized)
        .map_err(|err| crate::string_error!("failed to serialize system config: {err}"))?;
    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, content)?;
    fs::rename(&tmp_path, &path)?;
    Ok(())
}

/// Locates the persisted system-configuration JSON document used by settings APIs and
/// configuration import/export.
pub fn system_config_file_path() -> PathBuf {
    system_config_dir().join(SYSTEM_CONFIG_FILE)
}

/// Locates the application-owned directory that contains the shared configuration document.
pub fn system_config_dir() -> PathBuf {
    application_data_dir().join(SYSTEM_CONFIG_DIR)
}

/// Resolves the platform configuration base directory before the application-specific
/// directory is appended.
pub fn app_data_dir() -> PathBuf {
    if cfg!(target_os = "windows") {
        std::env::var("APPDATA")
            .or_else(|_| std::env::var("LOCALAPPDATA"))
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
    } else {
        std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|_| std::env::var("HOME").map(|home| PathBuf::from(home).join(".config")))
            .unwrap_or_else(|_| PathBuf::from("."))
    }
}

/// Returns the canonical application-owned data root for plugins, chat history, setup
/// state, local memory, hyperparameters, and installation artifacts.
pub fn application_data_dir() -> PathBuf {
    app_data_dir().join(APP_DIR_NAME)
}

/// Creates the in-memory default configuration root for first run when no document exists.
fn default_system_config_root() -> Value {
    let mut object = Map::new();
    ensure_version(&mut object);
    Value::Object(object)
}

/// Validates the shared document shape and adds its version field before reads and writes.
fn normalize_root(root: &mut Value) -> Result<()> {
    let object = match root {
        Value::Object(object) => object,
        Value::Null => {
            *root = default_system_config_root();
            return Ok(());
        }
        _ => return Err(crate::string_error!("system config root must be a JSON object")),
    };
    ensure_version(object);
    Ok(())
}

/// Ensures every persisted configuration root declares a schema version for future migration.
fn ensure_version(object: &mut Map<String, Value>) {
    object
        .entry(VERSION_KEY.to_string())
        .or_insert_with(|| Value::from(DEFAULT_VERSION));
}

/// Supplies the default retention window used by global task cleanup when no setting exists.
fn default_task_ttl_hours() -> u64 {
    168 // 7 days
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalSettings {
    #[serde(default = "default_task_ttl_hours")]
    pub task_ttl_hours: u64,
    #[serde(default)]
    pub model_http_service: ModelHttpServiceSettings,
}

impl Default for GlobalSettings {
    /// Creates the first-run global settings used when the shared configuration has no
    /// persisted `global_settings` section.
    fn default() -> Self {
        Self {
            task_ttl_hours: default_task_ttl_hours(),
            model_http_service: ModelHttpServiceSettings::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelHttpServiceSettings {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub public_model_config_ids: Vec<String>,
    #[serde(default)]
    pub api_keys: Vec<ModelHttpApiKey>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelHttpApiKey {
    pub id: String,
    pub name: String,
    pub secret_hash: String,
    pub secret_prefix: String,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default = "default_api_key_enabled")]
    pub enabled: bool,
}

/// Supplies the default enabled state for persisted model HTTP API keys.
fn default_api_key_enabled() -> bool {
    true
}

pub struct GlobalSettingsSection;

impl SystemConfigSection for GlobalSettingsSection {
    const SECTION_KEY: &'static str = "global_settings";
    type Value = GlobalSettings;
}
