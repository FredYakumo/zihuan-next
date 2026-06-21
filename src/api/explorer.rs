use model_inference::nn::embedding::embedding_runtime_manager::RuntimeEmbeddingModelManager;
use redis::AsyncCommands;
use salvo::prelude::*;
use salvo::writing::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sqlx::Row as SqlxRow;

use crate::system_config::load_connections;
use storage_handler::{
    create_memory_record_with_vector, delete_memory_record, get_memory_record, list_recent_memory_keys,
    resource_resolver::{self, build_rdb_ref}, search_memory_content_by_vector, update_memory_record_with_vector,
    weaviate::build_weaviate_ref as build_storage_weaviate_ref, AgentMemoryAccessContext, AgentMemoryUpsert,
    ConnectionKind, WeaviateClient, WeaviateCollectionSchema,
};

use super::config::{render_bad_request, render_internal_error};

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
        weaviate.collection_schema,
    )?;
    Ok(weaviate_ref)
}
