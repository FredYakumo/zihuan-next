use std::collections::HashMap;
use std::sync::Arc;

use ims_bot_adapter::models::message::{Message, PersistedMedia};
use zihuan_core::data_refs::RelationalDbConnection;
use zihuan_core::error::Result;

use crate::agent::qq_chat::msg_send::QqChatServiceReplyDirective;

/// Simple report of handling result.
#[derive(Debug, Clone)]
pub(crate) struct QqChatServiceHandleReport {
    pub(crate) result_summary: String,
}

/// Request to build a reply batch from the model's reply text.
#[derive(Debug, Clone)]
pub(crate) struct QqChatServiceReplyBuildRequest {
    pub assistant_text: String,
    pub is_group: bool,
    pub sender_id: String,
    pub sender_nickname: String,
    pub sender_card: String,
    pub bot_id: String,
    pub bot_name: String,
    pub max_message_length: usize,
    pub reply_directive: Option<QqChatServiceReplyDirective>,
    pub trigger_message_id: Option<i64>,
    pub available_media: HashMap<String, PersistedMedia>,
    pub rdb_pool: Option<RelationalDbConnection>,
}

/// Result of building reply batches.
#[derive(Debug, Clone)]
pub(crate) struct QqChatServiceReplyBuildResult {
    pub batches: Vec<Vec<Message>>,
    pub suppress_send: bool,
}

/// Builder type for constructing reply batches from a build request.
pub(crate) type QqChatServiceReplyBatchBuilder =
    Arc<dyn Fn(&QqChatServiceReplyBuildRequest) -> Result<QqChatServiceReplyBuildResult> + Send + Sync>;

/// Result summary for a single QQ chat turn.
#[derive(Debug, Clone)]
pub(crate) struct QqChatServiceTurnResult {
    pub(crate) result_summary: String,
}
