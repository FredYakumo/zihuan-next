mod connection_manager;
mod message_store;
pub mod mysql;
pub mod object_storage;
pub mod redis;
pub mod resource_resolver;
pub mod rustfs;
pub mod weaviate;

use serde::{Deserialize, Serialize};
use zihuan_core::error::Result;
use zihuan_core::system_config::{load_section, save_section, SystemConfigSection};

pub use connection_manager::{
    cleanup_runtime_storage_instances, close_runtime_storage_instance, list_runtime_storage_instances,
    MessageStoreConnectionAccess, RuntimeStorageConnectionManager, StorageRuntimeHandle,
};
pub use message_store::{MessageRecord, MessageStore};
pub use mysql::MySqlNode;
pub use object_storage::{
    enrich_event_images, enrich_message_images, save_image_to_object_storage, ImageCacheAdapter,
    ImageObjectStorageInput, ObjectStorageConfig, PendingImageUpload, SavedImageObject,
};
pub use redis::RedisNode;
pub use resource_resolver::{
    build_mysql_ref, build_redis_ref, build_s3_ref, build_tavily_ref, build_weaviate_ref,
    find_connection, resolve_connection_data_value,
};
pub use rustfs::RustfsNode;
pub use weaviate::WeaviateNode;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ConnectionConfig {
    pub id: String,
    #[serde(default)]
    pub config_id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub enabled: bool,
    pub kind: ConnectionKind,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConnectionKind {
    Mysql(MysqlConnection),
    Redis(RedisConnection),
    Weaviate(WeaviateConnection),
    Rustfs(RustfsConnection),
    BotAdapter(serde_json::Value),
    Tavily(TavilyConnection),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MysqlConnection {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisConnection {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeaviateConnection {
    pub base_url: String,
    pub class_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RustfsConnection {
    pub endpoint: String,
    pub bucket: String,
    pub region: String,
    pub access_key: String,
    pub secret_key: String,
    #[serde(default)]
    pub public_base_url: Option<String>,
    #[serde(default)]
    pub path_style: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TavilyConnection {
    pub api_token: String,
    #[serde(default = "default_tavily_timeout_secs")]
    pub timeout_secs: u64,
}

fn default_tavily_timeout_secs() -> u64 {
    30
}

impl ConnectionConfig {
    pub fn canonical_config_id(&self) -> &str {
        self.config_id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(&self.id)
    }

    pub fn is_valid(&self) -> bool {
        match &self.kind {
            ConnectionKind::Tavily(tavily) => !tavily.api_token.trim().is_empty(),
            _ => true,
        }
    }
}

pub struct ConnectionsSection;

impl SystemConfigSection for ConnectionsSection {
    const SECTION_KEY: &'static str = "connections";
    type Value = Vec<ConnectionConfig>;
}

pub fn load_connections() -> Result<Vec<ConnectionConfig>> {
    let mut connections = load_section::<ConnectionsSection>()?;
    for connection in &mut connections {
        if connection
            .config_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
        {
            connection.config_id = Some(connection.id.clone());
        }
        if connection.id.trim().is_empty() {
            connection.id = connection.canonical_config_id().to_string();
        }
    }
    Ok(connections)
}

pub fn save_connections(connections: Vec<ConnectionConfig>) -> Result<()> {
    let normalized = connections
        .into_iter()
        .map(|mut connection| {
            let canonical = connection.canonical_config_id().to_string();
            connection.id = canonical.clone();
            connection.config_id = Some(canonical);
            connection
        })
        .collect::<Vec<_>>();
    save_section::<ConnectionsSection>(&normalized)
}

pub fn init_node_registry() -> Result<()> {
    use zihuan_graph_engine::image_weaviate_persistence::ImageWeaviatePersistenceNode;
    use zihuan_graph_engine::message_mysql_get_group_history::MessageMySQLGetGroupHistoryNode;
    use zihuan_graph_engine::message_mysql_get_user_history::MessageMySQLGetUserHistoryNode;
    use zihuan_graph_engine::qq_message_list_mysql_persistence::QQMessageListMySQLPersistenceNode;
    use zihuan_graph_engine::qq_message_list_weaviate_persistence::QQMessageListWeaviatePersistenceNode;
    use zihuan_graph_engine::register_node;

    register_node!(
        "redis",
        "Redis连接",
        "数据库",
        "从系统连接配置中选择 Redis 并输出 RedisRef 引用",
        RedisNode
    );
    register_node!(
        "mysql",
        "MySQL连接",
        "数据库",
        "从系统连接配置中选择 MySQL 并输出 MySqlRef 引用",
        MySqlNode
    );
    register_node!(
        "rustfs",
        "RustFS对象存储",
        "数据库",
        "从系统连接配置中选择 RustFS 并输出 S3Ref 引用",
        RustfsNode
    );
    register_node!(
        "weaviate",
        "Weaviate向量数据库",
        "数据库",
        "从系统连接配置中选择 Weaviate 并输出 WeaviateRef 引用",
        WeaviateNode
    );
    register_node!(
        "qq_message_list_mysql_persistence",
        "QQMessage列表MySQL持久化",
        "消息存储",
        "将Vec<QQMessage>及调用方提供的元数据持久化到MySQL数据库",
        QQMessageListMySQLPersistenceNode
    );
    register_node!(
        "qq_message_list_weaviate_persistence",
        "QQMessage列表向量持久化",
        "消息存储",
        "将Vec<QQMessage>及调用方提供的元数据向量化后持久化到Weaviate数据库",
        QQMessageListWeaviatePersistenceNode
    );
    register_node!(
        "image_weaviate_persistence",
        "图片向量持久化",
        "消息存储",
        "将对象存储路径、图片总结与向量持久化到Weaviate数据库",
        ImageWeaviatePersistenceNode
    );
    register_node!(
        "message_mysql_get_user_history",
        "获取QQ号消息历史",
        "消息存储",
        "根据 sender_id 读取最近消息历史，可选限定某个群",
        MessageMySQLGetUserHistoryNode
    );
    register_node!(
        "message_mysql_get_group_history",
        "获取QQ群聊消息历史",
        "消息存储",
        "根据 group_id 读取最近消息历史",
        MessageMySQLGetGroupHistoryNode
    );

    Ok(())
}
