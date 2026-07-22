use std::sync::{Arc, Mutex};

use serde_json::Value;
use zihuan_agent::brain::BrainTool;
use zihuan_agent::session_state::{EmotionAdjustmentDirection, QqChatAgentServiceSessionState};
use zihuan_core::agent_config::qq_chat::QqChatEmotionDimensionConfig;
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::llm_base::LLMBase;
use zihuan_core::llm::tooling::FunctionTool;
use zihuan_core::llm::{InferenceParam, LLMMessage};

use super::common::{optional_string_argument, StaticFunctionToolSpec};

pub(crate) struct UpdateAgentStateBrainTool {
    session_state: Arc<Mutex<QqChatAgentServiceSessionState>>,
    emotion_dimensions: Vec<QqChatEmotionDimensionConfig>,
    llm: Arc<dyn LLMBase>,
    current_user_message: String,
}

impl UpdateAgentStateBrainTool {
    pub(crate) fn new(
        session_state: Arc<Mutex<QqChatAgentServiceSessionState>>,
        emotion_dimensions: Vec<QqChatEmotionDimensionConfig>,
        llm: Arc<dyn LLMBase>,
        current_user_message: String,
    ) -> Self {
        Self {
            session_state,
            emotion_dimensions,
            llm,
            current_user_message,
        }
    }
}

impl BrainTool for UpdateAgentStateBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: "update_agent_state",
            description: "调整当前 QQ Chat Agent Service 的某个情绪维度。只说明升高还是降低，具体幅度由后端配置决定。",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "dimension": {
                        "type": "string",
                        "description": "要调整的情绪维度名称，例如 开心、烦恼、生气"
                    },
                    "direction": {
                        "type": "string",
                        "enum": ["increase", "decrease"],
                        "description": "对该情绪维度进行提升或降低"
                    },
                    "reason": {
                        "type": "string",
                        "description": "调整情绪的原因；留空时使用当前用户消息"
                    }
                },
                "required": ["dimension", "direction"],
                "additionalProperties": false
            }),
        })
    }

    fn execute(&self, _call_content: &str, arguments: &Value) -> String {
        let result = (|| -> Result<String> {
            let dimension = optional_string_argument(arguments, "dimension")
                .ok_or_else(|| Error::ValidationError("dimension is required".to_string()))?;
            let direction = optional_string_argument(arguments, "direction")
                .ok_or_else(|| Error::ValidationError("direction is required".to_string()))?;
            let direction = match direction.as_str() {
                "increase" => EmotionAdjustmentDirection::Increase,
                "decrease" => EmotionAdjustmentDirection::Decrease,
                other => return Err(Error::ValidationError(format!("unsupported direction '{}'", other))),
            };
            let reason = optional_string_argument(arguments, "reason")
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| self.current_user_message.clone());
            let emotion_prompt = self.emotion_prompt_for(&dimension, direction)?;
            let current_value = {
                let mut session_state = self.session_state.lock().unwrap();
                session_state.apply_emotion_adjustment(&self.emotion_dimensions, &dimension, direction)?
            };
            let generated_prompt = self.generate_expression_prompt(&emotion_prompt, &reason);
            if let Some(prompt) = generated_prompt.as_ref() {
                self.session_state
                    .lock()
                    .unwrap()
                    .set_emotion_expression_prompt(dimension.trim(), prompt.clone());
            }
            Ok(serde_json::json!({
                "ok": true,
                "dimension": dimension,
                "direction": direction,
                "current_value": current_value,
                "expression_prompt_generated": generated_prompt.is_some(),
            })
            .to_string())
        })();

        match result {
            Ok(message) => message,
            Err(error) => serde_json::json!({
                "ok": false,
                "error": error.to_string(),
            })
            .to_string(),
        }
    }
}

impl UpdateAgentStateBrainTool {
    fn emotion_prompt_for(&self, dimension_name: &str, direction: EmotionAdjustmentDirection) -> Result<String> {
        let dimension = self
            .emotion_dimensions
            .iter()
            .find(|item| item.name.trim() == dimension_name.trim())
            .ok_or_else(|| Error::ValidationError(format!("unsupported emotion dimension '{}'", dimension_name)))?;
        let configured_prompt = match direction {
            EmotionAdjustmentDirection::Increase => dimension.positive_prompt.as_deref(),
            EmotionAdjustmentDirection::Decrease => dimension.negative_prompt.as_deref(),
        }
        .map(str::trim)
        .filter(|value| !value.is_empty());
        Ok(configured_prompt.unwrap_or(dimension.name.trim()).to_string())
    }

    fn generate_expression_prompt(&self, emotion_prompt: &str, reason: &str) -> Option<String> {
        let messages = vec![
            LLMMessage::system(
                "你负责生成 QQ 机器人回复时可直接使用的语言风格指令。只输出简洁、可执行的风格提示词，不要解释，不要提及情绪管理、工具或用户消息。",
            ),
            LLMMessage::user(format!(
                "请使用以下情绪提示词或情绪维度：{emotion_prompt}\n因为：{reason}\n生成一条指导回复语言风格的提示词。"
            )),
        ];
        self.llm
            .inference(&InferenceParam {
                messages: &messages,
                tools: None,
            })
            .content_text_owned()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    }
}
