use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use log::{info, warn};
use serde::Deserialize;
use serde_json::Value;

use crate::agent::brain::{Brain, BrainTool, BrainStopReason};
use zihuan_bot_adapter::adapter::shared_from_handle;
use zihuan_bot_adapter::message_helpers::{
    get_bot_id, send_friend_batches, send_friend_progress_notification, send_friend_text,
    send_group_batches, send_group_progress_notification, send_group_text,
};
use zihuan_bot_adapter::models::event_model::MessageType;
use zihuan_bot_adapter::models::message::{AtTargetMessage, Message, MessageProp, PlainTextMessage};
use zihuan_core::error::{Error, Result};
use zihuan_core::runtime::block_async;
use zihuan_llm_types::tooling::FunctionTool;
use zihuan_llm_types::OpenAIMessage;
use zihuan_node::data_value::{
    OpenAIMessageSessionCacheRef, SessionClaim, SessionStateRef, TavilyRef, SESSION_CLAIM_CONTEXT,
};
use zihuan_node::{node_input, node_output, DataType, DataValue, Node, Port};

const LOG_PREFIX: &str = "[QqMessageAgentNode]";
const BUSY_REPLY: &str = "我还在思考中，你别急";
const FALLBACK_REPLY: &str = "对不起,我无法回复这条消息";
const MAX_REPLY_CHARS: usize = 250;

/// System prompt template (shared, private variant).
fn build_private_system_prompt(bot_name: &str, time: &str, sender_id: &str) -> String {
    format!(
        "你的角色是{bot_name}。现在时间是{time}，你的QQ好友{sender_id}向你发送了一条消息。\n\
         你可以选择调用相关工具来获取信息，并通过 reply_* 工具把特定 QQ 消息加入待发送列表。\n\
         最终请直接输出你想发送给对方的自然语言，不要输出 JSON、代码块或额外格式说明。\n\
         如果你调用了 reply_* 工具，这些工具加入的消息会先发送，你最后一条 assistant 自然语言回复会作为最后一条普通文本消息追加发送。\n\
         如果你决定这轮不回复，请调用 no_reply；调用后本轮不会发送任何 QQ 消息，但你仍然需要正常完成这一轮 assistant 收尾。\n\
         `reply_plain_text` 用于追加纯文本消息；`reply_at` 用于追加单独的 @ 消息；`reply_combine_text` 用于在同一次发送里组合 at 和文本片段。\n\
         对于超过250字的最终自然语言回复，系统会自动拆分发送。\n\
         当你决定调用工具时，请在工具 content 里用一句话说明你即将做什么（例如\"我将搜索关于xxx的信息\"）。"
    )
}

/// System prompt template (group variant).
fn build_group_system_prompt(bot_name: &str, time: &str, sender_id: &str) -> String {
    format!(
        "你的角色是{bot_name}。现在时间是{time}，你的QQ群友{sender_id}向你发送了一条消息。\n\
         你可以选择调用相关工具来获取信息，并通过 reply_* 工具把特定 QQ 消息加入待发送列表。\n\
         最终请直接输出你想发送到群里的自然语言，不要输出 JSON、代码块或额外格式说明。\n\
         如果你调用了 reply_* 工具，这些工具加入的消息会先发送，你最后一条 assistant 自然语言回复会作为最后一条普通文本消息追加发送。\n\
         如果你决定这轮不回复，请调用 no_reply；调用后本轮不会发送任何 QQ 消息，但你仍然需要正常完成这一轮 assistant 收尾。\n\
         `reply_plain_text` 用于追加纯文本消息；`reply_at` 用于追加单独的 @ 消息；`reply_combine_text` 用于在同一次发送里组合 at 和文本片段。\n\
         对于超过250字的最终自然语言回复，系统会自动拆分发送。\n\
         当你决定调用工具时，请在工具 content 里用一句话说明你即将做什么（例如\"我将搜索关于xxx的信息\"）。"
    )
}

fn load_history(cache: &Arc<OpenAIMessageSessionCacheRef>, sender_id: &str) -> Vec<OpenAIMessage> {
    block_async(cache.get_messages(sender_id)).unwrap_or_default()
}

fn save_history(
    cache: &Arc<OpenAIMessageSessionCacheRef>,
    sender_id: &str,
    messages: Vec<OpenAIMessage>,
) {
    if let Err(e) = block_async(cache.set_messages(sender_id, messages)) {
        warn!("{LOG_PREFIX} Failed to save history for {sender_id}: {e}");
    }
}

