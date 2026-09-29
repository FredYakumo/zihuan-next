//! Host capabilities a scheduled job script may call. The surface is deliberately narrow:
//! scripts orchestrate policy while all state access goes through these named handlers.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{json, Value};

use super::{JobMemoryResources, JobResources};
use crate::agent::declarative_agent::{AgentDefinition, AgentHost};
use crate::agent::tools::memory_tools::MemoryBackend;
use crate::agent::tools::NodeGraphTool;
use crate::agent::LLM_KIND_MAIN;
use crate::error::{Error, Result};
use crate::graph::DataValue;
use crate::model_inference::llm::MessageRole;
use crate::storage::{
    list_memory_in_store, search_memory_in_store, upsert_memory_in_store, AgentMemoryAccessContext,
    AgentMemoryRecord, AgentMemoryUpsert,
};

pub fn dispatch(
    resources: &JobResources,
    method: &str,
    params: &Value,
) -> std::result::Result<Value, String> {
    dispatch_inner(resources, method, params).map_err(|error| error.to_string())
}

fn dispatch_inner(resources: &JobResources, method: &str, params: &Value) -> Result<Value> {
    match method {
        "subagent.run" => run_subagent(resources, params),
        "history.load" => load_history(resources, params),
        "history.clear" => clear_history(resources, params),
        "memory.upsert" => memory_upsert(resources, params),
        "memory.search" => memory_search(resources, params),
        "memory.list" => memory_list(resources, params),
        "log" => log_message(params),
        other => Err(Error::ValidationError(format!("不支持的调度任务宿主调用: {other}"))),
    }
}

fn run_subagent(resources: &JobResources, params: &Value) -> Result<Value> {
    let definition_text = params
        .get("definition")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::ValidationError("subagent.run 缺少 definition".to_string()))?;
    let inputs = params
        .get("inputs")
        .and_then(Value::as_object)
        .ok_or_else(|| Error::ValidationError("subagent.run 缺少 inputs 对象".to_string()))?;
    let mut host = AgentHost::new();
    for definition in resources
        .tool_definitions
        .iter()
        .filter(|definition| definition.uses_subgraph())
    {
        host.register_graph_tool(Arc::new(NodeGraphTool::new(definition.clone())));
    }
    host.register_llm(LLM_KIND_MAIN, resources.llm.clone());
    let mut definition: AgentDefinition =
        serde_yaml::from_str(definition_text).map_err(|error| {
            Error::ValidationError(format!("subagent.run definition 不是有效的代理定义: {error}"))
        })?;
    definition.validate(&host.available_tool_ids())?;
    let agent = host.build_definition(definition)?;
    let mut input = HashMap::new();
    for (key, value) in inputs {
        let text = value.as_str().ok_or_else(|| {
            Error::ValidationError(format!("subagent.run inputs 的 '{key}' 必须是字符串"))
        })?;
        input.insert(key.clone(), DataValue::String(text.to_string()));
    }
    let result = agent.run_text(input)?;
    Ok(Value::String(result))
}

fn load_history(resources: &JobResources, params: &Value) -> Result<Value> {
    let sender_id = required_str(params, "sender_id")?;
    let entries = (resources.history_loader)(sender_id)
        .into_iter()
        .map(|message| {
            let text = message.content_text_owned();
            let role = match message.role {
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
                MessageRole::System => "system",
                MessageRole::Tool => "tool",
            };
            json!({"role": role, "text": text})
        })
        .collect();
    Ok(Value::Array(entries))
}

fn clear_history(resources: &JobResources, params: &Value) -> Result<Value> {
    let sender_id = required_str(params, "sender_id")?;
    (resources.history_clearer)(sender_id)?;
    Ok(Value::Null)
}

fn memory_unavailable() -> Error {
    Error::ValidationError("此服务未注册记忆后端，无法使用 memory 能力".to_string())
}

/// Builds the per-call access scope: jobs are sender-scoped, never admin, and never
/// group-scoped because the trigger carries a single sender.
fn job_memory_access(params: &Value) -> AgentMemoryAccessContext {
    AgentMemoryAccessContext {
        sender_id: params.get("sender_id").and_then(Value::as_str).map(str::to_string),
        group_id: None,
        is_group: false,
        admin: false,
        skip_expiry_extend: false,
    }
}

