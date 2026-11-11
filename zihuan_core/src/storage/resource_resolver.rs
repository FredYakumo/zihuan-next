use crate::connection_manager::ConnectionManager;
use std::sync::Arc;

use crate::data_refs::RelationalDbConnection;
use crate::error::{Error, Result};
use crate::rag::WebSearchEngine;
use crate::retrieval::{RetrievalSchema, RetrievalStoreRef};
use crate::storage::RedisConfig;
use crate::storage::S3Ref;
use crate::weaviate::WeaviateRef;

use crate::storage::{
    ConnectionConfig, ConnectionKind, ElasticsearchRef, RuntimeConnectionManager,
};

pub fn find_connection<'a>(
    connections: &'a [ConnectionConfig],
    id: &str,
) -> Result<&'a ConnectionConfig> {
    connections
        .iter()
        .find(|connection| connection.id == id)
        .ok_or_else(|| Error::ValidationError(format!("connection '{}' not found", id)))
}

pub async fn build_rdb_ref(
    connection_id: Option<&str>,
    connections: &[ConnectionConfig],
) -> Result<Option<RelationalDbConnection>> {
    let Some(connection_id) = connection_id else {
        return Ok(None);
    };
    super::rdb::build_relational_db_connection_for_connection(connection_id, connections)
        .await
        .map(Some)
}

pub fn build_redis_ref(
    connection_id: Option<&str>,
    connections: &[ConnectionConfig],
) -> Result<Option<Arc<RedisConfig>>> {
    let Some(connection_id) = connection_id else {
        return Ok(None);
    };
    let connection = find_connection(connections, connection_id)?;
    if !matches!(connection.kind, ConnectionKind::Redis(_)) {
        return Err(crate::string_error!("connection '{}' is not Redis", connection_id));
    }
    Ok(Some(
        crate::runtime::block_async(
            RuntimeConnectionManager::shared().get_or_create(connection_id),
        )?
        .try_into()?,
    ))
}

/// Builds a Weaviate reference bound to one retrieval schema's class, reusing the
/// cached runtime instance for that (connection, schema) pair.
pub fn build_weaviate_ref(
    connection_id: Option<&str>,
    connections: &[ConnectionConfig],
    schema: RetrievalSchema,
) -> Result<Option<Arc<WeaviateRef>>> {
    let Some(connection_id) = connection_id else {
        return Ok(None);
    };
    let connection = find_connection(connections, connection_id)?;
    let ConnectionKind::Weaviate(_) = &connection.kind else {
        return Err(Error::ValidationError(format!(
            "connection '{}' is not a weaviate connection",
            connection.name
        )));
    };
    let handle = crate::runtime::block_async(
        RuntimeConnectionManager::shared().get_or_create_variant(connection_id, schema.as_str()),
    )?;
    Ok(Some(handle.try_into()?))
}

/// Builds an Elasticsearch reference bound to one retrieval schema's index.
pub fn build_elasticsearch_ref(
    connection_id: Option<&str>,
    connections: &[ConnectionConfig],
    schema: RetrievalSchema,
) -> Result<Option<Arc<ElasticsearchRef>>> {
    let Some(connection_id) = connection_id else {
        return Ok(None);
    };
    let connection = find_connection(connections, connection_id)?;
    if !matches!(connection.kind, ConnectionKind::Elasticsearch(_)) {
        return Err(crate::string_error!("connection '{}' is not Elasticsearch", connection_id));
    }
    Ok(Some(
        crate::runtime::block_async(
            RuntimeConnectionManager::shared()
                .get_or_create_variant(connection_id, schema.as_str()),
        )?
        .try_into()?,
    ))
}

/// Resolves one retrieval-store connection into a schema-agnostic store reference.
///
/// The connection kind decides the backend; consumers pick the schema at the
/// operation call, so one configured retrieval store backs image semantic search
/// and agent memory without reconfiguration.
pub fn build_retrieval_store_ref(
    connection_id: &str,
    connections: &[ConnectionConfig],
) -> Result<RetrievalStoreRef> {
    RetrievalStoreRef::resolve(connection_id, connections)
}

pub async fn build_s3_ref(
    connection_id: Option<&str>,
    connections: &[ConnectionConfig],
) -> Result<Option<Arc<S3Ref>>> {
    let Some(connection_id) = connection_id else {
        return Ok(None);
    };
    let connection = find_connection(connections, connection_id)?;
    if !matches!(connection.kind, ConnectionKind::Rustfs(_)) {
        return Err(crate::string_error!("connection '{}' is not RustFS", connection_id));
    }
    Ok(Some(
        RuntimeConnectionManager::shared()
            .get_or_create(connection_id)
            .await?
            .try_into()?,
    ))
}

pub fn build_web_search_engine_ref(
    connection_id: Option<&str>,
    connections: &[ConnectionConfig],
) -> Result<Option<Arc<dyn WebSearchEngine>>> {
    let Some(connection_id) = connection_id else {
        return Ok(None);
    };
    let connection = find_connection(connections, connection_id)?;
    if !matches!(connection.kind, ConnectionKind::WebSearchEngine(_)) {
        return Err(crate::string_error!("connection '{}' is not WebSearchEngine", connection_id));
    }
    Ok(Some(
        crate::runtime::block_async(
            RuntimeConnectionManager::shared().get_or_create(connection_id),
        )?
        .try_into()?,
    ))
}

/// Resolves a configured tokenizer through the same lifecycle/cache as network connections.
pub fn build_tokenizer_ref(
    connection_id: &str,
    connections: &[ConnectionConfig],
) -> Result<Arc<crate::nlp::BpeSegmenter>> {
    let connection = find_connection(connections, connection_id)?;
    if !matches!(connection.kind, ConnectionKind::Tokenizer(_)) {
        return Err(crate::string_error!("connection '{}' is not a tokenizer", connection_id));
    }
    crate::runtime::block_async(RuntimeConnectionManager::shared().get_or_create(connection_id))?
        .try_into()
}
