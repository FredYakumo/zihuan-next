use std::sync::Arc;
use std::time::Duration;

use zihuan_core::data_refs::{MySqlConfig, SqliteConfig};
use zihuan_core::error::{Error, Result};
use zihuan_core::rag::{BraveSearch, TavilySearch, WebSearchEngine, WebSearchEngineRef};
use zihuan_core::weaviate::WeaviateRef;
use zihuan_graph_engine::data_value::RedisConfig;
use zihuan_graph_engine::object_storage::S3Ref;
use zihuan_graph_engine::DataValue;

use crate::WeaviateCollectionSchema;
use crate::{redis::build_redis_connection_url, ConnectionConfig, ConnectionKind, RuntimeStorageConnectionManager};

pub fn find_connection<'a>(connections: &'a [ConnectionConfig], id: &str) -> Result<&'a ConnectionConfig> {
    connections
        .iter()
        .find(|connection| connection.id == id)
        .ok_or_else(|| Error::ValidationError(format!("connection '{}' not found", id)))
}

pub async fn build_mysql_ref(
    connection_id: Option<&str>,
    connections: &[ConnectionConfig],
) -> Result<Option<Arc<MySqlConfig>>> {
    let Some(connection_id) = connection_id else {
        return Ok(None);
    };
    let _ = connections;
    Ok(Some(
        RuntimeStorageConnectionManager::shared()
            .get_or_create_mysql_ref(connection_id)
            .await?,
    ))
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
    let url = build_redis_connection_url(&redis.url, redis.username.as_deref(), redis.password.as_deref())?;
    Ok(Some(Arc::new(RedisConfig::new(
        Some(url),
        redis.username.clone(),
        redis.password.clone(),
        None,
        None,
    ))))
}

pub fn build_weaviate_ref(
    connection_id: Option<&str>,
    connections: &[ConnectionConfig],
    expected_schema: Option<WeaviateCollectionSchema>,
) -> Result<Option<Arc<WeaviateRef>>> {
    let Some(connection_id) = connection_id else {
        return Ok(None);
    };
    let connection = find_connection(connections, connection_id)?;
    let ConnectionKind::Weaviate(weaviate) = &connection.kind else {
        return Err(Error::ValidationError(format!(
            "connection '{}' is not a weaviate connection",
            connection.name
        )));
    };

    if let Some(expected_schema) = expected_schema {
        if weaviate.collection_schema != expected_schema {
            return Err(Error::ValidationError(format!(
                "weaviate connection '{}' schema mismatch: expected {:?}, got {:?}",
                connection.name, expected_schema, weaviate.collection_schema
            )));
        }
    }
    Ok(Some(zihuan_core::runtime::block_async(
        RuntimeStorageConnectionManager::shared().get_or_create_weaviate_ref(connection_id),
    )?))
}

pub async fn build_s3_ref(connection_id: Option<&str>, connections: &[ConnectionConfig]) -> Result<Option<Arc<S3Ref>>> {
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
) -> Result<Option<Arc<WebSearchEngineRef>>> {
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
        .ok_or_else(|| Error::ValidationError("web_search_engine.api_token must not be empty".to_string()))?;
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
    Ok(Some(Arc::new(WebSearchEngineRef::new(engine_ref))))
}

pub async fn build_sqlite_ref(
    connection_id: Option<&str>,
    connections: &[ConnectionConfig],
) -> Result<Option<Arc<SqliteConfig>>> {
    let Some(connection_id) = connection_id else {
        return Ok(None);
    };
    let _ = connections;
    Ok(Some(
        RuntimeStorageConnectionManager::shared()
            .get_or_create_sqlite_ref(connection_id)
            .await?,
    ))
}

pub async fn resolve_connection_data_value(
    data_type: &zihuan_graph_engine::DataType,
    connection_id: &str,
    connections: &[ConnectionConfig],
) -> Result<Option<DataValue>> {
    match data_type {
        zihuan_graph_engine::DataType::MySqlRef => build_mysql_ref(Some(connection_id), connections)
            .await
            .map(|value| value.map(DataValue::MySqlRef)),
        zihuan_graph_engine::DataType::RedisRef => {
            build_redis_ref(Some(connection_id), connections).map(|value| value.map(DataValue::RedisRef))
        }
        zihuan_graph_engine::DataType::WeaviateRef => {
            build_weaviate_ref(Some(connection_id), connections, None).map(|value| value.map(DataValue::WeaviateRef))
        }
        zihuan_graph_engine::DataType::S3Ref => build_s3_ref(Some(connection_id), connections)
            .await
            .map(|value| value.map(DataValue::S3Ref)),
        zihuan_graph_engine::DataType::WebSearchEngineRef => {
            build_web_search_engine_ref(Some(connection_id), connections)
                .map(|value| value.map(DataValue::WebSearchEngineRef))
        }
        zihuan_graph_engine::DataType::SqliteRef => build_sqlite_ref(Some(connection_id), connections)
            .await
            .map(|value| value.map(DataValue::SqliteRef)),
        _ => Ok(None),
    }
}

fn ensure_endpoint_bypasses_proxy(endpoint: &str) {
    let Some(host) = extract_host(endpoint) else {
        return;
    };

    append_no_proxy_var("NO_PROXY", &host);
    append_no_proxy_var("no_proxy", &host);
}

fn append_no_proxy_var(var_name: &str, host: &str) -> bool {
    let current = std::env::var(var_name).unwrap_or_default();
    let already_present = current.split(',').map(str::trim).any(|entry| entry.eq_ignore_ascii_case(host));
    if already_present {
        return false;
    }

    let updated = if current.trim().is_empty() {
        host.to_string()
    } else {
        format!("{current},{host}")
    };
    std::env::set_var(var_name, updated);
    true
}

fn extract_host(endpoint: &str) -> Option<String> {
    let endpoint = endpoint
        .strip_prefix("http://")
        .or_else(|| endpoint.strip_prefix("https://"))
        .unwrap_or(endpoint);
    let authority = endpoint.split('/').next()?.trim();
    if authority.is_empty() {
        return None;
    }
    let host = authority
        .rsplit_once('@')
        .map(|(_, suffix)| suffix)
        .unwrap_or(authority)
        .split(':')
        .next()
        .unwrap_or("")
        .trim();
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}
