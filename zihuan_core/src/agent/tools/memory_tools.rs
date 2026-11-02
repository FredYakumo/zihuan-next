use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use chrono::{Duration, Utc};
use serde_json::{json, Value};

use crate::agent::declarative_agent::{AgentDefinition, AgentHost, AgentOutputMode};
use crate::agent::tools::Tool;
use crate::error::{Error, Result};
use crate::graph::function_graph::FunctionPortDef;
use crate::graph::DataType;
use crate::model_inference::llm::embedding_base::EmbeddingBase;
use crate::model_inference::llm::llm_base::LLMBase;
use crate::model_inference::llm::tooling::FunctionTool;
use crate::retrieval::RetrievalStoreRef;
use crate::storage::{
    list_memory_in_store, memory_is_accessible, search_memory_in_store, upsert_memory_in_store,
    AgentMemoryAccessContext, AgentMemoryUpsert, LocalMemoryStore, MemoryAttributes,
    MemoryCategory, PersonMemory,
};
use crate::system_config::current_memory_agent_prompts;

const DEFAULT_MEMORY_TOP_N: i64 = 5;
const MAX_MEMORY_TOP_N: i64 = 20;

#[derive(Clone)]
pub struct MemoryAgentResources {
    pub memory_backend: MemoryBackend,
    pub embedding_model: Option<Arc<dyn EmbeddingBase>>,
    pub llm: Arc<dyn LLMBase>,
    pub access: AgentMemoryAccessContext,
}

/// Where agent memory is stored.
///
/// External backends are reached through the agent's unified retrieval store,
/// which selects the concrete Weaviate/Elasticsearch collection at call time.
#[derive(Clone)]
pub enum MemoryBackend {
    LocalFile(Arc<LocalMemoryStore>),
    RetrievalStore(Arc<RetrievalStoreRef>),
}

pub struct ListMemoryKeysTool {
    resources: MemoryAgentResources,
}
impl ListMemoryKeysTool {
    pub fn new(resources: MemoryAgentResources) -> Self {
        Self { resources }
    }
}

/// Memory tools directly available to Brain and selected sub agents.
pub const MEMORY_TOOL_IDS: [&str; 3] = ["list_memory_keys", "search_memory", "update_memory"];

/// One memory title returned by [`relevant_memory_titles`].
#[derive(Debug, Clone)]
pub struct MemoryTitleHit {
    pub title: String,
    /// Relevance after the backend's normalization (`higher is better`); `None` for backends
    /// that do not rank results (local-file memory returns query-filtered titles only).
    pub relevance: Option<f64>,
}

/// Searches agent memory for the titles most relevant to `query`.
///
/// This is the cheap counterpart of [`SearchMemoryTool`]: it reads only titles (never the
/// stored content), so a turn can hint at existing memories without loading them. Ranking
/// remains the backend's, and results are ordered by relevance when the backend reports it.
/// The lookup is read-only: unlike an agent-initiated search it never extends the expiry of the
/// records it touches.
pub fn relevant_memory_titles(
    resources: &MemoryAgentResources,
    query: &str,
    top_n: usize,
) -> Result<Vec<MemoryTitleHit>> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let limit = top_n.max(1);
    let access = AgentMemoryAccessContext {
        skip_expiry_extend: true,
        ..resources.access.clone()
    };
    let hits = match &resources.memory_backend {
        MemoryBackend::LocalFile(store) => store.list(Some(query), limit)?,
        MemoryBackend::RetrievalStore(store) => search_memory_in_store(
            store,
            &access,
            query,
            &embedding_vector(resources.embedding_model.as_deref(), query)?,
            limit,
        )?,
    };
    let mut titles = hits
        .into_iter()
        .map(|hit| MemoryTitleHit {
            title: hit.record.key,
            relevance: hit.distance,
        })
        .collect::<Vec<_>>();
    titles.sort_by(|left, right| {
        right
            .relevance
            .unwrap_or_default()
            .partial_cmp(&left.relevance.unwrap_or_default())
            .unwrap_or(Ordering::Equal)
    });
    Ok(titles)
}

