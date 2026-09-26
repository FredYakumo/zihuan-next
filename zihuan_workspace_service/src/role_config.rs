use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use zihuan_core::retrieval::RetrievalStoreConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceRoleServiceConfig {
    #[serde(default)]
    pub llm_ref_id: Option<String>,
    #[serde(default)]
    pub orchestration_llm_ref_id: Option<String>,
    #[serde(default)]
    pub image_understand_llm_ref_id: Option<String>,
    #[serde(default)]
    pub agents_md_enabled: bool,
    #[serde(default)]
    pub memory_enabled: bool,
    #[serde(default)]
    pub embedding_model_ref_id: Option<String>,
    #[serde(default)]
    pub retrieval_store: Option<RetrievalStoreConfig>,
    #[serde(default)]
    pub web_search_engine_connection_id: Option<String>,
    #[serde(default = "default_workspace_default_tools_enabled")]
    pub default_tools_enabled: HashMap<String, bool>,
}

impl WorkspaceRoleServiceConfig {
    /// The configured retrieval-store connection, when the agent uses an external store.
    pub fn retrieval_store_connection_id(&self) -> Option<&str> {
        self.retrieval_store.as_ref().and_then(RetrievalStoreConfig::connection_id)
    }

    /// Whether the agent's retrieval store is the local on-disk backend.
    pub fn retrieval_store_is_local(&self) -> bool {
        self.retrieval_store.as_ref().is_some_and(RetrievalStoreConfig::is_local)
    }
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
        ("image_understand".to_string(), true),
        ("web_search".to_string(), false),
    ]
    .into_iter()
    .collect()
}
