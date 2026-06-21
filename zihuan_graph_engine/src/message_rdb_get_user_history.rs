use crate::message_rdb_history_common::{
    aggregate_history_rows, format_history_messages, history_query_row_limit, message_history_chunk_row_from_row,
    run_mysql_query, user_history_query,
};
use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;
use zihuan_core::error::{Error, Result};

pub struct MessageRdbGetUserHistoryNode {
    id: String,
    name: String,
}

impl MessageRdbGetUserHistoryNode {
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

fn extract_optional_group_id(inputs: &HashMap<String, DataValue>) -> Result<Option<String>> {
    match inputs.get("group_id") {
        Some(DataValue::String(group_id)) => {
            let group_id = group_id.trim();
            if group_id.is_empty() {
                Ok(None)
            } else {
                Ok(Some(group_id.to_string()))
            }
        }
        Some(_) => Err(Error::InvalidNodeInput("group_id must be a string".to_string())),
        None => Ok(None),
    }
}

impl Node for MessageRdbGetUserHistoryNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("按发送者查询最近消息历史，可选限定某个群")
    }

    node_input![
        port! { name = "mysql_ref", ty = RdbRef, desc = "关系数据库连接引用" },
        port! { name = "sender_id", ty = String, desc = "要查询的发送者 ID" },
        port! { name = "group_id", ty = String, desc = "可选的群 ID 过滤条件", optional },
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

        let sender_id = inputs
            .get("sender_id")
            .and_then(|value| match value {
                DataValue::String(sender_id) => Some(sender_id.clone()),
                _ => None,
            })
            .ok_or_else(|| Error::InvalidNodeInput("sender_id is required".to_string()))?;

        let group_id = extract_optional_group_id(&inputs)?;
        let limit = extract_limit(&inputs)?;

        let query_group_id = group_id.clone();
        let query_sender_id = sender_id.clone();

        let rows = run_mysql_query(&mysql_config, move |pool| {
            Box::pin(async move {
                if let Some(group_id) = query_group_id {
                    sqlx::query(user_history_query(Some(group_id.as_str())))
                        .bind(&query_sender_id)
                        .bind(&group_id)
                        .bind(history_query_row_limit(limit))
                        .fetch_all(pool)
                        .await
                } else {
                    sqlx::query(user_history_query(None))
                        .bind(&query_sender_id)
                        .bind(history_query_row_limit(limit))
                        .fetch_all(pool)
                        .await
                }
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