pub const MEMORY_AGENT_ID: &str = "memory_agent";
const MEMORY_AGENT_NAME: &str = "Memory";
/// Registers the built-in memory tools on an [`AgentHost`] under the ids referenced by the
/// memory agent definition.
pub fn register_memory_tools(host: &mut AgentHost, resources: MemoryAgentResources) {
    host.register_tool("list_memory_keys", Arc::new(ListMemoryKeysTool::new(resources.clone())));
    host.register_tool("search_memory", Arc::new(SearchMemoryTool::new(resources.clone())));
    host.register_tool("update_memory", Arc::new(RememberMemoryTool::new(resources)));
}

fn memory_string_port(name: &str, description: &str, required: bool) -> FunctionPortDef {
    FunctionPortDef {
        name: name.to_string(),
        data_type: DataType::String,
        description: description.to_string(),
        required,
    }
}

/// Builds the memory agent definition from the configurable prompts. This replaces the
/// former built-in `memory_agent.yaml`; the Procedure owns recall and exposes only full search.
pub fn memory_agent_definition(
    prompts: &crate::system_config::MemoryAgentPrompts,
) -> AgentDefinition {
    AgentDefinition {
        id: MEMORY_AGENT_ID.to_string(),
        name: MEMORY_AGENT_NAME.to_string(),
        description: "Recall information relevant to the current user input".to_string(),
        builtin: true,
        inputs: vec![memory_string_port("content", "Current input with verified sender QQ ID", true)],
        outputs: vec![memory_string_port("result", "Recall summary", true)],
        system_prompt: format!("{}\nYou are the Memory Agent Procedure phase. Only recall, never write. Search relevant facts, people and behavior using FullSearchConceptMemory. QQ IDs identify people; never infer shared identity from aliases. When the user challenges a recollection use force_check=true. Query the current sender's QQ ID as well as topics. Do not invent memories.", prompts.system_prompt),
        user_prompt: Some("{content}".to_string()),
        prompt_parts: vec![],
        output_mode: AgentOutputMode::Text,
        llm_kind: crate::agent::LLM_KIND_MAIN.to_string(),
        progress_message: None,
        include_graph_tools: false,
        run_duration: crate::tool_runtime::ToolRunDuration::default(),
        tool_ids: vec!["FullSearchConceptMemory".to_string()],
    }
}

fn memory_fingerprint(record: &crate::storage::AgentMemoryRecord) -> Result<String> {
    use sha2::{Digest, Sha256};
    let content = serde_json::to_vec(&(&record.value, &record.attributes))?;
    Ok(format!(
        "memory:{}:{}:{}",
        record.attributes.category.schema().as_str(),
        record.object_id,
        hex::encode(Sha256::digest(content))
    ))
}

fn category_argument(arguments: &Value) -> Result<MemoryCategory> {
    serde_json::from_value(
        arguments
            .get("category")
            .cloned()
            .ok_or_else(|| Error::ValidationError("category is required".to_string()))?,
    )
    .map_err(Error::from)
}

