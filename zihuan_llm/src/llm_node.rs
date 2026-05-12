use std::collections::HashMap;
use std::sync::Arc;

use crate::system_config::{load_llm_refs, LlmApiStyle, LlmServiceConfig, ModelRefSpec};
use zihuan_core::error::Result;
use zihuan_graph_engine::{
    node_output, DataType, DataValue, Node, NodeConfigField, NodeConfigWidget, Port,
};

/// LlmNode — Select and output an LLM model reference.
///
/// Selects a system-configured LLM ref and outputs a `DataValue::LLModel`
/// reference that downstream nodes (e.g. LLMInferNode) can use for inference.
pub struct LlmNode {
    id: String,
    name: String,
    llm_ref_id: Option<String>,
}

const LLM_REF_ID_FIELD: &str = "llm_ref_id";

/// Build an `Arc<dyn LLMBase>` from a service config, dispatching to the
/// appropriate backend based on `api_style`.
pub fn build_llm(config: LlmServiceConfig) -> Result<Arc<dyn zihuan_core::llm::llm_base::LLMBase>> {
    match config.api_style {
        LlmApiStyle::OpenAiChatCompletions | LlmApiStyle::OpenAiResponses => {
            let api = crate::llm_api::LLMAPI::new(
                config.model_name,
                config.api_endpoint,
                config.api_key,
                config.api_style,
                config.stream,
                config.supports_multimodal_input,
                std::time::Duration::from_secs(config.timeout_secs),
            )
            .with_retry_count(config.retry_count);
            Ok(Arc::new(api))
        }
        LlmApiStyle::Candle => Err(zihuan_core::error::Error::ValidationError(
            "Candle backend is not implemented yet".to_string(),
        )),
    }
}

impl LlmNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            llm_ref_id: None,
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

    fn resolve_llm_config(&self) -> Result<LlmServiceConfig> {
        let llm_ref_id = self
            .llm_ref_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                zihuan_core::error::Error::ValidationError("llm_ref_id is required".to_string())
            })?;

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
            ModelRefSpec::ChatLlm { llm } => Ok(llm),
            ModelRefSpec::TextEmbeddingLocal { .. } => {
                Err(zihuan_core::error::Error::ValidationError(format!(
                    "llm_ref '{}' is not a chat LLM config",
                    llm_ref.name
                )))
            }
        }
    }
}

impl Node for LlmNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("选择LLM配置，输出LLModel引用供下游节点使用")
    }

    fn input_ports(&self) -> Vec<Port> {
        Vec::new()
    }

    node_output![
        port! { name = "llm_model", ty = LLModel, desc = "LLM模型引用，传递给推理节点使用" },
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
        Ok(())
    }

    fn execute(
        &mut self,
        _inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        let llm_config = self.resolve_llm_config()?;
        let llm = build_llm(llm_config)?;

        let mut outputs = HashMap::new();
        outputs.insert("llm_model".to_string(), DataValue::LLModel(llm));

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
