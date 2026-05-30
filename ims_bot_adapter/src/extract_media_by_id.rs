use log::info;
use node_macros::return_with_node_output;
use std::collections::HashMap;
use std::sync::Arc;
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::OpenAIMessage;
use zihuan_graph_engine::message_restore::{register_mysql_ref, restore_media_by_id};
use zihuan_graph_engine::object_storage::S3Ref;
use zihuan_graph_engine::{node_input, node_output, DataType, DataValue, Node, Port};

use crate::models::message::ImageMessage;
use crate::multimodal_image_url::resolve_image_message_part;

pub struct ExtractMediaByIdNode {
    id: String,
    name: String,
}

impl ExtractMediaByIdNode {
    const LOG_PREFIX: &str = "[ExtractMediaByIdNode]";

    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for ExtractMediaByIdNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("通过持久化媒体 ID 从数据库恢复图片并转换为 OpenAIMessage")
    }

    node_input![
        port! { name = "media_id", ty = String, desc = "持久化媒体 ID" },
        port! { name = "mysql_ref", ty = MySqlRef, desc = "MySQL 连接引用，用于查询消息记录表" },
        port! { name = "s3_ref", ty = S3Ref, desc = "对象存储引用，用于读取图片字节" }
    ];

    node_output![
        port! { name = "messages", ty = Vec(OpenAIMessage), desc = "Vec<OpenAIMessage> 包含一条带图片内容的用户消息" },
        port! { name = "content", ty = String, desc = "人类可读的图片摘要标签" }
    ];

    fn execute(
        &mut self,
        inputs: zihuan_graph_engine::NodeInputFlow,
    ) -> Result<zihuan_graph_engine::NodeOutputFlow> {
        self.validate_inputs(&inputs)?;

        let media_id = match inputs.get("media_id") {
            Some(DataValue::String(value)) => value.trim(),
            _ => {
                return Err(Error::InvalidNodeInput(
                    "media_id input is required and must be a non-empty String".to_string(),
                ));
            }
        };
        if media_id.is_empty() {
            return Err(Error::InvalidNodeInput(
                "media_id must be a non-empty String".to_string(),
            ));
        }

        if let Some(DataValue::MySqlRef(mysql_ref)) = inputs.get("mysql_ref") {
            register_mysql_ref(mysql_ref.clone());
        }

        let s3_ref: Option<Arc<S3Ref>> = inputs.get("s3_ref").and_then(|value| match value {
            DataValue::S3Ref(s3_ref) => Some(s3_ref.clone()),
            _ => None,
        });

        info!(
            "{} looking up media by id media_id={}",
            Self::LOG_PREFIX,
            media_id,
        );

        let persisted_media = restore_media_by_id(media_id)?.ok_or_else(|| {
            Error::ValidationError(format!("media_id {media_id} not found in database"))
        })?;

        let image_message = ImageMessage::new(persisted_media);

        let resolved =
            resolve_image_message_part(&image_message, s3_ref.as_deref(), false, Self::LOG_PREFIX)
                .ok_or_else(|| {
                    Error::ValidationError(format!(
                "failed to resolve image bytes for media_id={media_id}: no safe source available"
            ))
                })?;

        let user_message = OpenAIMessage::user_with_parts(vec![resolved.part]);
        let content_label = format!("[Image media_id={media_id}]");

        info!(
            "{} resolved media to user message media_id={}",
            Self::LOG_PREFIX,
            media_id,
        );

        return_with_node_output![self;
            "messages" => DataValue::Vec(
                Box::new(DataType::OpenAIMessage),
                vec![DataValue::OpenAIMessage(user_message)],
            ),
            "content" => DataValue::String(content_label)
        ]
    }
}