pub fn search_category(
    backend: &MemoryBackend,
    embedding_model: Option<&dyn EmbeddingBase>,
    access_context: &AgentMemoryAccessContext,
    category: MemoryCategory,
    query: Option<&str>,
    limit: usize,
) -> Result<Vec<crate::storage::AgentMemorySearchHit>> {
    let access = AgentMemoryAccessContext {
        category,
        skip_expiry_extend: true,
        ..access_context.clone()
    };
    if category == MemoryCategory::Person
        && query.is_some_and(|query| !query.is_empty() && query.chars().all(|c| c.is_ascii_digit()))
    {
        let identity = AgentMemoryUpsert {
            attributes: MemoryAttributes { category, ..Default::default() },
            key: format!("person-{}", query.unwrap()),
            ..Default::default()
        };
        let record = match backend {
            MemoryBackend::LocalFile(store) => store.get(&identity)?,
            MemoryBackend::RetrievalStore(store) => {
                crate::storage::get_memory_in_store(store, &identity)?
            }
        };
        if let Some(record) = record.filter(|record| memory_is_accessible(record, &access)) {
            return Ok(vec![crate::storage::AgentMemorySearchHit { record, distance: None }]);
        }
    }

    let mut hits = match backend {
        MemoryBackend::LocalFile(store) => store.list_category(category, query, 100)?,
        MemoryBackend::RetrievalStore(store) => match query {
            Some(query) => search_memory_in_store(
                store,
                &access,
                query,
                &embedding_vector(embedding_model, query)?,
                limit.max(20),
            )?,
            None => list_memory_in_store(store, &access, 100)?,
        },
    };
    hits.retain(|hit| memory_is_accessible(&hit.record, &access));
    if category == MemoryCategory::Concept {
        hits.sort_by(|a, b| {
            b.record
                .attributes
                .starts_at
                .as_ref()
                .unwrap_or(&b.record.updated_at)
                .cmp(a.record.attributes.starts_at.as_ref().unwrap_or(&a.record.updated_at))
        });
    }
    hits.truncate(limit);
    if !access_context.skip_expiry_extend && category == MemoryCategory::Behavioral {
        let _guard = crate::storage::MEMORY_MUTATION_LOCK
            .lock()
            .map_err(|_| crate::string_error!("memory mutation lock poisoned"))?;
        for hit in &hits {
            let identity = AgentMemoryUpsert {
                attributes: hit.record.attributes.clone(),
                key: hit.record.key.clone(),
                sender_id_list: hit.record.sender_id_list.clone(),
                group_id_list: hit.record.group_id_list.clone(),
                ..Default::default()
            };
            let latest = match backend {
                MemoryBackend::LocalFile(store) => store.get(&identity)?,
                MemoryBackend::RetrievalStore(store) => {
                    crate::storage::get_memory_in_store(store, &identity)?
                }
            };
            let Some(record) = latest.filter(|record| memory_is_accessible(record, &access)) else {
                continue;
            };
            if let Some(expiry) = record
                .expires_at
                .as_ref()
                .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
            {
                let now = Utc::now();
                let next =
                    (now + (expiry.with_timezone(&Utc) - now) * 2).min(now + Duration::days(365));
                let input = AgentMemoryUpsert {
                    attributes: record.attributes,
                    key: record.key,
                    value: record.value,
                    expires_at: Some(next.to_rfc3339()),
                    sender_id_list: record.sender_id_list,
                    group_id_list: record.group_id_list,
                };
                write_memory(backend, embedding_model, &input)?;
            }
        }
    }
    Ok(hits)
}

pub fn write_memory(
    backend: &MemoryBackend,
    embedding_model: Option<&dyn EmbeddingBase>,
    input: &AgentMemoryUpsert,
) -> Result<crate::storage::AgentMemoryRecord> {
    match backend {
        MemoryBackend::LocalFile(store) => store.create_or_update(input),
        MemoryBackend::RetrievalStore(store) => upsert_memory_in_store(
            store,
            input,
            embedding_vector(
                embedding_model,
                &format!(
                    "{}\n{}\n{}",
                    input.key,
                    input.value,
                    serde_json::to_string(&input.attributes.person)?
                ),
            )?,
        ),
    }
}

