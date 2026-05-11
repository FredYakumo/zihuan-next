use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use super::{AgentManager, AgentRuntimeState, AgentRuntimeStatus};
use crate::resource_resolver::{
    build_embedding_model, build_llm_model, resolve_llm_service_config,
    resolve_local_embedding_model_name,
};
use chrono::Local;
use ims_bot_adapter::adapter::BotAdapter;
use ims_bot_adapter::event::EventHandler;
use ims_bot_adapter::models::message::{
    AtTargetMessage, ForwardMessage, ForwardNodeMessage, ImageMessage, Message, PlainTextMessage,
    ReplyMessage,
};
use ims_bot_adapter::{build_ims_bot_adapter, parse_ims_bot_adapter_connection};
use log::{error, info};
use storage_handler::{
    build_mysql_ref, build_s3_ref, build_tavily_ref, build_weaviate_ref, find_connection,
    ConnectionConfig, ConnectionKind,
};
use tokio::task::JoinHandle;
use zihuan_core::error::{Error, Result};
use zihuan_core::task_context::AgentTaskRuntime;
use zihuan_core::worker_pool::WorkerPool;
use zihuan_graph_engine::data_value::{OpenAIMessageSessionCacheRef, SessionStateRef};
use zihuan_graph_engine::function_graph::FunctionPortDef;
use zihuan_graph_engine::graph_boundary::{root_graph_to_tool_subgraph, sync_root_graph_io};
use zihuan_graph_engine::message_restore::register_mysql_ref;
use zihuan_graph_engine::DataType;
use zihuan_llm::agent::qq_chat_agent::{
    QqAgentReplyBatchBuilder, QqAgentReplyBuildRequest, QqAgentReplyBuildResult,
    QqChatAgentService, QqChatAgentServiceConfig,
};
use zihuan_llm::brain_tool::{
    fixed_tool_runtime_inputs, BrainToolDefinition, QQ_AGENT_TOOL_OWNER_TYPE,
};
use zihuan_llm::nn::embedding::embedding_runtime_manager::RuntimeEmbeddingModelManager;
use zihuan_llm::system_config::{
    load_llm_refs, AgentConfig, AgentToolConfig, AgentToolType, LlmRefConfig, NodeGraphToolConfig,
    QqChatAgentConfig,
};

const FORWARD_SPLIT_PREFERRED_SEPARATORS: [char; 14] = [
    '\n', '。', '！', '？', '；', '：', '.', '!', '?', ';', ':', '，', ',', ' ',
];
const LEGACY_QQ_AGENT_TOOL_OWNER_TYPE: &str = "qq_message_agent";

#[derive(Debug, Clone)]
enum ReplySegment {
    Text(String),
    Message(Message),
    NoReply,
}

#[derive(Debug, Clone, Copy, Default)]
struct SplitRepairState {
    in_code_fence: bool,
    in_double_quote: bool,
    in_cn_quote: bool,
}

fn build_reply_batch_builder() -> QqAgentReplyBatchBuilder {
    Arc::new(build_reply_batches_from_model_text)
}

