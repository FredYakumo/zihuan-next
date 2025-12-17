# Copilot Instructions: src/bot_adapter/models/

## Purpose
Serde-based data models for type-safe event and message handling.

---

## src/bot_adapter/models/event_model.rs

### Core Models

#### MessageType Enum
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageType {
    Private,  // 1-on-1 chat
    Group,    // Group chat
}
```
Extensible for new platforms: Add `Web`, `Edge`, etc.

#### Sender Model
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sender {
    pub user_id: i64,
    pub nickname: String,
    #[serde(default)]
    pub card: String,           // Group card/alias (may differ from nickname)
    pub role: Option<String>,   // Group role: "owner", "admin", "member", etc.
}
```
Serde deserializes from JSON automatically. `#[serde(default)]` on `card` allows missing field.

#### MessageEvent Model
```rust
#[derive(Debug, Clone)]
pub struct MessageEvent {
    pub message_id: i64,
    pub message_type: MessageType,
    pub sender: Sender,
    pub message_list: Vec<Message>,  // Typed message enum variants
}
```

**Usage**: This is the canonical event format passed to all handlers in `src/bot_adapter/event.rs`.

---

## src/bot_adapter/models/message.rs

### Message Enum Hierarchy

All message types are variants of the `Message` enum:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "lowercase")]
pub enum Message {
    #[serde(rename = "text")]
    PlainText(PlainTextMessage),
    
    #[serde(rename = "at")]
    At(AtTargetMessage),
    
    #[serde(rename = "replay")]
    Replay(ReplayMessage),
}
```

### Built-in Message Types

#### PlainTextMessage
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlainTextMessage {
    pub text: String,
}
```
**Use case**: Standard text content.

#### AtTargetMessage
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtTargetMessage {
    #[serde(alias = "target", alias = "qq")]
    pub target_id: i64,  // QQ ID of mentioned user
}
```
**Use case**: @ mentions. Check if bot is mentioned via `target_id`. Handles both `target` and `qq` field names.

#### ReplayMessage
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayMessage {
    #[serde(alias = "id")]
    pub message_id: i64,  // ID of original message
}
```
**Use case**: Reply to previous message. Retrieve original via `MessageStore::get_message()`.

---

## Deserialization Pattern

Serde handles deserialization automatically via tagged enum format:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "lowercase")]
pub enum Message {
    #[serde(rename = "text")]
    PlainText(PlainTextMessage),
    
    #[serde(rename = "at")]
    At(AtTargetMessage),
    
    #[serde(rename = "replay")]
    Replay(ReplayMessage),
}
```

**JSON structure**: `{"type": "text|at|replay", "data": {...}}`

### Lenient Array Deserialization

To handle unsupported message types gracefully:

```rust
fn deserialize_message_vec_lenient<'de, D>(deserializer: D) -> Result<Vec<Message>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw_values = Vec::<serde_json::Value>::deserialize(deserializer)?;
    
    let mut out = Vec::with_capacity(raw_values.len());
    for v in raw_values {
        match serde_json::from_value::<Message>(v) {
            Ok(m) => out.push(m),
            Err(e) => {
                warn!("Skipping unsupported message element: {}", e);
            }
        }
    }
    Ok(out)
}
```

**Benefit**: Skips unsupported message elements instead of failing the entire event parsing.

---

## Extension Pattern: Add New Message Type

### 1. Define Struct
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageMessage {
    pub url: String,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
}
```

### 2. Add Variant to Enum
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "lowercase")]
pub enum Message {
    #[serde(rename = "text")]
    PlainText(PlainTextMessage),
    
    #[serde(rename = "at")]
    At(AtTargetMessage),
    
    #[serde(rename = "replay")]
    Replay(ReplayMessage),
    
    #[serde(rename = "image")]
    Image(ImageMessage),  // New message type
}
```

### 3. Handle in Event Processor
```rust
pub fn process_group_message(event: &MessageEvent) {
    for msg in &event.message_list {
        match msg {
            Message::Image(img_msg) => {
                info!("Received image: {}", img_msg.url);
                // Download, process, or store image
            }
            // ... other cases
        }
    }
}
```

---

## Integration Points
- **Created by**: `BotAdapter::process_event()` during serde deserialization
- **Used by**: All event handlers in `src/bot_adapter/event.rs`
- **Type safety**: Serde validates data at deserialization time; lenient parser skips unsupported elements
