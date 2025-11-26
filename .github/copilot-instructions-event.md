# Copilot Instructions: bot_adapter/event.py

## Purpose
Platform-specific event handlers. Currently implements logging-only handlers for QQ private/group messages.

## Current Implementation

### Handler Signature
```python
def process_friend_message(event: MessageEvent):
    logger.info(f"Sender: {event.sender.user_id}, Message: {[str(e) for e in event.message_list]}")

def process_group_message(event: MessageEvent):
    logger.info(f"Sender: {event.sender.user_id}, Message: {[str(e) for e in event.message_list]}")
```

### MessageEvent Structure
```python
class MessageEvent(BaseModel):
    message_id: int
    message_type: MessageType  # "private" or "group"
    sender: Sender             # user_id, nickname, card, role
    message_list: List[MessageBase]  # Typed message objects
```

## Extension Patterns

### Add Response Logic
```python
def process_group_message(event: MessageEvent):
    # 1. Log incoming message
    logger.info(f"Sender: {event.sender.user_id}, Message: {[str(e) for e in event.message_list]}")
    
    # 2. Extract text content
    text_messages = [msg.text for msg in event.message_list if isinstance(msg, PlainTextMessage)]
    full_text = "".join(text_messages)
    
    # 3. Check for mentions
    at_messages = [msg for msg in event.message_list if isinstance(msg, AtTargetMessage)]
    
    # 4. Retrieve context (if needed)
    from utils.message_store import get_message
    for reply_msg in [m for m in event.message_list if isinstance(m, ReplayMessage)]:
        original = get_message(reply_msg.message_id)
        # Process original message
    
    # 5. Generate and send response
    # TODO: Implement LLM call + response sending
```

### Handle Specific Message Types
```python
def process_group_message(event: MessageEvent):
    for msg in event.message_list:
        if isinstance(msg, AtTargetMessage):
            logger.info(f"Bot was mentioned by {event.sender.user_id}")
            # Trigger special behavior
        elif isinstance(msg, ReplayMessage):
            logger.info(f"Reply to message {msg.message_id}")
            # Load context from original message
        elif isinstance(msg, PlainTextMessage):
            # Process text content
            pass
```

### Add New Platform Handler
```python
def process_web_message(event: MessageEvent):
    """Handler for web platform messages."""
    logger.info(f"Web message from: {event.sender.user_id}")
    # Implement web-specific logic (e.g., HTML formatting, image rendering)
```

## Persistent Storage Integration

### Save to MySQL
```python
from database.db import SessionLocal
from database.models.message_record import MessageRecord
from datetime import datetime

def process_group_message(event: MessageEvent):
    # Log to console
    logger.info(f"Sender: {event.sender.user_id}, Message: {[str(e) for e in event.message_list]}")
    
    # Save to database for training/analytics
    session = SessionLocal()
    try:
        record = MessageRecord(
            message_id=str(event.message_id),
            sender_id=str(event.sender.user_id),
            sender_name=event.sender.nickname,
            send_time=datetime.now(),
            group_id="group_id_here",  # Extract from event
            group_name="group_name_here",
            content="".join([str(msg) for msg in event.message_list]),
            at_target_list=",".join([str(msg.target_id) for msg in event.message_list if isinstance(msg, AtTargetMessage)])
        )
        session.add(record)
        session.commit()
    except Exception as e:
        logger.error(f"Failed to save message: {e}")
        session.rollback()
    finally:
        session.close()
```

## Integration Points
- **Called by**: `BotAdapter.bot_event_process()` based on `message_type`
- **Uses**: `MessageEvent` from `models/event_model.py`
- **Accesses**: `utils/message_store.py` for message history
- **Could use**: `database/models/message_record.py` for persistent storage
