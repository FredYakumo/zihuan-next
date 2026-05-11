use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crate::llm_api::LLMAPI;
use crate::system_config::{load_llm_refs, LlmServiceConfig, ModelRefSpec};
use zihuan_core::error::Result;
use zihuan_graph_engine::{
    node_output, DataType, DataValue, Node, NodeConfigField, NodeConfigWidget, Port,
};

/// LLMApiNode — Configure and output an LLM model reference.
///
/// Takes connection parameters as input and outputs a `DataValue::LLModel`
/// reference that downstream nodes (e.g. LLMInferNode) can use to call the API.
pub struct LLMApiNode {
    id: String,
    name: String,
    llm_ref_id: Option<String>,
    legacy_model_name: Option<String>,
    legacy_api_endpoint: Option<String>,
    legacy_api_key: Option<String>,
    legacy_supports_multimodal_input: Option<bool>,
    legacy_timeout_secs: Option<u64>,
    legacy_retry_count: Option<u32>,
}

const DEFAULT_RETRY_COUNT: u32 = 2;
const DEFAULT_TIMEOUT_SECS: u64 = 120;
const LLM_REF_ID_FIELD: &str = "llm_ref_id";

impl LLMApiNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            llm_ref_id: None,
            legacy_model_name: None,
            legacy_api_endpoint: None,
            legacy_api_key: None,
            legacy_supports_multimodal_input: None,
            legacy_timeout_secs: None,
            legacy_retry_count: None,
        }
    }

    fn llm_ref_select_field() -> NodeConfigField {
        NodeConfigField::new(
            LLM_REF_ID_FIELD,
            DataType::String,
            NodeConfigWidget::LlmRefSelect,
        )
        .with_description("选择系统中的聊天 LLM 配置")
    }

    fn selected_llm_config(&self) -> Result<LlmServiceConfig> {
        let llm_ref_id = self
            .llm_ref_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());

        if let Some(llm_ref_id) = llm_ref_id {
            let llm_ref = load_llm_refs()?
                .into_iter()
                .find(|item| item.id == llm_ref_id || item.config_id == llm_ref_id)
                .ok_or_else(|| {
                    zihuan_core::error::Error::ValidationError(format!(
                        "llm_ref '{}' not found",
                        llm_ref_id
                    ))
                })?;
            if !llm_ref.enabled {
                return Err(zihuan_core::error::Error::ValidationError(format!(
                    "llm_ref '{}' is disabled",
                    llm_ref.name
                )));
            }
            match llm_ref.model {
                ModelRefSpec::ChatLlm { llm } => return Ok(llm),
                ModelRefSpec::TextEmbeddingLocal { .. } => {
                    return Err(zihuan_core::error::Error::ValidationError(format!(
                        "llm_ref '{}' is not a chat LLM config",
                        llm_ref.name
                    )))
                }
            }
        }

        let model_name = self
            .legacy_model_name
            .clone()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                zihuan_core::error::Error::ValidationError("llm_ref_id is required".to_string())
            })?;
        let api_endpoint = self
            .legacy_api_endpoint
            .clone()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                zihuan_core::error::Error::ValidationError(
                    "Missing required legacy config: api_endpoint".to_string(),
                )
            })?;

        Ok(LlmServiceConfig {
            model_name,
            api_endpoint,
            api_key: self
                .legacy_api_key
                .clone()
                .filter(|value| !value.trim().is_empty()),
            stream: false,
            supports_multimodal_input: self.legacy_supports_multimodal_input.unwrap_or(false),
            timeout_secs: self.legacy_timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS),
            retry_count: self.legacy_retry_count.unwrap_or(DEFAULT_RETRY_COUNT),
        })
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
        Some("配置语言模型连接，输出LLModel引用供下游节点使用")
    }

    fn input_ports(&self) -> Vec<Port> {
        Vec::new()
    }

    node_output![
        port! { name = "llm_model", ty = LLModel, desc = "LLM模型引用，传递给LLMInfer等节点使用" },
    ];

    fn config_fields(&self) -> Vec<NodeConfigField> {
        vec![Self::llm_ref_select_field()]
    }

    fn apply_inline_config(&mut self, inline_values: &HashMap<String, DataValue>) -> Result<()> {
        self.llm_ref_id = inline_values
            .get(LLM_REF_ID_FIELD)
            .and_then(|value| match value {
                DataValue::String(value) => Some(value.clone()),
                _ => None,
            });
        self.legacy_model_name = inline_values
            .get("model_name")
            .and_then(|value| match value {
                DataValue::String(value) => Some(value.clone()),
                _ => None,
            });
        self.legacy_api_endpoint =
            inline_values
                .get("api_endpoint")
                .and_then(|value| match value {
                    DataValue::String(value) => Some(value.clone()),
                    _ => None,
                });
        self.legacy_api_key = inline_values.get("api_key").and_then(|value| match value {
            DataValue::Password(value) => Some(value.clone()),
            DataValue::String(value) => Some(value.clone()),
            _ => None,
        });
        self.legacy_supports_multimodal_input = inline_values
            .get("supports_multimodal_input")
            .and_then(|value| match value {
                DataValue::Boolean(value) => Some(*value),
                _ => None,
            });
        self.legacy_timeout_secs =
            inline_values
                .get("timeout_secs")
                .and_then(|value| match value {
                    DataValue::Integer(value) => Some((*value).max(0) as u64),
                    _ => None,
                });
        self.legacy_retry_count = inline_values
            .get("retry_count")
            .and_then(|value| match value {
                DataValue::Integer(value) => Some((*value).max(0) as u32),
                _ => None,
            });
        Ok(())
    }

    fn execute(
        &mut self,
        _inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        let llm_config = self.selected_llm_config()?;

        let llm: Arc<dyn zihuan_core::llm::llm_base::LLMBase> = Arc::new(
            LLMAPI::new(
                llm_config.model_name,
                llm_config.api_endpoint,
                llm_config.api_key,
                llm_config.stream,
                llm_config.supports_multimodal_input,
                Duration::from_secs(llm_config.timeout_secs),
            )
            .with_retry_count(llm_config.retry_count),
        );

        let mut outputs = HashMap::new();
        outputs.insert("llm_model".to_string(), DataValue::LLModel(llm));

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
