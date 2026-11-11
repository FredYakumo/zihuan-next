use crate::connection::Connection;
use crate::data_refs::{MySqlConfig, SqliteConfig};
use crate::error::{Error, Result};
use crate::nlp::BpeSegmenter;
use crate::rag::{PluginWebSearchEngine, WebSearchEngine};
use crate::retrieval::RetrievalSchema;
use crate::storage::{ConnectionConfig, ConnectionKind, ElasticsearchRef, RedisConfig, S3Ref};
use crate::weaviate::WeaviateRef;
use std::sync::Arc;

#[derive(Clone)]
pub enum ConnectionHandle {
    MySql(Arc<MySqlConfig>),
    Sqlite(Arc<SqliteConfig>),
    S3(Arc<S3Ref>),
    Weaviate(Arc<WeaviateRef>),
    Redis(Arc<RedisConfig>),
    Elasticsearch(Arc<ElasticsearchRef>),
    WebSearchEngine(Arc<dyn WebSearchEngine>),
    Tokenizer(Arc<BpeSegmenter>),
}

impl ConnectionHandle {
    pub fn connection(&self) -> &dyn Connection {
        match self {
            Self::MySql(value) => value.as_ref(),
            Self::Sqlite(value) => value.as_ref(),
            Self::S3(value) => value.as_ref(),
            Self::Weaviate(value) => value.as_ref(),
            Self::Redis(value) => value.as_ref(),
            Self::Elasticsearch(value) => value.as_ref(),
            Self::WebSearchEngine(value) => value.as_ref(),
            Self::Tokenizer(value) => value.as_ref(),
        }
    }

    pub(super) fn has_external_refs(&self) -> bool {
        match self {
            Self::MySql(value) => Arc::strong_count(value) > 1,
            Self::Sqlite(value) => Arc::strong_count(value) > 1,
            Self::S3(value) => Arc::strong_count(value) > 1,
            Self::Weaviate(value) => Arc::strong_count(value) > 1,
            Self::Redis(value) => Arc::strong_count(value) > 1,
            Self::Elasticsearch(value) => Arc::strong_count(value) > 1,
            Self::WebSearchEngine(value) => Arc::strong_count(value) > 1,
            Self::Tokenizer(value) => Arc::strong_count(value) > 1,
        }
    }
}

impl ConnectionHandle {
    /// Dispatch configuration to the resource that owns its construction details.
    pub(super) async fn from_config(config: &ConnectionConfig, variant: &str) -> Result<Self> {
        match &config.kind {
            ConnectionKind::Mysql(value) => {
                Ok(Self::MySql(Arc::new(MySqlConfig::from_config(value).await?)))
            }
            ConnectionKind::Sqlite(value) => {
                Ok(Self::Sqlite(Arc::new(SqliteConfig::from_config(value).await?)))
            }
            ConnectionKind::MockRdb => Ok(Self::Sqlite(super::mock_rdb::connect().await?)),
            ConnectionKind::Redis(value) => {
                Ok(Self::Redis(Arc::new(RedisConfig::from_config(value).await?)))
            }
            ConnectionKind::Elasticsearch(value) => Ok(Self::Elasticsearch(Arc::new(
                ElasticsearchRef::from_config(value, RetrievalSchema::parse(variant)?).await?,
            ))),
            ConnectionKind::WebSearchEngine(value) => Ok(Self::WebSearchEngine(Arc::new(
                PluginWebSearchEngine::from_config(value).await?,
            ))),
            ConnectionKind::Tokenizer(value) => {
                Ok(Self::Tokenizer(Arc::new(BpeSegmenter::from_config(value).await?)))
            }
            ConnectionKind::Rustfs(value) => {
                Ok(Self::S3(Arc::new(S3Ref::from_config(value).await?)))
            }
            ConnectionKind::Weaviate(value) => Ok(Self::Weaviate(Arc::new(
                WeaviateRef::from_config(value, RetrievalSchema::parse(variant)?).await?,
            ))),
            ConnectionKind::BotAdapter(_) => Err(crate::string_error!(
                "connection '{}' is managed by ActiveAdapterManager",
                config.name
            )),
        }
    }
}

impl TryFrom<ConnectionHandle> for Arc<MySqlConfig> {
    type Error = Error;
    fn try_from(handle: ConnectionHandle) -> Result<Self> {
        match handle {
            ConnectionHandle::MySql(value) => Ok(value),
            _ => Err(crate::string_error!("expected a MySql connection")),
        }
    }
}

impl TryFrom<ConnectionHandle> for Arc<SqliteConfig> {
    type Error = Error;
    fn try_from(handle: ConnectionHandle) -> Result<Self> {
        match handle {
            ConnectionHandle::Sqlite(value) => Ok(value),
            _ => Err(crate::string_error!("expected a Sqlite connection")),
        }
    }
}

impl TryFrom<ConnectionHandle> for Arc<S3Ref> {
    type Error = Error;
    fn try_from(handle: ConnectionHandle) -> Result<Self> {
        match handle {
            ConnectionHandle::S3(value) => Ok(value),
            _ => Err(crate::string_error!("expected a S3 connection")),
        }
    }
}

impl TryFrom<ConnectionHandle> for Arc<WeaviateRef> {
    type Error = Error;
    fn try_from(handle: ConnectionHandle) -> Result<Self> {
        match handle {
            ConnectionHandle::Weaviate(value) => Ok(value),
            _ => Err(crate::string_error!("expected a Weaviate connection")),
        }
    }
}

impl TryFrom<ConnectionHandle> for Arc<RedisConfig> {
    type Error = Error;
    fn try_from(handle: ConnectionHandle) -> Result<Self> {
        match handle {
            ConnectionHandle::Redis(value) => Ok(value),
            _ => Err(crate::string_error!("expected a Redis connection")),
        }
    }
}

impl TryFrom<ConnectionHandle> for Arc<ElasticsearchRef> {
    type Error = Error;
    fn try_from(handle: ConnectionHandle) -> Result<Self> {
        match handle {
            ConnectionHandle::Elasticsearch(value) => Ok(value),
            _ => Err(crate::string_error!("expected a Elasticsearch connection")),
        }
    }
}

impl TryFrom<ConnectionHandle> for Arc<dyn WebSearchEngine> {
    type Error = Error;
    fn try_from(handle: ConnectionHandle) -> Result<Self> {
        match handle {
            ConnectionHandle::WebSearchEngine(value) => Ok(value),
            _ => Err(crate::string_error!("expected a WebSearchEngine connection")),
        }
    }
}

impl TryFrom<ConnectionHandle> for Arc<BpeSegmenter> {
    type Error = Error;
    fn try_from(handle: ConnectionHandle) -> Result<Self> {
        match handle {
            ConnectionHandle::Tokenizer(value) => Ok(value),
            _ => Err(crate::string_error!("expected a Tokenizer connection")),
        }
    }
}
