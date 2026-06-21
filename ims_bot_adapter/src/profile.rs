use serde::{Deserialize, Serialize};
use storage_handler::{find_connection, ConnectionConfig, ConnectionKind};
use zihuan_core::error::Result;

use crate::login_info::{fetch_login_info_via_adapter_connection, qq_avatar_url, BotLoginInfo};
use crate::parse_ims_bot_adapter_connection;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QqBotProfile {
    pub bot_user_id: Option<String>,
    pub bot_nickname: Option<String>,
    pub avatar_url: Option<String>,
}

pub fn resolve_fallback_bot_profile(
    connections: &[ConnectionConfig],
    connection_id: &str,
) -> Result<Option<QqBotProfile>> {
    let connection = find_connection(connections, connection_id)?;
    Ok(resolve_fallback_bot_profile_from_connection(connection))
}

pub fn resolve_fallback_bot_profile_from_connection(connection: &ConnectionConfig) -> Option<QqBotProfile> {
    let ConnectionKind::BotAdapter(raw) = &connection.kind else {
        return None;
    };
    let bot_connection = parse_ims_bot_adapter_connection(raw).ok()?;
    let bot_user_id = normalized_string(bot_connection.qq_id.as_deref());
    let avatar_url = bot_user_id.as_deref().and_then(qq_avatar_url);
    Some(QqBotProfile {
        bot_user_id,
        bot_nickname: None,
        avatar_url,
    })
}

pub async fn resolve_active_or_fallback_bot_profile(
    connections: &[ConnectionConfig],
    connection_id: &str,
) -> Result<Option<QqBotProfile>> {
    let connection = find_connection(connections, connection_id)?;
    Ok(resolve_active_or_fallback_bot_profile_from_connection(connection).await)
}

pub async fn resolve_active_or_fallback_bot_profile_from_connection(
    connection: &ConnectionConfig,
) -> Option<QqBotProfile> {
    let fallback = resolve_fallback_bot_profile_from_connection(connection)?;
    match fetch_login_info_via_adapter_connection(&connection.id).await {
        Ok(info) => Some(profile_from_login_info(info)),
        Err(_) => Some(fallback),
    }
}

pub fn profile_from_login_info(info: BotLoginInfo) -> QqBotProfile {
    let bot_user_id = normalized_string(Some(info.user_id.as_str()));
    let avatar_url = bot_user_id.as_deref().and_then(qq_avatar_url);
    QqBotProfile {
        bot_user_id,
        bot_nickname: normalized_string(Some(info.nickname.as_str())),
        avatar_url,
    }
}

fn normalized_string(value: Option<&str>) -> Option<String> {
    value.map(str::trim).filter(|value| !value.is_empty()).map(ToOwned::to_owned)
}
