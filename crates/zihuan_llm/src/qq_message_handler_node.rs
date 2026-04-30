//! QQ 消息总处理节点
//!
//! 将顶层图中的 `message_event_type_filter` + `好友消息处理` + `群聊消息处理` 三段结构收敛成一个节点。
//!
//! 执行顺序（好友/群聊公共流程）：
//! 1. 从 `MessageEvent` 提取 `sender_id` 与 `target_id`（群聊时为 `group_id`）
//! 2. 尝试占用 `session_ref`（同一 sender 同时只允许一个推理在跑）
//!    - 好友：占用失败 → 发送"我还在思考中，你别急"并返回
//!    - 群聊：占用失败 → 静默跳过
//! 3. 从 `cache_ref` 加载历史消息
//! 4. 提取本条消息文本，构造 user message，并拼装带角色/时间/格式约束的 system prompt
//! 5. 拼接历史 + user message，调用 Brain 推理（含 Tavily 搜索工具）
//!    - 工具调用时，若 content 非空则先向目标发送进度通知消息
//! 6. 取 Brain 输出最后一条 assistant message，解析 QQ JSON
//!    - 解析成功 → 批量发送；失败 → 发送兜底"对不起,我无法回复这条消息"
//! 7. 将历史 + Brain 输出全部写回 cache_ref
//! 8. 释放 session_ref

use std::collections::HashMap;
use std::sync::Arc;

use log::{info, warn};
use serde_json::Value;
use tokio::task::block_in_place;

use zihuan_bot_adapter::adapter::shared_from_handle;
use zihuan_bot_adapter::models::event_model::MessageType;
use zihuan_bot_adapter::models::message::{AtTargetMessage, Message, MessageProp, PlainTextMessage};
use zihuan_bot_adapter::ws_action::ws_send_action;
use zihuan_bot_types::natural_language_reply::{json_value_to_qq_message_vec, qq_message_json_output_system_prompt};
use zihuan_core::error::{Error, Result};
use crate::agent::brain::{Brain, BrainTool};
use zihuan_llm_types::OpenAIMessage;
use zihuan_llm_types::tooling::FunctionTool;
use zihuan_node::data_value::{
    OpenAIMessageSessionCacheRef, SessionClaim, SessionStateRef, TavilyRef, SESSION_CLAIM_CONTEXT,
};
use zihuan_node::{node_input, node_output, DataType, DataValue, Node, Port};

const LOG_PREFIX: &str = "[QqMessageHandlerNode]";
const BUSY_REPLY: &str = "我还在思考中，你别急";
const FALLBACK_REPLY: &str = "对不起,我无法回复这条消息";

/// System prompt template (shared, private variant).
/// Variables: {bot_name}, {time}, {sender_id}, {format}
fn build_private_system_prompt(bot_name: &str, time: &str, sender_id: &str, format_prompt: &str) -> String {
    format!(
        "你的角色是{bot_name}。现在时间是{time}，你的QQ好友{sender_id}向你发送了一条消息。\n\
         你可以选择调用相关工具来获取信息，并发送QQ消息回复对方。也可以不回复对方，发送空的JSON数组。\n\
         以下是你的输出格式，必须严格遵守\n\
         对于超过250字过长的文本，请拆成多段，每一段信息要足够完整。\n\
         当你决定调用工具时，请在工具 content 里用一句话说明你即将做什么（例如\"我将搜索关于xxx的信息\"）。\n\
         {format_prompt}"
    )
}

