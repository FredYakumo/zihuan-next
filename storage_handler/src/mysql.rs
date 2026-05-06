use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use sqlx::mysql::{MySqlPool, MySqlPoolOptions};
use zihuan_core::error::{Error, Result};
use zihuan_graph_engine::data_value::MySqlConfig;
use zihuan_graph_engine::message_restore::register_mysql_ref;
use zihuan_graph_engine::{DataType, DataValue, Node, NodeConfigField, NodeConfigWidget, Port};

use crate::{find_connection, load_connections, ConnectionKind};

const CONNECTION_ID_FIELD: &str = "connection_id";
const DEFAULT_MAX_CONNECTIONS: u32 = 10;
const DEFAULT_ACQUIRE_TIMEOUT_SECS: u64 = 30;

pub async fn build_mysql_ref(url: &str) -> Result<Arc<MySqlConfig>> {
    let pool = MySqlPoolOptions::new()
        .max_connections(DEFAULT_MAX_CONNECTIONS)
        .min_connections(1)
        .connect(url)
        .await?;

    Ok(Arc::new(MySqlConfig {
        url: Some(url.to_string()),
        reconnect_max_attempts: None,
        reconnect_interval_secs: None,
        pool: Some(pool),
        runtime_handle: Some(tokio::runtime::Handle::current()),
    }))
}

pub fn get_pool(mysql_ref: &Arc<MySqlConfig>) -> Option<&MySqlPool> {
    mysql_ref.pool.as_ref()
}

pub struct MySqlNode {
    id: String,
    name: String,
    pool: Option<MySqlPool>,
    last_url: Option<String>,
    runtime: Option<tokio::runtime::Runtime>,
    runtime_handle: Option<tokio::runtime::Handle>,
    connection_id: Option<String>,
}

impl MySqlNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            pool: None,
            last_url: None,
            runtime: None,
            runtime_handle: None,
            connection_id: None,
        }
    }

    fn connection_select_field() -> NodeConfigField {
        NodeConfigField::new(
            CONNECTION_ID_FIELD,
            DataType::String,
            NodeConfigWidget::ConnectionSelect,
        )
        .with_connection_kind("mysql")
        .with_description("选择系统中的 MySQL 连接配置")
    }

    fn ensure_runtime_handle(&mut self) -> Result<tokio::runtime::Handle> {
        if let Some(handle) = &self.runtime_handle {
            return Ok(handle.clone());
        }

        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            self.runtime_handle = Some(handle.clone());
            return Ok(handle);
        }

        let runtime = tokio::runtime::Runtime::new()?;
        let handle = runtime.handle().clone();
        self.runtime = Some(runtime);
        self.runtime_handle = Some(handle.clone());
        Ok(handle)
    }

    fn get_or_create_pool(
        &mut self,
        url: &str,
        max_connections: u32,
        acquire_timeout_secs: u64,
    ) -> Result<MySqlPool> {
        if self.last_url.as_deref() == Some(url) {
            if let Some(ref pool) = self.pool {
                return Ok(pool.clone());
            }
        }

        let handle = self.ensure_runtime_handle()?;
        let url_str = url.to_string();
        let pool_opts = MySqlPoolOptions::new()
            .max_connections(max_connections)
            .min_connections(1)
            .acquire_timeout(Duration::from_secs(acquire_timeout_secs))
            .idle_timeout(Duration::from_secs(600))
            .max_lifetime(Duration::from_secs(1800));
        let pool = zihuan_core::runtime::block_async(pool_opts.connect(&url_str)).map_err(|e| {
            Error::StringError(format!("[MySqlNode] Failed to connect to MySQL: {e}"))
        })?;

        self.pool = Some(pool.clone());
        self.last_url = Some(url_str);
        self.runtime_handle = Some(handle);
        Ok(pool)
    }

    fn selected_url(&self) -> Result<String> {
        let connection_id = self
            .connection_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| Error::ValidationError("connection_id is required".to_string()))?;
        let connections = load_connections()?;
        let connection = find_connection(&connections, connection_id)?;
        let ConnectionKind::Mysql(mysql) = &connection.kind else {
            return Err(Error::ValidationError(format!(
                "connection '{}' is not a mysql connection",
                connection.name
            )));
        };
        if !connection.enabled {
            return Err(Error::ValidationError(format!(
                "connection '{}' is disabled",
                connection.name
            )));
        }
        Ok(mysql.url.clone())
    }
}

impl Node for MySqlNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("MySQL连接配置 - 从系统连接中选择并输出 MySqlRef")
    }

    fn input_ports(&self) -> Vec<Port> {
        Vec::new()
    }

    fn output_ports(&self) -> Vec<Port> {
        vec![Port::new("mysql_ref", DataType::MySqlRef).with_description("MySQL连接配置引用")]
    }

    fn config_fields(&self) -> Vec<NodeConfigField> {
        vec![Self::connection_select_field()]
    }

    fn apply_inline_config(&mut self, inline_values: &HashMap<String, DataValue>) -> Result<()> {
        self.connection_id = inline_values
            .get(CONNECTION_ID_FIELD)
            .and_then(|value| match value {
                DataValue::String(value) => Some(value.clone()),
                _ => None,
            });
        Ok(())
    }

    fn execute(
        &mut self,
        _inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        let url = self.selected_url()?;
        let pool =
            self.get_or_create_pool(&url, DEFAULT_MAX_CONNECTIONS, DEFAULT_ACQUIRE_TIMEOUT_SECS)?;

        let config = Arc::new(MySqlConfig {
            url: Some(url),
            reconnect_max_attempts: None,
            reconnect_interval_secs: None,
            pool: Some(pool),
            runtime_handle: self.runtime_handle.clone(),
        });
        register_mysql_ref(config.clone());

        Ok(HashMap::from([(
            "mysql_ref".to_string(),
            DataValue::MySqlRef(config),
        )]))
    }
}