impl Tool for ListMemoryKeysTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(MemoryFunctionToolSpec::new(
            "list_memory_keys",
            "List accessible memory titles by category.",
            json!({"type":"object","properties":{"category":{"type":"string","enum":["concept","person","behavioral"]},"top_n":{"type":"integer"},"query":{"type":"string"}},"required":["category"],"additionalProperties":false}),
        ))
    }
    fn execute(&self, _: &str, arguments: &Value) -> Result<String> {
        let query = optional_string_argument(arguments, "query");
        let hits = search_category(
            &self.resources.memory_backend,
            self.resources.embedding_model.as_deref(),
            &self.resources.access,
            category_argument(arguments)?,
            query.as_deref(),
            memory_limit(arguments.get("top_n").and_then(Value::as_i64)) as usize,
        )?;
        Ok(json!({"ok":true,"items":hits.into_iter().map(|hit| json!({"object_id":hit.record.object_id,"title":hit.record.key,"category":hit.record.attributes.category,"access_level":hit.record.attributes.access_level})).collect::<Vec<_>>()}).to_string())
    }
}

pub struct SearchMemoryTool {
    resources: MemoryAgentResources,
}
impl SearchMemoryTool {
    pub fn new(resources: MemoryAgentResources) -> Self {
        Self { resources }
    }
}
impl Tool for SearchMemoryTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(MemoryFunctionToolSpec::new("search_memory", "Search concept, person or behavioral memory. Use qq_id for an exact person identity lookup; aliases never prove identity.", json!({"type":"object","properties":{"category":{"type":"string","enum":["concept","person","behavioral"]},"query":{"type":"string"},"qq_id":{"type":"string"},"top_n":{"type":"integer"}},"required":["category"],"additionalProperties":false})))
    }
    fn execute(&self, _: &str, arguments: &Value) -> Result<String> {
        let category = category_argument(arguments)?;
        let qq_id = optional_string_argument(arguments, "qq_id");
        if qq_id.is_some() && category != MemoryCategory::Person {
            return Err(Error::ValidationError("qq_id requires person category".to_string()));
        }
        let query = qq_id
            .clone()
            .or_else(|| optional_string_argument(arguments, "query"))
            .ok_or_else(|| Error::ValidationError("query or qq_id required".to_string()))?;
        let mut hits = search_category(
            &self.resources.memory_backend,
            self.resources.embedding_model.as_deref(),
            &self.resources.access,
            category,
            Some(&query),
            memory_limit(arguments.get("top_n").and_then(Value::as_i64)) as usize,
        )?;
        if let Some(qq_id) = qq_id {
            hits.retain(|hit| hit.record.attributes.person.qq_id == qq_id);
        }
        let items = hits
            .into_iter()
            .map(|hit| {
                let fingerprint = memory_fingerprint(&hit.record)?;
                let mut value = serde_json::to_value(hit)?;
                value["fingerprint"] = json!(fingerprint);
                Ok(value)
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(json!({"ok":true,"items":items}).to_string())
    }
}

pub struct RememberMemoryTool {
    resources: MemoryAgentResources,
}
impl RememberMemoryTool {
    pub fn new(resources: MemoryAgentResources) -> Self {
        Self { resources }
    }
}
impl Tool for RememberMemoryTool {
    fn execution_resource(&self, _: &Value) -> crate::agent::tools::ToolExecutionResource {
        crate::agent::tools::ToolExecutionResource::Exclusive
    }
    fn returns_failure_details_to_agent(&self) -> bool {
        true
    }
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(MemoryFunctionToolSpec::new("update_memory", "Create or correct a memory using its existing key. Person requires immutable qq_id and structured profile; never merge different QQ IDs. Behavioral content must be short. Source is bot for independent reflection, or user with the verified triggering sender_id and access_level. User corrections must use user source. Existing scope and access level are preserved.", json!({"type":"object","properties":{
            "category":{"type":"string","enum":["concept","person","behavioral"]},"key":{"type":"string"},"content":{"type":"string"},
            "source":{"type":"object","properties":{"kind":{"type":"string","enum":["bot","user"]},"sender_id":{"type":"string"},"access_level":{"type":"integer","minimum":0}},"required":["kind"],"additionalProperties":false},
            "person":{"type":"object","properties":{"qq_id":{"type":"string"},"name":{"type":"string"},"preferred_name":{"type":"string"},"aliases":{"type":"array","items":{"type":"string"}},"impression":{"type":"string"},"guideline":{"type":"string"}},"additionalProperties":false},
            "starts_at":{"type":"string"},"scope":{"type":"string","enum":["current","global","group","sender"]}
        },"required":["category","content","source"],"additionalProperties":false})))
    }
    fn execute(&self, _: &str, arguments: &Value) -> Result<String> {
        let _write_guard = crate::storage::MEMORY_MUTATION_LOCK
            .lock()
            .map_err(|_| crate::string_error!("memory mutation lock poisoned"))?;
        let category = category_argument(arguments)?;
        let source = arguments
            .get("source")
            .ok_or_else(|| Error::ValidationError("source required".to_string()))?;
        let source_kind = required_string_argument(source, "kind")?;
        let level = match source_kind.as_str() {
            "bot" => 2,
            "user" => {
                let sender = required_string_argument(source, "sender_id")?;
                if self.resources.access.sender_id.as_deref() != Some(&sender) {
                    return Err(Error::ValidationError(
                        "source sender must be the authenticated triggering user".to_string(),
                    ));
                }
                let actual = crate::storage::user_permission::memory_source_level(
                    &self.resources.access,
                    &sender,
                )?;
                if source.get("access_level").and_then(Value::as_u64) != Some(u64::from(actual)) {
                    return Err(Error::ValidationError(format!(
                        "source access_level does not match authenticated level {actual}"
                    )));
                }
                actual
            }
            _ => return Err(Error::ValidationError("source kind must be bot or user".to_string())),
        };
        let content = required_string_argument(arguments, "content")?;
        let person_patch = arguments.get("person");
        let mut person: PersonMemory =
            serde_json::from_value(person_patch.cloned().unwrap_or_else(|| json!({})))?;
        let key = if category == MemoryCategory::Person {
            if person.qq_id.is_empty() || !person.qq_id.chars().all(|c| c.is_ascii_digit()) {
                return Err(Error::ValidationError("person.qq_id must be a QQ ID".to_string()));
            }
            format!("person-{}", person.qq_id)
        } else if category == MemoryCategory::Behavioral {
            optional_string_argument(arguments, "key").unwrap_or_default()
        } else {
            required_string_argument(arguments, "key")?
        };
        let scope = arguments.get("scope").and_then(Value::as_str).unwrap_or("current");
        let sender_id_list: Vec<String> =
            if category != MemoryCategory::Person && matches!(scope, "current" | "sender") {
                self.resources.access.sender_id.clone().into_iter().collect()
            } else {
                vec![]
            };
        let group_id_list: Vec<String> =
            if category != MemoryCategory::Person && matches!(scope, "current" | "group") {
                self.resources.access.group_id.clone().into_iter().collect()
            } else {
                vec![]
            };
        if !matches!(scope, "current" | "sender" | "group" | "global") {
            return Err(Error::ValidationError("invalid scope".to_string()));
        }
        let identity = AgentMemoryUpsert {
            attributes: MemoryAttributes { category, ..Default::default() },
            key: key.clone(),
            value: content.clone(),
            sender_id_list: sender_id_list.clone(),
            group_id_list: group_id_list.clone(),
            ..Default::default()
        };
        let mut identity = identity;
        if category == MemoryCategory::Behavioral && identity.key.is_empty() {
            let mut content_identity = identity.clone();
            content_identity.key = content.clone();
            identity.key =
                format!("behavior-{}", crate::storage::memory_object_id(&content_identity));
        }
        let key = identity.key.clone();
        let existing = match &self.resources.memory_backend {
            MemoryBackend::LocalFile(store) => store.get(&identity)?,
            MemoryBackend::RetrievalStore(store) => {
                crate::storage::get_memory_in_store(store, &identity)?
            }
        };
        if let Some(existing) = &existing {
            if source_kind == "user" && level > existing.attributes.access_level {
                return Err(Error::ValidationError(
                    "user access level cannot modify this memory".to_string(),
                ));
            }
            if category == MemoryCategory::Person {
                let mut merged = serde_json::to_value(&existing.attributes.person)?;
                if let Some(patch) = person_patch.and_then(Value::as_object) {
                    merged.as_object_mut().unwrap().extend(patch.clone());
                }
                person = serde_json::from_value(merged)?;
            }
        }
        if category == MemoryCategory::Person && person.impression.is_empty() {
            person.impression = content.clone();
        }
        let starts_at = optional_string_argument(arguments, "starts_at")
            .unwrap_or_else(|| Utc::now().to_rfc3339());
        chrono::DateTime::parse_from_rfc3339(&starts_at)
            .map_err(|_| Error::ValidationError("invalid starts_at".to_string()))?;
        let value = if category == MemoryCategory::Person {
            serde_json::to_string(&person)?
        } else {
            content
        };
        let input = AgentMemoryUpsert {
            attributes: MemoryAttributes {
                category,
                access_level: existing
                    .as_ref()
                    .map(|record| record.attributes.access_level)
                    .unwrap_or(level),
                starts_at: (category == MemoryCategory::Concept).then_some(starts_at),
                person,
            },
            key,
            value,
            expires_at: (category == MemoryCategory::Behavioral)
                .then(|| (Utc::now() + Duration::days(2)).to_rfc3339()),
            sender_id_list,
            group_id_list,
        };
        let record = write_memory(
            &self.resources.memory_backend,
            self.resources.embedding_model.as_deref(),
            &input,
        )?;
        Ok(json!({"ok":true,"item":record}).to_string())
    }
}

