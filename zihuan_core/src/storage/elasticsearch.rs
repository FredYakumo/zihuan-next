use crate::connection::Connection;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use base64::Engine;
use chrono::Utc;
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use serde_json::{json, Value};

use crate::error::{Error, Result};
use crate::retrieval::RetrievalSchema;

use crate::ims_bot_adapter::models::message::PersistedMedia;
use crate::storage::memory::{
    parse_behavioral_record, parse_concept_record, parse_person_record, prepare_person,
    BehavioralMemoryPrepared, ConceptMemoryPrepared, PersonMemoryWrite,
};
use crate::storage::{
    memory_is_accessible, validate_connection_authentication, AgentMemoryAccessContext,
    ElasticsearchConnection, MemoryRecord, MemorySearchHit,
};

const REQUEST_TIMEOUT_SECS: u64 = 30;
const CONNECT_TIMEOUT_SECS: u64 = 5;
/// Index initialization right after a container starts; the HTTP API becomes
/// reachable slightly after the socket does and answers 503 until the cluster
/// finishes forming.
const INDEX_STARTUP_RETRY_TIMEOUT: Duration = Duration::from_secs(60);
const INDEX_CHECK_RETRY_DELAY: Duration = Duration::from_secs(1);
const MAX_QUERY_CANDIDATES: usize = 100;

#[derive(Clone)]
pub struct ElasticsearchRef {
    client: Arc<RwLock<Option<Client>>>,
    headers: HeaderMap,
    base_url: String,
    pub index_name: String,
    pub schema: RetrievalSchema,
    pub vector_dimensions: usize,
    memory_min_similarity: f64,
}

impl std::fmt::Debug for ElasticsearchRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ElasticsearchRef")
            .field("base_url", &self.base_url)
            .field("index_name", &self.index_name)
            .field("schema", &self.schema)
            .field("vector_dimensions", &self.vector_dimensions)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone)]
pub struct ElasticsearchImageSearchHit {
    pub object_id: String,
    pub properties: Value,
    pub score: Option<f64>,
    pub keyword_match: bool,
}

impl ElasticsearchRef {
    /// Builds a reference bound to the index hosting `schema`.
    pub fn new(config: ElasticsearchConnection, schema: RetrievalSchema) -> Result<Self> {
        let base_url = config.base_url.trim().trim_end_matches('/').to_string();
        if !(base_url.starts_with("http://") || base_url.starts_with("https://")) {
            return Err(Error::ValidationError(
                "elasticsearch base_url must use http:// or https://".to_string(),
            ));
        }
        if config.vector_dimensions == 0 {
            return Err(Error::ValidationError(
                "elasticsearch vector_dimensions is required".to_string(),
            ));
        }
        if !config.memory_min_similarity.is_finite()
            || !(-1.0..=1.0).contains(&config.memory_min_similarity)
        {
            return Err(Error::ValidationError(
                "elasticsearch memory_min_similarity must be between -1 and 1".to_string(),
            ));
        }
        validate_connection_authentication(
            config.auth_method,
            config.username.as_deref(),
            config.password.as_deref(),
            config.api_key.as_deref(),
            "elasticsearch",
        )?;
        let username = config.username.as_deref().map(str::trim).filter(|value| !value.is_empty());
        let password = config.password.as_deref().map(str::trim).filter(|value| !value.is_empty());
        let mut headers = HeaderMap::new();
        match config.auth_method {
            crate::storage::ConnectionAuthMethod::ApiKey => {
                let api_key = config
                    .api_key
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or_default();
                headers.insert(
                    AUTHORIZATION,
                    HeaderValue::from_str(&format!("ApiKey {api_key}")).map_err(|_| {
                        Error::ValidationError("invalid elasticsearch api_key".to_string())
                    })?,
                );
            }
            crate::storage::ConnectionAuthMethod::Password => {
                let credentials = base64::engine::general_purpose::STANDARD.encode(format!(
                    "{}:{}",
                    username.unwrap_or_default(),
                    password.unwrap_or_default()
                ));
                headers.insert(
                    AUTHORIZATION,
                    HeaderValue::from_str(&format!("Basic {credentials}")).map_err(|_| {
                        Error::ValidationError("invalid elasticsearch basic auth".to_string())
                    })?,
                );
            }
        }
        // Fix: `reqwest::blocking::ClientBuilder::build()` creates and immediately drops a
        // temporary current-thread tokio runtime in debug builds (see reqwest
        // `blocking::wait::enter`) to assert the caller is not blocking an async thread.
        // Dropping a runtime from within an entered async context panics with
        // "Cannot drop a runtime in a context where blocking is not allowed".
        //
        // This function is called synchronously from tokio async handlers (for example the
        // Data Explorer memory and image endpoints, which build a reference per request). The
        // panic aborted the handler task before any response was written, so the connection was
        // reset and the frontend only saw `Failed to fetch`.
        //
        // Building the client on a thread spawned by `run_outside_async_runtime` avoids the
        // problem: that thread has no entered async context, so the temporary runtime is created
        // and dropped safely. Every caller works afterwards, whether it is an async handler, a
        // blocking thread, or a scheduled job. The `request()` method below already re-entered
        // through the same helper for each HTTP call; this was the missing construction step.
        //
        // Previous implementation (built on the calling thread, panics in an async context):
        // let builder = Client::builder()
        //     .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        //     .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
        //     .default_headers(headers.clone());
        // Ok(Self {
        //     client: builder.build().map_err(|error| {
        //         crate::string_error!("build elasticsearch client failed: {error}")
        //     })?,
        //     base_url,
        //     index_name: schema.elasticsearch_index_name().to_string(),
        //     schema,
        //     vector_dimensions: config.vector_dimensions,
        // })
        let client = crate::runtime::run_outside_async_runtime(|| {
            Client::builder()
                .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
                .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
                .default_headers(headers.clone())
                .build()
                .map_err(|error| crate::string_error!("build elasticsearch client failed: {error}"))
        })?;
        Ok(Self {
            client: Arc::new(RwLock::new(Some(client))),
            headers,
            base_url,
            index_name: schema.elasticsearch_index_name().to_string(),
            schema,
            vector_dimensions: config.vector_dimensions,
            memory_min_similarity: config.memory_min_similarity,
        })
    }

