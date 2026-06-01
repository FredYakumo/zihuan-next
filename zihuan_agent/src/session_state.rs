use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QqChatAgentSessionState {
    #[serde(default)]
    pub emotion_state: String,
    #[serde(default)]
    pub extra_state: HashMap<String, Value>,
}
