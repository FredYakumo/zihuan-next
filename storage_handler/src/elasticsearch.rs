use std::time::Duration;

use base64::Engine;
use chrono::Utc;
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use serde_json::{json, Value};
use uuid::Uuid;

use zihuan_core::error::{Error, Result};
use zihuan_core::weaviate::WeaviateCollectionSchema;

use crate::{
    memory_is_accessible, AgentMemoryAccessContext, AgentMemoryRecord, AgentMemorySearchHit, AgentMemoryUpsert,
    ElasticsearchConnection,
};
use zihuan_core::ims_bot_adapter::models::message::PersistedMedia;

const REQUEST_TIMEOUT_SECS: u64 = 30;
const INDEX_CHECK_RETRY_ATTEMPTS: usize = 15;
const INDEX_CHECK_RETRY_DELAY: Duration = Duration::from_secs(1);
const MAX_QUERY_CANDIDATES: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElasticsearchIndexSchema {
    AgentMemory,
    ImageSemantic,
}

impl From<WeaviateCollectionSchema> for ElasticsearchIndexSchema {
    fn from(value: WeaviateCollectionSchema) -> Self {
        match value {
            WeaviateCollectionSchema::AgentMemory => Self::AgentMemory,
            WeaviateCollectionSchema::ImageSemantic => Self::ImageSemantic,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ElasticsearchRef {
    client: Client,
    base_url: String,
    pub index_name: String,
    pub schema: ElasticsearchIndexSchema,
    pub vector_dimensions: usize,
}

impl ElasticsearchRef {
    pub fn new(config: ElasticsearchConnection) -> Result<Self> {
        let base_url = config.base_url.trim().trim_end_matches('/').to_string();
        if !(base_url.starts_with("http://") || base_url.starts_with("https://")) {
            return Err(Error::ValidationError(
                "elasticsearch base_url must use http:// or https://".to_string(),
            ));
        }
        if config.index_name.trim().is_empty() || config.vector_dimensions == 0 {
            return Err(Error::ValidationError(
                "elasticsearch index_name and vector_dimensions are required".to_string(),
            ));
        }
        let username = config.username.as_deref().map(str::trim).filter(|value| !value.is_empty());
        let password = config.password.as_deref().map(str::trim).filter(|value| !value.is_empty());
        if username.is_some() != password.is_some() {
            return Err(Error::ValidationError(
                "elasticsearch username and password must be supplied together".to_string(),
            ));
        }
        let mut headers = HeaderMap::new();
        if let Some(api_key) = config.api_key.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("ApiKey {api_key}"))
                    .map_err(|_| Error::ValidationError("invalid elasticsearch api_key".to_string()))?,
            );
        } else if let (Some(username), Some(password)) = (username, password) {
            let credentials = base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"));
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&format!("Basic {credentials}"))
                    .map_err(|_| Error::ValidationError("invalid elasticsearch basic auth".to_string()))?,
            );
        }
        let builder = Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .default_headers(headers);
        Ok(Self {
            client: builder
                .build()
                .map_err(|error| Error::StringError(format!("build elasticsearch client failed: {error}")))?,
            base_url,
            index_name: config.index_name.trim().to_string(),
            schema: config.collection_schema.into(),
            vector_dimensions: config.vector_dimensions,
        })
    }

    fn request(&self, method: reqwest::Method, path: &str, body: Option<Value>) -> Result<Value> {
        let url = format!("{}{}", self.base_url, path);
        let request = self.client.request(method, url);
        let request = if let Some(body) = body {
            request.json(&body)
        } else {
            request
        };
        let response = request
            .send()
            .map_err(|error| Error::StringError(format!("elasticsearch request failed: {error}")))?;
        let status = response.status();
        let value = response.json::<Value>().unwrap_or(Value::Null);
        if !status.is_success() {
            return Err(Error::ValidationError(format!(
                "elasticsearch request failed ({status}): {value}"
            )));
        }
        Ok(value)
    }
}

