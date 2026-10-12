//! The retrieval databases a role service reads from.
//!
//! An agent's memory and its image library are different collections, so a Weaviate or
//! Elasticsearch connection can serve only one of them: the connection carries the class
//! (or index) it addresses together with the [`WeaviateCollectionSchema`] that describes it.
//! A service therefore selects a *set* of stores, and each capability resolves the store
//! whose schema matches it — memory from `AgentMemory`, images from `ImageSemantic`.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::role::service_config::MemoryBackendKind;
use crate::storage::{ConnectionConfig, ConnectionKind, WeaviateCollectionSchema};

/// One database an agent retrieves from.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RetrievalStoreConfig {
    /// Memory kept in local Markdown files, with no external database.
    LocalMarkdown,
    /// A configured Weaviate or Elasticsearch connection.
    Connection { connection_id: String },
}

impl RetrievalStoreConfig {
    pub fn connection_id(&self) -> Option<&str> {
        match self {
            Self::LocalMarkdown => None,
            Self::Connection { connection_id } => Some(connection_id.as_str()),
        }
    }
}

/// The backend a service's memory resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryStoreSelection {
    LocalMarkdown,
    Connection { connection_id: String, kind: MemoryBackendKind },
}

fn find_connection<'a>(
    connections: &'a [ConnectionConfig],
    connection_id: &str,
) -> Result<&'a ConnectionConfig> {
    connections
        .iter()
        .find(|connection| connection.id == connection_id || connection.config_id == connection_id)
        .ok_or_else(|| {
            Error::ValidationError(format!("检索数据库连接 '{connection_id}' 不存在"))
        })
}

/// The schema a connection is able to serve, or `None` when it is not a retrieval database.
fn connection_schema(connection: &ConnectionConfig) -> Option<WeaviateCollectionSchema> {
    match &connection.kind {
        ConnectionKind::Weaviate(weaviate) => Some(weaviate.collection_schema),
        ConnectionKind::Elasticsearch(elasticsearch) => Some(elasticsearch.collection_schema),
        _ => None,
    }
}

/// Resolves the store that feeds agent memory, or `None` when memory is not configured.
///
/// Stores serving another schema (an image library, say) are skipped rather than rejected, so
/// one list can describe every database a service uses.
pub fn resolve_memory_store(
    stores: &[RetrievalStoreConfig],
    connections: &[ConnectionConfig],
) -> Result<Option<MemoryStoreSelection>> {
    let mut selected = None;
    for store in stores {
        let candidate = match store {
            RetrievalStoreConfig::LocalMarkdown => MemoryStoreSelection::LocalMarkdown,
            RetrievalStoreConfig::Connection { connection_id } => {
                let connection = find_connection(connections, connection_id)?;
                let kind = match connection_schema(connection) {
                    Some(WeaviateCollectionSchema::AgentMemory) => match &connection.kind {
                        ConnectionKind::Weaviate(_) => MemoryBackendKind::Weaviate,
                        _ => MemoryBackendKind::Elasticsearch,
                    },
                    _ => continue,
                };
                MemoryStoreSelection::Connection {
                    connection_id: connection.id.clone(),
                    kind,
                }
            }
        };
        if selected.is_some() {
            return Err(Error::ValidationError(
                "检索数据库只能配置一个记忆库（AgentMemory）".to_string(),
            ));
        }
        selected = Some(candidate);
    }
    Ok(selected)
}

/// Resolves the connection that stores image vectors, or `None` when none is configured.
pub fn resolve_image_store(
    stores: &[RetrievalStoreConfig],
    connections: &[ConnectionConfig],
) -> Result<Option<String>> {
    let mut selected: Option<String> = None;
    for store in stores {
        let RetrievalStoreConfig::Connection { connection_id } = store else {
            continue;
        };
        let connection = find_connection(connections, connection_id)?;
        if connection_schema(connection) != Some(WeaviateCollectionSchema::ImageSemantic) {
            continue;
        }
        if selected.is_some() {
            return Err(Error::ValidationError(
                "检索数据库只能配置一个图片向量库（ImageSemantic）".to_string(),
            ));
        }
        selected = Some(connection.id.clone());
    }
    Ok(selected)
}

/// Rejects a store set a service could not resolve unambiguously.
///
/// Called on save so a bad selection is reported while the operator is looking at the form,
/// rather than surfacing later as an unavailable memory or image library.
pub fn validate_retrieval_stores(
    stores: &[RetrievalStoreConfig],
    connections: &[ConnectionConfig],
) -> Result<()> {
    let mut seen: Vec<&str> = Vec::new();
    for store in stores {
        let RetrievalStoreConfig::Connection { connection_id } = store else {
            continue;
        };
        if seen.contains(&connection_id.as_str()) {
            return Err(Error::ValidationError(format!(
                "检索数据库连接 '{connection_id}' 被重复选择"
            )));
        }
        seen.push(connection_id.as_str());
        let connection = find_connection(connections, connection_id)?;
        if connection_schema(connection).is_none() {
            return Err(Error::ValidationError(format!(
                "连接 '{}' 不是检索数据库，请选择 Weaviate 或 Elasticsearch 连接",
                connection.name
            )));
        }
    }
    resolve_memory_store(stores, connections)?;
    resolve_image_store(stores, connections)?;
    Ok(())
}
