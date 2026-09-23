use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::storage::RuntimeStorageConnectionManager;
use crate::storage::{ConnectionConfig, ConnectionKind, ElasticsearchRef};
use crate::weaviate::WeaviateRef;

/// A business schema served by a retrieval store.
///
/// A retrieval-store connection identifies *where* vectors live (a Weaviate or
/// Elasticsearch service); the schema identifies *which* collection a consumer
/// needs. One connection therefore serves every schema, and the collection name
/// is derived here rather than stored on the connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalSchema {
    ImageSemantic,
    AgentMemory,
    QqMessage,
}

impl RetrievalSchema {
    /// Stable snake_case identifier used by configuration and the node editor.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ImageSemantic => "image_semantic",
            Self::AgentMemory => "agent_memory",
            Self::QqMessage => "qq_message",
        }
    }

    /// Parses the snake_case identifier produced by [`Self::as_str`].
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim() {
            "image_semantic" => Ok(Self::ImageSemantic),
            "agent_memory" => Ok(Self::AgentMemory),
            "qq_message" => Ok(Self::QqMessage),
            other => Err(Error::ValidationError(format!(
                "unknown retrieval schema '{other}'; expected 'image_semantic', 'agent_memory' or 'qq_message'"
            ))),
        }
    }

    /// Weaviate class hosting this schema.
    pub fn weaviate_class_name(self) -> &'static str {
        match self {
            Self::ImageSemantic => "ImageSemantic",
            Self::AgentMemory => "AgentMemory",
            Self::QqMessage => "QqMessage",
        }
    }

    /// Elasticsearch index hosting this schema.
    pub fn elasticsearch_index_name(self) -> &'static str {
        match self {
            Self::ImageSemantic => "zihuan_image_semantic",
            Self::AgentMemory => "zihuan_agent_memory",
            Self::QqMessage => "zihuan_qq_message",
        }
    }

    /// Elasticsearch `dense_vector` field holding this schema's embedding.
    pub fn vector_field(self) -> &'static str {
        match self {
            Self::ImageSemantic => "description_vector",
            Self::AgentMemory => "embedding",
            Self::QqMessage => "embedding",
        }
    }
}

/// Backend serving a retrieval store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetrievalBackend {
    Weaviate,
    Elasticsearch,
}

impl RetrievalBackend {
    /// Stable identifier reported to callers and the UI.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Weaviate => "weaviate",
            Self::Elasticsearch => "elasticsearch",
        }
    }
}

/// The retrieval store an agent is configured with.
///
/// Only the connection is configured; the schema is chosen by whichever
/// capability consumes the store.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RetrievalStoreConfig {
    /// On-disk store with no external service.
    LocalMarkdown,
    /// External retrieval service; the backend comes from the connection kind.
    Connection { connection_id: String },
}

impl RetrievalStoreConfig {
    /// Connection id when the store is an external service.
    pub fn connection_id(&self) -> Option<&str> {
        match self {
            Self::Connection { connection_id } => {
                let trimmed = connection_id.trim();
                (!trimmed.is_empty()).then_some(trimmed)
            }
            Self::LocalMarkdown => None,
        }
    }

    pub fn is_local(&self) -> bool {
        matches!(self, Self::LocalMarkdown)
    }
}

/// A resolved retrieval store, independent of any business schema.
///
/// Consumers pick the collection by passing the [`RetrievalSchema`] they need to
/// the schema-bound accessor or operation, so a single configured store serves
/// image semantic search and agent memory alike.
#[derive(Clone)]
pub struct RetrievalStoreRef {
    connection_id: String,
    backend: RetrievalBackend,
}

impl RetrievalStoreRef {
    /// Builds a store reference from a connection id, deriving the backend from
    /// the connection kind.
    pub fn resolve(connection_id: &str, connections: &[ConnectionConfig]) -> Result<Self> {
        let connection = connections
            .iter()
            .find(|item| item.id == connection_id || item.config_id == connection_id)
            .ok_or_else(|| {
                Error::ValidationError(format!("retrieval connection '{connection_id}' not found"))
            })?;
        Self::resolve_from_connection(connection)
    }

    /// Builds a store reference directly from an already-loaded connection.
    pub fn resolve_from_connection(connection: &ConnectionConfig) -> Result<Self> {
        let backend = match &connection.kind {
            ConnectionKind::Weaviate(_) => RetrievalBackend::Weaviate,
            ConnectionKind::Elasticsearch(_) => RetrievalBackend::Elasticsearch,
            _ => {
                return Err(Error::ValidationError(format!(
                    "connection '{}' is not a retrieval store (weaviate or elasticsearch)",
                    connection.name
                )))
            }
        };
        Ok(Self {
            connection_id: connection.canonical_config_id().to_string(),
            backend,
        })
    }

    pub fn connection_id(&self) -> &str {
        &self.connection_id
    }

    pub fn backend(&self) -> RetrievalBackend {
        self.backend
    }

    /// Resolves the schema-bound Weaviate reference, reusing the cached runtime
    /// instance for this (connection, schema) pair.
    pub fn weaviate(&self, schema: RetrievalSchema) -> Result<Arc<WeaviateRef>> {
        if self.backend != RetrievalBackend::Weaviate {
            return Err(Error::ValidationError(format!(
                "retrieval connection '{}' is not a weaviate store",
                self.connection_id
            )));
        }
        crate::runtime::block_async(
            RuntimeStorageConnectionManager::shared()
                .get_or_create_weaviate_ref_for_schema(&self.connection_id, schema),
        )
    }

    /// Resolves the schema-bound Elasticsearch reference for this store.
    pub fn elasticsearch(&self, schema: RetrievalSchema) -> Result<Arc<ElasticsearchRef>> {
        if self.backend != RetrievalBackend::Elasticsearch {
            return Err(Error::ValidationError(format!(
                "retrieval connection '{}' is not an elasticsearch store",
                self.connection_id
            )));
        }
        let connections = crate::storage::load_connections()?;
        let connection =
            connections.iter().find(|item| item.id == self.connection_id).ok_or_else(|| {
                Error::ValidationError(format!(
                    "retrieval connection '{}' not found",
                    self.connection_id
                ))
            })?;
        let ConnectionKind::Elasticsearch(elasticsearch) = &connection.kind else {
            return Err(Error::ValidationError(format!(
                "retrieval connection '{}' is not an elasticsearch store",
                self.connection_id
            )));
        };
        Ok(Arc::new(ElasticsearchRef::new(elasticsearch.clone(), schema)?))
    }
}

impl std::fmt::Debug for RetrievalStoreRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RetrievalStoreRef")
            .field("connection_id", &self.connection_id)
            .field("backend", &self.backend.as_str())
            .finish()
    }
}