pub fn ensure_elasticsearch_index(reference: &ElasticsearchRef, create_missing: bool) -> Result<bool> {
    let index_url = format!("{}/{}", reference.base_url, reference.index_name);
    let mut attempt = 0;
    let exists = loop {
        match reference.client.head(&index_url).send() {
            Ok(response) => break response.status().is_success(),
            Err(error) if attempt + 1 == INDEX_CHECK_RETRY_ATTEMPTS => {
                return Err(Error::StringError(format!(
                    "elasticsearch index check failed after {INDEX_CHECK_RETRY_ATTEMPTS} attempts: {error}"
                )));
            }
            Err(_) => {
                attempt += 1;
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
            Some(index_definition(reference)),
        )?;
        return Ok(true);
    }
    let mapping = reference.request(reqwest::Method::GET, &format!("/{}/_mapping", reference.index_name), None)?;
    let dims = mapping
        .pointer(&format!("/{}/mappings/properties/embedding/dims", reference.index_name))
        .and_then(Value::as_u64);
    if dims != Some(reference.vector_dimensions as u64) {
        return Err(Error::ValidationError(format!(
            "elasticsearch index '{}' embedding dimensions do not match configured value {}",
            reference.index_name, reference.vector_dimensions
        )));
    }
    Ok(false)
}

pub fn create_elasticsearch_memory_record(
    reference: &ElasticsearchRef,
    input: &AgentMemoryUpsert,
    vector: Vec<f32>,
) -> Result<AgentMemoryRecord> {
    validate_vector(reference, &vector)?;
    let key = input.key.trim();
    let value = input.value.trim();
    if key.is_empty() || value.is_empty() {
        return Err(Error::ValidationError("memory title and value must not be empty".to_string()));
    }
    let now = Utc::now().to_rfc3339();
    let object_id = Uuid::new_v4().to_string();
    let document = json!({"key": key, "value": value, "embedding": vector, "expires_at": input.expires_at, "sender_id_list": input.sender_id_list, "group_id_list": input.group_id_list, "created_at": now, "updated_at": now});
    reference.request(
        reqwest::Method::PUT,
        &format!("/{}/_doc/{}", reference.index_name, object_id),
        Some(document),
    )?;
    get_memory_record(reference, &object_id)
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

pub fn list_elasticsearch_memory_keys(
    reference: &ElasticsearchRef,
    access: &AgentMemoryAccessContext,
    top_n: usize,
    query: Option<&str>,
) -> Result<Vec<AgentMemorySearchHit>> {
    let query = query.unwrap_or_default().trim();
    let body = if query.is_empty() {
        json!({"size": top_n.min(MAX_QUERY_CANDIDATES), "sort": [{"updated_at": "desc"}], "query": access_filter(access)})
    } else {
        memory_search_body(reference, access, query, None, top_n)
    };
    parse_memory_hits(
        reference.request(reqwest::Method::POST, &format!("/{}/_search", reference.index_name), Some(body))?,
        access,
    )
}

pub fn search_elasticsearch_memory(
    reference: &ElasticsearchRef,
    access: &AgentMemoryAccessContext,
    query: &str,
    vector: &[f32],
    top_n: usize,
) -> Result<Vec<AgentMemorySearchHit>> {
    validate_vector(reference, vector)?;
    let response = reference.request(
        reqwest::Method::POST,
        &format!("/{}/_search", reference.index_name),
        Some(memory_search_body(reference, access, query, Some(vector), top_n)),
    )?;
    parse_memory_hits(response, access)
}

fn memory_search_body(
    reference: &ElasticsearchRef,
    access: &AgentMemoryAccessContext,
    query: &str,
    vector: Option<&[f32]>,
    top_n: usize,
) -> Value {
    let size = top_n.max(1).min(20);
    let window = (size * 5).min(MAX_QUERY_CANDIDATES);
    let lexical = json!({"bool":{"filter":[access_filter(access)],"should":[
        {"multi_match":{"query":query,"fields":["key^5","value^2"],"type":"best_fields","operator":"and"}},
        {"multi_match":{"query":query,"fields":["key.pinyin^4","value.pinyin"],"type":"best_fields"}}
    ],"minimum_should_match":1}});
    let mut retrievers = vec![json!({"standard":{"query":lexical}})];
    if let Some(vector) = vector {
        retrievers.push(json!({"knn":{"field":"embedding","query_vector":vector,"k":window,"num_candidates":window,"filter":access_filter(access)}}));
    }
    if retrievers.len() == 1 {
        return json!({"size":size,"query":retrievers.remove(0)["standard"]["query"].clone(),"sort":[{"_score":"desc"},{"updated_at":"desc"}]});
    }
    json!({"size":size,"retriever":{"rrf":{"retrievers":retrievers,"rank_window_size":window,"rank_constant":60}},"sort":[{"updated_at":"desc"}]})
}

fn access_filter(access: &AgentMemoryAccessContext) -> Value {
    if access.admin {
        return json!({"bool":{"must_not":[{"range":{"expires_at":{"lte":"now"}}}]}});
    }
    let mut should =
        vec![json!({"bool":{"must_not":[{"exists":{"field":"group_id_list"}},{"exists":{"field":"sender_id_list"}}]}})];
    if let Some(group_id) = access.group_id.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
        should.push(json!({"terms":{"group_id_list":[group_id]}}));
    }
    if let Some(sender_id) = access.sender_id.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
        should.push(json!({"bool":{"must_not":[{"exists":{"field":"group_id_list"}}],"filter":[{"terms":{"sender_id_list":[sender_id]}}]}}));
    }
    json!({"bool":{"must_not":[{"range":{"expires_at":{"lte":"now"}}}],"should":should,"minimum_should_match":1}})
}

