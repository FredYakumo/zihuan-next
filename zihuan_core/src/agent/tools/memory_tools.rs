use std::sync::Arc;

use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::agent::declarative_agent::{
    AgentDefinition, AgentHost, AgentOutputMode, AgentPromptPart, DeclarativeAgentTool,
};
use crate::agent::tools::Tool;
use crate::error::{Error, Result};
use crate::graph::function_graph::FunctionPortDef;
use crate::graph::DataType;
use crate::model_inference::llm::embedding_base::EmbeddingBase;
use crate::model_inference::llm::llm_base::LLMBase;
use crate::model_inference::llm::tooling::FunctionTool;
use crate::model_inference::llm::{InferenceParam, LLMMessage};
use crate::retrieval::RetrievalStoreRef;
use crate::storage::{
    list_memory_in_store, search_memory_in_store, upsert_memory_in_store, AgentMemoryAccessContext,
    AgentMemoryUpsert, LocalMemoryStore,
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

/// Ids the memory agent definition references in its `tool_ids`, in registration order.
pub const MEMORY_TOOL_IDS: [&str; 3] = ["list_memory_keys", "search_memory", "update_memory"];

pub const MEMORY_AGENT_ID: &str = "memory_agent";
const MEMORY_AGENT_NAME: &str = "Memory";
const MEMORY_AGENT_DESCRIPTION: &str = "Call the Memory Agent. Given content, it independently decides whether to retrieve relevant memories, update memories worth saving, or report that no relevant memories exist. Pass operation to force a specific memory operation.";

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
/// former built-in `memory_agent.yaml`: same id, ports, prompt wiring, and tool ids.
pub fn memory_agent_definition(prompts: &crate::system_config::MemoryAgentPrompts) -> AgentDefinition {
    AgentDefinition {
        id: MEMORY_AGENT_ID.to_string(),
        name: MEMORY_AGENT_NAME.to_string(),
        description: MEMORY_AGENT_DESCRIPTION.to_string(),
        builtin: true,
        inputs: vec![
            memory_string_port("content", "Content for the memory agent to process", true),
            memory_string_port(
                "operation",
                "Optional: force a memory operation, either search_memory or update_memory. Omit to let the agent decide by itself.",
                false,
            ),
        ],
        outputs: vec![memory_string_port("result", "Memory result", true)],
        system_prompt: prompts.system_prompt.clone(),
        user_prompt: Some("{content}".to_string()),
        prompt_parts: vec![
            AgentPromptPart {
                port: "operation".to_string(),
                equals: Some("search_memory".to_string()),
                template: prompts.search_operation_prompt.clone(),
            },
            AgentPromptPart {
                port: "operation".to_string(),
                equals: Some("update_memory".to_string()),
                template: prompts.update_operation_prompt.clone(),
            },
        ],
        output_mode: AgentOutputMode::Text,
        llm_kind: crate::agent::LLM_KIND_MAIN.to_string(),
        progress_message: None,
        include_graph_tools: false,
        run_duration: crate::tool_runtime::ToolRunDuration::default(),
        tool_ids: MEMORY_TOOL_IDS.iter().map(|id| (*id).to_string()).collect(),
    }
}

/// Composes the memory agent from the configured prompts and registers it as a callable
/// tool under [`MEMORY_AGENT_ID`]. The memory tools and the `main` LLM must already be
/// registered on the host.
pub fn register_memory_agent_tool(host: &mut AgentHost) -> Result<Arc<dyn Tool>> {
    let definition = memory_agent_definition(&current_memory_agent_prompts());
    let agent = Arc::new(host.build_definition(definition)?);
    let tool: Arc<dyn Tool> = Arc::new(DeclarativeAgentTool::new(agent));
    host.register_tool(MEMORY_AGENT_ID, Arc::clone(&tool));
    Ok(tool)
}
impl Tool for ListMemoryKeysTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(MemoryFunctionToolSpec::new("list_memory_keys", "List titles of memories accessible in the current context; optionally filter by query.", json!({"type":"object","properties":{"top_n":{"type":"integer"},"query":{"type":"string"}},"additionalProperties":false})))
    }
    fn execute(&self, _call_content: &str, arguments: &Value) -> String {
        let result = (|| -> Result<Value> {
            let top_n = memory_limit(arguments.get("top_n").and_then(Value::as_i64));
            let query = optional_string_argument(arguments, "query");
            let hits = if let Some(query) = query.as_deref() {
                let mut hits = match &self.resources.memory_backend {
                    MemoryBackend::LocalFile(store) => store.list(Some(query), top_n as usize)?,
                    MemoryBackend::RetrievalStore(store) => search_memory_in_store(
                        store,
                        &self.resources.access,
                        query,
                        &embedding_vector(&self.resources, query)?,
                        top_n as usize,
                    )?,
                };
                hits.sort_by(|left, right| right.record.updated_at.cmp(&left.record.updated_at));
                hits
            } else {
                match &self.resources.memory_backend {
                    MemoryBackend::LocalFile(store) => store.list(None, top_n as usize)?,
                    MemoryBackend::RetrievalStore(store) => {
                        list_memory_in_store(store, &self.resources.access, top_n as usize)?
                    }
                }
            };
            Ok(
                json!({"ok":true,"items":hits.into_iter().map(|hit| json!({"object_id":hit.record.object_id,"title":hit.record.key,"updated_at":hit.record.updated_at,"expires_at":hit.record.expires_at})).collect::<Vec<_>>() }),
            )
        })();
        render_value_result(result)
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
        Arc::new(MemoryFunctionToolSpec::new("search_memory", "Search memories accessible in the current context and return their titles and content.", json!({"type":"object","properties":{"query":{"type":"string"},"top_n":{"type":"integer"}},"required":["query"],"additionalProperties":false})))
    }
    fn execute(&self, _call_content: &str, arguments: &Value) -> String {
        let result = (|| -> Result<Value> {
            let query = required_string_argument(arguments, "query")?;
            let top_n = memory_limit(arguments.get("top_n").and_then(Value::as_i64));
            let hits = match &self.resources.memory_backend {
                MemoryBackend::LocalFile(store) => store.list(Some(&query), top_n as usize)?,
                MemoryBackend::RetrievalStore(store) => search_memory_in_store(
                    store,
                    &self.resources.access,
                    &query,
                    &embedding_vector(&self.resources, &query)?,
                    top_n as usize,
                )?,
            };
            Ok(
                json!({"ok":true,"items":hits.into_iter().map(|hit| json!({"object_id":hit.record.object_id,"title":hit.record.key,"value":hit.record.value,"updated_at":hit.record.updated_at,"expires_at":hit.record.expires_at,"sender_id_list":hit.record.sender_id_list,"group_id_list":hit.record.group_id_list})).collect::<Vec<_>>() }),
            )
        })();
        render_value_result(result)
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
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(MemoryFunctionToolSpec::new(
            "update_memory",
            "Organize and write information that should be remembered long term.",
            json!({"type":"object","properties":{"content":{"type":"string"}},"required":["content"],"additionalProperties":false}),
        ))
    }
    fn execute(&self, _call_content: &str, arguments: &Value) -> String {
        let result = (|| -> Result<Value> {
            let content = required_string_argument(arguments, "content")?;
            let items = split_memory_items(&self.resources, &content)?;
            let expires_at = (Utc::now() + Duration::days(2)).to_rfc3339();
            let sender_id_list: Vec<String> =
                self.resources.access.sender_id.clone().into_iter().collect();
            let group_id_list: Vec<String> =
                self.resources.access.group_id.clone().into_iter().collect();
            let stored = items
                .into_iter()
                .map(|item| {
                    let input = AgentMemoryUpsert {
                        key: item.title,
                        value: item.value,
                        expires_at: Some(expires_at.clone()),
                        sender_id_list: sender_id_list.clone(),
                        group_id_list: group_id_list.clone(),
                    };
                    match &self.resources.memory_backend {
                        MemoryBackend::LocalFile(store) => store.create_or_update(&input),
                        MemoryBackend::RetrievalStore(store) => upsert_memory_in_store(
                            store,
                            &input,
                            embedding_vector(
                                &self.resources,
                                &format!("{}\n{}", input.key, input.value),
                            )?,
                        ),
                    }
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(
                json!({"ok":true,"items":stored.into_iter().map(|item| json!({"object_id":item.object_id,"title":item.key,"value":item.value,"expires_at":item.expires_at})).collect::<Vec<_>>() }),
            )
        })();
        render_value_result(result)
    }
}

#[derive(Debug, Clone, Deserialize)]
struct MemoryDraftItem {
    #[serde(alias = "key")]
    title: String,
    value: String,
}
fn split_memory_items(
    resources: &MemoryAgentResources,
    content: &str,
) -> Result<Vec<MemoryDraftItem>> {
    let prompt = vec![LLMMessage::system("You are a memory organizer. Split the user-provided content into memories suitable for long-term retrieval. Return only a JSON array with no Markdown or explanation. Each item must use this format: {\"title\":\"memory title\",\"value\":\"memory content\"}. If the content suits only one memory, return a single-item array. Titles must be concise, clear, and useful for future retrieval. Do not disclose or reference information outside the current conversation."), LLMMessage::user(format!("Organize the following content into memory JSON:\n{content}"))];
    if let Some(text) = resources
        .llm
        .inference(&InferenceParam { messages: &prompt, tools: None })
        .ok()
        .and_then(|response| response.content_text_owned())
    {
        if let Some(items) = parse_memory_json(&text) {
            let items = normalize_draft_items(items);
            if !items.is_empty() {
                return Ok(items);
            }
        }
    }
    Ok(vec![MemoryDraftItem {
        title: summarize_memory_key(content),
        value: content.trim().to_string(),
    }])
}
fn parse_memory_json(text: &str) -> Option<Vec<MemoryDraftItem>> {
    let trimmed = text.trim();
    serde_json::from_str(trimmed).ok().or_else(|| {
        trimmed
            .strip_prefix("```json")
            .or_else(|| trimmed.strip_prefix("```"))
            .and_then(|value| value.strip_suffix("```"))
            .map(str::trim)
            .and_then(|value| serde_json::from_str(value).ok())
    })
}
fn normalize_draft_items(items: Vec<MemoryDraftItem>) -> Vec<MemoryDraftItem> {
    items
        .into_iter()
        .filter_map(|item| {
            let title = item.title.trim();
            let value = item.value.trim();
            (!title.is_empty() && !value.is_empty()).then(|| MemoryDraftItem {
                title: title.to_string(),
                value: value.to_string(),
            })
        })
        .collect()
}
fn summarize_memory_key(content: &str) -> String {
    let normalized = content.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = normalized.chars();
    let summary = chars.by_ref().take(32).collect::<String>();
    if summary.is_empty() {
        "memory".to_string()
    } else {
        summary
    }
}
fn memory_limit(value: Option<i64>) -> i64 {
    value.unwrap_or(DEFAULT_MEMORY_TOP_N).clamp(1, MAX_MEMORY_TOP_N)
}
fn embedding_vector(resources: &MemoryAgentResources, text: &str) -> Result<Vec<f32>> {
    resources
        .embedding_model
        .as_ref()
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
fn render_value_result(result: Result<Value>) -> String {
    result
        .map(|value| value.to_string())
        .unwrap_or_else(|error| json!({"ok":false,"error":error.to_string()}).to_string())
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
