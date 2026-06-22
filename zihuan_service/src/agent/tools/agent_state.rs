use std::sync::{Arc, Mutex};

use serde_json::Value;
use zihuan_agent::brain::BrainTool;
use zihuan_agent::session_state::{EmotionAdjustmentDirection, QqChatAgentServiceSessionState};
use zihuan_core::agent_config::qq_chat::QqChatEmotionDimensionConfig;
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::tooling::FunctionTool;

use super::common::{optional_string_argument, StaticFunctionToolSpec};

pub(crate) struct UpdateAgentStateBrainTool {
    session_state: Arc<Mutex<QqChatAgentServiceSessionState>>,
    emotion_dimensions: Vec<QqChatEmotionDimensionConfig>,
}

impl UpdateAgentStateBrainTool {
    pub(crate) fn new(
        session_state: Arc<Mutex<QqChatAgentServiceSessionState>>,
        emotion_dimensions: Vec<QqChatEmotionDimensionConfig>,
    ) -> Self {
        Self {
            session_state,
            emotion_dimensions,
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
            let mut session_state = self.session_state.lock().unwrap();
            let current_value =
                session_state.apply_emotion_adjustment(&self.emotion_dimensions, &dimension, direction)?;
            Ok(serde_json::json!({
                "ok": true,
                "dimension": dimension,
                "direction": direction,
                "current_value": current_value,
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
