use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crate::linalg::embedding_api::EmbeddingAPI;
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::embedding_base::EmbeddingBase;
use zihuan_graph_engine::{node_input, node_output, DataType, DataValue, Node, Port};

const DEFAULT_RETRY_COUNT: u32 = 2;

pub struct LoadTextEmbedderNode {
    id: String,
    name: String,
}

impl LoadTextEmbedderNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self { id: id.into(), name: name.into() }
    }
}

impl Node for LoadTextEmbedderNode {
    fn id(&self) -> &str { &self.id }
    fn name(&self) -> &str { &self.name }
    fn description(&self) -> Option<&str> {
        Some("加载文本 embedding 模型配置，输出 EmbeddingModel 引用供下游文本向量化节点使用")
    }

    node_input![
        port! { name = "model_name", ty = String, desc = "embedding 模型名称，例如 text-embedding-3-small" },
        port! { name = "api_endpoint", ty = String, desc = "embedding 端点，例如 https://api.openai.com/v1/embeddings" },
        port! { name = "api_key", ty = Password, desc = "API 密钥，可选", optional },
        port! { name = "timeout_secs", ty = Integer, desc = "超时秒数，可选，默认 60 秒", optional },
        port! { name = "retry_count", ty = Integer, desc = "重试次数，可选，默认 2 次", optional },
    ];

    node_output![
        port! { name = "embedding_model", ty = EmbeddingModel, desc = "Embedding 模型引用" },
    ];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let model_name = match inputs.get("model_name") {
            Some(DataValue::String(value)) if !value.trim().is_empty() => value.trim().to_string(),
            _ => return Err(Error::ValidationError("Missing required input: model_name".to_string())),
        };

        let api_endpoint = match inputs.get("api_endpoint") {
            Some(DataValue::String(value)) if !value.trim().is_empty() => value.trim().to_string(),
            _ => return Err(Error::ValidationError("Missing required input: api_endpoint".to_string())),
        };

        let api_key = inputs.get("api_key").and_then(|value| match value {
            DataValue::Password(secret) if !secret.is_empty() => Some(secret.clone()),
            _ => None,
        });

        let timeout_secs = inputs
            .get("timeout_secs")
            .and_then(|value| match value {
                DataValue::Integer(raw) if *raw > 0 => Some(*raw as u64),
                _ => None,
            })
            .unwrap_or(60);

        let retry_count = inputs
            .get("retry_count")
            .and_then(|value| match value {
                DataValue::Integer(raw) => Some((*raw).max(0) as u32),
                _ => None,
            })
            .unwrap_or(DEFAULT_RETRY_COUNT);

        let model: Arc<dyn EmbeddingBase> = Arc::new(
            EmbeddingAPI::new(model_name, api_endpoint, api_key, Duration::from_secs(timeout_secs))
                .with_retry_count(retry_count),
        );

        let outputs = HashMap::from([("embedding_model".to_string(), DataValue::EmbeddingModel(model))]);
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
