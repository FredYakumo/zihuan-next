use std::sync::Arc;

use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::agent::tools::{ToolCallingEngine, ToolCallingStopReason, Tool};
use crate::error::{Error, Result};
use crate::llm::embedding_base::EmbeddingBase;
use crate::llm::llm_base::LLMBase;
use crate::llm::tooling::FunctionTool;
use crate::llm::{InferenceParam, LLMMessage, MessageRole};
use crate::storage::{
    create_elasticsearch_memory_record, create_memory_record_with_vector, list_elasticsearch_memory_keys,
    list_recent_memory_keys, search_elasticsearch_memory, search_memory_content_by_vector, AgentMemoryAccessContext,
    AgentMemoryUpsert, ElasticsearchRef, LocalMemoryStore,
};
use crate::weaviate::WeaviateRef;

const DEFAULT_MEMORY_TOP_N: i64 = 5;
const MAX_MEMORY_TOP_N: i64 = 20;



// prompt engineering

const MEMORY_AGENT_SYSTEM_PROMPT: &str = "You are a memory management agent with private tools for searching, listing, and writing memories. Based on the request, decide whether to retrieve relevant memories, update facts worth retaining long term, or state that no relevant memories exist. Do not fabricate memories. Return only a concise result useful to the caller.";
const SEARCH_OPERATION_PROMPT: &str = "\n\n[Memory Operation]\nSearch memories: you must use the memory search tool to find saved memories relevant to the chat context above. Return only relevant memories and explicitly state when none are found. Do not write any memories.";
const UPDATE_OPERATION_PROMPT: &str = "\n\n[Memory Operation]\nUpdate memories: you must attempt to extract facts, preferences, or relationships from the chat context above that are worth retaining long term, and save them with the memory writing tool. You may search first to verify them. If there is nothing appropriate to save, explicitly state that no memories were updated.";


// 


#[derive(Clone)]
pub struct MemoryAgentResources {
    pub memory_backend: MemoryBackend,
    pub embedding_model: Option<Arc<dyn EmbeddingBase>>,
    pub llm: Arc<dyn LLMBase>,
    pub access: AgentMemoryAccessContext,
}

#[derive(Clone)]
pub enum MemoryBackend {
    LocalFile(Arc<LocalMemoryStore>),
    Weaviate(Arc<WeaviateRef>),
    Elasticsearch(Arc<ElasticsearchRef>),
}

#[derive(Clone)]
pub struct MemoryBrainAgent {
    resources: MemoryAgentResources,
}

impl MemoryBrainAgent {
    pub fn new(resources: MemoryAgentResources) -> Self {
        Self { resources }
    }

    pub fn tool(&self) -> MemoryBrainAgentTool {
        MemoryBrainAgentTool::new(self.clone())
    }

    pub fn context_tool(&self) -> MemoryBrainAgentContextTool {
        MemoryBrainAgentContextTool::new(self.clone())
    }

    fn run(&self, user_message: String) -> Result<String> {
        let agent = self.clone();
        std::thread::Builder::new()
            .name("memory-brain-agent".to_string())
            .spawn(move || agent.run_inner(user_message))
            .map_err(|error| crate::string_error!("failed to start Memory ToolCallingEngine Agent thread: {error}"))?
            .join()
            .map_err(|_| crate::string_error!("Memory ToolCallingEngine Agent thread panicked"))?
    }

    fn run_inner(&self, user_message: String) -> Result<String> {
        let mut brain = ToolCallingEngine::new(Arc::clone(&self.resources.llm));
        brain.add_tool(ListMemoryKeysTool::new(self.resources.clone()));
        brain.add_tool(SearchMemoryTool::new(self.resources.clone()));
        brain.add_tool(RememberMemoryTool::new(self.resources.clone()));
        let (output, stop_reason) = brain.run(vec![
            LLMMessage::system(MEMORY_AGENT_SYSTEM_PROMPT),
            LLMMessage::user(user_message),
        ]);
        if !matches!(stop_reason, ToolCallingStopReason::Done) {
            return Err(crate::string_error!("Memory ToolCallingEngine Agent did not complete normally: {stop_reason:?}"));
        }
        output
            .iter()
            .rev()
            .find(|message| matches!(message.role, MessageRole::Assistant))
            .and_then(LLMMessage::content_text_owned)
            .filter(|content| !content.trim().is_empty())
            .ok_or_else(|| crate::string_error!("Memory ToolCallingEngine Agent returned no text"))
    }

