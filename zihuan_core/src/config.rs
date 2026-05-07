use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::connection_manager::{RuntimeConnectionInstanceSummary, RuntimeConnectionStatus};
use crate::error::{Error, Result};
use crate::system_config::system_config_file_path;

const CONFIG_ROOT_VERSION: u32 = 2;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ConfigCategory {
    Connection,
    LlmRef,
    Agent,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ConfigKind {
    ConnectionMysql,
    ConnectionRedis,
    ConnectionWeaviate,
    ConnectionRustfs,
    ConnectionBotAdapter,
    ConnectionTavily,
    LlmRef,
    AgentQqChat,
    AgentHttpStream,
}

impl ConfigKind {
    pub fn category(self) -> ConfigCategory {
        match self {
            Self::ConnectionMysql
            | Self::ConnectionRedis
            | Self::ConnectionWeaviate
            | Self::ConnectionRustfs
            | Self::ConnectionBotAdapter
            | Self::ConnectionTavily => ConfigCategory::Connection,
            Self::LlmRef => ConfigCategory::LlmRef,
            Self::AgentQqChat | Self::AgentHttpStream => ConfigCategory::Agent,
        }
    }
}

pub trait ConfigRecord {
    fn config_id(&self) -> &str;
    fn name(&self) -> &str;
    fn enabled(&self) -> bool;
    fn updated_at(&self) -> &str;
    fn kind(&self) -> ConfigKind;
    fn validate(&self) -> Result<()>;
    fn redacted_summary(&self) -> Value;
}

pub trait RuntimeInstance {
    fn instance_id(&self) -> &str;
    fn config_id(&self) -> &str;
    fn kind(&self) -> &str;
    fn status(&self) -> RuntimeConnectionStatus;
    fn started_at(&self) -> chrono::DateTime<chrono::Utc>;
    fn last_used_at(&self) -> chrono::DateTime<chrono::Utc>;
    fn keep_alive(&self) -> bool;
    fn heartbeat_interval_secs(&self) -> Option<u64>;
}

pub type RuntimeInstanceSummary = RuntimeConnectionInstanceSummary;

pub trait InstanceFactory<C, I> {
    fn create(&self, config: &C) -> Result<I>;
}

pub trait InstanceManager {
    type Handle: Clone + Send + Sync + 'static;

    fn get_or_create_by_config_id(
        &self,
        config_id: &str,
    ) -> impl std::future::Future<Output = Result<Self::Handle>> + Send;
}

pub trait ConfigRepository: Send + Sync {
    fn load_root(&self) -> Result<ConfigRoot>;
    fn save_root(&self, root: &ConfigRoot) -> Result<()>;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoredConfigRecord {
    pub config_id: String,
    pub kind: ConfigKind,
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub updated_at: String,
    pub spec: Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ConfigCollections {
    #[serde(default)]
    pub connections: Vec<StoredConfigRecord>,
    #[serde(default)]
    pub llm_refs: Vec<StoredConfigRecord>,
    #[serde(default)]
    pub agents: Vec<StoredConfigRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConfigRoot {
    #[serde(default = "default_config_root_version")]
    pub version: u32,
    #[serde(default)]
    pub configs: ConfigCollections,
}

impl Default for ConfigRoot {
    fn default() -> Self {
        Self {
            version: default_config_root_version(),
            configs: ConfigCollections::default(),
        }
    }
}

fn default_config_root_version() -> u32 {
    CONFIG_ROOT_VERSION
}

impl ConfigRoot {
    pub fn from_value(root: Value) -> Result<Self> {
        match root {
            Value::Null => Ok(Self::default()),
            Value::Object(object) => {
                if object.contains_key("configs") {
                    serde_json::from_value::<Self>(Value::Object(object)).map_err(|err| {
                        Error::StringError(format!("failed to parse unified config root: {err}"))
                    })
                } else {
                    Self::from_legacy_object(object)
                }
            }
            _ => Err(Error::StringError(
                "system config root must be a JSON object".to_string(),
            )),
        }
    }

    fn from_legacy_object(object: Map<String, Value>) -> Result<Self> {
        let mut root = Self::default();
        root.configs.connections = migrate_legacy_collection(
            object.get("connections").and_then(Value::as_array),
            ConfigCategory::Connection,
        )?;
        root.configs.llm_refs = migrate_legacy_collection(
            object.get("llm_refs").and_then(Value::as_array),
            ConfigCategory::LlmRef,
        )?;
        root.configs.agents = migrate_legacy_collection(
            object.get("agents").and_then(Value::as_array),
            ConfigCategory::Agent,
        )?;
        Ok(root)
    }

    pub fn to_value(&self) -> Result<Value> {
        serde_json::to_value(self).map_err(|err| {
            Error::StringError(format!("failed to serialize unified config root: {err}"))
        })
    }
}

fn migrate_legacy_collection(
    items: Option<&Vec<Value>>,
    category: ConfigCategory,
) -> Result<Vec<StoredConfigRecord>> {
    let mut records = Vec::new();
    for item in items.into_iter().flatten() {
        let Some(object) = item.as_object() else {
            continue;
        };
        let config_id = config_id_from_legacy_object(object);
        let kind = infer_kind_from_legacy_object(category, object)?;
        let name = object
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let enabled = object
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let updated_at = object
            .get("updated_at")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let spec = legacy_object_to_spec(category, object)?;
        records.push(StoredConfigRecord {
            config_id,
            kind,
            name,
            enabled,
            updated_at,
            spec,
        });
    }
    Ok(records)
}

fn config_id_from_legacy_object(object: &Map<String, Value>) -> String {
    object
        .get("config_id")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            object
                .get("id")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
        })
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| Uuid::new_v4().to_string())
}

fn infer_kind_from_legacy_object(
    category: ConfigCategory,
    object: &Map<String, Value>,
) -> Result<ConfigKind> {
    match category {
        ConfigCategory::Connection => match object
            .get("kind")
            .and_then(Value::as_object)
            .and_then(|kind| kind.get("type"))
            .and_then(Value::as_str)
        {
            Some("mysql") => Ok(ConfigKind::ConnectionMysql),
            Some("redis") => Ok(ConfigKind::ConnectionRedis),
            Some("weaviate") => Ok(ConfigKind::ConnectionWeaviate),
            Some("rustfs") => Ok(ConfigKind::ConnectionRustfs),
            Some("bot_adapter") | Some("ims_bot_adapter") => Ok(ConfigKind::ConnectionBotAdapter),
            Some("tavily") => Ok(ConfigKind::ConnectionTavily),
            Some(other) => Err(Error::ValidationError(format!(
                "unsupported legacy connection kind '{other}'"
            ))),
            None => Err(Error::ValidationError(
                "legacy connection is missing kind.type".to_string(),
            )),
        },
        ConfigCategory::LlmRef => Ok(ConfigKind::LlmRef),
        ConfigCategory::Agent => match object
            .get("agent_type")
            .and_then(Value::as_object)
            .and_then(|agent_type| agent_type.get("type"))
            .and_then(Value::as_str)
        {
            Some("qq_chat") => Ok(ConfigKind::AgentQqChat),
            Some("http_stream") => Ok(ConfigKind::AgentHttpStream),
            Some(other) => Err(Error::ValidationError(format!(
                "unsupported legacy agent type '{other}'"
            ))),
            None => Err(Error::ValidationError(
                "legacy agent is missing agent_type.type".to_string(),
            )),
        },
    }
}

fn legacy_object_to_spec(category: ConfigCategory, object: &Map<String, Value>) -> Result<Value> {
    match category {
        ConfigCategory::Connection => Ok(object.get("kind").cloned().unwrap_or(Value::Null)),
        ConfigCategory::LlmRef => Ok(object.get("llm").cloned().unwrap_or(Value::Null)),
        ConfigCategory::Agent => {
            let mut spec = Map::new();
            spec.insert(
                "agent_type".to_string(),
                object.get("agent_type").cloned().unwrap_or(Value::Null),
            );
            spec.insert(
                "auto_start".to_string(),
                object
                    .get("auto_start")
                    .cloned()
                    .unwrap_or(Value::Bool(false)),
            );
            spec.insert(
                "is_default".to_string(),
                object
                    .get("is_default")
                    .cloned()
                    .unwrap_or(Value::Bool(false)),
            );
            spec.insert(
                "tools".to_string(),
                object
                    .get("tools")
                    .cloned()
                    .unwrap_or_else(|| Value::Array(Vec::new())),
            );
            Ok(Value::Object(spec))
        }
    }
}

#[derive(Debug, Clone)]
pub struct FsConfigRepository {
    path: PathBuf,
}

impl Default for FsConfigRepository {
    fn default() -> Self {
        Self {
            path: system_config_file_path(),
        }
    }
}

impl ConfigRepository for FsConfigRepository {
    fn load_root(&self) -> Result<ConfigRoot> {
        if !self.path.exists() {
            return Ok(ConfigRoot::default());
        }

        let content = fs::read_to_string(&self.path)?;
        let value = serde_json::from_str::<Value>(&content).map_err(|err| {
            Error::StringError(format!("failed to parse {}: {err}", self.path.display()))
        })?;
        ConfigRoot::from_value(value)
    }

    fn save_root(&self, root: &ConfigRoot) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let value = root.to_value()?;
        let content = serde_json::to_string_pretty(&value).map_err(|err| {
            Error::StringError(format!("failed to serialize unified config root: {err}"))
        })?;
        let tmp_path = self.path.with_extension("json.tmp");
        fs::write(&tmp_path, content)?;
        fs::rename(&tmp_path, &self.path)?;
        Ok(())
    }
}

