# QQMessage Storage

This document describes the current QQ message storage and MySQL retrieval paths.

The canonical runtime table is `message_record`. MySQL-backed storage is optional; it is only active when a graph or service path provides a `MySqlRef`.

## Relevant Implementation

Connection and runtime handles:

- `zihuan_core/src/data_refs.rs` defines `MySqlConfig`
- `zihuan_graph_engine/src/data_value.rs` defines `DataType::MySqlRef` and `DataValue::MySqlRef`
- `storage_handler/src/mysql.rs` defines the current system-config backed `MySqlNode`
- `storage_handler/src/connection_manager.rs` creates and caches live MySQL pools
- `storage_handler/src/resource_resolver.rs` resolves MySQL connections for APIs and graph bindings

Message persistence and retrieval:

- `zihuan_graph_engine/src/message_persistence.rs` persists a `MessageEvent`
- `zihuan_graph_engine/src/qq_message_list_mysql_persistence.rs` persists a caller-supplied `Vec<QQMessage>`
- `zihuan_graph_engine/src/message_mysql_history_common.rs` contains shared MySQL query helpers
- `zihuan_graph_engine/src/message_mysql_get_user_history.rs` reads recent messages for one sender
- `zihuan_graph_engine/src/message_mysql_get_group_history.rs` reads recent messages for one group
- `zihuan_graph_engine/src/message_mysql_search.rs` searches `message_record`
- `zihuan_graph_engine/src/message_restore.rs` restores referenced messages from runtime cache or MySQL
- `src/api/explorer.rs` exposes the admin resource explorer MySQL query endpoint
- `storage_handler/src/message_store.rs` contains a Redis/MySQL helper store that is not the primary graph-node path

Schema references:

- `database/models/message_record.py`
- `migrations/versions/6d101e418d9b_add_message_record_table.py`
- `migrations/versions/e8c7d6f2b123_make_at_target_nullable.py`
- `migrations/versions/4f2a8c1d9e3b_add_media_json_to_message_record.py`
- `migrations/versions/9b7f4c2d1a6e_add_raw_message_json_to_message_record.py`

## MySQL Connection Chain

The current preferred path uses saved system connection configs:

1. A MySQL connection is saved in the system config `connections` collection.
2. The graph uses the `mysql` node from `storage_handler/src/mysql.rs`.
3. `MySqlNode` stores the selected `config_id` in inline config.
4. During `execute`, `MySqlNode` calls:

```rust
RuntimeStorageConnectionManager::shared().get_or_create_mysql_ref(config_id)
```

5. `RuntimeStorageConnectionManager` loads the connection definition, creates a `sqlx::MySqlPool`, keeps a Tokio runtime for pool background work, and returns `Arc<MySqlConfig>`.
6. `MySqlNode` outputs `DataValue::MySqlRef(config)`.
7. Downstream storage/search nodes read the `mysql_ref` input and query through the pool.

## Stored Object

The storage layer does not persist one `QQMessage` segment directly. It stores normalized message metadata plus rendered content and optional serialized message trees:

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

Field meaning:

- `message_id`: platform message ID
- `sender_id` / `sender_name`: sender identity
- `send_time`: persistence timestamp
- `group_id` / `group_name`: group context for group messages
- `content`: rendered readable message text
- `at_target_list`: comma-separated mentioned targets
- `media_json`: serialized direct media metadata, mainly for fallback reconstruction
- `raw_message_json`: serialized `Vec<Message>` tree when the writer has a full `MessageEvent`

## MySQL Table Schema

The current ORM model is:

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

Important constraints:

- `id` is the primary key.
- `message_id` is not declared unique.
- The ORM model does not declare indexes.
- Long content is split into multiple rows with the same `message_id`; consumers that need a complete message must aggregate chunks ordered by `id`.

## Write Paths

### MessageEvent Persistence

`zihuan_graph_engine::message_persistence::persist_message_event(...)` is the generic event persistence entry point.

It does the following:

1. Stores the event in the in-process restore cache through `cache_message_snapshot(event)`.
2. Optionally writes `message_id -> content` to Redis if a Redis ref is provided or globally registered.
3. Optionally writes the event to MySQL if a MySQL ref is provided or globally registered.

The MySQL insert path is `persist_message_to_mysql(...)`.

For MySQL it stores:

