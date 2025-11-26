# Copilot Instructions: models/

## Purpose
Pydantic-based data models for type-safe event and message handling.

---

## bot_adapter/models/event_model.py

### Core Models

#### MessageType Enum
```python
class MessageType(str, Enum):
    PRIVATE = "private"  # 1-on-1 chat
    GROUP = "group"      # Group chat
```
Extensible for new platforms: Add `WEB = "web"`, `EDGE = "edge"`, etc.

#### Sender Model
```python
class Sender(BaseModel):
    user_id: int          # Unique user identifier
    nickname: str         # Display name
    card: str             # Group card/alias (may differ from nickname)
    role: Optional[str]   # Group role: "owner", "admin", "member", etc.
```

#### MessageEvent Model
```python
class MessageEvent(BaseModel):
    message_id: int                    # Unique message identifier
    message_type: MessageType          # Platform type
    sender: Sender                     # Who sent it
    message_list: List[MessageBase]    # Typed message components
```

**Usage**: This is the canonical event format passed to all handlers in `bot_adapter/event.py`.

---

## bot_adapter/models/message.py

### Message Type Hierarchy

All message types inherit from `MessageBase`:

```python
class MessageBase(BaseModel, ABC):
    @abstractmethod
    def __str__(self) -> str:
        """Human-readable representation"""
        pass

    @abstractmethod
    def get_type(self) -> str:
        """Type identifier for serialization"""
        pass
```

### Built-in Message Types

#### PlainTextMessage
```python
class PlainTextMessage(MessageBase):
    text: str
    
    def __str__(self) -> str:
        return self.text
    
    def get_type(self) -> str:
        return "text"
```
**Use case**: Standard text content.

#### AtTargetMessage
```python
class AtTargetMessage(MessageBase):
    target_id: int  # QQ ID of mentioned user
    
    def __str__(self) -> str:
        return f"@{self.target_id}"
    
    def get_type(self) -> str:
        return "at"
```
**Use case**: @ mentions. Check if bot is mentioned via `target_id`.

#### ReplayMessage
```python
class ReplayMessage(MessageBase):
    message_id: int                           # ID of original message
    message_source: Optional[MessageBase]     # Optionally populated with original content
    
    def __str__(self) -> str:
        if self.message_source:
            return f"[Replay of message ID {self.message_id}: {str(self.message_source)}]"
        else:
            return f"[Replay of message ID {self.message_id}]"
    
    def get_type(self) -> str:
        return "replay"
```
**Use case**: Reply to previous message. Retrieve original via `get_message(message_id)`.

---

## Deserialization: convert_message_from_json()

Maps raw QQ JSON to typed message objects:

```python
def convert_message_from_json(json_data: dict) -> MessageBase:
    message_type: str = json_data.get("type")      # "text", "at", "replay"
    message_data: dict = json_data.get("data")     # Type-specific payload
    
    if message_type == "text":
        return PlainTextMessage(text=message_data.get("text", ""))
    
    elif message_type == "at":
        target = message_data.get("target") or message_data.get("qq")  # Fallback key
        return AtTargetMessage(target_id=target or 0)
    
    elif message_type == "replay":
        return ReplayMessage(message_id=message_data.get("id", 0))
    
    else:
        raise ValueError(f"Unsupported message type: {message_type}")
```

**Note**: Handles key variations (e.g., `target` vs `qq` for @ mentions).

---

## Extension Pattern: Add New Message Type

### 1. Define Model
```python
class ImageMessage(MessageBase):
    url: str
    width: int
    height: int
    
    def __str__(self) -> str:
        return f"[Image: {self.url}]"
    
    def get_type(self) -> str:
        return "image"
```

### 2. Update Deserializer
```python
def convert_message_from_json(json_data: dict) -> MessageBase:
    message_type = json_data.get("type")
    message_data = json_data.get("data")
    
    # Existing types...
    
    elif message_type == "image":
        return ImageMessage(
            url=message_data.get("url"),
            width=message_data.get("width", 0),
            height=message_data.get("height", 0)
        )
    
    else:
        raise ValueError(f"Unsupported message type: {message_type}")
```

### 3. Handle in Event Processor
```python
def process_group_message(event: MessageEvent):
    for msg in event.message_list:
        if isinstance(msg, ImageMessage):
            logger.info(f"Received image: {msg.url}")
            # Download, process, or store image
```

---

## Integration Points
- **Created by**: `BotAdapter.bot_event_process()` during deserialization
- **Used by**: All event handlers in `bot_adapter/event.py`
- **Type safety**: Pydantic validates data at construction time
