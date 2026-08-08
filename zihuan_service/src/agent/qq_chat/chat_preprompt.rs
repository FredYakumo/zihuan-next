use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use log::{info, warn};
use serde_json::{json, Map, Value};

use zihuan_core::model_inference::inference_function::compact_message::compact_message_history;

use zihuan_core::agent::brain::{Brain, BrainStopReason, BrainTool, ToolExecutionOutput, ToolRunDuration};
use zihuan_core::agent::emotion::utils::emotion_dimensions_snapshot_text;
use zihuan_core::agent::session_state::QqChatAgentServiceSessionState;
use zihuan_core::agent_config::qq_chat::QqChatEmotionDimensionConfig;
use zihuan_core::data_refs::RelationalDbConnection;
use zihuan_core::llm::llm_base::LLMBase;
use zihuan_core::llm::tooling::FunctionTool;
use zihuan_core::llm::{LLMMessage, MessageRole};
use zihuan_core::runtime::block_async;
use zihuan_core::steer::message_with_api_style;
use zihuan_core::graph_engine::data_value::LLMMessageSessionCacheRef;
use zihuan_core::graph_engine::brain_tool_spec::{brain_tool_input_signature, BrainToolDefinition, ToolParamDef};
use zihuan_core::graph_engine::function_graph::{
    sync_function_subgraph_signature, FUNCTION_INPUTS_NODE_ID, FUNCTION_OUTPUTS_NODE_ID,
};
use zihuan_core::graph_engine::graph_io::refresh_port_types;
use zihuan_core::graph_engine::registry::build_node_graph_from_definition;
use zihuan_core::graph_engine::util::function::{
    data_value_from_json_with_declared_type, inject_runtime_values_into_function_inputs_node,
};
use zihuan_core::graph_engine::{DataType, DataValue};

use crate::agent::tools::{
    AgentMemoryToolResources, GetRecentGroupMessagesBrainTool, GetRecentUserMessagesBrainTool,
    ListAvailableMemoryKeysBrainTool, SearchMemoryContentBrainTool, ToolNotificationTarget, UpdateAgentStateBrainTool,
    DEFAULT_TOOL_GET_RECENT_GROUP_MESSAGES, DEFAULT_TOOL_GET_RECENT_USER_MESSAGES,
    DEFAULT_TOOL_LIST_AVAILABLE_MEMORY_KEYS, DEFAULT_TOOL_SEARCH_MEMORY_CONTENT,
};
use crate::storage::qq_chat_history_store::{load_history, save_history};

use super::{logging::QqChatBrainObserver, PreparedCurrentTurnUserInput, QqChatTaskTrace};

const LOG_PREFIX: &str = "[QqChatPrepromptAgent]";



/// prompt engineering

const DREAM_SYSTEM_PROMPT: &str =
    "You are the Dream memory consolidation agent. Produce concise long-term memories in English. Do not address the user. Use the available node graph tools synchronously when they are relevant to consolidating the memory.";

fn build_dream_user_prompt(previous_memory: &str, transcript: &str) -> String {
    format!(
        "Combine the previous Dream memory with this conversation. Record durable facts, preferences, relationships, emotions, and emotional continuity. Do not invent information.\n\nPrevious Dream memory:\n{previous_memory}\n\nCurrent conversation:\n{transcript}"
    )
}

