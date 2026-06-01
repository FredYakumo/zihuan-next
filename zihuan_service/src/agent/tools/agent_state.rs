use std::sync::{Arc, Mutex};

use serde_json::Value;
use zihuan_agent::brain::BrainTool;
use zihuan_agent::session_state::QqChatAgentSessionState;
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::tooling::FunctionTool;

use super::common::{optional_string_argument, StaticFunctionToolSpec};

pub(crate) struct UpdateAgentStateBrainTool {
    session_state: Arc<Mutex<QqChatAgentSessionState>>,
}

impl UpdateAgentStateBrainTool {
    pub(crate) fn new(session_state: Arc<Mutex<QqChatAgentSessionState>>) -> Self {
        Self { session_state }
    }
}

impl BrainTool for UpdateAgentStateBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: "update_agent_state",
            description: "更新当前 QQ Chat Agent 的会话状态。v1 仅允许更新 emotion_state。",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "emotion_state": {
                        "type": "string",
                        "description": "当前情绪状态，例如 calm、happy、angry、sad"
                    }
                },
                "required": ["emotion_state"],
                "additionalProperties": false
            }),
        })
    }

    fn execute(&self, _call_content: &str, arguments: &Value) -> String {
        let result = (|| -> Result<String> {
            let emotion_state = optional_string_argument(arguments, "emotion_state")
                .ok_or_else(|| Error::ValidationError("emotion_state is required".to_string()))?;
            let mut session_state = self.session_state.lock().unwrap();
            session_state.emotion_state = emotion_state.clone();
            Ok(serde_json::json!({
                "ok": true,
                "emotion_state": emotion_state,
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