/// Try to claim a session slot. Returns `(claimed, claim_token)`.
fn try_claim_session(session: &Arc<SessionStateRef>, sender_id: &str) -> (bool, Option<u64>) {
    let (state, claimed) = block_async(session.try_claim(sender_id, None));

    if claimed {
        let claim_token = state.claim_token();
        if let (Ok(ctx), Some(token)) = (SESSION_CLAIM_CONTEXT.try_with(Arc::clone), claim_token) {
            ctx.register_claim(SessionClaim {
                session_ref: session.clone(),
                sender_id: sender_id.to_string(),
                claim_token: token,
            });
        }
        (true, claim_token)
    } else {
        (false, None)
    }
}

fn release_session(session: &Arc<SessionStateRef>, sender_id: &str, claim_token: Option<u64>) {
    if let Ok(ctx) = SESSION_CLAIM_CONTEXT.try_with(Arc::clone) {
        ctx.unregister_claim(&session.node_id, sender_id);
    }
    let released = block_async(session.release(sender_id, claim_token));
    info!("{LOG_PREFIX} Released session for {sender_id}: released={released}");
}

/// Extract the plain-text user message from `MessageEvent.message_list`.
fn extract_user_text(msg_list: &[Message], bot_id: &str) -> String {
    let msg_prop = MessageProp::from_messages(msg_list, Some(bot_id));
    let mut text = msg_prop.content.unwrap_or_default();
    if let Some(ref_cnt) = msg_prop.ref_content.as_deref() {
        if !ref_cnt.is_empty() {
            if !text.is_empty() {
                text.push_str("\n\n");
            }
            text.push_str("[引用内容]\n");
            text.push_str(ref_cnt);
        }
    }
    if text.trim().is_empty() {
        text = "(无文本内容，可能是仅@或回复)".to_string();
    }
    text
}

fn split_text_for_qq(content: &str) -> Vec<String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let chars: Vec<char> = trimmed.chars().collect();
    let mut start = 0;
    let mut chunks = Vec::new();
    while start < chars.len() {
        let end = (start + MAX_REPLY_CHARS).min(chars.len());
        chunks.push(chars[start..end].iter().collect());
        start = end;
    }
    chunks
}

fn plain_text_batches(content: &str) -> Vec<Vec<Message>> {
    split_text_for_qq(content)
        .into_iter()
        .map(|chunk| vec![Message::PlainText(PlainTextMessage { text: chunk })])
        .collect()
}

type SharedPendingReplyState = Arc<Mutex<PendingReplyState>>;

#[derive(Debug, Default, Clone)]
struct PendingReplyState {
    batches: Vec<Vec<Message>>,
    suppress_send: bool,
}

impl PendingReplyState {
    fn append_batches(&mut self, batches: Vec<Vec<Message>>) -> Result<usize> {
        if self.suppress_send {
            return Ok(0);
        }
        if batches.iter().any(Vec::is_empty) {
            return Err(Error::ValidationError(
                "QQ message batch must not be empty".to_string(),
            ));
        }
        let count = batches.len();
        self.batches.extend(batches);
        Ok(count)
    }

    fn append_batch(&mut self, batch: Vec<Message>) -> Result<()> {
        self.append_batches(vec![batch]).map(|_| ())
    }

    fn mark_no_reply(&mut self) {
        self.suppress_send = true;
        self.batches.clear();
    }
}

fn lock_pending_state(state: &SharedPendingReplyState) -> Result<std::sync::MutexGuard<'_, PendingReplyState>> {
    state
        .lock()
        .map_err(|_| Error::ValidationError("pending reply state lock poisoned".to_string()))
}

#[derive(Debug)]
struct StaticFunctionToolSpec {
    name: &'static str,
    description: &'static str,
    parameters: Value,
}

impl FunctionTool for StaticFunctionToolSpec {
    fn name(&self) -> &str {
        self.name
    }

    fn description(&self) -> &str {
        self.description
    }

    fn parameters(&self) -> Value {
        self.parameters.clone()
    }

    fn call(&self, _arguments: Value) -> Result<Value> {
        Ok(Value::Null)
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "message_type", rename_all = "snake_case")]
enum CombineTextItem {
    PlainText { content: String },
    At { target: String },
}

