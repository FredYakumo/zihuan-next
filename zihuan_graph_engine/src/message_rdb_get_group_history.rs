use crate::message_rdb_history_common::{
    aggregate_history_rows, format_history_messages, group_history_query, history_query_row_limit,
    message_history_chunk_row_from_row, run_mysql_query,
};
use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;
use zihuan_core::error::{Error, Result};

pub struct MessageRdbGetGroupHistoryNode {
    id: String,
    name: String,
}

impl MessageRdbGetGroupHistoryNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

fn extract_limit(inputs: &HashMap<String, DataValue>) -> Result<u32> {
    let limit = inputs
        .get("limit")
        .and_then(|value| match value {
            DataValue::Integer(limit) => Some(*limit),
            _ => None,
        })
        .ok_or_else(|| Error::InvalidNodeInput("limit is required".to_string()))?;

    if limit <= 0 {
        return Err(Error::ValidationError("limit must be greater than 0".to_string()));
    }

    Ok(limit as u32)
}

impl Node for MessageRdbGetGroupHistoryNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("按群查询最近消息历史")
    }

    node_input![
        port! { name = "mysql_ref", ty = RdbRef, desc = "关系数据库连接引用" },
        port! { name = "group_id", ty = String, desc = "要查询的群 ID" },
        port! { name = "limit", ty = Integer, desc = "要读取的最近消息数量" },
    ];

    node_output![port! { name = "messages", ty = Vec(String), desc = "格式化后的历史消息列表" },];

    fn execute(&mut self, inputs: crate::NodeInputFlow) -> Result<crate::NodeOutputFlow> {
        self.validate_inputs(&inputs)?;

        let rdb_pool = inputs
            .get("mysql_ref")
            .and_then(|value| match value {
                DataValue::RdbRef(connection) => Some(connection.clone()),
                _ => None,
            })
            .ok_or_else(|| Error::InvalidNodeInput("mysql_ref is required".to_string()))?;

        let mysql_config = match rdb_pool {
            zihuan_core::data_refs::RelationalDbConnection::MySql(config) => config,
            _ => return Err(Error::InvalidNodeInput("mysql_ref must be a MySQL connection".to_string())),
        };

        let group_id = inputs
            .get("group_id")
            .and_then(|value| match value {
                DataValue::String(group_id) => Some(group_id.clone()),
                _ => None,
            })
            .ok_or_else(|| Error::InvalidNodeInput("group_id is required".to_string()))?;

        let limit = extract_limit(&inputs)?;

        let rows = run_mysql_query(&mysql_config, move |pool| {
            Box::pin(async move {
                sqlx::query(group_history_query())
                    .bind(&group_id)
                    .bind(history_query_row_limit(limit))
                    .fetch_all(pool)
                    .await
            })
        })?;

        let messages = format_history_messages(aggregate_history_rows(
            rows.into_iter().map(message_history_chunk_row_from_row).collect(),
            limit as usize,
        ));

        crate::return_with_node_output![self;
            "messages" => DataValue::Vec(
                Box::new(DataType::String),
                messages.into_iter().map(DataValue::String).collect(),
            ),
        ]
    }
}
