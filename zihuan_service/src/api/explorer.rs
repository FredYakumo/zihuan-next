use log::warn;
use redis::AsyncCommands;
use salvo::prelude::*;
use salvo::writing::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sqlx::Row as SqlxRow;
use zihuan_core::config::role_services::load_role_services;
use zihuan_core::model_inference::inference_function::compact_message::estimate_messages_tokens;
use zihuan_core::model_inference::nn::embedding::embedding_runtime_manager::RuntimeEmbeddingModelManager;
use zihuan_core::url_utils::{
    content_type_from_url, image_content_type_from_bytes, pct_encode, supported_image_content_type,
};

use crate::system_config::load_connections;
use zihuan_core::data_refs::RelationalDbConnection;
use zihuan_core::storage::{
    build_relational_db_connection_for_connection, create_elasticsearch_memory_record,
    create_memory_record_with_vector, delete_elasticsearch_memory_record, delete_memory_record,
    get_elasticsearch_memory_record, get_memory_record, list_recent_memory_keys,
    resource_resolver::{self, build_rdb_ref},
    search_memory_content_by_vector, update_elasticsearch_memory_record,
    update_memory_record_with_vector,
    weaviate::build_weaviate_ref as build_storage_weaviate_ref,
    AgentMemoryAccessContext, AgentMemorySearchHit, AgentMemoryUpsert, ConnectionKind,
    ObjectStorageConfig, RetrievalDatabase, RetrievalSchema, WeaviateClient,
};
use zihuan_ims_service::qq_chat::{list_message_rate_limit_usage, reset_message_rate_limit_usage};

use super::config::{render_bad_request, render_internal_error, render_not_found};

#[derive(Deserialize)]
pub struct QqChatRateLimitUsageQuery {
    pub agent_id: String,
}

#[derive(Deserialize)]
pub struct QqChatRateLimitUsageResetRequest {
    pub agent_id: String,
    pub sender_id: String,
}

#[derive(Serialize)]
struct MysqlExploreResponse {
    records: Vec<MessageRecordResponse>,
    total: i64,
    page: u32,
    page_size: u32,
}

#[derive(Serialize)]
struct MessageRecordResponse {
    message_id: String,
    sender_id: String,
    sender_name: String,
    send_time: String,
    group_id: Option<String>,
    group_name: Option<String>,
    content: String,
    at_target_list: Option<String>,
    media_json: Option<String>,
}

#[handler]
pub async fn query_qq_chat_rate_limit_usage(
    req: &mut Request,
    res: &mut Response,
    _depot: &mut Depot,
) {
    let query: QqChatRateLimitUsageQuery = match req.parse_queries() {
        Ok(query) => query,
        Err(err) => return render_bad_request(res, err.to_string()),
    };
    let agent_id = query.agent_id.trim();
    if agent_id.is_empty() {
        return render_bad_request(res, "agent_id is required".into());
    }

    let connection = match resolve_agent_rdb_connection(agent_id).await {
        Ok(connection) => connection,
        Err(err) => return render_internal_error(res, err),
    };
    match list_message_rate_limit_usage(&connection, agent_id).await {
        Ok(items) => res.render(Json(json!({ "items": items }))),
        Err(err) => render_internal_error(res, err),
    }
}

#[handler]
pub async fn reset_qq_chat_rate_limit_usage(
    req: &mut Request,
    res: &mut Response,
    _depot: &mut Depot,
) {
    let body: QqChatRateLimitUsageResetRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(err) => return render_bad_request(res, err.to_string()),
    };
    let agent_id = body.agent_id.trim();
    let sender_id = body.sender_id.trim();
    if agent_id.is_empty() || sender_id.is_empty() {
        return render_bad_request(res, "agent_id and sender_id are required".into());
    }

    let connection = match resolve_agent_rdb_connection(agent_id).await {
        Ok(connection) => connection,
        Err(err) => return render_internal_error(res, err),
    };
    match reset_message_rate_limit_usage(&connection, agent_id, sender_id).await {
        Ok(deleted) => res.render(Json(json!({ "ok": true, "deleted": deleted }))),
        Err(err) => render_internal_error(res, err),
    }
}

#[handler]
pub async fn query_mysql(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let connection_id = match req.query::<String>("connection_id") {
        Some(id) => id,
        None => return render_bad_request(res, "connection_id is required".into()),
    };
    let page = req.query::<u32>("page").unwrap_or(1).max(1);
    let page_size = req.query::<u32>("page_size").unwrap_or(20).min(100).max(1);

    let connections = match load_connections() {
        Ok(c) => c,
        Err(e) => return render_internal_error(res, e),
    };

    let mysql_ref = match build_rdb_ref(Some(&connection_id), &connections).await {
        Ok(Some(zihuan_core::data_refs::RelationalDbConnection::MySql(r))) => r,
        Ok(Some(_)) => {
            return render_bad_request(res, "connection is not a MySQL connection".into())
        }
        Ok(None) => return render_bad_request(res, "connection not found".into()),
        Err(e) => return render_internal_error(res, e),
    };

    let pool = match mysql_ref.pool.as_ref() {
        Some(p) => p.clone(),
        None => return render_internal_error(res, "mysql pool not available"),
    };

    let message_id = req.query::<String>("message_id");
    let sender_id = req.query::<String>("sender_id");
    let sender_name = req.query::<String>("sender_name");
    let group_id = req.query::<String>("group_id");
    let content = req.query::<String>("content");
    let send_time_start = req.query::<String>("send_time_start");
    let send_time_end = req.query::<String>("send_time_end");

    let mut where_clauses = Vec::new();
    let mut bind_values: Vec<String> = Vec::new();

    if let Some(ref v) = message_id {
        if !v.is_empty() {
            where_clauses.push("message_id LIKE ?".to_string());
            bind_values.push(format!("%{}%", v));
        }
    }
    if let Some(ref v) = sender_id {
        if !v.is_empty() {
            where_clauses.push("sender_id LIKE ?".to_string());
            bind_values.push(format!("%{}%", v));
        }
    }
    if let Some(ref v) = sender_name {
        if !v.is_empty() {
            where_clauses.push("sender_name LIKE ?".to_string());
            bind_values.push(format!("%{}%", v));
        }
    }
    if let Some(ref v) = group_id {
        if !v.is_empty() {
            where_clauses.push("group_id LIKE ?".to_string());
            bind_values.push(format!("%{}%", v));
        }
    }
    if let Some(ref v) = content {
        if !v.is_empty() {
            where_clauses.push("content LIKE ?".to_string());
            bind_values.push(format!("%{}%", v));
        }
    }
    if let Some(ref v) = send_time_start {
        if !v.is_empty() {
            where_clauses.push("send_time >= ?".to_string());
            bind_values.push(v.clone());
        }
    }
    if let Some(ref v) = send_time_end {
        if !v.is_empty() {
            where_clauses.push("send_time <= ?".to_string());
            bind_values.push(v.clone());
        }
    }

    let where_sql = if where_clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", where_clauses.join(" AND "))
    };

    let count_sql = format!("SELECT COUNT(*) as cnt FROM message_record {where_sql}");
    let data_sql = format!(
        "SELECT message_id, sender_id, sender_name, send_time, group_id, group_name, content, at_target_list, media_json FROM message_record {where_sql} ORDER BY send_time DESC, id DESC LIMIT ? OFFSET ?"
    );

    let total: i64 = match {
        let mut query = sqlx::query(&count_sql);
        for val in &bind_values {
            query = query.bind(val);
        }
        async {
            let row = query.fetch_one(&pool).await?;
            let count: i64 = row.try_get("cnt")?;
            Ok::<i64, sqlx::Error>(count)
        }
        .await
    } {
        Ok(t) => t,
        Err(e) => return render_internal_error(res, format!("mysql count query failed: {e}")),
    };

    let offset = (page - 1) * page_size;
    let records = match {
        let mut query = sqlx::query(&data_sql);
        for val in &bind_values {
            query = query.bind(val);
        }
        query = query.bind(page_size).bind(offset);
        async {
            let rows = query.fetch_all(&pool).await?;
            let mut result = Vec::with_capacity(rows.len());
            for row in rows {
                let send_time: chrono::NaiveDateTime = row.try_get("send_time")?;
                let content_raw: String = row.try_get("content").unwrap_or_default();
                let content_display = truncate_preview(&content_raw, 500);
                result.push(MessageRecordResponse {
                    message_id: row.try_get("message_id").unwrap_or_default(),
                    sender_id: row.try_get("sender_id").unwrap_or_default(),
                    sender_name: row.try_get("sender_name").unwrap_or_default(),
                    send_time: send_time.format("%Y-%m-%d %H:%M:%S").to_string(),
                    group_id: row.try_get("group_id").unwrap_or(None),
                    group_name: row.try_get("group_name").unwrap_or(None),
                    content: content_display,
                    at_target_list: row.try_get("at_target_list").unwrap_or(None),
                    media_json: row.try_get("media_json").unwrap_or(None),
                });
            }
            Ok::<Vec<MessageRecordResponse>, sqlx::Error>(result)
        }
        .await
    } {
        Ok(r) => r,
        Err(e) => return render_internal_error(res, format!("mysql query failed: {e}")),
    };

    res.render(Json(MysqlExploreResponse { records, total, page, page_size }));
}

