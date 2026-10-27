use std::sync::Arc;
use std::time::Duration;

use crate::data_refs::RelationalDbConnection;
use crate::error::{Error, Result};
use crate::rag::{BraveSearch, TavilySearch, WebSearchEngine};
use crate::retrieval::{RetrievalSchema, RetrievalStoreRef};
use crate::storage::RedisConfig;
use crate::storage::S3Ref;
use crate::weaviate::WeaviateRef;

use crate::storage::{
    redis::build_redis_connection_url, ConnectionConfig, ConnectionKind, ElasticsearchRef,
    RuntimeStorageConnectionManager,
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
    let connection = find_connection(connections, connection_id)?;
    match &connection.kind {
        ConnectionKind::Mysql(_) => Ok(Some(RelationalDbConnection::MySql(
            RuntimeStorageConnectionManager::shared()
                .get_or_create_mysql_ref(connection_id)
                .await?,
        ))),
        ConnectionKind::Sqlite(_) => Ok(Some(RelationalDbConnection::Sqlite(
            RuntimeStorageConnectionManager::shared()
                .get_or_create_sqlite_ref(connection_id)
                .await?,
        ))),
        _ => Err(Error::ValidationError(format!(
            "connection '{}' is not a relational database connection",
            connection.name
        ))),
    }
}

pub fn build_redis_ref(
    connection_id: Option<&str>,
    connections: &[ConnectionConfig],
) -> Result<Option<Arc<RedisConfig>>> {
    let Some(connection_id) = connection_id else {
        return Ok(None);
    };
    let connection = find_connection(connections, connection_id)?;
    let ConnectionKind::Redis(redis) = &connection.kind else {
        return Err(Error::ValidationError(format!(
            "connection '{}' is not a redis connection",
            connection.name
        )));
    };
    let url = build_redis_connection_url(
        &redis.url,
        redis.username.as_deref(),
        redis.password.as_deref(),
    )?;
    Ok(Some(Arc::new(RedisConfig::new(
        Some(url),
        redis.username.clone(),
        redis.password.clone(),
        None,
        None,
    ))))
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
    Ok(Some(crate::runtime::block_async(
        RuntimeStorageConnectionManager::shared()
            .get_or_create_weaviate_ref_for_schema(connection_id, schema),
    )?))
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
    let ConnectionKind::Elasticsearch(elasticsearch) = &connection.kind else {
        return Err(Error::ValidationError(format!(
            "connection '{}' is not an elasticsearch connection",
            connection.name
        )));
    };
    Ok(Some(Arc::new(ElasticsearchRef::new(elasticsearch.clone(), schema)?)))
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
    let _ = connections;
    Ok(Some(
        RuntimeStorageConnectionManager::shared()
            .get_or_create_s3_ref(connection_id)
            .await?,
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
    let ConnectionKind::WebSearchEngine(engine) = &connection.kind else {
        return Err(Error::ValidationError(format!(
            "connection '{}' is not a web search engine connection",
            connection.name
        )));
    };
    let api_token = engine
        .api_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            Error::ValidationError("web_search_engine.api_token must not be empty".to_string())
        })?;
    let engine_ref = match engine.provider.as_str() {
        "tavily" => Arc::new(TavilySearch::new(
            api_token.to_string(),
            Duration::from_secs(engine.timeout_secs),
        )) as Arc<dyn WebSearchEngine>,
        "brave" => Arc::new(BraveSearch::new(
            api_token.to_string(),
            Duration::from_secs(engine.timeout_secs),
        )) as Arc<dyn WebSearchEngine>,
        other => {
            return Err(Error::ValidationError(format!(
                "unsupported web search engine provider: {}",
                other
            )))
        }
    };
    Ok(Some(engine_ref))
}
