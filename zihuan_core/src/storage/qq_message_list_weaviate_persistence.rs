use std::sync::Arc;

use log::error;

use crate::error::{Error, Result};
use crate::ims_bot_adapter::models::message::Message;
use crate::model_inference::llm::embedding_base::EmbeddingBase;
use crate::storage::weaviate_persistence::upsert_qq_message_list;
use crate::weaviate::WeaviateRef;

pub fn persist_qq_message_list(
    weaviate_ref: &Arc<WeaviateRef>,
    embedding_model: &dyn EmbeddingBase,
    messages: &[Message],
    message_id: &str,
    sender_id: &str,
    sender_name: &str,
    group_id: Option<&str>,
    group_name: Option<&str>,
) -> Result<bool> {
    let message_id = required_string(message_id, "message_id")?;
    let sender_id = required_string(sender_id, "sender_id")?;
    let sender_name = required_string(sender_name, "sender_name")?;
    let group_id = optional_non_empty_string(group_id);
    let group_name = optional_non_empty_string(group_name);

    match upsert_qq_message_list(
        weaviate_ref,
        messages,
        &message_id,
        &sender_id,
        &sender_name,
        group_id.as_deref(),
        group_name.as_deref(),
        embedding_model,
    ) {
        Ok(_) => Ok(true),
        Err(error) => {
            error!(
                "[qq_message_list_weaviate_persistence] failed to persist message vector: {error}"
            );
            Ok(false)
        }
    }
}

fn required_string(value: &str, key: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(Error::ValidationError(format!("{key} must not be empty")));
    }
    Ok(value.to_string())
}

fn optional_non_empty_string(value: Option<&str>) -> Option<String> {
    value.map(str::trim).filter(|value| !value.is_empty()).map(ToOwned::to_owned)
}
