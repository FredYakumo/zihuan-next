use std::sync::Arc;

use log::{info, warn};
use serde_json::Value;

use zihuan_agent::brain::BrainTool;
use zihuan_core::error::{Error, Result};
use zihuan_core::ims_bot_adapter::models::message::{PersistedMedia, PersistedMediaSource};
use zihuan_core::llm::tooling::{FunctionTool, StaticFunctionToolSpec};
use zihuan_graph_engine::message_restore::register_media;
use zihuan_graph_engine::object_storage::S3Ref;

use crate::adapter::SharedBotAdapter;
use crate::login_info::qq_avatar_url;
use crate::message_helpers::get_bot_id;
use crate::models::MessageEvent;
use crate::ws_action::ws_send_action;

const LOG_PREFIX: &str = "[qq_profile]";

const FIELD_QQ: &str = "qq号";
const FIELD_SEX: &str = "性别";
const FIELD_AGE: &str = "年龄";
const FIELD_AVATAR_MEDIA_ID: &str = "头像media_id";
const FIELD_IDENTITY: &str = "身份";

const VALID_QUERY_FIELDS: &[&str] = &[FIELD_QQ, FIELD_SEX, FIELD_AGE, FIELD_AVATAR_MEDIA_ID, FIELD_IDENTITY];

const AVATAR_S3_KEY_PREFIX: &str = "qq_avatar";
const AVATAR_CONTENT_TYPE: &str = "image/jpeg";

#[derive(Debug, Clone, Default)]
struct StrangerInfo {
    user_id: String,
    nickname: String,
    sex: String,
    age: i64,
}

#[derive(Debug, Clone, Default)]
struct MemberInfo {
    role: String,
}

pub struct GetBotProfileBrainTool {
    adapter: SharedBotAdapter,
    event: MessageEvent,
    s3_ref: Option<Arc<S3Ref>>,
}

impl GetBotProfileBrainTool {
    pub fn new(adapter: SharedBotAdapter, event: MessageEvent, s3_ref: Option<Arc<S3Ref>>) -> Self {
        Self { adapter, event, s3_ref }
    }
}

impl BrainTool for GetBotProfileBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: "get_bot_profile",
            description: "获取当前 QQ bot 自身的信息，包括 QQ号、性别、年龄、头像media_id、在当前聊天中的身份（群主/管理员/普通群员/私聊状态）。",
            parameters: query_schema(),
        })
    }

    fn execute(&self, _call_content: &str, arguments: &Value) -> String {
        let result = (|| -> Result<String> {
            let query = parse_query_fields(arguments)?;
            let bot_id = bot_qq_id(&self.adapter)?;
            execute_profile_query(&self.adapter, &self.event, &self.s3_ref, &bot_id, &query)
        })();

        match result {
            Ok(json) => json,
            Err(error) => error_tool_result(&error),
        }
    }
}

pub struct GetQqUserProfileBrainTool {
    adapter: SharedBotAdapter,
    event: MessageEvent,
    s3_ref: Option<Arc<S3Ref>>,
}

impl GetQqUserProfileBrainTool {
    pub fn new(adapter: SharedBotAdapter, event: MessageEvent, s3_ref: Option<Arc<S3Ref>>) -> Self {
        Self { adapter, event, s3_ref }
    }
}

impl BrainTool for GetQqUserProfileBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: "get_qq_user_profile",
            description: "获取指定 QQ 用户的信息，包括 QQ号、性别、年龄、头像media_id、在当前聊天中的身份（群主/管理员/普通群员/私聊状态）。需要提供目标用户的 QQ 号。",
            parameters: user_profile_schema(),
        })
    }

    fn execute(&self, _call_content: &str, arguments: &Value) -> String {
        let result = (|| -> Result<String> {
            let user_id = required_string_argument(arguments, "id", "id 是必填参数")?;
            let query = parse_query_fields(arguments)?;
            execute_profile_query(&self.adapter, &self.event, &self.s3_ref, &user_id, &query)
        })();

        match result {
            Ok(json) => json,
            Err(error) => error_tool_result(&error),
        }
    }
}

fn query_field_schema() -> Value {
    serde_json::json!({
        "type": "string",
        "enum": VALID_QUERY_FIELDS,
        "description": "要查询的字段名"
    })
}

fn query_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "query": {
                "type": "array",
                "items": query_field_schema(),
                "description": "要查询的字段列表，可选值：qq号、性别、年龄、头像media_id、身份。支持多选。",
                "minItems": 1
            }
        },
        "required": ["query"],
        "additionalProperties": false
    })
}

fn user_profile_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "id": {
                "type": "string",
                "description": "目标用户的 QQ 号"
            },
            "query": {
                "type": "array",
                "items": query_field_schema(),
                "description": "要查询的字段列表，可选值：qq号、性别、年龄、头像media_id、身份。支持多选。",
                "minItems": 1
            }
        },
        "required": ["id", "query"],
        "additionalProperties": false
    })
}