- normalized metadata
- readable `content`, split by `CONTENT_MAX_CHARS`
- `at_target_list` only on the first chunk
- `media_json` only on the first chunk
- `raw_message_json` only on the first chunk

Current direct callers include outbound message helpers in `ims_bot_adapter/src/message_helpers.rs`, such as:

- `send_friend_text_with_persistence`
- `send_group_text_with_persistence`
- `send_friend_batches_with_persistence`
- `send_group_batches_with_persistence`

Incoming adapter events are not automatically inserted by `ims_bot_adapter/src/event.rs`; they are dispatched to registered handlers and the Brain agent. Persist incoming messages explicitly in a graph or service path when MySQL history is required.

### QQMessage List Node Persistence

`QQMessageListMySQLPersistenceNode` stores a caller-supplied `Vec<QQMessage>` plus required metadata.

Inputs:

- `qq_message_list`
- `message_id`
- `sender_id`
- `sender_name`
- optional `group_id`
- optional `group_name`
- `mysql_ref`

This node writes normalized metadata, rendered `content`, `at_target_list`, and `media_json`.

It does not currently write `raw_message_json`, because it does not receive a full `MessageEvent` and uses a separate insert SQL shape.

## Read Paths

### Graph Nodes

The message history graph nodes all take `mysql_ref` and call `run_mysql_query(...)`:

- `message_mysql_get_user_history`
- `message_mysql_get_group_history`
- `message_mysql_search`

The shared helper:

- applies a 30 second query timeout
- runs queries on the `MySqlConfig.runtime_handle` when present
- aggregates chunked rows by `message_id`
- formats messages as readable strings

Query patterns:

- user history: `sender_id = ?`
- user history in a group: `sender_id = ? AND group_id = ?`
- group history: `group_id = ?`
- search: optional `sender_id`, `group_id`, content `LIKE`, time range, sort direction, and limit

### Reply Snapshot Restore

`restore_message_snapshot(message_id)` restores referenced messages in this order:

1. in-process runtime cache
2. MySQL `raw_message_json`
3. fallback reconstruction from concatenated `content` plus `media_json`

`ims_bot_adapter/src/adapter.rs` uses this when hydrating `reply.message_source`.

The MySQL lookup reads:

```sql
SELECT content, media_json, raw_message_json
FROM message_record
WHERE message_id = ?
ORDER BY id ASC
```

This is why chunk order and first-chunk metadata matter.

### Admin Resource Explorer

`src/api/explorer.rs::query_mysql` powers the MySQL resource explorer endpoint.

It resolves a `connection_id` through `storage_handler::resource_resolver::build_mysql_ref(...)`, then queries `message_record` with optional filters for:

- `message_id`
- `sender_id`
- `sender_name`
- `group_id`
- `content`
- `send_time_start`
- `send_time_end`

The response is paginated and truncates displayed content to a preview.

### MessageStore Helper

`storage_handler::MessageStore` is a helper around:

- Redis string cache
- MySQL `message_record`
- in-memory fallback maps

Available methods include:

- `load_messages_from_mysql(limit)`
- `get_messages_by_sender(sender_id, group_id, limit)`
- `store_message(message_id, message)`
- `store_message_record(record)`
- `get_message_record(message_id)`
- `get_message(message_id)`
- `get_message_with_mysql(message_id)`

This helper does not currently drive the main graph-node persistence path. Prefer the graph nodes and `message_persistence` functions when adding new message storage behavior.

## Registration

Storage-related nodes are registered from `storage_handler::init_node_registry()`:

- `mysql`
- `qq_message_list_mysql_persistence`
- `message_mysql_get_user_history`
- `message_mysql_get_group_history`
- `message_mysql_search`

`storage_handler` extends the base graph registry through the main registry bootstrap path described in `document/dev-guides/node-system.md`.

## Practical Notes

- Use the system-config backed `mysql` node for new graphs.
- Pass `mysql_ref` explicitly into history/search/persistence nodes.
- For reply/forward reconstruction, prefer write paths that persist `raw_message_json`.
- `QQMessageListMySQLPersistenceNode` is useful for simple graph-level persistence, but it cannot restore full nested structures as accurately as `MessageEvent` persistence.
- The current schema allows duplicate `message_id` rows and also uses duplicate `message_id` for chunks. Do not add deduplication logic without first deciding how chunked messages should be represented.
- Redis is a text cache, not the source of truth for full QQ message reconstruction.
