pub mod agents {
    #[allow(unused_imports)]
    pub use zihuan_llm::system_config::{
        AgentConfig, AgentToolConfig, AgentToolType, AgentType, EmbeddingServiceConfig,
        HttpStreamAgentConfig, LlmServiceConfig, NodeGraphToolConfig, QqChatAgentConfig,
    };
}

pub mod connections {
    #[allow(unused_imports)]
    pub use ims_bot_adapter::BotAdapterConnection;
    #[allow(unused_imports)]
    pub use storage_handler::{
        ConnectionConfig, ConnectionKind, MysqlConnection, RedisConnection, RustfsConnection,
        TavilyConnection, WeaviateConnection,
    };
}

pub mod llm_refs {
    #[allow(unused_imports)]
    pub use zihuan_llm::system_config::LlmRefConfig;
}

#[allow(unused_imports)]
pub use storage_handler::{load_connections, save_connections};
#[allow(unused_imports)]
pub use zihuan_core::system_config::{load_system_config_root, save_system_config_root};
#[allow(unused_imports)]
pub use zihuan_llm::system_config::{load_agents, load_llm_refs, save_agents, save_llm_refs};