    fn run_content(&self, content: String) -> Result<String> {
        self.run(content)
    }

    fn run_context(&self, chat_context: String, operation: MemoryAgentOperation) -> Result<String> {
        let operation_prompt = match operation {
            MemoryAgentOperation::SearchMemory => SEARCH_OPERATION_PROMPT,
            MemoryAgentOperation::UpdateMemory => UPDATE_OPERATION_PROMPT,
        };
        self.run(format!("{chat_context}{operation_prompt}"))
    }
}

#[derive(Debug, Clone, Copy)]
pub enum MemoryAgentOperation {
    SearchMemory,
    UpdateMemory,
}

impl MemoryAgentOperation {
    fn parse(value: &str) -> Result<Self> {
        match value.trim() {
            "search_memory" => Ok(Self::SearchMemory),
            "update_memory" => Ok(Self::UpdateMemory),
            _ => Err(Error::ValidationError("operation must be search_memory or update_memory".to_string())),
        }
    }
}

pub struct MemoryBrainAgentTool {
    agent: MemoryBrainAgent,
}

impl MemoryBrainAgentTool {
    pub fn new(agent: MemoryBrainAgent) -> Self {
        Self { agent }
    }
}

impl Tool for MemoryBrainAgentTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(MemoryFunctionToolSpec::new(
            "memory_agent",
            "Call the Memory ToolCallingEngine Agent. Given content, it independently decides whether to retrieve relevant memories, update memories worth saving, or report that no relevant memories exist.",
            json!({"type":"object","properties":{"content":{"type":"string","description":"Content for the memory agent to process"}},"required":["content"],"additionalProperties":false}),
        ))
    }

    fn execute(&self, _call_content: &str, arguments: &Value) -> String {
        let result = required_string_argument(arguments, "content").and_then(|content| self.agent.run_content(content));
        render_result(result)
    }
}

pub struct MemoryBrainAgentContextTool {
    agent: MemoryBrainAgent,
}

impl MemoryBrainAgentContextTool {
    pub fn new(agent: MemoryBrainAgent) -> Self {
        Self { agent }
    }
}

impl Tool for MemoryBrainAgentContextTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(MemoryFunctionToolSpec::new(
            "memory_agent_with_context",
            "Call the Memory ToolCallingEngine Agent with chat context. Search returns relevant memories; update extracts and saves memories worth retaining long term.",
            json!({"type":"object","properties":{"chat_context":{"type":"string","description":"Complete chat context"},"operation":{"type":"string","enum":["search_memory","update_memory"],"description":"Memory operation"}},"required":["chat_context","operation"],"additionalProperties":false}),
        ))
    }

    fn execute(&self, _call_content: &str, arguments: &Value) -> String {
        let result = (|| -> Result<String> {
            let context = required_string_argument(arguments, "chat_context")?;
            let operation = required_string_argument(arguments, "operation")?;
            self.agent.run_context(context, MemoryAgentOperation::parse(&operation)?)
        })();
        render_result(result)
    }
}

struct ListMemoryKeysTool { resources: MemoryAgentResources }
impl ListMemoryKeysTool { fn new(resources: MemoryAgentResources) -> Self { Self { resources } } }
impl Tool for ListMemoryKeysTool {
    fn spec(&self) -> Arc<dyn FunctionTool> { Arc::new(MemoryFunctionToolSpec::new("list_memory_keys", "List titles of memories accessible in the current context; optionally filter by query.", json!({"type":"object","properties":{"top_n":{"type":"integer"},"query":{"type":"string"}},"additionalProperties":false}))) }
    fn execute(&self, _call_content: &str, arguments: &Value) -> String {
        let result = (|| -> Result<Value> {
            let top_n = memory_limit(arguments.get("top_n").and_then(Value::as_i64));
            let query = optional_string_argument(arguments, "query");
            let hits = if let Some(query) = query.as_deref() {
                let mut hits = match &self.resources.memory_backend {
                    MemoryBackend::LocalFile(store) => store.list(Some(query), top_n as usize)?,
                    MemoryBackend::Weaviate(reference) => search_memory_content_by_vector(reference, &self.resources.access, &embedding_vector(&self.resources, query)?, top_n as usize)?,
                    MemoryBackend::Elasticsearch(reference) => search_elasticsearch_memory(reference, &self.resources.access, query, &embedding_vector(&self.resources, query)?, top_n as usize)?,
                };
                hits.sort_by(|left, right| right.record.updated_at.cmp(&left.record.updated_at)); hits
            } else { match &self.resources.memory_backend {
                MemoryBackend::LocalFile(store) => store.list(None, top_n as usize)?,
                MemoryBackend::Weaviate(reference) => list_recent_memory_keys(reference, &self.resources.access, top_n as usize, None)?,
                MemoryBackend::Elasticsearch(reference) => list_elasticsearch_memory_keys(reference, &self.resources.access, top_n as usize, None)?,
            }};
            Ok(json!({"ok":true,"items":hits.into_iter().map(|hit| json!({"object_id":hit.record.object_id,"title":hit.record.key,"updated_at":hit.record.updated_at,"expires_at":hit.record.expires_at})).collect::<Vec<_>>() }))
        })(); render_value_result(result)
    }
}

