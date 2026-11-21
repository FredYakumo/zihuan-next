use std::sync::Arc;

use log::warn;
use serde_json::Value;

use zihuan_core::agent::tools::Tool;
use zihuan_core::data_refs::RelationalDbConnection;
use zihuan_core::error::{Error, Result};
use zihuan_core::ims_bot_adapter::adapter::SharedBotAdapter;
use zihuan_core::ims_bot_adapter::runtime::tools::qq_profile::fetch_group_member_role;
use zihuan_core::model_inference::llm::tooling::FunctionTool;
use zihuan_core::permission::{check_permission, EntityType};
use zihuan_core::storage::message::history::{search_message_records, MessageSearchFilters};
use zihuan_core::storage::user_permission::{
    access_level, DEFAULT_ACCESS_LEVEL, PLANER_ACCESS_LEVEL, ROLE_SELF_ACCESS_LEVEL,
};
use zihuan_core::utils::json_utils::optional_string_argument;

use super::common::{sanitize_positive_limit, StaticFunctionToolSpec, ToolNotificationTarget};

const DEFAULT_SEARCH_LIMIT: i64 = 50;
const MAX_SEARCH_LIMIT: i64 = 200;
const SEARCH_PERMISSION_CATEGORY: &str = "search.search_message";

/// Runtime identity and permission context the tool needs to enforce search rules.
///
/// The QQ turn owns these values; the tool only reads them. Keeping them together avoids a
/// long constructor argument list as the permission rules grow.
pub(crate) struct SearchQqMessagesAccessContext {
    pub(crate) agent_id: String,
    pub(crate) sender_id: String,
    pub(crate) sender_name: String,
    /// The role the adapter reported for the current sender in the current group, if any.
    pub(crate) sender_role: Option<String>,
    pub(crate) bot_id: String,
    pub(crate) permission_group: String,
    pub(crate) entity_type: EntityType,
    /// Planer turns have no inbound member and use the Planer access level.
    pub(crate) planer: bool,
    pub(crate) adapter: Option<SharedBotAdapter>,
}

/// Filtered search over the persisted QQ message history.
///
/// The tool is built per turn with the session it belongs to, so an unrestricted caller that
/// omits `group_id` searches the current group instead of the whole table. Every other filter
/// narrows the same query: sender, content keyword, and a time range.
///
/// Low-privilege senders (access level above 2) may only search their own messages, the bot's
/// messages in the current conversation, the current group's messages, or another member's
/// messages in the current group when the sender owns or administers that group. The
/// `search.search_message` permission and access levels 0-2 bypass these rules.
pub(crate) struct SearchQqMessagesTool {
    rdb_pool: Option<RelationalDbConnection>,
    notification_target: ToolNotificationTarget,
    access: SearchQqMessagesAccessContext,
}

impl SearchQqMessagesTool {
    pub(crate) fn new(
        rdb_pool: Option<RelationalDbConnection>,
        notification_target: ToolNotificationTarget,
        access: SearchQqMessagesAccessContext,
    ) -> Self {
        Self { rdb_pool, notification_target, access }
    }

    /// Group to search when the caller does not name one.
    ///
    /// Only a group session supplies a default: a private chat has no group history to fall back
    /// on, so an omitted `group_id` leaves the search without a group scope.
    fn default_group_id(&self) -> Option<String> {
        let target = &self.notification_target;
        (target.is_group() && !target.target_id().is_empty())
            .then(|| target.target_id().to_string())
    }

    fn effective_access_level(&self) -> Result<u32> {
        if self.access.planer {
            return Ok(PLANER_ACCESS_LEVEL);
        }
        if self.access.sender_id.trim().is_empty() {
            return Ok(ROLE_SELF_ACCESS_LEVEL);
        }
        let Some(rdb_pool) = self.rdb_pool.as_ref() else {
            return Ok(DEFAULT_ACCESS_LEVEL);
        };
        zihuan_core::runtime::block_async(access_level(
            rdb_pool,
            &self.access.agent_id,
            &self.access.sender_id,
        ))
    }

    /// Whether the sender holds the explicit `search.search_message` grant.
    ///
    /// A permission lookup failure is treated as no grant so the read rules stay fail-closed.
    fn has_search_message_permission(&self) -> bool {
        let sender_id = self.access.sender_id.trim();
        if sender_id.is_empty() {
            return false;
        }
        match check_permission(
            &self.access.permission_group,
            sender_id,
            self.access.entity_type,
            SEARCH_PERMISSION_CATEGORY,
        ) {
            Ok(allowed) => allowed,
            Err(error) => {
                warn!("[search_qq_messages] failed to check {SEARCH_PERMISSION_CATEGORY}: {error}");
                false
            }
        }
    }

