use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use zihuan_agent::brain::BrainTool;
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::tooling::{FunctionTool, StaticFunctionToolSpec};
use zihuan_core::utils::bm25::rank_bm25_matches;

use crate::adapter::SharedBotAdapter;
use crate::models::MessageEvent;
use crate::ws_action::{json_i64, ws_send_action};

const NOT_IN_GROUP_MESSAGE: &str = "当前不在群聊中，无法获取成员列表";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum GroupMemberRole {
    Owner,
    Admin,
    Member,
}

impl GroupMemberRole {
    fn heading(self) -> &'static str {
        match self {
            Self::Owner => "群主",
            Self::Admin => "管理员列表",
            Self::Member => "普通成员列表",
        }
    }
}

#[derive(Debug, Clone)]
struct GroupMemberEntry {
    user_id: String,
    nickname: String,
    card: String,
    role: GroupMemberRole,
    original_index: usize,
}

impl GroupMemberEntry {
    fn display_name(&self) -> &str {
        if !self.card.trim().is_empty() {
            self.card.trim()
        } else if !self.nickname.trim().is_empty() {
            self.nickname.trim()
        } else {
            self.user_id.as_str()
        }
    }
}

pub struct GetCurrentGroupMembersBrainTool {
    adapter: SharedBotAdapter,
    event: MessageEvent,
}

impl GetCurrentGroupMembersBrainTool {
    pub fn new(adapter: SharedBotAdapter, event: MessageEvent) -> Self {
        Self { adapter, event }
    }
}

impl BrainTool for GetCurrentGroupMembersBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: "get_current_group_members",
            description:
                "获取当前群聊的成员列表，可按名称或 QQ 号做模糊过滤，并按群主、管理员、普通成员返回文本表格。仅在当前会话是群聊时使用。",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "可选：按成员名称做 BM25 模糊搜索" },
                    "id": { "type": "string", "description": "可选：按 QQ 号做 BM25 模糊搜索" },
                    "filter": {
                        "type": "string",
                        "description": "可选：仅返回群主、管理员、普通成员之一；也支持 owner/admin/member"
                    },
                    "limit": { "type": "integer", "description": "可选：限制返回的最大成员数量" }
                },
                "additionalProperties": false
            }),
        })
    }

    fn execute(&self, _call_content: &str, arguments: &Value) -> String {
        match execute_group_member_tool(&self.adapter, &self.event, arguments) {
            Ok(result) => result,
            Err(error) => error.to_string(),
        }
    }
}

fn execute_group_member_tool(adapter: &SharedBotAdapter, event: &MessageEvent, arguments: &Value) -> Result<String> {
    let Some(group_id) = event.group_id.filter(|_| is_group_event(event)) else {
        return Ok(NOT_IN_GROUP_MESSAGE.to_string());
    };

    let criteria = SearchCriteria::from_arguments(arguments)?;
    let members = fetch_group_member_list(adapter, group_id)?;
    let filtered = filter_members(members, &criteria);

    if filtered.is_empty() {
        return Ok("未找到匹配的群成员".to_string());
    }

    Ok(render_group_member_sections(&filtered, criteria.filter))
}

#[derive(Debug, Clone, Default)]
struct SearchCriteria {
    name_keyword: Option<String>,
    id_keyword: Option<String>,
    filter: Option<GroupMemberRole>,
    limit: Option<usize>,
}

impl SearchCriteria {
    fn from_arguments(arguments: &Value) -> Result<Self> {
        let limit = match arguments.get("limit").and_then(Value::as_i64) {
            Some(value) if value > 0 => Some(value as usize),
            Some(_) => {
                return Err(Error::ValidationError("limit 必须是大于 0 的整数".to_string()));
            }
            None => None,
        };

        Ok(Self {
            name_keyword: optional_trimmed_string(arguments, "name"),
            id_keyword: optional_trimmed_string(arguments, "id"),
            filter: optional_trimmed_string(arguments, "filter")
                .map(|value| parse_role_filter(&value))
                .transpose()?,
            limit,
        })
    }
}

fn optional_trimmed_string(arguments: &Value, key: &str) -> Option<String> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn parse_role_filter(raw: &str) -> Result<GroupMemberRole> {
    match raw.trim().to_lowercase().as_str() {
        "群主" | "owner" => Ok(GroupMemberRole::Owner),
        "管理员" | "admin" => Ok(GroupMemberRole::Admin),
        "普通成员" | "成员" | "member" => Ok(GroupMemberRole::Member),
        _ => Err(Error::ValidationError(
            "filter 仅支持 群主 / 管理员 / 普通成员 或 owner / admin / member".to_string(),
        )),
    }
}

