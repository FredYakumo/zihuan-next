use std::collections::HashMap;
use std::sync::Arc;

use super::inference::{InferenceToolContext, InferenceToolProvider};
use super::qq_chat_agent_core::{
    build_info_brain_tools, QqAgentReplyBatchBuilder, QqAgentReplyBuildRequest,
    QqAgentReplyBuildResult, QqChatAgentService, QqChatAgentServiceConfig,
};
use super::{AgentManager, AgentRuntimeState, AgentRuntimeStatus};
use crate::agent::qq_chat_agent_inbox::{QqChatAgentInbox, QqChatAgentSupervisorEvent};
use crate::agent::tool_definitions::build_enabled_tool_definitions;
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
use log::{error, info, warn};
use model_inference::nn::embedding::embedding_runtime_manager::RuntimeEmbeddingModelManager;
use model_inference::system_config::{load_llm_refs, AgentConfig, LlmRefConfig};
use storage_handler::{
    build_mysql_ref, build_s3_ref, build_tavily_ref, build_weaviate_ref, find_connection,
    ConnectionConfig, ConnectionKind,
};
use tokio::task::JoinHandle;
use zihuan_agent::brain::BrainTool;
use zihuan_core::agent_config::QqChatAgentConfig;
use zihuan_core::data_refs::MySqlConfig;
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::embedding_base::EmbeddingBase;
use zihuan_core::llm::OpenAIMessage;
use zihuan_core::rag::TavilyRef;
use zihuan_core::runtime::block_async;
use zihuan_core::task_context::AgentTaskRuntime;
use zihuan_core::weaviate::WeaviateRef;
use zihuan_graph_engine::brain_tool_spec::BrainToolDefinition;
use zihuan_graph_engine::data_value::{OpenAIMessageSessionCacheRef, SessionStateRef};
use zihuan_graph_engine::function_graph::FunctionPortDef;
use zihuan_graph_engine::message_restore::register_mysql_ref;

