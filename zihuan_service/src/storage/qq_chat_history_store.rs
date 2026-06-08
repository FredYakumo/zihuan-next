use std::sync::Arc;

use log::warn;

use zihuan_core::error::{Error, Result};
use zihuan_core::llm::LLMMessage;
use zihuan_core::runtime::block_async;
use zihuan_graph_engine::data_value::LLMMessageSessionCacheRef;

const LOG_PREFIX: &str = "[QqChatAgent]";

pub(crate) fn conversation_history_key(bot_id: &str, sender_id: &str, is_group: bool, group_id: Option<i64>) -> String {
    if is_group {
        format!("group:{bot_id}:{}:{sender_id}", group_id.unwrap_or_default())
    } else {
        format!("private:{bot_id}:{sender_id}")
    }
}

pub(crate) fn load_history(
    cache: &Arc<LLMMessageSessionCacheRef>,
    history_key: &str,
    legacy_key: &str,
) -> Vec<LLMMessage> {
    let history = block_async(cache.get_messages(history_key)).unwrap_or_default();
    if history.is_empty() && history_key != legacy_key {
        block_async(cache.get_messages(legacy_key)).unwrap_or_default()
    } else {
        history
    }
}

pub(crate) fn save_history(cache: &Arc<LLMMessageSessionCacheRef>, history_key: &str, messages: Vec<LLMMessage>) {
    if let Err(err) = block_async(cache.set_messages(history_key, messages)) {
        warn!("{LOG_PREFIX} Failed to save history for {history_key}: {err}");
    }
}

fn clear_history_key(cache: &Arc<LLMMessageSessionCacheRef>, history_key: &str) -> Result<()> {
    block_async(cache.clear_messages(history_key))
        .map(|_| ())
        .map_err(|err| Error::StringError(format!("failed to clear QQ chat history for key '{history_key}': {err}")))
}

pub(crate) fn clear_history(
    cache: &Arc<LLMMessageSessionCacheRef>,
    bot_id: &str,
    sender_id: &str,
    is_group: bool,
    group_id: Option<i64>,
) -> Result<()> {
    let history_key = conversation_history_key(bot_id, sender_id, is_group, group_id);
    clear_history_key(cache, &history_key)?;

    let legacy_key = sender_id.trim();
    if !legacy_key.is_empty() && legacy_key != history_key {
        clear_history_key(cache, legacy_key)?;
    }

    Ok(())
}
