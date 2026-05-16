use log::error;
use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::{Hash, Hasher};

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
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PersistedMediaSource {
    #[default]
    Upload,
    QqChat,
    WebSearch,
}

impl fmt::Display for PersistedMediaSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            PersistedMediaSource::Upload => "upload",
            PersistedMediaSource::QqChat => "qq_chat",
            PersistedMediaSource::WebSearch => "web_search",
        };
        write!(f, "{value}")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PersistedMedia {
    pub media_id: String,
    pub source: PersistedMediaSource,
    pub original_source: String,
    pub rustfs_path: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub mime_type: Option<String>,
}

impl PersistedMedia {
    pub fn new(
        source: PersistedMediaSource,
        original_source: impl Into<String>,
        rustfs_path: impl Into<String>,
        name: Option<String>,
        description: Option<String>,
        mime_type: Option<String>,
    ) -> Self {
        let original_source = original_source.into();
        let rustfs_path = rustfs_path.into();
        let media_id = build_persisted_media_id(&source, &original_source, &rustfs_path);

        Self {
            media_id,
            source,
            original_source,
            rustfs_path,
            name,
            description,
            mime_type,
        }
    }

    pub fn with_rustfs_path(&self, rustfs_path: impl Into<String>) -> Self {
        let rustfs_path = rustfs_path.into();
        Self {
            media_id: build_persisted_media_id(&self.source, &self.original_source, &rustfs_path),
            source: self.source.clone(),
            original_source: self.original_source.clone(),
            rustfs_path,
            name: self.name.clone(),
            description: self.description.clone(),
            mime_type: self.mime_type.clone(),
        }
    }

    pub fn primary_locator(&self) -> Option<&str> {
        if !self.rustfs_path.trim().is_empty() {
            Some(self.rustfs_path.as_str())
        } else if !self.original_source.trim().is_empty() {
            Some(self.original_source.as_str())
        } else {
            None
        }
    }
}

fn build_persisted_media_id(
    source: &PersistedMediaSource,
    original_source: &str,
    rustfs_path: &str,
) -> String {
    let seed = format!("{source}|{original_source}|{rustfs_path}");
    let mut hasher = DefaultHasher::new();
    seed.hash(&mut hasher);
    format!("media-{:016x}", hasher.finish())
}

/// Image message segment.
#[derive(Debug, Clone, Serialize, Default)]
pub struct ImageMessage {
    pub media: PersistedMedia,
}

#[derive(Debug, Deserialize)]
struct NapCatImagePayload {
    #[serde(default)]
    file: Option<String>,
    #[serde(default)]
    url: Option<String>,
}

impl ImageMessage {
    pub fn new(media: PersistedMedia) -> Self {
        Self { media }
    }

    pub fn source_locator(&self) -> Option<&str> {
        self.media.primary_locator()
    }

    pub fn rustfs_path(&self) -> Option<&str> {
        (!self.media.rustfs_path.trim().is_empty()).then_some(self.media.rustfs_path.as_str())
    }

    pub fn original_source(&self) -> Option<&str> {
        (!self.media.original_source.trim().is_empty())
            .then_some(self.media.original_source.as_str())
    }

    pub fn name(&self) -> Option<&str> {
        self.media.name.as_deref()
    }

    pub fn description(&self) -> Option<&str> {
        self.media.description.as_deref()
    }

    pub fn mime_type(&self) -> Option<&str> {
        self.media.mime_type.as_deref()
    }
}

impl fmt::Display for ImageMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[Image")?;
        if let Some(ref name) = self.media.name {
            write!(f, ": name={name}")?;
        }
        write!(f, ", source={}", self.media.source)?;
        if !self.media.rustfs_path.trim().is_empty() {
            write!(f, ", rustfs_path={}", self.media.rustfs_path)?;
        } else if let Some(locator) = self.source_locator() {
            write!(f, ", source={locator}")?;
        }
        write!(f, "]")
    }
}

impl MessageBase for ImageMessage {
    fn get_type(&self) -> &'static str {
        "image"
    }
}

impl<'de> Deserialize<'de> for ImageMessage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        if let Some(media_value) = value.get("media") {
            let media = PersistedMedia::deserialize(media_value.clone()).map_err(|error| {
                error!(
                    "[ImageMessage] failed to deserialize PersistedMedia from image JSON: {} payload={}",
                    error, value
                );
                de::Error::custom(format!("invalid media field: {error}"))
            })?;

