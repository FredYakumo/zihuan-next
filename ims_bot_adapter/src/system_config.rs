use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use zihuan_core::error::{Error, Result};
use zihuan_core::system_config::{
    load_system_config_root, save_system_config_root, SystemConfigSection,
};
use zihuan_graph_engine::object_storage::S3Ref;

use crate::adapter::{BotAdapter, BotAdapterConfig, SharedBotAdapter};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotAdapterConnection {
    pub bot_server_url: String,
    #[serde(default)]
    pub adapter_server_url: Option<String>,
    #[serde(default)]
    pub bot_server_token: Option<String>,
    #[serde(default)]
    pub qq_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotAdapterConnectionConfig {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
    pub kind: BotAdapterConnectionKind,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BotAdapterConnectionKind {
    BotAdapter(BotAdapterConnection),
}

pub struct BotAdapterConnectionsSection;

impl SystemConfigSection for BotAdapterConnectionsSection {
    const SECTION_KEY: &'static str = "connections";
    type Value = Vec<BotAdapterConnectionConfig>;

    fn read_from_root(root: &Value) -> Result<Self::Value> {
        let mut items = Vec::new();
        let Some(array) = root.get(Self::SECTION_KEY).and_then(Value::as_array) else {
            return Ok(items);
        };

        for item in array {
            if let Ok(parsed) = serde_json::from_value::<BotAdapterConnectionConfig>(item.clone()) {
                items.push(parsed);
            }
        }

        Ok(items)
    }

    fn write_to_root(root: &mut Value, value: &Self::Value) -> Result<()> {
        let object = root.as_object_mut().ok_or_else(|| {
            Error::StringError("system config root must be a JSON object".to_string())
        })?;
        let existing = object
            .get(Self::SECTION_KEY)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let mut merged = Vec::new();
        for item in existing {
            if serde_json::from_value::<BotAdapterConnectionConfig>(item.clone()).is_err() {
                merged.push(item);
            }
        }
        for item in value {
            merged.push(serde_json::to_value(item).map_err(|err| {
                Error::StringError(format!("failed to serialize bot adapter connection: {err}"))
            })?);
        }

        object.insert(Self::SECTION_KEY.to_string(), Value::Array(merged));
        if !object.contains_key("version") {
            object.insert("version".to_string(), Value::from(1));
        }
        Ok(())
    }
}

pub fn load_ims_bot_adapter_connections() -> Result<Vec<BotAdapterConnectionConfig>> {
    let root = load_system_config_root()?;
    BotAdapterConnectionsSection::read_from_root(&root)
}

pub fn save_ims_bot_adapter_connections(
    connections: Vec<BotAdapterConnectionConfig>,
) -> Result<()> {
    let mut root = load_system_config_root()?;
    BotAdapterConnectionsSection::write_to_root(&mut root, &connections)?;
    save_system_config_root(&root)
}

pub fn parse_ims_bot_adapter_connection(value: &Value) -> Result<BotAdapterConnection> {
    serde_json::from_value::<BotAdapterConnection>(value.clone()).map_err(|err| {
        Error::ValidationError(format!("invalid ims_bot_adapter connection config: {err}"))
    })
}

pub async fn build_ims_bot_adapter(
    connection: &BotAdapterConnection,
    object_storage: Option<Arc<S3Ref>>,
) -> SharedBotAdapter {
    BotAdapter::new(
        BotAdapterConfig::new(
            connection.bot_server_url.clone(),
            connection.bot_server_token.clone().unwrap_or_default(),
            connection.qq_id.clone().unwrap_or_default(),
        )
        .with_object_storage(object_storage),
    )
    .await
    .into_shared()
}
