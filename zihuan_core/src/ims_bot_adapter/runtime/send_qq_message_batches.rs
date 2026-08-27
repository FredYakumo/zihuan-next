use std::thread;
use std::time::Duration;

use crate::ims_bot_adapter::runtime::adapter::SharedBotAdapter;
use crate::ims_bot_adapter::runtime::models::message::{ForwardMessage, ForwardNodeMessage, Message};
use crate::ims_bot_adapter::runtime::ws_action::{
    json_i64, qq_message_list_to_send_json, response_message_id, response_success, ws_send_action,
    ws_send_action_with_timeout,
};
use log::{info, warn};
use crate::error::{Error, Result};

pub const TARGET_TYPE_FRIEND: &str = "friend";
pub const TARGET_TYPE_GROUP: &str = "group";
const DEFAULT_LOG_PREFIX: &str = "[SendQQMessageBatchesNode]";
const FORWARD_MESSAGE_RESPONSE_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendBatchResult {
    pub batch_index: usize,
    pub success: bool,
    pub skipped: bool,
    pub message_id: i64,
    pub retcode: Option<i64>,
    pub status: Option<String>,
    pub wording: Option<String>,
    pub text_length: usize,
    pub segment_count: usize,
}

pub fn qq_message_text_length(messages: &[Message]) -> usize {
    messages
        .iter()
        .map(|message| match message {
            Message::PlainText(text) => text.text.chars().count(),
            Message::Image(_) => 0,
            Message::Forward(forward) => forward.content.iter().map(|node| qq_message_text_length(&node.content)).sum(),
            _ => 0,
        })
        .sum()
}

pub fn describe_message_segments(messages: &[Message]) -> String {
    if messages.is_empty() {
        return "segments=0, text_length=0, preview=[]".to_string();
    }

    let preview = messages
        .iter()
        .map(|message| match message {
            Message::PlainText(text) => {
                let content: String = text.text.chars().take(24).collect();
                format!("text:{content}")
            }
            Message::At(at) => format!("at:{}", at.target.as_deref().unwrap_or("null")),
            Message::Reply(reply) => format!("reply:{}", reply.id),
            Message::Image(image) => format!(
                "image:{}",
                image
                    .name()
                    .or(image.rustfs_path())
                    .or(image.original_source())
                    .unwrap_or("unknown")
            ),
            Message::Forward(forward) => format!("forward:{}nodes", forward.content.len()),
        })
        .collect::<Vec<_>>()
        .join(" | ");

    format!(
        "segments={}, text_length={}, preview=[{}]",
        messages.len(),
        qq_message_text_length(messages),
        preview
    )
}

fn forward_nodes_to_send_json(
    adapter_ref: &SharedBotAdapter,
    nodes: &[ForwardNodeMessage],
) -> Result<serde_json::Value> {
    Ok(serde_json::Value::Array(
        nodes
            .iter()
            .map(|node| {
                let mut data = serde_json::Map::new();

                if let Some(ref id) = node.id {
                    data.insert("id".to_string(), serde_json::Value::String(id.to_string()));
                }
                if let Some(ref user_id) = node.user_id {
                    data.insert("user_id".to_string(), serde_json::Value::String(user_id.to_string()));
                    data.insert("uin".to_string(), serde_json::Value::String(user_id.to_string()));
                }
                if let Some(ref nickname) = node.nickname {
                    data.insert("nickname".to_string(), serde_json::Value::String(nickname.to_string()));
                    data.insert("name".to_string(), serde_json::Value::String(nickname.to_string()));
                }
                if !node.content.is_empty() {
                    data.insert("content".to_string(), qq_message_list_to_send_json(adapter_ref, &node.content)?);
                }

                Ok(serde_json::json!({
                    "type": "node",
                    "data": data,
                }))
            })
            .collect::<Result<Vec<_>>>()?,
    ))
}