fn truncate_preview(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let preview = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{preview}...")
    } else {
        preview
    }
}

// ── Redis ──────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct RedisExploreResponse {
    keys: Vec<RedisKeyEntry>,
    total: usize,
    page: u32,
    page_size: u32,
    scan_cursor: u64,
}

#[derive(Serialize)]
struct RedisKeyEntry {
    key: String,
    key_type: String,
    ttl: i64,
    value_preview: Option<String>,
}

#[handler]
pub async fn query_redis(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let connection_id = match req.query::<String>("connection_id") {
        Some(id) => id,
        None => return render_bad_request(res, "connection_id is required".into()),
    };
    let pattern = req.query::<String>("pattern").unwrap_or_else(|| "*".to_string());
    let scan_cursor = req.query::<u64>("scan_cursor").unwrap_or(0);
    let page = req.query::<u32>("page").unwrap_or(1).max(1);
    let page_size = req.query::<u32>("page_size").unwrap_or(20).min(100).max(1);

    let connections = match load_connections() {
        Ok(c) => c,
        Err(e) => return render_internal_error(res, e),
    };

    let redis_ref = match resource_resolver::build_redis_ref(Some(&connection_id), &connections) {
        Ok(Some(r)) => r,
        Ok(None) => return render_bad_request(res, "connection not found".into()),
        Err(e) => return render_internal_error(res, e),
    };

    // Ensure connection is established
    {
        let mut cm = redis_ref.redis_cm.lock().await;
        if cm.is_none() {
            if let Some(ref url) = redis_ref.url {
                let client = match redis::Client::open(url.as_str()) {
                    Ok(c) => c,
                    Err(e) => {
                        return render_internal_error(res, format!("redis client open failed: {e}"))
                    }
                };
                match client.get_tokio_connection().await {
                    Ok(conn) => {
                        *cm = Some(conn);
                    }
                    Err(e) => {
                        return render_internal_error(res, format!("redis connect failed: {e}"))
                    }
                }
            } else {
                return render_bad_request(res, "redis connection has no url".into());
            }
        }
    }

    let mut cursor: u64 = scan_cursor;
    let mut all_keys: Vec<String> = Vec::new();
    let needed = (page * page_size) as usize;

    loop {
        let mut cm = redis_ref.redis_cm.lock().await;
        let conn = match cm.as_mut() {
            Some(c) => c,
            None => return render_bad_request(res, "redis connection lost".into()),
        };

        let (new_cursor, batch): (u64, Vec<String>) = match redis::cmd("SCAN")
            .arg(cursor)
            .arg("MATCH")
            .arg(&pattern)
            .arg("COUNT")
            .arg(200u64)
            .query_async::<_, (u64, Vec<String>)>(conn)
            .await
        {
            Ok(result) => result,
            Err(e) => return render_internal_error(res, format!("redis SCAN failed: {e}")),
        };

        drop(cm);
        all_keys.extend(batch);
        cursor = new_cursor;

        if all_keys.len() >= needed || cursor == 0 {
            break;
        }
    }

    let total = all_keys.len();
    let start = ((page - 1) * page_size) as usize;
    let end = (start + page_size as usize).min(total);
    let page_keys: Vec<String> = if start < total {
        all_keys[start..end].to_vec()
    } else {
        Vec::new()
    };

    let mut entries = Vec::with_capacity(page_keys.len());
    for key in &page_keys {
        let mut cm = redis_ref.redis_cm.lock().await;
        let conn = match cm.as_mut() {
            Some(c) => c,
            None => break,
        };

        let key_type: String =
            match redis::cmd("TYPE").arg(key.as_str()).query_async::<_, String>(conn).await {
                Ok(t) => t,
                Err(_) => "unknown".to_string(),
            };

        let ttl: i64 = match conn.ttl::<_, i64>(key).await {
            Ok(t) => t,
            Err(_) => -2,
        };

        let value_preview = if key_type == "string" {
            match conn.get::<_, String>(key).await {
                Ok(v) => {
                    if v.len() > 500 {
                        Some(format!("{}...", &v[..500]))
                    } else {
                        Some(v)
                    }
                }
                Err(_) => None,
            }
        } else {
            None
        };

        entries.push(RedisKeyEntry {
            key: key.clone(),
            key_type,
            ttl,
            value_preview,
        });
    }

    res.render(Json(RedisExploreResponse {
        keys: entries,
        total,
        page,
        page_size,
        scan_cursor: cursor,
    }));
}

// ── RustFS ─────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct RustfsExploreResponse {
    objects: Vec<RustfsObjectEntry>,
    prefixes: Vec<String>,
    total: usize,
    page: u32,
    page_size: u32,
}

#[derive(Serialize)]
struct RustfsObjectEntry {
    key: String,
    size: i64,
    last_modified: Option<String>,
    url: String,
}

#[handler]
pub async fn query_rustfs(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let connection_id = match req.query::<String>("connection_id") {
        Some(id) => id,
        None => return render_bad_request(res, "connection_id is required".into()),
    };
    let prefix = req.query::<String>("prefix").unwrap_or_default();
    let search = req.query::<String>("search");
    let page = req.query::<u32>("page").unwrap_or(1).max(1);
    let page_size = req.query::<u32>("page_size").unwrap_or(20).min(100).max(1);

    let connections = match load_connections() {
        Ok(c) => c,
        Err(e) => return render_internal_error(res, e),
    };

    let s3_ref = match resource_resolver::build_s3_ref(Some(&connection_id), &connections).await {
        Ok(Some(r)) => r,
        Ok(None) => return render_bad_request(res, "connection not found".into()),
        Err(e) => return render_internal_error(res, e),
    };

    let prefix_opt = if prefix.is_empty() {
        None
    } else {
        Some(prefix.as_str())
    };

    let output = match s3_ref.list_objects(prefix_opt, Some("/"), Some(1000)).await {
        Ok(o) => o,
        Err(e) => return render_internal_error(res, format!("S3 list_objects failed: {e}")),
    };

    let common_prefixes: Vec<String> = output
        .common_prefixes()
        .iter()
        .filter_map(|p| p.prefix().map(|s| s.to_string()))
        .collect();

    // collect metas to Generate URL with auth
    let object_metas: Vec<_> = output
        .contents()
        .iter()
        .filter_map(|obj| {
            let key = obj.key()?.to_string();
            let size = obj.size().unwrap_or(0);

            if let Some(ref s) = search {
                if !s.is_empty() && !key.contains(s.as_str()) {
                    return None;
                }
            }

            let last_modified = obj.last_modified().map(|dt| dt.to_string());
            Some((key, size, last_modified))
        })
        .collect();

    //generate URL with auth
    let mut objects = Vec::with_capacity(object_metas.len());
    for (key, size, last_modified) in object_metas {
        let url = s3_ref.object_url_for_key(&key).await.unwrap_or_default();
        objects.push(RustfsObjectEntry { key, size, last_modified, url });
    }

    let total = objects.len();
    let start = ((page - 1) * page_size) as usize;
    if start < total {
        objects = objects.split_off(start);
        objects.truncate(page_size as usize);
    } else {
        objects.clear();
    }

    res.render(Json(RustfsExploreResponse {
        objects,
        prefixes: common_prefixes,
        total,
        page,
        page_size,
    }));
}

// ── Weaviate ───────────────────────────────────────────────────────────

#[derive(Serialize)]
struct WeaviateExploreResponse {
    items: Vec<WeaviateSearchResult>,
    total: usize,
    limit: usize,
    class_name: String,
    schema: RetrievalSchema,
}

#[derive(Serialize)]
struct WeaviateSearchResult {
    object_id: Option<String>,
    distance: Option<f64>,
    properties: Value,
}

#[derive(Deserialize)]
struct AgentMemoryMutationRequest {
    #[serde(alias = "key")]
    title: String,
    value: String,
    #[serde(default)]
    expires_at: Option<String>,
    #[serde(default)]
    sender_id_list: Vec<String>,
    #[serde(default)]
    group_id_list: Vec<String>,
}