fn build_reply_batches_from_model_text(
    request: &QqAgentReplyBuildRequest,
) -> Result<QqAgentReplyBuildResult> {
    let resolved_text;
    let text = if let Some(mid) = request.trigger_message_id {
        resolved_text = resolve_reply_his_message_aliases(&request.assistant_text, mid);
        &resolved_text
    } else {
        &request.assistant_text
    };
    let sender_resolved_text;
    let text = if request.is_group {
        sender_resolved_text = text.replace("@sender", &format!("@{}", request.sender_id));
        &sender_resolved_text
    } else {
        text
    };
    let segments = parse_reply_segments(text);
    if segments
        .iter()
        .any(|segment| matches!(segment, ReplySegment::NoReply))
    {
        return Ok(QqAgentReplyBuildResult {
            batches: Vec::new(),
            suppress_send: true,
        });
    }

    let mut batches = Vec::new();
    let mut current_batch = Vec::new();
    let mut current_text_chars = 0usize;
    let mut pending_reply: Option<ReplyMessage> = None;

    for segment in segments {
        match segment {
            ReplySegment::Text(text) => {
                let text = text.trim().to_string();
                if text.is_empty() {
                    continue;
                }

                let text_chars = text.chars().count();
                if text_chars > request.max_message_length {
                    flush_batch(&mut batches, &mut current_batch);
                    if let Some(forward) = build_forward_from_text(
                        &text,
                        request.max_message_length,
                        &request.bot_id,
                        &request.bot_name,
                    )? {
                        batches.push(vec![Message::Forward(forward)]);
                    }
                    current_text_chars = 0;
                    continue;
                }

                if current_text_chars > 0
                    && current_text_chars + text_chars > request.max_message_length
                {
                    flush_batch(&mut batches, &mut current_batch);
                    current_text_chars = 0;
                }

                current_text_chars += text_chars;
                current_batch.push(Message::PlainText(PlainTextMessage { text }));
            }
            ReplySegment::Message(Message::At(at)) => {
                current_batch.push(Message::At(at));
            }
            ReplySegment::Message(Message::Reply(reply)) => {
                if pending_reply.is_none() {
                    pending_reply = Some(reply);
                }
            }
            ReplySegment::Message(message) => {
                flush_batch(&mut batches, &mut current_batch);
                current_text_chars = 0;
                batches.push(vec![message]);
            }
            ReplySegment::NoReply => {}
        }
    }

    flush_batch(&mut batches, &mut current_batch);
    if let Some(reply) = pending_reply {
        attach_reply_to_first_batch(&mut batches, reply);
    }
    ensure_space_after_at(&mut batches);
    Ok(QqAgentReplyBuildResult {
        batches,
        suppress_send: false,
    })
}

fn resolve_reply_his_message_aliases(text: &str, trigger_message_id: i64) -> String {
    text.replace(
        "[Reply his_message]",
        &format!("[Reply message_id={trigger_message_id}]"),
    )
    .replace(
        "[Reply his message]",
        &format!("[Reply message_id={trigger_message_id}]"),
    )
}

fn ensure_space_after_at(batches: &mut [Vec<Message>]) {
    for batch in batches {
        for i in 0..batch.len().saturating_sub(1) {
            if matches!(batch[i], Message::At(_)) {
                if let Message::PlainText(ref mut pt) = batch[i + 1] {
                    if !pt.text.starts_with(' ') {
                        pt.text = format!(" {}", pt.text);
                    }
                }
            }
        }
    }
}

fn flush_batch(batches: &mut Vec<Vec<Message>>, current_batch: &mut Vec<Message>) {
    if !current_batch.is_empty() {
        batches.push(std::mem::take(current_batch));
    }
}

fn attach_reply_to_first_batch(batches: &mut Vec<Vec<Message>>, reply: ReplyMessage) {
    if let Some(first_batch) = batches.first_mut() {
        first_batch.insert(0, Message::Reply(reply));
    } else {
        batches.push(vec![Message::Reply(reply)]);
    }
}

fn build_forward_from_text(
    text: &str,
    max_chars: usize,
    bot_id: &str,
    bot_name: &str,
) -> Result<Option<ForwardMessage>> {
    let chunks = split_plain_text_for_forward(text, max_chars);
    if chunks.is_empty() {
        return Ok(None);
    }

    let content = chunks
        .into_iter()
        .map(|chunk| ForwardNodeMessage {
            user_id: Some(bot_id.to_string()),
            nickname: Some(bot_name.to_string()),
            id: None,
            content: vec![Message::PlainText(PlainTextMessage { text: chunk })],
        })
        .collect();

    Ok(Some(ForwardMessage { id: None, content }))
}