pub struct ConfigCenter<R: ConfigRepository = FsConfigRepository> {
    repository: R,
}

impl ConfigCenter<FsConfigRepository> {
    pub fn shared() -> &'static Self {
        static INSTANCE: OnceLock<ConfigCenter<FsConfigRepository>> = OnceLock::new();
        INSTANCE.get_or_init(|| ConfigCenter::new(FsConfigRepository::default()))
    }
}

impl<R: ConfigRepository> ConfigCenter<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }

    pub fn new_config_id(&self) -> String {
        Uuid::new_v4().to_string()
    }

    pub fn load_root(&self) -> Result<ConfigRoot> {
        self.repository.load_root()
    }

    pub fn save_root(&self, root: &ConfigRoot) -> Result<()> {
        self.repository.save_root(root)
    }

    pub fn list_configs(&self, category: ConfigCategory) -> Result<Vec<StoredConfigRecord>> {
        let root = self.load_root()?;
        Ok(match category {
            ConfigCategory::Connection => root.configs.connections,
            ConfigCategory::LlmRef => root.configs.llm_refs,
            ConfigCategory::Agent => root.configs.agents,
        })
    }

    pub fn get_config(&self, config_id: &str) -> Result<Option<StoredConfigRecord>> {
        let root = self.load_root()?;
        Ok(root
            .configs
            .connections
            .into_iter()
            .chain(root.configs.llm_refs)
            .chain(root.configs.agents)
            .find(|record| record.config_id == config_id))
    }

    pub fn upsert_config(&self, record: StoredConfigRecord) -> Result<()> {
        let mut root = self.load_root()?;
        let bucket = bucket_mut(&mut root, record.kind.category());
        if let Some(existing) = bucket
            .iter_mut()
            .find(|item| item.config_id == record.config_id)
        {
            *existing = record;
        } else {
            bucket.push(record);
        }
        self.save_root(&root)
    }

    pub fn delete_config(&self, category: ConfigCategory, config_id: &str) -> Result<bool> {
        let mut root = self.load_root()?;
        let bucket = bucket_mut(&mut root, category);
        let before = bucket.len();
        bucket.retain(|record| record.config_id != config_id);
        if before == bucket.len() {
            return Ok(false);
        }
        self.save_root(&root)?;
        Ok(true)
    }
}