fn build_chat_preprompt_agent_system_prompt(bot_name: &str, emotion_snapshot: &str) -> String {
    format!(
        "You are the chat-preprompt agent for the QQ bot `{bot_name}`. You run before the main reply agent every turn and prepare a context block that anchors its reply. You have two responsibilities.\n\
         \n[Responsibility 1: Emotion management]\n\
         Current emotion state:\n{emotion_snapshot}\n\
         Based on the current event and the independent emotion history, decide whether the emotion should be adjusted. Call `update_agent_state` only when a change is truly warranted; do not call any tool when no change is needed. When an adjustment is needed, specify an emotion dimension and `increase` or `decrease`. You may adjust multiple dimensions in the same event if each is genuinely necessary.\n\
         \n[Responsibility 2: Recall & consistency preprompt]\n\
         - Extract the key nouns / entities / proper nouns from the user's current message.\n\
         - For each, call `search_memory_content` to check whether you have related memory or an existing stance.\n\
         - When memory contains your prior stance on a topic, surface it so the main agent stays consistent and does not flip its likes/dislikes or opinions across turns.\n\
         - If a [Candidate Dream Memory] block is present, judge whether it is relevant to the current event. Only include relevant durable facts or continuity in the final context; omit unrelated Dream content completely.\n\
         - For nouns that have no related memory and that you do not already know, include in the final context block a line exactly like: 「xxx」这些名词没有相关内容，可能需要联网查询？\n\
         - When the user's question references something you said before, or tests consistency of your preferences, call `get_recent_user_messages` with your own id (provided in the event message below) to recall your own previous replies; you may also pass the current sender's id (also provided below) to recall that user's recent messages. In a group you may also use `get_recent_group_messages` for surrounding context.\n\
         - Only surface prior statements that are genuinely relevant to the current topic; never include unrelated recent replies just because they are recent.\n\
         \n[Output contract]\n\
         Your FINAL assistant message (the one with no further tool calls) MUST be a concise context block, and nothing else. Use this fixed shape, omitting any empty section:\n\
         [Recalled Memory]\n\
         - <title>: <brief>\n\
         [Dream Memory]\n\
         - <only relevant durable facts or continuity from the candidate Dream memory>\n\
         [Missing Knowledge]\n\
         - 「xxx」这些名词没有相关内容，可能需要联网查询？\n\
         [Recent Self Statements] (仅纳入与当前话题相关的过往发言，话题不相关的不要引入)\n\
         - {bot_name} 之前说过: \"<summary>\"\n\
         [Emotion Note]\n\
         - <only when emotion changed this turn>\n\
         If nothing relevant was found and no emotion changed, output a single line: [Preprompt] no recall needed.\n\
         This block is injected into the main reply prompt; it is NOT a reply to the user. Never claim to have sent a message."
    )
}

fn build_chat_preprompt_agent_user_message(
    input: &PreparedCurrentTurnUserInput,
    bot_name: &str,
    bot_id: &str,
    sender_id: &str,
    dream_memory: Option<&str>,
) -> String {
    let sender_name =
        zihuan_core::ims_bot_adapter::utils::sender_display_name!(&input.event.sender.nickname, &input.event.sender.card);
    let dream_candidate = dream_memory
        .filter(|content| !content.trim().is_empty())
        .map(|content| format!("\n\n[Candidate Dream Memory]\n{content}"))
        .unwrap_or_default();
    format!(
        "[Current QQ Event]\n`{sender_name}` sent a message to you (`{bot_name}`):\n{}\n\n\
         Your own id is `{bot_id}`; pass it to `get_recent_user_messages` to recall your own previous replies.\n\
         The current sender's id is `{sender_id}`; pass it to `get_recent_user_messages` to recall this user's recent messages.\n\
         Evaluate whether this event should change your emotion state, and prepare the preprompt context block per the output contract.{dream_candidate}",
        input.current_text_for_prompt(),
    )
}

/// ================================================

struct DreamNodeGraphTool {
    definition: BrainToolDefinition,
}

impl DreamNodeGraphTool {
    fn new(definition: BrainToolDefinition) -> Self {
        Self { definition }
    }