fn split_plain_text_for_forward(text: &str, max_chars: usize) -> Vec<String> {
    let normalized = text.replace("\r\n", "\n");
    let trimmed = normalized.trim();
    if trimmed.is_empty() || max_chars == 0 {
        return Vec::new();
    }

    let chars: Vec<char> = trimmed.chars().collect();
    let mut start = 0usize;
    let mut carry_prefix = String::new();
    let mut state = SplitRepairState::default();
    let mut chunks = Vec::new();

    while start < chars.len() {
        let prefix_chars = carry_prefix.chars().count();
        let available = max_chars.saturating_sub(prefix_chars).max(1);
        let end = find_split_end(&chars, start, available);
        let mut chunk = carry_prefix.clone();
        chunk.extend(chars[start..end].iter());
        start = end;

        let has_more = start < chars.len();
        let analysis = analyze_chunk_state(&chunk, state);
        let mut finalized = chunk.trim().to_string();
        carry_prefix.clear();

        if has_more {
            if analysis.in_code_fence {
                finalized.push_str("\n```");
                carry_prefix.push_str("```\n");
            }
            if analysis.in_cn_quote {
                finalized.push('”');
                carry_prefix.push('“');
            }
            if analysis.in_double_quote {
                finalized.push('"');
                carry_prefix.push('"');
            }
        }

        let finalized = finalized.trim().to_string();
        if !finalized.is_empty() {
            chunks.push(finalized);
        }
        state = analysis;
    }

    chunks
}

fn find_split_end(chars: &[char], start: usize, max_chars: usize) -> usize {
    let hard_end = (start + max_chars).min(chars.len());
    if hard_end >= chars.len() {
        return chars.len();
    }

    for idx in (start + 1..hard_end).rev() {
        if FORWARD_SPLIT_PREFERRED_SEPARATORS.contains(&chars[idx - 1]) {
            return idx;
        }
    }

    hard_end
}

fn analyze_chunk_state(chunk: &str, mut state: SplitRepairState) -> SplitRepairState {
    let mut iter = chunk.chars().peekable();
    while let Some(ch) = iter.next() {
        if ch == '`' {
            let mut count = 1usize;
            while matches!(iter.peek(), Some('`')) {
                iter.next();
                count += 1;
            }
            if count >= 3 {
                state.in_code_fence = !state.in_code_fence;
                continue;
            }
        }

        if state.in_code_fence {
            continue;
        }

        match ch {
            '"' => state.in_double_quote = !state.in_double_quote,
            '“' => state.in_cn_quote = true,
            '”' => state.in_cn_quote = false,
            _ => {}
        }
    }
    state
}

fn parse_reply_segments(text: &str) -> Vec<ReplySegment> {
    let mut segments = Vec::new();
    let mut buffer = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0usize;

    while index < chars.len() {
        if chars[index] == '@' && is_mention_prefix_boundary(&chars, index) {
            if let Some((target, next_index)) = parse_at_segment(&chars, index) {
                push_text_segment(&mut segments, &mut buffer);
                segments.push(ReplySegment::Message(Message::At(AtTargetMessage {
                    target: Some(target),
                })));
                index = next_index;
                continue;
            }
        }

        if chars[index] == '[' {
            if let Some((segment, next_index)) = parse_bracket_segment(&chars, index) {
                push_text_segment(&mut segments, &mut buffer);
                segments.push(segment);
                index = next_index;
                continue;
            }
        }

        buffer.push(chars[index]);
        index += 1;
    }

    push_text_segment(&mut segments, &mut buffer);
    merge_adjacent_text_segments(segments)
}

fn push_text_segment(segments: &mut Vec<ReplySegment>, buffer: &mut String) {
    if !buffer.is_empty() {
        segments.push(ReplySegment::Text(std::mem::take(buffer)));
    }
}

fn merge_adjacent_text_segments(segments: Vec<ReplySegment>) -> Vec<ReplySegment> {
    let mut merged = Vec::new();
    for segment in segments {
        match segment {
            ReplySegment::Text(text) => {
                if let Some(ReplySegment::Text(last)) = merged.last_mut() {
                    last.push_str(&text);
                } else {
                    merged.push(ReplySegment::Text(text));
                }
            }
            other => merged.push(other),
        }
    }
    merged
}

