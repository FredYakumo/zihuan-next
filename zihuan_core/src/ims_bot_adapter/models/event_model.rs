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

/// The sender's gender, as the adapter reports it on the wire (`male` / `female` / `unknown`).
///
/// Deserialization never fails: a missing, non-string, or unrecognized value becomes
/// [`SenderGender::Unknown`], because a sender parse error would drop the whole message event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SenderGender {
    Male,
    Female,
    #[default]
    Unknown,
}

impl SenderGender {
    pub fn from_wire(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "male" | "m" | "男" => Self::Male,
            "female" | "f" | "女" => Self::Female,
            _ => Self::Unknown,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            SenderGender::Male => "male",
            SenderGender::Female => "female",
            SenderGender::Unknown => "unknown",
        }
    }

    /// `None` when the gender is unknown, so prompts can omit the clause instead of
    /// telling the model "gender: unknown".
    pub fn known(self) -> Option<Self> {
        match self {
            SenderGender::Unknown => None,
            known => Some(known),
        }
    }
}

impl fmt::Display for SenderGender {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for SenderGender {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = Option::<serde_json::Value>::deserialize(deserializer)?;
        Ok(raw
            .as_ref()
            .and_then(serde_json::Value::as_str)
            .map(Self::from_wire)
            .unwrap_or_default())
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
    /// The wire field is `sex`; adapters that omit it, and outbound (bot) senders, leave the
    /// default [`SenderGender::Unknown`].
    #[serde(default, rename = "sex")]
    pub gender: SenderGender,
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