fn bucket_mut(root: &mut ConfigRoot, category: ConfigCategory) -> &mut Vec<StoredConfigRecord> {
    match category {
        ConfigCategory::Connection => &mut root.configs.connections,
        ConfigCategory::LlmRef => &mut root.configs.llm_refs,
        ConfigCategory::Agent => &mut root.configs.agents,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_legacy_root_into_unified_configs() {
        let root = serde_json::json!({
            "version": 1,
            "connections": [{
                "id": "conn-1",
                "name": "MySQL Default",
                "enabled": true,
                "updated_at": "2026-01-01T00:00:00Z",
                "kind": { "type": "mysql", "url": "mysql://root@localhost/demo" }
            }],
            "llm_refs": [{
                "id": "llm-1",
                "name": "Main Model",
                "enabled": true,
                "updated_at": "2026-01-01T00:00:00Z",
                "llm": { "model_name": "demo", "api_endpoint": "http://localhost", "stream": false, "supports_multimodal_input": false, "timeout_secs": 30, "retry_count": 2 }
            }],
            "agents": [{
                "id": "agent-1",
                "name": "QQ Bot",
                "enabled": true,
                "auto_start": true,
                "is_default": false,
                "updated_at": "2026-01-01T00:00:00Z",
                "agent_type": { "type": "qq_chat", "ims_bot_adapter_connection_id": "conn-1", "tavily_connection_id": "conn-2" },
                "tools": []
            }]
        });

        let parsed = ConfigRoot::from_value(root).expect("legacy root should migrate");
        assert_eq!(parsed.version, CONFIG_ROOT_VERSION);
        assert_eq!(parsed.configs.connections.len(), 1);
        assert_eq!(parsed.configs.llm_refs.len(), 1);
        assert_eq!(parsed.configs.agents.len(), 1);
        assert_eq!(parsed.configs.connections[0].config_id, "conn-1");
        assert_eq!(parsed.configs.llm_refs[0].kind, ConfigKind::LlmRef);
        assert_eq!(parsed.configs.agents[0].kind, ConfigKind::AgentQqChat);
    }
}
