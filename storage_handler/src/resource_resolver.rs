use std::sync::Arc;
use std::time::Duration;

use sqlx::mysql::MySqlPoolOptions;
use zihuan_core::error::{Error, Result};
use zihuan_graph_engine::data_value::{MySqlConfig, RedisConfig, TavilyRef};
use zihuan_graph_engine::database::weaviate::WeaviateRef;
use zihuan_graph_engine::database::{WeaviateImageCollectionNode, WeaviateNode};
use zihuan_graph_engine::object_storage::S3Ref;
use zihuan_graph_engine::{DataValue, Node};

use crate::{ConnectionConfig, ConnectionKind};

pub fn find_connection<'a>(
    connections: &'a [ConnectionConfig],
    id: &str,
) -> Result<&'a ConnectionConfig> {
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
    let connection = find_connection(connections, connection_id)?;
    let ConnectionKind::Mysql(mysql) = &connection.kind else {
        return Err(Error::ValidationError(format!(
            "connection '{}' is not a mysql connection",
            connection.name
        )));
    };
    let pool = MySqlPoolOptions::new()
        .max_connections(10)
        .min_connections(1)
        .connect(&mysql.url)
        .await?;
    Ok(Some(Arc::new(MySqlConfig {
        url: Some(mysql.url.clone()),
        reconnect_max_attempts: None,
        reconnect_interval_secs: None,
        pool: Some(pool),
        runtime_handle: Some(tokio::runtime::Handle::current()),
    })))
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
    Ok(Some(Arc::new(RedisConfig::new(
        Some(redis.url.clone()),
        None,
        None,
    ))))
}

pub fn build_weaviate_ref(
    connection_id: Option<&str>,
    connections: &[ConnectionConfig],
    image_collection: bool,
) -> Result<Option<Arc<WeaviateRef>>> {
    use std::collections::HashMap;

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

    let mut node: Box<dyn Node> = if image_collection {
        Box::new(WeaviateImageCollectionNode::new(
            "__service__",
            "__service__",
        ))
    } else {
        Box::new(WeaviateNode::new("__service__", "__service__"))
    };

    let outputs = node.execute(HashMap::from([
        (
            "base_url".to_string(),
            DataValue::String(weaviate.base_url.clone()),
        ),
        (
            "class_name".to_string(),
            DataValue::String(weaviate.class_name.clone()),
        ),
    ]))?;

    match outputs.get("weaviate_ref") {
        Some(DataValue::WeaviateRef(reference)) => Ok(Some(reference.clone())),
        _ => Err(Error::StringError(
            "weaviate node did not return weaviate_ref".to_string(),
        )),
    }
}

pub async fn build_s3_ref(
    connection_id: Option<&str>,
    connections: &[ConnectionConfig],
) -> Result<Option<Arc<S3Ref>>> {
    let Some(connection_id) = connection_id else {
        return Ok(None);
    };
    let connection = find_connection(connections, connection_id)?;
    let ConnectionKind::Rustfs(rustfs) = &connection.kind else {
        return Err(Error::ValidationError(format!(
            "connection '{}' is not a rustfs connection",
            connection.name
        )));
    };

    let s3_ref = Arc::new(S3Ref {
        endpoint: rustfs.endpoint.clone(),
        bucket: rustfs.bucket.clone(),
        access_key: rustfs.access_key.clone(),
        secret_key: rustfs.secret_key.clone(),
        region: rustfs.region.clone(),
        public_base_url: rustfs.public_base_url.clone(),
        path_style: rustfs.path_style,
    });

    ensure_endpoint_bypasses_proxy(&s3_ref.endpoint);
    s3_ref.ensure_bucket_exists().await?;
    Ok(Some(s3_ref))
}

pub fn build_tavily_ref(
    connection_id: Option<&str>,
    connections: &[ConnectionConfig],
) -> Result<Option<Arc<TavilyRef>>> {
    let Some(connection_id) = connection_id else {
        return Ok(None);
    };
    let connection = find_connection(connections, connection_id)?;
    let ConnectionKind::Tavily(tavily) = &connection.kind else {
        return Err(Error::ValidationError(format!(
            "connection '{}' is not a tavily connection",
            connection.name
        )));
    };
    if tavily.api_token.trim().is_empty() {
        return Err(Error::ValidationError(
            "tavily.api_token must not be empty".to_string(),
        ));
    }
    Ok(Some(Arc::new(TavilyRef::new(
        tavily.api_token.clone(),
        Duration::from_secs(tavily.timeout_secs),
    ))))
}

pub async fn resolve_connection_data_value(
    data_type: &zihuan_graph_engine::DataType,
    connection_id: &str,
    connections: &[ConnectionConfig],
) -> Result<Option<DataValue>> {
    match data_type {
        zihuan_graph_engine::DataType::MySqlRef => {
            build_mysql_ref(Some(connection_id), connections)
                .await
                .map(|value| value.map(DataValue::MySqlRef))
        }
        zihuan_graph_engine::DataType::RedisRef => {
            build_redis_ref(Some(connection_id), connections)
                .map(|value| value.map(DataValue::RedisRef))
        }
        zihuan_graph_engine::DataType::WeaviateRef => {
            build_weaviate_ref(Some(connection_id), connections, false)
                .map(|value| value.map(DataValue::WeaviateRef))
        }
        zihuan_graph_engine::DataType::S3Ref => build_s3_ref(Some(connection_id), connections)
            .await
            .map(|value| value.map(DataValue::S3Ref)),
        zihuan_graph_engine::DataType::TavilyRef => {
            build_tavily_ref(Some(connection_id), connections)
                .map(|value| value.map(DataValue::TavilyRef))
        }
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
    let already_present = current
        .split(',')
        .map(str::trim)
        .any(|entry| entry.eq_ignore_ascii_case(host));
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
