use zihuan_core::model_inference::nn::embedding::embedding_runtime_manager::RuntimeEmbeddingModelManager;
use zihuan_core::agent::service_config::{RoleServiceConfig, RoleServiceType};
use zihuan_core::config::role_services::load_role_services;
use redis::AsyncCommands;
use salvo::prelude::*;
use salvo::writing::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sqlx::Row as SqlxRow;

use crate::system_config::load_connections;
use zihuan_core::storage::{
    build_relational_db_connection_for_connection, create_memory_record_with_vector, delete_memory_record,
    get_memory_record, list_elasticsearch_memory_keys, list_recent_memory_keys,
    resource_resolver::{self, build_rdb_ref},
    search_elasticsearch_images, search_elasticsearch_memory, search_memory_content_by_vector,
    update_memory_record_with_vector,
    weaviate::build_weaviate_ref as build_storage_weaviate_ref,
    AgentMemoryAccessContext, AgentMemorySearchHit, AgentMemoryUpsert, ConnectionKind, WeaviateClient,
    WeaviateCollectionSchema,
};
use zihuan_core::data_refs::RelationalDbConnection;
use zihuan_ims_service::qq_chat::{list_message_rate_limit_usage, reset_message_rate_limit_usage};

use super::config::{render_bad_request, render_internal_error};

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
pub async fn query_qq_chat_rate_limit_usage(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
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
pub async fn reset_qq_chat_rate_limit_usage(req: &mut Request, res: &mut Response, _depot: &mut Depot) {
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
        Ok(Some(_)) => return render_bad_request(res, "connection is not a MySQL connection".into()),
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

    res.render(Json(MysqlExploreResponse {
        records,
        total,
        page,
        page_size,
    }));
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
                    Err(e) => return render_internal_error(res, format!("redis client open failed: {e}")),
                };
                match client.get_tokio_connection().await {
                    Ok(conn) => {
                        *cm = Some(conn);
                    }
                    Err(e) => return render_internal_error(res, format!("redis connect failed: {e}")),
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

        let key_type: String = match redis::cmd("TYPE").arg(key.as_str()).query_async::<_, String>(conn).await {
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

    let prefix_opt = if prefix.is_empty() { None } else { Some(prefix.as_str()) };

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
    collection_schema: WeaviateCollectionSchema,
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
    let collection_schema = weaviate.collection_schema;

    let weaviate_ref = match build_storage_weaviate_ref(
        &weaviate.base_url,
        &weaviate.class_name,
        weaviate.username.clone(),
        weaviate.password.clone(),
        weaviate.api_key.clone(),
        weaviate.auth_method,
        collection_schema,
    ) {
        Ok(weaviate_ref) => weaviate_ref,
        Err(err) => return render_internal_error(res, err),
    };

    let property_names = match list_weaviate_class_properties(&weaviate_ref) {
        Ok(properties) if !properties.is_empty() => properties,
        Ok(_) => return render_internal_error(res, "weaviate class has no readable properties"),
        Err(err) => return render_internal_error(res, err),
    };

    if collection_schema == WeaviateCollectionSchema::AgentMemory {
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
                        "embedding_model_ref_id is required for agent_memory semantic search".into(),
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
                Ok(_) => return render_internal_error(res, "embedding model returned an empty vector"),
                Err(err) => return render_internal_error(res, err),
            };
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
            collection_schema,
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

        let target_vector = match collection_schema {
            WeaviateCollectionSchema::ImageSemantic => Some("description_vector".to_string()),
            WeaviateCollectionSchema::AgentMemory => None,
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
        collection_schema,
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
    let weaviate_ref = match resolve_agent_memory_weaviate_ref(&connection_id) {
        Ok(weaviate_ref) => weaviate_ref,
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
    match create_memory_record_with_vector(
        &weaviate_ref,
        &AgentMemoryUpsert {
            key: body.title,
            value: body.value,
            expires_at: body.expires_at,
            sender_id_list: body.sender_id_list,
            group_id_list: body.group_id_list,
        },
        Some(vector),
    ) {
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
    let weaviate_ref = match resolve_agent_memory_weaviate_ref(&connection_id) {
        Ok(weaviate_ref) => weaviate_ref,
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
    match update_memory_record_with_vector(
        &weaviate_ref,
        &object_id,
        &AgentMemoryUpsert {
            key: body.title,
            value: body.value,
            expires_at: body.expires_at,
            sender_id_list: body.sender_id_list,
            group_id_list: body.group_id_list,
        },
        Some(vector),
    ) {
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
    let weaviate_ref = match resolve_agent_memory_weaviate_ref(&connection_id) {
        Ok(weaviate_ref) => weaviate_ref,
        Err(err) => return render_internal_error(res, err),
    };
    match delete_memory_record(&weaviate_ref, &object_id) {
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
    let weaviate_ref = match resolve_agent_memory_weaviate_ref(&connection_id) {
        Ok(weaviate_ref) => weaviate_ref,
        Err(err) => return render_internal_error(res, err),
    };
    match get_memory_record(&weaviate_ref, &object_id) {
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

fn resolve_agent_memory_weaviate_ref(
    connection_id: &str,
) -> zihuan_core::error::Result<std::sync::Arc<zihuan_core::weaviate::WeaviateRef>> {
    let connections = load_connections()?;
    let connection = resource_resolver::find_connection(&connections, connection_id)?;
    let ConnectionKind::Weaviate(weaviate) = &connection.kind else {
        return Err(zihuan_core::error::Error::ValidationError(
            "connection is not a weaviate connection".to_string(),
        ));
    };
    if weaviate.collection_schema != WeaviateCollectionSchema::AgentMemory {
        return Err(zihuan_core::error::Error::ValidationError(format!(
            "connection '{}' is not an agent_memory collection",
            connection.name
        )));
    }
    let weaviate_ref = build_storage_weaviate_ref(
        &weaviate.base_url,
        &weaviate.class_name,
        weaviate.username.clone(),
        weaviate.password.clone(),
        weaviate.api_key.clone(),
        weaviate.auth_method,
        weaviate.collection_schema,
    )?;
    Ok(weaviate_ref)
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
        Ok((records, total)) => res.render(Json(MysqlExploreResponse {
            records,
            total,
            page,
            page_size,
        })),
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
            "message_id" | "sender_id" | "sender_name" | "group_id" | "content" => format!("{key} LIKE ?"),
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
            let rows = data_query.bind(page_size).bind((page - 1) * page_size).fetch_all($pool).await?;
            let mut records = Vec::with_capacity(rows.len());
            for row in rows {
                records.push(MessageRecordResponse {
                    message_id: row.try_get("message_id").unwrap_or_default(),
                    sender_id: row.try_get("sender_id").unwrap_or_default(),
                    sender_name: row.try_get("sender_name").unwrap_or_default(),
                    send_time: $time(&row),
                    group_id: row.try_get("group_id").unwrap_or(None),
                    group_name: row.try_get("group_name").unwrap_or(None),
                    content: truncate_preview(&row.try_get::<String, _>("content").unwrap_or_default(), 500),
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
    let (weaviate_id, elasticsearch_id, embedding_id) = service_memory_config(&agent);
    let access = AgentMemoryAccessContext {
        admin: true,
        skip_expiry_extend: true,
        ..Default::default()
    };
    if let Some(connection_id) = weaviate_id {
        let reference = match resource_resolver::build_weaviate_ref(
            Some(&connection_id),
            &connections,
            Some(WeaviateCollectionSchema::AgentMemory),
        ) {
            Ok(Some(reference)) => reference,
            Ok(None) => return render_bad_request(res, "memory connection is not configured".into()),
            Err(error) => return render_internal_error(res, error),
        };
        let hits = match query.as_deref() {
            Some(value) => {
                let Some(model_id) = embedding_id.as_deref() else {
                    return render_bad_request(res, "Service has no memory embedding model configured".into());
                };
                let vector = match service_memory_query_vector(model_id, value).await {
                    Ok(vector) => vector,
                    Err(error) => return render_internal_error(res, error),
                };
                match search_memory_content_by_vector(&reference, &access, &vector, limit) {
                    Ok(hits) => hits,
                    Err(error) => return render_internal_error(res, error),
                }
            }
            None => match list_recent_memory_keys(&reference, &access, limit, None) {
                Ok(hits) => hits,
                Err(error) => return render_internal_error(res, error),
            },
        };
        res.render(Json(ServiceMemoryResponse {
            items: memory_items(
                hits,
                if query.is_some() { "semantic" } else { "recent" },
                "weaviate",
                true,
            ),
            backend: "weaviate",
            mutable: true,
        }));
        return;
    }
    let Some(connection_id) = elasticsearch_id else {
        return render_bad_request(res, "Service has no memory store configured".into());
    };
    let reference = match resource_resolver::build_elasticsearch_ref(
        Some(&connection_id),
        &connections,
        Some(WeaviateCollectionSchema::AgentMemory),
    ) {
        Ok(Some(reference)) => reference,
        Ok(None) => return render_bad_request(res, "memory connection is not configured".into()),
        Err(error) => return render_internal_error(res, error),
    };
    let hits = match query.as_deref() {
        Some(value) => {
            let Some(model_id) = embedding_id.as_deref() else {
                return render_bad_request(res, "Service has no memory embedding model configured".into());
            };
            let vector = match service_memory_query_vector(model_id, value).await {
                Ok(vector) => vector,
                Err(error) => return render_internal_error(res, error),
            };
            match search_elasticsearch_memory(&reference, &access, value, &vector, limit) {
                Ok(hits) => hits,
                Err(error) => return render_internal_error(res, error),
            }
        }
        None => match list_elasticsearch_memory_keys(&reference, &access, limit, None) {
            Ok(hits) => hits,
            Err(error) => return render_internal_error(res, error),
        },
    };
    res.render(Json(ServiceMemoryResponse {
        items: memory_items(
            hits,
            if query.is_some() { "hybrid" } else { "recent" },
            "elasticsearch",
            false,
        ),
        backend: "elasticsearch",
        mutable: false,
    }));
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
    let (weaviate_id, elasticsearch_id, embedding_id) = service_image_config(&agent);
    let (name_vector, description_vector) =
        match embedding_vectors(embedding_id.as_deref(), name_query.as_deref(), description_query.as_deref()).await {
            Ok(value) => value,
            Err(error) => return render_internal_error(res, error),
        };
    if let Some(connection_id) = weaviate_id {
        let reference = match resource_resolver::build_weaviate_ref(
            Some(&connection_id),
            &connections,
            Some(WeaviateCollectionSchema::ImageSemantic),
        ) {
            Ok(Some(reference)) => reference,
            Ok(None) => return render_bad_request(res, "image connection is not configured".into()),
            Err(error) => return render_internal_error(res, error),
        };
        let properties = image_property_names();
        let mut items = Vec::new();
        for (query, field) in [
            (name_query.as_deref(), "name"),
            (description_query.as_deref(), "description"),
        ] {
            if let Some(query) = query {
                let args = format!(
                    "bm25: {{ query: \"{}\", properties: [\"{}\"] }}, limit: {}",
                    zihuan_core::weaviate::gql_escape(query),
                    field,
                    limit
                );
                if let Ok(response) = reference.query_with_args(&reference.class_name, &args, &properties) {
                    merge_image_items(&mut items, weaviate_image_items(response, "keyword"));
                }
            }
        }
        for (vector, target) in [
            (name_vector.as_deref(), "name_vector"),
            (description_vector.as_deref(), "description_vector"),
        ] {
            if let Some(vector) = vector {
                if let Ok(response) = reference.query_near_vector(
                    &reference.class_name,
                    vector,
                    Some(target),
                    limit,
                    &properties,
                    true,
                    false,
                ) {
                    merge_image_items(&mut items, weaviate_image_items(response, "semantic"));
                }
            }
        }
        if name_query.is_none() && description_query.is_none() {
            if let Ok(response) = reference.query_all(&reference.class_name, limit, &properties) {
                merge_image_items(&mut items, weaviate_image_items(response, "recent"));
            }
        }
        res.render(Json(ServiceImageResponse { items, backend: "weaviate" }));
        return;
    }
    let Some(connection_id) = elasticsearch_id else {
        return render_bad_request(res, "Service has no image store configured".into());
    };
    let reference = match resource_resolver::build_elasticsearch_ref(
        Some(&connection_id),
        &connections,
        Some(WeaviateCollectionSchema::ImageSemantic),
    ) {
        Ok(Some(reference)) => reference,
        Ok(None) => return render_bad_request(res, "image connection is not configured".into()),
        Err(error) => return render_internal_error(res, error),
    };
    let hits = match search_elasticsearch_images(
        &reference,
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
                if hit.keyword_match { "keyword" } else { "semantic" },
                hit.score,
                "elasticsearch",
            )
        })
        .collect();
    res.render(Json(ServiceImageResponse {
        items,
        backend: "elasticsearch",
    }));
}

fn load_service_and_connections(
    service_id: &str,
) -> zihuan_core::error::Result<(
    zihuan_core::agent::service_config::RoleServiceConfig,
    Vec<zihuan_core::storage::ConnectionConfig>,
)> {
    let agent = load_role_services()?
        .into_iter()
        .find(|item| item.id == service_id)
        .ok_or_else(|| zihuan_core::string_error!("Service '{}' not found", service_id))?;
    Ok((agent, load_connections()?))
}

fn service_memory_config(
    agent: &zihuan_core::agent::service_config::RoleServiceConfig,
) -> (Option<String>, Option<String>, Option<String>) {
    match &agent.role_service_type {
        RoleServiceType::QqChat(config) => (
            config.weaviate_memory_connection_id.clone(),
            config.elasticsearch_memory_connection_id.clone(),
            config.embedding_model_ref_id.clone(),
        ),
        RoleServiceType::Workspace(_) => (None, None, None),
    }
}

fn service_image_config(
    agent: &zihuan_core::agent::service_config::RoleServiceConfig,
) -> (Option<String>, Option<String>, Option<String>) {
    match &agent.role_service_type {
        RoleServiceType::QqChat(config) => (
            config.weaviate_image_connection_id.clone(),
            config.elasticsearch_image_connection_id.clone(),
            config.embedding_model_ref_id.clone(),
        ),
        _ => (None, None, None),
    }
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

fn image_property_names() -> Vec<String> {
    [
        "media_id",
        "original_source",
        "rustfs_path",
        "name",
        "description",
        "mime_type",
        "source",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}
fn weaviate_image_items(response: Value, kind: &'static str) -> Vec<ServiceImageItem> {
    response
        .get("data")
        .and_then(|value| value.get("Get"))
        .and_then(Value::as_object)
        .and_then(|items| items.values().next())
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let id = item
                .get("_additional")
                .and_then(|value| value.get("id"))
                .and_then(Value::as_str)?;
            Some(image_item(
                id.to_string(),
                item,
                kind,
                item.get("_additional")
                    .and_then(|value| value.get("distance"))
                    .and_then(Value::as_f64),
                "weaviate",
            ))
        })
        .collect()
}
fn image_item(
    object_id: String,
    value: &Value,
    kind: &'static str,
    score: Option<f64>,
    backend: &'static str,
) -> ServiceImageItem {
    let string = |key| value.get(key).and_then(Value::as_str).map(ToOwned::to_owned);
    ServiceImageItem {
        object_id,
        media_id: string("media_id"),
        name: string("name"),
        description: string("description"),
        original_source: string("original_source"),
        rustfs_path: string("rustfs_path"),
        mime_type: string("mime_type"),
        source: string("source"),
        url: None,
        match_kinds: vec![kind],
        score,
        backend,
    }
}
fn merge_image_items(target: &mut Vec<ServiceImageItem>, incoming: Vec<ServiceImageItem>) {
    for item in incoming {
        if let Some(existing) = target.iter_mut().find(|current| current.object_id == item.object_id) {
            if !existing.match_kinds.contains(&item.match_kinds[0]) {
                existing.match_kinds.push(item.match_kinds[0]);
            }
        } else {
            target.push(item);
        }
    }
    target.sort_by_key(|item| !item.match_kinds.contains(&"keyword"));
}

async fn resolve_agent_rdb_connection(
    agent_id: &str,
) -> zihuan_core::error::Result<zihuan_core::data_refs::RelationalDbConnection> {
    let agents = load_role_services()?;
    let agent = agents
        .into_iter()
        .find(|item| item.id == agent_id)
        .ok_or_else(|| zihuan_core::string_error!("agent '{}' not found", agent_id))?;
    let RoleServiceType::QqChat(config) = agent.role_service_type else {
        return Err(zihuan_core::string_error!(
            "agent '{}' is not a QQ Chat Agent Service",
            agent_id
        ));
    };
    let rdb_id = config
        .resolved_rdb_id()
        .ok_or_else(|| zihuan_core::string_error!("QQ Chat Agent Service '{}' has no rdb_id configured", agent_id))?;
    let connections = load_connections()?;
    build_relational_db_connection_for_connection(rdb_id, &connections).await
}
