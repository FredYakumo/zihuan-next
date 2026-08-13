pub mod agents {
    #[allow(unused_imports)]
    pub use zihuan_core::inference::system_config::{
        AgentConfig, AgentToolConfig, AgentToolType, AgentType, LlmServiceConfig,
        NodeGraphToolConfig, WorkspaceAgentServiceConfig,
    };
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
    pub use zihuan_core::inference::system_config::LlmRefConfig;
}

#[allow(unused_imports)]
pub use zihuan_core::inference::system_config::{load_agents, load_llm_refs, save_agents, save_llm_refs};
#[allow(unused_imports)]
pub use zihuan_core::storage::{load_connections, save_connections};
#[allow(unused_imports)]
pub use zihuan_core::config::{
    ConfigCategory, ConfigCenter, ConfigKind, ConfigRecord, RuntimeInstance, RuntimeInstanceSummary, StoredConfigRecord,
};
#[allow(unused_imports)]
pub use zihuan_core::system_config::{load_system_config_root, save_system_config_root};
