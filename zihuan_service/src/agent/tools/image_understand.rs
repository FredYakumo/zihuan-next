use std::collections::HashMap;
use std::sync::Arc;

use ims_bot_adapter::models::message::{ImageMessage, PersistedMedia};
use log::warn;
use model_inference::system_config::load_llm_refs;
use serde_json::Value;
use storage_handler::RuntimeStorageConnectionManager;
use zihuan_agent::brain::BrainTool;
use zihuan_core::agent_config::{current_qq_chat_agent_config, image_understand_llm_ref_id};
use zihuan_core::data_refs::MySqlConfig;
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::tooling::FunctionTool;
use zihuan_core::llm::{ContentPart, InferenceParam, OpenAIMessage};
use zihuan_core::runtime::block_async;
use zihuan_graph_engine::message_restore::{
    find_media_in_messages, register_mysql_ref, restore_media_by_id,
};
use zihuan_graph_engine::object_storage::S3Ref;
use zihuan_graph_engine::DataValue;

use crate::resource_resolver::{build_llm_model, resolve_llm_service_config};

use super::common::{optional_string_argument, StaticFunctionToolSpec, ToolNotificationTarget};

const LOG_PREFIX: &str = "[QqChatAgent]";
pub(crate) const DEFAULT_TOOL_IMAGE_UNDERSTAND: &str = "image_understand";

pub(crate) struct ImageUnderstandBrainTool {
    current_event: Option<ims_bot_adapter::models::MessageEvent>,
    mysql_ref: Option<Arc<MySqlConfig>>,
    s3_ref: Option<Arc<S3Ref>>,
    notification_target: ToolNotificationTarget,
}

impl ImageUnderstandBrainTool {
    pub(crate) fn new(
        current_event: Option<ims_bot_adapter::models::MessageEvent>,
        mysql_ref: Option<Arc<MySqlConfig>>,
        s3_ref: Option<Arc<S3Ref>>,
        notification_target: ToolNotificationTarget,
    ) -> Self {
        Self {
            current_event,
            mysql_ref,
            s3_ref,
            notification_target,
        }
    }
}

impl BrainTool for ImageUnderstandBrainTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(build_image_understand_spec())
    }

    fn execute(&self, call_content: &str, arguments: &Value) -> String {
        self.notification_target.notify_progress(call_content);
        let result = execute_image_understand(
            arguments,
            self.current_event.as_ref(),
            self.mysql_ref.clone(),
            self.s3_ref.clone(),
        );

        match result {
            Ok(text) => text,
            Err(error) => serde_json::json!({"error": error.to_string()}).to_string(),
        }
    }
}

pub(crate) fn build_image_understand_spec() -> StaticFunctionToolSpec {
    StaticFunctionToolSpec {
        name: DEFAULT_TOOL_IMAGE_UNDERSTAND,
        description: "根据 media_id 理解图片内容，并返回简洁、客观的文字描述",
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "media_id": { "type": "string", "description": "要分析的图片 media_id" },
                "question": { "type": "string", "description": "可选：本次关注点、问题或分析要求" }
            },
            "required": ["media_id"]
        }),
    }
}

pub(crate) fn execute_image_understand_tool(
    arguments: &Value,
    runtime_values: &HashMap<String, DataValue>,
) -> Result<String> {
    let message_event = runtime_values
        .get("message_event")
        .and_then(|value| match value {
            DataValue::MessageEvent(event) => Some(event),
            _ => None,
        });
    let mysql_ref = runtime_values
        .get("mysql_ref")
        .and_then(|value| match value {
            DataValue::MySqlRef(mysql_ref) => Some(mysql_ref.clone()),
            _ => None,
        });
    let s3_ref = runtime_values.get("s3_ref").and_then(|value| match value {
        DataValue::S3Ref(s3_ref) => Some(s3_ref.clone()),
        _ => None,
    });

    execute_image_understand(arguments, message_event, mysql_ref, s3_ref)
}

fn execute_image_understand(
    arguments: &Value,
    current_event: Option<&ims_bot_adapter::models::MessageEvent>,
    mysql_ref: Option<Arc<MySqlConfig>>,
    s3_ref: Option<Arc<S3Ref>>,
) -> Result<String> {
    let media_id = optional_string_argument(arguments, "media_id")
        .ok_or_else(|| Error::ValidationError("media_id is required".to_string()))?;
    let focus_text = optional_string_argument(arguments, "content");

    let persisted_media = resolve_image_understand_media(&media_id, current_event, mysql_ref)?;
    let s3_ref = resolve_image_understand_s3_ref(s3_ref)?;
    let description =
        analyze_persisted_media(&persisted_media, focus_text.as_deref(), s3_ref.as_deref())?;
    Ok(description)
}

