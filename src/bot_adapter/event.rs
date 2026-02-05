use log::{info, error, debug};
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;
use std::future::Future;
use std::pin::Pin;
use chrono::Local;

use super::models::{MessageEvent, MessageType};
use crate::{bot_adapter::adapter::SharedBotAdapter, util::message_store::{MessageRecord, MessageStore}};

/// Process messages (both private and group)
pub async fn process_message(bot_adapter: SharedBotAdapter, event: MessageEvent, store: Arc<TokioMutex<MessageStore>>) {
    let messages: Vec<String> = event.message_list.iter()
        .map(|m| m.to_string())
        .collect();
    
    // Log based on message type
    match event.message_type {
        MessageType::Private => {
            info!(
                "[Friend Message] [Sender: {}({})] Message: {:?}",
                event.sender.nickname,
                event.sender.user_id,
                messages
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

    // Determine sender name (prefer card for group messages)
    let sender_name = if event.is_group_message && !event.sender.card.is_empty() {
        event.sender.card.clone()
    } else {
        event.sender.nickname.clone()
    };

    // Extract @mentions for group messages, or use bot_id for private messages
    let at_target_list = if event.is_group_message {
        let at_list: Vec<String> = event.message_list.iter()
            .filter_map(|m| {
                if let crate::bot_adapter::models::message::Message::At(at_msg) = m {
                    at_msg.target.as_ref().map(|id| id.to_string())
                } else {
                    None
                }
            })
            .collect();
        
        if at_list.is_empty() {
            None
        } else {
            Some(at_list.join(","))
        }
    } else {
        // For private messages, use bot_id as at_target
        let bot_id = {
            let bot_adapter_guard = bot_adapter.lock().await;
            bot_adapter_guard.get_bot_id().to_string()
        };
        Some(bot_id)
    };

    // Store full message record to MySQL
    let record = MessageRecord {
        message_id: event.message_id.to_string(),
        sender_id: event.sender.user_id.to_string(),
        sender_name,
        send_time: Local::now().naive_local(),
        group_id: event.group_id.map(|id| id.to_string()),
        group_name: event.group_name.clone(),
        content: messages.join(" "),
        at_target_list,
    };

    let store_guard = store.lock().await;
    if let Err(e) = store_guard.store_message_record(&record).await {
        error!("[Event] Failed to persist message record: {}", e);
    } else {
        debug!("[Event] Message record persisted: {}", record.message_id);
    }

    let handlers = {
        let bot_adapter_guard = bot_adapter.lock().await;
        bot_adapter_guard.get_event_handlers()
    };

    for handler in handlers {
        (handler)(&event, store.clone()).await;
    }

    let brain_agent = {
        let bot_adapter_guard = bot_adapter.lock().await;
        bot_adapter_guard.get_brain_agent().cloned()
    };

    if let Some(brain) = brain_agent {
        let bot_adapter_clone = bot_adapter.clone();
        tokio::spawn(async move {
            let mut bot_adapter_guard = bot_adapter_clone.lock().await;
            if let Err(e) = brain.on_event(&mut bot_adapter_guard, &event) {
                error!("[Brain Agent] Error processing event: {}", e);
            }
        });
    }
}

/// Event handler type alias
pub type EventHandler = Arc<
    dyn for<'a> Fn(&'a MessageEvent, Arc<TokioMutex<MessageStore>>) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>
        + Send
        + Sync,
>;
