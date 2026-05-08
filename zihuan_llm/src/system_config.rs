use std::collections::HashMap;

use log::info;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use zihuan_core::config::{
    ConfigCategory, ConfigCenter, ConfigKind, ConfigRecord, StoredConfigRecord,
};
use zihuan_core::error::Result;
use zihuan_graph_engine::function_graph::FunctionPortDef;
use zihuan_graph_engine::graph_io::NodeGraphDefinition;

use crate::brain_tool::ToolParamDef;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    #[serde(default, skip_serializing)]
    pub id: String,
    #[serde(default)]
    pub config_id: String,
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
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub llm_ref_id: Option<String>,
    #[serde(default)]
    pub intent_llm_ref_id: Option<String>,
    #[serde(default)]
    pub math_programming_llm_ref_id: Option<String>,
    #[serde(default)]
    pub embedding_model_ref_id: Option<String>,
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
    #[serde(default = "default_stream")]
    pub stream: bool,
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
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ModelRefSpec {
    ChatLlm { llm: LlmServiceConfig },
    TextEmbeddingLocal { model_name: String },
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
    #[serde(default, skip_serializing)]
    pub id: String,
    #[serde(default)]
    pub config_id: String,
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
    pub model: ModelRefSpec,
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

fn default_stream() -> bool {
    false
}

impl AgentConfig {
    pub fn canonical_config_id(&self) -> &str {
        if self.config_id.trim().is_empty() {
            &self.id
        } else {
            &self.config_id
        }
    }
}

impl LlmRefConfig {
    pub fn canonical_config_id(&self) -> &str {
        if self.config_id.trim().is_empty() {
            &self.id
        } else {
            &self.config_id
        }
    }

    pub fn chat_llm(&self) -> Option<&LlmServiceConfig> {
        match &self.model {
            ModelRefSpec::ChatLlm { llm } => Some(llm),
            ModelRefSpec::TextEmbeddingLocal { .. } => None,
        }
    }
}

impl ConfigRecord for AgentConfig {
    fn config_id(&self) -> &str {
        self.canonical_config_id()
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn enabled(&self) -> bool {
        self.enabled
    }

    fn updated_at(&self) -> &str {
        &self.updated_at
    }

    fn kind(&self) -> ConfigKind {
        match self.agent_type {
            AgentType::QqChat(_) => ConfigKind::AgentQqChat,
            AgentType::HttpStream(_) => ConfigKind::AgentHttpStream,
        }
    }

    fn validate(&self) -> Result<()> {
        if self.canonical_config_id().trim().is_empty() {
            return Err(zihuan_core::string_error!(
                "agent config_id must not be empty"
            ));
        }
        if self.name.trim().is_empty() {
            return Err(zihuan_core::string_error!("agent name must not be empty"));
        }
        Ok(())
    }

    fn redacted_summary(&self) -> Value {
        json!({
            "config_id": self.canonical_config_id(),
            "kind": self.kind(),
            "name": self.name,
            "enabled": self.enabled,
        })
    }
}

impl ConfigRecord for LlmRefConfig {
    fn config_id(&self) -> &str {
        self.canonical_config_id()
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn enabled(&self) -> bool {
        self.enabled
    }

    fn updated_at(&self) -> &str {
        &self.updated_at
    }

    fn kind(&self) -> ConfigKind {
        ConfigKind::LlmRef
    }

    fn validate(&self) -> Result<()> {
        if self.canonical_config_id().trim().is_empty() {
            return Err(zihuan_core::string_error!(
                "llm_ref config_id must not be empty"
            ));
        }
        if self.name.trim().is_empty() {
            return Err(zihuan_core::string_error!("llm_ref name must not be empty"));
        }
        match &self.model {
            ModelRefSpec::ChatLlm { llm } => {
                if llm.model_name.trim().is_empty() {
                    return Err(zihuan_core::string_error!(
                        "chat_llm model_name must not be empty"
                    ));
                }
                if llm.api_endpoint.trim().is_empty() {
                    return Err(zihuan_core::string_error!(
                        "chat_llm api_endpoint must not be empty"
                    ));
                }
            }
            ModelRefSpec::TextEmbeddingLocal { model_name } => {
                if model_name.trim().is_empty() {
                    return Err(zihuan_core::string_error!(
                        "text_embedding_local model_name must not be empty"
                    ));
                }
            }
        }
        Ok(())
    }

    fn redacted_summary(&self) -> Value {
        let model = match &self.model {
            ModelRefSpec::ChatLlm { llm } => json!({
                "type": "chat_llm",
                "llm": {
                    "model_name": llm.model_name,
                    "api_endpoint": llm.api_endpoint,
                }
            }),
            ModelRefSpec::TextEmbeddingLocal { model_name } => json!({
                "type": "text_embedding_local",
                "model_name": model_name,
            }),
        };
        json!({
            "config_id": self.canonical_config_id(),
            "kind": self.kind(),
            "name": self.name,
            "enabled": self.enabled,
            "model": model,
        })
    }
}

pub fn load_agents() -> Result<Vec<AgentConfig>> {
    let agents = ConfigCenter::shared()
        .list_configs(ConfigCategory::Agent)?
        .into_iter()
        .map(agent_from_record)
        .collect::<Result<Vec<_>>>()?;
    for agent in &agents {
        info!(
            "[config_center] loaded agent config_id={} kind={:?} name='{}'",
            agent.canonical_config_id(),
            agent.kind(),
            agent.name
        );
    }
    Ok(agents)
}

pub fn save_agents(agents: Vec<AgentConfig>) -> Result<()> {
    save_records(
        ConfigCategory::Agent,
        agents,
        normalize_agent_identity,
        agent_to_record,
    )
}

pub fn load_llm_refs() -> Result<Vec<LlmRefConfig>> {
    let llm_refs = ConfigCenter::shared()
        .list_configs(ConfigCategory::LlmRef)?
        .into_iter()
        .map(llm_ref_from_record)
        .collect::<Result<Vec<_>>>()?;
    for llm_ref in &llm_refs {
        info!(
            "[config_center] loaded llm_ref config_id={} name='{}'",
            llm_ref.canonical_config_id(),
            llm_ref.name
        );
    }
    Ok(llm_refs)
}

pub fn save_llm_refs(llm_refs: Vec<LlmRefConfig>) -> Result<()> {
    save_records(
        ConfigCategory::LlmRef,
        llm_refs,
        normalize_llm_ref_identity,
        llm_ref_to_record,
    )
}

fn save_records<T>(
    category: ConfigCategory,
    items: Vec<T>,
    normalize: fn(T, String) -> T,
    to_record: fn(&T) -> Result<StoredConfigRecord>,
) -> Result<()> {
    let center = ConfigCenter::shared();
    let existing = center.list_configs(category)?;
    let existing_ids = existing
        .into_iter()
        .map(|record| record.config_id)
        .collect::<std::collections::HashSet<_>>();
    let mut incoming_ids = std::collections::HashSet::new();

    for item in items {
        let normalized = normalize(item, center.new_config_id());
        let record = to_record(&normalized)?;
        incoming_ids.insert(record.config_id.clone());
        center.upsert_config(record)?;
    }

    for config_id in existing_ids {
        if !incoming_ids.contains(&config_id) {
            let _ = center.delete_config(category, &config_id)?;
        }
    }

    Ok(())
}

fn normalize_agent_identity(mut agent: AgentConfig, fallback_id: String) -> AgentConfig {
    let canonical = if agent.config_id.trim().is_empty() {
        if agent.id.trim().is_empty() {
            fallback_id
        } else {
            agent.id.clone()
        }
    } else {
        agent.config_id.clone()
    };
    agent.id = canonical.clone();
    agent.config_id = canonical;
    agent
}

fn normalize_llm_ref_identity(mut llm_ref: LlmRefConfig, fallback_id: String) -> LlmRefConfig {
    let canonical = if llm_ref.config_id.trim().is_empty() {
        if llm_ref.id.trim().is_empty() {
            fallback_id
        } else {
            llm_ref.id.clone()
        }
    } else {
        llm_ref.config_id.clone()
    };
    llm_ref.id = canonical.clone();
    llm_ref.config_id = canonical;
    llm_ref
}

fn agent_to_record(agent: &AgentConfig) -> Result<StoredConfigRecord> {
    agent.validate()?;
    let mut spec = Map::new();
    spec.insert(
        "agent_type".to_string(),
        serde_json::to_value(&agent.agent_type)?,
    );
    spec.insert("auto_start".to_string(), Value::Bool(agent.auto_start));
    spec.insert("is_default".to_string(), Value::Bool(agent.is_default));
    spec.insert("tools".to_string(), serde_json::to_value(&agent.tools)?);
    Ok(StoredConfigRecord {
        config_id: agent.canonical_config_id().to_string(),
        kind: agent.kind(),
        name: agent.name.clone(),
        enabled: agent.enabled,
        updated_at: agent.updated_at.clone(),
        spec: Value::Object(spec),
    })
}

fn agent_from_record(record: StoredConfigRecord) -> Result<AgentConfig> {
    if record.kind.category() != ConfigCategory::Agent {
        return Err(zihuan_core::string_error!(
            "config '{}' is not an agent config",
            record.config_id
        ));
    }
    let spec = record.spec.as_object().ok_or_else(|| {
        zihuan_core::string_error!("agent config '{}' spec must be an object", record.config_id)
    })?;
    Ok(AgentConfig {
        id: record.config_id.clone(),
        config_id: record.config_id.clone(),
        name: record.name,
        agent_type: serde_json::from_value(spec.get("agent_type").cloned().unwrap_or(Value::Null))?,
        enabled: record.enabled,
        auto_start: spec
            .get("auto_start")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_default: spec
            .get("is_default")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        updated_at: record.updated_at,
        tools: serde_json::from_value(
            spec.get("tools")
                .cloned()
                .unwrap_or_else(|| Value::Array(Vec::new())),
        )?,
    })
}

fn llm_ref_to_record(llm_ref: &LlmRefConfig) -> Result<StoredConfigRecord> {
    llm_ref.validate()?;
    Ok(StoredConfigRecord {
        config_id: llm_ref.canonical_config_id().to_string(),
        kind: ConfigKind::LlmRef,
        name: llm_ref.name.clone(),
        enabled: llm_ref.enabled,
        updated_at: llm_ref.updated_at.clone(),
        spec: serde_json::to_value(&llm_ref.model)?,
    })
}

fn llm_ref_from_record(record: StoredConfigRecord) -> Result<LlmRefConfig> {
    if record.kind != ConfigKind::LlmRef {
        return Err(zihuan_core::string_error!(
            "config '{}' is not an llm_ref config",
            record.config_id
        ));
    }
    Ok(LlmRefConfig {
        id: record.config_id.clone(),
        config_id: record.config_id,
        name: record.name,
        enabled: record.enabled,
        model: model_ref_spec_from_value(record.spec)?,
        updated_at: record.updated_at,
    })
}

fn model_ref_spec_from_value(value: Value) -> Result<ModelRefSpec> {
    if value
        .as_object()
        .and_then(|object| object.get("type"))
        .and_then(Value::as_str)
        .is_some()
    {
        return Ok(serde_json::from_value(value)?);
    }

    Ok(ModelRefSpec::ChatLlm {
        llm: serde_json::from_value(value)?,
    })
}