fn is_mention_prefix_boundary(chars: &[char], index: usize) -> bool {
    if index == 0 {
        return true;
    }

    !chars[index - 1].is_ascii_alphanumeric()
}

fn parse_at_segment(chars: &[char], start: usize) -> Option<(String, usize)> {
    let mut end = start + 1;
    while end < chars.len() && chars[end].is_ascii_digit() {
        end += 1;
    }

    if end == start + 1 {
        return None;
    }

    if end < chars.len() {
        let boundary = chars[end];
        if !boundary.is_whitespace()
            && !matches!(
                boundary,
                ',' | '，' | '。' | ':' | '：' | '!' | '！' | '?' | '？' | ')' | '）' | ']' | '】'
            )
        {
            return None;
        }
    }

    Some((chars[start + 1..end].iter().collect(), end))
}

fn parse_bracket_segment(chars: &[char], start: usize) -> Option<(ReplySegment, usize)> {
    let mut end = start + 1;
    while end < chars.len() {
        if chars[end] == ']' {
            let inner: String = chars[start + 1..end].iter().collect();
            let inner = inner.trim();
            if let Some(control) = parse_bracket_control(inner) {
                return Some((control, end + 1));
            }
            if let Some(message) = parse_bracket_message(inner) {
                return Some((ReplySegment::Message(message), end + 1));
            }
            return None;
        }
        end += 1;
    }
    None
}

fn parse_bracket_message(inner: &str) -> Option<Message> {
    if let Some(value) = inner.strip_prefix("Reply message_id=") {
        let message_id = value.trim().parse::<i64>().ok()?;
        return Some(Message::Reply(ReplyMessage {
            id: message_id,
            message_source: None,
        }));
    }

    if let Some(value) = inner.strip_prefix("Image path=") {
        let path = parse_tag_value(value)?;
        return Some(Message::Image(ImageMessage {
            path: Some(path),
            ..ImageMessage::default()
        }));
    }

    if let Some(value) = inner.strip_prefix("Image url=") {
        let url = parse_tag_value(value)?;
        return Some(Message::Image(ImageMessage {
            url: Some(url),
            ..ImageMessage::default()
        }));
    }

    None
}

fn parse_bracket_control(inner: &str) -> Option<ReplySegment> {
    if inner.eq_ignore_ascii_case("no reply") {
        return Some(ReplySegment::NoReply);
    }
    None
}

fn parse_tag_value(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    if trimmed.len() >= 2 {
        let quoted = trimmed
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .or_else(|| {
                trimmed
                    .strip_prefix('\'')
                    .and_then(|value| value.strip_suffix('\''))
            });
        if let Some(value) = quoted {
            let inner = value.trim();
            if !inner.is_empty() {
                return Some(inner.to_string());
            }
        }
    }

    Some(trimmed.to_string())
}

