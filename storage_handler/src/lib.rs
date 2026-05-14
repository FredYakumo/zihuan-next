mod connection_manager;
mod message_store;
pub mod mysql;
pub mod object_storage;
pub mod redis;
pub mod resource_resolver;
pub mod rustfs;
mod tavily_provider_node;
mod tavily_search_node;
pub mod weaviate;
mod weaviate_image_search_node;

use log::{debug, info};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use zihuan_core::config::{
    ConfigCategory, ConfigCenter, ConfigKind, ConfigRecord, StoredConfigRecord,
};
use zihuan_core::error::Result;

pub use connection_manager::{
    cleanup_runtime_storage_instances, close_runtime_storage_instance,
    close_runtime_storage_instances_for_config, list_runtime_storage_instances,
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
pub use zihuan_core::weaviate::WeaviateCollectionSchema;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ConnectionConfig {
    #[serde(default, skip_serializing)]
    pub id: String,
    #[serde(default)]
    pub config_id: String,
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
    pub collection_schema: WeaviateCollectionSchema,
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
        if self.config_id.trim().is_empty() {
            &self.id
        } else {
            &self.config_id
        }
    }

    pub fn is_valid(&self) -> bool {
        match &self.kind {
            ConnectionKind::Tavily(tavily) => !tavily.api_token.trim().is_empty(),
            _ => true,
        }
    }
}

impl ConfigRecord for ConnectionConfig {
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
        match &self.kind {
            ConnectionKind::Mysql(_) => ConfigKind::ConnectionMysql,
            ConnectionKind::Redis(_) => ConfigKind::ConnectionRedis,
            ConnectionKind::Weaviate(_) => ConfigKind::ConnectionWeaviate,
            ConnectionKind::Rustfs(_) => ConfigKind::ConnectionRustfs,
            ConnectionKind::BotAdapter(_) => ConfigKind::ConnectionBotAdapter,
            ConnectionKind::Tavily(_) => ConfigKind::ConnectionTavily,
        }
    }

    fn validate(&self) -> Result<()> {
        if self.canonical_config_id().trim().is_empty() {
            return Err(zihuan_core::string_error!(
                "connection config_id must not be empty"
            ));
        }
        if self.name.trim().is_empty() {
            return Err(zihuan_core::string_error!(
                "connection name must not be empty"
            ));
        }
        Ok(())
    }

    fn redacted_summary(&self) -> serde_json::Value {
        json!({
            "config_id": self.canonical_config_id(),
            "kind": self.kind(),
            "name": self.name,
            "enabled": self.enabled,
        })
    }
}

pub fn load_connections() -> Result<Vec<ConnectionConfig>> {
    let center = ConfigCenter::shared();
    let mut migrated = Vec::new();
    let connections = center
        .list_configs(ConfigCategory::Connection)?
        .into_iter()
        .map(|record| {
            let (connection, did_migrate) = connection_from_record(record)?;
            if did_migrate {
                migrated.push(connection.clone());
            }
            Ok(connection)
        })
        .collect::<Result<Vec<_>>>()?;
    for connection in migrated {
        center.upsert_config(connection_to_record(&connection)?)?;
        info!(
            "[config_center] migrated weaviate connection config_id={} collection_schema={:?}",
            connection.canonical_config_id(),
            match &connection.kind {
                ConnectionKind::Weaviate(weaviate) => Some(weaviate.collection_schema),
                _ => None,
            }
        );
    }
    for connection in &connections {
        debug!(
            "[config_center] loaded connection config_id={} kind={:?} name='{}'",
            connection.canonical_config_id(),
            connection.kind(),
            connection.name
        );
    }
    Ok(connections)
}

pub fn save_connections(connections: Vec<ConnectionConfig>) -> Result<()> {
    let center = ConfigCenter::shared();
    let existing = center.list_configs(ConfigCategory::Connection)?;
    let existing_ids = existing
        .into_iter()
        .map(|record| record.config_id)
        .collect::<std::collections::HashSet<_>>();
    let mut incoming_ids = std::collections::HashSet::new();

    for connection in connections {
        let normalized = normalize_connection_identity(connection, center.new_config_id());
        incoming_ids.insert(normalized.config_id.clone());
        center.upsert_config(connection_to_record(&normalized)?)?;
    }

    for config_id in existing_ids {
        if !incoming_ids.contains(&config_id) {
            let _ = center.delete_config(ConfigCategory::Connection, &config_id)?;
        }
    }

    Ok(())
}