fn forward_payload(
    adapter_ref: &SharedBotAdapter,
    target_type: &str,
    target_id: &str,
    forward: &ForwardMessage,
) -> Result<(&'static str, serde_json::Value)> {
    if forward.content.is_empty() {
        return Err(Error::ValidationError(
            "forward message must contain at least one node".to_string(),
        ));
    }

    let action_name = if target_type == TARGET_TYPE_GROUP {
        "send_group_forward_msg"
    } else {
        "send_private_forward_msg"
    };
    let messages = forward_nodes_to_send_json(adapter_ref, &forward.content)?;
    let params = if target_type == TARGET_TYPE_GROUP {
        serde_json::json!({
            "group_id": target_id,
            "messages": messages.clone(),
        })
    } else {
        serde_json::json!({
            "user_id": target_id,
            "messages": messages,
        })
    };

    Ok((action_name, params))
}

fn send_one_batch(
    adapter_ref: &SharedBotAdapter,
    target_type: &str,
    target_id: &str,
    batch_index: usize,
    messages: &[Message],
) -> Result<SendBatchResult> {
    let contains_forward = messages.iter().any(|message| matches!(message, Message::Forward(_)));
    if contains_forward && (messages.len() != 1 || !matches!(messages[0], Message::Forward(_))) {
        return Err(Error::ValidationError(
            "forward message batch must contain exactly one forward message".to_string(),
        ));
    }

    let (action_name, params) = if let [Message::Forward(forward)] = messages {
        forward_payload(adapter_ref, target_type, target_id, forward)?
    } else {
        let params = if target_type == TARGET_TYPE_GROUP {
            serde_json::json!({
                "group_id": target_id,
                "message": qq_message_list_to_send_json(adapter_ref, messages)?,
            })
        } else {
            serde_json::json!({
                "user_id": target_id,
                "message": qq_message_list_to_send_json(adapter_ref, messages)?,
            })
        };

        let action_name = if target_type == TARGET_TYPE_GROUP {
            "send_group_msg"
        } else {
            "send_private_msg"
        };

        (action_name, params)
    };

    let response = if matches!(action_name, "send_group_forward_msg" | "send_private_forward_msg") {
        ws_send_action_with_timeout(adapter_ref, action_name, params, FORWARD_MESSAGE_RESPONSE_TIMEOUT)?
    } else {
        ws_send_action(adapter_ref, action_name, params)?
    };
    Ok(SendBatchResult {
        batch_index,
        success: response_success(&response),
        skipped: false,
        message_id: response_message_id(&response).unwrap_or(-1),
        retcode: json_i64(response.get("retcode")),
        status: response
            .get("status")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string()),
        wording: response
            .get("wording")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string()),
        text_length: qq_message_text_length(messages),
        segment_count: messages.len(),
    })
}

fn skipped_batch_result(batch_index: usize) -> SendBatchResult {
    SendBatchResult {
        batch_index,
        success: true,
        skipped: true,
        message_id: -1,
        retcode: None,
        status: None,
        wording: Some("empty batch skipped".to_string()),
        text_length: 0,
        segment_count: 0,
    }
}

pub fn send_qq_message_batches_with_delay(
    adapter_ref: &SharedBotAdapter,
    target_type: &str,
    target_id: &str,
    batches: &[Vec<Message>],
    delay_millis: u64,
    log_prefix: &str,
) -> Vec<SendBatchResult> {
    let mut results = Vec::with_capacity(batches.len());
    let mut has_attempted_actual_send = false;

    info!(
        "{log_prefix} Preparing to send {} batch(es) to {}:{} with delay={}ms",
        batches.len(),
        target_type,
        target_id,
        delay_millis
    );

    for (index, batch) in batches.iter().enumerate() {
        if batch.is_empty() {
            info!(
                "{log_prefix} Skipping empty batch {} for {}:{}",
                index + 1,
                target_type,
                target_id
            );
            results.push(skipped_batch_result(index));
            continue;
        }

        if has_attempted_actual_send && delay_millis > 0 {
            info!(
                "{log_prefix} Waiting {} ms before batch {} to {}:{}",
                delay_millis,
                index + 1,
                target_type,
                target_id
            );
            thread::sleep(Duration::from_millis(delay_millis));
        }

        has_attempted_actual_send = true;
        info!(
            "{log_prefix} Sending batch {} to {}:{} with {}",
            index + 1,
            target_type,
            target_id,
            describe_message_segments(batch)
        );

        match send_one_batch(adapter_ref, target_type, target_id, index, batch) {
            Ok(result) => {
                if result.success {
                    info!(
                        "{log_prefix} Sent batch {} to {}:{} (message_id={}, retcode={:?}, status={:?}, segments={}, text_length={})",
                        index + 1,
                        target_type,
                        target_id,
                        result.message_id,
                        result.retcode,
                        result.status,
                        result.segment_count,
                        result.text_length
                    );
                } else {
                    warn!(
                        "{log_prefix} Failed to send batch {} to {}:{} (message_id={}, retcode={:?}, status={:?}, wording={:?}, {})",
                        index + 1,
                        target_type,
                        target_id,
                        result.message_id,
                        result.retcode,
                        result.status,
                        result.wording,
                        describe_message_segments(batch)
                    );
                }
                results.push(result);
            }
            Err(err) => {
                warn!(
                    "{log_prefix} Error sending batch {} to {}:{}: {} ({})",
                    index + 1,
                    target_type,
                    target_id,
                    err,
                    describe_message_segments(batch)
                );
                results.push(SendBatchResult {
                    batch_index: index,
                    success: false,
                    skipped: false,
                    message_id: -1,
                    retcode: None,
                    status: None,
                    wording: Some(err.to_string()),
                    text_length: qq_message_text_length(batch),
                    segment_count: batch.len(),
                });
            }
        }
    }

    results
}

