use crate::{node_input, node_output, DataType, DataValue, Node, NodeType, Port};
use log::error;
use std::collections::HashMap;
use zihuan_core::error::{Error, Result};

pub struct ImageWeaviatePersistenceNode {
    id: String,
    name: String,
}

impl ImageWeaviatePersistenceNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for ImageWeaviatePersistenceNode {
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
        Some("图片向量持久化 - 将对象存储路径 + 向量 + 总结写入 Weaviate")
    }

    node_input![
        port! { name = "object_storage_path", ty = String, desc = "对象存储路径（object_key/object_url）" },
        port! { name = "vector", ty = Vector, desc = "图片语义向量" },
        port! { name = "summary", ty = String, desc = "图片总结说明" },
        port! { name = "weaviate_ref", ty = crate::database::weaviate::WeaviateRef, desc = "Weaviate连接配置引用" },
        port! { name = "source", ty = String, desc = "可选：图片来源（qq/tavily等）", optional },
        port! { name = "message_id", ty = String, desc = "可选：关联消息ID", optional },
        port! { name = "sender_id", ty = String, desc = "可选：发送者ID", optional },
    ];

    node_output![
        port! { name = "success", ty = Boolean, desc = "是否存储成功" },
        port! { name = "object_storage_path", ty = String, desc = "透传对象存储路径" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let object_storage_path = required_string(&inputs, "object_storage_path")?;
        let summary = required_string(&inputs, "summary")?;
        let vector = inputs
            .get("vector")
            .and_then(|v| match v {
                DataValue::Vector(value) => Some(value.clone()),
                _ => None,
            })
            .ok_or_else(|| Error::InvalidNodeInput("vector is required".to_string()))?;

        if vector.is_empty() {
            return Err(Error::ValidationError("vector must not be empty".to_string()));
        }

        let weaviate_ref = inputs
            .get("weaviate_ref")
            .and_then(|v| match v {
                DataValue::WeaviateRef(r) => Some(r.clone()),
                _ => None,
            })
            .ok_or_else(|| Error::InvalidNodeInput("weaviate_ref is required".to_string()))?;

        let source = optional_non_empty_string(&inputs, "source");
        let message_id = optional_non_empty_string(&inputs, "message_id");
        let sender_id = optional_non_empty_string(&inputs, "sender_id");

        let success = match weaviate_ref.upsert_image_record(
            &object_storage_path,
            &summary,
            &vector,
            source.as_deref(),
            message_id.as_deref(),
            sender_id.as_deref(),
        ) {
            Ok(_) => true,
            Err(err) => {
                error!(
                    "[ImageWeaviatePersistenceNode] Failed to persist image vector: {}",
                    err
                );
                false
            }
        };

        let mut outputs = HashMap::new();
        outputs.insert("success".to_string(), DataValue::Boolean(success));
        outputs.insert(
            "object_storage_path".to_string(),
            DataValue::String(object_storage_path),
        );
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
