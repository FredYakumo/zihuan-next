use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use std::collections::HashMap;

use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::{ContentPart, MediaUrlSpec};

/// Encodes raw `Vec<u8>` bytes as a base64 data URL and wraps them as a multimodal `ContentPart`.
pub struct BinaryToImageContentPartNode {
    id: String,
    name: String,
}

impl BinaryToImageContentPartNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for BinaryToImageContentPartNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("将二进制字节 + MIME 编码为 base64 data URL，并封装为多模态 ContentPart")
    }

    node_input![
        port! { name = "bytes", ty = Binary, desc = "图片或视频字节流" },
        port! { name = "mime", ty = String, desc = "MIME 类型，例如 image/png、image/jpeg、video/mp4，默认 image/png", optional },
        port! { name = "media_type", ty = String, desc = "媒体类型，可选 image 或 video，默认 image", optional },
    ];

    node_output![
        port! { name = "content_part", ty = ContentPart, desc = "封装后的多模态 ContentPart" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let bytes = match inputs.get("bytes") {
            Some(DataValue::Binary(b)) => b.clone(),
            _ => return Err(Error::ValidationError("bytes is required".to_string())),
        };

        let mime = match inputs.get("mime") {
            Some(DataValue::String(s)) if !s.trim().is_empty() => s.trim().to_string(),
            Some(_) | None => "image/png".to_string(),
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

        let data_url = format!("data:{mime};base64,{}", STANDARD.encode(&bytes));

        let part = match media_type.as_str() {
            "" | "image" => ContentPart::ImageUrl {
                image_url: MediaUrlSpec::Bare(data_url),
            },
            "video" => ContentPart::VideoUrl {
                video_url: MediaUrlSpec::Bare(data_url),
            },
            other => {
                return Err(Error::ValidationError(format!(
                    "media_type must be 'image' or 'video', got '{other}'"
                )))
            }
        };

        let mut outputs = HashMap::new();
        outputs.insert("content_part".to_string(), DataValue::ContentPart(part));

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