#[handler]
pub async fn query_weaviate(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let connection_id = match req.query::<String>("connection_id") {
        Some(id) => id,
        None => return render_bad_request(res, "connection_id is required".into()),
    };
    let embedding_model_ref_id = match req.query::<String>("embedding_model_ref_id") {
        Some(id) => id,
        None => return render_bad_request(res, "embedding_model_ref_id is required".into()),
    };
    let query = req
        .query::<String>("query")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let limit = req.query::<usize>("limit").unwrap_or(10).clamp(1, 50);
    let schema = match req.query::<String>("schema") {
        Some(value) => match RetrievalSchema::parse(&value) {
            Ok(schema) => schema,
            Err(error) => return render_bad_request(res, error.to_string()),
        },
        None => RetrievalSchema::AgentMemory,
    };

    let connections = match load_connections() {
        Ok(c) => c,
        Err(e) => return render_internal_error(res, e),
    };

    let connection = match resource_resolver::find_connection(&connections, &connection_id) {
        Ok(connection) => connection,
        Err(err) => return render_internal_error(res, err),
    };
    let ConnectionKind::Weaviate(weaviate) = &connection.kind else {
        return render_bad_request(res, "connection is not a weaviate connection".into());
    };

    let weaviate_ref = match build_storage_weaviate_ref(
        &weaviate.base_url,
        schema,
        weaviate.username.clone(),
        weaviate.password.clone(),
        weaviate.api_key.clone(),
        weaviate.auth_method,
    ) {
        Ok(weaviate_ref) => weaviate_ref,
        Err(err) => return render_internal_error(res, err),
    };

    let property_names = match list_weaviate_class_properties(&weaviate_ref) {
        Ok(properties) if !properties.is_empty() => properties,
        Ok(_) => return render_internal_error(res, "weaviate class has no readable properties"),
        Err(err) => return render_internal_error(res, err),
    };

    if schema == RetrievalSchema::AgentMemory {
        let access = AgentMemoryAccessContext {
            admin: true,
            skip_expiry_extend: true,
            ..Default::default()
        };
        let items = if let Some(query) = query {
            let embedding_model_ref_id = match req.query::<String>("embedding_model_ref_id") {
                Some(id) => id,
                None => {
                    return render_bad_request(
                        res,
                        "embedding_model_ref_id is required for agent_memory semantic search"
                            .into(),
                    )
                }
            };
            let embedding_model = match RuntimeEmbeddingModelManager::shared()
                .get_or_create_embedding_model(&embedding_model_ref_id)
                .await
            {
                Ok(model) => model,
                Err(err) => return render_internal_error(res, err),
            };
            let vector = match tokio::task::block_in_place(|| embedding_model.inference(&query)) {
                Ok(vector) if !vector.is_empty() => vector,
                Ok(_) => {
                    return render_internal_error(res, "embedding model returned an empty vector")
                }
                Err(err) => return render_internal_error(res, err),
            };
            // The raw connection explorer displays the backend's own distance,
            // so it reads the low-level search instead of the normalized
            // [`RetrievalDatabase`] relevance.
            match search_memory_content_by_vector(&weaviate_ref, &access, &vector, limit) {
                Ok(items) => items,
                Err(err) => return render_internal_error(res, err),
            }
        } else {
            match list_recent_memory_keys(&weaviate_ref, &access, limit, None) {
                Ok(items) => items,
                Err(err) => return render_internal_error(res, err),
            }
        };
        let results = items
            .into_iter()
            .map(|item| WeaviateSearchResult {
                object_id: Some(item.record.object_id),
                distance: item.distance,
                properties: json!({
                    "title": item.record.key,
                    "value": item.record.value,
                    "expires_at": item.record.expires_at,
                    "sender_id_list": item.record.sender_id_list,
                    "group_id_list": item.record.group_id_list,
                    "created_at": item.record.created_at,
                    "updated_at": item.record.updated_at,
                }),
            })
            .collect::<Vec<_>>();
        res.render(Json(WeaviateExploreResponse {
            total: results.len(),
            limit,
            class_name: weaviate_ref.class_name.clone(),
            schema,
            items: results,
        }));
        return;
    }

    let response = if let Some(query) = query {
        let embedding_model = match RuntimeEmbeddingModelManager::shared()
            .get_or_create_embedding_model(&embedding_model_ref_id)
            .await
        {
            Ok(model) => model,
            Err(err) => return render_internal_error(res, err),
        };

        let vector = match tokio::task::block_in_place(|| embedding_model.inference(&query)) {
            Ok(vector) if !vector.is_empty() => vector,
            Ok(_) => return render_internal_error(res, "embedding model returned an empty vector"),
            Err(err) => return render_internal_error(res, err),
        };

        let target_vector = match schema {
            RetrievalSchema::ImageSemantic => Some("description_vector".to_string()),
            RetrievalSchema::AgentMemory | RetrievalSchema::QqMessage => None,
        };

        match weaviate_ref.query_near_vector(
            &weaviate_ref.class_name,
            &vector,
            target_vector.as_deref(),
            limit,
            &property_names,
            true,
            false,
        ) {
            Ok(value) => value,
            Err(err) => return render_internal_error(res, err),
        }
    } else {
        match weaviate_ref.query_all(&weaviate_ref.class_name, limit, &property_names) {
            Ok(value) => value,
            Err(err) => return render_internal_error(res, err),
        }
    };

    let items = response
        .get("data")
        .and_then(|value| value.get("Get"))
        .and_then(|value| value.get(&weaviate_ref.class_name))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(weaviate_search_result_from_value)
        .collect::<Vec<_>>();

    res.render(Json(WeaviateExploreResponse {
        total: items.len(),
        limit,
        class_name: weaviate_ref.class_name.clone(),
        schema,
        items,
    }));
}

#[handler]
pub async fn create_agent_memory(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let connection_id = match req.query::<String>("connection_id") {
        Some(id) => id,
        None => return render_bad_request(res, "connection_id is required".into()),
    };
    let embedding_model_ref_id = match req.query::<String>("embedding_model_ref_id") {
        Some(id) => id,
        None => return render_bad_request(res, "embedding_model_ref_id is required".into()),
    };
    let body: AgentMemoryMutationRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(err) => return render_bad_request(res, err.to_string()),
    };
    let backend = match resolve_agent_memory_backend(&connection_id) {
        Ok(backend) => backend,
        Err(err) => return render_internal_error(res, err),
    };
    let embedding_model = match RuntimeEmbeddingModelManager::shared()
        .get_or_create_embedding_model(&embedding_model_ref_id)
        .await
    {
        Ok(model) => model,
        Err(err) => return render_internal_error(res, err),
    };
    let vector = match tokio::task::block_in_place(|| {
        embedding_model.inference(&format!("{}\n{}", body.title.trim(), body.value.trim()))
    }) {
        Ok(vector) => vector,
        Err(err) => return render_internal_error(res, err),
    };
    let input = AgentMemoryUpsert {
        key: body.title,
        value: body.value,
        expires_at: body.expires_at,
        sender_id_list: body.sender_id_list,
        group_id_list: body.group_id_list,
    };
    let result = match &backend {
        AgentMemoryBackendRef::Weaviate(reference) => {
            create_memory_record_with_vector(reference, &input, Some(vector))
        }
        AgentMemoryBackendRef::Elasticsearch(reference) => {
            create_elasticsearch_memory_record(reference, &input, vector)
        }
    };
    match result {
        Ok(record) => res.render(Json(record)),
        Err(err) => render_internal_error(res, err),
    }
}

#[handler]
pub async fn update_agent_memory(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let connection_id = match req.query::<String>("connection_id") {
        Some(id) => id,
        None => return render_bad_request(res, "connection_id is required".into()),
    };
    let embedding_model_ref_id = match req.query::<String>("embedding_model_ref_id") {
        Some(id) => id,
        None => return render_bad_request(res, "embedding_model_ref_id is required".into()),
    };
    let object_id = req.param::<String>("object_id").unwrap_or_default();
    if object_id.trim().is_empty() {
        return render_bad_request(res, "object_id is required".into());
    }
    let body: AgentMemoryMutationRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(err) => return render_bad_request(res, err.to_string()),
    };
    let backend = match resolve_agent_memory_backend(&connection_id) {
        Ok(backend) => backend,
        Err(err) => return render_internal_error(res, err),
    };
    let embedding_model = match RuntimeEmbeddingModelManager::shared()
        .get_or_create_embedding_model(&embedding_model_ref_id)
        .await
    {
        Ok(model) => model,
        Err(err) => return render_internal_error(res, err),
    };
    let vector = match tokio::task::block_in_place(|| {
        embedding_model.inference(&format!("{}\n{}", body.title.trim(), body.value.trim()))
    }) {
        Ok(vector) => vector,
        Err(err) => return render_internal_error(res, err),
    };
    let input = AgentMemoryUpsert {
        key: body.title,
        value: body.value,
        expires_at: body.expires_at,
        sender_id_list: body.sender_id_list,
        group_id_list: body.group_id_list,
    };
    let result = match &backend {
        AgentMemoryBackendRef::Weaviate(reference) => {
            update_memory_record_with_vector(reference, &object_id, &input, Some(vector))
        }
        AgentMemoryBackendRef::Elasticsearch(reference) => {
            update_elasticsearch_memory_record(reference, &object_id, &input, vector)
        }
    };
    match result {
        Ok(record) => res.render(Json(record)),
        Err(err) => render_internal_error(res, err),
    }
}

