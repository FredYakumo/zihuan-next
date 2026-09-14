//! Host capabilities a scheduled job script may call. The surface is deliberately narrow:
//! scripts orchestrate policy while all state access goes through these named handlers.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{json, Value};

use super::JobResources;
use crate::agent::tools::NodeGraphTool;
use crate::agent::yaml_agent::{YamlAgentDefinition, YamlAgentHost};
use crate::agent::LLM_KIND_MAIN;
use crate::error::{Error, Result};
use crate::graph::DataValue;
use crate::model_inference::llm::MessageRole;
use crate::runtime::block_async;
use crate::scheduled_task;

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
        "dream_memory.latest" => latest_dream_memory(resources, params),
        "dream_memory.insert" => insert_dream_memory(resources, params),
        "log" => log_message(params),
        other => Err(Error::ValidationError(format!("不支持的调度任务宿主调用: {other}"))),
    }
}

fn run_subagent(resources: &JobResources, params: &Value) -> Result<Value> {
    let definition_text = params.get("definition").and_then(Value::as_str).ok_or_else(|| {
        Error::ValidationError("subagent.run 缺少 definition YAML".to_string())
    })?;
    let inputs = params
        .get("inputs")
        .and_then(Value::as_object)
        .ok_or_else(|| Error::ValidationError("subagent.run 缺少 inputs 对象".to_string()))?;
    let mut host = YamlAgentHost::new();
    for definition in resources
        .tool_definitions
        .iter()
        .filter(|definition| definition.uses_subgraph())
    {
        host.register_graph_tool(Arc::new(NodeGraphTool::new(definition.clone())));
    }
    host.register_llm(LLM_KIND_MAIN, resources.llm.clone());
    let mut definition: YamlAgentDefinition =
        serde_yaml::from_str(definition_text).map_err(|error| {
            Error::ValidationError(format!("subagent.run definition 不是有效的代理 YAML: {error}"))
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

fn latest_dream_memory(resources: &JobResources, params: &Value) -> Result<Value> {
    let agent_id = required_str(params, "agent_id")?;
    let sender_id = required_str(params, "sender_id")?;
    let memory = block_async(scheduled_task::latest_dream_memory(
        &resources.connection,
        agent_id,
        sender_id,
    ))?;
    Ok(memory.map(Value::String).unwrap_or(Value::Null))
}

fn insert_dream_memory(resources: &JobResources, params: &Value) -> Result<Value> {
    let agent_id = required_str(params, "agent_id")?;
    let sender_id = required_str(params, "sender_id")?;
    let chars = params.get("chars").and_then(Value::as_i64).ok_or_else(|| {
        Error::ValidationError("dream_memory.insert 缺少参数 'chars'".to_string())
    })?;
    let content = required_str(params, "content")?;
    block_async(scheduled_task::insert_dream_memory(
        &resources.connection,
        agent_id,
        sender_id,
        chars,
        content,
    ))?;
    Ok(Value::Null)
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
