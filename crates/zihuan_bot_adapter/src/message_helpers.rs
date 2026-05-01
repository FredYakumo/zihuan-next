//! Stateless helpers for interacting with the bot adapter from application code.
//!
//! These functions replace boilerplate in node implementations that need to send messages
//! or query the adapter without going through the node-graph port system.

use crate::adapter::SharedBotAdapter;
use crate::models::message::{AtTargetMessage, Message, PlainTextMessage};
use crate::send_qq_message_batches::send_qq_message_batches;
use crate::ws_action::{qq_message_list_to_json, ws_send_action};
use log::{info, warn};
use tokio::task::block_in_place;

const LOG_PREFIX: &str = "[message_helpers]";

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
