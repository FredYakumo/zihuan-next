use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use log::{info, warn};
use serde::Deserialize;
use serde_json::Value;

use crate::agent::brain::{Brain, BrainStopReason, BrainTool};
use zihuan_bot_adapter::adapter::shared_from_handle;
use zihuan_bot_adapter::message_helpers::{
    get_bot_id, send_friend_batches, send_friend_progress_notification, send_friend_text,
    send_group_batches, send_group_progress_notification,
};
use zihuan_bot_adapter::models::event_model::MessageType;
use zihuan_bot_adapter::models::message::{
    AtTargetMessage, ForwardMessage, ForwardNodeMessage, Message, MessageProp, PlainTextMessage,
};
use zihuan_core::error::{Error, Result};
use zihuan_core::runtime::block_async;
use zihuan_llm_types::tooling::FunctionTool;
use zihuan_llm_types::InferenceParam;
use zihuan_llm_types::OpenAIMessage;
use zihuan_node::data_value::{
    OpenAIMessageSessionCacheRef, SessionClaim, SessionStateRef, TavilyRef, SESSION_CLAIM_CONTEXT,
};
use zihuan_node::{node_input, node_output, DataType, DataValue, Node, Port};

const LOG_PREFIX: &str = "[QqMessageAgentNode]";
const BUSY_REPLY: &str = "我还在思考中，你别急";
const MAX_REPLY_CHARS: usize = 250;
const MAX_FORWARD_NODE_CHARS: usize = 800;
const DEFAULT_MAX_MESSAGE_LENGTH: usize = 500;

/// System prompt template (shared, private variant).
fn build_private_system_prompt(
    bot_name: &str,
    bot_id: &str,
    time: &str,
    sender_id: &str,
    sender_name: &str,
) -> String {
    format!(
        "你的名字叫`{bot_name}`(QQ号为`{bot_id}`)。现在时间是{time}，你的QQ好友`{sender_name}`(QQ号`{sender_id}`)向你发送了一条消息。\n\
         重要规则：当前 user 消息永远代表发送者，不代表你自己；如果消息里出现 @你，那只是对你的呼叫，不是在引入新的说话人；当用户问“你是谁/你叫什么”时，请以你自己的身份回答。\n\
         如果你要在群里或消息结构里 @ 某个人，不要把 @xxx 直接写进最终自然语言；必须调用 `reply_at` 或 `reply_combine_text` 来发送真正的 @ 消息段。\n\
         你可以选择调用相关工具来获取信息，并通过 reply_* 工具把特定 QQ 消息加入待发送列表。\n\
         最终请直接输出你想发送给对方的自然语言，不要输出 JSON、代码块或额外格式说明。\n\
         如果你调用了 reply_* 工具，这些工具加入的消息会先发送，你最后一条 assistant 自然语言回复会作为最后一条普通文本消息追加发送。\n\
         如果你决定这轮不回复，请调用 no_reply；调用后本轮不会发送任何 QQ 消息，但你仍然需要正常完成这一轮 assistant 收尾。\n\
         `reply_plain_text` 用于追加纯文本消息；`reply_at` 用于追加单独的 @ 消息；`reply_combine_text` 用于在同一次发送里组合 at 和文本片段。\n\
         当你需要输出较长总结、长文档解读、分点说明或超过一两屏的正文时，优先调用 `reply_forward_text`；它会把长正文整理成转发消息，适合给对方点开查看详情。\n\
         使用 `reply_forward_text` 后，最终 assistant 自然语言回复应保持简短，只用一两句话提醒对方查看你刚发的转发消息，不要把长正文再重复一遍。\n\
         对于超过250字的最终自然语言回复，系统会自动拆分发送。\n\
         当你决定调用工具时，请在工具 content 里用一句话说明你即将做什么（例如\"我将搜索关于xxx的信息\"）。"
    )
}

