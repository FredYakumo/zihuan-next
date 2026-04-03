use std::collections::HashMap;
use std::thread;
use std::time::Duration;

use crate::bot_adapter::adapter::SharedBotAdapter;
use crate::bot_adapter::models::message::Message;
use crate::bot_adapter::ws_action::{
    json_i64, qq_message_list_to_json, response_message_id, response_success, ws_send_action,
};
use crate::error::{Error, Result};
use crate::node::{node_input, node_output, DataType, DataValue, Node, Port};
use log::{info, warn};

pub const TARGET_TYPE_FRIEND: &str = "friend";
pub const TARGET_TYPE_GROUP: &str = "group";
const DEFAULT_LOG_PREFIX: &str = "[SendQQMessageBatchesNode]";

pub struct SendQQMessageBatchesNode {
    id: String,
    name: String,
}

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

impl SendQQMessageBatchesNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

fn normalize_target_type(value: Option<&DataValue>) -> &'static str {
    match value {
        Some(DataValue::String(target_type))
            if target_type.eq_ignore_ascii_case(TARGET_TYPE_GROUP) =>
        {
            TARGET_TYPE_GROUP
        }
        _ => TARGET_TYPE_FRIEND,
    }
}

pub fn qq_messages_from_data_value(
    value: Option<&DataValue>,
    input_name: &str,
) -> Result<Vec<Message>> {
    match value {
        Some(DataValue::Vec(_, items)) => Ok(items
            .iter()
            .filter_map(|item| match item {
                DataValue::QQMessage(message) => Some(message.clone()),
                _ => None,
            })
            .collect()),
        _ => Err(Error::InvalidNodeInput(format!(
            "{input_name} input is required"
        ))),
    }
}

pub fn qq_message_batches_from_data_value(
    value: Option<&DataValue>,
    input_name: &str,
) -> Result<Vec<Vec<Message>>> {
    match value {
        Some(DataValue::Vec(_, batch_values)) => batch_values
            .iter()
            .map(|batch_value| qq_messages_from_data_value(Some(batch_value), input_name))
            .collect(),
        _ => Err(Error::InvalidNodeInput(format!(
            "{input_name} input is required"
        ))),
    }
}

pub fn delay_millis_from_data_value(value: Option<&DataValue>, input_name: &str) -> Result<u64> {
    match value {
        Some(DataValue::Integer(delay)) => Ok((*delay).max(0) as u64),
        None => Ok(0),
        _ => Err(Error::InvalidNodeInput(format!(
            "{input_name} must be an integer when provided"
        ))),
    }
}