    fn sender_is_group_owner_or_admin(&self, group_id: &str) -> bool {
        if let Some(role) = self.access.sender_role.as_deref() {
            let role = role.trim().to_ascii_lowercase();
            match role.as_str() {
                "owner" | "admin" => return true,
                "member" => return false,
                _ => {}
            }
        }
        let Some(adapter) = self.access.adapter.as_ref() else {
            return false;
        };
        match fetch_group_member_role(adapter, group_id, &self.access.sender_id) {
            Ok(role) => matches!(role.trim().to_ascii_lowercase().as_str(), "owner" | "admin"),
            Err(error) => {
                warn!(
                    "[search_qq_messages] failed to resolve group role for {} in {}: {error}",
                    self.access.sender_id, group_id
                );
                false
            }
        }
    }

    /// Whether the sender falls under the restricted search rules.
    ///
    /// Access levels 0-2 and the `search.search_message` grant keep the tool unrestricted.
    fn is_restricted(&self) -> Result<bool> {
        if self.effective_access_level()? <= 2 {
            return Ok(false);
        }
        Ok(!self.has_search_message_permission())
    }

    /// Returns the denial message when a restricted sender may not run this query.
    ///
    /// Restricted queries must name the current group explicitly: an omitted `group_id` is an
    /// unbounded group search and is denied rather than defaulted to the current group.
    fn permission_denial(
        &self,
        requested_group: Option<&str>,
        sender_id: Option<&str>,
        contain: Option<&str>,
        start_time: Option<&str>,
        end_time: Option<&str>,
    ) -> Option<String> {
        let is_group = self.notification_target.is_group();
        let current_group = self.default_group_id();
        let current_group_id = current_group.as_deref();
        let query = readable_query(sender_id, requested_group, contain, start_time, end_time);

        let Some(sender_id) = sender_id else {
            if !is_group {
                return Some(self.denial(&query, "仅能查询自己的消息，或机器人发给你的消息"));
            }
            if current_group_id.is_none() || requested_group != current_group_id {
                return Some(self.denial(&query, "仅能查询在当前群聊的消息"));
            }
            return None;
        };

        if sender_id == self.access.sender_id {
            return None;
        }

        if sender_id == self.access.bot_id {
            if !is_group {
                if requested_group.is_some() {
                    return Some(self.denial(&query, "仅能查询机器人发给你的私聊消息"));
                }
                return None;
            }
            if current_group_id.is_none() || requested_group != current_group_id {
                return Some(self.denial(&query, "仅能查询在当前群聊的消息"));
            }
            return None;
        }

        if !is_group {
            return Some(self.denial(&query, "仅能查询自己的消息，或机器人发给你的消息"));
        }
        let Some(group_id) = current_group_id else {
            return Some(self.denial(&query, "仅能查询在当前群聊的消息"));
        };
        if requested_group != Some(group_id) {
            return Some(self.denial(&query, "仅能查询在当前群聊的消息"));
        }
        if !self.sender_is_group_owner_or_admin(group_id) {
            let suggestion = format!(
                "让\"{}\"成为群主或者管理员，或改为查询\"{}\"群里的所有消息",
                self.access.sender_name, group_id
            );
            return Some(self.denial(&query, &suggestion));
        }
        None
    }

    fn denial(&self, query: &str, suggestion: &str) -> String {
        format!(
            "\"{}\"没有权限查询\"{}\"，建议改为查询: {}",
            self.access.sender_name, query, suggestion
        )
    }
}

fn readable_query(
    sender_id: Option<&str>,
    group_id: Option<&str>,
    contain: Option<&str>,
    start_time: Option<&str>,
    end_time: Option<&str>,
) -> String {
    let mut text = match (sender_id, group_id) {
        (Some(sender), Some(group)) => format!("{sender}在{group}群的消息"),
        (Some(sender), None) => format!("{sender}的消息"),
        (None, Some(group)) => format!("{group}群的消息"),
        (None, None) => "所有消息".to_string(),
    };
    if let Some(query) = contain {
        text.push_str(&format!("（内容包含\"{query}\"）"));
    }
    match (start_time, end_time) {
        (Some(start), Some(end)) => text.push_str(&format!("（时间 {start} 至 {end}）")),
        (Some(start), None) => text.push_str(&format!("（{start} 之后）")),
        (None, Some(end)) => text.push_str(&format!("（{end} 之前）")),
        (None, None) => {}
    }
    text
}