    fn run_node_graph(&self, call_content: &str, arguments: &Value) -> zihuan_core::error::Result<String> {
        let arguments = arguments.as_object().ok_or_else(|| {
            zihuan_core::error::Error::ValidationError(format!(
                "Dream node graph tool '{}' requires JSON object arguments",
                self.definition.name
            ))
        })?;
        let mut runtime_values = HashMap::new();
        runtime_values.insert("content".to_string(), DataValue::String(call_content.to_string()));
        for parameter in &self.definition.parameters {
            let Some(value) = arguments.get(&parameter.name) else {
                if parameter.required {
                    return Err(zihuan_core::error::Error::ValidationError(format!(
                        "Dream node graph tool '{}' is missing required parameter '{}'",
                        self.definition.name, parameter.name
                    )));
                }
                continue;
            };
            if value.is_null() && !parameter.required {
                continue;
            }
            let port = zihuan_core::graph_engine::function_graph::FunctionPortDef {
                name: parameter.name.clone(),
                data_type: parameter.data_type.clone(),
                description: parameter.desc.clone(),
                required: parameter.required,
            };
            runtime_values.insert(
                parameter.name.clone(),
                data_value_from_json_with_declared_type(&port, value)?,
            );
        }

        let input_signature = brain_tool_input_signature("brain", &[], &self.definition);
        let mut subgraph = self.definition.subgraph.clone();
        sync_function_subgraph_signature(&mut subgraph, &input_signature, &self.definition.outputs);
        refresh_port_types(&mut subgraph);

        let function_inputs_node = subgraph.nodes.iter_mut().find(|node| node.id == FUNCTION_INPUTS_NODE_ID).ok_or_else(|| {
            zihuan_core::error::Error::ValidationError(format!(
                "Dream node graph tool '{}' is missing the function_inputs boundary node",
                self.definition.name
            ))
        })?;
        function_inputs_node.inline_values.insert(
            zihuan_core::graph_engine::function_graph::FUNCTION_SIGNATURE_PORT.to_string(),
            serde_json::to_value(&input_signature).unwrap_or(Value::Null),
        );

        let function_outputs_node = subgraph.nodes.iter_mut().find(|node| node.id == FUNCTION_OUTPUTS_NODE_ID).ok_or_else(|| {
            zihuan_core::error::Error::ValidationError(format!(
                "Dream node graph tool '{}' is missing the function_outputs boundary node",
                self.definition.name
            ))
        })?;
        function_outputs_node.inline_values.insert(
            zihuan_core::graph_engine::function_graph::FUNCTION_SIGNATURE_PORT.to_string(),
            serde_json::to_value(&self.definition.outputs).unwrap_or(Value::Null),
        );

        let mut graph = build_node_graph_from_definition(&subgraph).map_err(|error| {
            zihuan_core::error::Error::ValidationError(format!(
                "Dream node graph tool '{}' could not build its subgraph: {error}",
                self.definition.name
            ))
        })?;
        inject_runtime_values_into_function_inputs_node(&mut graph, runtime_values.into()).map_err(|error| {
            zihuan_core::error::Error::ValidationError(format!(
                "Dream node graph tool '{}' could not inject runtime inputs: {error}",
                self.definition.name
            ))
        })?;
        let execution_result = graph.execute_and_capture_results();
        if let Some(error) = execution_result.error_message {
            return Err(zihuan_core::error::Error::ValidationError(format!(
                "Dream node graph tool '{}' failed: {error}",
                self.definition.name
            )));
        }
        let output_values = execution_result.node_results.get(FUNCTION_OUTPUTS_NODE_ID).ok_or_else(|| {
            zihuan_core::error::Error::ValidationError(format!(
                "Dream node graph tool '{}' produced no function_outputs result",
                self.definition.name
            ))
        })?;
        let mut result = Map::new();
        for output in &self.definition.outputs {
            let value = output_values.get(&output.name).ok_or_else(|| {
                zihuan_core::error::Error::ValidationError(format!(
                    "Dream node graph tool '{}' did not provide output '{}'",
                    self.definition.name, output.name
                ))
            })?;
            if !output.data_type.is_compatible_with(&value.data_type()) {
                return Err(zihuan_core::error::Error::ValidationError(format!(
                    "Dream node graph tool '{}' output '{}' type mismatch: expected {}, got {}",
                    self.definition.name, output.name, output.data_type, value.data_type()
                )));
            }
            result.insert(output.name.clone(), value.to_json());
        }
        Ok(Value::Object(result).to_string())
    }
}