fn parse_memory_hits(value: Value, access: &AgentMemoryAccessContext) -> Result<Vec<AgentMemorySearchHit>> {
    let hits = value
        .pointer("/hits/hits")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|hit| {
            let source = hit.get("_source")?;
            let record = AgentMemoryRecord {
                object_id: hit.get("_id")?.as_str()?.to_string(),
                key: source.get("key")?.as_str()?.to_string(),
                value: source.get("value")?.as_str()?.to_string(),
                expires_at: source.get("expires_at").and_then(Value::as_str).map(ToOwned::to_owned),
                sender_id_list: string_list(source, "sender_id_list"),
                group_id_list: string_list(source, "group_id_list"),
                created_at: source.get("created_at")?.as_str()?.to_string(),
                updated_at: source.get("updated_at")?.as_str()?.to_string(),
            };
            memory_is_accessible(&record, access).then_some(AgentMemorySearchHit {
                record,
                distance: hit.get("_score").and_then(Value::as_f64),
            })
        })
        .collect::<Vec<_>>();
    Ok(hits)
}

fn get_memory_record(reference: &ElasticsearchRef, object_id: &str) -> Result<AgentMemoryRecord> {
    let value = reference.request(
        reqwest::Method::GET,
        &format!("/{}/_doc/{}", reference.index_name, object_id),
        None,
    )?;
    let source = value
        .get("_source")
        .ok_or_else(|| Error::ValidationError("elasticsearch memory document is missing _source".to_string()))?;
    Ok(AgentMemoryRecord {
        object_id: object_id.to_string(),
        key: source.get("key").and_then(Value::as_str).unwrap_or_default().to_string(),
        value: source.get("value").and_then(Value::as_str).unwrap_or_default().to_string(),
        expires_at: source.get("expires_at").and_then(Value::as_str).map(ToOwned::to_owned),
        sender_id_list: string_list(source, "sender_id_list"),
        group_id_list: string_list(source, "group_id_list"),
        created_at: source.get("created_at").and_then(Value::as_str).unwrap_or_default().to_string(),
        updated_at: source.get("updated_at").and_then(Value::as_str).unwrap_or_default().to_string(),
    })
}

fn string_list(source: &Value, field: &str) -> Vec<String> {
    source
        .get(field)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(ToOwned::to_owned)
        .collect()
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

fn index_definition(reference: &ElasticsearchRef) -> Value {
    let text = json!({"type":"text","analyzer":"icu_analyzer","fields":{"pinyin":{"type":"text","analyzer":"pinyin_analyzer","search_analyzer":"pinyin_analyzer"}}});
    let properties = match reference.schema {
        ElasticsearchIndexSchema::AgentMemory => {
            json!({"key":text,"value":text,"embedding":{"type":"dense_vector","dims":reference.vector_dimensions,"index":true,"similarity":"cosine"},"expires_at":{"type":"date"},"sender_id_list":{"type":"keyword"},"group_id_list":{"type":"keyword"},"created_at":{"type":"date"},"updated_at":{"type":"date"}})
        }
        ElasticsearchIndexSchema::ImageSemantic => {
            json!({"media_id":{"type":"keyword"},"original_source":{"type":"keyword","index":false},"rustfs_path":{"type":"keyword"},"name":text,"description":text,"description_vector":{"type":"dense_vector","dims":reference.vector_dimensions,"index":true,"similarity":"cosine"},"name_vector":{"type":"dense_vector","dims":reference.vector_dimensions,"index":true,"similarity":"cosine"},"mime_type":{"type":"keyword"},"source":{"type":"keyword"}})
        }
    };
    json!({"settings":{"analysis":{"analyzer":{"pinyin_analyzer":{"tokenizer":"standard","filter":["lowercase","pinyin"]}}}},"mappings":{"properties":properties}})
}
