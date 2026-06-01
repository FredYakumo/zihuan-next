use std::sync::Arc;

use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::{json, Value};

use storage_handler::{
    create_memory_record_with_vector, list_recent_memory_keys, search_memory_content_by_vector,
    AgentMemoryAccessContext, AgentMemoryUpsert,
};
use zihuan_agent::brain::BrainTool;
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::llm_base::LLMBase;
use zihuan_core::llm::tooling::FunctionTool;
use zihuan_core::llm::{InferenceParam, OpenAIMessage};
use zihuan_core::weaviate::WeaviateRef;
use zihuan_core::llm::embedding_base::EmbeddingBase;

use super::common::{
    optional_string_argument, optional_string_list_argument, sanitize_positive_limit,
    StaticFunctionToolSpec,
};

const DEFAULT_MEMORY_TOP_N: i64 = 5;
const MAX_MEMORY_TOP_N: i64 = 20;

#[derive(Clone)]
pub(crate) struct AgentMemoryToolResources {
    pub memory_ref: Arc<WeaviateRef>,
    pub embedding_model: Arc<dyn EmbeddingBase>,
    pub llm: Arc<dyn LLMBase>,
    pub access: AgentMemoryAccessContext,
}

pub(crate) struct ListAvailableMemoryKeysBrainTool {
    resources: AgentMemoryToolResources,
}

impl ListAvailableMemoryKeysBrainTool {
    pub(crate) fn new(resources: AgentMemoryToolResources) -> Self {
        Self { resources }
    }
}

impl BrainTool for ListAvailableMemoryKeysBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: "list_available_memory_keys",
            description: "列出当前上下文可访问的记忆标题，返回最近更新的记忆条目；可选按 query 过滤结果",
            parameters: json!({
                "type": "object",
                "properties": {
                    "top_n": { "type": "integer", "description": "返回条数，默认 5，最大 20" },
                    "query": { "type": "string", "description": "可选：用于过滤记忆结果的查询文本" }
                }
            }),
        })
    }

    fn execute(&self, _call_content: &str, arguments: &Value) -> String {
        let result = (|| -> Result<Value> {
            let top_n = sanitize_positive_limit(
                arguments.get("top_n").and_then(Value::as_i64),
                DEFAULT_MEMORY_TOP_N,
                MAX_MEMORY_TOP_N,
            );
            let query = optional_string_argument(arguments, "query");
            let hits = if let Some(query) = query.as_deref() {
                let vector = self.resources.embedding_model.inference(query)?;
                let mut hits = search_memory_content_by_vector(
                    &self.resources.memory_ref,
                    &self.resources.access,
                    &vector,
                    top_n,
                )?;
                hits.sort_by(|left, right| right.record.updated_at.cmp(&left.record.updated_at));
                hits
            } else {
                list_recent_memory_keys(
                    &self.resources.memory_ref,
                    &self.resources.access,
                    top_n,
                    None,
                )?
            };
            Ok(json!({
                "ok": true,
                "items": hits.into_iter().map(|hit| {
                    json!({
                        "object_id": hit.record.object_id,
                        "title": hit.record.key,
                        "updated_at": hit.record.updated_at,
                        "expires_at": hit.record.expires_at,
                    })
                }).collect::<Vec<_>>()
            }))
        })();

        render_tool_result(result)
    }
}

pub(crate) struct SearchMemoryContentBrainTool {
    resources: AgentMemoryToolResources,
}

impl SearchMemoryContentBrainTool {
    pub(crate) fn new(resources: AgentMemoryToolResources) -> Self {
        Self { resources }
    }
}

impl BrainTool for SearchMemoryContentBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: "search_memory_content",
            description: "搜索当前上下文可访问的记忆，返回相关记忆的标题与内容",
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "要搜索的记忆查询文本" },
                    "top_n": { "type": "integer", "description": "返回条数，默认 5，最大 20" }
                },
                "required": ["query"]
            }),
        })
    }

    fn execute(&self, _call_content: &str, arguments: &Value) -> String {
        let result = (|| -> Result<Value> {
            let query = optional_string_argument(arguments, "query")
                .ok_or_else(|| Error::ValidationError("query is required".to_string()))?;
            let top_n = sanitize_positive_limit(
                arguments.get("top_n").and_then(Value::as_i64),
                DEFAULT_MEMORY_TOP_N,
                MAX_MEMORY_TOP_N,
            );
            let vector = self.resources.embedding_model.inference(&query)?;
            let hits = search_memory_content_by_vector(
                &self.resources.memory_ref,
                &self.resources.access,
                &vector,
                top_n,
            )?;
            Ok(json!({
                "ok": true,
                "items": hits.into_iter().map(|hit| {
                    json!({
                        "object_id": hit.record.object_id,
                        "title": hit.record.key,
                        "value": hit.record.value,
                        "updated_at": hit.record.updated_at,
                        "expires_at": hit.record.expires_at,
                        "sender_id_list": hit.record.sender_id_list,
                        "group_id_list": hit.record.group_id_list,
                    })
                }).collect::<Vec<_>>()
            }))
        })();

        render_tool_result(result)
    }
}

