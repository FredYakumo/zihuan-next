use std::sync::Arc;

use log::warn;

use zihuan_core::error::{Error, Result};
use zihuan_core::llm::LLMMessage;
use zihuan_core::runtime::block_async;
use zihuan_graph_engine::data_value::LLMMessageSessionCacheRef;

const LOG_PREFIX: &str = "[QqChatAgentService]";

pub(crate) fn conversation_history_key(sender_id: &str) -> String {
    sender_id.to_string()
}

pub(crate) fn emotion_history_key(sender_id: &str) -> String {
    format!("emotion:{sender_id}")
}

pub(crate) fn load_history(cache: &Arc<LLMMessageSessionCacheRef>, history_key: &str) -> Vec<LLMMessage> {
    block_async(cache.get_messages(history_key)).unwrap_or_default()
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

pub(crate) fn clear_history(cache: &Arc<LLMMessageSessionCacheRef>, sender_id: &str) -> Result<()> {
    let history_key = conversation_history_key(sender_id);
    clear_history_key(cache, &history_key)?;

    let emotion_history_key = emotion_history_key(sender_id);
    clear_history_key(cache, &emotion_history_key)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{conversation_history_key, emotion_history_key};

    #[test]
    fn history_keys_are_scoped_to_sender() {
        let conversation_key = conversation_history_key("sender");
        let emotion_key = emotion_history_key("sender");

        assert_eq!(conversation_key, "sender");
        assert_eq!(emotion_key, "emotion:sender");
        assert_ne!(conversation_key, emotion_key);
    }
}
