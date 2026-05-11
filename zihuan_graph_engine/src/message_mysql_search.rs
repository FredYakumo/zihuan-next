use crate::message_mysql_history_common::{
    aggregate_history_rows, format_history_messages, message_history_chunk_row_from_row,
    run_mysql_query, SearchMessagesQueryBuilder,
};
use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;
use zihuan_core::error::{Error, Result};

pub struct MessageMySQLSearchNode {
    id: String,
    name: String,
}

impl MessageMySQLSearchNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for MessageMySQLSearchNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("在消息记录中搜索，支持发送者、群组、内容关键词、时间范围过滤")
    }

    node_input![
        port! { name = "mysql_ref", ty = MySqlRef, desc = "MySQL连接配置引用" },
        port! { name = "sender_id", ty = String, desc = "可选：按发送者ID过滤", optional },
        port! { name = "group_id", ty = String, desc = "可选：按群ID过滤", optional },
        port! { name = "contain", ty = String, desc = "可选：消息内容包含的关键词（模糊匹配）", optional },
        port! { name = "start_time", ty = String, desc = "可选：时间范围起始（YYYY-MM-DD HH:MM:SS）", optional },
        port! { name = "end_time", ty = String, desc = "可选：时间范围结束（YYYY-MM-DD HH:MM:SS）", optional },
        port! { name = "limit", ty = Integer, desc = "返回消息数量" },
        port! { name = "sort_by_time_desc", ty = Boolean, desc = "是否按发送时间从新到旧排序，默认true" },
    ];

    node_output![
        port! { name = "messages", ty = Vec(String), desc = "格式化后的搜索结果消息列表" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let mysql_config = inputs
            .get("mysql_ref")
            .and_then(|value| match value {
                DataValue::MySqlRef(config) => Some(config.clone()),
                _ => None,
            })
            .ok_or_else(|| Error::InvalidNodeInput("mysql_ref is required".to_string()))?;

        let sender_id = inputs.get("sender_id").and_then(|value| match value {
            DataValue::String(s) => {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }
            _ => None,
        });

        let group_id = inputs.get("group_id").and_then(|value| match value {
            DataValue::String(s) => {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }
            _ => None,
        });

        let contain = inputs.get("contain").and_then(|value| match value {
            DataValue::String(s) => {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }
            _ => None,
        });

        let start_time = inputs.get("start_time").and_then(|value| match value {
            DataValue::String(s) => {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }
            _ => None,
        });

        let end_time = inputs.get("end_time").and_then(|value| match value {
            DataValue::String(s) => {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }
            _ => None,
        });

        let limit = inputs
            .get("limit")
            .and_then(|value| match value {
                DataValue::Integer(limit) => Some(*limit),
                _ => None,
            })
            .unwrap_or(100);

        if limit <= 0 {
            return Err(Error::ValidationError(
                "limit must be greater than 0".to_string(),
            ));
        }

        let sort_by_time_desc = inputs
            .get("sort_by_time_desc")
            .and_then(|value| match value {
                DataValue::Boolean(b) => Some(*b),
                _ => None,
            })
            .unwrap_or(true);

        let builder = SearchMessagesQueryBuilder {
            sender_id,
            group_id,
            contain,
            start_time,
            end_time,
            sort_by_time_desc,
            limit: limit as u32,
        };

        let (sql, params) = builder.build();

        let rows = run_mysql_query(&mysql_config, move |pool| {
            Box::pin(async move {
                let mut query = sqlx::query(&sql);
                for param in &params {
                    query = query.bind(param);
                }
                query.fetch_all(pool).await
            })
        })?;

        let messages = format_history_messages(aggregate_history_rows(
            rows.into_iter()
                .map(message_history_chunk_row_from_row)
                .collect(),
            limit as usize,
        ));

        let mut outputs = HashMap::new();
        outputs.insert(
            "messages".to_string(),
            DataValue::Vec(
                Box::new(DataType::String),
                messages.into_iter().map(DataValue::String).collect(),
            ),
        );
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
