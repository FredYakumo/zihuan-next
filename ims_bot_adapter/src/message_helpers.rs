//! Stateless helpers for interacting with the bot adapter from application code.
//!
//! These functions replace boilerplate in node implementations that need to send messages
//! or query the adapter without going through the node-graph port system.

use crate::adapter::SharedBotAdapter;
use crate::models::event_model::{MessageEvent, MessageType, Sender};
use crate::models::message::{AtTargetMessage, Message, PlainTextMessage};
use crate::send_qq_message_batches::send_qq_message_batches;
use crate::ws_action::{
    qq_message_list_to_json, response_message_id, response_success, ws_send_action,
};
use log::{info, warn};
use std::sync::Arc;
use tokio::task::block_in_place;
use zihuan_core::data_refs::MySqlConfig;
use zihuan_graph_engine::data_value::RedisConfig;
use zihuan_graph_engine::message_persistence::persist_message_event;

const LOG_PREFIX: &str = "[message_helpers]";

#[derive(Debug, Clone, Default)]
pub struct OutboundMessagePersistence {
    pub mysql_ref: Option<Arc<MySqlConfig>>,
    pub redis_ref: Option<Arc<RedisConfig>>,
    pub group_name: Option<String>,
    pub sender_name: Option<String>,
}

fn build_outbound_event(
    adapter: &SharedBotAdapter,
    message_id: i64,
    message_type: MessageType,
    target_id: &str,
    group_name: Option<&str>,
    messages: &[Message],
    sender_name_override: Option<&str>,
) -> Option<MessageEvent> {
    let (bot_id, sender_name) = if let Ok(handle) = tokio::runtime::Handle::try_current() {
        block_in_place(|| {
            let guard = handle.block_on(adapter.lock());
            let bot_id = guard.get_bot_id().to_string();
            let profile_name = guard
                .get_bot_profile()
                .map(|profile| profile.nickname.clone())
                .unwrap_or_default();
            (bot_id, profile_name)
        })
    } else {
        let guard = adapter.blocking_lock();
        let bot_id = guard.get_bot_id().to_string();
        let profile_name = guard
            .get_bot_profile()
            .map(|profile| profile.nickname.clone())
            .unwrap_or_default();
        (bot_id, profile_name)
    };

    let sender_user_id = match bot_id.parse::<i64>() {
        Ok(value) => value,
        Err(error) => {
            warn!(
                "{LOG_PREFIX} Failed to parse bot_id '{bot_id}' into i64 for persistence: {error}"
            );
            return None;
        }
    };
    let sender_name = sender_name_override
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            let trimmed = sender_name.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        })
        .unwrap_or_else(|| bot_id.clone());

    let group_id = if message_type == MessageType::Group {
        match target_id.parse::<i64>() {
            Ok(value) => Some(value),
            Err(error) => {
                warn!("{LOG_PREFIX} Failed to parse group_id '{target_id}' into i64 for persistence: {error}");
                None
            }
        }
    } else {
        None
    };

    Some(MessageEvent {
        message_id,
        message_type,
        sender: Sender {
            user_id: sender_user_id,
            nickname: sender_name.clone(),
            card: sender_name,
            role: None,
        },
        message_list: messages.to_vec(),
        group_id,
        group_name: group_name.map(ToOwned::to_owned),
        is_group_message: message_type == MessageType::Group,
    })
}

fn persist_outbound_messages(
    adapter: &SharedBotAdapter,
    message_type: MessageType,
    target_id: &str,
    message_id: i64,
    messages: &[Message],
    persistence: &OutboundMessagePersistence,
) {
    if message_id <= 0 {
        return;
    }

    let Some(event) = build_outbound_event(
        adapter,
        message_id,
        message_type,
        target_id,
        persistence.group_name.as_deref(),
        messages,
        persistence.sender_name.as_deref(),
    ) else {
        return;
    };

    if let Err(error) = persist_message_event(
        &event,
        persistence.mysql_ref.as_ref(),
        persistence.redis_ref.as_ref(),
    ) {
        warn!(
            "{LOG_PREFIX} Failed to persist outbound {} message {}: {}",
            message_type.as_str(),
            message_id,
            error
        );
    }
}

