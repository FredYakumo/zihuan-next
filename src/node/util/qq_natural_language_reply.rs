use std::collections::HashMap;

use crate::bot_adapter::send_qq_message_batches::{build_send_summary, send_qq_message_batches};
use crate::error::{Error, Result};
use crate::llm::natural_language_reply::{
    infer_qq_reply_batches, normalize_target_type, TARGET_TYPE_GROUP,
};
use crate::node::{node_input, node_output, DataType, DataValue, Node, Port};

pub struct QQNaturalLanguageReplyNode {
    id: String,
    name: String,
}

impl QQNaturalLanguageReplyNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
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
        Some("兼容节点：先调用 LLM 生成结构化 QQ 回复，再逐批发送到好友或群组")
    }

    node_input![
        port! { name = "content", ty = String, desc = "待修改的原文草稿，将作为第一条 user 消息传给二次 LLM 做润色/改写" },
        port! { name = "character_name", ty = String, desc = "角色名字" },
        port! { name = "target_id", ty = String, desc = "目标 QQ 号或群号" },
        port! { name = "mention_target_id", ty = String, desc = "可选：群聊中优先 @ 的成员 QQ 号，仅用于提示 LLM", optional },
        port! { name = "target_type", ty = String, desc = "目标类型：friend 或 group", optional },
        port! { name = "style", ty = String, desc = "语言风格或情绪" },
        port! { name = "max_one_reply_length", ty = Integer, desc = "单条 plain_text 最多字符数" },
        port! { name = "llm_ref", ty = LLModel, desc = "LLMBase 引用" },
        port! { name = "bot_adapter_ref", ty = BotAdapterRef, desc = "Bot 适配器引用" },
    ];

    node_output![port! { name = "summary", ty = String, desc = "已发送消息的一句话总结" },];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let content = match inputs.get("content") {
            Some(DataValue::String(value)) => value.clone(),
            _ => return Err(Error::InvalidNodeInput("content is required".to_string())),
        };
        let character_name = match inputs.get("character_name") {
            Some(DataValue::String(value)) => value.clone(),
            _ => {
                return Err(Error::InvalidNodeInput(
                    "character_name is required".to_string(),
                ))
            }
        };
        let target_id = match inputs.get("target_id") {
            Some(DataValue::String(value)) => value.clone(),
            _ => return Err(Error::InvalidNodeInput("target_id is required".to_string())),
        };
        let target_type = normalize_target_type(inputs.get("target_type"));
        let mention_target_id = if target_type == TARGET_TYPE_GROUP {
            match inputs.get("mention_target_id") {
                Some(DataValue::String(value)) => {
                    let mention_target_id = value.trim();
                    if mention_target_id.is_empty() {
                        None
                    } else {
                        Some(mention_target_id.to_string())
                    }
                }
                _ => None,
            }
        } else {
            None
        };
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
            _ => {
                return Err(Error::InvalidNodeInput(
                    "bot_adapter_ref is required".to_string(),
                ))
            }
        };

        let inference = infer_qq_reply_batches(
            llm_ref.as_ref(),
            &content,
            &character_name,
            &target_id,
            target_type,
            &style,
            max_one_reply_length,
            mention_target_id.as_deref(),
        )?;
        let batches: Vec<Vec<crate::bot_adapter::models::message::Message>> = inference
            .batches
            .into_iter()
            .map(|batch| batch.messages)
            .collect();
        let results = send_qq_message_batches(&bot_adapter_ref, target_type, &target_id, &batches);
        let summary = build_send_summary(target_type, &target_id, &results);

        let mut outputs = HashMap::new();
        outputs.insert("summary".to_string(), DataValue::String(summary));
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use super::QQNaturalLanguageReplyNode;
    use crate::bot_adapter::adapter::{BotAdapter, BotAdapterConfig, SharedBotAdapter};
    use crate::error::Result;
    use crate::llm::llm_base::LLMBase;
    use crate::llm::{InferenceParam, MessageRole, OpenAIMessage};
    use crate::node::{DataValue, Node};
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use tokio::sync::mpsc;

    #[derive(Debug)]
    struct RecordingLlm {
        response: String,
        seen_messages: Mutex<Vec<OpenAIMessage>>,
    }

    impl LLMBase for RecordingLlm {
        fn get_model_name(&self) -> &str {
            "recording-model"
        }

        fn inference(&self, param: &InferenceParam) -> OpenAIMessage {
            *self
                .seen_messages
                .lock()
                .expect("recording llm mutex should lock") = param.messages.to_vec();
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
        llm_ref: Arc<RecordingLlm>,
        adapter_ref: SharedBotAdapter,
        target_id: &str,
        target_type: &str,
        mention_target_id: Option<&str>,
    ) -> HashMap<String, DataValue> {
        let mut inputs = HashMap::from([
            ("content".to_string(), DataValue::String("你好".to_string())),
            (
                "character_name".to_string(),
                DataValue::String("测试角色".to_string()),
            ),
            (
                "target_id".to_string(),
                DataValue::String(target_id.to_string()),
            ),
            (
                "target_type".to_string(),
                DataValue::String(target_type.to_string()),
            ),
            ("style".to_string(), DataValue::String("温柔".to_string())),
            ("max_one_reply_length".to_string(), DataValue::Integer(8)),
            ("llm_ref".to_string(), DataValue::LLModel(llm_ref)),
            (
                "bot_adapter_ref".to_string(),
                DataValue::BotAdapterRef(adapter_ref),
            ),
        ]);

        if let Some(mention_target_id) = mention_target_id {
            inputs.insert(
                "mention_target_id".to_string(),
                DataValue::String(mention_target_id.to_string()),
            );
        }

        inputs
    }

    #[test]
    fn mention_target_id_port_is_optional() {
        let node = QQNaturalLanguageReplyNode::new("reply", "Reply");
        let port = node
            .input_ports()
            .into_iter()
            .find(|port| port.name == "mention_target_id")
            .expect("mention_target_id port should exist");

        assert!(!port.required);
    }

    #[test]
    fn execute_passes_group_mention_target_to_prompt() -> Result<()> {
        let llm = Arc::new(RecordingLlm {
            response: r#"[{"message_type":"plain_text","content":"你好"}]"#.to_string(),
            seen_messages: Mutex::new(Vec::new()),
        });
        let (adapter_ref, handle) = create_mock_bot_adapter(vec![json!({
            "status": "ok",
            "retcode": 0,
            "data": { "message_id": 11 }
        })])?;

        let mut node = QQNaturalLanguageReplyNode::new("reply", "Reply");
        let outputs = node.execute(base_inputs(
            llm.clone(),
            adapter_ref.clone(),
            "987654",
            "group",
            Some("123456"),
        ))?;

        drop(adapter_ref);
        handle.join().expect("mock bot thread should join");

        assert!(matches!(outputs.get("summary"), Some(DataValue::String(_))));

        let seen_messages = llm
            .seen_messages
            .lock()
            .expect("recording llm mutex should lock")
            .clone();
        let system_prompt = seen_messages
            .first()
            .and_then(|message| message.content.as_deref())
            .expect("system prompt should exist");

        assert!(system_prompt.contains("本次优先回复给群成员 123456"));
        assert!(system_prompt.contains("@ 后面必须保留一个半角空格"));
        Ok(())
    }

    #[test]
    fn execute_ignores_mention_target_for_friend_target() -> Result<()> {
        let llm = Arc::new(RecordingLlm {
            response: r#"[{"message_type":"plain_text","content":"你好"}]"#.to_string(),
            seen_messages: Mutex::new(Vec::new()),
        });
        let (adapter_ref, handle) = create_mock_bot_adapter(vec![json!({
            "status": "ok",
            "retcode": 0,
            "data": { "message_id": 22 }
        })])?;

        let mut node = QQNaturalLanguageReplyNode::new("reply", "Reply");
        node.execute(base_inputs(
            llm.clone(),
            adapter_ref.clone(),
            "123456",
            "friend",
            Some("999999"),
        ))?;

        drop(adapter_ref);
        handle.join().expect("mock bot thread should join");

        let seen_messages = llm
            .seen_messages
            .lock()
            .expect("recording llm mutex should lock")
            .clone();
        let system_prompt = seen_messages
            .first()
            .and_then(|message| message.content.as_deref())
            .expect("system prompt should exist");

        assert!(!system_prompt.contains("群聊定向回复补充规则"));
        assert!(!system_prompt.contains("999999"));
        Ok(())
    }
}
