use std::sync::Arc;

use log::warn;

use zihuan_core::error::Result;
use zihuan_core::model_inference::llm::LLMMessage;
use zihuan_core::runtime::block_async;
use zihuan_core::storage::LLMMessageSessionCacheRef;

const LOG_PREFIX: &str = "[QqChatAgentService]";

pub(crate) fn conversation_history_key(sender_id: &str) -> String {
    sender_id.to_string()
}

fn chat_preprompt_history_key(sender_id: &str) -> String {
    format!("chat_preprompt:{sender_id}")
}

pub(crate) fn load_history(
    cache: &Arc<LLMMessageSessionCacheRef>,
    history_key: &str,
) -> Vec<LLMMessage> {
    match block_async(cache.get_messages(history_key)) {
        Ok(messages) => messages,
        Err(err) => {
            // The turn still runs, but with no history. That is impossible to notice from the
            // reply alone, so the failure must be visible in the logs.
            warn!("{LOG_PREFIX} Failed to load history for {history_key}: {err}");
            Vec::new()
        }
    }
}

pub(crate) fn save_history(
    cache: &Arc<LLMMessageSessionCacheRef>,
    history_key: &str,
    messages: Vec<LLMMessage>,
) {
    if let Err(err) = block_async(cache.set_messages(history_key, messages)) {
        warn!("{LOG_PREFIX} Failed to save history for {history_key}: {err}");
    }
}

fn clear_history_key(cache: &Arc<LLMMessageSessionCacheRef>, history_key: &str) -> Result<()> {
    block_async(cache.clear_messages(history_key)).map(|_| ()).map_err(|err| {
        zihuan_core::string_error!("failed to clear QQ chat history for key '{history_key}': {err}")
    })
}

pub fn clear_history(cache: &Arc<LLMMessageSessionCacheRef>, sender_id: &str) -> Result<()> {
    let history_key = conversation_history_key(sender_id);
    clear_history_key(cache, &history_key)?;

    let chat_preprompt_history_key = chat_preprompt_history_key(sender_id);
    clear_history_key(cache, &chat_preprompt_history_key)?;

    Ok(())
}
