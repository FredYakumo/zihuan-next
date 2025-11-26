# Copilot Instructions: bot_adapter/adapter.py

## Purpose
Core event processing engine. Connects to QQ bot server via WebSocket, receives messages, and dispatches to platform-specific handlers.

## Architecture Details

### WebSocket Connection
```python
# Connects with Bearer token authentication
async with websockets.connect(
    self.url, 
    additional_headers={"Authorization": f"Bearer {self.token}"}
) as websocket:
    message = await websocket.recv()
    self.bot_event_process(message)
```

### Event Dispatch Pattern
```python
self.event_process_func = {
    "private": event.process_friend_message,  # 1-on-1 chats
    "group": event.process_group_message       # Group chats
}
```

### Message Processing Flow
1. **Receive**: Raw WebSocket message (JSON string)
2. **Validate**: Check for `message_type` field
3. **Store**: Call `store_message(message_id, message)` → Redis or memory
4. **Parse**: Convert to `MessageEvent` with typed message list
5. **Dispatch**: Route to handler based on `message_type`

## Key Patterns

### Initialization Sequence
```python
def __init__(self, url: str, token: str):
    self.url = url
    self.token = token
    self.event_process_func = {...}  # Register handlers
    init_message_store(config, logger)  # CRITICAL: Must be first
```

**Why**: `init_message_store()` establishes Redis connection or fallback. Must run before any message processing.

### Error Handling
- Non-string messages → Warning + ignore
- Missing `message_type` → Debug log + skip (non-message events)
- Parse errors → Error log + continue loop (prevents crash)

### Message Deserialization
```python
message_list=[
    convert_message_from_json(message) 
    for message in message_json.get("message", [])
]
```
Converts raw JSON array to typed `MessageBase` subclasses (`PlainTextMessage`, `AtTargetMessage`, etc.).

## Extension Points

### Add New Platform
1. **Handler**: Create function in `bot_adapter/event.py`:
   ```python
   def process_web_message(event: MessageEvent):
       # Your implementation
   ```

2. **Register**: Add to dispatch dict:
   ```python
   self.event_process_func = {
       "private": event.process_friend_message,
       "group": event.process_group_message,
       "web": event.process_web_message  # New platform
   }
   ```

3. **MessageEvent**: If needed, extend `MessageType` enum in `models/event_model.py`

### Hybrid RAG Implementation
Current code stores messages but doesn't implement RAG retrieval. To add:
- Use `get_message(message_id)` from `utils/message_store.py` for chat history
- Integrate Weaviate client for vector search (dependency already in `pyproject.toml`)
- Combine results before passing to LLM

## Integration Points
- **Inbound**: Receives events from external QQ bot server (configured via `BOT_SERVER_URL`)
- **Outbound**: Calls event handlers in `bot_adapter/event.py`
- **Storage**: Uses `utils/message_store.py` for caching
- **Config**: Reads `config.BOT_SERVER_URL` and `config.BOT_SERVER_TOKEN`
