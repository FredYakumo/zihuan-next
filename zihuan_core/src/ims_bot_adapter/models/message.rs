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

fn deserialize_option_i32_from_string_or_number<'de, D>(
    deserializer: D,
) -> Result<Option<i32>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<serde_json::Value>::deserialize(deserializer)?;
    match opt {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::Number(n)) => Ok(n.as_i64().map(|value| value as i32)),
        Some(serde_json::Value::String(s)) => s
            .parse::<i32>()
            .map(Some)
            .map_err(|e| de::Error::custom(format!("failed to parse i32 from string: {e}"))),
        Some(other) => Err(de::Error::custom(format!(
            "expected null/string/number for Option<i32>, got {other}"
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
    #[serde(rename = "image")]
    Image(ImageMessage),
    #[serde(rename = "forward")]
    Forward(ForwardMessage),
}

impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Message::PlainText(msg) => write!(f, "{}", msg),
            Message::At(msg) => write!(f, "{}", msg),
            Message::Reply(msg) => write!(f, "{}", msg),
            Message::Image(msg) => write!(f, "{}", msg),
            Message::Forward(msg) => write!(f, "{}", msg),
        }
    }
}

impl MessageBase for Message {
    fn get_type(&self) -> &'static str {
        match self {
            Message::PlainText(_) => "text",
            Message::At(_) => "at",
            Message::Reply(_) => "reply",
            Message::Image(_) => "image",
            Message::Forward(_) => "forward",
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
    pub message_source: Option<Vec<Message>>,
}

impl fmt::Display for ReplyMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[Reply of message ID {}]", self.id)
    }
}

impl MessageBase for ReplyMessage {
    fn get_type(&self) -> &'static str {
        "reply"
    }
}

/// Image message segment.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ImageMessage {
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub thumb: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_option_i32_from_string_or_number"
    )]
    pub sub_type: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_status: Option<String>,
}

impl ImageMessage {
    pub fn source_locator(&self) -> Option<&str> {
        self.object_key
            .as_deref()
            .or(self.local_path.as_deref())
            .or(self.path.as_deref())
            .or(self.object_url.as_deref())
            .or(self.url.as_deref())
            .or(self.file.as_deref())
    }
}

impl fmt::Display for ImageMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[Image")?;
        if let Some(ref name) = self.name {
            write!(f, ": name={name}")?;
        }
        if let Some(ref object_key) = self.object_key {
            write!(f, ", object_key={object_key}")?;
        } else if let Some(locator) = self.source_locator() {
            write!(f, ", source={locator}")?;
        }
        if let Some(ref status) = self.cache_status {
            write!(f, ", cache_status={status}")?;
        }
        write!(f, "]")
    }
}

impl MessageBase for ImageMessage {
    fn get_type(&self) -> &'static str {
        "image"
    }
}

/// Forward / merged-forward message.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ForwardMessage {
    #[serde(
        default,
        deserialize_with = "deserialize_option_string_from_string_or_number"
    )]
    pub id: Option<String>,
    #[serde(default)]
    pub content: Vec<ForwardNodeMessage>,
}

impl fmt::Display for ForwardMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref id) = self.id {
            write!(f, "[Forward of message ID {}]", id)
        } else {
            write!(f, "[Forward with {} node(s)]", self.content.len())
        }
    }
}

impl MessageBase for ForwardMessage {
    fn get_type(&self) -> &'static str {
        "forward"
    }
}

/// One node in a merged-forward payload.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ForwardNodeMessage {
    #[serde(
        default,
        alias = "uin",
        deserialize_with = "deserialize_option_string_from_string_or_number"
    )]
    pub user_id: Option<String>,
    #[serde(default, alias = "name")]
    pub nickname: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_option_string_from_string_or_number"
    )]
    pub id: Option<String>,
    #[serde(default)]
    pub content: Vec<Message>,
}

fn append_rendered_segment(buffer: &mut String, segment: &str) {
    let segment = segment.trim();
    if segment.is_empty() {
        return;
    }

    if !buffer.is_empty() {
        let last_char = buffer.chars().last();
        if !matches!(last_char, Some('\n') | Some(' ')) {
            buffer.push(' ');
        }
    }

    buffer.push_str(segment);
}

fn render_forward_node_readable(node: &ForwardNodeMessage) -> String {
    let sender = node
        .nickname
        .as_deref()
        .or(node.user_id.as_deref())
        .unwrap_or("unknown");
    let rendered = render_messages_readable(&node.content);
    if rendered.trim().is_empty() {
        format!("{sender}: (empty)")
    } else {
        format!("{sender}: {rendered}")
    }
}

