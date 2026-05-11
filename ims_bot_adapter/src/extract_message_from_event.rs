use crate::models::message::MessageProp;
use base64::Engine;
use log::warn;
use std::collections::HashMap;
use std::path::Path;
use tokio::task::block_in_place;
use zihuan_core::error::Result;
use zihuan_core::llm::{ContentPart, OpenAIMessage};
use zihuan_graph_engine::object_storage::S3Ref;
use zihuan_graph_engine::{node_input, node_output, DataType, DataValue, Node, Port};

use crate::models::message::{ImageMessage, Message};

/// Node that converts a MessageEvent to an LLM prompt message list
///
/// Inputs:
///   - message_event: MessageEvent containing message data
///   - ims_bot_adapter: BotAdapterRef for building context-aware system message
///
/// Outputs:
///   - messages: Vec<OpenAIMessage>: One user message
pub struct ExtractMessageFromEventNode {
    id: String,
    name: String,
}

impl ExtractMessageFromEventNode {
    const LOG_PREFIX: &str = "[ExtractMessageFromEventNode]";

    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }

    fn append_text_segment(buffer: &mut String, segment: &str) {
        let segment = segment.trim();
        if segment.is_empty() {
            return;
        }

        if !buffer.is_empty() {
            buffer.push(' ');
        }
        buffer.push_str(segment);
    }

    fn flush_text_part(parts: &mut Vec<ContentPart>, buffer: &mut String) {
        let text = buffer.trim();
        if !text.is_empty() {
            parts.push(ContentPart::text(text.to_string()));
        }
        buffer.clear();
    }

    fn infer_content_type(file_name: &str) -> &'static str {
        match Path::new(file_name)
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref()
        {
            Some("jpg") | Some("jpeg") => "image/jpeg",
            Some("gif") => "image/gif",
            Some("webp") => "image/webp",
            Some("bmp") => "image/bmp",
            Some("svg") => "image/svg+xml",
            _ => "image/png",
        }
    }

    fn image_name(image: &ImageMessage) -> &str {
        image
            .name
            .as_deref()
            .or_else(|| {
                image
                    .local_path
                    .as_deref()
                    .and_then(|path| Path::new(path).file_name())
                    .and_then(|name| name.to_str())
            })
            .or_else(|| {
                image
                    .path
                    .as_deref()
                    .and_then(|path| Path::new(path).file_name())
                    .and_then(|name| name.to_str())
            })
            .unwrap_or("image.png")
    }

    fn image_part_from_bytes(image: &ImageMessage, bytes: Vec<u8>) -> ContentPart {
        let base64_payload = base64::engine::general_purpose::STANDARD.encode(bytes);
        ContentPart::image_data_url(
            Self::infer_content_type(Self::image_name(image)),
            base64_payload,
        )
    }

    fn image_part_from_object_storage(
        s3_ref: &S3Ref,
        object_key: &str,
        image: &ImageMessage,
    ) -> Option<ContentPart> {
        let s3_ref = s3_ref.clone();
        let key = object_key.to_string();
        let result = if tokio::runtime::Handle::try_current().is_ok() {
            block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async move {
                    s3_ref.get_object_bytes(&key).await
                })
            })
        } else {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .ok()
                .and_then(|runtime| {
                    runtime
                        .block_on(async move { s3_ref.get_object_bytes(&key).await })
                        .ok()
                })
                .ok_or_else(|| {
                    zihuan_core::error::Error::StringError(
                        "failed to create runtime for object storage read".to_string(),
                    )
                })
        };

        match result {
            Ok(bytes) => Some(Self::image_part_from_bytes(image, bytes)),
            Err(error) => {
                warn!(
                    "{} failed to read object storage image for multimodal input object_key={}: {}",
                    Self::LOG_PREFIX,
                    object_key,
                    error
                );
                None
            }
        }
    }

    fn image_part_from_local_file(path: &str, image: &ImageMessage) -> Option<ContentPart> {
        let file_path = Path::new(path);
        if !file_path.exists() {
            return None;
        }

        match std::fs::read(file_path) {
            Ok(bytes) => Some(Self::image_part_from_bytes(image, bytes)),
            Err(error) => {
                warn!(
                    "{} failed to read image file for multimodal input path={}: {}",
                    Self::LOG_PREFIX,
                    path,
                    error
                );
                None
            }
        }
    }

    async fn download_remote_bytes(url: &str) -> Option<Vec<u8>> {
        let response = match reqwest::Client::new().get(url).send().await {
            Ok(response) => response,
            Err(error) => {
                warn!(
                    "{} failed to download remote image for multimodal input url={}: {}",
                    Self::LOG_PREFIX,
                    url,
                    error
                );
                return None;
            }
        };

        if !response.status().is_success() {
            warn!(
                "{} remote image returned non-success status for multimodal input url={} status={}",
                Self::LOG_PREFIX,
                url,
                response.status()
            );
            return None;
        }

        match response.bytes().await {
            Ok(bytes) => Some(bytes.to_vec()),
            Err(error) => {
                warn!(
                    "{} failed to read remote image body for multimodal input url={}: {}",
                    Self::LOG_PREFIX,
                    url,
                    error
                );
                None
            }
        }
    }

    fn image_part_from_remote_url(url: &str, image: &ImageMessage) -> Option<ContentPart> {
        let bytes = if tokio::runtime::Handle::try_current().is_ok() {
            block_in_place(|| {
                tokio::runtime::Handle::current().block_on(Self::download_remote_bytes(url))
            })
        } else {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .ok()
                .and_then(|runtime| runtime.block_on(Self::download_remote_bytes(url)))
        }?;

        Some(Self::image_part_from_bytes(image, bytes))
    }

    fn image_part(image: &ImageMessage, s3_ref: Option<&S3Ref>) -> Option<ContentPart> {
        for local_path in [
            image.local_path.as_deref(),
            image.path.as_deref(),
            image
                .file
                .as_deref()
                .and_then(|value| value.strip_prefix("file://")),
        ]
        .into_iter()
        .flatten()
        {
            if let Some(part) = Self::image_part_from_local_file(local_path, image) {
                return Some(part);
            }
        }

        if let (Some(s3_ref), Some(object_key)) = (s3_ref, image.object_key.as_deref()) {
            if let Some(part) = Self::image_part_from_object_storage(s3_ref, object_key, image) {
                return Some(part);
            }
        }

        for direct_url in [image.object_url.as_deref(), image.url.as_deref()]
            .into_iter()
            .flatten()
        {
            if direct_url.starts_with("data:") {
                return Some(ContentPart::image_url_string(direct_url.to_string()));
            }

            if let Some(part) = Self::image_part_from_remote_url(direct_url, image) {
                return Some(part);
            }
        }

        let file_value = image.file.as_deref()?;
        if file_value.starts_with("data:") {
            return Some(ContentPart::image_url_string(file_value.to_string()));
        }
        if file_value.starts_with("https://") {
            if let Some(part) = Self::image_part_from_remote_url(file_value, image) {
                return Some(part);
            }
        }

        warn!(
            "{} skipping multimodal image because no safe source could be resolved: {}",
            Self::LOG_PREFIX,
            image
        );
        None
    }

    /// Purpose: Recursively process a list of messages, <br />
    /// appending text segments to the text buffer and flushing to parts when media is encountered.
    fn append_messages_as_parts(
        messages: &[Message],
        parts: &mut Vec<ContentPart>,
        text_buffer: &mut String,
        has_media: &mut bool,
        include_reply_source_block: bool,
        s3_ref: Option<&S3Ref>,
    ) {
        for message in messages {
            match message {
                Message::PlainText(plain) => {
                    Self::append_text_segment(text_buffer, &plain.text);
                }
                Message::Image(image) => {
                    if let Some(part) = Self::image_part(image, s3_ref) {
                        Self::flush_text_part(parts, text_buffer);
                        parts.push(part);
                        *has_media = true;
                    } else {
                        Self::append_text_segment(text_buffer, &image.to_string());
                    }
                }
                Message::Reply(reply) => {
                    Self::append_text_segment(text_buffer, &reply.to_string());

                    if include_reply_source_block {
                        if let Some(source_messages) = reply.message_source.as_deref() {
                            if !text_buffer.is_empty() {
                                text_buffer.push_str("\n\n");
                            }
                            text_buffer.push_str("[引用内容]\n");
                            Self::append_messages_as_parts(
                                source_messages,
                                parts,
                                text_buffer,
                                has_media,
                                false,
                                s3_ref,
                            );
                        }
                    }
                }
                Message::Forward(forward) => {
                    if forward.content.is_empty() {
                        Self::append_text_segment(text_buffer, &forward.to_string());
                    } else {
                        if !text_buffer.is_empty() {
                            text_buffer.push_str("\n\n");
                        }
                        text_buffer.push_str("[转发内容]\n");
                        for (index, node) in forward.content.iter().enumerate() {
                            if index > 0 && !text_buffer.ends_with('\n') {
                                text_buffer.push('\n');
                            }
                            let sender = node
                                .nickname
                                .as_deref()
                                .or(node.user_id.as_deref())
                                .unwrap_or("unknown");
                            text_buffer.push_str(sender);
                            text_buffer.push_str(": ");
                            Self::append_messages_as_parts(
                                &node.content,
                                parts,
                                text_buffer,
                                has_media,
                                false,
                                s3_ref,
                            );
                            if !text_buffer.ends_with('\n') {
                                text_buffer.push('\n');
                            }
                        }
                    }
                }
                other => {
                    Self::append_text_segment(text_buffer, &other.to_string());
                }
            }
        }
    }

    fn build_user_message(
        messages: &[Message],
        msg_prop: &MessageProp,
        s3_ref: Option<&S3Ref>,
    ) -> OpenAIMessage {
        let mut parts = Vec::new();
        let mut text_buffer = String::new();
        let mut has_media = false;

        Self::append_messages_as_parts(
            messages,
            &mut parts,
            &mut text_buffer,
            &mut has_media,
            true,
            s3_ref,
        );

        if let Some(ref_cnt) = msg_prop
            .ref_content
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            if text_buffer.contains("[引用内容]") {
                if !text_buffer.is_empty() {
                    text_buffer.push_str("\n\n");
                }
                text_buffer.push_str("[引用内容补充摘要]\n");
                text_buffer.push_str(ref_cnt);
            } else {
                if !text_buffer.is_empty() {
                    text_buffer.push_str("\n\n");
                }
                text_buffer.push_str("[引用内容]\n");
                text_buffer.push_str(ref_cnt);
            }
        }

        Self::flush_text_part(&mut parts, &mut text_buffer);

        if has_media {
            if parts.is_empty() {
                OpenAIMessage::user("(无可用文本内容)")
            } else {
                OpenAIMessage::user_with_parts(parts)
            }
        } else {
            let user_text = msg_prop
                .content
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .map(str::to_string)
                .or_else(|| {
                    parts.into_iter().find_map(|part| match part {
                        ContentPart::Text { text } if !text.trim().is_empty() => Some(text),
                        _ => None,
                    })
                })
                .unwrap_or_else(|| "(无文本内容，可能是仅@或回复)".to_string());

            OpenAIMessage::user(user_text)
        }
    }
}

