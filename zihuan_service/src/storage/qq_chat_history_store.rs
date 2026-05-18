use std::sync::Arc;

use log::warn;

use zihuan_core::llm::OpenAIMessage;
use zihuan_core::runtime::block_async;
use zihuan_graph_engine::data_value::OpenAIMessageSessionCacheRef;

const LOG_PREFIX: &str = "[QqChatAgent]";

pub(crate) fn conversation_history_key(
    bot_id: &str,
    sender_id: &str,
    is_group: bool,
    group_id: Option<i64>,
) -> String {
    if is_group {
        format!(
            "group:{bot_id}:{}:{sender_id}",
            group_id.unwrap_or_default()
        )
    } else {
        format!("private:{bot_id}:{sender_id}")
    }
}

pub(crate) fn load_history(
    cache: &Arc<OpenAIMessageSessionCacheRef>,
    history_key: &str,
    legacy_key: &str,
) -> Vec<OpenAIMessage> {
    let history = block_async(cache.get_messages(history_key)).unwrap_or_default();
    if history.is_empty() && history_key != legacy_key {
        block_async(cache.get_messages(legacy_key)).unwrap_or_default()
    } else {
        history
    }
}

pub(crate) fn save_history(
    cache: &Arc<OpenAIMessageSessionCacheRef>,
    history_key: &str,
    messages: Vec<OpenAIMessage>,
) {
    if let Err(err) = block_async(cache.set_messages(history_key, messages)) {
        warn!("{LOG_PREFIX} Failed to save history for {history_key}: {err}");
    }
}