/// Return the bot's self QQ ID from a shared adapter handle.
///
/// Uses `block_in_place` so it is safe to call from inside a Tokio worker thread.
pub fn get_bot_id(adapter: &SharedBotAdapter) -> String {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        block_in_place(|| {
            let guard = handle.block_on(adapter.lock());
            guard.get_bot_id().to_string()
        })
    } else {
        adapter.blocking_lock().get_bot_id().to_string()
    }
}

/// Send a single plain-text message to a QQ friend.
pub fn send_friend_text(adapter: &SharedBotAdapter, target_id: &str, text: &str) {
    let params = serde_json::json!({
        "user_id": target_id,
        "message": [{ "type": "text", "data": { "text": text } }]
    });
    if let Err(e) = ws_send_action(adapter, "send_private_msg", params) {
        warn!("{LOG_PREFIX} Failed to send friend text to {target_id}: {e}");
    }
}

pub fn send_friend_text_with_persistence(
    adapter: &SharedBotAdapter,
    target_id: &str,
    text: &str,
    persistence: &OutboundMessagePersistence,
) {
    let params = serde_json::json!({
        "user_id": target_id,
        "message": [{ "type": "text", "data": { "text": text } }]
    });
    match ws_send_action(adapter, "send_private_msg", params) {
        Ok(response) => {
            if response_success(&response) {
                let messages = vec![Message::PlainText(PlainTextMessage {
                    text: text.to_string(),
                })];
                persist_outbound_messages(
                    adapter,
                    MessageType::Private,
                    target_id,
                    response_message_id(&response).unwrap_or(-1),
                    &messages,
                    persistence,
                );
            }
        }
        Err(e) => {
            warn!("{LOG_PREFIX} Failed to send friend text to {target_id}: {e}");
        }
    }
}

/// Send a single plain-text message to a QQ group.
pub fn send_group_text(adapter: &SharedBotAdapter, target_id: &str, text: &str) {
    let params = serde_json::json!({
        "group_id": target_id,
        "message": [{ "type": "text", "data": { "text": text } }]
    });
    if let Err(e) = ws_send_action(adapter, "send_group_msg", params) {
        warn!("{LOG_PREFIX} Failed to send group text to {target_id}: {e}");
    }
}

pub fn send_group_text_with_persistence(
    adapter: &SharedBotAdapter,
    target_id: &str,
    text: &str,
    persistence: &OutboundMessagePersistence,
) {
    let params = serde_json::json!({
        "group_id": target_id,
        "message": [{ "type": "text", "data": { "text": text } }]
    });
    match ws_send_action(adapter, "send_group_msg", params) {
        Ok(response) => {
            if response_success(&response) {
                let messages = vec![Message::PlainText(PlainTextMessage {
                    text: text.to_string(),
                })];
                persist_outbound_messages(
                    adapter,
                    MessageType::Group,
                    target_id,
                    response_message_id(&response).unwrap_or(-1),
                    &messages,
                    persistence,
                );
            }
        }
        Err(e) => {
            warn!("{LOG_PREFIX} Failed to send group text to {target_id}: {e}");
        }
    }
}

/// Send multiple `Vec<Message>` batches to a QQ friend.
///
/// Mirrors `SendFriendMessageBatchesNode` behaviour without requiring node wiring.
pub fn send_friend_batches(adapter: &SharedBotAdapter, target_id: &str, batches: &[Vec<Message>]) {
    let results = send_qq_message_batches(adapter, "friend", target_id, batches);
    let all_ok = results.iter().filter(|r| !r.skipped).all(|r| r.success);
    info!(
        "{LOG_PREFIX} Sent friend batches to {target_id}: all_ok={all_ok}, count={}",
        batches.len()
    );
}

pub fn send_friend_batches_with_persistence(
    adapter: &SharedBotAdapter,
    target_id: &str,
    batches: &[Vec<Message>],
    persistence: &OutboundMessagePersistence,
) {
    let results = send_qq_message_batches(adapter, "friend", target_id, batches);
    for (batch, result) in batches.iter().zip(results.iter()) {
        if result.success && !result.skipped {
            persist_outbound_messages(
                adapter,
                MessageType::Private,
                target_id,
                result.message_id,
                batch,
                persistence,
            );
        }
    }
    let all_ok = results.iter().filter(|r| !r.skipped).all(|r| r.success);
    info!(
        "{LOG_PREFIX} Sent friend batches to {target_id}: all_ok={all_ok}, count={}",
        batches.len()
    );
}