/// System prompt template (group variant).
fn build_group_system_prompt(
    bot_name: &str,
    bot_id: &str,
    time: &str,
    sender_id: &str,
    sender_name: &str,
    group_name: &str,
    group_id: &str,
) -> String {
    format!(
        "你的名字叫`{bot_name}`(QQ号为`{bot_id}`)。现在时间是{time}，你正在`{group_name}`群(群号:{group_id})里聊天，群友`{sender_name}`(QQ号`{sender_id}`)向你发送了一条消息。\n\
         重要规则：当前 user 消息永远代表发送者，不代表你自己；如果消息里出现 @你，那只是对你的呼叫，不是在引入新的说话人；当用户问“你是谁/你叫什么”时，请以你自己的身份回答。\n\
         如果你要 @ 某个人，不要把 @xxx 直接写进最终自然语言；必须调用 `reply_at` 或 `reply_combine_text` 来发送真正的 @ 消息段。当前这位发送者的 QQ 号是`{sender_id}`。\n\
         你可以选择调用相关工具来获取信息，并通过 reply_* 工具把特定 QQ 消息加入待发送列表。\n\
         最终请直接输出你想发送到群里的自然语言，不要输出 JSON、代码块或额外格式说明。\n\
         如果你调用了 reply_* 工具，这些工具加入的消息会先发送，你最后一条 assistant 自然语言回复会作为最后一条普通文本消息追加发送。\n\
         如果你决定这轮不回复，请调用 no_reply；调用后本轮不会发送任何 QQ 消息，但你仍然需要正常完成这一轮 assistant 收尾。\n\
         `reply_plain_text` 用于追加纯文本消息；`reply_at` 用于追加单独的 @ 消息；`reply_combine_text` 用于在同一次发送里组合 at 和文本片段。\n\
         当你需要输出较长总结、长文档解读、分点说明或超过一两屏的正文时，优先调用 `reply_forward_text`；它会把长正文整理成转发消息，适合给群友点开查看详情。\n\
         使用 `reply_forward_text` 后，最终 assistant 自然语言回复应保持简短，通常先 @ 发送者，再用一两句话提醒对方查看你刚发的转发消息，不要把长正文再重复一遍。\n\
         对于超过250字的最终自然语言回复，系统会自动拆分发送。\n\
         当你决定调用工具时，请在工具 content 里用一句话说明你即将做什么（例如\"我将搜索关于xxx的信息\"）。"
    )
}

fn conversation_history_key(
    bot_id: &str,
    sender_id: &str,
    is_group: bool,
    group_id: Option<i64>,
) -> String {
    if is_group {
        format!(
            "group:{bot_id}:{}:{sender_id}",
            group_id.unwrap_or_default()
        )
    } else {
        format!("private:{bot_id}:{sender_id}")
    }
}

fn load_history(
    cache: &Arc<OpenAIMessageSessionCacheRef>,
    history_key: &str,
    legacy_key: &str,
) -> Vec<OpenAIMessage> {
    let history = block_async(cache.get_messages(history_key)).unwrap_or_default();
    if history.is_empty() && history_key != legacy_key {
        block_async(cache.get_messages(legacy_key)).unwrap_or_default()
    } else {
        history
    }
}

fn save_history(
    cache: &Arc<OpenAIMessageSessionCacheRef>,
    history_key: &str,
    messages: Vec<OpenAIMessage>,
) {
    if let Err(e) = block_async(cache.set_messages(history_key, messages)) {
        warn!("{LOG_PREFIX} Failed to save history for {history_key}: {e}");
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

fn sender_display_name(sender_name: &str, sender_card: &str) -> String {
    let card = sender_card.trim();
    if card.is_empty() {
        sender_name.to_string()
    } else {
        card.to_string()
    }
}

fn strip_leading_bot_mention(text: &str, bot_id: &str, bot_name: &str) -> String {
    let mut remaining = text.trim_start();
    loop {
        let mut stripped = false;

        for pattern in [bot_id, bot_name] {
            let pattern = pattern.trim();
            if pattern.is_empty() {
                continue;
            }

            for prefix in [format!("@{pattern}"), format!("＠{pattern}")] {
                if let Some(rest) = remaining.strip_prefix(&prefix) {
                    remaining = rest.trim_start_matches(|c: char| {
                        matches!(
                            c,
                            ' ' | '\t'
                                | '\n'
                                | '\r'
                                | ','
                                | '，'
                                | '。'
                                | ':'
                                | '：'
                                | '!'
                                | '！'
                                | '?'
                                | '？'
                        )
                    });
                    stripped = true;
                    break;
                }
            }

            if stripped {
                break;
            }
        }

        if !stripped {
            break;
        }
    }

    remaining.trim().to_string()
}

fn strip_leading_textual_mention<'a>(text: &'a str, patterns: &[String]) -> Option<&'a str> {
    let mut remaining = text.trim_start();

    for pattern in patterns {
        let pattern = pattern.trim();
        if pattern.is_empty() {
            continue;
        }

        for prefix in [format!("@{pattern}"), format!("＠{pattern}")] {
            if let Some(rest) = remaining.strip_prefix(&prefix) {
                remaining = rest.trim_start_matches(|c: char| {
                    matches!(
                        c,
                        ' ' | '\t'
                            | '\n'
                            | '\r'
                            | ','
                            | '，'
                            | '。'
                            | ':'
                            | '：'
                            | '!'
                            | '！'
                            | '?'
                            | '？'
                    )
                });
                return Some(remaining);
            }
        }
    }

    None
}

