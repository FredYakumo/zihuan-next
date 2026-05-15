use serde::{Deserialize, Serialize};

use crate::llm::tooling::ToolCalls;

use super::message_role::MessageRole;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Parts(Vec<ContentPart>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    Text { text: String },
    ImageUrl { image_url: MediaUrlSpec },
    VideoUrl { video_url: MediaUrlSpec },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MediaUrlSpec {
    Bare(String),
    Object { url: String },
}

impl MediaUrlSpec {
    pub fn as_url(&self) -> &str {
        match self {
            MediaUrlSpec::Bare(s) => s.as_str(),
            MediaUrlSpec::Object { url } => url.as_str(),
        }
    }
}

impl ContentPart {
    pub fn text<S: Into<String>>(s: S) -> Self {
        ContentPart::Text { text: s.into() }
    }

    pub fn image_url_string<S: Into<String>>(url: S) -> Self {
        ContentPart::ImageUrl {
            image_url: MediaUrlSpec::Bare(url.into()),
        }
    }

    pub fn image_data_url<M: AsRef<str>, B: AsRef<str>>(mime: M, base64_payload: B) -> Self {
        ContentPart::ImageUrl {
            image_url: MediaUrlSpec::Bare(format!(
                "data:{};base64,{}",
                mime.as_ref(),
                base64_payload.as_ref()
            )),
        }
    }

    pub fn video_url_string<S: Into<String>>(url: S) -> Self {
        ContentPart::VideoUrl {
            video_url: MediaUrlSpec::Bare(url.into()),
        }
    }

    pub fn video_data_url<M: AsRef<str>, B: AsRef<str>>(mime: M, base64_payload: B) -> Self {
        ContentPart::VideoUrl {
            video_url: MediaUrlSpec::Bare(format!(
                "data:{};base64,{}",
                mime.as_ref(),
                base64_payload.as_ref()
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIMessage {
    pub role: MessageRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_style: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<MessageContent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCalls>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl OpenAIMessage {
    pub fn system<S: Into<String>>(content: S) -> Self {
        Self {
            role: MessageRole::System,
            api_style: None,
            content: Some(MessageContent::Text(content.into())),
            reasoning_content: None,
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    pub fn user<S: Into<String>>(content: S) -> Self {
        Self {
            role: MessageRole::User,
            api_style: None,
            content: Some(MessageContent::Text(content.into())),
            reasoning_content: None,
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    pub fn user_with_parts(parts: Vec<ContentPart>) -> Self {
        Self {
            role: MessageRole::User,
            api_style: None,
            content: Some(MessageContent::Parts(parts)),
            reasoning_content: None,
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    pub fn assistant_text<S: Into<String>>(content: S) -> Self {
        Self {
            role: MessageRole::Assistant,
            api_style: None,
            content: Some(MessageContent::Text(content.into())),
            reasoning_content: None,
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    pub fn tool_result<S: Into<String>>(tool_call_id: S, content: S) -> Self {
        Self {
            role: MessageRole::Tool,
            api_style: None,
            content: Some(MessageContent::Text(content.into())),
            reasoning_content: None,
            tool_calls: Vec::new(),
            tool_call_id: Some(tool_call_id.into()),
        }
    }

    pub fn content_text(&self) -> Option<&str> {
        match self.content.as_ref()? {
            MessageContent::Text(s) => Some(s.as_str()),
            MessageContent::Parts(_) => None,
        }
    }

    pub fn with_api_style<S: Into<String>>(mut self, api_style: S) -> Self {
        self.api_style = Some(api_style.into());
        self
    }

    pub fn content_text_owned(&self) -> Option<String> {
        match self.content.as_ref()? {
            MessageContent::Text(s) => Some(s.clone()),
            MessageContent::Parts(parts) => {
                let combined = parts
                    .iter()
                    .filter_map(|p| match p {
                        ContentPart::Text { text } => Some(text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                if combined.is_empty() {
                    None
                } else {
                    Some(combined)
                }
            }
        }
    }
}
