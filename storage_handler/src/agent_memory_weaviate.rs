use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use zihuan_core::error::{Error, Result};
use zihuan_core::weaviate::WeaviateRef;

use crate::WeaviateClient;

const DEFAULT_QUERY_CANDIDATE_MULTIPLIER: usize = 5;
const MAX_QUERY_CANDIDATES: usize = 100;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentMemoryAccessContext {
    #[serde(default)]
    pub sender_id: Option<String>,
    #[serde(default)]
    pub group_id: Option<String>,
    #[serde(default)]
    pub is_group: bool,
    /// When true, bypasses all scope restrictions. Use only for admin/dashboard contexts.
    #[serde(default)]
    pub admin: bool,
    /// When true, search hits do not trigger expiry extension. Use for
    /// read-only UI views that must not mutate stored records.
    #[serde(default)]
    pub skip_expiry_extend: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMemoryRecord {
    pub object_id: String,
    #[serde(rename = "title", alias = "key")]
    pub key: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sender_id_list: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub group_id_list: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMemoryUpsert {
    pub key: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sender_id_list: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub group_id_list: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMemorySearchHit {
    #[serde(flatten)]
    pub record: AgentMemoryRecord,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distance: Option<f64>,
}

pub fn create_memory_record(weaviate_ref: &WeaviateRef, input: &AgentMemoryUpsert) -> Result<AgentMemoryRecord> {
    create_memory_record_with_vector(weaviate_ref, input, None)
}

pub fn create_memory_record_with_vector(
    weaviate_ref: &WeaviateRef,
    input: &AgentMemoryUpsert,
    vector: Option<Vec<f32>>,
) -> Result<AgentMemoryRecord> {
    let now = Utc::now().to_rfc3339();
    let properties = build_memory_properties(input, &now, Some(&now))?;
    let response = weaviate_ref.upsert_object(&weaviate_ref.class_name, properties, vector, None)?;
    parse_memory_record_with_fallback(response, None)
}

pub fn update_memory_record(
    weaviate_ref: &WeaviateRef,
    object_id: &str,
    input: &AgentMemoryUpsert,
) -> Result<AgentMemoryRecord> {
    update_memory_record_with_vector(weaviate_ref, object_id, input, None)
}

pub fn update_memory_record_with_vector(
    weaviate_ref: &WeaviateRef,
    object_id: &str,
    input: &AgentMemoryUpsert,
    vector: Option<Vec<f32>>,
) -> Result<AgentMemoryRecord> {
    let existing = get_memory_record(weaviate_ref, object_id)?;
    let now = Utc::now().to_rfc3339();
    let properties = build_memory_properties(input, &existing.created_at, Some(&now))?;
    let preserve_vector = vector.unwrap_or_else(|| {
        weaviate_ref
            .get_object_vector(&weaviate_ref.class_name, object_id)
            .ok()
            .flatten()
            .unwrap_or_default()
    });
    let response =
        weaviate_ref.update_object_with_vector(&weaviate_ref.class_name, object_id, properties, preserve_vector)?;
    parse_memory_record_with_fallback(response, Some(object_id))
}

pub fn delete_memory_record(weaviate_ref: &WeaviateRef, object_id: &str) -> Result<()> {
    weaviate_ref.delete_object(&weaviate_ref.class_name, object_id)
}

pub fn get_memory_record(weaviate_ref: &WeaviateRef, object_id: &str) -> Result<AgentMemoryRecord> {
    let response = weaviate_ref.get_object(&weaviate_ref.class_name, object_id)?;
    parse_memory_record_with_fallback(response, Some(object_id))
}

pub fn list_recent_memory_keys(
    weaviate_ref: &WeaviateRef,
    access: &AgentMemoryAccessContext,
    top_n: usize,
    query: Option<&str>,
) -> Result<Vec<AgentMemorySearchHit>> {
    let limit = candidate_limit(top_n);
    let records = if let Some(query) = query.map(str::trim).filter(|value| !value.is_empty()) {
        query_memory_records(weaviate_ref, query, limit)?
    } else {
        list_memory_records(weaviate_ref, limit)?
    };
    Ok(filter_and_sort_records(records, access).into_iter().take(top_n).collect())
}

pub fn search_memory_content(
    weaviate_ref: &WeaviateRef,
    access: &AgentMemoryAccessContext,
    query: &str,
    top_n: usize,
) -> Result<Vec<AgentMemorySearchHit>> {
    let mut hits = filter_and_sort_records(query_memory_records(weaviate_ref, query, candidate_limit(top_n))?, access);
    hits.truncate(top_n);
    if !access.skip_expiry_extend {
        extend_expiry_for_hits(weaviate_ref, &hits)?;
    }
    Ok(hits)
}

pub fn search_memory_content_by_vector(
    weaviate_ref: &WeaviateRef,
    access: &AgentMemoryAccessContext,
    vector: &[f32],
    top_n: usize,
) -> Result<Vec<AgentMemorySearchHit>> {
    let mut hits = filter_and_sort_records(
        query_memory_records_by_vector(weaviate_ref, vector, candidate_limit(top_n))?,
        access,
    );
    hits.truncate(top_n);
    if !access.skip_expiry_extend {
        extend_expiry_for_hits(weaviate_ref, &hits)?;
    }
    Ok(hits)
}

pub fn normalize_memory_scope_lists(items: &[String]) -> Vec<String> {
    let mut result = Vec::new();
    for item in items {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        if result.iter().any(|existing| existing == trimmed) {
            continue;
        }
        result.push(trimmed.to_string());
    }
    result
}

pub fn memory_is_accessible(record: &AgentMemoryRecord, access: &AgentMemoryAccessContext) -> bool {
    if is_memory_expired(record) {
        return false;
    }

    if access.admin {
        return true;
    }

    if !record.group_id_list.is_empty() {
        let Some(group_id) = access.group_id.as_deref().map(str::trim).filter(|value| !value.is_empty()) else {
            return false;
        };
        return record.group_id_list.iter().any(|value| value == group_id);
    }

    if !record.sender_id_list.is_empty() {
        let Some(sender_id) = access.sender_id.as_deref().map(str::trim).filter(|value| !value.is_empty()) else {
            return false;
        };
        return record.sender_id_list.iter().any(|value| value == sender_id);
    }

    true
}

pub fn is_memory_expired(record: &AgentMemoryRecord) -> bool {
    let Some(expires_at) = record.expires_at.as_deref() else {
        return false;
    };
    parse_rfc3339(expires_at)
        .map(|expires_at| expires_at <= Utc::now())
        .unwrap_or(false)
}

pub fn extend_expiry_for_hits(weaviate_ref: &WeaviateRef, hits: &[AgentMemorySearchHit]) -> Result<()> {
    for hit in hits {
        let Some(current_expiry) = hit.record.expires_at.as_deref().and_then(parse_rfc3339) else {
            continue;
        };
        if current_expiry <= Utc::now() {
            continue;
        }
        let now = Utc::now();
        let remaining = current_expiry - now;
        let doubled = now + remaining + remaining;
        let max_expiry = now + Duration::days(365);
        let next_expiry = if doubled > max_expiry { max_expiry } else { doubled };
        let payload = AgentMemoryUpsert {
            key: hit.record.key.clone(),
            value: hit.record.value.clone(),
            expires_at: Some(next_expiry.to_rfc3339()),
            sender_id_list: hit.record.sender_id_list.clone(),
            group_id_list: hit.record.group_id_list.clone(),
        };
        let _ = update_memory_record(weaviate_ref, &hit.record.object_id, &payload)?;
    }
    Ok(())
}

fn filter_and_sort_records(
    records: Vec<AgentMemorySearchHit>,
    access: &AgentMemoryAccessContext,
) -> Vec<AgentMemorySearchHit> {
    let mut filtered = records
        .into_iter()
        .filter(|item| memory_is_accessible(&item.record, access))
        .collect::<Vec<_>>();
    filtered.sort_by(|left, right| right.record.updated_at.cmp(&left.record.updated_at));
    filtered
}

fn list_memory_records(weaviate_ref: &WeaviateRef, limit: usize) -> Result<Vec<AgentMemorySearchHit>> {
    let args = format!("limit: {limit}, sort: [{{ path: [\"updated_at\"], order: desc }}]");
    let response = weaviate_ref.query_with_args(&weaviate_ref.class_name, &args, &memory_property_names())?;
    parse_memory_hits(response)
}

fn query_memory_records(weaviate_ref: &WeaviateRef, query: &str, limit: usize) -> Result<Vec<AgentMemorySearchHit>> {
    let response = weaviate_ref.query_hybrid(
        &weaviate_ref.class_name,
        query,
        limit,
        &memory_property_names(),
        None,
        None,
        Some(json!([
            {
                "path": ["updated_at"],
                "order": "desc"
            }
        ])),
        true,
    )?;
    parse_memory_hits(response)
}

fn query_memory_records_by_vector(
    weaviate_ref: &WeaviateRef,
    vector: &[f32],
    limit: usize,
) -> Result<Vec<AgentMemorySearchHit>> {
    let response = weaviate_ref.query_near_vector(
        &weaviate_ref.class_name,
        vector,
        None,
        limit,
        &memory_property_names(),
        true,
        false,
    )?;
    parse_memory_hits(response)
}

fn parse_memory_hits(response: Value) -> Result<Vec<AgentMemorySearchHit>> {
    let class_name = response
        .get("data")
        .and_then(|value| value.get("Get"))
        .and_then(Value::as_object)
        .and_then(|object| object.keys().next().cloned())
        .ok_or_else(|| Error::ValidationError("weaviate response missing Get class".to_string()))?;
    let items = response
        .get("data")
        .and_then(|value| value.get("Get"))
        .and_then(|value| value.get(&class_name))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    items.into_iter().map(parse_memory_hit).collect()
}

fn parse_memory_hit(value: Value) -> Result<AgentMemorySearchHit> {
    let distance = value
        .get("_additional")
        .and_then(|extra| extra.get("distance"))
        .and_then(Value::as_f64);
    let record = parse_memory_record_with_fallback(value, None)?;
    Ok(AgentMemorySearchHit { record, distance })
}

fn parse_memory_record_with_fallback(value: Value, fallback_id: Option<&str>) -> Result<AgentMemoryRecord> {
    let object_id = value
        .get("id")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| {
            value
                .get("_additional")
                .and_then(|extra| extra.get("id"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .or_else(|| fallback_id.map(ToOwned::to_owned))
        .ok_or_else(|| Error::ValidationError("memory record missing object id".to_string()))?;
    let properties = value.get("properties").cloned().unwrap_or_else(|| value.clone());
    Ok(AgentMemoryRecord {
        object_id,
        key: required_string(&properties, "key")?,
        value: required_string(&properties, "value")?,
        expires_at: optional_string(&properties, "expires_at"),
        sender_id_list: string_list(&properties, "sender_id_list"),
        group_id_list: string_list(&properties, "group_id_list"),
        created_at: required_string(&properties, "created_at")?,
        updated_at: required_string(&properties, "updated_at")?,
    })
}

fn build_memory_properties(input: &AgentMemoryUpsert, created_at: &str, updated_at: Option<&str>) -> Result<Value> {
    let key = input.key.trim();
    if key.is_empty() {
        return Err(Error::ValidationError("memory title must not be empty".to_string()));
    }
    let value = input.value.trim();
    if value.is_empty() {
        return Err(Error::ValidationError("memory value must not be empty".to_string()));
    }
    let sender_id_list = normalize_memory_scope_lists(&input.sender_id_list);
    let group_id_list = normalize_memory_scope_lists(&input.group_id_list);
    if let Some(expires_at) = input.expires_at.as_deref() {
        let _ = parse_rfc3339(expires_at)
            .ok_or_else(|| Error::ValidationError(format!("invalid expires_at '{}'", expires_at)))?;
    }
    Ok(json!({
        "key": key,
        "value": value,
        "expires_at": input.expires_at.as_deref().map(str::trim).filter(|value| !value.is_empty()),
        "sender_id_list": sender_id_list,
        "group_id_list": group_id_list,
        "created_at": created_at,
        "updated_at": updated_at.unwrap_or(created_at),
    }))
}

fn memory_property_names() -> Vec<String> {
    [
        "key",
        "value",
        "expires_at",
        "sender_id_list",
        "group_id_list",
        "created_at",
        "updated_at",
    ]
    .into_iter()
    .map(|value| value.to_string())
    .collect()
}

fn candidate_limit(top_n: usize) -> usize {
    (top_n.max(1) * DEFAULT_QUERY_CANDIDATE_MULTIPLIER).min(MAX_QUERY_CANDIDATES)
}

fn required_string(value: &Value, key: &str) -> Result<String> {
    optional_string(value, key).ok_or_else(|| Error::ValidationError(format!("memory record missing '{}'", key)))
}

fn optional_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
}

fn string_list(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn parse_rfc3339(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value).ok().map(|value| value.with_timezone(&Utc))
}