fn build_combine_text_batch(arguments: &Value, is_group: bool) -> Result<Vec<Message>> {
    let content_list = arguments
        .get("content_list")
        .cloned()
        .ok_or_else(|| Error::ValidationError("content_list is required".to_string()))?;
    let items: Vec<CombineTextItem> = serde_json::from_value(content_list)?;
    if items.is_empty() {
        return Err(Error::ValidationError(
            "combine_text.content_list must not be empty".to_string(),
        ));
    }

    let mut contains_substantive_text = false;
    let mut messages = Vec::with_capacity(items.len());

    for item in items {
        match item {
            CombineTextItem::PlainText { content } => {
                if content.is_empty() {
                    return Err(Error::ValidationError(
                        "combine_text plain_text.content must not be empty".to_string(),
                    ));
                }
                contains_substantive_text |= !content.trim().is_empty();
                messages.push(Message::PlainText(PlainTextMessage { text: content }));
            }
            CombineTextItem::At { target } => {
                if !is_group {
                    return Err(Error::ValidationError(
                        "reply_combine_text only supports at segments in group chat".to_string(),
                    ));
                }
                let target = target.trim().to_string();
                if target.is_empty() {
                    return Err(Error::ValidationError(
                        "combine_text at.target must not be empty".to_string(),
                    ));
                }
                messages.push(Message::At(AtTargetMessage {
                    target: Some(target),
                }));
            }
        }
    }

    if !contains_substantive_text {
        return Err(Error::ValidationError(
            "combine_text must contain at least one substantive plain_text item".to_string(),
        ));
    }

    Ok(messages)
}

struct TavilyBrainTool {
    tavily_ref: Arc<TavilyRef>,
    adapter: zihuan_bot_adapter::adapter::SharedBotAdapter,
    target_id: String,
    mention_target_id: Option<String>,
    is_group: bool,
}

impl BrainTool for TavilyBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: "web_search",
            description: "使用 Tavily 搜索引擎在互联网上搜索信息，返回相关网页的标题、链接和内容摘要",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "搜索关键词或问题" },
                    "search_count": { "type": "integer", "description": "搜索结果数量，通常为 3，最大 10" }
                },
                "required": ["query", "search_count"]
            }),
        })
    }

    fn execute(&self, call_content: &str, arguments: &Value) -> String {
        if self.is_group {
            if let Some(mid) = &self.mention_target_id {
                send_group_progress_notification(&self.adapter, &self.target_id, mid, call_content);
            }
        } else {
            send_friend_progress_notification(&self.adapter, &self.target_id, call_content);
        }

        let query = arguments
            .get("query")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let search_count = arguments
            .get("search_count")
            .and_then(|v| v.as_i64())
            .unwrap_or(3);

        if query.trim().is_empty() {
            return serde_json::json!({"results": []}).to_string();
        }

        let results = self.tavily_ref.search(&query, search_count);
        match results {
            Ok(items) => serde_json::json!({ "results": items }).to_string(),
            Err(e) => {
                warn!("{LOG_PREFIX} Tavily search failed: {e}");
                serde_json::json!({"results": [], "error": e.to_string()}).to_string()
            }
        }
    }
}

struct ReplyPlainTextBrainTool {
    pending_reply_state: SharedPendingReplyState,
}

impl BrainTool for ReplyPlainTextBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: "reply_plain_text",
            description: "向本轮待发送的 QQ 消息列表追加纯文本消息。长文本会自动拆成多条发送。",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "content": { "type": "string", "description": "要发送的文本内容" }
                },
                "required": ["content"]
            }),
        })
    }

    fn execute(&self, _call_content: &str, arguments: &Value) -> String {
        let result = (|| -> Result<Value> {
            let content = arguments
                .get("content")
                .and_then(|v| v.as_str())
                .ok_or_else(|| Error::ValidationError("content is required".to_string()))?;
            let batches = plain_text_batches(content);
            if batches.is_empty() {
                return Err(Error::ValidationError(
                    "reply_plain_text.content must not be blank".to_string(),
                ));
            }

            let appended = {
                let mut state = lock_pending_state(&self.pending_reply_state)?;
                state.append_batches(batches)?
            };

            Ok(serde_json::json!({
                "ok": true,
                "appended_batches": appended
            }))
        })();

        match result {
            Ok(value) => value.to_string(),
            Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}).to_string(),
        }
    }
}

struct ReplyAtBrainTool {
    pending_reply_state: SharedPendingReplyState,
    is_group: bool,
}