#[handler]
pub async fn delete_agent_memory(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let connection_id = match req.query::<String>("connection_id") {
        Some(id) => id,
        None => return render_bad_request(res, "connection_id is required".into()),
    };
    let object_id = req.param::<String>("object_id").unwrap_or_default();
    if object_id.trim().is_empty() {
        return render_bad_request(res, "object_id is required".into());
    }
    let backend = match resolve_agent_memory_backend(&connection_id) {
        Ok(backend) => backend,
        Err(err) => return render_internal_error(res, err),
    };
    let result = match &backend {
        AgentMemoryBackendRef::Weaviate(reference) => delete_memory_record(reference, &object_id),
        AgentMemoryBackendRef::Elasticsearch(reference) => {
            delete_elasticsearch_memory_record(reference, &object_id)
        }
    };
    match result {
        Ok(()) => res.render(Json(serde_json::json!({ "ok": true }))),
        Err(err) => render_internal_error(res, err),
    }
}

#[handler]
pub async fn get_agent_memory(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let connection_id = match req.query::<String>("connection_id") {
        Some(id) => id,
        None => return render_bad_request(res, "connection_id is required".into()),
    };
    let object_id = req.param::<String>("object_id").unwrap_or_default();
    if object_id.trim().is_empty() {
        return render_bad_request(res, "object_id is required".into());
    }
    let backend = match resolve_agent_memory_backend(&connection_id) {
        Ok(backend) => backend,
        Err(err) => return render_internal_error(res, err),
    };
    let result = match &backend {
        AgentMemoryBackendRef::Weaviate(reference) => get_memory_record(reference, &object_id),
        AgentMemoryBackendRef::Elasticsearch(reference) => {
            get_elasticsearch_memory_record(reference, &object_id)
        }
    };
    match result {
        Ok(record) => res.render(Json(record)),
        Err(err) => render_internal_error(res, err),
    }
}

fn list_weaviate_class_properties(
    weaviate_ref: &zihuan_core::weaviate::WeaviateRef,
) -> zihuan_core::error::Result<Vec<String>> {
    let schema = weaviate_ref.schema()?;
    Ok(schema
        .get("classes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|class| {
            class
                .get("class")
                .and_then(Value::as_str)
                .map(|name| name == weaviate_ref.class_name)
                .unwrap_or(false)
        })
        .and_then(|class| class.get("properties"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|property| {
            property
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(ToOwned::to_owned)
        })
        .collect())
}