/// System prompt template (group variant).
fn build_group_system_prompt(bot_name: &str, time: &str, sender_id: &str, format_prompt: &str) -> String {
    format!(
        "你的角色是{bot_name}。现在时间是{time}，你的QQ群友{sender_id}向你发送了一条消息。\n\
         你可以选择调用相关工具来获取信息，并发送QQ消息回复对方。也可以不回复对方，发送空的JSON数组。\n\
         以下是你的输出格式，必须严格遵守\n\
         对于超过250字过长的文本，请拆成多段，每一段信息要足够完整。\n\
         当你决定调用工具时，请在工具 content 里用一句话说明你即将做什么（例如\"我将搜索关于xxx的信息\"）。\n\
         {format_prompt}"
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers — mirror the behaviour that used to live inside function subgraphs
// ─────────────────────────────────────────────────────────────────────────────

fn block_async<F, T>(f: F) -> T
where
    F: std::future::Future<Output = T>,
{
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        block_in_place(|| handle.block_on(f))
    } else {
        tokio::runtime::Runtime::new()
            .expect("tokio runtime")
            .block_on(f)
    }
}

fn load_history(cache: &Arc<OpenAIMessageSessionCacheRef>, sender_id: &str) -> Vec<OpenAIMessage> {
    block_async(cache.get_messages(sender_id)).unwrap_or_default()
}

fn save_history(cache: &Arc<OpenAIMessageSessionCacheRef>, sender_id: &str, messages: Vec<OpenAIMessage>) {
    if let Err(e) = block_async(cache.set_messages(sender_id, messages)) {
        warn!("{LOG_PREFIX} Failed to save history for {sender_id}: {e}");
    }
}

/// Try to claim a session slot.  Returns `(claimed, claim_token)`.
fn try_claim_session(session: &Arc<SessionStateRef>, sender_id: &str) -> (bool, Option<u64>) {
    let (state, claimed) = block_async(session.try_claim(sender_id, None));

    // Mirror what SessionStateTryClaimNode does: register the claim in the
    // task-local context so that SessionStateReleaseNode (if present elsewhere)
    // can also find it, but we manage the token ourselves here.
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
    // Unregister from the task-local context first (mirrors SessionStateReleaseNode).
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

/// Retrieve the bot's QQ ID from the adapter.
fn get_bot_id(adapter: &zihuan_bot_adapter::adapter::SharedBotAdapter) -> String {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        block_in_place(|| {
            let guard = handle.block_on(adapter.lock());
            guard.get_bot_id().to_string()
        })
    } else {
        adapter.blocking_lock().get_bot_id().to_string()
    }
}

/// Send a single plain-text message to a friend.
fn send_friend_text(
    adapter: &zihuan_bot_adapter::adapter::SharedBotAdapter,
    target_id: &str,
    text: &str,
) {
    let params = serde_json::json!({
        "user_id": target_id,
        "message": [{ "type": "text", "data": { "text": text } }]
    });
    if let Err(e) = ws_send_action(adapter, "send_private_msg", params) {
        warn!("{LOG_PREFIX} Failed to send friend text to {target_id}: {e}");
    }
}

/// Send a single plain-text message to a group.
fn send_group_text(
    adapter: &zihuan_bot_adapter::adapter::SharedBotAdapter,
    target_id: &str,
    text: &str,
) {
    let params = serde_json::json!({
        "group_id": target_id,
        "message": [{ "type": "text", "data": { "text": text } }]
    });
    if let Err(e) = ws_send_action(adapter, "send_group_msg", params) {
        warn!("{LOG_PREFIX} Failed to send group text to {target_id}: {e}");
    }
}

/// Send Vec<Vec<QQMessage>> batches to a friend (mirrors SendFriendMessageBatchesNode behaviour).
fn send_friend_batches(
    adapter: &zihuan_bot_adapter::adapter::SharedBotAdapter,
    target_id: &str,
    batches: &[Vec<Message>],
) {
    use zihuan_bot_adapter::send_qq_message_batches::send_qq_message_batches;
    let results = send_qq_message_batches(adapter, "friend", target_id, batches);
    let all_ok = results.iter().filter(|r| !r.skipped).all(|r| r.success);
    info!("{LOG_PREFIX} Sent friend batches to {target_id}: all_ok={all_ok}, batches={}", batches.len());
}

/// Send Vec<Vec<QQMessage>> batches to a group (mirrors SendGroupMessageBatchesNode behaviour).
fn send_group_batches(
    adapter: &zihuan_bot_adapter::adapter::SharedBotAdapter,
    target_id: &str,
    batches: &[Vec<Message>],
) {
    use zihuan_bot_adapter::send_qq_message_batches::send_qq_message_batches;
    let results = send_qq_message_batches(adapter, "group", target_id, batches);
    let all_ok = results.iter().filter(|r| !r.skipped).all(|r| r.success);
    info!("{LOG_PREFIX} Sent group batches to {target_id}: all_ok={all_ok}, batches={}", batches.len());
}

/// Send an `@mention + plain_text` progress notification to a group.
/// Used inside tool execution to tell the group "I'm searching for X".
fn send_group_progress_notification(
    adapter: &zihuan_bot_adapter::adapter::SharedBotAdapter,
    group_id: &str,
    mention_target_id: &str,
    content: &str,
) {
    if content.trim().is_empty() {
        return;
    }
    use zihuan_bot_adapter::ws_action::qq_message_list_to_json;
    let messages = vec![
        Message::At(AtTargetMessage { target: Some(mention_target_id.to_string()) }),
        Message::PlainText(PlainTextMessage { text: content.to_string() }),
    ];
    let params = serde_json::json!({
        "group_id": group_id,
        "message": qq_message_list_to_json(&messages),
    });
    if let Err(e) = ws_send_action(adapter, "send_group_msg", params) {
        warn!("{LOG_PREFIX} Failed to send group progress notification: {e}");
    }
}

/// Send a progress notification to a friend (no @mention, just text).
fn send_friend_progress_notification(
    adapter: &zihuan_bot_adapter::adapter::SharedBotAdapter,
    target_id: &str,
    content: &str,
) {
    if content.trim().is_empty() {
        return;
    }
    send_friend_text(adapter, target_id, content);
}

// ─────────────────────────────────────────────────────────────────────────────
// TavilyBrainTool — Tavily search wrapped as a BrainTool
// ─────────────────────────────────────────────────────────────────────────────

struct TavilyBrainTool {
    tavily_ref: Arc<TavilyRef>,
    adapter: zihuan_bot_adapter::adapter::SharedBotAdapter,
    target_id: String,
    mention_target_id: Option<String>, // group only
    is_group: bool,
}

impl BrainTool for TavilyBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        #[derive(Debug)]
        struct TavilySpec;
        impl FunctionTool for TavilySpec {
            fn name(&self) -> &str { "web_search" }
            fn description(&self) -> &str {
                "使用 Tavily 搜索引擎在互联网上搜索信息，返回相关网页的标题、链接和内容摘要"
            }
            fn parameters(&self) -> serde_json::Value {
                serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "搜索关键词或问题" },
                        "search_count": { "type": "integer", "description": "搜索结果数量，通常为 3，最大 10" }
                    },
                    "required": ["query", "search_count"]
                })
            }
            fn call(&self, _arguments: serde_json::Value) -> zihuan_core::error::Result<serde_json::Value> {
                Ok(serde_json::Value::Null)
            }
        }
        Arc::new(TavilySpec)
    }

    fn execute(&self, call_content: &str, arguments: &Value) -> String {
        // Send progress notification.
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

        let results = crate::rag::tavily_search_node::TavilySearchNode::execute_with_endpoint(
            self.tavily_ref.as_ref(),
            &query,
            search_count,
            "https://api.tavily.com/search",
        );
        match results {
            Ok(items) => serde_json::json!({ "results": items }).to_string(),
            Err(e) => {
                warn!("{LOG_PREFIX} Tavily search failed: {e}");
                serde_json::json!({"results": [], "error": e.to_string()}).to_string()
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Node
// ─────────────────────────────────────────────────────────────────────────────

pub struct QqMessageHandlerNode {
    id: String,
    name: String,
}

impl QqMessageHandlerNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }

    fn wrap_err(&self, msg: impl Into<String>) -> Error {
        Error::ValidationError(format!("[NODE_ERROR:{}] {}", self.id, msg.into()))
    }

    // ── core handler ──────────────────────────────────────────────────────────

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

        //  Session claim
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

        let format_prompt = qq_message_json_output_system_prompt();
        let system_prompt = if is_group {
            build_group_system_prompt(bot_name, time, sender_id, format_prompt)
        } else {
            build_private_system_prompt(bot_name, time, sender_id, format_prompt)
        };
        let system_msg = OpenAIMessage::system(system_prompt);

        // Build: [system, ...history, user]
        let mut conversation: Vec<OpenAIMessage> = Vec::with_capacity(history.len() + 2);
        conversation.push(system_msg);
        conversation.extend(history.iter().cloned());
        conversation.push(user_msg.clone());

        //  Brain loop
        let brain_output = Brain::new(llm.clone())
            .with_tool(TavilyBrainTool {
                tavily_ref: tavily.clone(),
                adapter: adapter.clone(),
                target_id: target_id.to_string(),
                mention_target_id: if is_group { Some(sender_id.to_string()) } else { None },
                is_group,
            })
            .run(conversation)
            .0;


        let last_assistant = brain_output.iter().rev().find(|m| {
            matches!(m.role, zihuan_llm_types::MessageRole::Assistant) && m.tool_calls.is_empty()
        });

        let sent_fallback = match last_assistant.and_then(|m| m.content.as_deref()) {
            Some(content) => match serde_json::from_str::<Value>(content) {
                Ok(json_val) => match json_value_to_qq_message_vec(&json_val) {
                    Ok(batches) if !batches.is_empty() => {
                        if is_group {
                            send_group_batches(adapter, target_id, &batches);
                        } else {
                            send_friend_batches(adapter, target_id, &batches);
                        }
                        false
                    }
                    Ok(_) => {
                        // Empty batches = LLM chose not to reply.
                        info!("{LOG_PREFIX} LLM returned empty reply, skipping send");
                        false
                    }
                    Err(e) => {
                        warn!("{LOG_PREFIX} QQ JSON conversion failed: {e}");
                        true
                    }
                },
                Err(e) => {
                    warn!("{LOG_PREFIX} LLM output is not valid JSON: {e}");
                    true
                }
            },
            None => {
                warn!("{LOG_PREFIX} No assistant message found in Brain output");
                true
            }
        };

        if sent_fallback {
            if is_group {
                send_group_text(adapter, target_id, FALLBACK_REPLY);
            } else {
                send_friend_text(adapter, target_id, FALLBACK_REPLY);
            }
        }

        history.push(user_msg);
        history.extend(brain_output);
        save_history(cache, sender_id, history);

        Ok(())
    }
}

