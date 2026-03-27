# QQMessage Storage

This document explains how QQ messages are stored in Redis and MySQL in this project, and what the current MySQL schema looks like.

Relevant implementation:

- `src/util/message_store.rs`
- `database/models/message_record.py`

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
}
```

This means:

- `message_id`: unique QQ platform message identifier
- `sender_id` / `sender_name`: sender information
- `send_time`: message timestamp
- `group_id` / `group_name`: group information for group messages, empty for private messages
- `content`: aggregated text content derived from the original `Vec<QQMessage>`
- `at_target_list`: mentioned targets, currently stored as a string

So in the current implementation, `QQMessage` is not stored as the original JSON array. It is first normalized into a searchable record and then written to Redis and MySQL.

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

### Loading Messages Back into Redis from MySQL

`load_messages_from_mysql(limit)` does the following:

1. Reads the most recent `limit` rows from `message_record` ordered by `send_time DESC`
2. Extracts `message_id` and `content`
3. Writes them back into Redis

In practice, Redis is used here as a hot cache rather than the source of truth.

## How MySQL Stores Messages

MySQL stores the full `MessageRecord`. The insert SQL is fixed as:

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
    at_target_list
)
VALUES (?, ?, ?, ?, ?, ?, ?, ?)
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
    at_target_list VARCHAR(512) NULL
);
```

Important details:

- the current ORM model does not declare a unique index
- `message_id` is not the primary key; the primary key is the auto-increment `id`
- duplicate inserts for the same message are not automatically deduplicated by the current code

## Fallback and Reconnection

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

## Current Boundaries

There are several important boundaries in the current implementation:

- Redis stores only `content`, not the full `QQMessage` JSON
- MySQL also does not store the original `Vec<QQMessage>` structure; it stores normalized record fields
- `at_target_list` is currently stored as a plain string, not as a JSON column or relation table
- startup runs `FLUSHDB`, so Redis is not treated as long-term storage
- `message_record` currently has no declared index or unique constraint in the ORM model

If the project later needs full reconstruction of the original QQ message segments, it will need to store the raw `Vec<QQMessage>` JSON in addition to the current normalized fields.