fn sender_mention_patterns(
    sender_id: &str,
    sender_nickname: &str,
    sender_card: &str,
) -> Vec<String> {
    let mut patterns = Vec::new();

    for candidate in [sender_id, sender_nickname, sender_card] {
        let candidate = candidate.trim();
        if candidate.is_empty() {
            continue;
        }
        if !patterns.iter().any(|item| item == candidate) {
            patterns.push(candidate.to_string());
        }
    }

    patterns
}

fn assistant_reply_batches(
    content: &str,
    is_group: bool,
    sender_id: &str,
    sender_nickname: &str,
    sender_card: &str,
) -> Vec<Vec<Message>> {
    if !is_group {
        return plain_text_batches(content);
    }

    let patterns = sender_mention_patterns(sender_id, sender_nickname, sender_card);
    if let Some(rest) = strip_leading_textual_mention(content, &patterns) {
        let trimmed_rest = rest.trim();
        let mut batch = vec![Message::At(AtTargetMessage {
            target: Some(sender_id.to_string()),
        })];

        if !trimmed_rest.is_empty() {
            batch.push(Message::PlainText(PlainTextMessage {
                text: format!(" {trimmed_rest}"),
            }));
        }

        return vec![batch];
    }

    plain_text_batches(content)
}

/// Build a structured user message for the LLM so sender identity and bot mentions stay explicit.
fn build_user_message(
    event: &zihuan_bot_adapter::models::MessageEvent,
    bot_id: &str,
    bot_name: &str,
) -> String {
    let msg_prop =
        MessageProp::from_messages_with_bot_name(&event.message_list, Some(bot_id), Some(bot_name));
    let sender_name = sender_display_name(&event.sender.nickname, &event.sender.card);
    let mut lines = Vec::new();
    lines.push("[消息元信息]".to_string());
    lines.push(format!("message_type: {}", event.message_type.as_str()));
    lines.push(format!("sender_id: {}", event.sender.user_id));
    lines.push(format!("sender_name: {}", sender_name));
    lines.push(format!("bot_id: {}", bot_id));
    lines.push(format!("bot_name: {}", bot_name));
    lines.push(format!("is_at_bot: {}", msg_prop.is_at_me));

    if !msg_prop.at_target_list.is_empty() {
        lines.push(format!(
            "at_targets: {}",
            msg_prop.at_target_list.join(", ")
        ));
    }

    lines.push(String::new());
    lines.push("[用户消息]".to_string());
    let mut user_text = msg_prop.content.unwrap_or_default();
    if msg_prop.is_at_me {
        user_text = strip_leading_bot_mention(&user_text, bot_id, bot_name);
    }
    if user_text.trim().is_empty() {
        user_text = "(无文本内容，可能是仅@或回复)".to_string();
    }
    lines.push(user_text);

    if let Some(ref_cnt) = msg_prop.ref_content.as_deref() {
        if !ref_cnt.is_empty() {
            lines.push(String::new());
            lines.push("[引用内容]".to_string());
            lines.push(ref_cnt.to_string());
        }
    }

    lines.join("\n")
}

fn split_text_for_qq(content: &str) -> Vec<String> {
    split_text_by_semantic_boundaries(content, MAX_REPLY_CHARS)
}

fn plain_text_batches(content: &str) -> Vec<Vec<Message>> {
    split_text_for_qq(content)
        .into_iter()
        .map(|chunk| vec![Message::PlainText(PlainTextMessage { text: chunk })])
        .collect()
}