fn parse_query_fields(arguments: &Value) -> Result<Vec<String>> {
    let raw = arguments
        .get("query")
        .and_then(|v| v.as_array())
        .ok_or_else(|| Error::ValidationError("query 参数必须是字符串数组".to_string()))?;

    let mut seen = std::collections::HashSet::new();
    let mut fields = Vec::new();

    for item in raw {
        let field = item
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| Error::ValidationError("query 数组中的每一项必须是非空字符串".to_string()))?;

        let normalized = normalize_field_name(field);
        if VALID_QUERY_FIELDS.contains(&normalized.as_str()) {
            if seen.insert(normalized.clone()) {
                fields.push(normalized);
            }
        } else {
            return Err(Error::ValidationError(format!(
                "无效的查询字段 '{field}'，可选值：{}",
                VALID_QUERY_FIELDS.join("、")
            )));
        }
    }

    if fields.is_empty() {
        return Err(Error::ValidationError("query 数组不能为空".to_string()));
    }

    Ok(fields)
}

fn normalize_field_name(raw: &str) -> String {
    match raw {
        "qq号" | "QQ号" | "qq" | "QQ" => FIELD_QQ.to_string(),
        "性别" | "sex" | "gender" => FIELD_SEX.to_string(),
        "年龄" | "age" => FIELD_AGE.to_string(),
        "头像media_id" | "头像" | "头像media" | "avatar" => FIELD_AVATAR_MEDIA_ID.to_string(),
        "身份" | "role" | "群身份" => FIELD_IDENTITY.to_string(),
        other => other.to_string(),
    }
}

fn required_string_argument(arguments: &Value, key: &str, error_msg: &str) -> Result<String> {
    arguments
        .get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| Error::ValidationError(error_msg.to_string()))
}

fn error_tool_result(error: &Error) -> String {
    serde_json::json!({
        "ok": false,
        "error": error.to_string(),
    })
    .to_string()
}

fn bot_qq_id(adapter: &SharedBotAdapter) -> Result<String> {
    Ok(get_bot_id(adapter))
}

/// Orchestrates a complete profile query: fetches stranger info from NapCat,
/// resolves group identity for group messages, and optionally downloads/caches
/// the avatar to S3.
fn execute_profile_query(
    adapter: &SharedBotAdapter,
    event: &MessageEvent,
    s3_ref: &Option<Arc<S3Ref>>,
    user_id: &str,
    query: &[String],
) -> Result<String> {
    let stranger = fetch_stranger_info(adapter, user_id, s3_ref)?;

    let identity = if is_group_event(event) {
        match event.group_id {
            Some(group_id) => match fetch_group_member_info(adapter, group_id, user_id) {
                Ok(member) => member.role,
                Err(_) => "unknown".to_string(),
            },
            None => "unknown".to_string(),
        }
    } else {
        "私聊".to_string()
    };

    let avatar_media_id = if query.contains(&FIELD_AVATAR_MEDIA_ID.to_string()) {
        resolve_avatar_media_id(user_id, s3_ref)
    } else {
        None
    };

    let result = build_profile_result(query, &stranger, &identity, avatar_media_id.as_deref());
    Ok(serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string()))
}

/// Calls NapCat `get_stranger_info` action. Falls back to `get_login_info`
/// (if S3 is available) when the stranger response is empty — this handles
/// the case where the bot queries its own profile.
fn fetch_stranger_info(adapter: &SharedBotAdapter, user_id: &str, s3_ref: &Option<Arc<S3Ref>>) -> Result<StrangerInfo> {
    let response = ws_send_action(adapter, "get_stranger_info", serde_json::json!({ "user_id": user_id }))?;

    let data = response
        .get("data")
        .ok_or_else(|| Error::ValidationError("get_stranger_info 响应缺少 data 字段".to_string()))?;

    let result = StrangerInfo {
        user_id: user_id.to_string(),
        nickname: string_field_or(data, "nickname", ""),
        sex: string_field_or(data, "sex", "unknown"),
        age: data.get("age").and_then(|v| v.as_i64()).unwrap_or(0),
    };

    if result.nickname.is_empty() && result.sex == "unknown" && result.age == 0 {
        let login = fallback_get_login_info(adapter, s3_ref)?;
        if let Some(login) = login {
            return Ok(StrangerInfo {
                user_id: login.user_id,
                nickname: login.nickname,
                sex: "unknown".to_string(),
                age: 0,
            });
        }
    }

    Ok(result)
}

fn fallback_get_login_info(
    adapter: &SharedBotAdapter,
    s3_ref: &Option<Arc<S3Ref>>,
) -> Result<Option<crate::login_info::BotLoginInfo>> {
    let s3_available = s3_ref.is_some();
    if !s3_available {
        return Ok(None);
    }
    match ws_send_action(adapter, "get_login_info", serde_json::json!({})) {
        Ok(response) => {
            let data = response
                .get("data")
                .ok_or_else(|| Error::ValidationError("get_login_info 响应缺少 data 字段".to_string()))?;
            let user_id = data
                .get("user_id")
                .and_then(|v| v.as_i64().map(|id| id.to_string()))
                .or_else(|| data.get("user_id").and_then(|v| v.as_str().map(ToOwned::to_owned)))
                .unwrap_or_default();
            let nickname = string_field_or(data, "nickname", "");
            Ok(Some(crate::login_info::BotLoginInfo { user_id, nickname }))
        }
        Err(e) => {
            warn!("{LOG_PREFIX} get_login_info fallback also failed: {e}");
            Ok(None)
        }
    }
}