pub fn send_qq_message_batches(
    adapter_ref: &SharedBotAdapter,
    target_type: &str,
    target_id: &str,
    batches: &[Vec<Message>],
) -> Vec<SendBatchResult> {
    send_qq_message_batches_with_delay(adapter_ref, target_type, target_id, batches, 0, DEFAULT_LOG_PREFIX)
}

pub fn message_ids_from_results(results: &[SendBatchResult]) -> Vec<i64> {
    results.iter().map(|result| result.message_id).collect()
}

pub fn actual_sends_all_successful(results: &[SendBatchResult]) -> bool {
    results.iter().filter(|result| !result.skipped).all(|result| result.success)
}

pub fn build_send_summary(target_type: &str, target_id: &str, results: &[SendBatchResult]) -> String {
    if results.is_empty() {
        return format!("未发送任何批次，目标={target_type}:{target_id}，共接收 0 批。");
    }

    let sent_results: Vec<&SendBatchResult> = results.iter().filter(|result| !result.skipped).collect();
    let success_count = sent_results.iter().filter(|result| result.success).count();
    let failure_count = sent_results.len().saturating_sub(success_count);
    let skipped_count = results.iter().filter(|result| result.skipped).count();
    let lengths = results
        .iter()
        .map(|result| result.text_length.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let segment_counts = results
        .iter()
        .map(|result| result.segment_count.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let failed_batches = sent_results
        .iter()
        .filter(|result| !result.success)
        .map(|result| {
            format!(
                "#{}(message_id={},retcode={:?},status={:?},wording={:?})",
                result.batch_index + 1,
                result.message_id,
                result.retcode,
                result.status,
                result.wording
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    let overall = if sent_results.is_empty() {
        "没有可发送的非空批次"
    } else if failure_count == 0 {
        "全部发送成功"
    } else if success_count == 0 {
        "全部发送失败"
    } else {
        "部分发送失败"
    };
    let skipped_suffix = if skipped_count == 0 {
        String::new()
    } else {
        format!("，跳过 {skipped_count} 批空消息")
    };

    if failed_batches.is_empty() {
        format!(
            "{overall}，目标={target_type}:{target_id}，共接收 {total} 批，实际发送 {sent} 批，成功 {success_count} 批，失败 {failure_count} 批{skipped_suffix}，每批文本长度=[{lengths}]，每批消息段数=[{segment_counts}]。",
            total = results.len(),
            sent = sent_results.len(),
        )
    } else {
        format!(
            "{overall}，目标={target_type}:{target_id}，共接收 {total} 批，实际发送 {sent} 批，成功 {success_count} 批，失败 {failure_count} 批{skipped_suffix}，每批文本长度=[{lengths}]，每批消息段数=[{segment_counts}]，失败批次={failed_batches}。",
            total = results.len(),
            sent = sent_results.len(),
        )
    }
}