impl BrainTool for ReplyAtBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: "reply_at",
            description: "向本轮待发送的 QQ 消息列表追加单独的 @ 消息。仅群聊可用。",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "target": { "type": "string", "description": "要 @ 的 QQ 号" }
                },
                "required": ["target"]
            }),
        })
    }

    fn execute(&self, _call_content: &str, arguments: &Value) -> String {
        let result = (|| -> Result<Value> {
            if !self.is_group {
                return Err(Error::ValidationError(
                    "reply_at can only be used in group chat".to_string(),
                ));
            }

            let target = arguments
                .get("target")
                .and_then(|v| v.as_str())
                .ok_or_else(|| Error::ValidationError("target is required".to_string()))?
                .trim()
                .to_string();

            if target.is_empty() {
                return Err(Error::ValidationError(
                    "reply_at.target must not be empty".to_string(),
                ));
            }

            {
                let mut state = lock_pending_state(&self.pending_reply_state)?;
                state.append_batch(vec![Message::At(AtTargetMessage {
                    target: Some(target.clone()),
                })])?;
            }

            Ok(serde_json::json!({
                "ok": true,
                "target": target
            }))
        })();

        match result {
            Ok(value) => value.to_string(),
            Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}).to_string(),
        }
    }
}

struct ReplyCombineTextBrainTool {
    pending_reply_state: SharedPendingReplyState,
    is_group: bool,
}

impl BrainTool for ReplyCombineTextBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: "reply_combine_text",
            description: "向本轮待发送的 QQ 消息列表追加一次组合发送的消息段，可混合 at 和 plain_text。",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "content_list": {
                        "type": "array",
                        "description": "同一次发送中的消息段列表，每个元素的 message_type 只能是 plain_text 或 at",
                        "items": {
                            "type": "object",
                            "properties": {
                                "message_type": { "type": "string", "enum": ["plain_text", "at"] },
                                "content": { "type": "string" },
                                "target": { "type": "string" }
                            },
                            "required": ["message_type"]
                        }
                    }
                },
                "required": ["content_list"]
            }),
        })
    }

    fn execute(&self, _call_content: &str, arguments: &Value) -> String {
        let result = (|| -> Result<Value> {
            let batch = build_combine_text_batch(arguments, self.is_group)?;
            {
                let mut state = lock_pending_state(&self.pending_reply_state)?;
                state.append_batch(batch)?;
            }

            Ok(serde_json::json!({
                "ok": true,
                "appended_batches": 1
            }))
        })();

        match result {
            Ok(value) => value.to_string(),
            Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}).to_string(),
        }
    }
}

struct NoReplyBrainTool {
    pending_reply_state: SharedPendingReplyState,
}

impl BrainTool for NoReplyBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: "no_reply",
            description: "标记本轮不发送任何 QQ 回复消息。调用后会清空已积累的待发送消息。",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        })
    }

    fn execute(&self, _call_content: &str, _arguments: &Value) -> String {
        let result = (|| -> Result<Value> {
            let mut state = lock_pending_state(&self.pending_reply_state)?;
            state.mark_no_reply();
            Ok(serde_json::json!({
                "ok": true,
                "suppressed": true
            }))
        })();

        match result {
            Ok(value) => value.to_string(),
            Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}).to_string(),
        }
    }
}

pub struct QqMessageAgentNode {
    id: String,
    name: String,
}

