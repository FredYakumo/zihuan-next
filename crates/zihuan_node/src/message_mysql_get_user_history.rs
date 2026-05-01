use crate::message_mysql_history_common::{
    format_history_messages, message_history_record_from_row, run_mysql_query, user_history_query,
};
use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;
use zihuan_core::error::{Error, Result};

pub struct MessageMySQLGetUserHistoryNode {
    id: String,
    name: String,
}

impl MessageMySQLGetUserHistoryNode {
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
        return Err(Error::ValidationError(
            "limit must be greater than 0".to_string(),
        ));
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
        Some(_) => Err(Error::InvalidNodeInput(
            "group_id must be a string".to_string(),
        )),
        None => Ok(None),
    }
}

impl Node for MessageMySQLGetUserHistoryNode {
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
        port! { name = "mysql_ref", ty = MySqlRef, desc = "MySQL连接配置引用" },
        port! { name = "sender_id", ty = String, desc = "要查询的发送者 ID" },
        port! { name = "group_id", ty = String, desc = "可选的群 ID 过滤条件", optional },
        port! { name = "limit", ty = Integer, desc = "要读取的最近消息数量" },
    ];

    node_output![port! { name = "messages", ty = Vec(String), desc = "格式化后的历史消息列表" },];

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
                        .bind(i64::from(limit))
                        .fetch_all(pool)
                        .await
                } else {
                    sqlx::query(user_history_query(None))
                        .bind(&query_sender_id)
                        .bind(i64::from(limit))
                        .fetch_all(pool)
                        .await
                }
            })
        })?;

        let messages = format_history_messages(
            rows.into_iter()
                .map(message_history_record_from_row)
                .collect(),
        );

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

#[cfg(test)]
mod tests {
    use super::{extract_optional_group_id, MessageMySQLGetUserHistoryNode};
    use crate::data_value::MySqlConfig;
    use crate::message_mysql_history_common::user_history_query;
    use crate::{DataValue, Node};
    use std::collections::HashMap;
    use std::sync::Arc;

    fn mysql_ref_without_pool() -> DataValue {
        DataValue::MySqlRef(Arc::new(MySqlConfig {
            url: Some("mysql://user:pass@127.0.0.1:3306/demo".to_string()),
            reconnect_max_attempts: None,
            reconnect_interval_secs: None,
            pool: None,
            runtime_handle: None,
        }))
    }

    fn base_inputs() -> HashMap<String, DataValue> {
        HashMap::from([
            ("mysql_ref".to_string(), mysql_ref_without_pool()),
            (
                "sender_id".to_string(),
                DataValue::String("user-1".to_string()),
            ),
            ("limit".to_string(), DataValue::Integer(5)),
        ])
    }

    #[test]
    fn missing_mysql_ref_is_rejected() {
        let mut node = MessageMySQLGetUserHistoryNode::new("history", "History");
        let inputs = HashMap::from([
            (
                "sender_id".to_string(),
                DataValue::String("user-1".to_string()),
            ),
            ("limit".to_string(), DataValue::Integer(5)),
        ]);

        let error = node
            .execute(inputs)
            .expect_err("missing mysql_ref should fail");
        assert_eq!(
            error.to_string(),
            "Validation error: Required input port 'mysql_ref' is missing"
        );
    }

    #[test]
    fn rejects_non_positive_limit() {
        let mut node = MessageMySQLGetUserHistoryNode::new("history", "History");
        let mut inputs = base_inputs();
        inputs.insert("limit".to_string(), DataValue::Integer(0));

        let error = node.execute(inputs).expect_err("limit <= 0 should fail");
        assert_eq!(
            error.to_string(),
            "Validation error: limit must be greater than 0"
        );
    }

    #[test]
    fn rejects_mysql_ref_without_pool() {
        let mut node = MessageMySQLGetUserHistoryNode::new("history", "History");
        let error = node
            .execute(base_inputs())
            .expect_err("mysql_ref without pool should fail");

        assert_eq!(
            error.to_string(),
            "Validation error: mysql_ref has no active pool — ensure the MySqlNode is connected"
        );
    }

    #[test]
    fn empty_group_id_is_treated_as_absent() {
        let inputs =
            HashMap::from([("group_id".to_string(), DataValue::String("   ".to_string()))]);

        assert_eq!(extract_optional_group_id(&inputs).unwrap(), None);
    }

    #[test]
    fn user_history_query_uses_sender_only_when_group_missing() {
        let query = user_history_query(None);
        assert!(query.contains("WHERE sender_id = ?"));
        assert!(!query.contains("group_id = ?"));
    }

    #[test]
    fn user_history_query_uses_sender_and_group_when_present() {
        let query = user_history_query(Some("group-1"));
        assert!(query.contains("WHERE sender_id = ? AND group_id = ?"));
    }
}