fn fetch_group_member_list(adapter: &SharedBotAdapter, group_id: i64) -> Result<Vec<GroupMemberEntry>> {
    let response = ws_send_action(
        adapter,
        "get_group_member_list",
        serde_json::json!({
            "group_id": group_id,
        }),
    )?;

    let data = response
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::ValidationError("get_group_member_list 响应缺少 data 数组".to_string()))?;

    let mut members = Vec::with_capacity(data.len());
    for (index, item) in data.iter().enumerate() {
        let user_id = json_i64(item.get("user_id"))
            .map(|value| value.to_string())
            .or_else(|| item.get("user_id").and_then(Value::as_str).map(ToOwned::to_owned))
            .unwrap_or_default();
        if user_id.is_empty() {
            continue;
        }

        members.push(GroupMemberEntry {
            user_id,
            nickname: string_field_or(item, "nickname"),
            card: string_field_or(item, "card"),
            role: parse_member_role(item.get("role").and_then(Value::as_str)),
            original_index: index,
        });
    }
    Ok(members)
}

fn string_field_or(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_default()
}

fn parse_member_role(raw: Option<&str>) -> GroupMemberRole {
    match raw.unwrap_or("member").trim().to_lowercase().as_str() {
        "owner" => GroupMemberRole::Owner,
        "admin" => GroupMemberRole::Admin,
        _ => GroupMemberRole::Member,
    }
}

fn filter_members(members: Vec<GroupMemberEntry>, criteria: &SearchCriteria) -> Vec<GroupMemberEntry> {
    let mut indexed_scores: HashMap<usize, f64> = members.iter().map(|item| (item.original_index, 0.0)).collect();
    let mut active_indices: Vec<usize> = members.iter().map(|item| item.original_index).collect();

    if let Some(filter) = criteria.filter {
        active_indices.retain(|index| members[*index].role == filter);
    }

    if let Some(keyword) = criteria.name_keyword.as_deref() {
        let documents = members
            .iter()
            .map(|item| format!("{} {}", item.display_name(), item.nickname))
            .collect::<Vec<_>>();
        let matches = rank_bm25_matches(keyword, &documents);
        active_indices = intersect_ranked_indices(active_indices, matches, &mut indexed_scores);
    }

    if let Some(keyword) = criteria.id_keyword.as_deref() {
        let documents = members.iter().map(|item| item.user_id.clone()).collect::<Vec<_>>();
        let matches = rank_bm25_matches(keyword, &documents);
        active_indices = intersect_ranked_indices(active_indices, matches, &mut indexed_scores);
    }

    let mut filtered: Vec<GroupMemberEntry> = active_indices.into_iter().map(|index| members[index].clone()).collect();
    filtered.sort_by(|left, right| {
        let right_score = indexed_scores.get(&right.original_index).copied().unwrap_or_default();
        let left_score = indexed_scores.get(&left.original_index).copied().unwrap_or_default();
        role_sort_key(left.role)
            .cmp(&role_sort_key(right.role))
            .then_with(|| right_score.total_cmp(&left_score))
            .then_with(|| left.original_index.cmp(&right.original_index))
    });

    if let Some(limit) = criteria.limit {
        filtered.truncate(limit);
    }

    filtered
}

fn intersect_ranked_indices(
    current_indices: Vec<usize>,
    matches: Vec<zihuan_core::utils::bm25::Bm25Match>,
    indexed_scores: &mut HashMap<usize, f64>,
) -> Vec<usize> {
    let score_map: HashMap<usize, f64> = matches.into_iter().map(|item| (item.index, item.score)).collect();
    current_indices
        .into_iter()
        .filter(|index| score_map.contains_key(index))
        .inspect(|index| {
            if let Some(score) = score_map.get(index) {
                let entry = indexed_scores.entry(*index).or_default();
                *entry += *score;
            }
        })
        .collect()
}

fn role_sort_key(role: GroupMemberRole) -> usize {
    match role {
        GroupMemberRole::Owner => 0,
        GroupMemberRole::Admin => 1,
        GroupMemberRole::Member => 2,
    }
}

fn render_group_member_sections(members: &[GroupMemberEntry], filter: Option<GroupMemberRole>) -> String {
    let roles = match filter {
        Some(role) => vec![role],
        None => vec![GroupMemberRole::Owner, GroupMemberRole::Admin, GroupMemberRole::Member],
    };

    let mut lines = Vec::new();
    for role in roles {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push(format!("{}:", role.heading()));
        lines.push("|名称|QQ号|".to_string());
        lines.push("|---|---|".to_string());

        let mut added = false;
        for member in members.iter().filter(|item| item.role == role) {
            lines.push(format!("|{}|{}|", escape_table_cell(member.display_name()), member.user_id));
            added = true;
        }

        if !added {
            lines.push("|无|无|".to_string());
        }
    }

    lines.join("\n")
}

fn escape_table_cell(value: &str) -> String {
    value.replace('|', "\\|")
}

fn is_group_event(event: &MessageEvent) -> bool {
    matches!(event.message_type, crate::models::event_model::MessageType::Group)
}
