//! Host capabilities a scheduled job script may call. The surface is deliberately narrow:
//! scripts orchestrate policy while all state access goes through these named handlers.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{json, Value};

use super::{JobMemoryResources, JobResources};
use crate::agent::declarative_agent::AgentDefinition;
use crate::agent::sub_agent_context::{AgentServiceContext, MemoryCapability};
use crate::agent::tools::NodeGraphTool;
use crate::agent::LLM_KIND_MAIN;
use crate::error::{Error, Result};
use crate::graph::DataValue;
use crate::model_inference::llm::MessageRole;
use crate::storage::{AgentMemoryAccessContext, MemoryRecord};

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
    // The job's memory backend backs the same memory tools the QQ turn's brain and the
    // deep-research sub agent use, so an inline definition may declare them in `tool_ids`;
    // without a backend they resolve as disabled placeholders and fail explicitly on call.
    let service_context = AgentServiceContext::new()
        .with_memory(resources.memory.as_ref().map(|memory| MemoryCapability {
            backend: memory.memory_backend.clone(),
            embedding_model: memory.embedding_model.clone(),
        }))
        .with_llm(LLM_KIND_MAIN, resources.llm.clone());
    let mut host = service_context.build_host(job_memory_access(params));
    for definition in resources
        .tool_definitions
        .iter()
        .filter(|definition| definition.uses_subgraph())
    {
        host.register_graph_tool(Arc::new(NodeGraphTool::new(definition.clone())));
    }
    let definition: AgentDefinition = serde_yaml::from_str(definition_text).map_err(|error| {
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
    let entries = (resources.history_loader)(sender_id)?
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
        write_source: crate::storage::MemoryWriteSource::Disabled,
        category: serde_json::from_value(
            params.get("category").cloned().unwrap_or_else(|| json!("concept")),
        )
        .unwrap_or_default(),
        permission_connection: None,
        agent_id: String::new(),
        sender_id: params.get("sender_id").and_then(Value::as_str).map(str::to_string),
        group_id: None,
        is_group: false,
        admin: false,
        max_access_level: None,
        skip_expiry_extend: false,
    }
}

fn memory_upsert(resources: &JobResources, params: &Value) -> Result<Value> {
    let memory = resources.memory.as_ref().ok_or_else(memory_unavailable)?;
    let record = store_agent_memory(memory, params)?;
    Ok(json!({
        "object_id": record.object_id(),
        "category": record.category(),
        "updated_at": record.updated_at(),
    }))
}

/// Writes one memory record described by the capability parameters into the job memory
/// backend, embedding the content when the backend needs a vector.
///
/// Parameter shape per category: `person` carries the profile fields (`qq_id` required,
/// the stored profile is kept and only `impression` is replaced), `concept` requires
/// `key` + `content` and honors `starts_at`, `behavioral` requires `key` + `content`
/// and honors `expires_at`. All categories accept `access_level`,
/// `sender_id_list` and `group_id_list`.
pub fn store_agent_memory(memory: &JobMemoryResources, params: &Value) -> Result<MemoryRecord> {
    let _write_guard = crate::storage::MEMORY_MUTATION_LOCK
        .lock()
        .map_err(|_| crate::string_error!("memory mutation lock poisoned"))?;
    let category: crate::storage::MemoryCategory = serde_json::from_value(
        params
            .get("category")
            .cloned()
            .ok_or_else(|| Error::ValidationError("缺少参数 'category'".to_string()))?,
    )?;
    let embedding_model = memory.embedding_model.as_deref();
    let access_level = params
        .get("access_level")
        .and_then(Value::as_u64)
        .map(|level| level as u32)
        .unwrap_or(crate::storage::user_permission::ROLE_SELF_ACCESS_LEVEL);
    match category {
        crate::storage::MemoryCategory::Person => {
            let mut profile: crate::storage::PersonMemory = serde_json::from_value(params.clone())?;
            let existing = memory.memory_backend.get_person(&profile.qq_id)?;
            let access_level =
                existing.as_ref().map(|record| record.access_level).unwrap_or(access_level);
            if let Some(existing) = existing {
                let impression = profile.impression;
                profile = existing.profile;
                profile.impression = impression;
            }
            memory
                .memory_backend
                .save_person(
                    embedding_model,
                    &crate::storage::PersonMemoryWrite { profile, access_level },
                )
                .map(MemoryRecord::Person)
        }
        crate::storage::MemoryCategory::Concept => {
            let write = crate::storage::ConceptMemoryWrite {
                key: required_str(params, "key")?.to_string(),
                content: required_str(params, "content")?.to_string(),
                access_level,
                starts_at: params.get("starts_at").and_then(Value::as_str).map(str::to_string),
                sender_id_list: optional_string_list(params, "sender_id_list"),
                group_id_list: optional_string_list(params, "group_id_list"),
            };
            memory
                .memory_backend
                .save_concept(embedding_model, &write)
                .map(MemoryRecord::Concept)
        }
        crate::storage::MemoryCategory::Behavioral => {
            let write = crate::storage::BehavioralMemoryWrite {
                key: required_str(params, "key")?.to_string(),
                content: required_str(params, "content")?.to_string(),
                access_level,
                expires_at: params.get("expires_at").and_then(Value::as_str).map(str::to_string),
                sender_id_list: optional_string_list(params, "sender_id_list"),
                group_id_list: optional_string_list(params, "group_id_list"),
            };
            memory
                .memory_backend
                .save_behavioral(embedding_model, &write)
                .map(MemoryRecord::Behavioral)
        }
    }
}

fn memory_search(resources: &JobResources, params: &Value) -> Result<Value> {
    let memory = resources.memory.as_ref().ok_or_else(memory_unavailable)?;
    let query: crate::storage::MemoryQuery = serde_json::from_value(
        params
            .get("query")
            .cloned()
            .ok_or_else(|| crate::string_error!("query required"))?,
    )?;
    let limit = optional_limit(params, "top_n", 5)?;
    let category = params
        .get("category")
        .cloned()
        .ok_or_else(|| crate::string_error!("category required"))?;
    let access = AgentMemoryAccessContext {
        category: serde_json::from_value(category)?,
        ..job_memory_access(params)
    };
    let hits = crate::agent::tools::memory::search_category(
        &memory.memory_backend,
        memory.embedding_model.as_deref(),
        &access,
        access.category,
        Some(&query),
        limit,
    )?;
    Ok(json!({"items": hits.into_iter().map(memory_hit_json).collect::<Vec<_>>()}))
}

fn memory_list(resources: &JobResources, params: &Value) -> Result<Value> {
    let memory = resources.memory.as_ref().ok_or_else(memory_unavailable)?;
    let limit = optional_limit(params, "limit", 20)?;
    let access = job_memory_access(params);
    let access = AgentMemoryAccessContext { skip_expiry_extend: true, ..access };
    let hits = crate::agent::tools::memory::search_category(
        &memory.memory_backend,
        memory.embedding_model.as_deref(),
        &access,
        access.category,
        None,
        limit,
    )?;
    Ok(json!({"items": hits.into_iter().map(memory_hit_json).collect::<Vec<_>>()}))
}

fn memory_hit_json(hit: crate::storage::MemorySearchHit) -> Value {
    serde_json::to_value(&hit).expect("memory serializes")
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