/// FullSearchConceptMemory and the memory explorer share the same parallel recall.
/// Behavioral rules are loaded by scope; concept and person memory use the query.
pub fn full_search_memory(
    backend: &MemoryBackend,
    embedding_model: Option<&dyn EmbeddingBase>,
    access: &AgentMemoryAccessContext,
    query: Option<&str>,
    limit: usize,
) -> Result<Vec<(MemoryCategory, Vec<crate::storage::AgentMemorySearchHit>)>> {
    std::thread::scope(|scope| {
        let tasks = MemoryCategory::ALL
            .into_iter()
            .map(|category| {
                scope.spawn(move || {
                    search_category(
                        backend,
                        embedding_model,
                        access,
                        category,
                        if category == MemoryCategory::Behavioral {
                            None
                        } else {
                            query
                        },
                        if category == MemoryCategory::Behavioral {
                            100
                        } else {
                            limit
                        },
                    )
                    .map(|hits| (category, hits))
                })
            })
            .collect::<Vec<_>>();
        tasks
            .into_iter()
            .map(|task| {
                task.join().map_err(|_| crate::string_error!("memory search worker failed"))?
            })
            .collect::<Result<Vec<_>>>()
    })
}

/// Shared only with the recall phase. Results are raw records, not an LLM summary.
pub struct FullSearchConceptMemory {
    resources: MemoryAgentResources,
    seen: Mutex<HashSet<String>>,
    previous_context: String,
    results: Arc<Mutex<Vec<Value>>>,
}
impl FullSearchConceptMemory {
    pub fn new(
        resources: MemoryAgentResources,
        previous_context: String,
        results: Arc<Mutex<Vec<Value>>>,
    ) -> Self {
        Self {
            resources,
            seen: Mutex::new(HashSet::new()),
            previous_context,
            results,
        }
    }
}
impl Tool for FullSearchConceptMemory {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(MemoryFunctionToolSpec::new("FullSearchConceptMemory", "Search all memory categories in parallel. Use force_check when the user disputes recalled information.", json!({"type":"object","properties":{"query":{"type":"string"},"force_check":{"type":"boolean"}},"required":["query"],"additionalProperties":false})))
    }
    fn execute(&self, _: &str, arguments: &Value) -> Result<String> {
        let query = required_string_argument(arguments, "query")?;
        let force = arguments.get("force_check").and_then(Value::as_bool).unwrap_or(false);
        let batches = full_search_memory(
            &self.resources.memory_backend,
            self.resources.embedding_model.as_deref(),
            &self.resources.access,
            Some(&query),
            10,
        )?;
        let mut grouped = serde_json::Map::new();
        let mut seen = self.seen.lock().unwrap();
        for (category, hits) in batches {
            let mut items = Vec::new();
            for hit in hits {
                let fingerprint = memory_fingerprint(&hit.record)?;
                if force
                    || (!seen.contains(&fingerprint)
                        && !self.previous_context.contains(&serde_json::to_string(&fingerprint)?))
                {
                    seen.insert(fingerprint.clone());
                    items.push(json!({"fingerprint":fingerprint,"memory":hit.record}));
                }
            }
            grouped.insert(
                serde_json::to_value(category)?.as_str().unwrap().to_string(),
                json!(items),
            );
        }
        let result = Value::Object(grouped);
        self.results.lock().unwrap().push(result.clone());
        Ok(result.to_string())
    }
}