            return Ok(ImageMessage::new(media));
        }

        let payload = NapCatImagePayload::deserialize(value.clone()).map_err(|error| {
            error!(
                "[ImageMessage] failed to deserialize NapCat image payload: {} payload={}",
                error, value
            );
            de::Error::custom(format!("invalid napcat image payload: {error}"))
        })?;

        let original_source = payload
            .url
            .clone()
            .or_else(|| payload.file.clone())
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                error!(
                    "[ImageMessage] image payload missing both media and napcat locators: payload={}",
                    value
                );
                de::Error::custom("image payload missing both media and napcat locators")
            })?;

        Ok(ImageMessage::new(PersistedMedia::new(
            PersistedMediaSource::QqChat,
            original_source,
            String::new(),
            payload.file.filter(|value| !value.trim().is_empty()),
            None,
            None,
        )))
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
    pub media_id: String,
    pub source: PersistedMediaSource,
    pub original_source: String,
    pub rustfs_path: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub mime_type: Option<String>,
}

pub fn collect_media_records(messages: &[Message]) -> Vec<MessageMediaRecord> {
    messages
        .iter()
        .enumerate()
        .filter_map(|(segment_index, message)| match message {
            Message::Image(image) => Some(MessageMediaRecord {
                segment_index,
                r#type: "image".to_string(),
                media_id: image.media.media_id.clone(),
                source: image.media.source.clone(),
                original_source: image.media.original_source.clone(),
                rustfs_path: image.media.rustfs_path.clone(),
                name: image.media.name.clone(),
                description: image.media.description.clone(),
                mime_type: image.media.mime_type.clone(),
            }),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persisted_media_source_serde_and_display() {
        let source = PersistedMediaSource::QqChat;
        assert_eq!(source.to_string(), "qq_chat");
        assert_eq!(
            serde_json::to_string(&source).expect("serialize source"),
            "\"qq_chat\""
        );
        let parsed: PersistedMediaSource =
            serde_json::from_str("\"web_search\"").expect("deserialize source");
        assert!(matches!(parsed, PersistedMediaSource::WebSearch));
    }

    #[test]
    fn image_message_roundtrip_uses_new_media_structure() {
        let message = ImageMessage::new(PersistedMedia::new(
            PersistedMediaSource::QqChat,
            "https://multimedia.nt.qq.com.cn/download?id=1",
            "qq-images/2026/05/16/sample.jpg",
            Some("download".to_string()),
            Some("图片描述".to_string()),
            Some("image/jpeg".to_string()),
        ));

        let json = serde_json::to_string(&message).expect("serialize image");
        assert!(json.contains("\"media\""));
        assert!(!json.contains("\"object_key\""));

        let restored: ImageMessage = serde_json::from_str(&json).expect("deserialize image");
        assert_eq!(restored.media.source.to_string(), "qq_chat");
        assert_eq!(restored.media.mime_type.as_deref(), Some("image/jpeg"));
        assert_eq!(
            restored.media.rustfs_path,
            "qq-images/2026/05/16/sample.jpg"
        );
    }

    #[test]
    fn image_message_deserializes_napcat_payload() {
        let json = r#"{
            "file":"580FDE1876D6C29E5F2AF42CC83D6E62.png",
            "file_size":"3168359",
            "sub_type":0,
            "summary":"",
            "url":"https://multimedia.nt.qq.com.cn/download?appid=1406&fileid=test"
        }"#;

        let restored: ImageMessage = serde_json::from_str(json).expect("deserialize napcat image");
        assert_eq!(restored.media.source, PersistedMediaSource::QqChat);
        assert_eq!(
            restored.media.original_source,
            "https://multimedia.nt.qq.com.cn/download?appid=1406&fileid=test"
        );
        assert_eq!(
            restored.media.name.as_deref(),
            Some("580FDE1876D6C29E5F2AF42CC83D6E62.png")
        );
        assert_eq!(restored.media.rustfs_path, "");
        assert_eq!(restored.media.mime_type, None);
    }

    #[test]
    fn message_display_contains_media_information() {
        let message = Message::Image(ImageMessage::new(PersistedMedia::new(
            PersistedMediaSource::Upload,
            "upload://manual/demo",
            "uploads/demo.png",
            Some("demo.png".to_string()),
            None,
            Some("image/png".to_string()),
        )));

        let rendered = render_messages_readable(&[message]);
        assert!(rendered.contains("demo.png"));
        assert!(rendered.contains("uploads/demo.png"));
    }
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
