use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use zihuan_core::error::Result;
use zihuan_core::system_config::{load_section, save_section, SystemConfigSection};
use zihuan_graph_engine::function_graph::FunctionPortDef;
use zihuan_graph_engine::graph_io::NodeGraphDefinition;

use crate::brain_tool::ToolParamDef;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub id: String,
    pub name: String,
    pub agent_type: AgentType,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentType {
    QqChat(QqChatAgentConfig),
    HttpStream(HttpStreamAgentConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QqChatAgentConfig {
    pub ims_bot_adapter_connection_id: String,
    #[serde(default)]
    pub rustfs_connection_id: Option<String>,
    #[serde(default)]
    pub bot_name: String,
    #[serde(default)]
    pub llm_ref_id: Option<String>,
    #[serde(default)]
    pub llm: Option<LlmServiceConfig>,
    pub tavily_connection_id: String,
    #[serde(default)]
    pub embedding: Option<EmbeddingServiceConfig>,
    #[serde(default)]
    pub mysql_connection_id: Option<String>,
    #[serde(default)]
    pub weaviate_connection_id: Option<String>,
    #[serde(default)]
    pub weaviate_image_connection_id: Option<String>,
    #[serde(default = "default_max_message_length")]
    pub max_message_length: usize,
    #[serde(default)]
    pub compact_context_length: usize,
    #[serde(default = "default_qq_chat_default_tools_enabled")]
    pub default_tools_enabled: HashMap<String, bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpStreamAgentConfig {
    #[serde(default = "default_http_bind")]
    pub bind: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub llm_ref_id: Option<String>,
    #[serde(default)]
    pub llm: Option<LlmServiceConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmServiceConfig {
    pub model_name: String,
    pub api_endpoint: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub supports_multimodal_input: bool,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default = "default_retry_count")]
    pub retry_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingServiceConfig {
    pub model_name: String,
    pub api_endpoint: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default = "default_retry_count")]
    pub retry_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentToolConfig {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub enabled: bool,
    pub tool_type: AgentToolType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentToolType {
    NodeGraph(NodeGraphToolConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "target_type", rename_all = "snake_case")]
pub enum NodeGraphToolConfig {
    FilePath {
        path: String,
        #[serde(default)]
        parameters: Vec<ToolParamDef>,
        #[serde(default)]
        outputs: Vec<FunctionPortDef>,
    },
    WorkflowSet {
        name: String,
        #[serde(default)]
        parameters: Vec<ToolParamDef>,
        #[serde(default)]
        outputs: Vec<FunctionPortDef>,
    },
    InlineGraph {
        graph: NodeGraphDefinition,
        #[serde(default)]
        parameters: Vec<ToolParamDef>,
        #[serde(default)]
        outputs: Vec<FunctionPortDef>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmRefConfig {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
    pub llm: LlmServiceConfig,
    #[serde(default)]
    pub updated_at: String,
}

fn default_http_bind() -> String {
    "127.0.0.1:18080".to_string()
}

fn default_max_message_length() -> usize {
    500
}

fn default_qq_chat_default_tools_enabled() -> HashMap<String, bool> {
    [
        "web_search",
        "get_agent_public_info",
        "get_function_list",
        "get_recent_group_messages",
        "get_recent_user_messages",
        "search_similar_messages",
        "search_similar_images",
        "reply_plain_text",
        "reply_at",
        "reply_combine_text",
        "reply_forward_text",
        "reply_send_image",
        "no_reply",
    ]
    .into_iter()
    .map(|name| (name.to_string(), true))
    .collect()
}

fn default_timeout_secs() -> u64 {
    30
}

fn default_retry_count() -> u32 {
    2
}

pub struct AgentsSection;

impl SystemConfigSection for AgentsSection {
    const SECTION_KEY: &'static str = "agents";
    type Value = Vec<AgentConfig>;
}

pub struct LlmRefsSection;

impl SystemConfigSection for LlmRefsSection {
    const SECTION_KEY: &'static str = "llm_refs";
    type Value = Vec<LlmRefConfig>;
}

pub fn load_agents() -> Result<Vec<AgentConfig>> {
    load_section::<AgentsSection>()
}

pub fn save_agents(agents: Vec<AgentConfig>) -> Result<()> {
    save_section::<AgentsSection>(&agents)
}

pub fn load_llm_refs() -> Result<Vec<LlmRefConfig>> {
    load_section::<LlmRefsSection>()
}

pub fn save_llm_refs(llm_refs: Vec<LlmRefConfig>) -> Result<()> {
    save_section::<LlmRefsSection>(&llm_refs)
}