impl Node for ExtractMessageFromEventNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("Converts MessageEvent to LLM prompt string")
    }

    node_input![
        port! { name = "message_event", ty = MessageEvent, desc = "MessageEvent containing message data" },
        port! { name = "ims_bot_adapter", ty = BotAdapterRef, desc = "BotAdapter reference for context-aware system message", required = true }
    ];

    node_output![
        port! { name = "messages", ty = Vec(OpenAIMessage), desc = "Vec<OpenAIMessage> containing system and user messages" },
        port! { name = "content", ty = String, desc = "Merged readable message body" },
        port! { name = "ref_content", ty = String, desc = "Referenced/replied message content" },
        port! { name = "is_at_me", ty = Boolean, desc = "Whether the message @'s the bot" },
        port! { name = "at_target_list", ty = Vec(String), desc = "List of all @ targets in the message" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let mut outputs = HashMap::new();

        if let Some(DataValue::MessageEvent(event)) = inputs.get("message_event") {
            let ims_bot_adapter_ref = inputs
                .get("ims_bot_adapter")
                .and_then(|v| {
                    if let DataValue::BotAdapterRef(handle) = v {
                        Some(crate::adapter::shared_from_handle(handle))
                    } else {
                        None
                    }
                })
                .ok_or("ims_bot_adapter input is required")?;

            // This node still has a sync execute() API, so if we're already on a Tokio
            // worker thread we must move the blocking lock into block_in_place.
            let (bot_id, object_storage) = if tokio::runtime::Handle::try_current().is_ok() {
                block_in_place(|| {
                    let adapter = ims_bot_adapter_ref.blocking_lock();
                    (
                        adapter.get_bot_id().to_string(),
                        adapter.get_object_storage(),
                    )
                })
            } else {
                let adapter = ims_bot_adapter_ref.blocking_lock();
                (
                    adapter.get_bot_id().to_string(),
                    adapter.get_object_storage(),
                )
            };

            let msg_prop = MessageProp::from_messages(&event.message_list, Some(&bot_id));

            let user_msg =
                Self::build_user_message(&event.message_list, &msg_prop, object_storage.as_deref());

            let messages = vec![user_msg];
            outputs.insert(
                "messages".to_string(),
                DataValue::Vec(
                    Box::new(zihuan_graph_engine::DataType::OpenAIMessage),
                    messages.into_iter().map(DataValue::OpenAIMessage).collect(),
                ),
            );
            outputs.insert(
                "content".to_string(),
                DataValue::String(msg_prop.content.unwrap_or_default()),
            );
            outputs.insert(
                "ref_content".to_string(),
                DataValue::String(msg_prop.ref_content.unwrap_or_default()),
            );
            outputs.insert(
                "is_at_me".to_string(),
                DataValue::Boolean(msg_prop.is_at_me),
            );
            outputs.insert(
                "at_target_list".to_string(),
                DataValue::Vec(
                    Box::new(DataType::String),
                    msg_prop
                        .at_target_list
                        .into_iter()
                        .map(DataValue::String)
                        .collect(),
                ),
            );
        } else {
            return Err("message_event input is required and must be MessageEvent type".into());
        }
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
