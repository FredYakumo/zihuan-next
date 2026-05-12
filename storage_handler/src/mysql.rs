use std::collections::HashMap;
use std::sync::Arc;

use zihuan_core::data_refs::MySqlConfig;
use zihuan_core::error::{Error, Result};
use zihuan_graph_engine::message_restore::register_mysql_ref;
use zihuan_graph_engine::{DataType, DataValue, Node, NodeConfigField, NodeConfigWidget, Port};

use crate::RuntimeStorageConnectionManager;

const CONFIG_ID_FIELD: &str = "config_id";
const LEGACY_CONNECTION_ID_FIELD: &str = "connection_id";

pub async fn build_mysql_ref(url: &str) -> Result<Arc<MySqlConfig>> {
    let pool = sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(10)
        .min_connections(1)
        .connect(url)
        .await?;

    Ok(Arc::new(MySqlConfig {
        url: Some(url.to_string()),
        reconnect_max_attempts: None,
        reconnect_interval_secs: None,
        pool: Some(pool),
        runtime_handle: tokio::runtime::Handle::try_current().ok(),
    }))
}

pub fn get_pool(mysql_ref: &Arc<MySqlConfig>) -> Option<&sqlx::mysql::MySqlPool> {
    mysql_ref.pool.as_ref()
}

pub struct MySqlNode {
    id: String,
    name: String,
    config_id: Option<String>,
}

impl MySqlNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            config_id: None,
        }
    }

    fn connection_select_field() -> NodeConfigField {
        NodeConfigField::new(
            CONFIG_ID_FIELD,
            DataType::String,
            NodeConfigWidget::ConnectionSelect,
        )
        .with_connection_kind("mysql")
        .with_description("选择系统中的 MySQL 连接配置")
    }

    fn selected_config_id(&self) -> Result<&str> {
        self.config_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| Error::ValidationError("config_id is required".to_string()))
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
        self.config_id = inline_values
            .get(CONFIG_ID_FIELD)
            .or_else(|| inline_values.get(LEGACY_CONNECTION_ID_FIELD))
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
        let config_id = self.selected_config_id()?;
        let config = zihuan_core::runtime::block_async(
            RuntimeStorageConnectionManager::shared().get_or_create_mysql_ref(config_id),
        )?;
        register_mysql_ref(config.clone());
        Ok(HashMap::from([(
            "mysql_ref".to_string(),
            DataValue::MySqlRef(config),
        )]))
    }
}
