use std::sync::Arc;

use zihuan_core::data_refs::{RelationalDbConnection, SqliteConfig};
use zihuan_core::error::{Error, Result};
use zihuan_graph_engine::{DataType, DataValue, Node, NodeConfigField, NodeConfigWidget, Port};

use crate::RuntimeStorageConnectionManager;

const CONFIG_ID_FIELD: &str = "config_id";

pub fn get_pool(sqlite_ref: &Arc<SqliteConfig>) -> Option<&sqlx::sqlite::SqlitePool> {
    sqlite_ref.pool.as_ref()
}

pub struct SqliteNode {
    id: String,
    name: String,
    config_id: Option<String>,
}

impl SqliteNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            config_id: None,
        }
    }

    fn connection_select_field() -> NodeConfigField {
        NodeConfigField::new(CONFIG_ID_FIELD, DataType::String, NodeConfigWidget::ConnectionSelect)
            .with_connection_kind("sqlite")
            .with_description("选择系统中的 SQLite 连接配置")
    }

    fn selected_config_id(&self) -> Result<&str> {
        self.config_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| Error::ValidationError("config_id is required".to_string()))
    }
}

impl Node for SqliteNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("SQLite连接配置 - 从系统连接中选择并输出 SqliteRef")
    }

    fn input_ports(&self) -> Vec<Port> {
        Vec::new()
    }

    fn output_ports(&self) -> Vec<Port> {
        vec![Port::new("sqlite_ref", DataType::RdbRef).with_description("关系数据库连接引用")]
    }

    fn config_fields(&self) -> Vec<NodeConfigField> {
        vec![Self::connection_select_field()]
    }

    fn apply_inline_config(&mut self, inline_values: &zihuan_graph_engine::NodeConfigFlow) -> Result<()> {
        self.config_id = inline_values.get(CONFIG_ID_FIELD).and_then(|value| match value {
            DataValue::String(value) => Some(value.clone()),
            _ => None,
        });
        Ok(())
    }

    fn execute(&mut self, _inputs: zihuan_graph_engine::NodeInputFlow) -> Result<zihuan_graph_engine::NodeOutputFlow> {
        let config_id = self.selected_config_id()?;
        let config = zihuan_core::runtime::block_async(
            RuntimeStorageConnectionManager::shared().get_or_create_sqlite_ref(config_id),
        )?;
        zihuan_graph_engine::return_with_node_output![self;
            "sqlite_ref" => DataValue::RdbRef(RelationalDbConnection::Sqlite(config)),
        ]
    }
}