const FORWARD_SPLIT_PREFERRED_SEPARATORS: [char; 14] = [
    '\n', '。', '！', '？', '；', '：', '.', '!', '?', ';', ':', '，', ',', ' ',
];

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
                    if pending_reply.is_some() {
                        let text_batches =
                            build_plain_text_batches_from_text(&text, request.max_message_length);
                        if let Some(reply) = pending_reply.take() {
                            append_reply_to_text_batches(&mut batches, text_batches, reply);
                        } else {
                            batches.extend(text_batches);
                        }
                    } else if let Some(forward) = build_forward_from_text(
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
                if let Message::PlainText(pt) = &mut batch[i + 1] {
                    if !pt.text.starts_with(' ') {
                        pt.text.insert(0, ' ');
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

fn build_plain_text_batches_from_text(text: &str, max_chars: usize) -> Vec<Vec<Message>> {
    split_plain_text_for_forward(text, max_chars)
        .into_iter()
        .map(|chunk| vec![Message::PlainText(PlainTextMessage { text: chunk })])
        .collect()
}

fn append_reply_to_text_batches(
    batches: &mut Vec<Vec<Message>>,
    mut text_batches: Vec<Vec<Message>>,
    reply: ReplyMessage,
) {
    if let Some(first_batch) = text_batches.first_mut() {
        first_batch.insert(0, Message::Reply(reply));
    }
    batches.extend(text_batches);
}

fn attach_reply_to_first_batch(batches: &mut Vec<Vec<Message>>, reply: ReplyMessage) {
    if let Some(first_batch) = batches
        .iter_mut()
        .find(|batch| !matches!(batch.as_slice(), [Message::Forward(_)]))
    {
        first_batch.insert(0, Message::Reply(reply));
    } else {
        warn!(
            "dropping reply marker because all outbound batches are forward messages and QQ does not support reply+forward in one batch"
        );
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

#[derive(Clone)]
struct QqLoadedInferenceResources {
    bot_name: String,
    default_tools_enabled: HashMap<String, bool>,
    tavily_ref: Option<Arc<TavilyRef>>,
    mysql_ref: Option<Arc<MySqlConfig>>,
    weaviate_image_ref: Option<Arc<WeaviateRef>>,
    embedding_model: Option<Arc<dyn EmbeddingBase>>,
}

pub struct QqInferenceToolProvider {
    resources: QqLoadedInferenceResources,
    tool_definitions: Vec<BrainToolDefinition>,
}

impl InferenceToolProvider for QqInferenceToolProvider {
    fn augment_messages(&self, messages: &mut Vec<OpenAIMessage>, _context: &InferenceToolContext) {
        messages.insert(
            0,
            OpenAIMessage::system(format!(
                "你是 {}。请保持回答简洁、友好、准确；当可调用工具时优先使用工具获取事实。",
                self.resources.bot_name
            )),
        );
    }

    fn build_default_tools(&self, context: &InferenceToolContext) -> Vec<Box<dyn BrainTool>> {
        build_info_brain_tools(
            &self.resources.default_tools_enabled,
            self.resources.tavily_ref.clone(),
            self.resources.mysql_ref.clone(),
            self.resources.weaviate_image_ref.clone(),
            self.resources.embedding_model.clone(),
            context.last_user_text.clone(),
        )
    }

    fn tool_definitions(&self) -> Vec<BrainToolDefinition> {
        self.tool_definitions.clone()
    }
}

pub fn load_inference_tool_provider(
    agent: &AgentConfig,
    config: &QqChatAgentConfig,
    connections: &[ConnectionConfig],
) -> Result<Arc<dyn InferenceToolProvider>> {
    Ok(Arc::new(QqInferenceToolProvider {
        resources: load_qq_resources(agent, config, connections)?,
        tool_definitions: build_enabled_tool_definitions(&agent.tools)?,
    }))
}

fn load_qq_resources(
    agent: &AgentConfig,
    config: &QqChatAgentConfig,
    connections: &[ConnectionConfig],
) -> Result<QqLoadedInferenceResources> {
    let tavily_ref = build_tavily_ref(
        if config.tavily_connection_id.trim().is_empty() {
            None
        } else {
            Some(config.tavily_connection_id.as_str())
        },
        connections,
    )
    .unwrap_or_else(|e| {
        warn!("[inference][qq_agent] tavily connection unavailable: {e}");
        None
    });

    let mysql_connection_id = config
        .mysql_connection_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let mysql_ref = block_async(build_mysql_ref(mysql_connection_id, connections)).map_err(|err| {
        let connection_label = mysql_connection_id.unwrap_or("<none>");
        Error::ValidationError(format!(
            "agent '{}' failed to initialize mysql dependency from mysql_connection_id='{}': {}",
            agent.name, connection_label, err
        ))
    })?;

    let weaviate_image_ref = tokio::task::block_in_place(|| {
        build_weaviate_ref(
            if config
                .weaviate_image_connection_id
                .as_deref()
                .map(str::trim)
                .unwrap_or("")
                .is_empty()
            {
                None
            } else {
                config.weaviate_image_connection_id.as_deref()
            },
            connections,
            true,
        )
    })
    .unwrap_or_else(|e| {
        warn!("[inference][qq_agent] weaviate image connection unavailable: {e}");
        None
    });

    let embedding_model = if let Some(model_ref_id) = config.embedding_model_ref_id.as_deref() {
        let llm_refs = model_inference::system_config::load_llm_refs().unwrap_or_default();
        match resolve_local_embedding_model_name(Some(model_ref_id), &llm_refs, &agent.name) {
            Ok(Some(_)) => block_async(
                RuntimeEmbeddingModelManager::shared().get_or_create_embedding_model(model_ref_id),
            )
            .ok(),
            Ok(None) => None,
            Err(err) => {
                warn!("[inference][qq_agent] embedding model ref unavailable: {err}");
                None
            }
        }
    } else {
        config.embedding.as_ref().map(build_embedding_model)
    };

    Ok(QqLoadedInferenceResources {
        bot_name: if config.bot_name.trim().is_empty() {
            agent.name.clone()
        } else {
            config.bot_name.clone()
        },
        default_tools_enabled: config.default_tools_enabled.clone(),
        tavily_ref,
        mysql_ref,
        weaviate_image_ref,
        embedding_model,
    })
}

/// Purpose: Bootstrap and launch a long-running QQ chat agent instance.
///
/// Resolves all runtime dependencies (`llm`, `embedding_model`, `tavily`, `s3_ref`,
/// `mysql_ref`, `weaviate_image_ref`), wires the IMS bot adapter event handler
/// through an inbox queue, then spawns a background task that runs the
/// `BotAdapter::start` loop until exit.
///
/// Called when the service layer starts an agent whose type is QQ chat —
/// typically from `AgentManager::start_agent` after validating the agent config.
///
/// Call chain:
///   `AgentManager::start_agent` → `QqChatAgent::spawn`
///     → build deps → register `EventHandler` on bot adapter
///     → `tokio::spawn`(`BotAdapter::start`) → `handle_event` per incoming message
///     → `on_finish` callback on exit
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

    let llm_config =
        resolve_llm_service_config(config.llm_ref_id.as_deref(), &llm_refs, &agent.name)?;
    let llm = build_llm_model(&llm_config)?;
    let intent_llm_config = resolve_llm_service_config(
        config
            .intent_llm_ref_id
            .as_deref()
            .or(config.llm_ref_id.as_deref()),
        &llm_refs,
        &agent.name,
    )?;
    let intent_llm = build_llm_model(&intent_llm_config)?;
    let math_programming_llm_config = resolve_llm_service_config(
        config
            .math_programming_llm_ref_id
            .as_deref()
            .or(config.llm_ref_id.as_deref()),
        &llm_refs,
        &agent.name,
    )?;
    let math_programming_llm = build_llm_model(&math_programming_llm_config)?;
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
    let redis_ref = resolve_inbox_redis_ref(&connections)?;
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

    let inbox = QqChatAgentInbox::new(
        Arc::clone(&service),
        adapter.clone(),
        redis_ref,
        &agent.id,
        config.event_handler_threads,
    );

    {
        let inbox = inbox.clone();
        let handler: EventHandler = Arc::new(move |event| {
            let event = event.clone();
            let inbox = inbox.clone();
            Box::pin(async move {
                let time = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
                inbox.enqueue(event, time).await?;
                Ok(())
            })
        });
        adapter.lock().await.register_event_handler(handler);
    }

    let manager = manager.clone();
    let agent_id = agent.id.clone();
    let agent_name = agent.name.clone();
    Ok(tokio::spawn(async move {
        info!("[service] starting QQ chat agent '{}'", agent_name);
        let mut tasks = tokio::task::JoinSet::new();
        inbox.spawn_consumers(&mut tasks);
        tasks.spawn(async move {
            match BotAdapter::start(adapter).await {
                Ok(()) => QqChatAgentSupervisorEvent::AdapterFinished {
                    success: true,
                    error_msg: None,
                },
                Err(err) => QqChatAgentSupervisorEvent::AdapterFinished {
                    success: false,
                    error_msg: Some(err.to_string()),
                },
            }
        });

        let mut adapter_result: Option<(bool, Option<String>)> = None;
        while let Some(result) = tasks.join_next().await {
            match result {
                Ok(QqChatAgentSupervisorEvent::AdapterFinished { success, error_msg }) => {
                    adapter_result = Some((success, error_msg));
                    inbox.request_shutdown();
                }
                Ok(QqChatAgentSupervisorEvent::RedisConsumerFinished) => {
                    if adapter_result.is_none() {
                        warn!("[service][qq_agent] a Redis inbox consumer exited unexpectedly");
                    }
                }
                Ok(QqChatAgentSupervisorEvent::MemoryConsumerFinished) => {
                    if adapter_result.is_none() {
                        warn!("[service][qq_agent] a memory inbox consumer exited unexpectedly");
                    }
                }
                Err(err) => {
                    error!("[service][qq_agent] inbox task join failed: {err}");
                }
            }
        }
        let (success, error_msg) = adapter_result.unwrap_or_else(|| {
            (
                false,
                Some("QQ chat agent task set ended unexpectedly".to_string()),
            )
        });

        if success {
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
        } else {
            let msg = error_msg
                .clone()
                .unwrap_or_else(|| "QQ chat agent exited unexpectedly".to_string());
            error!(
                "[service] QQ chat agent '{}' exited with error: {}",
                agent_name, msg
            );
            manager.update_state(
                &agent_id,
                AgentRuntimeState {
                    instance_id: None,
                    status: AgentRuntimeStatus::Error,
                    started_at: None,
                    last_error: Some(msg.clone()),
                },
            );
        }
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

fn resolve_inbox_redis_ref(
    connections: &[ConnectionConfig],
) -> Result<Option<Arc<zihuan_graph_engine::data_value::RedisConfig>>> {
    let redis_connection_id = connections.iter().find_map(|connection| {
        if connection.enabled && matches!(connection.kind, ConnectionKind::Redis(_)) {
            Some(connection.id.as_str())
        } else {
            None
        }
    });
    storage_handler::build_redis_ref(redis_connection_id, connections)
}
