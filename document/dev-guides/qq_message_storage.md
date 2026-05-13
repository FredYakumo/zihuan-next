# QQMessage Storage

This document explains how QQ messages are stored in Redis and MySQL in this project, what the current `message_record` schema looks like, and how reply / forward reconstruction works.

Relevant implementation:

- `storage_handler/src/message_store.rs`
- `database/models/message_record.py`
- `model_inference/src/agent/qq_chat_agent.rs`
- `zihuan_graph_engine/src/message_restore.rs`

## Stored Object

The storage layer does not persist a single `QQMessage` segment directly. Instead, it stores a flattened `MessageRecord` derived from a full QQ message:

```rust
pub struct MessageRecord {
    pub message_id: String,
    pub sender_id: String,
    pub sender_name: String,
    pub send_time: NaiveDateTime,
    pub group_id: Option<String>,
    pub group_name: Option<String>,
    pub content: String,
    pub at_target_list: Option<String>,
    pub media_json: Option<String>,
    pub raw_message_json: Option<String>,
}
```

This means:

- `message_id`: unique QQ platform message identifier
- `sender_id` / `sender_name`: sender information
- `send_time`: message timestamp
- `group_id` / `group_name`: group information for group messages, empty for private messages
- `content`: recursively rendered text content derived from the hydrated `Vec<QQMessage>`
- `at_target_list`: mentioned targets, currently stored as a string
- `media_json`: serialized direct media metadata, mainly for legacy compatibility
- `raw_message_json`: serialized hydrated `Vec<QQMessage>` tree for lossless reconstruction

In the current implementation:

- Redis still stores only normalized text
- MySQL stores normalized fields and, for new messages, the hydrated raw message tree

## Overall Flow

`MessageStore` uses a three-layer strategy:

1. Redis stores a fast cache of `message_id -> content`
2. MySQL stores the full `MessageRecord`
3. If either backend is unavailable, the store falls back to in-memory `HashMap`s

Main entry points:

- `store_message(message_id, message)`: writes to Redis, falls back to in-memory cache on failure
- `store_message_record(record)`: writes to MySQL, falls back to in-memory record buffer on failure
- `get_message_with_mysql(message_id)`: reads in Redis -> MySQL -> memory fallback order

## How Redis Stores Messages

Redis stores only the message text, not the full struct.

Write path:

```rust
conn.set(message_id, message).await
```

So the Redis layout is:

- key: `message_id`
- value: `content`

The current implementation does not add a prefix, bucket, hash structure, or TTL. It uses the message ID directly as the Redis key.

### Redis Startup Behavior

If Redis is configured, `MessageStore::new()` runs:

```rust
redis::cmd("FLUSHDB")
```

after a successful connection.

This means the selected Redis database is cleared on startup, and recent messages are later repopulated from MySQL if needed.

### Loading Messages Back Into Redis From MySQL

`load_messages_from_mysql(limit)` does the following:

1. Reads the most recent `limit` rows from `message_record` ordered by `send_time DESC`
2. Extracts `message_id` and `content`
3. Writes them back into Redis

In practice, Redis is used here as a hot cache rather than the source of truth.

## How MySQL Stores Messages

MySQL stores the full `MessageRecord`. The insert SQL is:

```sql
INSERT INTO message_record
(
    message_id,
    sender_id,
    sender_name,
    send_time,
    group_id,
    group_name,
    content,
    at_target_list,
    media_json,
    raw_message_json
)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
```

The main query patterns are:

- query one record by `message_id`
- query recent records by `sender_id`
- query recent records by `sender_id + group_id`

Ordering is consistently based on `send_time DESC`.

## MySQL Table Schema

The current ORM model is defined in `database/models/message_record.py`:

```python
class MessageRecord(Base):
    __tablename__ = 'message_record'

    id = Column(Integer, primary_key=True, autoincrement=True)
    message_id = Column(String(64), nullable=False)
    sender_id = Column(String(64), nullable=False)
    sender_name = Column(String(128), nullable=False)
    send_time = Column(DateTime, nullable=False)
    group_id = Column(String(64), nullable=True)
    group_name = Column(String(128), nullable=True)
    content = Column(String(2048), nullable=False)
    at_target_list = Column(String(512), nullable=True)
    media_json = Column(String(4096), nullable=True)
    raw_message_json = Column(String(65535), nullable=True)
```

