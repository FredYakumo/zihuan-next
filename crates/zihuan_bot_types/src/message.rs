use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};
use std::fmt;

fn deserialize_i64_from_string_or_number<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: Deserializer<'de>,
{
    let v = serde_json::Value::deserialize(deserializer)?;
    match v {
        serde_json::Value::Number(n) => n
            .as_i64()
            .ok_or_else(|| de::Error::custom("numeric value is not an i64")),
        serde_json::Value::String(s) => s
            .parse::<i64>()
            .map_err(|e| de::Error::custom(format!("failed to parse i64 from string: {e}"))),
        other => Err(de::Error::custom(format!(
            "expected string or number for i64, got {other}"
        ))),
    }
}

fn deserialize_option_i64_from_string_or_number<'de, D>(
    deserializer: D,
) -> Result<Option<i64>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<serde_json::Value>::deserialize(deserializer)?;
    match opt {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::Number(n)) => Ok(n.as_i64()),
        Some(serde_json::Value::String(s)) => {
            let parsed = s
                .parse::<i64>()
                .map_err(|e| de::Error::custom(format!("failed to parse i64 from string: {e}")))?;
            Ok(Some(parsed))
        }
        Some(other) => Err(de::Error::custom(format!(
            "expected null/string/number for Option<i64>, got {other}"
        ))),
    }
}

fn deserialize_option_string_from_string_or_number<'de, D>(
    deserializer: D,
) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<serde_json::Value>::deserialize(deserializer)?;
    match opt {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(s)) => Ok(Some(s)),
        Some(serde_json::Value::Number(n)) => Ok(Some(n.to_string())),
        Some(other) => Err(de::Error::custom(format!(
            "expected null/string/number for Option<String>, got {other}"
        ))),
    }
}

/// Base trait for all message types
pub trait MessageBase: fmt::Display + fmt::Debug + Send + Sync {
    fn get_type(&self) -> &'static str;
}

/// Enum representing all possible message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum Message {
    #[serde(rename = "text")]
    PlainText(PlainTextMessage),
    #[serde(rename = "at")]
    At(AtTargetMessage),
    #[serde(rename = "reply", alias = "replay")]
    Reply(ReplyMessage),
}

impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Message::PlainText(msg) => write!(f, "{}", msg),
            Message::At(msg) => write!(f, "{}", msg),
            Message::Reply(msg) => write!(f, "{}", msg),
        }
    }
}

impl MessageBase for Message {
    fn get_type(&self) -> &'static str {
        match self {
            Message::PlainText(_) => "text",
            Message::At(_) => "at",
            Message::Reply(_) => "reply",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlainTextMessage {
    pub text: String,
}

impl fmt::Display for PlainTextMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.text)
    }
}

impl MessageBase for PlainTextMessage {
    fn get_type(&self) -> &'static str {
        "text"
    }
}

/// @ mention message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtTargetMessage {
    #[serde(rename = "qq", alias = "target")]
    #[serde(
        default,
        deserialize_with = "deserialize_option_string_from_string_or_number"
    )]
    pub target: Option<String>,
}

impl AtTargetMessage {
    pub fn target_id(&self) -> String {
        self.target.clone().unwrap_or_else(|| "null".to_string())
    }
}

impl fmt::Display for AtTargetMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "@{}", self.target_id())
    }
}

impl MessageBase for AtTargetMessage {
    fn get_type(&self) -> &'static str {
        "at"
    }
}

/// Reply message (references another message)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplyMessage {
    #[serde(deserialize_with = "deserialize_i64_from_string_or_number")]
    pub id: i64,
    #[serde(skip)]
    pub message_source: Option<Box<Message>>,
}

impl fmt::Display for ReplyMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref source) = self.message_source {
            write!(f, "[Reply of message ID {}: {}]", self.id, source)
        } else {
            write!(f, "[Reply of message ID {}]", self.id)
        }
    }
}

impl MessageBase for ReplyMessage {
    fn get_type(&self) -> &'static str {
        "reply"
    }
}

/// Abstracts and encapsulates the raw messages received by the bot, refining them into structured fields convenient for LLM processing:
/// - `content`: The merged readable body (text/@/reply, etc.), used directly for feeding to the model
/// - `ref_content`: Contextual summary from reference/reply chains (e.g., replied content), used to supplement context
/// - `is_at_me`: Whether the message @'s the bot itself, facilitating priority/trigger judgment
/// - `at_target_list`: List of all @ targets in the message (QQ numbers, etc.), used for intent recognition and routing
#[derive(Clone, Debug)]
pub struct MessageProp {
    pub content: Option<String>,
    pub ref_content: Option<String>,
    pub is_at_me: bool,
    pub at_target_list: Vec<String>,
}

impl MessageProp {
    /// Build a MessageProp from a list of messages.
    pub fn from_messages(messages: &[Message], bot_id: Option<&str>) -> Self {
        use std::collections::HashSet;

        let mut content_parts: Vec<String> = Vec::with_capacity(messages.len());
        let mut ref_parts: Vec<String> = Vec::new();
        let mut at_targets: Vec<String> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        for m in messages {
            content_parts.push(m.to_string());

            if let Message::At(at) = m {
                if let Some(id) = &at.target {
                    if seen.insert(id.clone()) {
                        at_targets.push(id.clone());
                    }
                }
            }

            if let Message::Reply(reply) = m {
                if let Some(ref src) = reply.message_source {
                    ref_parts.push(src.to_string());
                }
            }
        }

        let content = {
            let s = content_parts.join(" ");
            if s.trim().is_empty() { None } else { Some(s) }
        };

        let ref_content = {
            let s = ref_parts.join("\n");
            if s.trim().is_empty() { None } else { Some(s) }
        };

        let is_at_me = match bot_id {
            Some(id) => at_targets.iter().any(|t| t.to_string() == *id),
            None => false,
        };

        MessageProp {
            content,
            ref_content,
            is_at_me,
            at_target_list: at_targets,
        }
    }
}
