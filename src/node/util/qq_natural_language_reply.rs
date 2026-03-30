use std::collections::HashMap;

use crate::bot_adapter::models::message::{AtTargetMessage, Message, PlainTextMessage};
use crate::bot_adapter::ws_action::{
    json_i64, qq_message_list_to_json, response_message_id, response_success, ws_send_action,
};
use crate::error::{Error, Result};
use crate::llm::{InferenceParam, OpenAIMessage};
use crate::node::{node_input, node_output, DataType, DataValue, Node, Port};
use log::{info, warn};
use serde::Deserialize;

const TARGET_TYPE_FRIEND: &str = "friend";
const TARGET_TYPE_GROUP: &str = "group";

pub struct QQNaturalLanguageReplyNode {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "message_type", rename_all = "snake_case")]
enum NaturalReplyItem {
    PlainText { content: String },
    CombineText { content_list: Vec<NaturalReplyContentItem> },
    At { target: String },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "message_type", rename_all = "snake_case")]
enum NaturalReplyContentItem {
    PlainText { content: String },
    At { target: String },
}

#[derive(Debug, Clone)]
struct SendBatch {
    messages: Vec<Message>,
    text_length: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SendBatchResult {
    batch_index: usize,
    success: bool,
    message_id: i64,
    retcode: Option<i64>,
    status: Option<String>,
    wording: Option<String>,
    text_length: usize,
    segment_count: usize,
}

impl QQNaturalLanguageReplyNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }

    fn normalize_target_type(value: Option<&DataValue>) -> &'static str {
        match value {
            Some(DataValue::String(target_type)) if target_type.eq_ignore_ascii_case(TARGET_TYPE_GROUP) => {
                TARGET_TYPE_GROUP
            }
            _ => TARGET_TYPE_FRIEND,
        }
    }

    fn build_system_prompt(
        character_name: &str,
        style: &str,
        target_id: &str,
        target_type: &str,
        max_one_reply_length: usize,
    ) -> String {
        let target_type_label = if target_type == TARGET_TYPE_GROUP { "群聊" } else { "好友私聊" };
        format!(
            concat!(
                "你要扮演 QQ 角色“{character_name}”，用 {style} 的语言风格生成自然、真实、可直接发送的 QQ 回复。\n",
                "当前回复目标类型：{target_type_label}；目标 ID：{target_id}。\n",
                "用户输入会作为第一条 user 消息提供给你。\n",
                "你必须只输出纯 JSON 数组，不能输出 markdown、代码块、解释、前后缀文本。\n",
                "数组元素支持三种 message_type：\n",
                "1. plain_text: {{\"message_type\":\"plain_text\",\"content\":\"文本\"}}\n",
                "2. at: {{\"message_type\":\"at\",\"target\":\"QQ号\"}}\n",
                "3. combine_text: {{\"message_type\":\"combine_text\",\"content_list\":[上面允许的 at/plain_text 对象列表]}}\n",
                "规则：\n",
                "- plain_text.content 必须是非空字符串，且尽量不超过 {max_one_reply_length} 个字符。\n",
                "- 如果回复较长，请主动拆成多个数组元素。\n",
                "- combine_text 表示这些消息段要在一次发送里组合发送；不要在 combine_text 里嵌套 combine_text。\n",
                "- 如果需要形成更真实的“@某人 文本”效果，请使用 combine_text，并把 at 放在文本前面。\n",
                "- 顶层 at 只允许用于群聊场景。\n",
                "- combine_text 里至少包含一个 plain_text，避免只发纯 @。\n",
                "- 如果你要输出“@某人 文本”且文本较长，请自行拆成多个 combine_text，而不是输出一个超长 combine_text。\n",
                "- 除 JSON 数组外不要输出任何其他内容。"
            ),
            character_name = character_name,
            style = style,
            target_type_label = target_type_label,
            target_id = target_id,
            max_one_reply_length = max_one_reply_length,
        )
    }

    fn split_plain_text(content: &str, max_one_reply_length: usize) -> Vec<String> {
        if content.is_empty() {
            return Vec::new();
        }
        if content.chars().count() <= max_one_reply_length {
            return vec![content.to_string()];
        }

        let mut chunks = Vec::new();
        let mut current = String::new();
        let mut current_len = 0usize;
        for ch in content.chars() {
            current.push(ch);
            current_len += 1;
            if current_len >= max_one_reply_length {
                chunks.push(std::mem::take(&mut current));
                current_len = 0;
            }
        }
        if !current.is_empty() {
            chunks.push(current);
        }
        chunks
    }

    fn parse_reply_items(content: &str) -> Result<Vec<NaturalReplyItem>> {
        let items: Vec<NaturalReplyItem> = serde_json::from_str(content)?;
        if items.is_empty() {
            return Err(Error::ValidationError(
                "LLM reply JSON array must not be empty".to_string(),
            ));
        }
        Ok(items)
    }

    fn convert_to_batches(
        items: Vec<NaturalReplyItem>,
        target_type: &str,
        max_one_reply_length: usize,
    ) -> Result<Vec<SendBatch>> {
        let mut batches = Vec::new();

        for item in items {
            match item {
                NaturalReplyItem::PlainText { content } => {
                    let content = content.trim().to_string();
                    if content.is_empty() {
                        return Err(Error::ValidationError(
                            "plain_text.content must not be empty".to_string(),
                        ));
                    }
                    for chunk in Self::split_plain_text(&content, max_one_reply_length) {
                        batches.push(SendBatch {
                            text_length: chunk.chars().count(),
                            messages: vec![Message::PlainText(PlainTextMessage { text: chunk })],
                        });
                    }
                }
                NaturalReplyItem::At { target } => {
                    if target_type != TARGET_TYPE_GROUP {
                        return Err(Error::ValidationError(
                            "top-level at is only allowed for group target".to_string(),
                        ));
                    }
                    let target = target.trim().to_string();
                    if target.is_empty() {
                        return Err(Error::ValidationError(
                            "at.target must not be empty".to_string(),
                        ));
                    }
                    batches.push(SendBatch {
                        text_length: 0,
                        messages: vec![Message::At(AtTargetMessage {
                            target: Some(target),
                        })],
                    });
                }
                NaturalReplyItem::CombineText { content_list } => {
                    if content_list.is_empty() {
                        return Err(Error::ValidationError(
                            "combine_text.content_list must not be empty".to_string(),
                        ));
                    }

                    let mut messages = Vec::new();
                    let mut text_length = 0usize;
                    let mut contains_plain_text = false;

                    for content_item in content_list {
                        match content_item {
                            NaturalReplyContentItem::PlainText { content } => {
                                let content = content.trim().to_string();
                                if content.is_empty() {
                                    return Err(Error::ValidationError(
                                        "combine_text plain_text.content must not be empty"
                                            .to_string(),
                                    ));
                                }
                                text_length += content.chars().count();
                                contains_plain_text = true;
                                messages.push(Message::PlainText(PlainTextMessage { text: content }));
                            }
                            NaturalReplyContentItem::At { target } => {
                                if target_type != TARGET_TYPE_GROUP {
                                    return Err(Error::ValidationError(
                                        "at inside combine_text is only allowed for group target"
                                            .to_string(),
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

                    if !contains_plain_text {
                        return Err(Error::ValidationError(
                            "combine_text must contain at least one plain_text item".to_string(),
                        ));
                    }

                    batches.push(SendBatch { messages, text_length });
                }
            }
        }

        Ok(batches)
    }

    fn send_batch(
        adapter_ref: &crate::bot_adapter::adapter::SharedBotAdapter,
        target_type: &str,
        target_id: &str,
        batch_index: usize,
        batch: &SendBatch,
    ) -> Result<SendBatchResult> {
        let params = if target_type == TARGET_TYPE_GROUP {
            serde_json::json!({
                "group_id": target_id,
                "message": qq_message_list_to_json(&batch.messages),
            })
        } else {
            serde_json::json!({
                "user_id": target_id,
                "message": qq_message_list_to_json(&batch.messages),
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
            text_length: batch.text_length,
            segment_count: batch.messages.len(),
        })
    }

    fn build_summary(
        target_type: &str,
        target_id: &str,
        results: &[SendBatchResult],
    ) -> String {
        let success_count = results.iter().filter(|result| result.success).count();
        let failure_count = results.len().saturating_sub(success_count);
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
        let failed_batches = results
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
        let overall = if failure_count == 0 {
            "全部发送成功"
        } else if success_count == 0 {
            "全部发送失败"
        } else {
            "部分发送失败"
        };

        if failed_batches.is_empty() {
            format!(
                "{overall}，目标={target_type}:{target_id}，共发送 {total} 批，成功 {success_count} 批，失败 {failure_count} 批，每批文本长度=[{lengths}]，每批消息段数=[{segment_counts}]。",
                total = results.len()
            )
        } else {
            format!(
                "{overall}，目标={target_type}:{target_id}，共发送 {total} 批，成功 {success_count} 批，失败 {failure_count} 批，每批文本长度=[{lengths}]，每批消息段数=[{segment_counts}]，失败批次={failed_batches}。",
                total = results.len()
            )
        }
    }
}

impl Node for QQNaturalLanguageReplyNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("调用 LLM 生成结构化 QQ 自然语言回复，并按好友/群组目标逐批发送")
    }

    node_input![
        port! { name = "content", ty = String, desc = "用户输入内容，将作为第一条 user 消息" },
        port! { name = "character_name", ty = String, desc = "角色名字" },
        port! { name = "target_id", ty = String, desc = "目标 QQ 号或群号" },
        port! { name = "target_type", ty = String, desc = "目标类型：friend 或 group", optional },
        port! { name = "style", ty = String, desc = "语言风格或情绪" },
        port! { name = "max_one_reply_length", ty = Integer, desc = "单条 plain_text 最多字符数" },
        port! { name = "llm_ref", ty = LLModel, desc = "LLMBase 引用" },
        port! { name = "bot_adapter_ref", ty = BotAdapterRef, desc = "Bot 适配器引用" },
    ];

    node_output![
        port! { name = "summary", ty = String, desc = "已发送消息的一句话总结" },
    ];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let content = match inputs.get("content") {
            Some(DataValue::String(value)) => value.clone(),
            _ => return Err(Error::InvalidNodeInput("content is required".to_string())),
        };
        let character_name = match inputs.get("character_name") {
            Some(DataValue::String(value)) => value.clone(),
            _ => return Err(Error::InvalidNodeInput("character_name is required".to_string())),
        };
        let target_id = match inputs.get("target_id") {
            Some(DataValue::String(value)) => value.clone(),
            _ => return Err(Error::InvalidNodeInput("target_id is required".to_string())),
        };
        let target_type = Self::normalize_target_type(inputs.get("target_type"));
        let style = match inputs.get("style") {
            Some(DataValue::String(value)) => value.clone(),
            _ => return Err(Error::InvalidNodeInput("style is required".to_string())),
        };
        let max_one_reply_length = match inputs.get("max_one_reply_length") {
            Some(DataValue::Integer(value)) if *value > 0 => *value as usize,
            Some(DataValue::Integer(_)) => {
                return Err(Error::ValidationError(
                    "max_one_reply_length must be greater than 0".to_string(),
                ))
            }
            _ => {
                return Err(Error::InvalidNodeInput(
                    "max_one_reply_length is required".to_string(),
                ))
            }
        };
        let llm_ref = match inputs.get("llm_ref") {
            Some(DataValue::LLModel(value)) => value.clone(),
            _ => return Err(Error::InvalidNodeInput("llm_ref is required".to_string())),
        };
        let bot_adapter_ref = match inputs.get("bot_adapter_ref") {
            Some(DataValue::BotAdapterRef(value)) => value.clone(),
            _ => return Err(Error::InvalidNodeInput("bot_adapter_ref is required".to_string())),
        };

        let messages = vec![
            OpenAIMessage::system(Self::build_system_prompt(
                &character_name,
                &style,
                &target_id,
                target_type,
                max_one_reply_length,
            )),
            OpenAIMessage::user(content),
        ];

        let reply = llm_ref.inference(&InferenceParam {
            messages: &messages,
            tools: None,
        });
        let reply_content = reply.content.ok_or_else(|| {
            Error::ValidationError("LLM response content is empty".to_string())
        })?;

        let items = Self::parse_reply_items(&reply_content)?;
        let batches = Self::convert_to_batches(items, target_type, max_one_reply_length)?;

        let mut results = Vec::with_capacity(batches.len());
        for (index, batch) in batches.iter().enumerate() {
            match Self::send_batch(&bot_adapter_ref, target_type, &target_id, index, batch) {
                Ok(result) => {
                    if result.success {
                        info!(
                            "[QQNaturalLanguageReplyNode] Sent batch {} to {}:{} (message_id={}, segments={}, text_length={})",
                            index + 1,
                            target_type,
                            target_id,
                            result.message_id,
                            result.segment_count,
                            result.text_length
                        );
                    } else {
                        warn!(
                            "[QQNaturalLanguageReplyNode] Failed to send batch {} to {}:{} (message_id={}, retcode={:?}, status={:?}, wording={:?})",
                            index + 1,
                            target_type,
                            target_id,
                            result.message_id,
                            result.retcode,
                            result.status,
                            result.wording
                        );
                    }
                    results.push(result);
                }
                Err(err) => {
                    warn!(
                        "[QQNaturalLanguageReplyNode] Error sending batch {} to {}:{}: {}",
                        index + 1,
                        target_type,
                        target_id,
                        err
                    );
                    results.push(SendBatchResult {
                        batch_index: index,
                        success: false,
                        message_id: -1,
                        retcode: None,
                        status: None,
                        wording: Some(err.to_string()),
                        text_length: batch.text_length,
                        segment_count: batch.messages.len(),
                    });
                }
            }
        }

        let summary = Self::build_summary(target_type, &target_id, &results);
        let mut outputs = HashMap::new();
        outputs.insert("summary".to_string(), DataValue::String(summary));
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use super::{QQNaturalLanguageReplyNode, SendBatchResult, TARGET_TYPE_FRIEND, TARGET_TYPE_GROUP};
    use crate::bot_adapter::adapter::{BotAdapter, BotAdapterConfig, SharedBotAdapter};
    use crate::error::Result;
    use crate::llm::llm_base::LLMBase;
    use crate::llm::{InferenceParam, MessageRole, OpenAIMessage};
    use crate::node::graph_io::{refresh_port_types, NodeDefinition, NodeGraphDefinition};
    use crate::node::registry::{build_node_graph_from_definition, init_node_registry};
    use crate::node::{DataType, DataValue, Node, Port};
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    #[derive(Debug)]
    struct StubLlm {
        response: String,
    }

    impl LLMBase for StubLlm {
        fn get_model_name(&self) -> &str {
            "stub"
        }

        fn inference(&self, _param: &InferenceParam) -> OpenAIMessage {
            OpenAIMessage {
                role: MessageRole::Assistant,
                content: Some(self.response.clone()),
                tool_calls: Vec::new(),
                tool_call_id: None,
            }
        }
    }

    fn create_mock_bot_adapter(
        responses: Vec<serde_json::Value>,
    ) -> Result<(SharedBotAdapter, std::thread::JoinHandle<()>)> {
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

    fn base_inputs(
        llm_response: &str,
        adapter_ref: SharedBotAdapter,
    ) -> HashMap<String, DataValue> {
        HashMap::from([
            ("content".to_string(), DataValue::String("你好".to_string())),
            (
                "character_name".to_string(),
                DataValue::String("测试角色".to_string()),
            ),
            ("target_id".to_string(), DataValue::String("123456".to_string())),
            (
                "target_type".to_string(),
                DataValue::String(TARGET_TYPE_GROUP.to_string()),
            ),
            ("style".to_string(), DataValue::String("温柔".to_string())),
            (
                "max_one_reply_length".to_string(),
                DataValue::Integer(5),
            ),
            (
                "llm_ref".to_string(),
                DataValue::LLModel(Arc::new(StubLlm {
                    response: llm_response.to_string(),
                })),
            ),
            (
                "bot_adapter_ref".to_string(),
                DataValue::BotAdapterRef(adapter_ref),
            ),
        ])
    }

    #[test]
    fn target_type_port_is_optional_and_defaults_to_friend() {
        let node = QQNaturalLanguageReplyNode::new("n1", "reply");
        let port = node
            .input_ports()
            .into_iter()
            .find(|port| port.name == "target_type")
            .expect("target_type port should exist");
        assert!(!port.required);
        assert_eq!(
            QQNaturalLanguageReplyNode::normalize_target_type(None),
            TARGET_TYPE_FRIEND
        );
    }

    #[test]
    fn parse_plain_text_into_single_batch() -> Result<()> {
        let items = QQNaturalLanguageReplyNode::parse_reply_items(
            r#"[{"message_type":"plain_text","content":"你好呀"}]"#,
        )?;
        let batches = QQNaturalLanguageReplyNode::convert_to_batches(items, TARGET_TYPE_GROUP, 10)?;
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].text_length, 3);
        assert_eq!(batches[0].messages.len(), 1);
        match &batches[0].messages[0] {
            crate::bot_adapter::models::message::Message::PlainText(message) => {
                assert_eq!(message.text, "你好呀");
            }
            other => panic!("unexpected batch message: {other:?}"),
        }
        Ok(())
    }

    #[test]
    fn split_top_level_plain_text_by_max_length() -> Result<()> {
        let items = QQNaturalLanguageReplyNode::parse_reply_items(
            r#"[{"message_type":"plain_text","content":"123456789"}]"#,
        )?;
        let batches = QQNaturalLanguageReplyNode::convert_to_batches(items, TARGET_TYPE_GROUP, 4)?;
        let lengths: Vec<usize> = batches.iter().map(|batch| batch.text_length).collect();
        assert_eq!(lengths, vec![4, 4, 1]);
        Ok(())
    }

    #[test]
    fn combine_text_keeps_at_and_text_in_same_batch() -> Result<()> {
        let items = QQNaturalLanguageReplyNode::parse_reply_items(
            r#"[{"message_type":"combine_text","content_list":[{"message_type":"at","target":"42"},{"message_type":"plain_text","content":"你好"}]}]"#,
        )?;
        let batches = QQNaturalLanguageReplyNode::convert_to_batches(items, TARGET_TYPE_GROUP, 4)?;
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].messages.len(), 2);
        assert_eq!(batches[0].text_length, 2);
        Ok(())
    }

    #[test]
    fn combine_text_preserves_inner_order() -> Result<()> {
        let items = QQNaturalLanguageReplyNode::parse_reply_items(
            r#"[{"message_type":"combine_text","content_list":[{"message_type":"plain_text","content":"A"},{"message_type":"at","target":"42"},{"message_type":"plain_text","content":"B"}]}]"#,
        )?;
        let batches = QQNaturalLanguageReplyNode::convert_to_batches(items, TARGET_TYPE_GROUP, 8)?;
        assert_eq!(batches.len(), 1);
        assert!(matches!(
            &batches[0].messages[0],
            crate::bot_adapter::models::message::Message::PlainText(_)
        ));
        assert!(matches!(
            &batches[0].messages[1],
            crate::bot_adapter::models::message::Message::At(_)
        ));
        assert!(matches!(
            &batches[0].messages[2],
            crate::bot_adapter::models::message::Message::PlainText(_)
        ));
        Ok(())
    }

    #[test]
    fn combine_text_without_plain_text_is_rejected() -> Result<()> {
        let items = QQNaturalLanguageReplyNode::parse_reply_items(
            r#"[{"message_type":"combine_text","content_list":[{"message_type":"at","target":"42"}]}]"#,
        )?;
        let err = QQNaturalLanguageReplyNode::convert_to_batches(items, TARGET_TYPE_GROUP, 8)
            .expect_err("combine_text without plain text should fail");
        assert!(err.to_string().contains("at least one plain_text"));
        Ok(())
    }

    #[test]
    fn nested_combine_text_is_rejected() {
        let err = QQNaturalLanguageReplyNode::parse_reply_items(
            r#"[{"message_type":"combine_text","content_list":[{"message_type":"combine_text","content_list":[{"message_type":"plain_text","content":"hi"}]}]}]"#,
        )
        .expect_err("nested combine_text should fail");
        assert!(err.to_string().contains("unknown variant"));
    }

    #[test]
    fn friend_target_rejects_at() -> Result<()> {
        let items = QQNaturalLanguageReplyNode::parse_reply_items(
            r#"[{"message_type":"at","target":"42"}]"#,
        )?;
        let err = QQNaturalLanguageReplyNode::convert_to_batches(items, TARGET_TYPE_FRIEND, 8)
            .expect_err("friend target should reject at");
        assert!(err.to_string().contains("only allowed for group"));
        Ok(())
    }

    #[test]
    fn invalid_json_and_empty_array_are_rejected() {
        assert!(QQNaturalLanguageReplyNode::parse_reply_items("oops").is_err());
        assert!(QQNaturalLanguageReplyNode::parse_reply_items("[]").is_err());
    }

    #[test]
    fn summary_includes_success_and_failure_counts() {
        let summary = QQNaturalLanguageReplyNode::build_summary(
            TARGET_TYPE_GROUP,
            "42",
            &[
                SendBatchResult {
                    batch_index: 0,
                    success: true,
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
                    message_id: -1,
                    retcode: Some(1),
                    status: Some("failed".to_string()),
                    wording: Some("boom".to_string()),
                    text_length: 4,
                    segment_count: 1,
                },
            ],
        );
        assert!(summary.contains("成功 1 批"));
        assert!(summary.contains("失败 1 批"));
        assert!(summary.contains("每批文本长度=[2,4]"));
    }

    #[test]
    fn execute_sends_batches_and_continues_after_failure() -> Result<()> {
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
        let mut node = QQNaturalLanguageReplyNode::new("reply", "Reply");
        let outputs = node.execute(base_inputs(
            r#"[
                {"message_type":"combine_text","content_list":[
                    {"message_type":"at","target":"123456"},
                    {"message_type":"plain_text","content":"你好"}
                ]},
                {"message_type":"plain_text","content":"123456"},
                {"message_type":"plain_text","content":"再见"}
            ]"#,
            adapter_ref.clone(),
        ))?;
        drop(adapter_ref);
        handle.join().expect("mock bot thread should join");

        let summary = match outputs.get("summary") {
            Some(DataValue::String(summary)) => summary,
            other => panic!("unexpected summary output: {other:?}"),
        };
        assert!(summary.contains("共发送 4 批"));
        assert!(summary.contains("成功 3 批"));
        assert!(summary.contains("失败 1 批"));
        Ok(())
    }

    #[test]
    fn max_one_reply_length_must_be_positive() -> Result<()> {
        let (adapter_ref, handle) = create_mock_bot_adapter(vec![])?;
        let mut node = QQNaturalLanguageReplyNode::new("reply", "Reply");
        let mut inputs = base_inputs(
            r#"[{"message_type":"plain_text","content":"你好"}]"#,
            adapter_ref.clone(),
        );
        inputs.insert("max_one_reply_length".to_string(), DataValue::Integer(0));
        let err = node.execute(inputs).expect_err("zero max length should fail");
        drop(adapter_ref);
        handle.join().expect("mock bot thread should join");
        assert!(err.to_string().contains("greater than 0"));
        Ok(())
    }

    #[test]
    fn registry_and_graph_json_restore_target_type() -> Result<()> {
        init_node_registry()?;

        let mut graph = NodeGraphDefinition {
            nodes: vec![NodeDefinition {
                id: "reply_1".to_string(),
                name: "Reply".to_string(),
                description: None,
                node_type: "qq_natural_language_reply".to_string(),
                input_ports: vec![Port::new("content", DataType::String)],
                output_ports: vec![Port::new("summary", DataType::String)],
                dynamic_input_ports: false,
                dynamic_output_ports: false,
                position: None,
                size: None,
                inline_values: HashMap::from([(
                    "target_type".to_string(),
                    json!("group"),
                )]),
                port_bindings: HashMap::new(),
                has_error: false,
                has_cycle: false,
            }],
            edges: vec![],
            hyperparameter_groups: vec![],
            hyperparameters: vec![],
            execution_results: HashMap::new(),
        };

        refresh_port_types(&mut graph);
        let node = &graph.nodes[0];
        assert!(node.input_ports.iter().any(|port| port.name == "target_type"));
        build_node_graph_from_definition(&graph)?;
        Ok(())
    }
}
