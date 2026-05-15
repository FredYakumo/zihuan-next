use serde::{Deserialize, Serialize};
use std::fmt;

use log::warn;
use serde::de::Deserializer;

use crate::ims_bot_adapter::models::message::Message;

/// Message type enum (private or group chat)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageType {
    Private,
    Group,
}

impl fmt::Display for MessageType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MessageType::Private => write!(f, "private"),
            MessageType::Group => write!(f, "group"),
        }
    }
}

impl MessageType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MessageType::Private => "private",
            MessageType::Group => "group",
        }
    }
}

/// Sender information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sender {
    pub user_id: i64,
    pub nickname: String,
    #[serde(default)]
    pub card: String,
    pub role: Option<String>,
}

/// Message event containing the full message information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageEvent {
    pub message_id: i64,
    pub message_type: MessageType,
    pub sender: Sender,
    pub message_list: Vec<Message>,
    pub group_id: Option<i64>,
    pub group_name: Option<String>,
    #[serde(default)]
    pub is_group_message: bool,
}

/// Raw message event structure for deserialization and serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawMessageEvent {
    pub message_id: i64,
    pub message_type: MessageType,
    pub sender: Sender,
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_message_vec_lenient")]
    pub message: Vec<Message>,
    #[serde(default)]
    pub group_id: Option<i64>,
    #[serde(default)]
    pub group_name: Option<String>,
}

fn deserialize_message_vec_lenient<'de, D>(deserializer: D) -> Result<Vec<Message>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw_values = Vec::<serde_json::Value>::deserialize(deserializer)?;

    let mut out = Vec::with_capacity(raw_values.len());
    for v in raw_values {
        match serde_json::from_value::<Message>(v) {
            Ok(m) => out.push(m),
            Err(e) => {
                warn!("Skipping unsupported message element: {}", e);
            }
        }
    }
    Ok(out)
}