struct SearchMemoryTool { resources: MemoryAgentResources }
impl SearchMemoryTool { fn new(resources: MemoryAgentResources) -> Self { Self { resources } } }
impl Tool for SearchMemoryTool {
    fn spec(&self) -> Arc<dyn FunctionTool> { Arc::new(MemoryFunctionToolSpec::new("search_memory", "Search memories accessible in the current context and return their titles and content.", json!({"type":"object","properties":{"query":{"type":"string"},"top_n":{"type":"integer"}},"required":["query"],"additionalProperties":false}))) }
    fn execute(&self, _call_content: &str, arguments: &Value) -> String {
        let result = (|| -> Result<Value> {
            let query = required_string_argument(arguments, "query")?; let top_n = memory_limit(arguments.get("top_n").and_then(Value::as_i64));
            let hits = match &self.resources.memory_backend {
                MemoryBackend::LocalFile(store) => store.list(Some(&query), top_n as usize)?,
                MemoryBackend::Weaviate(reference) => search_memory_content_by_vector(reference, &self.resources.access, &embedding_vector(&self.resources, &query)?, top_n as usize)?,
                MemoryBackend::Elasticsearch(reference) => search_elasticsearch_memory(reference, &self.resources.access, &query, &embedding_vector(&self.resources, &query)?, top_n as usize)?,
            };
            Ok(json!({"ok":true,"items":hits.into_iter().map(|hit| json!({"object_id":hit.record.object_id,"title":hit.record.key,"value":hit.record.value,"updated_at":hit.record.updated_at,"expires_at":hit.record.expires_at,"sender_id_list":hit.record.sender_id_list,"group_id_list":hit.record.group_id_list})).collect::<Vec<_>>() }))
        })(); render_value_result(result)
    }
}

struct RememberMemoryTool { resources: MemoryAgentResources }
impl RememberMemoryTool { fn new(resources: MemoryAgentResources) -> Self { Self { resources } } }
impl Tool for RememberMemoryTool {
    fn spec(&self) -> Arc<dyn FunctionTool> { Arc::new(MemoryFunctionToolSpec::new("update_memory", "Organize and write information that should be remembered long term.", json!({"type":"object","properties":{"content":{"type":"string"}},"required":["content"],"additionalProperties":false}))) }
    fn execute(&self, _call_content: &str, arguments: &Value) -> String {
        let result = (|| -> Result<Value> {
            let content = required_string_argument(arguments, "content")?; let items = split_memory_items(&self.resources, &content)?; let expires_at = (Utc::now() + Duration::days(2)).to_rfc3339();
            let sender_id_list: Vec<String> = self.resources.access.sender_id.clone().into_iter().collect();
            let group_id_list: Vec<String> = self.resources.access.group_id.clone().into_iter().collect();
            let stored = items.into_iter().map(|item| { let input = AgentMemoryUpsert { key:item.title, value:item.value, expires_at:Some(expires_at.clone()), sender_id_list:sender_id_list.clone(), group_id_list:group_id_list.clone() }; match &self.resources.memory_backend { MemoryBackend::LocalFile(store) => store.create_or_update(&input), MemoryBackend::Weaviate(reference) => create_memory_record_with_vector(reference, &input, Some(embedding_vector(&self.resources, &format!("{}\n{}", input.key, input.value))?)), MemoryBackend::Elasticsearch(reference) => create_elasticsearch_memory_record(reference, &input, embedding_vector(&self.resources, &format!("{}\n{}", input.key, input.value))?) } }).collect::<Result<Vec<_>>>()?;
            Ok(json!({"ok":true,"items":stored.into_iter().map(|item| json!({"object_id":item.object_id,"title":item.key,"value":item.value,"expires_at":item.expires_at})).collect::<Vec<_>>() }))
        })(); render_value_result(result)
    }
}