impl Node for QqMessageHandlerNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("QQ 消息总处理节点：内部完成好友/群聊分流、会话占用、历史缓存、Brain+Tavily 推理与消息发送")
    }

    node_input![
        port! { name = "message_event",  ty = MessageEvent,                    desc = "来自 bot_adapter 的消息事件" },
        port! { name = "qq_bot_adapter", ty = BotAdapterRef,                   desc = "Bot 适配器引用，用于发送消息" },
        port! { name = "time",           ty = String,                           desc = "当前时间字符串，注入 system prompt" },
        port! { name = "bot_name",       ty = String,                           desc = "机器人角色名称，注入 system prompt" },
        port! { name = "cache_ref",      ty = OpenAIMessageSessionCacheRef,     desc = "OpenAIMessage 会话历史缓存引用" },
        port! { name = "session_ref",    ty = SessionStateRef,                  desc = "运行时会话占用引用，防止并发推理" },
        port! { name = "llm_model",      ty = LLModel,                          desc = "LLM 模型引用" },
        port! { name = "tavily_ref",     ty = TavilyRef,                        desc = "Tavily 搜索引用" },
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
            &event,
            &adapter,
            &time,
            &bot_name,
            &cache,
            &session,
            &llm,
            &tavily,
        )?;

        Ok(HashMap::new())
    }
}
