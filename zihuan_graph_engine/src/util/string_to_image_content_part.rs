use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::MessagePart;

/// Wraps a String URL (or `data:` URL) into a multimodal `MessagePart`.
pub struct StringToImageMessagePartNode {
    id: String,
    name: String,
}

impl StringToImageMessagePartNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for StringToImageMessagePartNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("将字符串 URL（或 data:image/...;base64,...）封装为多模态 MessagePart")
    }

    node_input![
        port! { name = "url", ty = String, desc = "图片或视频的 URL，支持 http(s) 远程地址或 data: URL" },
        port! { name = "media_type", ty = String, desc = "媒体类型，可选 image 或 video，默认 image", optional },
    ];

    node_output![
        port! { name = "content_part", ty = MessagePart, desc = "封装后的多模态 MessagePart" },
    ];

    fn execute(&mut self, inputs: crate::NodeInputFlow) -> Result<crate::NodeOutputFlow> {
        self.validate_inputs(&inputs)?;

        let url = match inputs.get("url") {
            Some(DataValue::String(s)) => s.clone(),
            _ => return Err(Error::ValidationError("url is required".to_string())),
        };

        let media_type = match inputs.get("media_type") {
            Some(DataValue::String(s)) => s.trim().to_ascii_lowercase(),
            Some(_) => {
                return Err(Error::ValidationError(
                    "media_type must be a string".to_string(),
                ))
            }
            None => "image".to_string(),
        };

        let part = match media_type.as_str() {
            "" | "image" => MessagePart::image_url_string(url),
            "video" => MessagePart::video_url_string(url),
            other => {
                return Err(Error::ValidationError(format!(
                    "media_type must be 'image' or 'video', got '{other}'"
                )))
            }
        };

        let mut outputs = HashMap::new();
        outputs.insert("content_part".to_string(), DataValue::MessagePart(part));

        let outputs = crate::NodeOutputFlow::from(outputs);
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