pub async fn spawn(
    manager: &AgentManager,
    agent: AgentConfig,
    config: QqChatAgentConfig,
    connections: Vec<ConnectionConfig>,
    on_finish: super::OnFinishShared,
    task_runtime: Option<Arc<dyn AgentTaskRuntime>>,
) -> Result<JoinHandle<()>> {
    let llm_refs = load_llm_refs()?;
    let bot_connection = find_connection(&connections, &config.ims_bot_adapter_connection_id)?;
    let ConnectionKind::BotAdapter(ims_bot_adapter_connection) = &bot_connection.kind else {
        return Err(Error::ValidationError(format!(
            "connection '{}' is not a bot adapter connection",
            bot_connection.name
        )));
    };
    let ims_bot_adapter_connection = parse_ims_bot_adapter_connection(ims_bot_adapter_connection)?;

    let llm_config = resolve_llm_service_config(
        config.llm_ref_id.as_deref(),
        config.llm.as_ref(),
        &llm_refs,
        &agent.name,
    )?;
    let llm = build_llm_model(&llm_config);
    let intent_llm_config = resolve_llm_service_config(
        config
            .intent_llm_ref_id
            .as_deref()
            .or(config.llm_ref_id.as_deref()),
        config.llm.as_ref(),
        &llm_refs,
        &agent.name,
    )?;
    let intent_llm = build_llm_model(&intent_llm_config);
    let math_programming_llm_config = resolve_llm_service_config(
        config
            .math_programming_llm_ref_id
            .as_deref()
            .or(config.llm_ref_id.as_deref()),
        config.llm.as_ref(),
        &llm_refs,
        &agent.name,
    )?;
    let math_programming_llm = build_llm_model(&math_programming_llm_config);
    let embedding_model = if let Some(model_ref_id) = config.embedding_model_ref_id.as_deref() {
        let model_name =
            resolve_local_embedding_model_name(Some(model_ref_id), &llm_refs, &agent.name)?;
        match model_name {
            Some(_) => Some(
                RuntimeEmbeddingModelManager::shared()
                    .get_or_create_embedding_model(model_ref_id)
                    .await?,
            ),
            None => None,
        }
    } else {
        config.embedding.as_ref().map(build_embedding_model)
    };
    let tavily = build_tavily_ref(Some(&config.tavily_connection_id), &connections)?
        .ok_or_else(|| Error::ValidationError("missing tavily connection".to_string()))?;
    let object_storage = build_s3_ref(config.rustfs_connection_id.as_deref(), &connections).await?;
    let mysql_ref = build_mysql_ref(config.mysql_connection_id.as_deref(), &connections).await?;
    let weaviate_image_ref = tokio::task::block_in_place(|| {
        build_weaviate_ref(
            config.weaviate_image_connection_id.as_deref(),
            &connections,
            true,
        )
    })?;
    let tool_definitions = build_enabled_tool_definitions(&agent.tools)?;

    if let Some(ref mysql) = mysql_ref {
        register_mysql_ref(mysql.clone());
    }

    let service = Arc::new(QqChatAgentService::new(QqChatAgentServiceConfig {
        agent_id: agent.id.clone(),
        qq_chat_config: config.clone(),
        node_id: format!("service_agent_{}", agent.id),
        bot_name: if config.bot_name.trim().is_empty() {
            agent.name.clone()
        } else {
            config.bot_name.clone()
        },
        system_prompt: config.system_prompt.clone(),
        cache: Arc::new(OpenAIMessageSessionCacheRef::new(format!(
            "service_agent_cache_{}",
            agent.id
        ))),
        session: Arc::new(SessionStateRef::new(format!(
            "service_agent_session_{}",
            agent.id
        ))),
        llm,
        intent_llm,
        math_programming_llm,
        main_llm_display_name: resolve_llm_ref_display_name(
            config.llm_ref_id.as_deref(),
            &llm_refs,
            &llm_config.model_name,
        ),
        intent_llm_display_name: resolve_llm_ref_display_name(
            config
                .intent_llm_ref_id
                .as_deref()
                .or(config.llm_ref_id.as_deref()),
            &llm_refs,
            &intent_llm_config.model_name,
        ),
        math_programming_llm_display_name: resolve_llm_ref_display_name(
            config
                .math_programming_llm_ref_id
                .as_deref()
                .or(config.llm_ref_id.as_deref()),
            &llm_refs,
            &math_programming_llm_config.model_name,
        ),
        mysql_ref,
        weaviate_image_ref,
        embedding_model,
        tavily,
        s3_ref: object_storage.clone(),
        max_message_length: config.max_message_length,
        compact_context_length: config.compact_context_length,
        reply_batch_builder: Some(build_reply_batch_builder()),
        default_tools_enabled: config.default_tools_enabled.clone(),
        shared_inputs: Vec::<FunctionPortDef>::new(),
        tool_definitions,
        shared_runtime_values: HashMap::new(),
        task_runtime,
    })?);

    let adapter = build_ims_bot_adapter(&ims_bot_adapter_connection, object_storage).await;

    let pool = WorkerPool::new(config.event_handler_threads.unwrap_or(8), 256);

    {
        let service = Arc::clone(&service);
        let adapter_for_handler = adapter.clone();
        let pool_for_handler = pool.clone();
        let handler: EventHandler = Arc::new(move |event| {
            let service = Arc::clone(&service);
            let adapter = adapter_for_handler.clone();
            let event = event.clone();
            let pool = pool_for_handler.clone();
            Box::pin(async move {
                let time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
                pool.submit(move || {
                    if let Err(err) = service.handle_event(&event, &adapter, &time) {
                        error!("[service][qq_agent] failed to handle message event: {err}");
                    }
                });
            })
        });
        adapter.lock().await.register_event_handler(handler);
    }

    let manager = manager.clone();
    let agent_id = agent.id.clone();
    let agent_name = agent.name.clone();
    Ok(tokio::spawn(async move {
        info!("[service] starting QQ chat agent '{}'", agent_name);
        let (success, error_msg) = match BotAdapter::start(adapter).await {
            Ok(()) => {
                info!("[service] QQ chat agent '{}' stopped", agent_name);
                manager.update_state(
                    &agent_id,
                    AgentRuntimeState {
                        instance_id: None,
                        status: AgentRuntimeStatus::Stopped,
                        started_at: None,
                        last_error: None,
                    },
                );
                (true, None)
            }
            Err(err) => {
                error!(
                    "[service] QQ chat agent '{}' exited with error: {}",
                    agent_name, err
                );
                let msg = err.to_string();
                manager.update_state(
                    &agent_id,
                    AgentRuntimeState {
                        instance_id: None,
                        status: AgentRuntimeStatus::Error,
                        started_at: None,
                        last_error: Some(msg.clone()),
                    },
                );
                (false, Some(msg))
            }
        };
        pool.wait_idle();
        if let Some(cb) = on_finish.lock().unwrap().take() {
            cb(success, error_msg);
        }
    }))
}

