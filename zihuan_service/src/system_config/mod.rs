pub mod role_services {
    #[allow(unused_imports)]
    pub use zihuan_core::agent::service_config::{RoleServiceConfig, RoleServiceType, WorkspaceAgentServiceConfig};
    pub use zihuan_core::agent::tool_config::{AgentToolConfig, AgentToolType, NodeGraphToolConfig};
    pub use zihuan_core::model_inference::model_config::LlmServiceConfig;
    pub use zihuan_core::agent::qq_chat::QqChatAgentServiceConfig;
    #[allow(unused_imports)]
    pub use zihuan_core::agent::EmbeddingServiceConfig;
}

pub mod connections {
    #[allow(unused_imports)]
    pub use zihuan_core::ims_bot_adapter::BotAdapterConnection;
    #[allow(unused_imports)]
    pub use zihuan_core::storage::{
        ConnectionConfig, ConnectionKind, MysqlConnection, RedisConnection, RustfsConnection, WeaviateConnection,
    };
}

pub mod llm_refs {
    #[allow(unused_imports)]
    pub use zihuan_core::config::llm_refs::LlmRefConfig;
}

#[allow(unused_imports)]
pub use zihuan_core::config::llm_refs::{load_llm_refs, save_llm_refs};
pub use zihuan_core::config::role_services::{load_role_services, save_role_services};
#[allow(unused_imports)]
pub use zihuan_core::storage::{load_connections, save_connections};
#[allow(unused_imports)]
pub use zihuan_core::config::{
    ConfigCategory, ConfigCenter, ConfigKind, ConfigRecord, RuntimeInstance, RuntimeInstanceSummary, StoredConfigRecord,
};
#[allow(unused_imports)]
pub use zihuan_core::system_config::{load_system_config_root, save_system_config_root};
