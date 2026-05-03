use crate::{node_input, node_output, DataType, DataValue, Node, NodeType, Port};
use log::error;
use std::collections::HashMap;
use zihuan_bot_types::message::Message;
use zihuan_core::error::{Error, Result};

pub struct QQMessageListWeaviatePersistenceNode {
    id: String,
    name: String,
}

impl QQMessageListWeaviatePersistenceNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for QQMessageListWeaviatePersistenceNode {
    fn node_type(&self) -> NodeType {
        NodeType::Simple
    }

    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("QQMessage列表向量持久化 - 将Vec<QQMessage>及元数据向量化后存储到Weaviate数据库")
    }

    node_input![
        port! { name = "qq_message_list", ty = Vec(QQMessage), desc = "要持久化的QQ消息列表" },
        port! { name = "message_id", ty = String, desc = "消息ID" },
        port! { name = "sender_id", ty = String, desc = "发送者ID" },
        port! { name = "sender_name", ty = String, desc = "发送者名称" },
        port! { name = "group_id", ty = String, desc = "群ID（可选）", optional },
        port! { name = "group_name", ty = String, desc = "群名称（可选）", optional },
        port! { name = "weaviate_ref", ty = crate::database::weaviate::WeaviateRef, desc = "Weaviate连接配置引用" },
        port! { name = "embedding_model", ty = EmbeddingModel, desc = "Embedding模型引用" },
    ];

    node_output![
        port! { name = "success", ty = Boolean, desc = "是否存储成功" },
        port! { name = "qq_message_list", ty = Vec(QQMessage), desc = "透传输入的消息列表" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        let (msg_item_type, msg_items) = inputs
            .get("qq_message_list")
            .and_then(|v| match v {
                DataValue::Vec(ty, items) => Some((ty.clone(), items.clone())),
                _ => None,
            })
            .ok_or_else(|| Error::InvalidNodeInput("qq_message_list is required".to_string()))?;

        let message_id = required_string(&inputs, "message_id")?;
        let sender_id = required_string(&inputs, "sender_id")?;
        let sender_name = required_string(&inputs, "sender_name")?;
        let group_id = optional_non_empty_string(&inputs, "group_id");
        let group_name = optional_non_empty_string(&inputs, "group_name");

        let weaviate_ref = inputs
            .get("weaviate_ref")
            .and_then(|v| match v {
                DataValue::WeaviateRef(r) => Some(r.clone()),
                _ => None,
            })
            .ok_or_else(|| Error::InvalidNodeInput("weaviate_ref is required".to_string()))?;

        let embedding_model = inputs
            .get("embedding_model")
            .and_then(|v| match v {
                DataValue::EmbeddingModel(model) => Some(model.clone()),
                _ => None,
            })
            .ok_or_else(|| Error::InvalidNodeInput("embedding_model is required".to_string()))?;

        let passthrough = DataValue::Vec(msg_item_type, msg_items.clone());

        let messages: Vec<Message> = msg_items
            .iter()
            .filter_map(|v| match v {
                DataValue::QQMessage(m) => Some(m.clone()),
                _ => None,
            })
            .collect();

        let success = match weaviate_ref.upsert_qq_message_list(
            &messages,
            &message_id,
            &sender_id,
            &sender_name,
            group_id.as_deref(),
            group_name.as_deref(),
            embedding_model.as_ref(),
        ) {
            Ok(_) => true,
            Err(err) => {
                error!(
                    "[QQMessageListWeaviatePersistenceNode] Failed to persist message vector: {}",
                    err
                );
                false
            }
        };

        let mut outputs = HashMap::new();
        outputs.insert("success".to_string(), DataValue::Boolean(success));
        outputs.insert("qq_message_list".to_string(), passthrough);
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

fn required_string(inputs: &HashMap<String, DataValue>, key: &str) -> Result<String> {
    let value = inputs
        .get(key)
        .and_then(|v| match v {
            DataValue::String(s) => Some(s.trim().to_string()),
            _ => None,
        })
        .ok_or_else(|| Error::InvalidNodeInput(format!("{key} is required")))?;

    if value.is_empty() {
        return Err(Error::ValidationError(format!("{key} must not be empty")));
    }

    Ok(value)
}

fn optional_non_empty_string(inputs: &HashMap<String, DataValue>, key: &str) -> Option<String> {
    inputs.get(key).and_then(|v| match v {
        DataValue::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        _ => None,
    })
}
