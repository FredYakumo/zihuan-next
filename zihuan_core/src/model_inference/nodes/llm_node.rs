use crate::graph::NodeOutputFlow;
use std::sync::Arc;

use crate::model_inference::llm_api::LLMAPI;
use crate::model_inference::nn::local_candle_llm_gguf::build_local_candle_gguf_llm;
use crate::model_inference::nn::local_candle_llm_hf::build_local_candle_hf_llm;
use crate::config::llm_refs::load_llm_refs;
use crate::model_inference::model_config::{LlmApiStyle, LlmServiceConfig, ModelRefSpec};
use crate::error::Result;
use crate::model_inference::llm::llm_base::LLMBase;
use crate::graph::{node_output, DataType, DataValue, Node, NodeConfigField, NodeConfigWidget, Port};

pub fn build_llm(config: LlmServiceConfig) -> Result<Arc<dyn LLMBase>> {
    match config.api_style {
        LlmApiStyle::OpenAiChatCompletions
        | LlmApiStyle::OpenAiChatCompletionsTencentMultimodalCompat
        | LlmApiStyle::OpenAiResponses
        | LlmApiStyle::OpenAiResponsesMessageCompat
        | LlmApiStyle::OpenAiResponsesImageUrlObjectCompat => {
            let api = LLMAPI::new(
                config.model_name,
                config.api_endpoint,
                config.api_key,
                config.api_style,
                config.stream,
                config.supports_multimodal_input,
                config.include_reasoning_content,
                config.thinking_type,
                config.reasoning_effort,
                std::time::Duration::from_secs(config.timeout_secs),
            )
            .with_retry_count(config.retry_count);
            Ok(Arc::new(api))
        }
        LlmApiStyle::CandleGguf => build_local_candle_gguf_llm(config),
        LlmApiStyle::CandleHf => build_local_candle_hf_llm(config),
    }
}

pub struct LlmNode {
    id: String,
    name: String,
    llm_ref_id: Option<String>,
}

const LLM_REF_ID_FIELD: &str = "llm_ref_id";

impl LlmNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            llm_ref_id: None,
        }
    }

    fn llm_ref_select_field() -> NodeConfigField {
        NodeConfigField::new(LLM_REF_ID_FIELD, DataType::String, NodeConfigWidget::LlmRefSelect)
            .with_description("选择系统中的聊天 LLM 配置")
    }

    fn resolve_llm_config(&self) -> Result<LlmServiceConfig> {
        let llm_ref_id = self
            .llm_ref_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| crate::error::Error::ValidationError("llm_ref_id is required".to_string()))?;

        let llm_ref = load_llm_refs()?
            .into_iter()
            .find(|item| item.id == llm_ref_id || item.config_id == llm_ref_id)
            .ok_or_else(|| crate::error::Error::ValidationError(format!("llm_ref '{}' not found", llm_ref_id)))?;

        if !llm_ref.enabled {
            return Err(crate::error::Error::ValidationError(format!(
                "llm_ref '{}' is disabled",
                llm_ref.name
            )));
        }

        match llm_ref.model {
            ModelRefSpec::ChatLlm { llm } => Ok(llm),
            ModelRefSpec::TextEmbeddingLocal { .. } => Err(crate::error::Error::ValidationError(format!(
                "llm_ref '{}' is not a chat LLM config",
                llm_ref.name
            ))),
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

    node_output![port! { name = "llm_model", ty = LLModel, desc = "LLM模型引用，传递给推理节点使用" },];

    fn config_fields(&self) -> Vec<NodeConfigField> {
        vec![Self::llm_ref_select_field()]
    }

    fn apply_inline_config(&mut self, inline_values: &crate::graph::NodeConfigFlow) -> Result<()> {
        self.llm_ref_id = inline_values.get(LLM_REF_ID_FIELD).and_then(|value| match value {
            DataValue::String(value) => Some(value.clone()),
            _ => None,
        });
        Ok(())
    }

    fn execute(&mut self, _inputs: crate::graph::NodeInputFlow) -> Result<crate::graph::NodeOutputFlow> {
        let llm_config = self.resolve_llm_config()?;
        let llm = build_llm(llm_config)?;
        crate::graph::return_with_node_output![self;
            "llm_model" => DataValue::LLModel(llm),
        ]
    }
}
