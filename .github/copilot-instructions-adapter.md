# Copilot Instructions: src/bot_adapter/adapter.rs

## Purpose
Core event processing engine. Connects to QQ bot server via WebSocket with Bearer auth, receives messages, and dispatches to platform-specific handlers.

## Architecture Details

### WebSocket Connection
```rust
let request = http::Request::builder()
    .uri(&self.url)
    .header("Authorization", format!("Bearer {}", self.token))
    .header("Host", extract_host(&self.url).unwrap_or("localhost"))
    .header("Connection", "Upgrade")
    .header("Upgrade", "websocket")
    .header("Sec-WebSocket-Version", "13")
    .header("Sec-WebSocket-Key", tokio_tungstenite::tungstenite::handshake::client::generate_key())
    .body(())?;

let (ws_stream, _) = connect_async(request).await?;
let (mut _write, mut read) = ws_stream.split();

while let Some(msg_result) = read.next().await {
    match msg_result {
        Ok(WsMessage::Text(text)) => self.process_event(&text),
        // Handle other message types...
    }
}
```

### Event Dispatch Pattern
```rust
let mut event_handlers: HashMap<MessageType, EventHandler> = HashMap::new();
event_handlers.insert(MessageType::Private, event::process_friend_message);
event_handlers.insert(MessageType::Group, event::process_group_message);
```

### Message Processing Flow
1. **Receive**: Raw WebSocket message (JSON string)
2. **Validate**: Check for `message_type` field
3. **Store**: Spawn async task to call `store.lock().await.store_message()` → Redis or memory
4. **Parse**: Deserialize to `RawMessageEvent` then convert to `MessageEvent` with typed message list
5. **Dispatch**: Route to handler based on `message_type`

## Key Patterns

### Initialization Sequence
```rust
pub async fn new(url: impl Into<String>, token: impl Into<String>, redis_url: Option<String>) -> Self {
    let mut event_handlers: HashMap<MessageType, EventHandler> = HashMap::new();
    event_handlers.insert(MessageType::Private, event::process_friend_message);
    event_handlers.insert(MessageType::Group, event::process_group_message);

    let redis_url = redis_url.or_else(|| env::var("REDIS_URL").ok());
    let message_store = Arc::new(TokioMutex::new(MessageStore::new(redis_url.as_deref()).await));
    
    Self { url: url.into(), token: token.into(), event_handlers, message_store }
}
```

**Why**: `MessageStore::new()` establishes Redis connection or fallback. Must run before any message processing.

### Error Handling
- Binary messages → Try UTF-8 decode, warn if invalid
- Missing `message_type` → Debug log + skip (non-message events)
- Parse errors → Error log + continue loop (prevents crash)

### Message Deserialization
```rust
let raw_event: RawMessageEvent = serde_json::from_value(message_json)?;

let event = MessageEvent {
    message_id: raw_event.message_id,
    message_type: raw_event.message_type,
    sender: raw_event.sender.clone(),
    message_list: raw_event.message.clone(),  // Already deserialized by serde
};
```
Serde deserializes JSON array to typed `Message` enum variants. Lenient parsing skips unsupported elements.

## Extension Points

### Add New Platform
1. **Handler**: Create function in `src/bot_adapter/event.rs`:
   ```rust
   pub fn process_web_message(event: &MessageEvent) {
       info!("Web message from: {}", event.sender.user_id);
       // Your implementation
   }
   ```

2. **Register**: Add to dispatch map in `BotAdapter::new()`:
   ```rust
   event_handlers.insert(MessageType::Web, event::process_web_message);
   ```

3. **Extend enum**: Add variant to `MessageType` in `src/bot_adapter/models/event_model.rs`:
   ```rust
   #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
   #[serde(rename_all = "lowercase")]
   pub enum MessageType {
       Private,
       Group,
       Web,  // New platform
   }
   ```

### Message Retrieval Pattern
Message storage exists but no retrieval pipeline is wired. To extend:
```rust
let store = self.message_store.lock().await;
if let Some(original_json) = store.get_message(&reply_msg_id).await {
    // Parse and process original message context
}
```

## Integration Points
- Inbound: WebSocket (config: `BOT_SERVER_URL`, `BOT_SERVER_TOKEN`)
- Outbound: `src/bot_adapter/event.rs` handlers
- Storage: `src/util/message_store.rs` (via `REDIS_URL` or in-memory)
- Config: `src/main.rs` loads `config.yaml` and constructs Redis URL with percent-encoded passwords