fn split_text_by_semantic_boundaries(content: &str, max_chars: usize) -> Vec<String> {
    let normalized = content.replace("\r\n", "\n");
    let trimmed = normalized.trim();
    if trimmed.is_empty() || max_chars == 0 {
        return Vec::new();
    }

    let mut chunks = Vec::new();
    let paragraphs: Vec<&str> = trimmed
        .split("\n\n")
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();

    let mut current = String::new();
    for paragraph in paragraphs {
        for unit in split_overlong_text_unit(paragraph, max_chars) {
            append_chunk_with_separator(&mut chunks, &mut current, &unit, "\n\n", max_chars);
        }
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    if chunks.is_empty() {
        split_text_hard(trimmed, max_chars)
    } else {
        chunks
    }
}

fn split_overlong_text_unit(content: &str, max_chars: usize) -> Vec<String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    if trimmed.chars().count() <= max_chars {
        return vec![trimmed.to_string()];
    }

    let lines: Vec<&str> = trimmed
        .split('\n')
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if lines.len() > 1 {
        return pack_text_units(lines, "\n", max_chars);
    }

    let sentences = split_into_sentence_like_units(trimmed);
    if sentences.len() > 1 {
        return pack_text_units(
            sentences.iter().map(String::as_str).collect(),
            "",
            max_chars,
        );
    }

    split_text_hard(trimmed, max_chars)
}

fn split_into_sentence_like_units(content: &str) -> Vec<String> {
    let mut units = Vec::new();
    let mut current = String::new();

    for ch in content.chars() {
        current.push(ch);
        if is_sentence_boundary(ch) {
            let segment = current.trim();
            if !segment.is_empty() {
                units.push(segment.to_string());
            }
            current.clear();
        }
    }

    let tail = current.trim();
    if !tail.is_empty() {
        units.push(tail.to_string());
    }

    units
}

fn is_sentence_boundary(ch: char) -> bool {
    matches!(
        ch,
        '。' | '！' | '？' | '；' | '!' | '?' | ';' | '\u{2026}' | '.'
    )
}

