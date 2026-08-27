use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::agent::qq_chat::QqChatAgentServiceConfig;
use crate::agent::tool_config::AgentToolConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleServiceConfig {
    #[serde(default, skip_serializing)]
    pub id: String,
    #[serde(default)]
    pub config_id: String,
    pub name: String,
    pub role_service_type: RoleServiceType,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub auto_start: bool,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub tools: Vec<AgentToolConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
}

impl RoleServiceConfig {
    pub fn canonical_config_id(&self) -> &str {
        if self.config_id.trim().is_empty() {
            &self.id
        } else {
            &self.config_id
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RoleServiceType {
    QqChat(QqChatAgentServiceConfig),
    Workspace(WorkspaceAgentServiceConfig),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryBackendKind {
    LocalFile,
    Weaviate,
    Elasticsearch,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceAgentServiceConfig {
    #[serde(default)]
    pub llm_ref_id: Option<String>,
    #[serde(default)]
    pub agents_md_enabled: bool,
    #[serde(default)]
    pub memory_enabled: bool,
    #[serde(default)]
    pub embedding_model_ref_id: Option<String>,
    #[serde(default)]
    pub weaviate_memory_connection_id: Option<String>,
    #[serde(default)]
    pub elasticsearch_memory_connection_id: Option<String>,
    #[serde(default)]
    pub memory_backend: Option<MemoryBackendKind>,
    #[serde(default)]
    pub web_search_engine_connection_id: Option<String>,
    #[serde(default = "default_workspace_default_tools_enabled")]
    pub default_tools_enabled: HashMap<String, bool>,
}

fn default_workspace_default_tools_enabled() -> HashMap<String, bool> {
    [
        ("read_file".to_string(), true),
        ("list_dir".to_string(), true),
        ("grep".to_string(), true),
        ("rg".to_string(), true),
        ("find_files".to_string(), true),
        ("copy_file".to_string(), true),
        ("move_file".to_string(), true),
        ("file_info".to_string(), true),
        ("git_status".to_string(), true),
        ("create_file".to_string(), true),
        ("delete_file".to_string(), true),
        ("edit_file".to_string(), true),
        ("exec_cmd".to_string(), true),
        ("ask_user".to_string(), true),
        ("web_search".to_string(), false),
    ]
    .into_iter()
    .collect()
}
