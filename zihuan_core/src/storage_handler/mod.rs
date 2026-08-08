mod agent_avatar_rdb_store;
pub mod agent_avatar_store;
mod agent_memory_weaviate;
mod connection_manager;
mod db_schema;
mod elasticsearch;
mod image_weaviate_persistence;
mod message_record;
pub mod mysql;
pub mod object_storage;
mod qq_message_list_weaviate_persistence;
pub mod rdb;
pub mod redis;
pub mod resource_resolver;
pub mod rustfs;
pub mod sqlite;
mod tavily_provider_node;
mod tavily_search_node;
pub mod weaviate;
mod weaviate_client;
mod weaviate_image_search_node;
mod weaviate_persistence;
mod weaviate_schema;

use log::{debug, info};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::config::{ConfigCategory, ConfigCenter, ConfigKind, ConfigRecord, StoredConfigRecord};
use crate::error::Result;
pub use crate::weaviate::WeaviateCollectionSchema;

pub use agent_avatar_rdb_store::{first_available_agent_avatar_store, RdbAgentAvatarStore};
pub use agent_avatar_store::{AgentAvatarData, AgentAvatarStore};
pub use agent_memory_weaviate::{
    create_memory_record, create_memory_record_with_vector, delete_memory_record, extend_expiry_for_hits,
    get_memory_record, is_memory_expired, list_recent_memory_keys, memory_is_accessible, normalize_memory_scope_lists,
    search_memory_content, search_memory_content_by_vector, update_memory_record, update_memory_record_with_vector,
    AgentMemoryAccessContext, AgentMemoryRecord, AgentMemorySearchHit, AgentMemoryUpsert,
};
pub use connection_manager::{
    cleanup_runtime_storage_instances, close_runtime_storage_instance, close_runtime_storage_instances_for_config,
    list_runtime_storage_instances, RuntimeStorageConnectionManager, StorageRuntimeHandle,
};
pub use db_schema::ensure_tables_for_connection;
pub use elasticsearch::{
    create_elasticsearch_memory_record, ensure_elasticsearch_index, list_elasticsearch_memory_keys,
    search_elasticsearch_images, search_elasticsearch_memory, upsert_elasticsearch_image,
    ElasticsearchImageSearchHit, ElasticsearchIndexSchema, ElasticsearchRef,
};
pub use message_record::MessageRecord;
pub use mysql::MySqlNode;
pub use object_storage::{
    enrich_event_images, enrich_message_images, save_image_to_object_storage, upload_remote_image_to_s3,
    ImageCacheAdapter, ImageObjectStorageInput, ObjectStorageConfig, PendingImageUpload, SavedImageObject,
};
pub use rdb::{build_relational_db_connection_for_connection, build_relational_db_connection_for_kind};
pub use redis::RedisNode;
pub use resource_resolver::{
    build_elasticsearch_ref, build_rdb_ref, build_redis_ref, build_s3_ref, build_weaviate_ref,
    build_web_search_engine_ref, find_connection, resolve_connection_data_value,
};
pub use rustfs::RustfsNode;
pub use sqlite::SqliteNode;
pub use weaviate::WeaviateNode;
pub use weaviate_client::WeaviateClient;
pub use weaviate_persistence::{
    build_image_record_properties, deterministic_media_object_id, deterministic_message_object_id, upsert_image_record,
    upsert_message_event, upsert_qq_message_list,
};
pub use weaviate_schema::{
    collection_config_for_schema, ensure_collection_schema, validate_collection_schema,
};

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
    Elasticsearch(ElasticsearchConnection),
    Rustfs(RustfsConnection),
    BotAdapter(serde_json::Value),
    WebSearchEngine(WebSearchEngineConnection),
    Tokenizer(TokenizerConnection),
    Sqlite(SqliteConnection),
}