fn memory_embedding_vector(memory: &JobMemoryResources, text: &str) -> Result<Vec<f32>> {
    memory
        .embedding_model
        .as_ref()
        .ok_or_else(|| Error::ValidationError("memory 后端需要配置 embedding 模型".to_string()))?
        .inference(text)
}

fn memory_upsert(resources: &JobResources, params: &Value) -> Result<Value> {
    let memory = resources.memory.as_ref().ok_or_else(memory_unavailable)?;
    let input = AgentMemoryUpsert {
        key: required_str(params, "key")?.to_string(),
        value: required_str(params, "value")?.to_string(),
        expires_at: params.get("expires_at").and_then(Value::as_str).map(str::to_string),
        sender_id_list: optional_string_list(params, "sender_id_list"),
        group_id_list: optional_string_list(params, "group_id_list"),
    };
    let record = store_agent_memory(memory, &input)?;
    Ok(json!({
        "object_id": record.object_id,
        "key": record.key,
        "value": record.value,
        "updated_at": record.updated_at,
    }))
}

/// Writes one memory record into the job memory backend, embedding the content when the
/// backend needs a vector. Shared by the `memory.upsert` capability and the legacy
/// dream-memory migration.
pub fn store_agent_memory(
    memory: &JobMemoryResources,
    input: &AgentMemoryUpsert,
) -> Result<AgentMemoryRecord> {
    match &memory.memory_backend {
        MemoryBackend::LocalFile(store) => store.create_or_update(input),
        MemoryBackend::RetrievalStore(store) => upsert_memory_in_store(
            store,
            input,
            memory_embedding_vector(memory, &format!("{}\n{}", input.key, input.value))?,
        ),
    }
}

fn memory_search(resources: &JobResources, params: &Value) -> Result<Value> {
    let memory = resources.memory.as_ref().ok_or_else(memory_unavailable)?;
    let query = required_str(params, "query")?;
    let limit = optional_limit(params, "top_n", 5)?;
    let access = job_memory_access(params);
    let hits = match &memory.memory_backend {
        MemoryBackend::LocalFile(store) => store.list(Some(query), limit)?,
        MemoryBackend::RetrievalStore(store) => search_memory_in_store(
            store,
            &access,
            query,
            &memory_embedding_vector(memory, query)?,
            limit,
        )?,
    };
    Ok(json!({"items": hits.into_iter().map(memory_hit_json).collect::<Vec<_>>()}))
}

fn memory_list(resources: &JobResources, params: &Value) -> Result<Value> {
    let memory = resources.memory.as_ref().ok_or_else(memory_unavailable)?;
    let limit = optional_limit(params, "limit", 20)?;
    let access = job_memory_access(params);
    let hits = match &memory.memory_backend {
        MemoryBackend::LocalFile(store) => store.list(None, limit)?,
        MemoryBackend::RetrievalStore(store) => list_memory_in_store(store, &access, limit)?,
    };
    Ok(json!({"items": hits.into_iter().map(memory_hit_json).collect::<Vec<_>>()}))
}

fn memory_hit_json(hit: crate::storage::AgentMemorySearchHit) -> Value {
    let record = hit.record;
    json!({
        "object_id": record.object_id,
        "key": record.key,
        "value": record.value,
        "updated_at": record.updated_at,
        "expires_at": record.expires_at,
        "sender_id_list": record.sender_id_list,
        "group_id_list": record.group_id_list,
    })
}

fn optional_limit(params: &Value, key: &str, default: i64) -> Result<usize> {
    let raw = params.get(key).and_then(Value::as_i64).unwrap_or(default);
    usize::try_from(raw.clamp(1, 100))
        .map_err(|_| Error::ValidationError(format!("参数 '{key}' 无效")))
}

fn optional_string_list(params: &Value, key: &str) -> Vec<String> {
    params
        .get(key)
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default()
}

fn log_message(params: &Value) -> Result<Value> {
    let level = params.get("level").and_then(Value::as_str).unwrap_or("info");
    let message = params.get("message").and_then(Value::as_str).unwrap_or_default();
    match level {
        "warn" => log::warn!("[SchedulerJob] {message}"),
        "error" => log::error!("[SchedulerJob] {message}"),
        _ => log::info!("[SchedulerJob] {message}"),
    }
    Ok(Value::Null)
}

fn required_str<'a>(params: &'a Value, key: &str) -> Result<&'a str> {
    params
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| Error::ValidationError(format!("缺少参数 '{key}'")))
}