fn weaviate_search_result_from_value(value: Value) -> WeaviateSearchResult {
    let object_id = value
        .get("_additional")
        .and_then(|extra| extra.get("id"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let distance = value
        .get("_additional")
        .and_then(|extra| extra.get("distance"))
        .and_then(Value::as_f64);

    let mut properties = match value {
        Value::Object(object) => object,
        _ => Map::new(),
    };
    properties.remove("_additional");

    WeaviateSearchResult {
        object_id,
        distance,
        properties: Value::Object(properties),
    }
}

enum AgentMemoryBackendRef {
    Weaviate(std::sync::Arc<zihuan_core::weaviate::WeaviateRef>),
    Elasticsearch(std::sync::Arc<zihuan_core::storage::ElasticsearchRef>),
}

/// Resolves the agent-memory collection for whichever retrieval backend hosts the service's
/// store, so admin mutations work on both Weaviate and Elasticsearch.
fn resolve_agent_memory_backend(
    connection_id: &str,
) -> zihuan_core::error::Result<AgentMemoryBackendRef> {
    let connections = load_connections()?;
    let connection = resource_resolver::find_connection(&connections, connection_id)?;
    match &connection.kind {
        ConnectionKind::Weaviate(weaviate) => {
            Ok(AgentMemoryBackendRef::Weaviate(build_storage_weaviate_ref(
                &weaviate.base_url,
                RetrievalSchema::AgentMemory,
                weaviate.username.clone(),
                weaviate.password.clone(),
                weaviate.api_key.clone(),
                weaviate.auth_method,
            )?))
        }
        ConnectionKind::Elasticsearch(_) => {
            let reference = resource_resolver::build_elasticsearch_ref(
                Some(connection_id),
                &connections,
                RetrievalSchema::AgentMemory,
            )?
            .ok_or_else(|| {
                zihuan_core::error::Error::ValidationError(
                    "memory connection is not configured".to_string(),
                )
            })?;
            Ok(AgentMemoryBackendRef::Elasticsearch(reference))
        }
        _ => Err(zihuan_core::error::Error::ValidationError(
            "connection is not a retrieval store (weaviate or elasticsearch)".to_string(),
        )),
    }
}

#[derive(Serialize)]
struct ServiceMemoryResponse {
    items: Vec<ServiceMemoryItem>,
    backend: &'static str,
    mutable: bool,
}

#[derive(Serialize)]
struct ServiceMemoryItem {
    #[serde(flatten)]
    record: zihuan_core::storage::AgentMemoryRecord,
    match_kinds: Vec<&'static str>,
    score: Option<f64>,
    backend: &'static str,
    mutable: bool,
}

#[derive(Serialize)]
struct ServiceImageResponse {
    items: Vec<ServiceImageItem>,
    backend: &'static str,
}

#[derive(Serialize)]
struct ServiceImageItem {
    object_id: String,
    media_id: Option<String>,
    name: Option<String>,
    description: Option<String>,
    original_source: Option<String>,
    rustfs_path: Option<String>,
    mime_type: Option<String>,
    source: Option<String>,
    url: Option<String>,
    match_kinds: Vec<&'static str>,
    score: Option<f64>,
    backend: &'static str,
}

#[handler]
pub async fn query_service_messages(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let service_id = req.param::<String>("service_id").unwrap_or_default();
    let connection = match resolve_agent_rdb_connection(&service_id).await {
        Ok(connection) => connection,
        Err(error) => return render_bad_request(res, error.to_string()),
    };
    let page = req.query::<u32>("page").unwrap_or(1).max(1);
    let page_size = req.query::<u32>("page_size").unwrap_or(20).clamp(1, 100);
    let filters = [
        "message_id",
        "sender_id",
        "sender_name",
        "group_id",
        "content",
        "send_time_start",
        "send_time_end",
    ]
    .into_iter()
    .map(|key| (key, req.query::<String>(key).unwrap_or_default()))
    .filter(|(_, value)| !value.trim().is_empty())
    .collect::<Vec<_>>();
    match query_service_message_rows(connection, &filters, page, page_size).await {
        Ok((records, total)) => {
            res.render(Json(MysqlExploreResponse { records, total, page, page_size }))
        }
        Err(error) => render_internal_error(res, error),
    }
}

async fn query_service_message_rows(
    connection: RelationalDbConnection,
    filters: &[(&str, String)],
    page: u32,
    page_size: u32,
) -> zihuan_core::error::Result<(Vec<MessageRecordResponse>, i64)> {
    let mut where_clauses = Vec::new();
    let mut values = Vec::new();
    for (key, value) in filters {
        let clause = match *key {
            "message_id" | "sender_id" | "sender_name" | "group_id" | "content" => {
                format!("{key} LIKE ?")
            }
            "send_time_start" => "send_time >= ?".to_string(),
            "send_time_end" => "send_time <= ?".to_string(),
            _ => continue,
        };
        where_clauses.push(clause);
        values.push(if key.starts_with("send_time") {
            value.clone()
        } else {
            format!("%{value}%")
        });
    }
    let where_sql = if where_clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", where_clauses.join(" AND "))
    };
    let count_sql = format!("SELECT COUNT(*) AS cnt FROM message_record {where_sql}");
    let data_sql = format!("SELECT message_id, sender_id, sender_name, send_time, group_id, group_name, content, at_target_list, media_json FROM message_record {where_sql} ORDER BY send_time DESC, id DESC LIMIT ? OFFSET ?");
    macro_rules! fetch_records {
        ($pool:expr, $time:expr) => {{
            let mut count_query = sqlx::query(&count_sql);
            for value in &values {
                count_query = count_query.bind(value);
            }
            let total: i64 = count_query.fetch_one($pool).await?.try_get("cnt")?;
            let mut data_query = sqlx::query(&data_sql);
            for value in &values {
                data_query = data_query.bind(value);
            }
            let rows =
                data_query.bind(page_size).bind((page - 1) * page_size).fetch_all($pool).await?;
            let mut records = Vec::with_capacity(rows.len());
            for row in rows {
                records.push(MessageRecordResponse {
                    message_id: row.try_get("message_id").unwrap_or_default(),
                    sender_id: row.try_get("sender_id").unwrap_or_default(),
                    sender_name: row.try_get("sender_name").unwrap_or_default(),
                    send_time: $time(&row),
                    group_id: row.try_get("group_id").unwrap_or(None),
                    group_name: row.try_get("group_name").unwrap_or(None),
                    content: truncate_preview(
                        &row.try_get::<String, _>("content").unwrap_or_default(),
                        500,
                    ),
                    at_target_list: row.try_get("at_target_list").unwrap_or(None),
                    media_json: row.try_get("media_json").unwrap_or(None),
                });
            }
            Ok((records, total))
        }};
    }
    match connection {
        RelationalDbConnection::MySql(config) => {
            let pool = config
                .pool
                .as_ref()
                .ok_or_else(|| zihuan_core::string_error!("mysql pool not available"))?;
            fetch_records!(pool, |row: &sqlx::mysql::MySqlRow| row
                .try_get::<chrono::NaiveDateTime, _>("send_time")
                .map(|time| time.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_default())
        }
        RelationalDbConnection::Sqlite(config) => {
            let pool = config
                .pool
                .as_ref()
                .ok_or_else(|| zihuan_core::string_error!("sqlite pool not available"))?;
            fetch_records!(pool, |row: &sqlx::sqlite::SqliteRow| row
                .try_get::<String, _>("send_time")
                .unwrap_or_default())
        }
    }
}

#[handler]
pub async fn query_service_memories(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let service_id = req.param::<String>("service_id").unwrap_or_default();
    let query = req
        .query::<String>("query")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let limit = req.query::<usize>("limit").unwrap_or(20).clamp(1, 50);
    let (agent, connections) = match load_service_and_connections(&service_id) {
        Ok(value) => value,
        Err(error) => return render_bad_request(res, error.to_string()),
    };
    let (retrieval_store_id, embedding_id) = zihuan_service::role::retrieval_store_config(&agent);
    let access = AgentMemoryAccessContext {
        admin: true,
        skip_expiry_extend: true,
        ..Default::default()
    };
    let Some(connection_id) = retrieval_store_id else {
        return render_bad_request(res, "Service has no memory store configured".into());
    };
    let Some(kind) = retrieval_backend_for_connection(&connections, &connection_id) else {
        return render_bad_request(res, "memory connection is not configured".into());
    };
    match kind {
        ConnectionKind::Weaviate(_) => {
            let reference = match resource_resolver::build_weaviate_ref(
                Some(&connection_id),
                &connections,
                RetrievalSchema::AgentMemory,
            ) {
                Ok(Some(reference)) => reference,
                Ok(None) => {
                    return render_bad_request(res, "memory connection is not configured".into())
                }
                Err(error) => return render_internal_error(res, error),
            };
            let hits = match query.as_deref() {
                Some(value) => {
                    let Some(model_id) = embedding_id.as_deref() else {
                        return render_bad_request(
                            res,
                            "Service has no memory embedding model configured".into(),
                        );
                    };
                    let vector = match service_memory_query_vector(model_id, value).await {
                        Ok(vector) => vector,
                        Err(error) => return render_internal_error(res, error),
                    };
                    match reference.search_memory(&access, value, &vector, limit) {
                        Ok(hits) => hits,
                        Err(error) => return render_internal_error(res, error),
                    }
                }
                None => match reference.list_memory(&access, limit, None) {
                    Ok(hits) => hits,
                    Err(error) => return render_internal_error(res, error),
                },
            };
            res.render(Json(ServiceMemoryResponse {
                items: memory_items(
                    hits,
                    if query.is_some() {
                        "semantic"
                    } else {
                        "recent"
                    },
                    "weaviate",
                    true,
                ),
                backend: "weaviate",
                mutable: true,
            }));
        }
        ConnectionKind::Elasticsearch(_) => {
            let reference = match resource_resolver::build_elasticsearch_ref(
                Some(&connection_id),
                &connections,
                RetrievalSchema::AgentMemory,
            ) {
                Ok(Some(reference)) => reference,
                Ok(None) => {
                    return render_bad_request(res, "memory connection is not configured".into())
                }
                Err(error) => return render_internal_error(res, error),
            };
            let hits = match query.as_deref() {
                Some(value) => {
                    let Some(model_id) = embedding_id.as_deref() else {
                        return render_bad_request(
                            res,
                            "Service has no memory embedding model configured".into(),
                        );
                    };
                    let vector = match service_memory_query_vector(model_id, value).await {
                        Ok(vector) => vector,
                        Err(error) => return render_internal_error(res, error),
                    };
                    match reference.search_memory(&access, value, &vector, limit) {
                        Ok(hits) => hits,
                        Err(error) => return render_internal_error(res, error),
                    }
                }
                None => match reference.list_memory(&access, limit, None) {
                    Ok(hits) => hits,
                    Err(error) => return render_internal_error(res, error),
                },
            };
            res.render(Json(ServiceMemoryResponse {
                items: memory_items(
                    hits,
                    if query.is_some() { "hybrid" } else { "recent" },
                    "elasticsearch",
                    true,
                ),
                backend: "elasticsearch",
                mutable: true,
            }));
        }
        _ => return render_bad_request(res, "memory connection is not a retrieval store".into()),
    }
}

#[handler]
pub async fn query_service_images(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let service_id = req.param::<String>("service_id").unwrap_or_default();
    let name_query = req
        .query::<String>("name_query")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let description_query = req
        .query::<String>("description_query")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let limit = req.query::<usize>("limit").unwrap_or(20).clamp(1, 50);
    let (agent, connections) = match load_service_and_connections(&service_id) {
        Ok(value) => value,
        Err(error) => return render_bad_request(res, error.to_string()),
    };
    let (retrieval_store_id, embedding_id) = zihuan_service::role::retrieval_store_config(&agent);
    let object_storage_connection_id = zihuan_service::role::object_storage_connection_id(&agent);
    let object_storage_connection_id = object_storage_connection_id.as_deref();
    let (name_vector, description_vector) = match embedding_vectors(
        embedding_id.as_deref(),
        name_query.as_deref(),
        description_query.as_deref(),
    )
    .await
    {
        Ok(value) => value,
        Err(error) => return render_internal_error(res, error),
    };
    let Some(connection_id) = retrieval_store_id else {
        return render_bad_request(res, "Service has no image store configured".into());
    };
    let Some(kind) = retrieval_backend_for_connection(&connections, &connection_id) else {
        return render_bad_request(res, "image connection is not configured".into());
    };
    if matches!(kind, ConnectionKind::Weaviate(_)) {
        let reference = match resource_resolver::build_weaviate_ref(
            Some(&connection_id),
            &connections,
            RetrievalSchema::ImageSemantic,
        ) {
            Ok(Some(reference)) => reference,
            Ok(None) => {
                return render_bad_request(res, "image connection is not configured".into())
            }
            Err(error) => return render_internal_error(res, error),
        };
        let hits = match reference.search_images(
            name_query.as_deref(),
            description_query.as_deref(),
            name_vector.as_deref(),
            description_vector.as_deref(),
            limit,
        ) {
            Ok(hits) => hits,
            Err(error) => return render_internal_error(res, error),
        };
        let items = hits
            .into_iter()
            .map(|hit| {
                image_item(
                    hit.object_id,
                    &hit.properties,
                    hit.match_kinds,
                    hit.score,
                    "weaviate",
                    object_storage_connection_id,
                )
            })
            .collect();
        res.render(Json(ServiceImageResponse { items, backend: "weaviate" }));
        return;
    }
    let reference = match resource_resolver::build_elasticsearch_ref(
        Some(&connection_id),
        &connections,
        RetrievalSchema::ImageSemantic,
    ) {
        Ok(Some(reference)) => reference,
        Ok(None) => return render_bad_request(res, "image connection is not configured".into()),
        Err(error) => return render_internal_error(res, error),
    };
    let hits = match reference.search_images(
        name_query.as_deref(),
        description_query.as_deref(),
        name_vector.as_deref(),
        description_vector.as_deref(),
        limit,
    ) {
        Ok(hits) => hits,
        Err(error) => return render_internal_error(res, error),
    };
    let items = hits
        .into_iter()
        .map(|hit| {
            image_item(
                hit.object_id,
                &hit.properties,
                hit.match_kinds,
                hit.score,
                "elasticsearch",
                object_storage_connection_id,
            )
        })
        .collect();
    res.render(Json(ServiceImageResponse { items, backend: "elasticsearch" }));
}

#[derive(Serialize)]
struct ServiceChatUserItem {
    sender_id: String,
    sender_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    group_name: Option<String>,
    /// Estimated tokens of the sender's live context window, computed by the same estimator
    /// as the context page's usage panel; an unreadable window reports 0.
    context_tokens: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_time: Option<String>,
}

#[derive(Serialize)]
struct ServiceChatUsersResponse {
    backend: &'static str,
    users: Vec<ServiceChatUserItem>,
}

/// The QQ users that currently have a live conversation context with this role agent.
///
/// The senders come from the agent's real conversation cache (so no bot and every entry is
/// viewable); display names, groups, and timestamps are enriched from `message_record`.
#[handler]
pub async fn query_service_chat_users(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let service_id = req.param::<String>("service_id").unwrap_or_default();
    let Some(cache) = zihuan_core::qq_context::qq_chat_history_cache(&service_id) else {
        // No running agent: nothing to read. Report an empty list rather than an error.
        return res.render(Json(ServiceChatUsersResponse { backend: "memory", users: Vec::new() }));
    };
    let backend = match cache.backend().await {
        zihuan_core::qq_context::QqChatContextBackend::Redis => "redis",
        zihuan_core::qq_context::QqChatContextBackend::Memory => "memory",
    };
    let sender_ids = cache.list_sender_ids().await;
    if sender_ids.is_empty() {
        return res.render(Json(ServiceChatUsersResponse { backend, users: Vec::new() }));
    }

    let profiles = match resolve_agent_rdb_connection(&service_id).await {
        Ok(connection) => load_message_sender_profiles(&connection).await.unwrap_or_default(),
        Err(_) => Vec::new(),
    };

    let mut users: Vec<ServiceChatUserItem> = Vec::with_capacity(sender_ids.len());
    for sender_id in sender_ids {
        let profile = profiles.iter().find(|row| row.0 == sender_id);
        let context_tokens = match cache.get_messages(&sender_id).await {
            Ok(messages) => estimate_messages_tokens(&messages),
            Err(err) => {
                // Report the sender anyway; a read failure must not look like an empty context.
                warn!("[explorer] failed to read context for {sender_id}: {err}");
                0
            }
        };
        users.push(ServiceChatUserItem {
            sender_name: profile
                .map(|row| row.1.clone())
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| sender_id.clone()),
            group_name: profile.and_then(|row| row.2.clone()),
            context_tokens,
            last_time: profile.and_then(|row| row.3.clone()),
            sender_id,
        });
    }
    users.sort_by(|left, right| right.last_time.cmp(&left.last_time));
    res.render(Json(ServiceChatUsersResponse { backend, users }));
}

/// `(sender_id, sender_name, group_name, last_send_time)` per sender.
async fn load_message_sender_profiles(
    connection: &RelationalDbConnection,
) -> zihuan_core::error::Result<Vec<(String, String, Option<String>, Option<String>)>> {
    const SQL: &str = "SELECT sender_id, MAX(sender_name) AS sender_name, MAX(group_name) AS group_name, MAX(send_time) AS last_time FROM message_record GROUP BY sender_id";
    let mut rows = Vec::new();
    match connection {
        RelationalDbConnection::MySql(config) => {
            let pool = config
                .pool
                .as_ref()
                .ok_or_else(|| zihuan_core::string_error!("mysql pool not available"))?;
            let records = sqlx::query(SQL).fetch_all(pool).await?;
            for row in records {
                rows.push((
                    row.try_get::<String, _>("sender_id").unwrap_or_default(),
                    row.try_get::<String, _>("sender_name").unwrap_or_default(),
                    row.try_get::<Option<String>, _>("group_name").unwrap_or(None),
                    row.try_get::<chrono::NaiveDateTime, _>("last_time")
                        .map(|time| time.format("%Y-%m-%d %H:%M:%S").to_string())
                        .ok(),
                ));
            }
        }
        RelationalDbConnection::Sqlite(config) => {
            let pool = config
                .pool
                .as_ref()
                .ok_or_else(|| zihuan_core::string_error!("sqlite pool not available"))?;
            let records = sqlx::query(SQL).fetch_all(pool).await?;
            for row in records {
                rows.push((
                    row.try_get::<String, _>("sender_id").unwrap_or_default(),
                    row.try_get::<String, _>("sender_name").unwrap_or_default(),
                    row.try_get::<Option<String>, _>("group_name").unwrap_or(None),
                    row.try_get::<String, _>("last_time").ok(),
                ));
            }
        }
    }
    Ok(rows)
}

#[derive(Serialize)]
struct ContextBubble {
    role: &'static str,
    text: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    media: Vec<ContextMedia>,
    #[serde(skip_serializing_if = "Option::is_none")]
    usage: Option<zihuan_core::model_inference::llm::TokenUsage>,
    estimated_tokens: usize,
}

#[derive(Serialize)]
struct ContextMedia {
    media_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    size_bytes: Option<u64>,
}

#[derive(Serialize)]
struct ServiceChatContextResponse {
    backend: &'static str,
    sender_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_prompt: Option<String>,
    bubbles: Vec<ContextBubble>,
    /// Complete recorded turns, including tool calls, stages and per-turn usage. The UI renders
    /// this flow; `bubbles` mirrors the conversation cache for context totals.
    turns: Vec<ContextTurn>,
    /// Turns archived in the relational database, oldest first, excluding those still in `turns`.
    archived: Vec<ContextTurn>,
    estimated_context_tokens: usize,
    /// Context window of the service's primary model, used by the UI as the usage ceiling.
    #[serde(skip_serializing_if = "Option::is_none")]
    context_limit_tokens: Option<usize>,
    input_tokens: usize,
    output_tokens: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    average_cache_hit_rate: Option<f64>,
}

#[derive(Serialize)]
struct ContextTurn {
    turn_id: String,
    created_at: String,
    user_text: String,
    /// Group the user message was sent in; absent for private chats.
    #[serde(skip_serializing_if = "Option::is_none")]
    group_name: Option<String>,
    #[serde(default)]
    is_group: bool,
    /// The message the user quoted, rendered as a collapsible quote block inside the user bubble.
    #[serde(skip_serializing_if = "Option::is_none")]
    quoted: Option<zihuan_core::qq_context::QqChatQuotedMessage>,
    media: Vec<ContextMedia>,
    entries: Vec<ContextTurnEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    assistant_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    usage: Option<zihuan_core::model_inference::llm::TokenUsage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_ms: Option<u128>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output_tokens_per_second: Option<f64>,
    /// This turn continues the previous one with queued steer messages; the UI keeps both in
    /// one block instead of drawing a divider.
    #[serde(default)]
    continues_previous: bool,
    finished: bool,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ContextTurnEntry {
    Stage {
        at: String,
        stage: String,
        /// Stable id of a stage updated in place (embedding start → finish).
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        detail: serde_json::Value,
    },
    ToolCall {
        at: String,
        call_id: String,
        name: String,
        arguments: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u128>,
    },
    /// One step of a sub-agent's own flow, nested under the tool call named by `path`.
    SubAgent {
        at: String,
        path: Vec<String>,
        event: zihuan_core::qq_context::QqChatSubAgentEntry,
    },
}

fn context_media_from_ref(
    media: &zihuan_core::qq_context::MediaRef,
    object_storage_connection_id: Option<&str>,
) -> ContextMedia {
    ContextMedia {
        media_id: media.media_id.clone(),
        url: image_preview_url(
            media.rustfs_path.as_deref(),
            media.original_source.as_deref(),
            media.mime_type.as_deref(),
            object_storage_connection_id,
        ),
        mime_type: media.mime_type.clone(),
        name: media.name.clone(),
        size_bytes: media.size_bytes,
    }
}

fn archived_turn_to_context(
    turn: zihuan_ims_service::qq_chat::ArchivedTurn,
    object_storage_connection_id: Option<&str>,
) -> ContextTurn {
    use zihuan_core::model_inference::llm::TokenUsage;
    ContextTurn {
        turn_id: turn.turn_id,
        created_at: turn.created_at,
        user_text: turn.user_text,
        group_name: turn.group_name,
        is_group: turn.is_group,
        quoted: turn.quoted,
        media: turn
            .media
            .iter()
            .map(|media| context_media_from_ref(media, object_storage_connection_id))
            .collect(),
        entries: turn
            .entries
            .into_iter()
            .map(|entry| match entry {
                zihuan_core::qq_context::QqChatTurnEntry::Stage { at, stage, detail, id } => {
                    ContextTurnEntry::Stage { at, stage, id, detail }
                }
                zihuan_core::qq_context::QqChatTurnEntry::ToolCall {
                    at,
                    call_id,
                    name,
                    arguments,
                    result,
                    duration_ms,
                } => ContextTurnEntry::ToolCall {
                    at,
                    call_id,
                    name,
                    arguments,
                    result,
                    duration_ms,
                },
                zihuan_core::qq_context::QqChatTurnEntry::SubAgent { at, path, event } => {
                    ContextTurnEntry::SubAgent { at, path, event }
                }
            })
            .collect(),
        assistant_text: turn.assistant_text,
        reasoning_text: turn.reasoning_text,
        usage: match (turn.prompt_tokens, turn.completion_tokens) {
            (None, None) => None,
            (prompt, completion) => Some(TokenUsage {
                prompt_tokens: prompt.map(|value| value as usize),
                cached_prompt_tokens: turn.cached_prompt_tokens.map(|value| value as usize),
                prompt_cache_miss_tokens: turn.prompt_cache_miss_tokens.map(|value| value as usize),
                completion_tokens: completion.map(|value| value as usize),
                total_tokens: turn.total_tokens.map(|value| value as usize),
            }),
        },
        generation_ms: turn.generation_ms.map(|value| value as u128),
        output_tokens_per_second: turn.output_tokens_per_second,
        continues_previous: turn.continues_previous,
        finished: turn.finished,
    }
}

/// The live conversation context the role agent currently holds for one sender, in message
/// order. Read straight from the agent's cache; the system prompt is split out of the first
/// user message for readability.
#[handler]
pub async fn query_service_chat_context(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let service_id = req.param::<String>("service_id").unwrap_or_default();
    let sender_id = req
        .query::<String>("sender_id")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let Some(sender_id) = sender_id else {
        return render_bad_request(res, "sender_id is required".into());
    };
    let Some(cache) = zihuan_core::qq_context::qq_chat_history_cache(&service_id) else {
        return render_bad_request(
            res,
            "Service is not running; no live context is available".into(),
        );
    };
    let backend = match cache.backend().await {
        zihuan_core::qq_context::QqChatContextBackend::Redis => "redis",
        zihuan_core::qq_context::QqChatContextBackend::Memory => "memory",
    };
    let messages = match cache.get_messages(&sender_id).await {
        Ok(messages) => messages,
        Err(error) => return render_internal_error(res, error.to_string()),
    };
    // The service's own RustFS connection is what cached the images, so its id is carried on
    // every preview URL for the media proxy to resolve. The same service config supplies the
    // primary model whose context window bounds the usage bar.
    let service_config = load_service_and_connections(&service_id).ok();
    let object_storage_connection_id = service_config
        .as_ref()
        .and_then(|(agent, _)| zihuan_service::role::object_storage_connection_id(agent));
    let context_limit_tokens = service_config
        .as_ref()
        .and_then(|(agent, _)| primary_context_limit_tokens(agent));
    let object_storage_connection_id = object_storage_connection_id.as_deref();
    // Object sizes are a label on the preview, so a missing or unreachable store just leaves
    // them blank instead of failing the context request.
    let object_storage = match object_storage_connection_id {
        Some(id) => resource_resolver::build_s3_ref(Some(id), &[]).await.ok().flatten(),
        None => None,
    };

    let mut bubbles = Vec::with_capacity(messages.len());
    let mut system_prompt: Option<String> = None;
    let mut input_tokens = 0usize;
    let mut output_tokens = 0usize;
    let mut cache_hit_rates: Vec<f64> = Vec::new();
    for message in &messages {
        let role = match message.role {
            zihuan_core::model_inference::llm::MessageRole::Assistant => "assistant",
            zihuan_core::model_inference::llm::MessageRole::Tool => "tool",
            _ => "user",
        };
        let mut media = Vec::new();
        let mut media_paths: Vec<Option<String>> = Vec::new();
        for part in &message.parts {
            if let zihuan_core::message_part::MessagePart::Image { media: image }
            | zihuan_core::message_part::MessagePart::Video { media: image } = part
            {
                let rustfs_path =
                    Some(image.rustfs_path.clone()).filter(|value| !value.trim().is_empty());
                media.push(ContextMedia {
                    media_id: image.media_id.clone(),
                    url: image_preview_url(
                        Some(image.rustfs_path.as_str()),
                        Some(image.original_source.as_str()),
                        image.mime_type.as_deref(),
                        object_storage_connection_id,
                    ),
                    mime_type: image.mime_type.clone(),
                    name: image.name.clone(),
                    size_bytes: None,
                });
                media_paths.push(rustfs_path);
            }
        }
        if let Some(storage) = object_storage.as_deref() {
            for (item, path) in media.iter_mut().zip(media_paths.iter()) {
                if let Some(path) = path {
                    item.size_bytes = storage.head_object_size(path).await;
                }
            }
        }
        let mut text = message.content_text_owned().unwrap_or_default();
        if role == "user" {
            let (prefix, body) = split_user_context(&text);
            if system_prompt.is_none() {
                if let Some(prefix) = prefix {
                    system_prompt = Some(prefix);
                }
            }
            if let Some(body) = body {
                text = body;
            }
        }
        if let Some(usage) = &message.usage {
            input_tokens += usage.prompt_tokens.unwrap_or(0);
            output_tokens += usage.completion_tokens.unwrap_or(0);
            if let (Some(cached), Some(prompt)) = (usage.cached_prompt_tokens, usage.prompt_tokens)
            {
                if prompt > 0 {
                    cache_hit_rates.push(cached as f64 / prompt as f64);
                }
            }
        }
        bubbles.push(ContextBubble {
            role,
            estimated_tokens: estimate_messages_tokens(std::slice::from_ref(message)),
            text,
            media,
            usage: message.usage.clone(),
        });
    }

    let average_cache_hit_rate = if cache_hit_rates.is_empty() {
        None
    } else {
        Some(cache_hit_rates.iter().sum::<f64>() / cache_hit_rates.len() as f64)
    };
    let turns: Vec<ContextTurn> =
        zihuan_core::qq_context::qq_chat_turn_records(&service_id, &sender_id)
            .await
            .into_iter()
            .map(|turn| ContextTurn {
                turn_id: turn.turn_id,
                created_at: turn.created_at,
                user_text: turn.user_text,
                group_name: turn.group_name,
                is_group: turn.is_group,
                quoted: turn.quoted,
                media: turn
                    .media
                    .iter()
                    .map(|media| context_media_from_ref(media, object_storage_connection_id))
                    .collect(),
                entries: turn
                    .entries
                    .into_iter()
                    .map(|entry| match entry {
                        zihuan_core::qq_context::QqChatTurnEntry::Stage {
                            at,
                            stage,
                            detail,
                            id,
                        } => ContextTurnEntry::Stage { at, stage, id, detail },
                        zihuan_core::qq_context::QqChatTurnEntry::ToolCall {
                            at,
                            call_id,
                            name,
                            arguments,
                            result,
                            duration_ms,
                        } => ContextTurnEntry::ToolCall {
                            at,
                            call_id,
                            name,
                            arguments,
                            result,
                            duration_ms,
                        },
                        zihuan_core::qq_context::QqChatTurnEntry::SubAgent { at, path, event } => {
                            ContextTurnEntry::SubAgent { at, path, event }
                        }
                    })
                    .collect(),
                assistant_text: turn.assistant_text,
                reasoning_text: turn.reasoning_text,
                usage: turn.usage,
                generation_ms: turn.generation_ms,
                output_tokens_per_second: turn.output_tokens_per_second,
                continues_previous: turn.continues_previous,
                finished: turn.finished,
            })
            .collect();
    // Archived turns come from the relational database; the live turns above come from the
    // runtime context (memory/Redis). The UI shows the archive first, separated from the live
    // window by a divider.
    let archived: Vec<ContextTurn> = match resolve_agent_rdb_connection(&service_id).await {
        Ok(connection) => {
            // One-time migration: turns recorded by a previous build sat in a Redis key that is
            // no longer read as live context. Move them into the archive so they stay visible.
            for legacy in
                zihuan_core::qq_context::take_legacy_turn_records(&service_id, &sender_id).await
            {
                zihuan_ims_service::qq_chat::persist_archived_context_turn(&connection, &legacy);
            }
            let live_ids: Vec<String> = turns.iter().map(|turn| turn.turn_id.clone()).collect();
            zihuan_ims_service::qq_chat::load_archived_context_turns(
                &connection,
                &service_id,
                &sender_id,
                500,
            )
            .map(|rows| {
                rows.into_iter()
                    .filter(|turn| !live_ids.contains(&turn.turn_id))
                    .map(|turn| archived_turn_to_context(turn, object_storage_connection_id))
                    .collect()
            })
            .unwrap_or_default()
        }
        Err(_) => Vec::new(),
    };
    res.render(Json(ServiceChatContextResponse {
        backend,
        sender_id,
        system_prompt,
        estimated_context_tokens: estimate_messages_tokens(&messages),
        context_limit_tokens,
        bubbles,
        turns,
        archived,
        input_tokens,
        output_tokens,
        average_cache_hit_rate,
    }));
}

/// Clear the live conversation context of one sender.
///
/// The runtime cache holds the current window only; finished turns were already archived in
/// the relational database, so dropping the cache and the process turn log makes the view show
/// them under history instead of as current context.
#[handler]
pub async fn clear_service_chat_context(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let service_id = req.param::<String>("service_id").unwrap_or_default();
    let sender_id = req
        .query::<String>("sender_id")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let Some(sender_id) = sender_id else {
        return render_bad_request(res, "sender_id is required".into());
    };
    let Some(cache) = zihuan_core::qq_context::qq_chat_history_cache(&service_id) else {
        return render_bad_request(
            res,
            "Service is not running; no live context is available".into(),
        );
    };
    // Archive whatever the process turn log still holds before dropping it: the requested
    // operation moves the current context into history, and a turn whose finish-time archive
    // write failed would otherwise disappear from the view entirely.
    if let Ok(connection) = resolve_agent_rdb_connection(&service_id).await {
        for turn in zihuan_core::qq_context::qq_chat_turn_records(&service_id, &sender_id).await {
            if turn.finished {
                zihuan_ims_service::qq_chat::persist_archived_context_turn(&connection, &turn);
            }
        }
    }
    // Clears the conversation history and the preprompt transcript of this sender, the same
    // reset the QQ runtime performs when the user starts a new conversation.
    if let Err(error) = zihuan_ims_service::qq_chat::clear_history(&cache, &sender_id) {
        return render_internal_error(res, error.to_string());
    }
    zihuan_core::qq_context::clear_qq_chat_turn_records(&service_id, &sender_id);
    res.render(Json(json!({ "cleared": true })));
}

/// Split an assembled QQ user message into its leading system/instruction prefix and the
/// remaining body. The body still carries the per-turn template; both halves are best-effort.
fn split_user_context(text: &str) -> (Option<String>, Option<String>) {
    const LABEL: &str = "\n[Current User Message]\n";
    let Some(index) = text.find(LABEL) else {
        return (None, None);
    };
    let prefix = text[..index].trim();
    let mut body = text[index + LABEL.len()..].to_string();
    if let Some(cut) = body.find("\n\n[Processing Instructions]") {
        body.truncate(cut);
    }
    let prefix = (!prefix.is_empty()).then(|| prefix.to_string());
    let body = body.trim();
    let body = (!body.is_empty()).then(|| body.to_string());
    (prefix, body)
}

/// Streams one cached media object from object storage so the WebUI can
/// preview images whose `rustfs_path` is not directly reachable.
#[handler]
pub async fn serve_media_image(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
    let Some(object_key) = req
        .query::<String>("path")
        .map(|value| value.trim().trim_start_matches('/').to_string())
        .filter(|value| !value.is_empty() && !value.contains(".."))
    else {
        return render_bad_request(res, "path is required".into());
    };
    // Prefer the connection the caller names: the preview URL was built from the service that
    // cached the object, so it points at the bucket that actually holds it.
    let storage = match req
        .query::<String>("connection_id")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        Some(connection_id) => {
            match resource_resolver::build_s3_ref(Some(&connection_id), &[]).await {
                Ok(Some(storage)) => storage,
                Ok(None) => return render_not_found(res, "object storage connection not found"),
                Err(error) => return render_internal_error(res, error.to_string()),
            }
        }
        None => match ObjectStorageConfig::from_env() {
            Some(storage) => storage.into_inner(),
            None => return render_internal_error(res, "object storage is not configured"),
        },
    };
    let bytes = match storage.as_ref().get_object_bytes(&object_key).await {
        Ok(bytes) => bytes,
        Err(error) => {
            log::warn!("[explorer] failed to load media object '{}': {}", object_key, error);
            return render_not_found(res, "media object not found");
        }
    };
    let content_type = req
        .query::<String>("mime_type")
        .and_then(|value| supported_image_content_type(&value).map(ToOwned::to_owned))
        .or_else(|| image_content_type_from_bytes(&bytes).map(ToOwned::to_owned))
        .unwrap_or_else(|| content_type_from_url(&object_key).to_string());
    res.headers_mut()
        .insert(salvo::http::header::CONTENT_TYPE, content_type.parse().unwrap());
    res.write_body(bytes).ok();
}

fn load_service_and_connections(
    service_id: &str,
) -> zihuan_core::error::Result<(
    zihuan_core::role::service_config::RoleServiceConfig,
    Vec<zihuan_core::storage::ConnectionConfig>,
)> {
    let agent = load_role_services()?
        .into_iter()
        .find(|item| item.id == service_id)
        .ok_or_else(|| zihuan_core::string_error!("Service '{}' not found", service_id))?;
    Ok((agent, load_connections()?))
}

/// Context window of the service's primary model, or `None` when it cannot be resolved.
fn primary_context_limit_tokens(
    agent: &zihuan_core::role::service_config::RoleServiceConfig,
) -> Option<usize> {
    let llm_ref_id = zihuan_service::role::primary_llm_ref_id(&agent.role_service_type);
    let llm_refs = zihuan_core::config::llm_refs::load_llm_refs().ok()?;
    zihuan_core::agent::resource_resolver::resolve_llm_service_config(
        llm_ref_id.as_deref(),
        &llm_refs,
    )
    .ok()
    .map(|config| config.context_length)
    .filter(|length| *length > 0)
}

/// The backend serving one retrieval connection, or `None` when it is not a
/// retrieval store.
fn retrieval_backend_for_connection(
    connections: &[zihuan_core::storage::ConnectionConfig],
    connection_id: &str,
) -> Option<ConnectionKind> {
    let connection = resource_resolver::find_connection(connections, connection_id).ok()?;
    matches!(connection.kind, ConnectionKind::Weaviate(_) | ConnectionKind::Elasticsearch(_))
        .then(|| connection.kind.clone())
}

async fn embedding_vectors(
    model_id: Option<&str>,
    name: Option<&str>,
    description: Option<&str>,
) -> zihuan_core::error::Result<(Option<Vec<f32>>, Option<Vec<f32>>)> {
    let Some(model_id) = model_id.filter(|value| !value.is_empty()) else {
        return Ok((None, None));
    };
    if name.is_none() && description.is_none() {
        return Ok((None, None));
    }
    let model = RuntimeEmbeddingModelManager::shared()
        .get_or_create_embedding_model(model_id)
        .await?;
    let name_vector = name.map(|value| model.inference(value)).transpose()?;
    let description_vector = description.map(|value| model.inference(value)).transpose()?;
    Ok((name_vector, description_vector))
}

fn memory_items(
    hits: Vec<AgentMemorySearchHit>,
    kind: &'static str,
    backend: &'static str,
    mutable: bool,
) -> Vec<ServiceMemoryItem> {
    hits.into_iter()
        .map(|hit| ServiceMemoryItem {
            record: hit.record,
            match_kinds: vec![kind],
            score: hit.distance,
            backend,
            mutable,
        })
        .collect()
}

async fn service_memory_query_vector(
    embedding_model_id: &str,
    query: &str,
) -> zihuan_core::error::Result<Vec<f32>> {
    let model = RuntimeEmbeddingModelManager::shared()
        .get_or_create_embedding_model(embedding_model_id)
        .await?;
    tokio::task::block_in_place(|| model.inference(query))
}

/// URL the WebUI can use to preview this image: prefer the cached object
/// storage copy served by the media proxy, then the original remote source.
fn image_preview_url(
    rustfs_path: Option<&str>,
    original_source: Option<&str>,
    mime_type: Option<&str>,
    connection_id: Option<&str>,
) -> Option<String> {
    if let Some(path) = rustfs_path.map(str::trim).filter(|value| !value.is_empty()) {
        let mut url = format!("/api/explorer/media-image?path={}", pct_encode(path));
        if let Some(mime) = mime_type.and_then(supported_image_content_type) {
            url.push_str("&mime_type=");
            url.push_str(mime);
        }
        if let Some(connection_id) = connection_id.filter(|value| !value.trim().is_empty()) {
            url.push_str("&connection_id=");
            url.push_str(&pct_encode(connection_id));
        }
        return Some(url);
    }
    original_source
        .filter(|value| {
            value.starts_with("http://")
                || value.starts_with("https://")
                || value.starts_with("data:image/")
        })
        .map(ToOwned::to_owned)
}

fn image_item(
    object_id: String,
    value: &Value,
    match_kinds: Vec<&'static str>,
    score: Option<f64>,
    backend: &'static str,
    object_storage_connection_id: Option<&str>,
) -> ServiceImageItem {
    let string = |key| value.get(key).and_then(Value::as_str).map(ToOwned::to_owned);
    let rustfs_path = string("rustfs_path");
    let mime_type = string("mime_type");
    let original_source = string("original_source");
    let url = image_preview_url(
        rustfs_path.as_deref(),
        original_source.as_deref(),
        mime_type.as_deref(),
        object_storage_connection_id,
    );
    ServiceImageItem {
        object_id,
        media_id: string("media_id"),
        name: string("name"),
        description: string("description"),
        original_source,
        url,
        rustfs_path,
        mime_type,
        source: string("source"),
        match_kinds,
        score,
        backend,
    }
}

async fn resolve_agent_rdb_connection(
    agent_id: &str,
) -> zihuan_core::error::Result<zihuan_core::data_refs::RelationalDbConnection> {
    let agents = load_role_services()?;
    let agent = agents
        .into_iter()
        .find(|item| item.id == agent_id)
        .ok_or_else(|| zihuan_core::string_error!("agent '{}' not found", agent_id))?;
    let config = zihuan_service::role::qq_chat_of(&agent.role_service_type).map_err(|_| {
        zihuan_core::string_error!("agent '{}' is not a QQ Chat Agent Service", agent_id)
    })?;
    let rdb_id = config.resolved_rdb_id().ok_or_else(|| {
        zihuan_core::string_error!("QQ Chat Agent Service '{}' has no rdb_id configured", agent_id)
    })?;
    let connections = load_connections()?;
    build_relational_db_connection_for_connection(rdb_id, &connections).await
}
