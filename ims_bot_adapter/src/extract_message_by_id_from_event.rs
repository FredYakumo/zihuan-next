use log::info;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::task::block_in_place;
use zihuan_core::error::{Error, Result};
use zihuan_graph_engine::message_restore::register_mysql_ref;
use zihuan_graph_engine::object_storage::S3Ref;
use zihuan_graph_engine::{node_input, node_output, DataType, DataValue, Node, Port};

use crate::adapter::{restore_message_list_for_message_id, shared_from_handle};
use crate::extract_message_from_event::{ExtractMessageFromEventNode, ExtractedMessageOutputs};

pub struct ExtractMessageByIdFromEventNode {
    id: String,
    name: String,
}

impl ExtractMessageByIdFromEventNode {
    const LOG_PREFIX: &str = "[ExtractMessageByIdFromEventNode]";

    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }

    fn collect_common_context(
        inputs: &HashMap<String, DataValue>,
    ) -> Result<(String, Option<Arc<S3Ref>>, crate::adapter::SharedBotAdapter)> {
        let ims_bot_adapter_ref = inputs
            .get("ims_bot_adapter")
            .and_then(|v| {
                if let DataValue::BotAdapterRef(handle) = v {
                    Some(shared_from_handle(handle))
                } else {
                    None
                }
            })
            .ok_or_else(|| Error::InvalidNodeInput("ims_bot_adapter input is required".to_string()))?;

        let explicit_s3_ref = inputs.get("s3_ref").and_then(|value| match value {
            DataValue::S3Ref(s3_ref) => Some(s3_ref.clone()),
            _ => None,
        });

        let (bot_id, adapter_object_storage) = if tokio::runtime::Handle::try_current().is_ok() {
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

        Ok((bot_id, explicit_s3_ref.or(adapter_object_storage), ims_bot_adapter_ref))
    }

    fn extract_target_message_id(inputs: &HashMap<String, DataValue>) -> Result<Option<i64>> {
        match inputs.get("message_id") {
            Some(DataValue::Integer(value)) if *value > 0 => Ok(Some(*value)),
            Some(DataValue::Integer(_)) => Err(Error::ValidationError(
                "message_id must be greater than 0".to_string(),
            )),
            None => Ok(None),
            Some(other) => Err(Error::ValidationError(format!(
                "message_id must be Integer when provided, got {}",
                other.data_type()
            ))),
        }
    }

    fn build_outputs_map(
        extracted: ExtractedMessageOutputs,
    ) -> HashMap<String, DataValue> {
        let mut outputs = HashMap::new();
        outputs.insert(
            "messages".to_string(),
            DataValue::Vec(
                Box::new(zihuan_graph_engine::DataType::OpenAIMessage),
                vec![DataValue::OpenAIMessage(extracted.user_message)],
            ),
        );
        outputs.insert("content".to_string(), DataValue::String(extracted.content));
        outputs.insert(
            "ref_content".to_string(),
            DataValue::String(extracted.ref_content),
        );
        outputs.insert("is_at_me".to_string(), DataValue::Boolean(extracted.is_at_me));
        outputs.insert(
            "at_target_list".to_string(),
            DataValue::Vec(
                Box::new(DataType::String),
                extracted
                    .at_target_list
                    .into_iter()
                    .map(DataValue::String)
                    .collect(),
            ),
        );
        outputs
    }
}

impl Node for ExtractMessageByIdFromEventNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("从当前消息事件或指定 message_id 恢复消息并提取 OpenAIMessage 列表")
    }

    node_input![
        port! { name = "message_event", ty = MessageEvent, desc = "当前触发的消息事件" },
        port! { name = "ims_bot_adapter", ty = BotAdapterRef, desc = "BotAdapter reference for context-aware system message", required = true },
        port! { name = "message_id", ty = Integer, desc = "可选：要恢复并分析的目标消息 ID", optional },
        port! { name = "mysql_ref", ty = MySqlRef, desc = "可选：显式注册给消息恢复链路的 MySQL 连接", optional },
        port! { name = "s3_ref", ty = S3Ref, desc = "可选：显式传入对象存储引用，优先用于多模态图片提取", optional }
    ];

    node_output![
        port! { name = "messages", ty = Vec(OpenAIMessage), desc = "Vec<OpenAIMessage> containing one user message" },
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

        let event = match inputs.get("message_event") {
            Some(DataValue::MessageEvent(event)) => event,
            _ => {
                return Err(Error::InvalidNodeInput(
                    "message_event input is required and must be MessageEvent type".to_string(),
                ))
            }
        };

        if let Some(DataValue::MySqlRef(mysql_ref)) = inputs.get("mysql_ref") {
            register_mysql_ref(mysql_ref.clone());
        }

        let (bot_id, object_storage, adapter_ref) = Self::collect_common_context(&inputs)?;
        let target_message_id = Self::extract_target_message_id(&inputs)?;

        info!(
            "{} resolving message content: target_message_id={:?} explicit_s3_ref_present={} mysql_ref_present={}",
            Self::LOG_PREFIX,
            target_message_id,
            inputs.contains_key("s3_ref"),
            inputs.contains_key("mysql_ref"),
        );

        let message_list = if let Some(message_id) = target_message_id {
            let resolved = if tokio::runtime::Handle::try_current().is_ok() {
                block_in_place(|| {
                    tokio::runtime::Handle::current()
                        .block_on(restore_message_list_for_message_id(&adapter_ref, message_id))
                })
            } else {
                tokio::runtime::Runtime::new()?.block_on(restore_message_list_for_message_id(
                    &adapter_ref,
                    message_id,
                ))
            }?;

            let resolved = resolved.ok_or_else(|| {
                Error::ValidationError(format!(
                    "message_id {} could not be restored from cache/redis/mysql/get_msg",
                    message_id
                ))
            })?;
            info!(
                "{} restored target message_id={} via {} (segments={})",
                Self::LOG_PREFIX,
                message_id,
                resolved.source_label,
                resolved.messages.len()
            );
            resolved.messages
        } else {
            event.message_list.clone()
        };

        let extracted = ExtractMessageFromEventNode::build_extracted_message_outputs(
            &message_list,
            &bot_id,
            object_storage.as_deref(),
        );
        info!(
            "{} output user message={}",
            Self::LOG_PREFIX,
            ExtractMessageFromEventNode::json_for_log(&extracted.user_message)
        );

        let outputs = Self::build_outputs_map(extracted);
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