impl BrainTool for DreamNodeGraphTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(DreamNodeGraphFunctionTool { definition: self.definition.clone() })
    }

    fn run_duration(&self) -> ToolRunDuration {
        self.definition.run_duration
    }

    fn execute(&self, call_content: &str, arguments: &Value) -> String {
        self.run_node_graph(call_content, arguments)
            .unwrap_or_else(|error| format!("Dream node graph tool '{}' failed: {error}", self.definition.name))
    }

    fn execute_with_outcome(&self, call_content: &str, arguments: &Value) -> ToolExecutionOutput {
        ToolExecutionOutput::text(self.execute(call_content, arguments))
    }
}

#[derive(Debug)]
struct DreamNodeGraphFunctionTool {
    definition: BrainToolDefinition,
}

impl FunctionTool for DreamNodeGraphFunctionTool {
    fn name(&self) -> &str {
        &self.definition.name
    }

    fn description(&self) -> &str {
        &self.definition.description
    }

    fn parameters(&self) -> Value {
        tool_parameters_to_json_schema(&self.definition.parameters)
    }

    fn call(&self, arguments: Value) -> zihuan_core::error::Result<Value> {
        Ok(arguments)
    }
}

fn tool_parameters_to_json_schema(parameters: &[ToolParamDef]) -> Value {
    let mut properties = Map::new();
    let mut required = Vec::new();
    for parameter in parameters {
        if parameter.required {
            required.push(Value::String(parameter.name.clone()));
        }
        properties.insert(
            parameter.name.clone(),
            json!({
                "type": data_type_to_json_schema_type(&parameter.data_type),
                "description": parameter.desc,
            }),
        );
    }
    json!({"type": "object", "properties": properties, "required": required})
}

fn data_type_to_json_schema_type(data_type: &DataType) -> &'static str {
    match data_type {
        DataType::String | DataType::Password | DataType::Binary => "string",
        DataType::Integer => "integer",
        DataType::Float => "number",
        DataType::Boolean => "boolean",
        DataType::Vec(_) | DataType::Vector => "array",
        _ => "object",
    }
}

pub(crate) fn run_dream_agent(
    llm: Arc<dyn LLMBase>,
    previous_memory: &str,
    transcript: &str,
    tool_definitions: Vec<BrainToolDefinition>,
) -> zihuan_core::error::Result<String> {
    let messages = vec![
        LLMMessage::system(DREAM_SYSTEM_PROMPT),
        LLMMessage::user(build_dream_user_prompt(previous_memory, transcript)),
    ];
    let mut brain = Brain::new(llm);
    for definition in tool_definitions.into_iter().filter(BrainToolDefinition::uses_subgraph) {
        brain.add_tool(DreamNodeGraphTool::new(definition));
    }
    let (output, stop_reason) = brain.run(messages);
    if !matches!(stop_reason, BrainStopReason::Done) {
        return Err(zihuan_core::error::Error::StringError("Dream Agent did not complete normally".to_string()));
    }
    output
        .last()
        .and_then(LLMMessage::content_text_owned)
        .filter(|content| !content.trim().is_empty())
        .ok_or_else(|| zihuan_core::error::Error::StringError("Dream Agent returned no text".to_string()))
}