pub fn recall_memory(
    resources: MemoryAgentResources,
    input: &str,
    previous_context: String,
) -> Result<String> {
    let results = Arc::new(Mutex::new(Vec::new()));
    let mut host = AgentHost::new();
    host.register_llm(crate::agent::LLM_KIND_MAIN, resources.llm.clone());
    host.register_tool(
        "FullSearchConceptMemory",
        Arc::new(FullSearchConceptMemory::new(
            resources.clone(),
            previous_context,
            results.clone(),
        )),
    );
    let definition = memory_agent_definition(&current_memory_agent_prompts());
    let agent = host.build_definition(definition)?;
    let mut ports = HashMap::new();
    ports.insert("content".to_string(), crate::graph::DataValue::String(input.to_string()));
    // Recall the verified sender and applicable instructions even when the phase model skips tools.
    let full = host.tool("FullSearchConceptMemory").unwrap();
    if let Some(sender) = &resources.access.sender_id {
        full.execute("", &json!({"query":sender}))?;
    }
    let _ = agent.run_text(ports)?;
    let recalled = serde_json::to_string(&*results.lock().unwrap())?;
    Ok(recalled)
}

fn memory_limit(value: Option<i64>) -> i64 {
    value.unwrap_or(DEFAULT_MEMORY_TOP_N).clamp(1, MAX_MEMORY_TOP_N)
}
fn embedding_vector(embedding_model: Option<&dyn EmbeddingBase>, text: &str) -> Result<Vec<f32>> {
    embedding_model
        .ok_or_else(|| {
            Error::ValidationError("memory backend requires an embedding model".to_string())
        })?
        .inference(text)
}
fn optional_string_argument(arguments: &Value, name: &str) -> Option<String> {
    arguments
        .get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}
fn required_string_argument(arguments: &Value, name: &str) -> Result<String> {
    optional_string_argument(arguments, name)
        .ok_or_else(|| Error::ValidationError(format!("{name} is required")))
}

#[derive(Debug)]
struct MemoryFunctionToolSpec {
    name: &'static str,
    description: &'static str,
    parameters: Value,
}
impl MemoryFunctionToolSpec {
    fn new(name: &'static str, description: &'static str, parameters: Value) -> Self {
        Self { name, description, parameters }
    }
}
impl FunctionTool for MemoryFunctionToolSpec {
    fn name(&self) -> &str {
        self.name
    }
    fn description(&self) -> &str {
        self.description
    }
    fn parameters(&self) -> Value {
        self.parameters.clone()
    }
    fn call(&self, arguments: Value) -> Result<Value> {
        Ok(arguments)
    }
}
