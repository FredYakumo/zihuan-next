use crate::database::weaviate::{WeaviateCollectionConfig, WeaviatePropertyConfig, WeaviateRef};
use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use zihuan_core::error::{Error, Result};

const DEFAULT_TIMEOUT_SECS: u64 = 30;

pub struct WeaviateImageCollectionNode {
    id: String,
    name: String,
}

impl WeaviateImageCollectionNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for WeaviateImageCollectionNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("Weaviate 图片 collection 配置，输出 WeaviateRef 引用供图片检索/持久化节点复用")
    }

    node_input![
        port! { name = "base_url", ty = String, desc = "Weaviate HTTP 地址，例如 http://127.0.0.1:8080" },
        port! { name = "class_name", ty = String, desc = "Weaviate 图片 collection 名称，例如 ImageRecordVector" },
        port! { name = "api_key", ty = Password, desc = "可选：Weaviate API Key", optional },
        port! { name = "timeout_secs", ty = Integer, desc = "可选：请求超时秒数，默认 30 秒", optional },
    ];

    node_output![port! { name = "weaviate_ref", ty = WeaviateRef, desc = "Weaviate 数据库引用" },];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let base_url = inputs
            .get("base_url")
            .and_then(|value| match value {
                DataValue::String(value) => Some(value.trim().to_string()),
                _ => None,
            })
            .ok_or_else(|| Error::ValidationError("base_url is required".to_string()))?;
        if base_url.is_empty() {
            return Err(Error::ValidationError(
                "base_url must not be empty".to_string(),
            ));
        }

        let class_name = inputs
            .get("class_name")
            .and_then(|value| match value {
                DataValue::String(value) => Some(value.trim().to_string()),
                _ => None,
            })
            .ok_or_else(|| Error::ValidationError("class_name is required".to_string()))?;
        if class_name.is_empty() {
            return Err(Error::ValidationError(
                "class_name must not be empty".to_string(),
            ));
        }

        let api_key = inputs.get("api_key").and_then(|value| match value {
            DataValue::Password(value) => {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }
            _ => None,
        });

        let timeout_secs = match inputs.get("timeout_secs") {
            Some(DataValue::Integer(value)) if *value > 0 => *value as u64,
            Some(DataValue::Integer(_)) | None => DEFAULT_TIMEOUT_SECS,
            Some(_) => {
                return Err(Error::ValidationError(
                    "timeout_secs must be an integer".to_string(),
                ))
            }
        };

        let weaviate_ref = Arc::new(WeaviateRef::new(
            base_url,
            class_name.clone(),
            api_key,
            Duration::from_secs(timeout_secs),
        )?);

        if !weaviate_ref.ready()? {
            return Err(Error::StringError(
                "Weaviate is reachable but not ready yet".to_string(),
            ));
        }

        weaviate_ref.ensure_collection(&image_vector_collection_config(class_name))?;

        let outputs = HashMap::from([(
            "weaviate_ref".to_string(),
            DataValue::WeaviateRef(weaviate_ref),
        )]);
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

fn image_vector_collection_config(class_name: String) -> WeaviateCollectionConfig {
    WeaviateCollectionConfig {
        class_name,
        description: Some("Image vector storage".to_string()),
        properties: vec![
            text_property(
                "object_storage_path",
                "对象存储路径（object_key/object_url）",
            ),
            text_property("summary", "图片总结说明"),
            text_property("source", "来源标记，如 qq 或 tavily"),
            text_property("message_id", "关联消息ID"),
            text_property("sender_id", "关联发送者ID"),
            date_property("send_time", "记录时间"),
        ],
        vectorizer: Some("none".to_string()),
    }
}

fn text_property(name: &str, description: &str) -> WeaviatePropertyConfig {
    WeaviatePropertyConfig {
        name: name.to_string(),
        data_type: vec!["text".to_string()],
        description: Some(description.to_string()),
    }
}

fn date_property(name: &str, description: &str) -> WeaviatePropertyConfig {
    WeaviatePropertyConfig {
        name: name.to_string(),
        data_type: vec!["date".to_string()],
        description: Some(description.to_string()),
    }
}
