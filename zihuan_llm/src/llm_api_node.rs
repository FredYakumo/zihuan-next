use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crate::llm_api::LLMAPI;
use zihuan_core::error::Result;
use zihuan_graph_engine::{node_input, node_output, DataType, DataValue, Node, Port};

/// LLMApiNode — Configure and output an LLM model reference.
///
/// Takes connection parameters as input and outputs a `DataValue::LLModel`
/// reference that downstream nodes (e.g. LLMInferNode) can use to call the API.
pub struct LLMApiNode {
    id: String,
    name: String,
}

const DEFAULT_RETRY_COUNT: u32 = 2;

impl LLMApiNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for LLMApiNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("配置语言模型API连接，输出LLModel引用供下游节点使用")
    }

    node_input![
        port! { name = "model_name",    ty = String,  desc = "模型名称，例如: gpt-4, deepseek-chat" },
        port! { name = "api_endpoint",  ty = String,  desc = "API端点URL，例如: https://api.openai.com/v1/chat/completions" },
        port! { name = "api_key",       ty = Password, desc = "API密钥 (可选，某些本地模型不需要)", optional },
        port! { name = "supports_multimodal_input", ty = Boolean, desc = "模型是否支持多模态输入（图片 parts）", optional },
        port! { name = "timeout_secs",  ty = Integer,  desc = "超时秒数 (可选，默认120秒)", optional },
        port! { name = "retry_count",   ty = Integer,  desc = "重试次数 (可选，默认2次，仅用于临时失败)", optional },
    ];

    node_output![
        port! { name = "llm_model", ty = LLModel, desc = "LLM模型引用，传递给LLMInfer等节点使用" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let model_name = match inputs.get("model_name") {
            Some(DataValue::String(s)) => s.clone(),
            _ => {
                return Err(zihuan_core::error::Error::ValidationError(
                    "Missing required input: model_name".to_string(),
                ))
            }
        };

        let api_endpoint = match inputs.get("api_endpoint") {
            Some(DataValue::String(s)) => s.clone(),
            _ => {
                return Err(zihuan_core::error::Error::ValidationError(
                    "Missing required input: api_endpoint".to_string(),
                ))
            }
        };

        let api_key = inputs.get("api_key").and_then(|v| {
            if let DataValue::Password(s) = v {
                if s.is_empty() {
                    None
                } else {
                    Some(s.clone())
                }
            } else {
                None
            }
        });

        let timeout_secs = inputs
            .get("timeout_secs")
            .and_then(|v| {
                if let DataValue::Integer(i) = v {
                    Some(*i as u64)
                } else {
                    None
                }
            })
            .unwrap_or(120);

        let retry_count = inputs
            .get("retry_count")
            .and_then(|v| {
                if let DataValue::Integer(i) = v {
                    Some((*i).max(0) as u32)
                } else {
                    None
                }
            })
            .unwrap_or(DEFAULT_RETRY_COUNT);

        let supports_multimodal_input = inputs
            .get("supports_multimodal_input")
            .and_then(|v| {
                if let DataValue::Boolean(value) = v {
                    Some(*value)
                } else {
                    None
                }
            })
            .unwrap_or(false);

        let llm: Arc<dyn zihuan_core::llm::llm_base::LLMBase> = Arc::new(
            LLMAPI::new(
                model_name,
                api_endpoint,
                api_key,
                supports_multimodal_input,
                Duration::from_secs(timeout_secs),
            )
            .with_retry_count(retry_count),
        );

        let mut outputs = HashMap::new();
        outputs.insert("llm_model".to_string(), DataValue::LLModel(llm));

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

