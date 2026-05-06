use std::fs;
use std::path::PathBuf;

use serde::{de::DeserializeOwned, Serialize};
use serde_json::{Map, Value};

use crate::error::{Error, Result};

const APP_DIR_NAME: &str = "zihuan-next_aibot";
const SYSTEM_CONFIG_DIR: &str = "system_config";
const SYSTEM_CONFIG_FILE: &str = "system_config.json";
const VERSION_KEY: &str = "version";
const DEFAULT_VERSION: u32 = 1;

pub trait SystemConfigSection {
    const SECTION_KEY: &'static str;
    type Value: DeserializeOwned + Serialize + Default;

    fn read_from_root(root: &Value) -> Result<Self::Value> {
        let value = root
            .get(Self::SECTION_KEY)
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new()));
        serde_json::from_value(value).map_err(|err| {
            Error::StringError(format!(
                "failed to parse system config section '{}': {err}",
                Self::SECTION_KEY
            ))
        })
    }

    fn write_to_root(root: &mut Value, value: &Self::Value) -> Result<()> {
        let object = root.as_object_mut().ok_or_else(|| {
            Error::StringError("system config root must be a JSON object".to_string())
        })?;
        object.insert(
            Self::SECTION_KEY.to_string(),
            serde_json::to_value(value).map_err(|err| {
                Error::StringError(format!(
                    "failed to serialize system config section '{}': {err}",
                    Self::SECTION_KEY
                ))
            })?,
        );
        ensure_version(object);
        Ok(())
    }
}

pub fn load_section<S: SystemConfigSection>() -> Result<S::Value> {
    let root = load_system_config_root()?;
    S::read_from_root(&root)
}

pub fn save_section<S: SystemConfigSection>(value: &S::Value) -> Result<()> {
    let mut root = load_system_config_root()?;
    S::write_to_root(&mut root, value)?;
    save_system_config_root(&root)
}

pub fn load_system_config_root() -> Result<Value> {
    let path = system_config_file_path();
    if !path.exists() {
        return Ok(default_system_config_root());
    }

    let content = fs::read_to_string(&path)?;
    let mut root: Value = serde_json::from_str(&content)
        .map_err(|err| Error::StringError(format!("failed to parse {}: {err}", path.display())))?;
    normalize_root(&mut root)?;
    Ok(root)
}

pub fn save_system_config_root(root: &Value) -> Result<()> {
    let mut normalized = root.clone();
    normalize_root(&mut normalized)?;

    let path = system_config_file_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let content = serde_json::to_string_pretty(&normalized)
        .map_err(|err| Error::StringError(format!("failed to serialize system config: {err}")))?;
    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, content)?;
    fs::rename(&tmp_path, &path)?;
    Ok(())
}

pub fn system_config_file_path() -> PathBuf {
    system_config_dir().join(SYSTEM_CONFIG_FILE)
}

pub fn system_config_dir() -> PathBuf {
    app_data_dir().join(APP_DIR_NAME).join(SYSTEM_CONFIG_DIR)
}

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

fn default_system_config_root() -> Value {
    let mut object = Map::new();
    ensure_version(&mut object);
    Value::Object(object)
}

fn normalize_root(root: &mut Value) -> Result<()> {
    let object = match root {
        Value::Object(object) => object,
        Value::Null => {
            *root = default_system_config_root();
            return Ok(());
        }
        _ => {
            return Err(Error::StringError(
                "system config root must be a JSON object".to_string(),
            ))
        }
    };
    ensure_version(object);
    Ok(())
}

fn ensure_version(object: &mut Map<String, Value>) {
    object
        .entry(VERSION_KEY.to_string())
        .or_insert_with(|| Value::from(DEFAULT_VERSION));
}