fn normalize_connection_identity(
    mut connection: ConnectionConfig,
    fallback_id: String,
) -> ConnectionConfig {
    let canonical = if connection.config_id.trim().is_empty() {
        if connection.id.trim().is_empty() {
            fallback_id
        } else {
            connection.id.clone()
        }
    } else {
        connection.config_id.clone()
    };
    connection.id = canonical.clone();
    connection.config_id = canonical;
    connection
}

fn connection_to_record(connection: &ConnectionConfig) -> Result<StoredConfigRecord> {
    connection.validate()?;
    Ok(StoredConfigRecord {
        config_id: connection.canonical_config_id().to_string(),
        kind: connection.kind(),
        name: connection.name.clone(),
        enabled: connection.enabled,
        updated_at: connection.updated_at.clone(),
        spec: serde_json::to_value(&connection.kind)?,
    })
}

fn connection_from_record(record: StoredConfigRecord) -> Result<(ConnectionConfig, bool)> {
    if record.kind.category() != ConfigCategory::Connection {
        return Err(zihuan_core::string_error!(
            "config '{}' is not a connection config",
            record.config_id
        ));
    }
    let (spec, migrated) = migrate_connection_spec(&record);
    let kind = serde_json::from_value::<ConnectionKind>(spec).map_err(|err| {
        zihuan_core::string_error!(
            "failed to parse connection spec for '{}': {}",
            record.config_id,
            err
        )
    })?;
    Ok((
        ConnectionConfig {
            id: record.config_id.clone(),
            config_id: record.config_id,
            name: record.name,
            enabled: record.enabled,
            kind,
            updated_at: record.updated_at,
        },
        migrated,
    ))
}

fn migrate_connection_spec(record: &StoredConfigRecord) -> (Value, bool) {
    if record.kind != ConfigKind::ConnectionWeaviate {
        return (record.spec.clone(), false);
    }
    let mut spec = record.spec.clone();
    let Some(object) = spec.as_object_mut() else {
        return (spec, false);
    };
    if object.contains_key("collection_schema") {
        return (spec, false);
    }
    let class_name = object
        .get("class_name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let inferred = infer_weaviate_collection_schema(&record.name, class_name);
    object.insert(
        "collection_schema".to_string(),
        serde_json::to_value(inferred)
            .unwrap_or_else(|_| Value::String("message_record_semantic".to_string())),
    );
    (spec, true)
}

pub fn infer_weaviate_collection_schema(
    connection_name: &str,
    class_name: &str,
) -> WeaviateCollectionSchema {
    let haystack = format!("{connection_name} {class_name}").to_lowercase();
    if ["image", "img", "picture", "photo", "图片", "图像"]
        .iter()
        .any(|needle| haystack.contains(needle))
    {
        WeaviateCollectionSchema::ImageSemantic
    } else {
        WeaviateCollectionSchema::MessageRecordSemantic
    }
}

pub fn init_node_registry() -> Result<()> {
    use zihuan_graph_engine::image_weaviate_persistence::ImageWeaviatePersistenceNode;
    use zihuan_graph_engine::message_mysql_get_group_history::MessageMySQLGetGroupHistoryNode;
    use zihuan_graph_engine::message_mysql_get_user_history::MessageMySQLGetUserHistoryNode;
    use zihuan_graph_engine::message_mysql_search::MessageMySQLSearchNode;
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
    register_node!(
        "message_mysql_search",
        "搜索消息记录",
        "消息存储",
        "在消息记录中搜索，支持发送者、群组、内容关键词、时间范围过滤",
        MessageMySQLSearchNode
    );
    register_node!(
        "tavily_provider",
        "Tavily Provider",
        "AI",
        "从系统连接中选择 Tavily 配置，输出 TavilyRef 引用",
        tavily_provider_node::TavilyProviderNode
    );
    register_node!(
        "tavily_search",
        "Tavily 搜索",
        "AI",
        "使用 TavilyRef 执行 Tavily 搜索并输出包含标题、链接和内容的 Vec<String>",
        tavily_search_node::TavilySearchNode
    );
    register_node!(
        "weaviate_image_search",
        "Weaviate 图片检索",
        "AI",
        "使用本地 Weaviate 图片库做语义检索，输出标准化图片结果 JSON",
        weaviate_image_search_node::WeaviateImageSearchNode
    );

    Ok(())
}
