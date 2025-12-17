# Copilot Instructions: src/bot_adapter/event.rs

## Purpose
Platform-specific event handlers for QQ private/group messages. Extend here to add bot responses, persistence, and context-aware logic.

## Current Implementation

### Handler Signature
```rust
pub fn process_friend_message(event: &MessageEvent) {
    info!("Sender: {}, Message: {:?}", event.sender.user_id, event.message_list);
}

pub fn process_group_message(event: &MessageEvent) {
    info!("Sender: {}, Message: {:?}", event.sender.user_id, event.message_list);
}
```

### MessageEvent Structure
```rust
pub struct MessageEvent {
    pub message_id: i64,
    pub message_type: MessageType,  // Private or Group
    pub sender: Sender,              // user_id, nickname, card, role
    pub message_list: Vec<Message>,  // Typed message enum variants
}
```
Note: Serde deserializes from `RawMessageEvent` into this structure automatically.

## Extension Patterns

### Add Response Logic
```rust
pub fn process_group_message(event: &MessageEvent) {
    // 1. Log incoming message
    info!("Sender: {}, Message: {:?}", event.sender.user_id, event.message_list);
    
    // 2. Extract text content
    let text_messages: Vec<String> = event.message_list.iter()
        .filter_map(|msg| {
            if let Message::PlainText(text_msg) = msg {
                Some(text_msg.text.clone())
            } else {
                None
            }
        })
        .collect();
    let full_text = text_messages.join("");
    
    // 3. Check for mentions
    let at_messages: Vec<&Message> = event.message_list.iter()
        .filter(|msg| matches!(msg, Message::At(_)))
        .collect();
    
    // 4. Retrieve context (if needed) - requires async context
    // In async handler:
    // for msg in &event.message_list {
    //     if let Message::Replay(reply_msg) = msg {
    //         let store = message_store.lock().await;
    //         if let Some(original) = store.get_message(&reply_msg.message_id.to_string()).await {
    //             // Process original JSON string
    //         }
    //     }
    // }
    
    // 5. Generate and send response
    // TODO: Implement LLM call + response sending
}
```

### Handle Specific Message Types
```rust
pub fn process_group_message(event: &MessageEvent) {
    for msg in &event.message_list {
        match msg {
            Message::At(at_msg) => {
                info!("Bot was mentioned: target_id={}", at_msg.target_id);
                // Trigger special behavior
            }
            Message::Replay(reply_msg) => {
                info!("Reply to message {}", reply_msg.message_id);
                // Load context from original message
            }
            Message::PlainText(text_msg) => {
                info!("Text content: {}", text_msg.text);
                // Process text content
            }
        }
    }
}
```

### Add New Platform Handler
```python
def process_web_message(event: MessageEvent):
    """Handler for web platform messages."""
    logger.info(f"Web message from: {event.sender.user_id}")
    # Implement web-specific logic (e.g., HTML formatting, image rendering)
```

## Persistent Storage Integration

Currently messages are stored in Redis cache only. To add database persistence:

### Example Database Layer (requires adding sqlx or diesel)
```rust
// Not yet implemented - would require adding database dependencies
// Example pattern with sqlx:
/*
use sqlx::PgPool;

pub async fn save_message_to_db(pool: &PgPool, event: &MessageEvent) -> Result<(), sqlx::Error> {
    let content = event.message_list.iter()
        .map(|msg| format!("{:?}", msg))
        .collect::<Vec<_>>()
        .join("");
    
    let at_targets = event.message_list.iter()
        .filter_map(|msg| {
            if let Message::At(at_msg) = msg {
                Some(at_msg.target_id.to_string())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join(",");
    
    sqlx::query!(
        "INSERT INTO message_record (message_id, sender_id, sender_name, content, at_target_list) 
         VALUES ($1, $2, $3, $4, $5)",
        event.message_id.to_string(),
        event.sender.user_id.to_string(),
        event.sender.nickname,
        content,
        at_targets
    )
    .execute(pool)
    .await?;
    
    Ok(())
}
*/
```

## Integration Points
- Called by: `BotAdapter::process_event()`
- Uses: `MessageEvent` from `src/bot_adapter/models/event_model.rs`
- Accesses: Message store via `Arc<TokioMutex<MessageStore>>` passed from adapter
- Persistence: Currently Redis cache only; database layer can be added as needed