fn resolve_llm_ref_display_name(
    llm_ref_id: Option<&str>,
    llm_refs: &[LlmRefConfig],
    fallback_model_name: &str,
) -> String {
    llm_ref_id
        .and_then(|id| llm_refs.iter().find(|item| item.id == id))
        .map(|item| item.name.clone())
        .unwrap_or_else(|| fallback_model_name.to_string())
}

pub fn build_enabled_tool_definitions(
    tools: &[AgentToolConfig],
) -> Result<Vec<BrainToolDefinition>> {
    let mut definitions = Vec::new();
    for tool in tools.iter().filter(|tool| tool.enabled) {
        match &tool.tool_type {
            AgentToolType::NodeGraph(config) => {
                definitions.push(build_node_graph_tool_definition(tool, config)?);
            }
        }
    }
    Ok(definitions)
}

fn build_node_graph_tool_definition(
    tool: &AgentToolConfig,
    config: &NodeGraphToolConfig,
) -> Result<BrainToolDefinition> {
    let (mut graph, parameters, outputs) = match config {
        NodeGraphToolConfig::FilePath {
            path,
            parameters,
            outputs,
        } => (
            load_graph_from_path(PathBuf::from(path))?,
            parameters.clone(),
            outputs.clone(),
        ),
        NodeGraphToolConfig::WorkflowSet {
            name,
            parameters,
            outputs,
        } => (
            load_graph_from_path(PathBuf::from("workflow_set").join(format!("{name}.json")))?,
            parameters.clone(),
            outputs.clone(),
        ),
        NodeGraphToolConfig::InlineGraph {
            graph,
            parameters,
            outputs,
        } => (graph.clone(), parameters.clone(), outputs.clone()),
    };

    sync_root_graph_io(&mut graph);
    validate_tool_graph_contract(tool, &graph, &parameters, &outputs)?;
    let subgraph = root_graph_to_tool_subgraph(&graph);

    Ok(BrainToolDefinition {
        id: tool.id.clone(),
        name: tool.name.clone(),
        description: tool.description.clone(),
        parameters,
        outputs,
        subgraph,
    })
}