fn pack_text_units(units: Vec<&str>, separator: &str, max_chars: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();

    for unit in units {
        for sub_unit in split_overlong_sub_unit(unit, max_chars) {
            append_chunk_with_separator(&mut chunks, &mut current, &sub_unit, separator, max_chars);
        }
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

fn split_overlong_sub_unit(content: &str, max_chars: usize) -> Vec<String> {
    if content.chars().count() <= max_chars {
        return vec![content.to_string()];
    }
    split_text_hard(content, max_chars)
}

fn append_chunk_with_separator(
    chunks: &mut Vec<String>,
    current: &mut String,
    unit: &str,
    separator: &str,
    max_chars: usize,
) {
    if unit.is_empty() {
        return;
    }

    let candidate = if current.is_empty() {
        unit.to_string()
    } else {
        format!("{current}{separator}{unit}")
    };

    if candidate.chars().count() <= max_chars {
        *current = candidate;
    } else {
        if !current.is_empty() {
            chunks.push(std::mem::take(current));
        }
        *current = unit.to_string();
    }
}

fn split_text_hard(content: &str, max_chars: usize) -> Vec<String> {
    let chars: Vec<char> = content.chars().collect();
    let mut start = 0;
    let mut chunks = Vec::new();

    while start < chars.len() {
        let end = (start + max_chars).min(chars.len());
        let chunk: String = chars[start..end].iter().collect();
        let trimmed = chunk.trim();
        if !trimmed.is_empty() {
            chunks.push(trimmed.to_string());
        }
        start = end;
    }

    chunks
}

fn build_forward_message(content: &str, bot_id: &str, bot_name: &str) -> Result<ForwardMessage> {
    build_forward_message_from_chunks(
        split_text_by_semantic_boundaries(content, MAX_FORWARD_NODE_CHARS),
        bot_id,
        bot_name,
    )
}

fn build_forward_message_from_chunks(
    chunks: Vec<String>,
    bot_id: &str,
    bot_name: &str,
) -> Result<ForwardMessage> {
    let nodes: Vec<ForwardNodeMessage> = chunks
        .into_iter()
        .filter(|chunk| !chunk.trim().is_empty())
        .map(|chunk| ForwardNodeMessage {
            user_id: Some(bot_id.to_string()),
            nickname: Some(bot_name.to_string()),
            id: None,
            content: vec![Message::PlainText(PlainTextMessage { text: chunk })],
        })
        .collect();

    if nodes.is_empty() {
        return Err(Error::ValidationError(
            "forward content must not be blank".to_string(),
        ));
    }

    Ok(ForwardMessage {
        id: None,
        content: nodes,
    })
}

fn split_text_with_llm_for_forward(
    llm: &Arc<dyn zihuan_llm_types::llm_base::LLMBase>,
    content: &str,
) -> Result<Vec<String>> {
    let messages = vec![
        OpenAIMessage::system(format!(
            "你是一个中文长文本整理助手。你的任务是把用户给出的长文本拆成适合 QQ 转发消息节点展示的多个自然语义片段。\n\
             你必须满足这些规则：\n\
             1. 只输出纯 JSON 字符串数组，例如 [\"第一段\", \"第二段\"]，不要输出 markdown、代码块或解释。\n\
             2. 不要改写原文事实，不要新增信息，不要省略关键内容。\n\
             3. 按自然语义分段，优先保持段落、列表项、主题边界完整。\n\
             4. 每个数组元素控制在 {MAX_FORWARD_NODE_CHARS} 字以内，尽量不要太碎。\n\
             5. 如果原文本来就适合直接分段展示，只做分段，不要总结。"
        )),
        OpenAIMessage::user(content.to_string()),
    ];
    let param = InferenceParam {
        messages: &messages,
        tools: None,
    };
    let response = llm.inference(&param);
    let response_text = response.content.unwrap_or_default();
    if response_text.starts_with("Error:") {
        return Err(Error::StringError(format!(
            "forward splitting LLM request failed: {response_text}"
        )));
    }
    if !response.tool_calls.is_empty() {
        return Err(Error::ValidationError(
            "forward splitting LLM unexpectedly returned tool calls".to_string(),
        ));
    }

    let chunks: Vec<String> = serde_json::from_str(response_text.trim()).map_err(|e| {
        Error::ValidationError(format!(
            "failed to parse forward splitting LLM response: {e}"
        ))
    })?;

    let chunks: Vec<String> = chunks
        .into_iter()
        .map(|chunk| chunk.trim().to_string())
        .filter(|chunk| !chunk.is_empty())
        .collect();

    if chunks.is_empty() {
        return Err(Error::ValidationError(
            "forward splitting LLM returned empty chunks".to_string(),
        ));
    }

    Ok(chunks)
}

fn build_forward_message_via_llm(
    llm: &Arc<dyn zihuan_llm_types::llm_base::LLMBase>,
    content: &str,
    bot_id: &str,
    bot_name: &str,
) -> Result<ForwardMessage> {
    match split_text_with_llm_for_forward(llm, content) {
        Ok(chunks) => build_forward_message_from_chunks(chunks, bot_id, bot_name),
        Err(err) => {
            warn!(
                "{LOG_PREFIX} Forward splitting LLM failed, falling back to local semantic split: {err}"
            );
            build_forward_message(content, bot_id, bot_name)
        }
    }
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

fn lock_pending_state(
    state: &SharedPendingReplyState,
) -> Result<std::sync::MutexGuard<'_, PendingReplyState>> {
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
            description:
                "使用 Tavily 搜索引擎在互联网上搜索信息，返回相关网页的标题、链接和内容摘要",
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
            description:
                "向本轮待发送的 QQ 消息列表追加一次组合发送的消息段，可混合 at 和 plain_text。",
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

struct ReplyForwardTextBrainTool {
    pending_reply_state: SharedPendingReplyState,
    bot_id: String,
    bot_name: String,
}

impl BrainTool for ReplyForwardTextBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: "reply_forward_text",
            description:
                "向本轮待发送的 QQ 消息列表追加一条转发消息。适合长总结、长文档解读、分点说明等较长正文，系统会按自然语义拆成多个转发节点。",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "content": { "type": "string", "description": "要放进转发消息中的长文本正文" }
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
            let forward = build_forward_message(content, &self.bot_id, &self.bot_name)?;
            let node_count = forward.content.len();

            {
                let mut state = lock_pending_state(&self.pending_reply_state)?;
                state.append_batch(vec![Message::Forward(forward)])?;
            }

            Ok(serde_json::json!({
                "ok": true,
                "appended_batches": 1,
                "forward_nodes": node_count
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
        max_message_length: usize,
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
            max_message_length,
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
        max_message_length: usize,
    ) -> Result<()> {
        let bot_id = get_bot_id(adapter);
        let user_msg = OpenAIMessage::user(build_user_message(event, &bot_id, bot_name));

        let history_key = conversation_history_key(&bot_id, sender_id, is_group, event.group_id);
        let legacy_history_key = sender_id.to_string();
        let mut history = load_history(cache, &history_key, &legacy_history_key);

        let system_prompt = if is_group {
            let group_name = event.group_name.as_deref().unwrap_or("未知");
            build_group_system_prompt(
                bot_name,
                &bot_id,
                time,
                sender_id,
                &sender_display_name(&event.sender.nickname, &event.sender.card),
                group_name,
                target_id,
            )
        } else {
            build_private_system_prompt(
                bot_name,
                &bot_id,
                time,
                sender_id,
                &sender_display_name(&event.sender.nickname, &event.sender.card),
            )
        };
        info!("{LOG_PREFIX} build System prompt:\n=======\n{system_prompt}\n=======\n");
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
            .with_tool(ReplyForwardTextBrainTool {
                pending_reply_state: pending_reply_state.clone(),
                bot_id: bot_id.clone(),
                bot_name: bot_name.to_string(),
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
        let final_assistant_text = match stop_reason {
            BrainStopReason::TransportError(_) => None,
            _ => final_assistant_text,
        };

        let pending_snapshot = {
            let state = lock_pending_state(&pending_reply_state)?;
            state.clone()
        };

        if pending_snapshot.suppress_send {
            info!("{LOG_PREFIX} no_reply was selected, skipping QQ send");
        } else {
            let mut batches = pending_snapshot.batches;
            let mut has_forward_reply = batches
                .iter()
                .flatten()
                .any(|message| matches!(message, Message::Forward(_)));
            let final_assistant_text = final_assistant_text.and_then(|content| {
                if content.chars().count() > max_message_length {
                    match build_forward_message_via_llm(llm, &content, &bot_id, bot_name) {
                        Ok(forward) => {
                            has_forward_reply = true;
                            batches.push(vec![Message::Forward(forward)]);
                            None
                        }
                        Err(err) => {
                            warn!(
                                "{LOG_PREFIX} Failed to convert long assistant reply into forward message: {err}"
                            );
                            Some(content)
                        }
                    }
                } else {
                    Some(content)
                }
            });

            if let Some(content) = final_assistant_text {
                let sender_card = event.sender.card.as_str();
                batches.extend(assistant_reply_batches(
                    &content,
                    is_group,
                    sender_id,
                    &event.sender.nickname,
                    sender_card,
                ));
            } else if has_forward_reply {
                let reminder = if is_group {
                    format!(
                        "@{} 我刚刚发了转发消息，你可以点开看看详细内容。",
                        sender_id
                    )
                } else {
                    "我刚刚发了转发消息，你可以点开看看详细内容。".to_string()
                };
                let sender_card = event.sender.card.as_str();
                batches.extend(assistant_reply_batches(
                    &reminder,
                    is_group,
                    sender_id,
                    &event.sender.nickname,
                    sender_card,
                ));
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
                    }
                    BrainStopReason::MaxIterationsReached => {
                        warn!("{LOG_PREFIX} Brain exceeded max tool iterations without reply");
                    }
                    BrainStopReason::Done => {
                        warn!("{LOG_PREFIX} Brain finished without any sendable reply content");
                    }
                }
            }
        }

        history.push(user_msg);
        history.extend(brain_output);
        save_history(cache, &history_key, history);

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
        port! { name = "max_message_length", ty = Integer,                     desc = "可选：最终回复超过该字数时强制转为 forward，默认 500", optional },
    ];

    node_output![];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
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
        let max_message_length = match inputs.get("max_message_length") {
            Some(DataValue::Integer(value)) if *value > 0 => *value as usize,
            Some(DataValue::Integer(_)) => DEFAULT_MAX_MESSAGE_LENGTH,
            None => DEFAULT_MAX_MESSAGE_LENGTH,
            _ => return Err(self.wrap_err("max_message_length must be an integer when provided")),
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
            max_message_length,
        )?;

        Ok(HashMap::new())
    }
}