pub fn qq_message_text_length(messages: &[Message]) -> usize {
    messages
        .iter()
        .map(|message| match message {
            Message::PlainText(text) => text.text.chars().count(),
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

fn send_one_batch(
    adapter_ref: &SharedBotAdapter,
    target_type: &str,
    target_id: &str,
    batch_index: usize,
    messages: &[Message],
) -> Result<SendBatchResult> {
    let params = if target_type == TARGET_TYPE_GROUP {
        serde_json::json!({
            "group_id": target_id,
            "message": qq_message_list_to_json(messages),
        })
    } else {
        serde_json::json!({
            "user_id": target_id,
            "message": qq_message_list_to_json(messages),
        })
    };

    let action_name = if target_type == TARGET_TYPE_GROUP {
        "send_group_msg"
    } else {
        "send_private_msg"
    };

    let response = ws_send_action(adapter_ref, action_name, params)?;
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
    send_qq_message_batches_with_delay(
        adapter_ref,
        target_type,
        target_id,
        batches,
        0,
        DEFAULT_LOG_PREFIX,
    )
}

pub fn message_ids_from_results(results: &[SendBatchResult]) -> Vec<i64> {
    results.iter().map(|result| result.message_id).collect()
}

pub fn actual_sends_all_successful(results: &[SendBatchResult]) -> bool {
    results
        .iter()
        .filter(|result| !result.skipped)
        .all(|result| result.success)
}

pub fn build_send_summary(
    target_type: &str,
    target_id: &str,
    results: &[SendBatchResult],
) -> String {
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

pub fn execute_fixed_target_batch_send(
    inputs: &HashMap<String, DataValue>,
    target_type: &str,
    log_prefix: &str,
) -> Result<HashMap<String, DataValue>> {
    let bot_adapter = match inputs.get("bot_adapter") {
        Some(DataValue::BotAdapterRef(handle)) => crate::bot_adapter::adapter::shared_from_handle(handle),
        _ => return Err(Error::InvalidNodeInput("bot_adapter is required".to_string())),
    };
    let target_id = match inputs.get("target_id") {
        Some(DataValue::String(value)) => value.clone(),
        _ => return Err(Error::InvalidNodeInput("target_id is required".to_string())),
    };
    let batches = qq_message_batches_from_data_value(inputs.get("message_batches"), "message_batches")?;
    let delay_millis = delay_millis_from_data_value(inputs.get("delay_millis"), "delay_millis")?;
    let results = send_qq_message_batches_with_delay(
        &bot_adapter,
        target_type,
        &target_id,
        &batches,
        delay_millis,
        log_prefix,
    );

    Ok(HashMap::from([
        (
            "summary".to_string(),
            DataValue::String(build_send_summary(target_type, &target_id, &results)),
        ),
        (
            "success".to_string(),
            DataValue::Boolean(actual_sends_all_successful(&results)),
        ),
        (
            "message_ids".to_string(),
            DataValue::Vec(
                Box::new(DataType::Integer),
                message_ids_from_results(&results)
                    .into_iter()
                    .map(DataValue::Integer)
                    .collect(),
            ),
        ),
    ]))
}

impl Node for SendQQMessageBatchesNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("将 QQ 消息批次逐批发送到好友或群组，并输出发送汇总")
    }

    node_input![
        port! { name = "bot_adapter_ref", ty = BotAdapterRef, desc = "Bot 适配器引用" },
        port! { name = "target_id", ty = String, desc = "目标 QQ 号或群号" },
        port! { name = "target_type", ty = String, desc = "目标类型：friend 或 group", optional },
        port! { name = "message_batches", ty = Vec(Vec(QQMessage)), desc = "要发送的 QQ 消息批次列表" },
    ];

    node_output![
        port! { name = "summary", ty = String, desc = "已发送消息的一句话总结" },
        port! { name = "success", ty = Boolean, desc = "是否全部发送成功" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let bot_adapter_ref = match inputs.get("bot_adapter_ref") {
            Some(DataValue::BotAdapterRef(handle)) => crate::bot_adapter::adapter::shared_from_handle(handle),
            _ => {
                return Err(Error::InvalidNodeInput(
                    "bot_adapter_ref is required".to_string(),
                ))
            }
        };
        let target_id = match inputs.get("target_id") {
            Some(DataValue::String(value)) => value.clone(),
            _ => return Err(Error::InvalidNodeInput("target_id is required".to_string())),
        };
        let target_type = normalize_target_type(inputs.get("target_type"));
        let batches =
            qq_message_batches_from_data_value(inputs.get("message_batches"), "message_batches")?;
        let results = send_qq_message_batches(
            &bot_adapter_ref,
            target_type,
            &target_id,
            &batches,
        );

        let mut outputs = HashMap::new();
        outputs.insert(
            "summary".to_string(),
            DataValue::String(build_send_summary(target_type, &target_id, &results)),
        );
        outputs.insert(
            "success".to_string(),
            DataValue::Boolean(actual_sends_all_successful(&results)),
        );
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

#[cfg(test)]
pub(crate) fn create_mock_bot_adapter(
    responses: Vec<serde_json::Value>,
) -> Result<(SharedBotAdapter, std::thread::JoinHandle<()>)> {
    use crate::bot_adapter::adapter::{BotAdapter, BotAdapterConfig};
    use serde_json::json;
    use tokio::sync::mpsc;

    let runtime = tokio::runtime::Runtime::new()?;
    let adapter = runtime.block_on(BotAdapter::new(BotAdapterConfig::new(
        "ws://127.0.0.1:12345".to_string(),
        "token".to_string(),
        "10000".to_string(),
    )));
    let adapter_ref = adapter.into_shared();
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    let pending_actions = runtime.block_on(async {
        let mut guard = adapter_ref.lock().await;
        guard.action_tx = Some(tx);
        guard.pending_actions.clone()
    });
    drop(runtime);

    let handle = std::thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new().expect("mock bot runtime should build");
        let mut responses = responses.into_iter();
        while let Some(payload) = rx.blocking_recv() {
            let value: serde_json::Value =
                serde_json::from_str(&payload).expect("payload should be valid json");
            let echo = value
                .get("echo")
                .and_then(|item| item.as_str())
                .expect("echo should exist")
                .to_string();
            let response = responses.next().unwrap_or_else(|| {
                json!({
                    "status": "ok",
                    "retcode": 0,
                    "data": { "message_id": 999 }
                })
            });
            runtime.block_on(async {
                if let Some(sender) = pending_actions.lock().await.remove(&echo) {
                    let _ = sender.send(response);
                }
            });
        }
    });

    Ok((adapter_ref, handle))
}

#[cfg(test)]
mod tests {
    use super::{
        actual_sends_all_successful, build_send_summary, create_mock_bot_adapter,
        describe_message_segments, message_ids_from_results, send_qq_message_batches,
        send_qq_message_batches_with_delay, SendBatchResult, SendQQMessageBatchesNode,
        TARGET_TYPE_GROUP,
    };
    use crate::bot_adapter::models::message::{AtTargetMessage, Message, PlainTextMessage};
    use crate::error::Result;
    use crate::node::{DataType, DataValue, Node};
    use serde_json::json;
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    #[test]
    fn describe_segments_includes_preview() {
        let summary = describe_message_segments(&[
            Message::At(AtTargetMessage {
                target: Some("42".to_string()),
            }),
            Message::PlainText(PlainTextMessage {
                text: "你好".to_string(),
            }),
        ]);
        assert!(summary.contains("segments=2"));
        assert!(summary.contains("at:42"));
        assert!(summary.contains("text:你好"));
    }

    #[test]
    fn summary_includes_success_failure_and_skipped_counts() {
        let summary = build_send_summary(
            "group",
            "42",
            &[
                SendBatchResult {
                    batch_index: 0,
                    success: true,
                    skipped: false,
                    message_id: 100,
                    retcode: Some(0),
                    status: Some("ok".to_string()),
                    wording: None,
                    text_length: 2,
                    segment_count: 2,
                },
                SendBatchResult {
                    batch_index: 1,
                    success: false,
                    skipped: false,
                    message_id: -1,
                    retcode: Some(1),
                    status: Some("failed".to_string()),
                    wording: Some("boom".to_string()),
                    text_length: 4,
                    segment_count: 1,
                },
                SendBatchResult {
                    batch_index: 2,
                    success: true,
                    skipped: true,
                    message_id: -1,
                    retcode: None,
                    status: None,
                    wording: Some("empty batch skipped".to_string()),
                    text_length: 0,
                    segment_count: 0,
                },
            ],
        );
        assert!(summary.contains("成功 1 批"));
        assert!(summary.contains("失败 1 批"));
        assert!(summary.contains("跳过 1 批空消息"));
        assert!(summary.contains("每批文本长度=[2,4,0]"));
    }

    #[test]
    fn send_batches_continues_after_failure() -> Result<()> {
        let (adapter_ref, handle) = create_mock_bot_adapter(vec![
            json!({
                "status": "ok",
                "retcode": 0,
                "data": { "message_id": 11 }
            }),
            json!({
                "status": "failed",
                "retcode": 1,
                "wording": "second failed",
                "data": {}
            }),
            json!({
                "status": "ok",
                "retcode": 0,
                "data": { "message_id": 33 }
            }),
        ])?;

        let results = send_qq_message_batches(
            &adapter_ref,
            TARGET_TYPE_GROUP,
            "123456",
            &[
                vec![
                    Message::At(AtTargetMessage {
                        target: Some("123456".to_string()),
                    }),
                    Message::PlainText(PlainTextMessage {
                        text: "你好".to_string(),
                    }),
                ],
                vec![Message::PlainText(PlainTextMessage {
                    text: "123456".to_string(),
                })],
                vec![Message::PlainText(PlainTextMessage {
                    text: "再见".to_string(),
                })],
            ],
        );

        drop(adapter_ref);
        handle.join().expect("mock bot thread should join");

        assert_eq!(results.len(), 3);
        assert_eq!(results.iter().filter(|result| result.success).count(), 2);
        assert!(!actual_sends_all_successful(&results));
        Ok(())
    }

    #[test]
    fn send_batches_skips_empty_batches_and_keeps_message_ids() -> Result<()> {
        let (adapter_ref, handle) = create_mock_bot_adapter(vec![
            json!({
                "status": "ok",
                "retcode": 0,
                "data": { "message_id": 11 }
            }),
            json!({
                "status": "ok",
                "retcode": 0,
                "data": { "message_id": 22 }
            }),
        ])?;

        let results = send_qq_message_batches_with_delay(
            &adapter_ref,
            TARGET_TYPE_GROUP,
            "123456",
            &[
                vec![Message::PlainText(PlainTextMessage {
                    text: "第一条".to_string(),
                })],
                Vec::new(),
                vec![Message::PlainText(PlainTextMessage {
                    text: "第二条".to_string(),
                })],
            ],
            0,
            "[Test]",
        );

        drop(adapter_ref);
        handle.join().expect("mock bot thread should join");

        assert_eq!(message_ids_from_results(&results), vec![11, -1, 22]);
        assert!(results[1].skipped);
        assert!(actual_sends_all_successful(&results));
        Ok(())
    }

    #[test]
    fn send_batches_respects_delay_between_actual_sends() -> Result<()> {
        let (adapter_ref, handle) = create_mock_bot_adapter(vec![
            json!({
                "status": "ok",
                "retcode": 0,
                "data": { "message_id": 11 }
            }),
            json!({
                "status": "ok",
                "retcode": 0,
                "data": { "message_id": 22 }
            }),
        ])?;

        let started_at = Instant::now();
        let results = send_qq_message_batches_with_delay(
            &adapter_ref,
            TARGET_TYPE_GROUP,
            "123456",
            &[
                vec![Message::PlainText(PlainTextMessage {
                    text: "第一条".to_string(),
                })],
                vec![Message::PlainText(PlainTextMessage {
                    text: "第二条".to_string(),
                })],
            ],
            40,
            "[Test]",
        );
        let elapsed = started_at.elapsed();

        drop(adapter_ref);
        handle.join().expect("mock bot thread should join");

        assert_eq!(results.len(), 2);
        assert!(elapsed >= Duration::from_millis(35));
        Ok(())
    }

    #[test]
    fn execute_accepts_nested_batches() -> Result<()> {
        let (adapter_ref, handle) = create_mock_bot_adapter(vec![json!({
            "status": "ok",
            "retcode": 0,
            "data": { "message_id": 11 }
        })])?;
        let mut node = SendQQMessageBatchesNode::new("send", "Send");
        let outputs = node.execute(HashMap::from([
            (
                "bot_adapter_ref".to_string(),
                DataValue::BotAdapterRef(adapter_ref.clone() as zihuan_bot_types::BotAdapterHandle),
            ),
            (
                "target_id".to_string(),
                DataValue::String("123456".to_string()),
            ),
            (
                "target_type".to_string(),
                DataValue::String("group".to_string()),
            ),
            (
                "message_batches".to_string(),
                DataValue::Vec(
                    Box::new(DataType::Vec(Box::new(DataType::QQMessage))),
                    vec![DataValue::Vec(
                        Box::new(DataType::QQMessage),
                        vec![DataValue::QQMessage(Message::PlainText(PlainTextMessage {
                            text: "你好".to_string(),
                        }))],
                    )],
                ),
            ),
        ]))?;

        drop(adapter_ref);
        handle.join().expect("mock bot thread should join");

        assert!(matches!(
            outputs.get("success"),
            Some(DataValue::Boolean(true))
        ));
        Ok(())
    }
}