fn fetch_group_member_info(adapter: &SharedBotAdapter, group_id: i64, user_id: &str) -> Result<MemberInfo> {
    let response = ws_send_action(
        adapter,
        "get_group_member_info",
        serde_json::json!({
            "group_id": group_id,
            "user_id": user_id,
        }),
    )?;

    let data = response
        .get("data")
        .ok_or_else(|| Error::ValidationError("get_group_member_info 响应缺少 data 字段".to_string()))?;

    Ok(MemberInfo {
        role: string_field_or(data, "role", "member"),
    })
}

/// Resolves a QQ avatar to a persisted media ID, with S3-backed caching.
///
/// On cache miss, downloads the avatar from QQ's CDN via `qq_avatar_url`,
/// uploads to S3, and returns the media ID. The S3 key is deterministic:
/// `qq_avatar/{user_id}.jpg`.
fn resolve_avatar_media_id(user_id: &str, s3_ref: &Option<Arc<S3Ref>>) -> Option<String> {
    let s3 = s3_ref.as_ref()?;
    let key = avatar_s3_key(user_id);
    let media = avatar_media_from_s3_key(&key);

    let cached = match zihuan_core::runtime::block_async(async { s3.get_object_bytes(&key).await }) {
        Ok(bytes) if !bytes.is_empty() => {
            info!("{LOG_PREFIX} avatar cache hit for user_id={user_id} key={key}");
            Some(bytes)
        }
        Ok(_) => None,
        Err(e) => {
            info!("{LOG_PREFIX} avatar cache miss for user_id={user_id}: {e}");
            None
        }
    };

    if cached.is_some() {
        register_media(media.clone());
        return Some(media.media_id);
    }

    let avatar_url = qq_avatar_url(user_id)?;
    info!("{LOG_PREFIX} downloading avatar for user_id={user_id} from {avatar_url}");

    let bytes = match reqwest::blocking::get(&avatar_url) {
        Ok(response) => match response.bytes() {
            Ok(bytes) if !bytes.is_empty() => bytes.to_vec(),
            Ok(_) => {
                warn!("{LOG_PREFIX} avatar download returned empty body for user_id={user_id}");
                return None;
            }
            Err(e) => {
                warn!("{LOG_PREFIX} failed to read avatar body for user_id={user_id}: {e}");
                return None;
            }
        },
        Err(e) => {
            warn!("{LOG_PREFIX} failed to download avatar for user_id={user_id}: {e}");
            return None;
        }
    };

    match zihuan_core::runtime::block_async(async { s3.put_object(&key, AVATAR_CONTENT_TYPE, &bytes).await }) {
        Ok(_) => {
            info!("{LOG_PREFIX} avatar uploaded to S3 for user_id={user_id} key={key}");
            register_media(media.clone());
            Some(media.media_id)
        }
        Err(e) => {
            warn!("{LOG_PREFIX} failed to upload avatar to S3 for user_id={user_id}: {e}");
            None
        }
    }
}

fn avatar_s3_key(user_id: &str) -> String {
    format!("{AVATAR_S3_KEY_PREFIX}/{user_id}.jpg")
}

fn avatar_media_from_s3_key(key: &str) -> PersistedMedia {
    PersistedMedia::new(
        PersistedMediaSource::QqChat,
        String::new(),
        key,
        None,
        None,
        Some(AVATAR_CONTENT_TYPE.to_string()),
    )
}

fn build_profile_result(
    query: &[String],
    stranger: &StrangerInfo,
    identity: &str,
    avatar_media_id: Option<&str>,
) -> Value {
    let mut result = serde_json::Map::new();
    result.insert("ok".to_string(), Value::Bool(true));

    for field in query {
        let (key, value) = match field.as_str() {
            FIELD_QQ => ("qq号", Value::String(stranger.user_id.clone())),
            FIELD_SEX => ("性别", Value::String(stranger.sex.clone())),
            FIELD_AGE => ("年龄", Value::Number(stranger.age.into())),
            FIELD_AVATAR_MEDIA_ID => (
                "头像media_id",
                match avatar_media_id {
                    Some(id) => Value::String(id.to_string()),
                    None => Value::Null,
                },
            ),
            FIELD_IDENTITY => ("身份", Value::String(identity.to_string())),
            _ => continue,
        };
        result.insert(key.to_string(), value);
    }

    Value::Object(result)
}

fn is_group_event(event: &MessageEvent) -> bool {
    matches!(event.message_type, crate::models::event_model::MessageType::Group)
}

fn string_field_or(value: &Value, key: &str, default: &str) -> String {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| default.to_string())
}