impl QqMessageAgentNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }

    fn wrap_err(&self, msg: impl Into<String>) -> Error {
        Error::ValidationError(format!("[NODE_ERROR:{}] {}", self.id, msg.into()))
    }

    fn handle(
        &self,
        event: &zihuan_bot_adapter::models::MessageEvent,
        adapter: &zihuan_bot_adapter::adapter::SharedBotAdapter,
        time: &str,
        bot_name: &str,
        cache: &Arc<OpenAIMessageSessionCacheRef>,
        session: &Arc<SessionStateRef>,
        llm: &Arc<dyn zihuan_llm_types::llm_base::LLMBase>,
        tavily: &Arc<TavilyRef>,
    ) -> Result<()> {
        let is_group = event.message_type == MessageType::Group;
        let sender_id = event.sender.user_id.to_string();
        let target_id = if is_group {
            event
                .group_id
                .ok_or_else(|| self.wrap_err("group_id missing on group message"))?
                .to_string()
        } else {
            sender_id.clone()
        };

        info!(
            "{LOG_PREFIX} Handling {} message: sender={} target={}",
            if is_group { "group" } else { "private" },
            sender_id,
            target_id
        );

        if is_group {
            let bot_id = get_bot_id(adapter);
            let msg_prop = MessageProp::from_messages_with_bot_name(
                &event.message_list,
                Some(&bot_id),
                Some(bot_name),
            );
            if !msg_prop.is_at_me {
                info!(
                    "{LOG_PREFIX} Skipping group message without @ mention: sender={} target={}",
                    sender_id,
                    target_id
                );
                return Ok(());
            }
        }

        let (claimed, claim_token) = try_claim_session(session, &sender_id);
        if !claimed {
            info!("{LOG_PREFIX} Session busy for {sender_id}");
            if !is_group {
                send_friend_text(adapter, &target_id, BUSY_REPLY);
            }
            return Ok(());
        }

        let result = self.handle_claimed(
            event,
            adapter,
            time,
            bot_name,
            cache,
            session,
            llm,
            tavily,
            &sender_id,
            &target_id,
            is_group,
        );

        release_session(session, &sender_id, claim_token);
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_claimed(
        &self,
        event: &zihuan_bot_adapter::models::MessageEvent,
        adapter: &zihuan_bot_adapter::adapter::SharedBotAdapter,
        time: &str,
        bot_name: &str,
        cache: &Arc<OpenAIMessageSessionCacheRef>,
        _session: &Arc<SessionStateRef>,
        llm: &Arc<dyn zihuan_llm_types::llm_base::LLMBase>,
        tavily: &Arc<TavilyRef>,
        sender_id: &str,
        target_id: &str,
        is_group: bool,
    ) -> Result<()> {
        let bot_id = get_bot_id(adapter);
        let user_text = extract_user_text(&event.message_list, &bot_id);
        let user_msg = OpenAIMessage::user(user_text);

        let mut history = load_history(cache, sender_id);

        let system_prompt = if is_group {
            build_group_system_prompt(bot_name, time, sender_id)
        } else {
            build_private_system_prompt(bot_name, time, sender_id)
        };
        let system_msg = OpenAIMessage::system(system_prompt);

        let mut conversation: Vec<OpenAIMessage> = Vec::with_capacity(history.len() + 2);
        conversation.push(system_msg);
        conversation.extend(history.iter().cloned());
        conversation.push(user_msg.clone());

        let pending_reply_state = Arc::new(Mutex::new(PendingReplyState::default()));
        let (brain_output, stop_reason) = Brain::new(llm.clone())
            .with_tool(TavilyBrainTool {
                tavily_ref: tavily.clone(),
                adapter: adapter.clone(),
                target_id: target_id.to_string(),
                mention_target_id: if is_group {
                    Some(sender_id.to_string())
                } else {
                    None
                },
                is_group,
            })
            .with_tool(ReplyPlainTextBrainTool {
                pending_reply_state: pending_reply_state.clone(),
            })
            .with_tool(ReplyAtBrainTool {
                pending_reply_state: pending_reply_state.clone(),
                is_group,
            })
            .with_tool(ReplyCombineTextBrainTool {
                pending_reply_state: pending_reply_state.clone(),
                is_group,
            })
            .with_tool(NoReplyBrainTool {
                pending_reply_state: pending_reply_state.clone(),
            })
            .run(conversation);

        let last_assistant = brain_output.iter().rev().find(|m| {
            matches!(m.role, zihuan_llm_types::MessageRole::Assistant) && m.tool_calls.is_empty()
        });
        let final_assistant_text = last_assistant
            .and_then(|m| m.content.as_deref())
            .map(str::trim)
            .filter(|content| !content.is_empty())
            .map(ToOwned::to_owned);

        let pending_snapshot = {
            let state = lock_pending_state(&pending_reply_state)?;
            state.clone()
        };

        if pending_snapshot.suppress_send {
            info!("{LOG_PREFIX} no_reply was selected, skipping QQ send");
        } else {
            let mut batches = pending_snapshot.batches;
            if let Some(content) = final_assistant_text {
                batches.extend(plain_text_batches(&content));
            }

            if !batches.is_empty() {
                if is_group {
                    send_group_batches(adapter, target_id, &batches);
                } else {
                    send_friend_batches(adapter, target_id, &batches);
                }
            } else {
                match stop_reason {
                    BrainStopReason::TransportError(ref err) => {
                        warn!("{LOG_PREFIX} Brain transport error without reply: {err}");
                        if is_group {
                            send_group_text(adapter, target_id, FALLBACK_REPLY);
                        } else {
                            send_friend_text(adapter, target_id, FALLBACK_REPLY);
                        }
                    }
                    BrainStopReason::MaxIterationsReached => {
                        warn!("{LOG_PREFIX} Brain exceeded max tool iterations without reply");
                        if is_group {
                            send_group_text(adapter, target_id, FALLBACK_REPLY);
                        } else {
                            send_friend_text(adapter, target_id, FALLBACK_REPLY);
                        }
                    }
                    BrainStopReason::Done => {
                        warn!("{LOG_PREFIX} Brain finished without any sendable reply content");
                    }
                }
            }
        }

        history.push(user_msg);
        history.extend(brain_output);
        save_history(cache, sender_id, history);

        Ok(())
    }
}