Based on that model, the corresponding MySQL table can be written as:

```sql
CREATE TABLE message_record (
    id INT AUTO_INCREMENT PRIMARY KEY,
    message_id VARCHAR(64) NOT NULL,
    sender_id VARCHAR(64) NOT NULL,
    sender_name VARCHAR(128) NOT NULL,
    send_time DATETIME NOT NULL,
    group_id VARCHAR(64) NULL,
    group_name VARCHAR(128) NULL,
    content VARCHAR(2048) NOT NULL,
    at_target_list VARCHAR(512) NULL,
    media_json VARCHAR(4096) NULL,
    raw_message_json TEXT NULL
);
```

Important details:

- the current ORM model does not declare a unique index
- `message_id` is not the primary key; the primary key is the auto-increment `id`
- duplicate inserts for the same message are not automatically deduplicated by the current code

## What Gets Persisted For Reply / Forward Messages

For new messages handled by the QQ chat agent:

- `content` is generated from recursive readable rendering, not only `to_string()`
- hydrated forward contents contribute their expanded text into `content`
- nested forwards remain visible in `content`
- `raw_message_json` stores the full hydrated `Vec<Message>` tree

This is the key difference from the older behavior where a forwarded message might be flattened into only:

```text
[Forward of message ID 123456]
```

and later become impossible to reconstruct accurately.

## Fallback And Reconnection

### When Redis Is Unavailable

- `store_message()` switches to the in-memory `memory_store`
- a background Redis reconnect task is scheduled
- once Redis reconnects, in-memory `message_id -> content` entries are migrated back into Redis
- after migration, the in-memory cache is cleared

### When MySQL Is Unavailable

- `store_message_record()` switches to the in-memory `mysql_memory_store`
- a background MySQL reconnect task is scheduled
- once MySQL reconnects, buffered `MessageRecord`s are inserted into `message_record`
- after migration, the in-memory record buffer is cleared

## Read Order

If the caller only wants cached message text:

```rust
get_message(message_id)
```

The read order is:

1. Redis
2. Redis fallback in-memory cache

If the caller wants MySQL fallback as well:

```rust
get_message_with_mysql(message_id)
```

The read order is:

1. Redis
2. MySQL `message_record.content`
3. MySQL fallback in-memory record buffer
4. Redis fallback in-memory cache

## Restore Strategy For Referenced Messages

`zihuan_graph_engine::message_restore::restore_message_snapshot()` now restores messages in this order:

1. runtime in-memory snapshot cache
2. MySQL `raw_message_json`
3. legacy fallback reconstruction from `content + media_json`

This matters for reply hydration:

- when a user replies to a previously stored forward message, the adapter can reconstruct the full nested message tree from `raw_message_json`
- nested forwards therefore survive persistence for new rows
- older rows without `raw_message_json` still fall back to legacy best-effort reconstruction

## Current Boundaries

There are several important boundaries in the current implementation:

- Redis stores only `content`, not the full `QQMessage` JSON
- MySQL now stores normalized fields plus `raw_message_json` for new messages
- `at_target_list` is currently stored as a plain string, not as a JSON column or relation table
- startup runs `FLUSHDB`, so Redis is not treated as long-term storage
- `message_record` currently has no declared index or unique constraint in the ORM model

Current remaining limits:

- Redis alone is still insufficient for full message reconstruction
- old MySQL rows that predate `raw_message_json` can only be restored approximately from normalized text and media metadata
- `media_json` is not a full recursive forward tree; it is mainly a legacy compatibility layer for direct media reconstruction

## Practical Consequences

For new incoming messages:

- referenced messages can be restored with their hydrated reply / forward structure
- nested forwards can survive both runtime caching and MySQL persistence
- QQ-agent inference can expand reply and forward messages from the restored tree before calling the brain / LLM

For old rows written before `raw_message_json` existed:

- a reply to an old forward message may still degrade to placeholder-style reconstruction
- image restoration may still work partially through `media_json`
- deep nested forward structure cannot be guaranteed
