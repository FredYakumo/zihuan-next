use log::{error, info};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use zihuan_core::error::Result;

use super::models::{MessageEvent, MessageType};
use crate::adapter::SharedBotAdapter;

/// Process messages (both private and group)
pub async fn process_message(ims_bot_adapter: SharedBotAdapter, event: MessageEvent) {
    let messages: Vec<String> = event.message_list.iter().map(|m| m.to_string()).collect();

    // Log based on message type
    match event.message_type {
        MessageType::Private => {
            info!(
                "[Friend Message] [Sender: {}({})] Message: {:?}",
                event.sender.nickname, event.sender.user_id, messages
            );
        }
        MessageType::Group => {
            info!(
                "[Group Message] [Group: {}({})] [Sender: {}({})] Message: {:?}",
                event.group_name.as_deref().unwrap_or_default(),
                event.group_id.unwrap_or_default(),
                event.sender.nickname,
                event.sender.user_id,
                messages
            );
        }
    }

    let handlers = {
        let ims_bot_adapter_guard = ims_bot_adapter.lock().await;
        ims_bot_adapter_guard.get_event_handlers()
    };

    for handler in handlers {
        if let Err(err) = (handler)(&event).await {
            error!("[Bot Adapter] Error processing event handler: {}", err);
        }
    }

    let brain_agent = {
        let ims_bot_adapter_guard = ims_bot_adapter.lock().await;
        ims_bot_adapter_guard.get_brain_agent().cloned()
    };

    if let Some(brain) = brain_agent {
        let ims_bot_adapter_clone = ims_bot_adapter.clone();
        tokio::spawn(async move {
            let mut ims_bot_adapter_guard = ims_bot_adapter_clone.lock().await;
            if let Err(e) = brain.on_event(&mut ims_bot_adapter_guard, &event) {
                error!("[Brain Agent] Error processing event: {}", e);
            }
        });
    }
}

/// Event handler type alias
pub type EventHandler = Arc<
    dyn for<'a> Fn(&'a MessageEvent) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>
        + Send
        + Sync,
>;
