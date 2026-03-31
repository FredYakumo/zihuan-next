use crate::error::{Error, Result};
use crate::node::message_mysql_history_common::{
    format_history_messages, group_history_query, message_history_record_from_row, run_mysql_query,
};
use crate::node::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;

pub struct MessageMySQLGetGroupHistoryNode {
    id: String,
    name: String,
}

impl MessageMySQLGetGroupHistoryNode {
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

impl Node for MessageMySQLGetGroupHistoryNode {
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
        port! { name = "mysql_ref", ty = MySqlRef, desc = "MySQL连接配置引用" },
        port! { name = "group_id", ty = String, desc = "要查询的群 ID" },
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
                    .bind(i64::from(limit))
                    .fetch_all(pool)
                    .await
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
    use super::MessageMySQLGetGroupHistoryNode;
    use crate::node::data_value::MySqlConfig;
    use crate::node::{DataValue, Node};
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
                "group_id".to_string(),
                DataValue::String("group-1".to_string()),
            ),
            ("limit".to_string(), DataValue::Integer(5)),
        ])
    }

    #[test]
    fn rejects_non_positive_limit() {
        let mut node = MessageMySQLGetGroupHistoryNode::new("history", "History");
        let mut inputs = base_inputs();
        inputs.insert("limit".to_string(), DataValue::Integer(-1));

        let error = node.execute(inputs).expect_err("limit <= 0 should fail");
        assert_eq!(
            error.to_string(),
            "Validation error: limit must be greater than 0"
        );
    }

    #[test]
    fn rejects_mysql_ref_without_pool() {
        let mut node = MessageMySQLGetGroupHistoryNode::new("history", "History");
        let error = node
            .execute(base_inputs())
            .expect_err("mysql_ref without pool should fail");

        assert_eq!(
            error.to_string(),
            "Validation error: mysql_ref has no active pool — ensure the MySqlNode is connected"
        );
    }
}
