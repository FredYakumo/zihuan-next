use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use zihuan_agent::brain::BrainTool;
use zihuan_core::data_refs::MySqlConfig;
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::tooling::FunctionTool;
use zihuan_graph_engine::data_value::DataValue;
use zihuan_graph_engine::message_mysql_get_group_history::MessageMySQLGetGroupHistoryNode;
use zihuan_graph_engine::message_mysql_get_user_history::MessageMySQLGetUserHistoryNode;
use zihuan_graph_engine::Node;

use super::common::{
    extract_string_list_output, optional_string_argument, sanitize_positive_limit,
    StaticFunctionToolSpec, ToolNotificationTarget,
};

const DEFAULT_HISTORY_TOOL_LIMIT: i64 = 10;
const MAX_HISTORY_TOOL_LIMIT: i64 = 50;

pub(crate) struct GetRecentGroupMessagesBrainTool {
    mysql_ref: Option<Arc<MySqlConfig>>,
    notification_target: ToolNotificationTarget,
}

impl GetRecentGroupMessagesBrainTool {
    pub(crate) fn new(
        mysql_ref: Option<Arc<MySqlConfig>>,
        notification_target: ToolNotificationTarget,
    ) -> Self {
        Self {
            mysql_ref,
            notification_target,
        }
    }
}

impl BrainTool for GetRecentGroupMessagesBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        let dashboard_mode = self.notification_target.target_id().is_empty();
        let mut properties = serde_json::json!({
            "limit": { "type": "integer", "description": "要查看的消息数量，默认 10，最大 50" }
        });
        if dashboard_mode {
            properties.as_object_mut().unwrap().insert(
                "group_id".to_string(),
                serde_json::json!({ "type": "string", "description": "要查询的群号" }),
            );
        }
        let mut schema = serde_json::json!({
            "type": "object",
            "properties": properties
        });
        if dashboard_mode {
            schema
                .as_object_mut()
                .unwrap()
                .insert("required".to_string(), serde_json::json!(["group_id"]));
        } else {
            schema
                .as_object_mut()
                .unwrap()
                .insert("additionalProperties".to_string(), serde_json::json!(false));
        }
        Arc::new(StaticFunctionToolSpec {
            name: "get_recent_group_messages",
            description:
                "只查看指定群或当前群里最新的少量消息，适合“刚刚/最近几条”；不适合按时间段检索、总结或详细分析历史聊天",
            parameters: schema,
        })
    }

    fn execute(&self, call_content: &str, arguments: &Value) -> String {
        self.notification_target.notify_progress(call_content);

        let result = (|| -> Result<Value> {
            let group_id = if self.notification_target.target_id().is_empty() {
                optional_string_argument(arguments, "group_id")
                    .ok_or_else(|| Error::ValidationError("group_id is required".to_string()))?
            } else {
                if !self.notification_target.is_group() {
                    return Err(Error::ValidationError(
                        "get_recent_group_messages can only be used in group chat".to_string(),
                    ));
                }
                self.notification_target.target_id().to_string()
            };
            let mysql_ref = self.mysql_ref.as_ref().ok_or_else(|| {
                Error::ValidationError("mysql_ref is required for message lookup".to_string())
            })?;
            let limit = sanitize_positive_limit(
                arguments.get("limit").and_then(Value::as_i64),
                DEFAULT_HISTORY_TOOL_LIMIT,
                MAX_HISTORY_TOOL_LIMIT,
            );
            let mut node = MessageMySQLGetGroupHistoryNode::new("__tool__", "__tool__");
            let outputs = node.execute(HashMap::from([
                (
                    "mysql_ref".to_string(),
                    DataValue::MySqlRef(mysql_ref.clone()),
                ),
                ("group_id".to_string(), DataValue::String(group_id)),
                ("limit".to_string(), DataValue::Integer(limit as i64)),
            ]).into())?;
            let items = extract_string_list_output(&outputs, "messages")?;
            Ok(serde_json::json!({
                "ok": true,
                "messages": items,
            }))
        })();

        match result {
            Ok(value) => value.to_string(),
            Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}).to_string(),
        }
    }
}

pub(crate) struct GetRecentUserMessagesBrainTool {
    mysql_ref: Option<Arc<MySqlConfig>>,
    notification_target: ToolNotificationTarget,
}

impl GetRecentUserMessagesBrainTool {
    pub(crate) fn new(
        mysql_ref: Option<Arc<MySqlConfig>>,
        notification_target: ToolNotificationTarget,
    ) -> Self {
        Self {
            mysql_ref,
            notification_target,
        }
    }
}

impl BrainTool for GetRecentUserMessagesBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: "get_recent_user_messages",
            description: "查看某人最近的 n 条消息，可选用 group_id 限定是否在某个群内",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "sender_id": { "type": "string", "description": "要查询的 QQ 号" },
                    "group_id": { "type": "string", "description": "可选：仅查看该群内的消息" },
                    "limit": { "type": "integer", "description": "要查看的消息数量，默认 10，最大 50" }
                },
                "required": ["sender_id"]
            }),
        })
    }

    fn execute(&self, call_content: &str, arguments: &Value) -> String {
        self.notification_target.notify_progress(call_content);

        let result = (|| -> Result<Value> {
            let mysql_ref = self.mysql_ref.as_ref().ok_or_else(|| {
                Error::ValidationError("mysql_ref is required for message lookup".to_string())
            })?;
            let sender_id = optional_string_argument(arguments, "sender_id")
                .ok_or_else(|| Error::ValidationError("sender_id is required".to_string()))?;
            let group_id = optional_string_argument(arguments, "group_id");
            let limit = sanitize_positive_limit(
                arguments.get("limit").and_then(Value::as_i64),
                DEFAULT_HISTORY_TOOL_LIMIT,
                MAX_HISTORY_TOOL_LIMIT,
            );
            let mut node = MessageMySQLGetUserHistoryNode::new("__tool__", "__tool__");
            let mut payload = HashMap::from([
                (
                    "mysql_ref".to_string(),
                    DataValue::MySqlRef(mysql_ref.clone()),
                ),
                ("sender_id".to_string(), DataValue::String(sender_id)),
                ("limit".to_string(), DataValue::Integer(limit as i64)),
            ]);
            if let Some(group_id) = group_id {
                payload.insert("group_id".to_string(), DataValue::String(group_id));
            }
            let outputs = node.execute(payload.into())?;
            let items = extract_string_list_output(&outputs, "messages")?;
            Ok(serde_json::json!({
                "ok": true,
                "messages": items,
            }))
        })();

        match result {
            Ok(value) => value.to_string(),
            Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}).to_string(),
        }
    }
}