impl Node for QqMessageAgentNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("使用Brain智能体响应消息事件，智能体会结合自身状态对消息事件进行判断并做出响应。")
    }

    node_input![
        port! { name = "message_event",  ty = MessageEvent,                    desc = "来自 bot_adapter 的消息事件" },
        port! { name = "qq_bot_adapter", ty = BotAdapterRef,                   desc = "Bot 适配器引用，用于发送消息" },
        port! { name = "time",           ty = String,                          desc = "当前时间字符串，注入 system prompt" },
        port! { name = "bot_name",       ty = String,                          desc = "机器人角色名称，注入 system prompt" },
        port! { name = "cache_ref",      ty = OpenAIMessageSessionCacheRef,    desc = "OpenAIMessage 会话历史缓存引用" },
        port! { name = "session_ref",    ty = SessionStateRef,                 desc = "运行时会话占用引用，防止并发推理" },
        port! { name = "llm_model",      ty = LLModel,                         desc = "LLM 模型引用" },
        port! { name = "tavily_ref",     ty = TavilyRef,                       desc = "Tavily 搜索引用" },
    ];

    node_output![];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let event = match inputs.get("message_event") {
            Some(DataValue::MessageEvent(e)) => e.clone(),
            _ => return Err(self.wrap_err("message_event is required")),
        };
        let adapter = match inputs.get("qq_bot_adapter") {
            Some(DataValue::BotAdapterRef(handle)) => shared_from_handle(handle),
            _ => return Err(self.wrap_err("qq_bot_adapter is required")),
        };
        let time = match inputs.get("time") {
            Some(DataValue::String(s)) => s.clone(),
            _ => return Err(self.wrap_err("time is required")),
        };
        let bot_name = match inputs.get("bot_name") {
            Some(DataValue::String(s)) => s.clone(),
            _ => return Err(self.wrap_err("bot_name is required")),
        };
        let cache = match inputs.get("cache_ref") {
            Some(DataValue::OpenAIMessageSessionCacheRef(r)) => r.clone(),
            _ => return Err(self.wrap_err("cache_ref is required")),
        };
        let session = match inputs.get("session_ref") {
            Some(DataValue::SessionStateRef(r)) => r.clone(),
            _ => return Err(self.wrap_err("session_ref is required")),
        };
        let llm = match inputs.get("llm_model") {
            Some(DataValue::LLModel(m)) => m.clone(),
            _ => return Err(self.wrap_err("llm_model is required")),
        };
        let tavily = match inputs.get("tavily_ref") {
            Some(DataValue::TavilyRef(t)) => t.clone(),
            _ => return Err(self.wrap_err("tavily_ref is required")),
        };

        self.handle(
            &event, &adapter, &time, &bot_name, &cache, &session, &llm, &tavily,
        )?;

        Ok(HashMap::new())
    }
}

#[cfg(test)]
mod tests {
    use super::{build_combine_text_batch, plain_text_batches, PendingReplyState};
    use zihuan_bot_adapter::models::message::Message;
    use zihuan_core::error::Result;

    #[test]
    fn plain_text_batches_split_long_text() {
        let input = "a".repeat(251);
        let batches = plain_text_batches(&input);
        assert_eq!(batches.len(), 2);
    }

    #[test]
    fn combine_text_batch_keeps_segment_order() -> Result<()> {
        let batch = build_combine_text_batch(
            &serde_json::json!({
                "content_list": [
                    { "message_type": "at", "target": "42" },
                    { "message_type": "plain_text", "content": "你好" }
                ]
            }),
            true,
        )?;

        assert!(matches!(
            batch.as_slice(),
            [Message::At(_), Message::PlainText(text)] if text.text == "你好"
        ));
        Ok(())
    }

    #[test]
    fn no_reply_clears_pending_batches() -> Result<()> {
        let mut state = PendingReplyState::default();
        state.append_batches(plain_text_batches("你好"))?;
        state.mark_no_reply();
        assert!(state.suppress_send);
        assert!(state.batches.is_empty());
        Ok(())
    }
}