impl Tool for SearchQqMessagesTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        let group_description: &'static str = if self.default_group_id().is_some() {
            "要查询的群号，留空则查询当前群"
        } else {
            "要查询的群号，留空则不限群"
        };
        Arc::new(StaticFunctionToolSpec {
            name: "search_qq_messages",
            description:
                "在 QQ 聊天记录中按条件检索消息，支持群号、发送者、内容关键词与时间范围过滤，返回按时间排序的消息列表。返回体包含 total（过滤条件命中的消息总数）、has_more（是否还有更早的消息）与分页游标 oldest_send_time/oldest_id；当 has_more 为 true 时，把这两个游标作为 before_time/before_id 传入即可继续获取更早的消息。",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "可选：消息内容包含的关键词" },
                    "group_id": { "type": "string", "description": group_description },
                    "sender_id": { "type": "string", "description": "可选：按发送者 QQ 号过滤" },
                    "start_time": { "type": "string", "description": "可选：起始时间，格式 YYYY-MM-DD HH:MM:SS" },
                    "end_time": { "type": "string", "description": "可选：结束时间，格式 YYYY-MM-DD HH:MM:SS" },
                    "before_time": { "type": "string", "description": "可选：分页游标，上一批结果返回的 oldest_send_time，与 before_id 成对使用" },
                    "before_id": { "type": "integer", "description": "可选：分页游标，上一批结果返回的 oldest_id，与 before_time 成对使用" },
                    "limit": { "type": "integer", "description": "返回消息数量，默认 50，最大 200" }
                },
                "additionalProperties": false
            }),
        })
    }

    fn execute(&self, _call_content: &str, arguments: &Value) -> Result<String> {
        let rdb_pool = self.rdb_pool.as_ref().ok_or_else(|| {
            Error::ValidationError("rdb_pool is required for message lookup".to_string())
        })?;
        let RelationalDbConnection::MySql(mysql) = rdb_pool else {
            return Err(Error::ValidationError("rdb_pool must be a MySQL connection".to_string()));
        };
        let requested_group = optional_string_argument(arguments, "group_id")
            .filter(|value| !value.trim().is_empty());
        let sender_id = optional_string_argument(arguments, "sender_id")
            .filter(|value| !value.trim().is_empty());
        let contain =
            optional_string_argument(arguments, "query").filter(|value| !value.trim().is_empty());
        let start_time = optional_string_argument(arguments, "start_time")
            .filter(|value| !value.trim().is_empty());
        let end_time = optional_string_argument(arguments, "end_time")
            .filter(|value| !value.trim().is_empty());
        let restricted = self.is_restricted()?;
        if restricted {
            if let Some(denial) = self.permission_denial(
                requested_group.as_deref(),
                sender_id.as_deref(),
                contain.as_deref(),
                start_time.as_deref(),
                end_time.as_deref(),
            ) {
                return Ok(denial);
            }
        }
        let group_id = requested_group.or_else(|| self.default_group_id());
        let before_time = optional_string_argument(arguments, "before_time")
            .filter(|value| !value.trim().is_empty());
        let before_id = arguments.get("before_id").and_then(|value| match value {
            Value::Number(number) => number.as_i64(),
            Value::String(text) => text.trim().parse::<i64>().ok(),
            _ => None,
        });
        let filters = MessageSearchFilters {
            sender_id,
            group_id,
            contain,
            start_time,
            end_time,
            before_time,
            before_id,
            sort_by_time_desc: true,
            limit: sanitize_positive_limit(
                arguments.get("limit").and_then(Value::as_i64),
                DEFAULT_SEARCH_LIMIT,
                MAX_SEARCH_LIMIT,
            ) as u32,
        };
        let page = search_message_records(mysql, filters)?;
        Ok(serde_json::json!({
            "ok": true,
            "count": page.messages.len(),
            "total": page.total,
            "has_more": page.has_more,
            "oldest_send_time": page.oldest_send_time,
            "oldest_id": page.oldest_id,
            "messages": page.messages,
        })
        .to_string())
    }
}
