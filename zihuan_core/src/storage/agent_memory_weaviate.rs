use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::{Error, Result};
use crate::retrieval::RetrievalSchema;
use crate::weaviate::WeaviateRef;

use super::memory::{
    parse_behavioral_record, parse_concept_record, parse_person_record, prepare_behavioral,
    prepare_person, BehavioralMemoryPrepared, BehavioralMemoryWrite, ConceptMemoryPrepared,
    PersonMemoryWrite,
};
use super::memory::{MemoryRecord, MemorySearchHit};
use super::user_permission::MemoryWriteSource;
use super::{memory_object_id, MemoryCategory};

const DEFAULT_QUERY_CANDIDATE_MULTIPLIER: usize = 5;
const MAX_QUERY_CANDIDATES: usize = 100;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentMemoryAccessContext {
    #[serde(skip)]
    pub write_source: MemoryWriteSource,
    #[serde(default)]
    pub category: MemoryCategory,
    #[serde(skip)]
    pub permission_connection: Option<crate::data_refs::RelationalDbConnection>,
    #[serde(default)]
    pub agent_id: String,
    #[serde(default)]
    pub sender_id: Option<String>,
    #[serde(default)]
    pub group_id: Option<String>,
    #[serde(default)]
    pub is_group: bool,
    /// When true, bypasses all scope restrictions. Use only for admin/dashboard contexts.
    #[serde(default)]
    pub admin: bool,
    /// When set, records above this access level are inaccessible. `None` (the default)
    /// imposes no level restriction. Admin bypasses it.
    #[serde(default)]
    pub max_access_level: Option<u32>,
    /// When true, search hits do not trigger expiry extension. Use for
    /// read-only UI views that must not mutate stored records.
    #[serde(default)]
    pub skip_expiry_extend: bool,
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

pub fn memory_is_accessible(record: &MemoryRecord, access: &AgentMemoryAccessContext) -> bool {
    if is_memory_expired(record) {
        return false;
    }
    if access.admin {
        return true;
    }
    if access.max_access_level.is_some_and(|max| record.access_level() > max) {
        return false;
    }
    let (sender_id_list, group_id_list) = match record {
        // Person records carry no scope: the profile is globally addressable identity.
        MemoryRecord::Person(_) => return true,
        MemoryRecord::Concept(record) => (&record.sender_id_list, &record.group_id_list),
        MemoryRecord::Behavioral(record) => (&record.sender_id_list, &record.group_id_list),
    };
    if !group_id_list.is_empty()
        && !access.group_id.as_ref().is_some_and(|id| group_id_list.contains(id))
    {
        return false;
    }
    if !sender_id_list.is_empty()
        && !access.sender_id.as_ref().is_some_and(|id| sender_id_list.contains(id))
    {
        return false;
    }
    true
}

pub fn is_memory_expired(record: &MemoryRecord) -> bool {
    let MemoryRecord::Behavioral(record) = record else {
        return false;
    };
    parse_rfc3339(&record.expires_at)
        .map(|expires_at| expires_at <= Utc::now())
        .unwrap_or(false)
}

// ── Weaviate records ──────────────────────────────────────────────────────────

pub fn get_memory_record(weaviate_ref: &WeaviateRef, object_id: &str) -> Result<MemoryRecord> {
    let response = weaviate_ref.get_object(&weaviate_ref.class_name, object_id)?;
    parse_memory_record(&weaviate_ref.class_name, response, Some(object_id))
}

pub fn delete_memory_record(weaviate_ref: &WeaviateRef, object_id: &str) -> Result<()> {
    weaviate_ref.delete_object(&weaviate_ref.class_name, object_id)
}

/// Creates or updates one concept record from a prepared write. The derived identity
/// decides whether this rewrites the existing fact or creates a new one.
pub fn upsert_concept_record(
    weaviate_ref: &WeaviateRef,
    prepared: &ConceptMemoryPrepared,
    vector: Option<Vec<f32>>,
) -> Result<MemoryRecord> {
    let now = Utc::now().to_rfc3339();
    let created_at = existing_created_at(weaviate_ref, &prepared.object_id, &now, "concept")?;
    let properties = json!({
        "key": prepared.key,
        "content": prepared.content,
        "access_level": prepared.access_level,
        "starts_at": prepared.starts_at,
        "sender_id_list": prepared.sender_id_list,
        "group_id_list": prepared.group_id_list,
        "created_at": created_at,
        "updated_at": now,
    });
    write_record(weaviate_ref, &prepared.object_id, properties, vector)?;
    get_memory_record(weaviate_ref, &prepared.object_id)
}

pub fn upsert_behavioral_record(
    weaviate_ref: &WeaviateRef,
    prepared: &BehavioralMemoryPrepared,
    vector: Option<Vec<f32>>,
) -> Result<MemoryRecord> {
    let now = Utc::now().to_rfc3339();
    let created_at = existing_created_at(weaviate_ref, &prepared.object_id, &now, "behavioral")?;
    let properties = json!({
        "key": prepared.key,
        "content": prepared.content,
        "access_level": prepared.access_level,
        "expires_at": prepared.expires_at,
        "sender_id_list": prepared.sender_id_list,
        "group_id_list": prepared.group_id_list,
        "created_at": created_at,
        "updated_at": now,
    });
    write_record(weaviate_ref, &prepared.object_id, properties, vector)?;
    get_memory_record(weaviate_ref, &prepared.object_id)
}

pub fn upsert_person_record(
    weaviate_ref: &WeaviateRef,
    write: &PersonMemoryWrite,
    vector: Option<Vec<f32>>,
) -> Result<MemoryRecord> {
    let prepared = prepare_person(write)?;
    let object_id = get_person_by_qq_id(weaviate_ref, &prepared.profile.qq_id)?
        .map(|record| record.object_id)
        .unwrap_or_else(|| {
            memory_object_id(MemoryCategory::Person, &prepared.profile.qq_id, &[], &[])
        });
    let now = Utc::now().to_rfc3339();
    let created_at = existing_created_at(weaviate_ref, &object_id, &now, "person")?;
    let properties = person_properties(&prepared.profile, prepared.access_level, &created_at, &now);
    write_record(weaviate_ref, &object_id, properties, vector)?;
    get_memory_record(weaviate_ref, &object_id)
}

/// The `created_at` of the stored record at `object_id`, failing when that object
/// belongs to another category. A missing object starts fresh with `now`.
fn existing_created_at(
    weaviate_ref: &WeaviateRef,
    object_id: &str,
    now: &str,
    category: &str,
) -> Result<String> {
    match get_memory_record(weaviate_ref, object_id) {
        Ok(MemoryRecord::Concept(record)) if category == "concept" => Ok(record.created_at),
        Ok(MemoryRecord::Behavioral(record)) if category == "behavioral" => Ok(record.created_at),
        Ok(MemoryRecord::Person(record)) if category == "person" => Ok(record.created_at),
        Ok(other) => Err(category_mismatch(category, other.object_id())),
        Err(Error::NotFound) => Ok(now.to_string()),
        Err(error) => Err(error),
    }
}

fn category_mismatch(category: &str, key: &str) -> Error {
    Error::ValidationError(format!(
        "memory object '{key}' already exists as another category than {category}"
    ))
}

/// Admin update at an explicit object id: identity fields must match the stored record.
pub fn update_concept_record(
    weaviate_ref: &WeaviateRef,
    object_id: &str,
    prepared: &ConceptMemoryPrepared,
    vector: Option<Vec<f32>>,
) -> Result<MemoryRecord> {
    let MemoryRecord::Concept(existing) = get_memory_record(weaviate_ref, object_id)? else {
        return Err(Error::NotFound);
    };
    require_same_identity(
        &prepared.key,
        &existing.key,
        &prepared.sender_id_list,
        &existing.sender_id_list,
        &prepared.group_id_list,
        &existing.group_id_list,
    )?;
    let now = Utc::now().to_rfc3339();
    let properties = json!({
        "key": prepared.key,
        "content": prepared.content,
        "access_level": prepared.access_level,
        "starts_at": prepared.starts_at,
        "sender_id_list": prepared.sender_id_list,
        "group_id_list": prepared.group_id_list,
        "created_at": existing.created_at,
        "updated_at": now,
    });
    write_record(weaviate_ref, object_id, properties, vector)?;
    get_memory_record(weaviate_ref, object_id)
}

pub fn update_behavioral_record(
    weaviate_ref: &WeaviateRef,
    object_id: &str,
    prepared: &BehavioralMemoryPrepared,
    vector: Option<Vec<f32>>,
) -> Result<MemoryRecord> {
    let MemoryRecord::Behavioral(existing) = get_memory_record(weaviate_ref, object_id)? else {
        return Err(Error::NotFound);
    };
    require_same_identity(
        &prepared.key,
        &existing.key,
        &prepared.sender_id_list,
        &existing.sender_id_list,
        &prepared.group_id_list,
        &existing.group_id_list,
    )?;
    let now = Utc::now().to_rfc3339();
    let properties = json!({
        "key": prepared.key,
        "content": prepared.content,
        "access_level": prepared.access_level,
        "expires_at": prepared.expires_at,
        "sender_id_list": prepared.sender_id_list,
        "group_id_list": prepared.group_id_list,
        "created_at": existing.created_at,
        "updated_at": now,
    });
    write_record(weaviate_ref, object_id, properties, vector)?;
    get_memory_record(weaviate_ref, object_id)
}

pub fn update_person_record(
    weaviate_ref: &WeaviateRef,
    object_id: &str,
    write: &PersonMemoryWrite,
    vector: Option<Vec<f32>>,
) -> Result<MemoryRecord> {
    let prepared = prepare_person(write)?;
    let MemoryRecord::Person(existing) = get_memory_record(weaviate_ref, object_id)? else {
        return Err(Error::NotFound);
    };
    if existing.profile.qq_id != prepared.profile.qq_id {
        return Err(Error::ValidationError("person QQ ID is immutable".to_string()));
    }
    let now = Utc::now().to_rfc3339();
    let properties =
        person_properties(&prepared.profile, prepared.access_level, &existing.created_at, &now);
    write_record(weaviate_ref, object_id, properties, vector)?;
    get_memory_record(weaviate_ref, object_id)
}

fn require_same_identity(
    key: &str,
    existing_key: &str,
    sender_id_list: &[String],
    existing_senders: &[String],
    group_id_list: &[String],
    existing_groups: &[String],
) -> Result<()> {
    if key != existing_key || sender_id_list != existing_senders || group_id_list != existing_groups
    {
        return Err(Error::ValidationError(
            "memory identity is immutable; create a new record to change title or scope"
                .to_string(),
        ));
    }
    Ok(())
}

fn person_properties(
    profile: &super::PersonMemory,
    access_level: u32,
    created_at: &str,
    updated_at: &str,
) -> Value {
    json!({
        "qq_id": profile.qq_id,
        "name": profile.name,
        "preferred_name": profile.preferred_name,
        "aliases": profile.aliases,
        "impression": profile.impression,
        "guideline": profile.guideline,
        "access_level": access_level,
        "created_at": created_at,
        "updated_at": updated_at,
    })
}

fn write_record(
    weaviate_ref: &WeaviateRef,
    object_id: &str,
    properties: Value,
    vector: Option<Vec<f32>>,
) -> Result<()> {
    let vector = match vector {
        Some(vector) => Some(vector),
        None => weaviate_ref
            .get_object_vector(&weaviate_ref.class_name, object_id)
            .ok()
            .flatten(),
    };
    weaviate_ref.upsert_object(&weaviate_ref.class_name, properties, vector, Some(object_id))?;
    Ok(())
}

/// Renews a recalled behavioral rule: the remaining lifetime doubles, capped at a year.
pub fn extend_expiry_for_hits(weaviate_ref: &WeaviateRef, hits: &[MemorySearchHit]) -> Result<()> {
    for hit in hits {
        let MemoryRecord::Behavioral(record) = &hit.record else {
            continue;
        };
        let Some(current_expiry) = parse_rfc3339(&record.expires_at) else {
            continue;
        };
        if current_expiry <= Utc::now() {
            continue;
        }
        let now = Utc::now();
        let doubled = now + (current_expiry - now) * 2;
        let next_expiry = doubled.min(now + Duration::days(365));
        let prepared = prepare_behavioral(&BehavioralMemoryWrite {
            key: record.key.clone(),
            content: record.content.clone(),
            access_level: record.access_level,
            expires_at: Some(next_expiry.to_rfc3339()),
            sender_id_list: record.sender_id_list.clone(),
            group_id_list: record.group_id_list.clone(),
        })?;
        upsert_behavioral_record(weaviate_ref, &prepared, None)?;
    }
    Ok(())
}

// ── Weaviate queries ──────────────────────────────────────────────────────────

pub fn list_recent_memory_keys(
    weaviate_ref: &WeaviateRef,
    access: &AgentMemoryAccessContext,
    top_n: usize,
    query: Option<&str>,
) -> Result<Vec<MemorySearchHit>> {
    let limit = candidate_limit(top_n);
    let records = if let Some(query) = query.map(str::trim).filter(|value| !value.is_empty()) {
        query_memory_records(weaviate_ref, query, limit)?
    } else {
        list_memory_records(weaviate_ref, limit)?
    };
    Ok(filter_accessible_records(records, access).into_iter().take(top_n).collect())
}

pub fn search_memory_content(
    weaviate_ref: &WeaviateRef,
    access: &AgentMemoryAccessContext,
    query: &str,
    top_n: usize,
) -> Result<Vec<MemorySearchHit>> {
    let mut hits = filter_accessible_records(
        query_memory_records(weaviate_ref, query, candidate_limit(top_n))?,
        access,
    );
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
) -> Result<Vec<MemorySearchHit>> {
    let mut hits = filter_accessible_records(
        query_memory_records_by_vector(weaviate_ref, vector, candidate_limit(top_n))?,
        access,
    );
    hits.truncate(top_n);
    if !access.skip_expiry_extend {
        extend_expiry_for_hits(weaviate_ref, &hits)?;
    }
    Ok(hits)
}

fn filter_accessible_records(
    records: Vec<MemorySearchHit>,
    access: &AgentMemoryAccessContext,
) -> Vec<MemorySearchHit> {
    // Preserve BM25 / vector rank. Recent listings are sorted by the server.
    records
        .into_iter()
        .filter(|item| memory_is_accessible(&item.record, access))
        .collect()
}

fn list_memory_records(weaviate_ref: &WeaviateRef, limit: usize) -> Result<Vec<MemorySearchHit>> {
    let args = format!("limit: {limit}, sort: [{{ path: [\"updated_at\"], order: desc }}]");
    let response = weaviate_ref.query_with_args(
        &weaviate_ref.class_name,
        &args,
        &memory_property_names(weaviate_ref),
        false,
    )?;
    parse_memory_hits(response)
}

fn query_memory_records(
    weaviate_ref: &WeaviateRef,
    query: &str,
    limit: usize,
) -> Result<Vec<MemorySearchHit>> {
    let properties =
        if weaviate_ref.class_name == RetrievalSchema::PersonMemory.weaviate_class_name() {
            ", properties: [\"qq_id\",\"name\",\"preferred_name\",\"aliases\",\"impression\"]"
        } else {
            ""
        };
    let args = format!(
        "bm25: {{ query: \"{}\"{properties} }}, limit: {limit}",
        crate::weaviate::gql_escape(query)
    );
    let response = weaviate_ref.query_with_args(
        &weaviate_ref.class_name,
        &args,
        &memory_property_names(weaviate_ref),
        true,
    )?;
    parse_memory_hits(response)
}

fn query_memory_records_by_vector(
    weaviate_ref: &WeaviateRef,
    vector: &[f32],
    limit: usize,
) -> Result<Vec<MemorySearchHit>> {
    let response = weaviate_ref.query_near_vector(
        &weaviate_ref.class_name,
        vector,
        None,
        limit,
        &memory_property_names(weaviate_ref),
        true,
        false,
    )?;
    parse_memory_hits(response)
}

pub(crate) fn get_person_by_qq_id(
    reference: &WeaviateRef,
    qq_id: &str,
) -> Result<Option<super::memory::PersonMemoryRecord>> {
    let args = format!(
        "where: {{path: [\"qq_id\"], operator: Equal, valueText: \"{}\"}}, limit: 1",
        crate::weaviate::gql_escape(qq_id)
    );
    let response = reference.query_with_args(
        &reference.class_name,
        &args,
        &memory_property_names(reference),
        false,
    )?;
    Ok(parse_memory_hits(response)?
        .into_iter()
        .filter_map(|hit| match hit.record {
            MemoryRecord::Person(record) => Some(record),
            _ => None,
        })
        .next())
}

fn parse_memory_hits(response: Value) -> Result<Vec<MemorySearchHit>> {
    let (class_name, items) = response_items(response)?;
    items
        .into_iter()
        .map(|item| {
            let distance = item
                .get("_additional")
                .and_then(|extra| extra.get("distance"))
                .and_then(Value::as_f64);
            let record = parse_memory_record(&class_name, item, None)?;
            Ok(MemorySearchHit { record, distance })
        })
        .collect()
}

/// Splits a GraphQL `Get` response into its class name and result items.
fn response_items(response: Value) -> Result<(String, Vec<Value>)> {
    let get = response
        .get("data")
        .and_then(|value| value.get("Get"))
        .and_then(Value::as_object)
        .ok_or_else(|| Error::ValidationError("weaviate response missing Get class".to_string()))?;
    let (class_name, items) = get
        .into_iter()
        .next()
        .ok_or_else(|| Error::ValidationError("weaviate response missing Get class".to_string()))?;
    Ok((class_name.clone(), items.as_array().cloned().unwrap_or_default()))
}

fn parse_memory_record(
    class_name: &str,
    value: Value,
    fallback_id: Option<&str>,
) -> Result<MemoryRecord> {
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
    let schema = memory_schema_for_class(class_name).ok_or_else(|| {
        Error::ValidationError(format!("weaviate class '{class_name}' is not a memory collection"))
    })?;
    match schema {
        RetrievalSchema::ConceptMemory => {
            Ok(MemoryRecord::Concept(parse_concept_record(properties, &object_id)?))
        }
        RetrievalSchema::PersonMemory => {
            Ok(MemoryRecord::Person(parse_person_record(properties, &object_id)?))
        }
        RetrievalSchema::BehavioralMemory => {
            Ok(MemoryRecord::Behavioral(parse_behavioral_record(properties, &object_id)?))
        }
        _ => Err(Error::ValidationError(format!(
            "weaviate class '{class_name}' is not a memory collection"
        ))),
    }
}

fn memory_schema_for_class(class_name: &str) -> Option<RetrievalSchema> {
    [
        RetrievalSchema::ConceptMemory,
        RetrievalSchema::PersonMemory,
        RetrievalSchema::BehavioralMemory,
    ]
    .into_iter()
    .find(|schema| schema.weaviate_class_name() == class_name)
}

/// The properties each memory collection actually carries, so queries never
/// request a property a per-category collection no longer has.
fn memory_property_names(weaviate_ref: &WeaviateRef) -> Vec<String> {
    let names: &[&str] = if weaviate_ref.class_name
        == RetrievalSchema::ConceptMemory.weaviate_class_name()
    {
        &[
            "access_level",
            "starts_at",
            "key",
            "content",
            "sender_id_list",
            "group_id_list",
            "created_at",
            "updated_at",
        ]
    } else if weaviate_ref.class_name == RetrievalSchema::PersonMemory.weaviate_class_name() {
        &[
            "access_level",
            "qq_id",
            "name",
            "preferred_name",
            "aliases",
            "impression",
            "guideline",
            "created_at",
            "updated_at",
        ]
    } else if weaviate_ref.class_name == RetrievalSchema::BehavioralMemory.weaviate_class_name() {
        &[
            "access_level",
            "key",
            "content",
            "expires_at",
            "sender_id_list",
            "group_id_list",
            "created_at",
            "updated_at",
        ]
    } else {
        // Unknown class: request the full set to stay compatible.
        &[
            "access_level",
            "starts_at",
            "qq_id",
            "name",
            "preferred_name",
            "aliases",
            "impression",
            "guideline",
            "key",
            "content",
            "expires_at",
            "sender_id_list",
            "group_id_list",
            "created_at",
            "updated_at",
        ]
    };
    names.iter().map(|value| value.to_string()).collect()
}

fn candidate_limit(top_n: usize) -> usize {
    (top_n.max(1) * DEFAULT_QUERY_CANDIDATE_MULTIPLIER).min(MAX_QUERY_CANDIDATES)
}

fn parse_rfc3339(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value).ok().map(|value| value.with_timezone(&Utc))
}