fn resolve_image_understand_media(
    media_id: &str,
    current_event: Option<&ims_bot_adapter::models::MessageEvent>,
    mysql_ref: Option<Arc<MySqlConfig>>,
) -> Result<PersistedMedia> {
    if let Some(event) = current_event {
        if let Some(media) = find_media_in_messages(&event.message_list, media_id) {
            return Ok(media);
        }
    }

    if let Some(mysql_ref) = mysql_ref {
        register_mysql_ref(mysql_ref);
    } else if let Some(mysql_ref) = load_agent_mysql_ref().transpose()? {
        register_mysql_ref(mysql_ref);
    }

    restore_media_by_id(media_id)?.ok_or_else(|| {
        Error::ValidationError(format!(
            "image_understand could not find media_id '{}'",
            media_id
        ))
    })
}

fn resolve_image_understand_s3_ref(s3_ref: Option<Arc<S3Ref>>) -> Result<Option<Arc<S3Ref>>> {
    if s3_ref.is_some() {
        return Ok(s3_ref);
    }
    load_agent_s3_ref().transpose()
}

fn load_agent_mysql_ref() -> Option<Result<Arc<MySqlConfig>>> {
    let config = current_qq_chat_agent_config().ok()?;
    let connection_id = config
        .resolved_rdb_id()
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    Some(block_async(
        RuntimeStorageConnectionManager::shared().get_or_create_mysql_ref(connection_id),
    ))
}

fn load_agent_s3_ref() -> Option<Result<Arc<S3Ref>>> {
    let config = current_qq_chat_agent_config().ok()?;
    let connection_id = config
        .rustfs_connection_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    Some(block_async(
        RuntimeStorageConnectionManager::shared().get_or_create_s3_ref(connection_id),
    ))
}

fn analyze_persisted_media(
    media: &PersistedMedia,
    focus_text: Option<&str>,
    s3_ref: Option<&S3Ref>,
) -> Result<String> {
    let image_message = ImageMessage::new(media.clone());
    let resolved = match ims_bot_adapter::multimodal_image_url::resolve_image_message_part(
        &image_message,
        s3_ref,
        false,
        LOG_PREFIX,
    ) {
        Some(resolved) => resolved,
        None => return Ok("Not any image found".to_string()),
    };

    let llm = load_multimodal_llm()?;

    let prompt = match focus_text.map(str::trim).filter(|value| !value.is_empty()) {
        Some(text) => format!(
            "请基于图片回答下面这个关注点，给出简洁、客观的中文描述。\n关注点：{}",
            text
        ),
        None => "请用简洁、客观的中文描述这张图片的主要内容。".to_string(),
    };

    let messages = vec![
        OpenAIMessage::system(
            "你是一个图片理解助手。只输出简洁、客观的中文描述，不要输出多余寒暄。如果图片内容为空、无效或无法识别，只输出\"没有识别到图片\"。".to_string(),
        ),
        OpenAIMessage::user_with_parts(vec![ContentPart::text(prompt), resolved.part]),
    ];
    let response = llm.inference(&InferenceParam {
        messages: &messages,
        tools: None,
    });

    let content = response.content_text_owned().unwrap_or_default();
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Err(Error::ValidationError(
            "image_understand returned empty response".to_string(),
        ));
    }

    Ok(trimmed.to_string())
}

fn load_multimodal_llm() -> Result<Arc<dyn zihuan_core::llm::llm_base::LLMBase>> {
    let agent_config = current_qq_chat_agent_config()?;
    let llm_refs = load_llm_refs()?;
    let llm_ref_id = image_understand_llm_ref_id(&agent_config)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            Error::ValidationError(
                "image_understand requires a main llm_ref_id or a dedicated image_understand_llm_ref_id"
                    .to_string(),
            )
        })?;
    let llm_config =
        resolve_llm_service_config(Some(llm_ref_id), &llm_refs, DEFAULT_TOOL_IMAGE_UNDERSTAND)?;
    if !llm_config.supports_multimodal_input {
        let error_message = if agent_config
            .image_understand_llm_ref_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some()
        {
            format!(
                "image_understand_llm_ref_id '{}' does not support multimodal input",
                llm_ref_id
            )
        } else {
            format!(
                "main llm_ref_id '{}' does not support multimodal input; please choose a multimodal model for image_understand_llm_ref_id",
                llm_ref_id
            )
        };
        return Err(Error::ValidationError(error_message));
    }

    build_llm_model(&llm_config).map_err(|error| {
        warn!(
            "{LOG_PREFIX} image_understand failed to build multimodal llm '{}': {}",
            llm_ref_id, error
        );
        error
    })
}