/// Send multiple `Vec<Message>` batches to a QQ group.
///
/// Mirrors `SendGroupMessageBatchesNode` behaviour without requiring node wiring.
pub fn send_group_batches(adapter: &SharedBotAdapter, target_id: &str, batches: &[Vec<Message>]) {
    let results = send_qq_message_batches(adapter, "group", target_id, batches);
    let all_ok = results.iter().filter(|r| !r.skipped).all(|r| r.success);
    info!(
        "{LOG_PREFIX} Sent group batches to {target_id}: all_ok={all_ok}, count={}",
        batches.len()
    );
}

pub fn send_group_batches_with_persistence(
    adapter: &SharedBotAdapter,
    target_id: &str,
    batches: &[Vec<Message>],
    persistence: &OutboundMessagePersistence,
) {
    let results = send_qq_message_batches(adapter, "group", target_id, batches);
    for (batch, result) in batches.iter().zip(results.iter()) {
        if result.success && !result.skipped {
            persist_outbound_messages(
                adapter,
                MessageType::Group,
                target_id,
                result.message_id,
                batch,
                persistence,
            );
        }
    }
    let all_ok = results.iter().filter(|r| !r.skipped).all(|r| r.success);
    info!(
        "{LOG_PREFIX} Sent group batches to {target_id}: all_ok={all_ok}, count={}",
        batches.len()
    );
}

/// Send `@mention + plain_text` to a group.
///
/// Useful for sending progress notifications during tool calls (e.g. "我将搜索…").
pub fn send_group_progress_notification(
    adapter: &SharedBotAdapter,
    group_id: &str,
    mention_target_id: &str,
    content: &str,
) {
    if content.trim().is_empty() {
        return;
    }
    let messages = vec![
        Message::At(AtTargetMessage {
            target: Some(mention_target_id.to_string()),
        }),
        Message::PlainText(PlainTextMessage {
            text: content.to_string(),
        }),
    ];
    let params = serde_json::json!({
        "group_id": group_id,
        "message": qq_message_list_to_json(&messages),
    });
    if let Err(e) = ws_send_action(adapter, "send_group_msg", params) {
        warn!("{LOG_PREFIX} Failed to send group progress notification: {e}");
    }
}

pub fn send_group_progress_notification_with_persistence(
    adapter: &SharedBotAdapter,
    group_id: &str,
    mention_target_id: &str,
    content: &str,
    persistence: &OutboundMessagePersistence,
) {
    if content.trim().is_empty() {
        return;
    }
    let messages = vec![
        Message::At(AtTargetMessage {
            target: Some(mention_target_id.to_string()),
        }),
        Message::PlainText(PlainTextMessage {
            text: content.to_string(),
        }),
    ];
    let params = serde_json::json!({
        "group_id": group_id,
        "message": qq_message_list_to_json(&messages),
    });
    match ws_send_action(adapter, "send_group_msg", params) {
        Ok(response) => {
            if response_success(&response) {
                persist_outbound_messages(
                    adapter,
                    MessageType::Group,
                    group_id,
                    response_message_id(&response).unwrap_or(-1),
                    &messages,
                    persistence,
                );
            }
        }
        Err(e) => {
            warn!("{LOG_PREFIX} Failed to send group progress notification: {e}");
        }
    }
}

/// Send a plain-text progress notification to a friend.
///
/// No-op when `content` is blank.
pub fn send_friend_progress_notification(
    adapter: &SharedBotAdapter,
    target_id: &str,
    content: &str,
) {
    if content.trim().is_empty() {
        return;
    }
    send_friend_text(adapter, target_id, content);
}

pub fn send_friend_progress_notification_with_persistence(
    adapter: &SharedBotAdapter,
    target_id: &str,
    content: &str,
    persistence: &OutboundMessagePersistence,
) {
    if content.trim().is_empty() {
        return;
    }
    send_friend_text_with_persistence(adapter, target_id, content, persistence);
}