    fn client(&self) -> Result<Client> {
        self.client
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .ok_or_else(|| crate::string_error!("Elasticsearch connection is closed"))
    }

    pub(crate) fn request(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value> {
        // `reqwest::blocking` creates and drops its own runtime, which must not happen while an
        // async context is entered.
        crate::runtime::run_outside_async_runtime(|| {
            let url = format!("{}{}", self.base_url, path);
            let request = self.client()?.request(method, url);
            let request = if let Some(body) = body {
                request.json(&body)
            } else {
                request
            };
            let response = request
                .send()
                .map_err(|error| crate::string_error!("elasticsearch request failed: {error}"))?;
            let status = response.status();
            if status.as_u16() == 404 {
                return Err(Error::NotFound);
            }
            let value = response.json::<Value>().unwrap_or(Value::Null);
            if !status.is_success() {
                return Err(Error::ValidationError(format!(
                    "elasticsearch request failed ({status}): {value}"
                )));
            }
            Ok(value)
        })
    }
}

pub fn ensure_elasticsearch_index(
    reference: &ElasticsearchRef,
    create_missing: bool,
) -> Result<bool> {
    // HEAD, retry sleeps and response teardown must all run outside Tokio's async
    // context, just like the other blocking requests made by ElasticsearchRef.
    crate::runtime::run_outside_async_runtime(|| {
        let index_url = format!("{}/{}", reference.base_url, reference.index_name);
        // A container that is still starting accepts the socket before its HTTP API
        // answers, so timeouts are retried. A refused connection means nothing is
        // listening at all, which is a configuration error: report it immediately
        // instead of stalling the request.
        let deadline = std::time::Instant::now() + INDEX_STARTUP_RETRY_TIMEOUT;
        let exists = loop {
            match reference.client()?.head(&index_url).send() {
                Ok(response) => {
                    let status = response.status();
                    if status.as_u16() == 404 {
                        break false;
                    }
                    // A cluster that is still forming reports itself as unavailable
                    // with 503; that is a startup state, not a missing index.
                    if status.is_server_error() && std::time::Instant::now() < deadline {
                        std::thread::sleep(INDEX_CHECK_RETRY_DELAY);
                        continue;
                    }
                    if !status.is_success() {
                        return Err(Error::ValidationError(format!(
                            "elasticsearch index check failed for {index_url} ({status})"
                        )));
                    }
                    break true;
                }
                Err(error) => {
                    let retryable = error.is_timeout() || !error.is_connect();
                    if !retryable || std::time::Instant::now() >= deadline {
                        return Err(crate::string_error!(
                            "elasticsearch index check failed for {index_url}: {error}"
                        ));
                    }
                    std::thread::sleep(INDEX_CHECK_RETRY_DELAY);
                }
            }
        };
        if !exists {
            if !create_missing {
                return Err(Error::ValidationError(format!(
                    "elasticsearch index '{}' does not exist",
                    reference.index_name
                )));
            }
            reference.request(
                reqwest::Method::PUT,
                &format!("/{}", reference.index_name),
                Some(index_definition(reference)?),
            )?;
            return Ok(true);
        }
        let mapping = reference.request(
            reqwest::Method::GET,
            &format!("/{}/_mapping", reference.index_name),
            None,
        )?;
        let dims = mapping
            .pointer(&format!(
                "/{}/mappings/properties/{}/dims",
                reference.index_name,
                reference.schema.vector_field()
            ))
            .and_then(Value::as_u64);
        if dims != Some(reference.vector_dimensions as u64) {
            return Err(Error::ValidationError(format!(
                "elasticsearch index '{}' embedding dimensions do not match configured value {}",
                reference.index_name, reference.vector_dimensions
            )));
        }
        Ok(false)
    })
}

/// Writes one concept record: create, or rewrite the fact carrying the same scoped title.
pub fn upsert_elasticsearch_concept_record(
    reference: &ElasticsearchRef,
    prepared: &ConceptMemoryPrepared,
    vector: Vec<f32>,
) -> Result<MemoryRecord> {
    validate_vector(reference, &vector)?;
    let now = Utc::now().to_rfc3339();
    let created_at = match get_elasticsearch_memory_record(reference, &prepared.object_id) {
        Ok(MemoryRecord::Concept(record)) => record.created_at,
        Ok(other) => return Err(category_mismatch("concept", other.object_id())),
        Err(Error::NotFound) => now.clone(),
        Err(error) => return Err(error),
    };
    let document = json!({
        "key": prepared.key,
        "content": prepared.content,
        "access_level": prepared.access_level,
        "starts_at": prepared.starts_at,
        "sender_id_list": prepared.sender_id_list,
        "group_id_list": prepared.group_id_list,
        "created_at": created_at,
        "updated_at": now,
        "embedding": vector,
    });
    put_memory_document(reference, &prepared.object_id, document)?;
    get_elasticsearch_memory_record(reference, &prepared.object_id)
}

/// Writes one behavioral rule: create, or rewrite the rule carrying the same scoped title.
pub fn upsert_elasticsearch_behavioral_record(
    reference: &ElasticsearchRef,
    prepared: &BehavioralMemoryPrepared,
    vector: Vec<f32>,
) -> Result<MemoryRecord> {
    validate_vector(reference, &vector)?;
    let now = Utc::now().to_rfc3339();
    let created_at = match get_elasticsearch_memory_record(reference, &prepared.object_id) {
        Ok(MemoryRecord::Behavioral(record)) => record.created_at,
        Ok(other) => return Err(category_mismatch("behavioral", other.object_id())),
        Err(Error::NotFound) => now.clone(),
        Err(error) => return Err(error),
    };
    let document = json!({
        "key": prepared.key,
        "content": prepared.content,
        "access_level": prepared.access_level,
        "expires_at": prepared.expires_at,
        "sender_id_list": prepared.sender_id_list,
        "group_id_list": prepared.group_id_list,
        "created_at": created_at,
        "updated_at": now,
        "embedding": vector,
    });
    put_memory_document(reference, &prepared.object_id, document)?;
    get_elasticsearch_memory_record(reference, &prepared.object_id)
}

/// Writes one person profile: the QQ ID is the identity, so the profile overwrites it.
pub fn upsert_elasticsearch_person_record(
    reference: &ElasticsearchRef,
    write: &PersonMemoryWrite,
    vector: Vec<f32>,
) -> Result<MemoryRecord> {
    let prepared = prepare_person(write)?;
    validate_vector(reference, &vector)?;
    let object_id = get_person_by_qq_id(reference, &prepared.profile.qq_id)?
        .map(|record| record.object_id)
        .unwrap_or_else(|| {
            crate::storage::memory_object_id(
                crate::storage::MemoryCategory::Person,
                &prepared.profile.qq_id,
                &[],
                &[],
            )
        });
    let now = Utc::now().to_rfc3339();
    let created_at = match get_elasticsearch_memory_record(reference, &object_id) {
        Ok(MemoryRecord::Person(record)) => record.created_at,
        Ok(other) => return Err(category_mismatch("person", other.object_id())),
        Err(Error::NotFound) => now.clone(),
        Err(error) => return Err(error),
    };
    let document = person_document(&prepared.profile, prepared.access_level, &created_at, &now);
    put_memory_document(reference, &object_id, document)?;
    get_elasticsearch_memory_record(reference, &object_id)
}

/// Admin update at an explicit object id: identity fields must match the stored record.
pub fn update_elasticsearch_concept_record(
    reference: &ElasticsearchRef,
    object_id: &str,
    prepared: &ConceptMemoryPrepared,
    vector: Vec<f32>,
) -> Result<MemoryRecord> {
    validate_vector(reference, &vector)?;
    let MemoryRecord::Concept(existing) = get_elasticsearch_memory_record(reference, object_id)?
    else {
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
    let document = json!({
        "key": prepared.key,
        "content": prepared.content,
        "access_level": prepared.access_level,
        "starts_at": prepared.starts_at,
        "sender_id_list": prepared.sender_id_list,
        "group_id_list": prepared.group_id_list,
        "created_at": existing.created_at,
        "updated_at": now,
        "embedding": vector,
    });
    put_memory_document(reference, object_id, document)?;
    get_elasticsearch_memory_record(reference, object_id)
}

pub fn update_elasticsearch_behavioral_record(
    reference: &ElasticsearchRef,
    object_id: &str,
    prepared: &BehavioralMemoryPrepared,
    vector: Vec<f32>,
) -> Result<MemoryRecord> {
    validate_vector(reference, &vector)?;
    let MemoryRecord::Behavioral(existing) = get_elasticsearch_memory_record(reference, object_id)?
    else {
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
    let document = json!({
        "key": prepared.key,
        "content": prepared.content,
        "access_level": prepared.access_level,
        "expires_at": prepared.expires_at,
        "sender_id_list": prepared.sender_id_list,
        "group_id_list": prepared.group_id_list,
        "created_at": existing.created_at,
        "updated_at": now,
        "embedding": vector,
    });
    put_memory_document(reference, object_id, document)?;
    get_elasticsearch_memory_record(reference, object_id)
}

pub fn update_elasticsearch_person_record(
    reference: &ElasticsearchRef,
    object_id: &str,
    write: &PersonMemoryWrite,
    vector: Vec<f32>,
) -> Result<MemoryRecord> {
    let prepared = prepare_person(write)?;
    validate_vector(reference, &vector)?;
    let MemoryRecord::Person(existing) = get_elasticsearch_memory_record(reference, object_id)?
    else {
        return Err(Error::NotFound);
    };
    if existing.profile.qq_id != prepared.profile.qq_id {
        return Err(Error::ValidationError("person QQ ID is immutable".to_string()));
    }
    let now = Utc::now().to_rfc3339();
    let document =
        person_document(&prepared.profile, prepared.access_level, &existing.created_at, &now);
    put_memory_document(reference, object_id, document)?;
    get_elasticsearch_memory_record(reference, object_id)
}

fn person_document(
    profile: &crate::storage::PersonMemory,
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

fn category_mismatch(category: &str, key: &str) -> Error {
    Error::ValidationError(format!(
        "memory object '{key}' already exists as another category than {category}"
    ))
}

fn put_memory_document(
    reference: &ElasticsearchRef,
    object_id: &str,
    document: Value,
) -> Result<()> {
    reference.request(
        reqwest::Method::PUT,
        &format!("/{}/_doc/{}?refresh=wait_for", reference.index_name, object_id),
        Some(document),
    )?;
    Ok(())
}

pub fn delete_elasticsearch_memory_record(
    reference: &ElasticsearchRef,
    object_id: &str,
) -> Result<()> {
    reference.request(
        reqwest::Method::DELETE,
        &format!("/{}/_doc/{}?refresh=wait_for", reference.index_name, object_id),
        None,
    )?;
    Ok(())
}

pub fn upsert_elasticsearch_image(
    reference: &ElasticsearchRef,
    media: &PersistedMedia,
    description_vector: Vec<f32>,
    name_vector: Option<Vec<f32>>,
) -> Result<()> {
    validate_vector(reference, &description_vector)?;
    if let Some(vector) = name_vector.as_deref() {
        validate_vector(reference, vector)?;
    }
    let body = json!({"media_id":media.media_id,"original_source":media.original_source,"rustfs_path":media.rustfs_path,"name":media.name,"description":media.description,"description_vector":description_vector,"name_vector":name_vector,"mime_type":media.mime_type,"source":media.source.to_string()});
    reference.request(
        reqwest::Method::PUT,
        &format!("/{}/_doc/media-{}", reference.index_name, media.media_id),
        Some(body),
    )?;
    Ok(())
}

pub fn search_elasticsearch_images(
    reference: &ElasticsearchRef,
    name_query: Option<&str>,
    description_query: Option<&str>,
    name_vector: Option<&[f32]>,
    description_vector: Option<&[f32]>,
    limit: usize,
) -> Result<Vec<ElasticsearchImageSearchHit>> {
    let limit = limit.clamp(1, 50);
    let mut merged = Vec::new();
    if let Some(query) = name_query.map(str::trim).filter(|value| !value.is_empty()) {
        merged.extend(parse_image_hits(
            reference.request(
                reqwest::Method::POST,
                &format!("/{}/_search", reference.index_name),
                Some(json!({"size": limit, "query": {"match": {"name": query}}})),
            )?,
            true,
        ));
    }
    if let Some(query) = description_query.map(str::trim).filter(|value| !value.is_empty()) {
        merged.extend(parse_image_hits(
            reference.request(
                reqwest::Method::POST,
                &format!("/{}/_search", reference.index_name),
                Some(json!({"size": limit, "query": {"match": {"description": query}}})),
            )?,
            true,
        ));
    }
    for (field, vector) in
        [("name_vector", name_vector), ("description_vector", description_vector)]
    {
        if let Some(vector) = vector {
            validate_vector(reference, vector)?;
            merged.extend(parse_image_hits(reference.request(reqwest::Method::POST, &format!("/{}/_search", reference.index_name), Some(json!({"size": limit, "knn": {"field": field, "query_vector": vector, "k": limit, "num_candidates": (limit * 5).min(MAX_QUERY_CANDIDATES)}})))?, false));
        }
    }
    let mut deduped = std::collections::BTreeMap::<String, ElasticsearchImageSearchHit>::new();
    for hit in merged {
        match deduped.get_mut(&hit.object_id) {
            Some(existing) => {
                existing.keyword_match |= hit.keyword_match;
                existing.score = match (existing.score, hit.score) {
                    (Some(current), Some(incoming)) => Some(current.max(incoming)),
                    (Some(current), None) => Some(current),
                    (None, score) => score,
                };
            }
            None => {
                deduped.insert(hit.object_id.clone(), hit);
            }
        }
    }
    let mut results = deduped.into_values().collect::<Vec<_>>();
    results.sort_by(|left, right| {
        right
            .keyword_match
            .cmp(&left.keyword_match)
            .then_with(|| right.score.partial_cmp(&left.score).unwrap_or(std::cmp::Ordering::Equal))
    });
    results.truncate(limit);
    Ok(results)
}

fn parse_image_hits(value: Value, keyword_match: bool) -> Vec<ElasticsearchImageSearchHit> {
    value
        .pointer("/hits/hits")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|hit| {
            Some(ElasticsearchImageSearchHit {
                object_id: hit.get("_id")?.as_str()?.to_string(),
                properties: hit.get("_source")?.clone(),
                score: hit.get("_score").and_then(Value::as_f64),
                keyword_match,
            })
        })
        .collect()
}

pub fn list_elasticsearch_memory_keys(
    reference: &ElasticsearchRef,
    access: &AgentMemoryAccessContext,
    top_n: usize,
    query: Option<&str>,
) -> Result<Vec<MemorySearchHit>> {
    let query = query.unwrap_or_default().trim();
    let body = if query.is_empty() {
        json!({"size": top_n.min(MAX_QUERY_CANDIDATES), "sort": [{"updated_at": "desc"}], "query": access_filter(access)})
    } else {
        let pinyin_query = memory_pinyin_query(reference, query)?;
        memory_lexical_body(access, query, pinyin_query.as_deref(), memory_result_size(top_n))
    };
    parse_memory_hits(
        reference,
        reference.request(
            reqwest::Method::POST,
            &format!("/{}/_search", reference.index_name),
            Some(body),
        )?,
        access,
    )
}

pub fn search_elasticsearch_memory(
    reference: &ElasticsearchRef,
    access: &AgentMemoryAccessContext,
    query: &str,
    vector: &[f32],
    top_n: usize,
) -> Result<Vec<MemorySearchHit>> {
    validate_vector(reference, vector)?;
    let size = memory_result_size(top_n);
    let window = (size * 5).min(MAX_QUERY_CANDIDATES);
    let pinyin_query = memory_pinyin_query(reference, query)?;
    // Lexical and vector retrieval are issued as separate plain searches and
    // fused locally: the server-side `rrf` retriever is an Enterprise feature,
    // and a retriever cannot be combined with a top-level sort.
    let lexical = parse_memory_hits(
        reference,
        reference.request(
            reqwest::Method::POST,
            &format!("/{}/_search", reference.index_name),
            Some(memory_lexical_body(access, query, pinyin_query.as_deref(), window)),
        )?,
        access,
    )?;
    let semantic = parse_memory_hits(
        reference,
        reference.request(
            reqwest::Method::POST,
            &format!("/{}/_search", reference.index_name),
            Some(memory_vector_body(access, vector, window, reference.memory_min_similarity)),
        )?,
        access,
    )?;
    let mut hits = super::memory::fuse_memory_hits(&[lexical, semantic]);
    super::memory::sort_memory_search_hits(query, &mut hits);
    hits.truncate(size);
    Ok(hits)
}

fn memory_result_size(top_n: usize) -> usize {
    top_n.clamp(1, 50)
}

fn memory_pinyin_query(reference: &ElasticsearchRef, query: &str) -> Result<Option<String>> {
    let query = query.trim();
    if query.chars().count() < 2 {
        return Ok(None);
    }
    if query.chars().all(|c| c.is_ascii_alphabetic() || c == ' ') {
        return Ok(Some(query.to_lowercase()));
    }
    if !query
        .chars()
        .any(|c| matches!(c, '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}'))
    {
        return Ok(None);
    }
    // Use the installed plugin's pronunciation dictionary, but exclude initials
    // and original Hanzi from the query. Existing pinyin fields already contain
    // full syllables, so this also works without changing or rebuilding indices.
    let analyzed = reference.request(
        reqwest::Method::POST,
        &format!("/{}/_analyze", reference.index_name),
        Some(json!({
            "tokenizer":"standard",
            "filter":["lowercase",{
                "type":"pinyin",
                "keep_first_letter":false,
                "keep_original":false,
                "keep_full_pinyin":true,
                "keep_joined_full_pinyin":false
            }],
            "text":query
        })),
    )?;
    let tokens = analyzed.get("tokens").and_then(Value::as_array).ok_or_else(|| {
        crate::string_error!("elasticsearch pinyin analysis returned no tokens array")
    })?;
    let syllables = tokens
        .iter()
        .filter_map(|token| token.get("token").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join(" ");
    Ok((!syllables.is_empty()).then_some(syllables))
}

fn memory_lexical_body(
    access: &AgentMemoryAccessContext,
    query: &str,
    pinyin_query: Option<&str>,
    size: usize,
) -> Value {
    json!({"size":size,"query":memory_lexical_query(access, query, pinyin_query),"sort":[{"_score":"desc"},{"updated_at":"desc"}]})
}

fn memory_lexical_query(
    access: &AgentMemoryAccessContext,
    query: &str,
    pinyin_query: Option<&str>,
) -> Value {
    let person = access.category == super::MemoryCategory::Person;
    let fields = if person {
        vec!["name^5", "preferred_name^5", "aliases^5", "impression^2"]
    } else {
        vec!["key^5", "content^2"]
    };
    let mut clauses = vec![
        json!({"multi_match":{"query":query,"fields":fields,"type":"best_fields","operator":"and"}}),
        json!({"multi_match":{"query":query,"fields":fields,"type":"phrase","boost":3}}),
    ];
    if person {
        clauses.push(json!({"term":{"qq_id":{"value":query,"boost":10}}}));
        if query.chars().all(|c| c.is_ascii_digit()) {
            clauses.push(json!({"wildcard":{"qq_id":{"value":format!("*{query}*")}}}));
        }
    }
    // All full syllables must occur in one field. Analyze these already-converted
    // terms with standard, so the field analyzer cannot add noisy initials.
    if let Some(pinyin_query) = pinyin_query {
        let fields = if person {
            vec![
                "name.pinyin^5",
                "preferred_name.pinyin^5",
                "aliases.pinyin^5",
                "impression.pinyin^2",
            ]
        } else {
            vec!["key.pinyin^5", "content.pinyin^2"]
        };
        clauses.push(
            json!({"multi_match":{"query":pinyin_query,"fields":fields,"analyzer":"standard","operator":"and","boost":0.5}}),
        );
    }
    json!({"bool":{"filter":[access_filter(access)],"should":clauses,"minimum_should_match":1}})
}

pub(crate) fn get_person_by_qq_id(
    reference: &ElasticsearchRef,
    qq_id: &str,
) -> Result<Option<crate::storage::memory::PersonMemoryRecord>> {
    let access = AgentMemoryAccessContext {
        category: super::MemoryCategory::Person,
        admin: true,
        skip_expiry_extend: true,
        ..Default::default()
    };
    let response = reference.request(
        reqwest::Method::POST,
        &format!("/{}/_search", reference.index_name),
        Some(json!({"size":1,"query":{"term":{"qq_id":qq_id}},"sort":[{"updated_at":"desc"}]})),
    )?;
    Ok(parse_memory_hits(reference, response, &access)?
        .into_iter()
        .filter_map(|hit| match hit.record {
            MemoryRecord::Person(record) => Some(record),
            _ => None,
        })
        .next())
}

fn memory_vector_body(
    access: &AgentMemoryAccessContext,
    vector: &[f32],
    size: usize,
    min_similarity: f64,
) -> Value {
    json!({"size":size,"knn":{"field":"embedding","query_vector":vector,"k":size,"num_candidates":(size * 5).min(1000),"similarity":min_similarity,"filter":access_filter(access)}})
}

/// The filter every memory query of a category runs under. Person profiles carry no
/// scope or expiry, so their queries run unfiltered.
fn access_filter(access: &AgentMemoryAccessContext) -> Value {
    let mut filters = Vec::new();
    if access.category == super::MemoryCategory::Behavioral {
        filters.push(json!({"bool":{"must_not":[{"range":{"expires_at":{"lte":"now"}}}]}}));
    }
    if access.category != super::MemoryCategory::Person && !access.admin {
        for (field, id) in [
            ("group_id_list", access.group_id.as_ref()),
            ("sender_id_list", access.sender_id.as_ref()),
        ] {
            let mut alternatives = vec![json!({"bool":{"must_not":[{"exists":{"field":field}}]}})];
            if let Some(id) = id {
                alternatives.push(json!({"term":{field:id}}));
            }
            filters.push(json!({"bool":{"should":alternatives,"minimum_should_match":1}}));
        }
    }
    json!({"bool":{"filter":filters}})
}

fn parse_memory_hits(
    reference: &ElasticsearchRef,
    value: Value,
    access: &AgentMemoryAccessContext,
) -> Result<Vec<MemorySearchHit>> {
    let category = reference.schema.memory_category().ok_or_else(|| {
        Error::ValidationError(format!(
            "elasticsearch index '{}' is not a memory index",
            reference.index_name
        ))
    })?;
    let hits = value
        .pointer("/hits/hits")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|hit| {
            let object_id = hit.get("_id")?.as_str()?.to_string();
            let record =
                parse_memory_record(category, hit.get("_source")?.clone(), &object_id).ok()?;
            memory_is_accessible(&record, access).then(|| MemorySearchHit {
                record,
                distance: hit.get("_score").and_then(Value::as_f64),
            })
        })
        .collect::<Vec<_>>();
    Ok(hits)
}

fn parse_memory_record(
    category: crate::storage::MemoryCategory,
    source: Value,
    object_id: &str,
) -> Result<MemoryRecord> {
    match category {
        crate::storage::MemoryCategory::Concept => {
            Ok(MemoryRecord::Concept(parse_concept_record(source, object_id)?))
        }
        crate::storage::MemoryCategory::Person => {
            Ok(MemoryRecord::Person(parse_person_record(source, object_id)?))
        }
        crate::storage::MemoryCategory::Behavioral => {
            Ok(MemoryRecord::Behavioral(parse_behavioral_record(source, object_id)?))
        }
    }
}

pub fn get_elasticsearch_memory_record(
    reference: &ElasticsearchRef,
    object_id: &str,
) -> Result<MemoryRecord> {
    let category = reference.schema.memory_category().ok_or_else(|| {
        Error::ValidationError(format!(
            "elasticsearch index '{}' is not a memory index",
            reference.index_name
        ))
    })?;
    let value = reference.request(
        reqwest::Method::GET,
        &format!("/{}/_doc/{}", reference.index_name, object_id),
        None,
    )?;
    let source = value.get("_source").ok_or_else(|| {
        Error::ValidationError("elasticsearch memory document is missing _source".to_string())
    })?;
    parse_memory_record(category, source.clone(), object_id)
}

fn validate_vector(reference: &ElasticsearchRef, vector: &[f32]) -> Result<()> {
    if vector.len() != reference.vector_dimensions {
        return Err(Error::ValidationError(format!(
            "elasticsearch vector dimension mismatch: expected {}, got {}",
            reference.vector_dimensions,
            vector.len()
        )));
    }
    Ok(())
}

fn index_definition(reference: &ElasticsearchRef) -> Result<Value> {
    let text = json!({"type":"text","analyzer":"icu_analyzer","fields":{"pinyin":{"type":"text","analyzer":"pinyin_analyzer","search_analyzer":"pinyin_analyzer"}}});
    let embedding = json!({"type":"dense_vector","dims":reference.vector_dimensions,"index":true,"similarity":"cosine"});
    let properties = match reference.schema {
        // Each category index maps exactly the fields its records carry.
        RetrievalSchema::ConceptMemory => {
            json!({"access_level":{"type":"integer"},"starts_at":{"type":"date"},"key":text,"content":text,"embedding":embedding,"sender_id_list":{"type":"keyword"},"group_id_list":{"type":"keyword"},"created_at":{"type":"date"},"updated_at":{"type":"date"}})
        }
        RetrievalSchema::PersonMemory => {
            json!({"access_level":{"type":"integer"},"qq_id":{"type":"keyword"},"name":text,"preferred_name":text,"aliases":text,"impression":text,"guideline":text,"embedding":embedding,"created_at":{"type":"date"},"updated_at":{"type":"date"}})
        }
        RetrievalSchema::BehavioralMemory => {
            json!({"access_level":{"type":"integer"},"key":text,"content":text,"embedding":embedding,"expires_at":{"type":"date"},"sender_id_list":{"type":"keyword"},"group_id_list":{"type":"keyword"},"created_at":{"type":"date"},"updated_at":{"type":"date"}})
        }
        RetrievalSchema::ImageSemantic => {
            json!({"media_id":{"type":"keyword"},"original_source":{"type":"keyword","index":false},"rustfs_path":{"type":"keyword"},"name":text,"description":text,"description_vector":{"type":"dense_vector","dims":reference.vector_dimensions,"index":true,"similarity":"cosine"},"name_vector":{"type":"dense_vector","dims":reference.vector_dimensions,"index":true,"similarity":"cosine"},"mime_type":{"type":"keyword"},"source":{"type":"keyword"}})
        }
        RetrievalSchema::QqMessage => {
            return Err(Error::ValidationError(
                "QQ message vectors are not supported by the elasticsearch retrieval backend"
                    .to_string(),
            ))
        }
    };
    Ok(
        json!({"settings":{"analysis":{"analyzer":{"pinyin_analyzer":{"tokenizer":"standard","filter":["lowercase","pinyin"]}}}},"mappings":{"properties":properties}}),
    )
}

#[async_trait::async_trait]
impl Connection for ElasticsearchRef {
    async fn connect(&mut self) -> Result<()> {
        if self.is_closed() {
            let client = crate::runtime::run_outside_async_runtime(|| {
                Client::builder()
                    .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
                    .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT_SECS))
                    .default_headers(self.headers.clone())
                    .build()
                    .map_err(|error| {
                        crate::string_error!("build elasticsearch client failed: {error}")
                    })
            })?;
            *self.client.write().unwrap_or_else(|e| e.into_inner()) = Some(client);
        }
        if let Err(error) = self.health_check().await {
            self.close().await?;
            return Err(error);
        }
        Ok(())
    }

    async fn health_check(&self) -> Result<()> {
        let reference = self.clone();
        tokio::task::spawn_blocking(move || {
            reference.request(reqwest::Method::GET, "/_cluster/health", None).map(|_| ())
        })
        .await
        .map_err(|error| crate::string_error!("Elasticsearch probe failed: {error}"))?
    }

    async fn close(&self) -> Result<()> {
        let client = self.client.write().unwrap_or_else(|e| e.into_inner()).take();
        crate::runtime::run_outside_async_runtime(|| {
            drop(client);
            Ok(())
        })
    }

    fn is_closed(&self) -> bool {
        self.client.read().unwrap_or_else(|e| e.into_inner()).is_none()
    }
}

impl ElasticsearchRef {
    pub async fn from_config(
        config: &ElasticsearchConnection,
        schema: RetrievalSchema,
    ) -> Result<Self> {
        let mut connection = Self::new(config.clone(), schema)?;
        connection.connect().await?;
        Ok(connection)
    }
}
