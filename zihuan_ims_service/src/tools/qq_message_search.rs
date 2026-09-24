use std::sync::Arc;

use serde_json::Value;

use zihuan_core::agent::tools::Tool;
use zihuan_core::data_refs::RelationalDbConnection;
use zihuan_core::error::{Error, Result};
use zihuan_core::graph::message_rdb_history_common::{
    search_message_records, MessageSearchFilters,
};
use zihuan_core::model_inference::llm::tooling::FunctionTool;

use super::common::{
    optional_string_argument, sanitize_positive_limit, StaticFunctionToolSpec,
    ToolNotificationTarget,
};

const DEFAULT_SEARCH_LIMIT: i64 = 50;
const MAX_SEARCH_LIMIT: i64 = 200;

/// Filtered search over the persisted QQ message history.
///
/// The tool is built per turn with the session it belongs to, so a caller that omits `group_id`
/// searches the current group instead of the whole table. Every other filter narrows the same
/// query: sender, content keyword, and a time range.
pub(crate) struct SearchQqMessagesTool {
    rdb_pool: Option<RelationalDbConnection>,
    notification_target: ToolNotificationTarget,
}

impl SearchQqMessagesTool {
    pub(crate) fn new(
        rdb_pool: Option<RelationalDbConnection>,
        notification_target: ToolNotificationTarget,
    ) -> Self {
        Self { rdb_pool, notification_target }
    }

    /// Group to search when the caller does not name one.
    ///
    /// Only a group session supplies a default: a private chat has no group history to fall back
    /// on, so omitting `group_id` there searches across groups the caller explicitly names.
    fn default_group_id(&self) -> Option<String> {
        let target = &self.notification_target;
        (target.is_group() && !target.target_id().is_empty())
            .then(|| target.target_id().to_string())
    }
}

impl Tool for SearchQqMessagesTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        let group_description = if self.default_group_id().is_some() {
            "要查询的群号，留空则查询当前群"
        } else {
            "要查询的群号，留空则不限群"
        };
        Arc::new(StaticFunctionToolSpec {
            name: "search_qq_messages",
            description:
                "在 QQ 聊天记录中按条件检索消息，支持群号、发送者、内容关键词与时间范围过滤，返回按时间排序的消息列表。",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "可选：消息内容包含的关键词" },
                    "group_id": { "type": "string", "description": group_description },
                    "sender_id": { "type": "string", "description": "可选：按发送者 QQ 号过滤" },
                    "start_time": { "type": "string", "description": "可选：起始时间，格式 YYYY-MM-DD HH:MM:SS" },
                    "end_time": { "type": "string", "description": "可选：结束时间，格式 YYYY-MM-DD HH:MM:SS" },
                    "limit": { "type": "integer", "description": "返回消息数量，默认 50，最大 200" }
                },
                "additionalProperties": false
            }),
        })
    }

    fn execute(&self, _call_content: &str, arguments: &Value) -> String {
        let result = (|| -> Result<Value> {
            let rdb_pool = self.rdb_pool.as_ref().ok_or_else(|| {
                Error::ValidationError("rdb_pool is required for message lookup".to_string())
            })?;
            let RelationalDbConnection::MySql(mysql) = rdb_pool else {
                return Err(Error::ValidationError(
                    "rdb_pool must be a MySQL connection".to_string(),
                ));
            };
            let group_id = optional_string_argument(arguments, "group_id")
                .filter(|value| !value.trim().is_empty())
                .or_else(|| self.default_group_id());
            let filters = MessageSearchFilters {
                sender_id: optional_string_argument(arguments, "sender_id")
                    .filter(|value| !value.trim().is_empty()),
                group_id,
                contain: optional_string_argument(arguments, "query")
                    .filter(|value| !value.trim().is_empty()),
                start_time: optional_string_argument(arguments, "start_time")
                    .filter(|value| !value.trim().is_empty()),
                end_time: optional_string_argument(arguments, "end_time")
                    .filter(|value| !value.trim().is_empty()),
                sort_by_time_desc: true,
                limit: sanitize_positive_limit(
                    arguments.get("limit").and_then(Value::as_i64),
                    DEFAULT_SEARCH_LIMIT,
                    MAX_SEARCH_LIMIT,
                ) as u32,
            };
            let messages = search_message_records(mysql, filters)?;
            Ok(serde_json::json!({
                "ok": true,
                "count": messages.len(),
                "messages": messages,
            }))
        })();

        match result {
            Ok(value) => value.to_string(),
            Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}).to_string(),
        }
    }
}