pub const DEFAULT_MYSQL_MAX_CONNECTIONS: u32 = 32;
pub const DEFAULT_MYSQL_ACQUIRE_TIMEOUT_SECS: u64 = 30;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MysqlConnection {
    pub url: String,
    #[serde(default = "default_mysql_max_connections")]
    pub max_connections: u32,
    #[serde(default = "default_mysql_acquire_timeout_secs")]
    pub acquire_timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedisConnection {
    pub url: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionAuthMethod {
    Password,
    ApiKey,
}

impl Default for ConnectionAuthMethod {
    fn default() -> Self {
        Self::ApiKey
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeaviateConnection {
    pub base_url: String,
    pub class_name: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub auth_method: ConnectionAuthMethod,
    pub collection_schema: WeaviateCollectionSchema,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElasticsearchConnection {
    pub base_url: String,
    pub index_name: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub auth_method: ConnectionAuthMethod,
    pub collection_schema: WeaviateCollectionSchema,
    pub vector_dimensions: usize,
}

pub fn validate_connection_authentication(
    auth_method: ConnectionAuthMethod,
    username: Option<&str>,
    password: Option<&str>,
    api_key: Option<&str>,
    connection_type: &str,
) -> Result<()> {
    let username = username.map(str::trim).filter(|value| !value.is_empty());
    let password = password.map(str::trim).filter(|value| !value.is_empty());
    let api_key = api_key.map(str::trim).filter(|value| !value.is_empty());

    match auth_method {
        ConnectionAuthMethod::Password => {
            if username.is_none() || password.is_none() {
                return Err(crate::string_error!(
                    "{connection_type} username and password are required when auth_method is password"
                ));
            }
            if api_key.is_some() {
                return Err(crate::string_error!(
                    "{connection_type} api_key must be empty when auth_method is password"
                ));
            }
        }
        ConnectionAuthMethod::ApiKey => {
            if api_key.is_none() {
                return Err(crate::string_error!(
                    "{connection_type} api_key is required when auth_method is api_key"
                ));
            }
            if username.is_some() || password.is_some() {
                return Err(crate::string_error!(
                    "{connection_type} username and password must be empty when auth_method is api_key"
                ));
            }
        }
    }
    Ok(())
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
pub struct WebSearchEngineConnection {
    pub provider: String,
    #[serde(default)]
    pub api_token: Option<String>,
    #[serde(default = "default_web_search_engine_timeout_secs")]
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenizerConnection {
    pub model_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqliteConnection {
    pub path: String,
}

fn default_web_search_engine_timeout_secs() -> u64 {
    30
}

fn default_mysql_max_connections() -> u32 {
    DEFAULT_MYSQL_MAX_CONNECTIONS
}

fn default_mysql_acquire_timeout_secs() -> u64 {
    DEFAULT_MYSQL_ACQUIRE_TIMEOUT_SECS
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
            ConnectionKind::Mysql(mysql) => {
                !mysql.url.trim().is_empty() && mysql.max_connections > 0 && mysql.acquire_timeout_secs > 0
            }
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
            ConnectionKind::Elasticsearch(_) => ConfigKind::ConnectionElasticsearch,
            ConnectionKind::Rustfs(_) => ConfigKind::ConnectionRustfs,
            ConnectionKind::BotAdapter(_) => ConfigKind::ConnectionBotAdapter,
            ConnectionKind::WebSearchEngine(_) => ConfigKind::ConnectionWebSearchEngine,
            ConnectionKind::Tokenizer(_) => ConfigKind::ConnectionTokenizer,
            ConnectionKind::Sqlite(_) => ConfigKind::ConnectionSqlite,
        }
    }

    fn validate(&self) -> Result<()> {
        if self.canonical_config_id().trim().is_empty() {
            return Err(crate::string_error!("connection config_id must not be empty"));
        }
        if self.name.trim().is_empty() {
            return Err(crate::string_error!("connection name must not be empty"));
        }
        if let ConnectionKind::Mysql(mysql) = &self.kind {
            if mysql.url.trim().is_empty() {
                return Err(crate::string_error!("mysql.url must not be empty"));
            }
            if mysql.max_connections == 0 {
                return Err(crate::string_error!("mysql.max_connections must be greater than 0"));
            }
            if mysql.acquire_timeout_secs == 0 {
                return Err(crate::string_error!("mysql.acquire_timeout_secs must be greater than 0"));
            }
        }
        if let ConnectionKind::Sqlite(sqlite) = &self.kind {
            if sqlite.path.trim().is_empty() {
                return Err(crate::string_error!("sqlite.path must not be empty"));
            }
        }
        if let ConnectionKind::Elasticsearch(elasticsearch) = &self.kind {
            validate_connection_authentication(
                elasticsearch.auth_method,
                elasticsearch.username.as_deref(),
                elasticsearch.password.as_deref(),
                elasticsearch.api_key.as_deref(),
                "elasticsearch",
            )?;
            let reference = crate::storage_handler::ElasticsearchRef::new(elasticsearch.clone())?;
            let _ = reference;
        }
        if let ConnectionKind::Weaviate(weaviate) = &self.kind {
            validate_connection_authentication(
                weaviate.auth_method,
                weaviate.username.as_deref(),
                weaviate.password.as_deref(),
                weaviate.api_key.as_deref(),
                "weaviate",
            )?;
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
        center.upsert_config(StoredConfigRecord {
            config_id: connection.canonical_config_id().to_string(),
            kind: connection.kind(),
            name: connection.name.clone(),
            enabled: connection.enabled,
            updated_at: connection.updated_at.clone(),
            spec: serde_json::to_value(&connection.kind)?,
        })?;
        info!(
            "[config_center] migrated connection config_id={} kind={:?}",
            connection.canonical_config_id(),
            connection.kind(),
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

pub fn connection_exists(config_id: &str) -> Result<bool> {
    let record = ConfigCenter::shared().get_config(config_id)?;
    Ok(matches!(record, Some(record) if record.kind.category() == ConfigCategory::Connection))
}

pub fn upsert_connection(connection: ConnectionConfig) -> Result<ConnectionConfig> {
    let center = ConfigCenter::shared();
    let connection = normalize_connection_identity(connection, center.new_config_id());
    center.upsert_config(connection_to_record(&connection)?)?;
    Ok(connection)
}

pub fn delete_connection(config_id: &str) -> Result<bool> {
    ConfigCenter::shared().delete_config(ConfigCategory::Connection, config_id)
}

fn normalize_connection_identity(mut connection: ConnectionConfig, fallback_id: String) -> ConnectionConfig {
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
        return Err(crate::string_error!(
            "config '{}' is not a connection config",
            record.config_id
        ));
    }
    let (spec, migrated) = migrate_connection_spec(&record);
    let kind = serde_json::from_value::<ConnectionKind>(spec).map_err(|err| {
        crate::string_error!("failed to parse connection spec for '{}': {}", record.config_id, err)
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
    if record.kind != ConfigKind::ConnectionWeaviate && record.kind != ConfigKind::ConnectionElasticsearch {
        return (record.spec.clone(), false);
    }
    let mut spec = record.spec.clone();
    let Some(object) = spec.as_object_mut() else {
        return (spec, false);
    };
    let mut migrated = false;
    if record.kind == ConfigKind::ConnectionWeaviate && !object.contains_key("collection_schema") {
        let class_name = object.get("class_name").and_then(Value::as_str).unwrap_or_default();
        let inferred = infer_weaviate_collection_schema(&record.name, class_name);
        object.insert(
            "collection_schema".to_string(),
            serde_json::to_value(inferred).unwrap_or_else(|_| Value::String("agent_memory".to_string())),
        );
        migrated = true;
    }
    if !object.contains_key("auth_method") {
        let api_key = object.get("api_key").and_then(Value::as_str).is_some_and(|value| !value.trim().is_empty());
        let has_password_credentials = object
            .get("username")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.trim().is_empty())
            || object
                .get("password")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.trim().is_empty());
        let auth_method = if api_key {
            object.insert("username".to_string(), Value::Null);
            object.insert("password".to_string(), Value::Null);
            ConnectionAuthMethod::ApiKey
        } else if has_password_credentials {
            object.insert("api_key".to_string(), Value::Null);
            ConnectionAuthMethod::Password
        } else {
            object.insert("username".to_string(), Value::Null);
            object.insert("password".to_string(), Value::Null);
            object.insert("api_key".to_string(), Value::Null);
            ConnectionAuthMethod::ApiKey
        };
        object.insert(
            "auth_method".to_string(),
            serde_json::to_value(auth_method).unwrap_or_else(|_| Value::String("api_key".to_string())),
        );
        migrated = true;
    }
    (spec, migrated)
}

pub fn infer_weaviate_collection_schema(connection_name: &str, class_name: &str) -> WeaviateCollectionSchema {
    let haystack = format!("{connection_name} {class_name}").to_lowercase();
    if ["image", "img", "picture", "photo", "图片", "图像"]
        .iter()
        .any(|needle| haystack.contains(needle))
    {
        WeaviateCollectionSchema::ImageSemantic
    } else {
        WeaviateCollectionSchema::AgentMemory
    }
}

pub fn init_node_registry() -> Result<()> {
    use crate::storage_handler::image_weaviate_persistence::ImageWeaviatePersistenceNode;
    use crate::storage_handler::qq_message_list_weaviate_persistence::QQMessageListWeaviatePersistenceNode;
    use crate::graph_engine::message_rdb_get_group_history::MessageRdbGetGroupHistoryNode;
    use crate::graph_engine::message_rdb_get_user_history::MessageRdbGetUserHistoryNode;
    use crate::graph_engine::message_rdb_search::MessageRdbSearchNode;
    use crate::graph_engine::qq_message_list_rdb_persistence::QQMessageListRdbPersistenceNode;
    use crate::register_node;

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
        "sqlite",
        "SQLite连接",
        "数据库",
        "从系统连接配置中选择 SQLite 并输出 SqliteRef 引用",
        SqliteNode
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
        "qq_message_list_rdb_persistence",
        "QQMessage列表RDB持久化",
        "消息存储",
        "将Vec<QQMessage>及调用方提供的元数据持久化到关系数据库",
        QQMessageListRdbPersistenceNode
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
        "message_rdb_get_user_history",
        "获取QQ号消息历史",
        "消息存储",
        "根据 sender_id 读取最近消息历史，可选限定某个群",
        MessageRdbGetUserHistoryNode
    );
    register_node!(
        "message_rdb_get_group_history",
        "获取QQ群聊消息历史",
        "消息存储",
        "根据 group_id 读取最近消息历史",
        MessageRdbGetGroupHistoryNode
    );
    register_node!(
        "message_rdb_search",
        "搜索消息记录",
        "消息存储",
        "在消息记录中搜索，支持发送者、群组、内容关键词、时间范围过滤",
        MessageRdbSearchNode
    );
    register_node!(
        "tavily_provider",
        "Web Search Engine Provider",
        "AI",
        "从系统连接中选择 Web Search Engine 配置，输出 WebSearchEngineRef 引用",
        tavily_provider_node::TavilyProviderNode
    );
    register_node!(
        "tavily_search",
        "网页搜索",
        "AI",
        "使用 WebSearchEngineRef 执行网页搜索并输出包含标题、链接和内容的 Vec<String>",
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