#[allow(clippy::too_many_arguments)]
pub(crate) fn run_chat_preprompt_agent(
    trace: &QqChatTaskTrace,
    llm: &Arc<dyn LLMBase>,
    cache: &Arc<LLMMessageSessionCacheRef>,
    history_key: &str,
    input: &PreparedCurrentTurnUserInput,
    bot_name: &str,
    bot_id: &str,
    agent_id: &str,
    sender_id: &str,
    target_id: &str,
    is_group: bool,
    session_state: Arc<Mutex<QqChatAgentServiceSessionState>>,
    emotion_dimensions: Vec<QqChatEmotionDimensionConfig>,
    compact_context_length: usize,
    memory_resources: Option<AgentMemoryToolResources>,
    rdb_pool: Option<RelationalDbConnection>,
    default_tools_enabled: &HashMap<String, bool>,
) -> Option<String> {
    trace.record_graph_phase("名词处理", serde_json::json!({"status": "preprompt"}));
    trace.record_graph_phase("情绪维度处理", serde_json::json!({"status": "preprompt"}));
    trace.record_graph_phase("获取最近消息", serde_json::json!({"status": "preprompt tool when needed"}));
    let original_session_state = {
        let session_state = session_state.lock().unwrap();
        session_state.clone()
    };

    let emotion_snapshot = emotion_dimensions_snapshot_text(&original_session_state, &emotion_dimensions);
    let dream_memory = rdb_pool.as_ref().and_then(|connection| {
        match block_async(crate::scheduled_task::latest_dream_memory(connection, agent_id, sender_id)) {
            Ok(memory) => memory,
            Err(err) => {
                warn!("{LOG_PREFIX} failed to load Dream memory for sender={sender_id}: {err}");
                None
            }
        }
    });
    let user_message = message_with_api_style(
        LLMMessage::user(build_chat_preprompt_agent_user_message(
            input,
            bot_name,
            bot_id,
            sender_id,
            dream_memory.as_deref(),
        )),
        llm.api_style(),
    );

    let history = load_history(cache, history_key);
    let compact_result = compact_message_history(llm, history, compact_context_length, &user_message);
    let mut history = compact_result.messages;
    if compact_result.did_compact {
        info!(
            "{LOG_PREFIX} history compacted for {history_key}: tokens {} -> {}",
            compact_result.estimated_tokens_before, compact_result.estimated_tokens_after
        );
    }

    let mut conversation = Vec::with_capacity(history.len() + 2);
    conversation.push(message_with_api_style(
        LLMMessage::system(build_chat_preprompt_agent_system_prompt(bot_name, &emotion_snapshot)),
        llm.api_style(),
    ));
    conversation.extend(history.iter().cloned());
    conversation.push(user_message.clone());

    let mut brain = Brain::new(Arc::clone(llm));
    brain.set_observer(Arc::new(QqChatBrainObserver { trace: trace.clone() }));
    brain.add_tool(UpdateAgentStateBrainTool::new(
        Arc::clone(&session_state),
        emotion_dimensions,
        Arc::clone(llm),
        input.current_text_for_prompt().to_string(),
    ));

    let is_enabled = |name: &str| *default_tools_enabled.get(name).unwrap_or(&true);

    if let Some(memory_resources) = memory_resources {
        if is_enabled(DEFAULT_TOOL_SEARCH_MEMORY_CONTENT) {
            brain.add_tool(SearchMemoryContentBrainTool::new(memory_resources.clone()));
        }
        if is_enabled(DEFAULT_TOOL_LIST_AVAILABLE_MEMORY_KEYS) {
            brain.add_tool(ListAvailableMemoryKeysBrainTool::new(memory_resources));
        }
    }

    // The preprompt agent must not emit user-facing tool progress notifications, so the
    // notification target carries no adapter and has progress disabled. Read-only history
    // tools only read `target_id` / `is_group` from it and never send messages.
    let notification_target = ToolNotificationTarget::new(None, target_id.to_string(), None, is_group, false);

    if rdb_pool.is_some() && is_enabled(DEFAULT_TOOL_GET_RECENT_USER_MESSAGES) {
        brain.add_tool(GetRecentUserMessagesBrainTool::new(
            rdb_pool.clone(),
            notification_target.clone(),
        ));
    }
    if is_group && rdb_pool.is_some() && is_enabled(DEFAULT_TOOL_GET_RECENT_GROUP_MESSAGES) {
        brain.add_tool(GetRecentGroupMessagesBrainTool::new(rdb_pool, notification_target));
    }

    let (output, stop_reason) = brain.run(conversation);

    let context_block = match stop_reason {
        BrainStopReason::Done => output
            .iter()
            .rev()
            .find(|message| matches!(message.role, MessageRole::Assistant))
            .and_then(|message| message.content_text_owned())
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty()),
        _ => {
            warn!("{LOG_PREFIX} inference ended without normal completion: {stop_reason:?}");
            *session_state.lock().unwrap() = original_session_state;
            None
        }
    };

    history.push(user_message);
    history.extend(output);
    save_history(cache, history_key, history);
    context_block
}
