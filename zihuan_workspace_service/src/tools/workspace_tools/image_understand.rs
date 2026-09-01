use std::sync::Arc;

use serde_json::Value;
use zihuan_core::agent::tools::{Tool, ToolExecutionOutput};
use zihuan_core::error::{Error, Result};
use zihuan_core::ims_bot_adapter::models::message::{ImageMessage, PersistedMedia};
use zihuan_core::ims_bot_adapter::runtime::multimodal_image_url::resolve_image_message_part;
use zihuan_core::model_inference::llm::llm_base::LLMBase;
use zihuan_core::model_inference::llm::tooling::{FunctionTool, StaticFunctionToolSpec};
use zihuan_core::model_inference::llm::{InferenceParam, LLMMessage, MessagePart};

pub(crate) const DEFAULT_TOOL_IMAGE_UNDERSTAND: &str = "image_understand";

pub(crate) struct ImageUnderstandTool {
    media: Vec<PersistedMedia>,
    llm: Arc<dyn LLMBase>,
}

impl ImageUnderstandTool {
    pub(crate) fn new(media: Vec<PersistedMedia>, llm: Arc<dyn LLMBase>) -> Self {
        Self { media, llm }
    }

    fn run(&self, arguments: &Value) -> Result<String> {
        if !self.llm.supports_multimodal_input() {
            return Err(Error::ValidationError(
                "当前图片理解模型不支持多模态输入，请先选择支持多模态的模型后再重试。".to_string(),
            ));
        }
        let media_id = arguments
            .get("media_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| Error::ValidationError("media_id is required".to_string()))?;
        let media = self.media.iter().find(|item| item.media_id == media_id).ok_or_else(|| {
            Error::ValidationError(format!(
                "image_understand could not find media_id '{}' in this conversation",
                media_id
            ))
        })?;
        let image = ImageMessage::new(media.clone());
        let resolved =
            resolve_image_message_part(&image, None, false, "[Workspace image_understand]")
                .ok_or_else(|| {
                    Error::ValidationError(
                        "image_understand could not resolve image content".to_string(),
                    )
                })?;
        let prompt = arguments.get("question").and_then(Value::as_str).map(str::trim).filter(|value| !value.is_empty())
            .map(|value| format!("Answer the following focus point based on the image and provide a concise, objective description.\nFocus: {value}"))
            .unwrap_or_else(|| "Describe the main content of this image concisely and objectively.".to_string());
        let messages = vec![
            LLMMessage::system("You are an image understanding assistant. Output only concise, objective descriptions without extra pleasantries. If the image content is empty, invalid, or unrecognizable, output only \"No image recognized.\""),
            LLMMessage::user_with_parts(vec![MessagePart::text(prompt), resolved.part]),
        ];
        self.llm
            .inference(&InferenceParam { messages: &messages, tools: None })
            .content_text_owned()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                Error::ValidationError("image_understand returned empty response".to_string())
            })
    }
}

impl Tool for ImageUnderstandTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(StaticFunctionToolSpec {
            name: DEFAULT_TOOL_IMAGE_UNDERSTAND,
            description: "Understand image content by media_id and return a concise, objective text description",
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "media_id": { "type": "string", "description": "The media_id of the image to analyze" },
                    "question": { "type": "string", "description": "Optional focus point or question" }
                },
                "required": ["media_id"]
            }),
        })
    }

    fn execute(&self, _call_content: &str, arguments: &Value) -> String {
        self.run(arguments)
            .unwrap_or_else(|error| serde_json::json!({ "error": error.to_string() }).to_string())
    }

    fn execute_with_outcome(&self, call_content: &str, arguments: &Value) -> ToolExecutionOutput {
        ToolExecutionOutput::text(self.execute(call_content, arguments))
    }
}