pub fn render_messages_readable(messages: &[Message]) -> String {
    let mut rendered = String::new();

    for message in messages {
        match message {
            Message::PlainText(plain) => append_rendered_segment(&mut rendered, &plain.text),
            Message::At(at) => append_rendered_segment(&mut rendered, &at.to_string()),
            Message::Reply(reply) => append_rendered_segment(&mut rendered, &reply.to_string()),
            Message::Image(image) => append_rendered_segment(&mut rendered, &image.to_string()),
            Message::Forward(forward) => {
                if forward.content.is_empty() {
                    append_rendered_segment(&mut rendered, &forward.to_string());
                    continue;
                }

                let body = forward
                    .content
                    .iter()
                    .map(render_forward_node_readable)
                    .collect::<Vec<_>>()
                    .join("\n");
                append_rendered_segment(&mut rendered, &format!("[Forward]\n{body}\n[/Forward]"));
            }
        }
    }

    rendered
}

#[derive(Clone, Debug)]
pub struct MessageProp {
    pub content: Option<String>,
    pub ref_content: Option<String>,
    pub is_at_me: bool,
    pub at_target_list: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MessageMediaRecord {
    pub segment_index: usize,
    pub r#type: String,
    pub name: Option<String>,
    pub file: Option<String>,
    pub path: Option<String>,
    pub url: Option<String>,
    pub thumb: Option<String>,
    pub summary: Option<String>,
    pub sub_type: Option<i32>,
    pub object_key: Option<String>,
    pub object_url: Option<String>,
    pub cache_status: Option<String>,
}

pub fn collect_media_records(messages: &[Message]) -> Vec<MessageMediaRecord> {
    messages
        .iter()
        .enumerate()
        .filter_map(|(segment_index, message)| match message {
            Message::Image(image) => Some(MessageMediaRecord {
                segment_index,
                r#type: "image".to_string(),
                name: image.name.clone(),
                file: image.file.clone(),
                path: image.local_path.clone().or_else(|| image.path.clone()),
                url: image.url.clone(),
                thumb: image.thumb.clone(),
                summary: image.summary.clone(),
                sub_type: image.sub_type,
                object_key: image.object_key.clone(),
                object_url: image.object_url.clone(),
                cache_status: image.cache_status.clone(),
            }),
            _ => None,
        })
        .collect()
}

impl MessageProp {
    fn text_mentions_bot_name(messages: &[Message], bot_name: Option<&str>) -> bool {
        let bot_name = match bot_name.map(str::trim) {
            Some(name) if !name.is_empty() => name,
            _ => return false,
        };

        let mention_patterns = [format!("@{bot_name}"), format!("＠{bot_name}")];

        messages.iter().any(|message| {
            matches!(message, Message::PlainText(plain) if mention_patterns.iter().any(|pattern| plain.text.contains(pattern)))
        })
    }

    pub fn from_messages(messages: &[Message], bot_id: Option<&str>) -> Self {
        Self::from_messages_with_bot_name(messages, bot_id, None)
    }

    pub fn from_messages_with_bot_name(
        messages: &[Message],
        bot_id: Option<&str>,
        bot_name: Option<&str>,
    ) -> Self {
        use std::collections::HashSet;

        let mut ref_parts: Vec<String> = Vec::new();
        let mut at_targets: Vec<String> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        for m in messages {
            if let Message::At(at) = m {
                if let Some(id) = &at.target {
                    if seen.insert(id.clone()) {
                        at_targets.push(id.clone());
                    }
                }
            }

            if let Message::Reply(reply) = m {
                if let Some(ref source_messages) = reply.message_source {
                    let rendered = render_messages_readable(source_messages);
                    if !rendered.trim().is_empty() {
                        ref_parts.push(rendered);
                    }
                }
            }
        }

        let content = {
            let s = render_messages_readable(messages);
            if s.trim().is_empty() {
                None
            } else {
                Some(s)
            }
        };

        let ref_content = {
            let s = ref_parts.join("\n");
            if s.trim().is_empty() {
                None
            } else {
                Some(s)
            }
        };

        let is_at_me = match bot_id {
            Some(id) => at_targets.iter().any(|t| t == id),
            None => false,
        } || Self::text_mentions_bot_name(messages, bot_name);

        MessageProp {
            content,
            ref_content,
            is_at_me,
            at_target_list: at_targets,
        }
    }
}