fn load_graph_from_path(
    path: PathBuf,
) -> Result<zihuan_graph_engine::graph_io::NodeGraphDefinition> {
    if !path.exists() {
        return Err(Error::ValidationError(format!(
            "tool graph file not found: {}",
            path.display()
        )));
    }
    let loaded = zihuan_graph_engine::load_graph_definition_from_json_with_migration(&path)?;
    Ok(loaded.graph)
}

fn validate_tool_graph_contract(
    tool: &AgentToolConfig,
    graph: &zihuan_graph_engine::graph_io::NodeGraphDefinition,
    parameters: &[zihuan_llm::brain_tool::ToolParamDef],
    outputs: &[FunctionPortDef],
) -> Result<()> {
    if graph.graph_inputs.is_empty() {
        return Err(Error::ValidationError(format!(
            "agent tool '{}' 引用的节点图未定义输入列表",
            tool.name
        )));
    }
    if graph.graph_outputs.is_empty() {
        return Err(Error::ValidationError(format!(
            "agent tool '{}' 引用的节点图未定义输出列表",
            tool.name
        )));
    }
    if outputs.is_empty() {
        return Err(Error::ValidationError(format!(
            "agent tool '{}' 未定义 outputs，必须与节点图输出匹配",
            tool.name
        )));
    }

    for port in &graph.graph_inputs {
        validate_tool_graph_input_port(tool, port)?;
    }

    if !same_param_signature(parameters, &graph.graph_inputs) {
        return Err(Error::ValidationError(format!(
            "agent tool '{}' 的 parameters 与节点图输入定义不匹配",
            tool.name
        )));
    }
    if !same_port_signature(outputs, &graph.graph_outputs) {
        return Err(Error::ValidationError(format!(
            "agent tool '{}' 的 outputs 与节点图输出定义不匹配",
            tool.name
        )));
    }

    Ok(())
}

fn validate_tool_graph_input_port(tool: &AgentToolConfig, port: &FunctionPortDef) -> Result<()> {
    if let Some(expected_type) = reserved_tool_graph_input_type(&port.name) {
        if port.data_type != expected_type {
            return Err(Error::ValidationError(format!(
                "agent tool '{}' 的保留输入 '{}' 类型不匹配：期望 {}，实际为 {}",
                tool.name, port.name, expected_type, port.data_type
            )));
        }
        return Ok(());
    }

    if matches!(
        port.data_type,
        DataType::Integer | DataType::Float | DataType::String | DataType::Boolean
    ) {
        return Ok(());
    }

    Err(Error::ValidationError(format!(
        "agent tool '{}' 的节点图输入 '{}' 类型必须是基础类型 int/float/string/boolean，或受支持的保留运行时输入；实际为 {}",
        tool.name, port.name, port.data_type
    )))
}

fn reserved_tool_graph_input_type(name: &str) -> Option<DataType> {
    let trimmed = name.trim();
    for owner_type in [
        "brain",
        QQ_AGENT_TOOL_OWNER_TYPE,
        LEGACY_QQ_AGENT_TOOL_OWNER_TYPE,
    ] {
        for port in fixed_tool_runtime_inputs(owner_type) {
            if port.name == trimmed {
                return Some(port.data_type);
            }
        }
    }
    None
}

fn same_param_signature(
    parameters: &[zihuan_llm::brain_tool::ToolParamDef],
    inputs: &[FunctionPortDef],
) -> bool {
    let exposed_inputs = inputs
        .iter()
        .filter(|input| reserved_tool_graph_input_type(&input.name).is_none())
        .collect::<Vec<_>>();

    parameters.len() == exposed_inputs.len()
        && parameters.iter().zip(exposed_inputs).all(|(param, input)| {
            param.name.trim() == input.name.trim() && param.data_type == input.data_type
        })
}

fn same_port_signature(left: &[FunctionPortDef], right: &[FunctionPortDef]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(a, b)| a.name.trim() == b.name.trim() && a.data_type == b.data_type)
}