#[derive(Debug, Clone, Deserialize)] struct MemoryDraftItem { #[serde(alias = "key")] title: String, value: String }
fn split_memory_items(resources: &MemoryAgentResources, content: &str) -> Result<Vec<MemoryDraftItem>> {
    let prompt = vec![LLMMessage::system("You are a memory organizer. Split the user-provided content into memories suitable for long-term retrieval. Return only a JSON array with no Markdown or explanation. Each item must use this format: {\"title\":\"memory title\",\"value\":\"memory content\"}. If the content suits only one memory, return a single-item array. Titles must be concise, clear, and useful for future retrieval. Do not disclose or reference information outside the current conversation."), LLMMessage::user(format!("Organize the following content into memory JSON:\n{content}"))];
    if let Some(text) = resources.llm.inference(&InferenceParam { messages: &prompt, tools: None }).content_text_owned() { if let Some(items) = parse_memory_json(&text) { let items = normalize_draft_items(items); if !items.is_empty() { return Ok(items); } } }
    Ok(vec![MemoryDraftItem { title: summarize_memory_key(content), value: content.trim().to_string() }])
}
fn parse_memory_json(text: &str) -> Option<Vec<MemoryDraftItem>> { let trimmed = text.trim(); serde_json::from_str(trimmed).ok().or_else(|| trimmed.strip_prefix("```json").or_else(|| trimmed.strip_prefix("```")).and_then(|value| value.strip_suffix("```")).map(str::trim).and_then(|value| serde_json::from_str(value).ok())) }
fn normalize_draft_items(items: Vec<MemoryDraftItem>) -> Vec<MemoryDraftItem> { items.into_iter().filter_map(|item| { let title=item.title.trim(); let value=item.value.trim(); (!title.is_empty() && !value.is_empty()).then(|| MemoryDraftItem { title:title.to_string(), value:value.to_string() }) }).collect() }
fn summarize_memory_key(content: &str) -> String { let normalized=content.split_whitespace().collect::<Vec<_>>().join(" "); let mut chars=normalized.chars(); let summary=chars.by_ref().take(32).collect::<String>(); if summary.is_empty() { "memory".to_string() } else { summary } }
fn memory_limit(value: Option<i64>) -> i64 { value.unwrap_or(DEFAULT_MEMORY_TOP_N).clamp(1, MAX_MEMORY_TOP_N) }
fn embedding_vector(resources: &MemoryAgentResources, text: &str) -> Result<Vec<f32>> { resources.embedding_model.as_ref().ok_or_else(|| Error::ValidationError("memory backend requires an embedding model".to_string()))?.inference(text) }
fn optional_string_argument(arguments: &Value, name: &str) -> Option<String> { arguments.get(name).and_then(Value::as_str).map(str::trim).filter(|value| !value.is_empty()).map(ToOwned::to_owned) }
fn required_string_argument(arguments: &Value, name: &str) -> Result<String> { optional_string_argument(arguments, name).ok_or_else(|| Error::ValidationError(format!("{name} is required"))) }
fn render_result(result: Result<String>) -> String { result.unwrap_or_else(|error| json!({"ok":false,"error":error.to_string()}).to_string()) }
fn render_value_result(result: Result<Value>) -> String { result.map(|value| value.to_string()).unwrap_or_else(|error| json!({"ok":false,"error":error.to_string()}).to_string()) }

#[derive(Debug)]
struct MemoryFunctionToolSpec { name: &'static str, description: &'static str, parameters: Value }
impl MemoryFunctionToolSpec { fn new(name: &'static str, description: &'static str, parameters: Value) -> Self { Self { name, description, parameters } } }
impl FunctionTool for MemoryFunctionToolSpec { fn name(&self) -> &str { self.name } fn description(&self) -> &str { self.description } fn parameters(&self) -> Value { self.parameters.clone() } fn call(&self, arguments: Value) -> Result<Value> { Ok(arguments) } }
