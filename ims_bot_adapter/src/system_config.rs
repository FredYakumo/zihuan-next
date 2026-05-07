use serde::{Deserialize, Serialize};
use std::sync::Arc;
use zihuan_core::error::{Error, Result};
use zihuan_graph_engine::object_storage::S3Ref;

use crate::adapter::{BotAdapter, BotAdapterConfig, SharedBotAdapter};
use storage_handler::{save_connections, ConnectionConfig, ConnectionKind};

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

pub fn load_ims_bot_adapter_connections() -> Result<Vec<BotAdapterConnectionConfig>> {
    Ok(storage_handler::load_connections()?
        .into_iter()
        .filter_map(|connection| match connection.kind {
            ConnectionKind::BotAdapter(raw) => Some(BotAdapterConnectionConfig {
                id: connection.id,
                name: connection.name,
                enabled: connection.enabled,
                kind: BotAdapterConnectionKind::BotAdapter(
                    parse_ims_bot_adapter_connection(&raw).ok()?,
                ),
                updated_at: connection.updated_at,
            }),
            _ => None,
        })
        .collect())
}

pub fn save_ims_bot_adapter_connections(
    connections: Vec<BotAdapterConnectionConfig>,
) -> Result<()> {
    let mut all = storage_handler::load_connections()?;
    all.retain(|connection| !matches!(connection.kind, ConnectionKind::BotAdapter(_)));
    all.extend(connections.into_iter().map(|connection| ConnectionConfig {
        id: connection.id.clone(),
        config_id: connection.id,
        name: connection.name,
        enabled: connection.enabled,
        kind: match connection.kind {
            BotAdapterConnectionKind::BotAdapter(bot) => {
                ConnectionKind::BotAdapter(serde_json::to_value(bot).unwrap_or(serde_json::Value::Null))
            }
        },
        updated_at: connection.updated_at,
    }));
    save_connections(all)
}

pub fn parse_ims_bot_adapter_connection(value: &serde_json::Value) -> Result<BotAdapterConnection> {
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
