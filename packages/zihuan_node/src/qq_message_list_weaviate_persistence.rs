use crate::{node_input, node_output, DataType, DataValue, Node, NodeType, Port};
use log::{error, info};
use serde_json::json;
use std::collections::HashMap;
use uuid::Uuid;
use zihuan_bot_types::message::{collect_media_records, Message};
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
        port! { name = "weaviate_ref", ty = WeaviateRef, desc = "Weaviate连接配置引用" },
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

        let content = messages
            .iter()
            .map(|m| m.to_string())
            .collect::<Vec<_>>()
            .join("");
        if content.trim().is_empty() {
            return Err(Error::ValidationError(
                "qq_message_list content must not be empty".to_string(),
            ));
        }

        let at_targets: Vec<String> = messages
            .iter()
            .filter_map(|m| {
                if let Message::At(at) = m {
                    Some(at.target_id())
                } else {
                    None
                }
            })
            .collect();
        let at_target_list = if at_targets.is_empty() {
            None
        } else {
            Some(at_targets.join(","))
        };

        let media_json = {
            let records = collect_media_records(&messages);
            if records.is_empty() {
                None
            } else {
                Some(serde_json::to_string(&records)?)
            }
        };

        let send_time = chrono::Local::now().to_rfc3339();
        let vector = embedding_model.inference(&content)?;
        let object_id = Uuid::new_v4().to_string();

        let properties = json!({
            "message_id": message_id,
            "sender_id": sender_id,
            "sender_name": sender_name,
            "send_time": send_time,
            "group_id": group_id,
            "group_name": group_name,
            "content": content,
            "at_target_list": at_target_list,
            "media_json": media_json,
        });

        info!(
            "[QQMessageListWeaviatePersistenceNode] Inserting message {} into Weaviate class {}",
            properties["message_id"].as_str().unwrap_or_default(),
            weaviate_ref.class_name
        );

        let success = match weaviate_ref.upsert_object(
            &weaviate_ref.class_name,
            properties,
            Some(vector),
            Some(&object_id),
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