pub(crate) struct RememberContentBrainTool {
    resources: AgentMemoryToolResources,
}

impl RememberContentBrainTool {
    pub(crate) fn new(resources: AgentMemoryToolResources) -> Self {
        Self { resources }
    }
}

impl BrainTool for RememberContentBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: "remember_content",
            description: "保存需要记住的信息，并返回已保存的记忆条目；可选限制可访问的用户或群范围",
            parameters: json!({
                "type": "object",
                "properties": {
                    "content": { "type": "string", "description": "要记住的内容" },
                    "sender_id_list": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "可选：仅这些 sender_id 可访问"
                    },
                    "group_id_list": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "可选：仅这些 group_id 可访问；若存在则访问判断优先看 group"
                    }
                },
                "required": ["content"]
            }),
        })
    }

    fn execute(&self, _call_content: &str, arguments: &Value) -> String {
        let result = (|| -> Result<Value> {
            let content = optional_string_argument(arguments, "content")
                .ok_or_else(|| Error::ValidationError("content is required".to_string()))?;
            let sender_id_list =
                optional_string_list_argument(arguments, "sender_id_list").unwrap_or_default();
            let group_id_list =
                optional_string_list_argument(arguments, "group_id_list").unwrap_or_default();
            let items = split_memory_items(&self.resources, &content)?;
            let expires_at = (Utc::now() + Duration::days(2)).to_rfc3339();
            let stored = items
                .into_iter()
                .map(|item| {
                    let vector = self
                        .resources
                        .embedding_model
                        .inference(&format!("{}\n{}", item.title, item.value))?;
                    create_memory_record_with_vector(
                        &self.resources.memory_ref,
                        &AgentMemoryUpsert {
                            key: item.title,
                            value: item.value,
                            expires_at: Some(expires_at.clone()),
                            sender_id_list: sender_id_list.clone(),
                            group_id_list: group_id_list.clone(),
                        },
                        Some(vector),
                    )
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(json!({
                "ok": true,
                "items": stored.into_iter().map(|item| {
                    json!({
                        "object_id": item.object_id,
                        "title": item.key,
                        "value": item.value,
                        "expires_at": item.expires_at,
                    })
                }).collect::<Vec<_>>()
            }))
        })();

        render_tool_result(result)
    }
}

#[derive(Debug, Clone, Deserialize)]
struct MemoryDraftItem {
    #[serde(alias = "key")]
    title: String,
    value: String,
}

fn split_memory_items(
    resources: &AgentMemoryToolResources,
    content: &str,
) -> Result<Vec<MemoryDraftItem>> {
    let prompt = vec![
        OpenAIMessage::system(
            "你是一个记忆整理器。把用户提供的内容拆成若干条适合长期检索的记忆。只返回 JSON 数组，不要 Markdown，不要解释。每项格式：{\"title\":\"记忆标题\",\"value\":\"记忆内容\"}。若内容只适合一条记忆，返回单元素数组。title 需要简洁明确，适合作为以后检索这条记忆的标题。不要泄露或引用当前对话之外的信息。",
        ),
        OpenAIMessage::user(format!("请整理下面的内容为记忆 JSON：\n{content}")),
    ];
    let response = resources.llm.inference(&InferenceParam {
        messages: &prompt,
        tools: None,
    });
    if let Some(text) = response.content_text_owned() {
        if let Some(parsed) = parse_memory_json(&text) {
            let normalized = normalize_draft_items(parsed);
            if !normalized.is_empty() {
                return Ok(normalized);
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
    let direct = serde_json::from_str::<Vec<MemoryDraftItem>>(trimmed).ok();
    if direct.is_some() {
        return direct;
    }
    let fenced = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .and_then(|value| value.strip_suffix("```"))
        .map(str::trim)?;
    serde_json::from_str::<Vec<MemoryDraftItem>>(fenced).ok()
}

fn normalize_draft_items(items: Vec<MemoryDraftItem>) -> Vec<MemoryDraftItem> {
    items
        .into_iter()
        .filter_map(|item| {
            let title = item.title.trim();
            let value = item.value.trim();
            if title.is_empty() || value.is_empty() {
                None
            } else {
                Some(MemoryDraftItem {
                    title: title.to_string(),
                    value: value.to_string(),
                })
            }
        })
        .collect()
}

fn summarize_memory_key(content: &str) -> String {
    let normalized = content.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = normalized.chars();
    let summary = chars.by_ref().take(32).collect::<String>();
    if chars.next().is_some() {
        summary
    } else if summary.is_empty() {
        "memory".to_string()
    } else {
        summary
    }
}

fn render_tool_result(result: Result<Value>) -> String {
    match result {
        Ok(value) => value.to_string(),
        Err(err) => json!({
            "ok": false,
            "error": err.to_string()
        })
        .to_string(),
    }
}
