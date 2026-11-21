use std::sync::Arc;

use zihuan_core::error::Result;
use zihuan_core::model_inference::llm::LLMMessage;
use zihuan_core::runtime::block_async;
use zihuan_core::storage::LLMMessageSessionCacheRef;

pub(crate) fn conversation_history_key(sender_id: &str) -> String {
    sender_id.to_string()
}

fn chat_preprompt_history_key(sender_id: &str) -> String {
    format!("chat_preprompt:{sender_id}")
}

pub(crate) fn load_history(
    cache: &Arc<LLMMessageSessionCacheRef>,
    history_key: &str,
) -> Result<Vec<LLMMessage>> {
    block_async(cache.get_messages(history_key))
}

pub(crate) fn save_history(
    cache: &Arc<LLMMessageSessionCacheRef>,
    history_key: &str,
    messages: Vec<LLMMessage>,
) -> Result<()> {
    let trace = zihuan_core::qq_context::current_turn_trace();
    block_async(cache.set_messages_for_turn(
        history_key,
        messages,
        trace.as_ref().map(|trace| trace.turn_id.as_str()),
    ))
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
