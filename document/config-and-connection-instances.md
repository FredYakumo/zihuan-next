# Configuration And Connection Instances

This document explains the difference between a **connection configuration** and a **runtime connection instance** in `zihuan-next`.

## Core Idea

The system now distinguishes between two layers:

- **configuration**
- **runtime instance**

They are related, but they are not the same thing.

## 1. Connection Configuration

A connection configuration is a persistent record stored in system config.

Examples:

- a MySQL connection definition
- a RustFS object storage definition
- a Weaviate definition
- an IMS BotAdapter definition

These records are managed in the admin UI under:

- `连接配置`

They are saved into the `connections` section of:

- Windows: `%APPDATA%/zihuan-next_aibot/system_config/system_config.json`
- Linux/macOS: `$XDG_CONFIG_HOME` or `$HOME/.config/zihuan-next_aibot/system_config/system_config.json`

The on-disk shape is now unified under one root object:

```json
{
  "version": 2,
  "configs": {
    "connections": [],
    "llm_refs": [],
    "agents": []
  }
}
```

Legacy top-level `connections` / `llm_refs` / `agents` sections are migrated on read and only the unified structure is written back.

Each configuration has a stable identifier:

- `config_id`

The public primary key is now:

- `config_id`

Legacy internal `id` values are kept only as a migration-time compatibility field.

In the current implementation, `connections`, `llm_refs`, and `agents` are all routed through the unified configuration center:

- `ConfigCenter` loads and saves the user config file
- `config_id` is the shared primary key
- legacy top-level structures are migrated automatically on read

In practice this means:

- connection configs use `config_id`
- model configs use `config_id`
- agent configs also use `config_id`

## 2. Runtime Connection Instance

A runtime connection instance is a live in-memory connection created from a configuration.

Examples:

- a live MySQL pool
- a live RustFS/S3 client
- a live Weaviate client
- a live IMS BotAdapter session

Runtime instances are:

- created on demand
- reused when possible
- not stored as persistent config
- identified by `instance_id`

Each runtime instance points back to its source configuration through:

- `config_id`

## 3. Why The System Uses Two Layers

This split makes the system safer and easier to operate.

Configuration layer responsibilities:

- define connection parameters
- enable or disable a connection
- give the connection a stable name and ID

Runtime instance layer responsibilities:

- create actual live clients only when needed
- reuse healthy live connections
- expose current activity in the connection manager UI
- allow force-close without deleting the underlying configuration

## 4. How Graph Nodes Use Them

Graph nodes do **not** store runtime instance IDs.

They store:

- `config_id`

At execution time, the node asks the corresponding runtime connection manager:

1. Find a healthy instance for this `config_id`
2. Reuse it if available
3. Create a new instance if none is available

If the configuration:

- does not exist
- is disabled

execution fails with an error.

For backward compatibility, older graph JSON may still contain:

- `connection_id`

During graph loading, it is migrated to:

- `config_id`

## 5. Connection Management And Auto-Reconnect

Not every connection type is managed in exactly the same way.

### RuntimeStorageConnectionManager

For storage-backed runtime instances such as:

- MySQL
- RustFS / S3
- Weaviate

the main runtime owner is:

- `storage_handler::RuntimeStorageConnectionManager`

Its job is to:

- load enabled connection definitions from system config
- create live runtime handles on demand
- reuse existing healthy handles for the same `config_id`
- destroy idle or disabled instances during cleanup
- expose active instances to the connection manager UI

For MySQL specifically, this means:

- creating and caching a `sqlx::MySqlPool`
- keeping a Tokio runtime handle when one is needed for pool background work
- returning `Arc<MySqlConfig>` to graph nodes, services, or APIs

This is instance-level lifecycle management, not per-query retry logic.

### Redis Connection Ownership

Redis currently uses a different pattern.

The saved Redis configuration is still a normal connection config, but the live connection used by callers is stored inside:

- `zihuan_graph_engine::data_value::RedisConfig`

That runtime ref carries:

- the resolved Redis URL
- `redis_cm`, the cached live Redis connection
- `cached_redis_url`, the URL used to build the cached connection

Shared Redis operations are centralized in:

- `storage_handler::redis`

Current helper behavior is:

1. ensure a cached connection exists
2. run the Redis command
3. if the command fails, mark the cached connection invalid
4. reconnect once
5. retry the command once

This is the current auto-reconnect behavior used by service/business code such as the QQ agent inbox path.

The important boundary is:

- connection creation, invalidation, and reconnect belong to `storage_handler`
- service/business modules may decide fallback behavior, such as degrading from Redis to memory
- service/business modules should not duplicate low-level `redis_cm` lifecycle logic

### What "Auto-Reconnect" Means Today

In the current codebase, auto-reconnect does not mean a universal background self-healing loop for every backend.

Instead, it means:

- MySQL / RustFS / Weaviate: runtime instance reuse and recreation are owned by `RuntimeStorageConnectionManager`
- Redis: reconnect is attempted by shared Redis helpers when an operation detects a failed connection
- higher-level modules may still choose fallback behavior after reconnect fails

So the system separates:

- runtime instance ownership
- low-level reconnect policy
- business-level degradation policy

## 6. Connection Manager UI

The admin UI now has a dedicated page:

- `连接管理器`

This page shows current runtime instances, not saved configurations.

It displays:

- connection name
- `config_id`
- `instance_id`
- start time
- duration
- keep-alive flag
- heartbeat interval
- status
- force close action

The current UI also applies two presentation rules:

- long IDs are shortened in cards/tables, for example `abcd1234...`
- if one configuration currently owns multiple runtime instances, the UI may render it as something like `abcd1234..., and 3 total`

Use this page when you want to inspect or terminate live connections.

Use `连接配置` when you want to create, edit, enable, or disable configurations.

At the moment, this UI is primarily driven by runtime managers such as `RuntimeStorageConnectionManager` and long-lived adapter managers. Redis helper-managed cached connections are not surfaced there as first-class rows.

## 7. Keep-Alive And Heartbeat

Runtime instances may expose two runtime-only behaviors:

- `keep_alive`
- `heartbeat_interval_secs`

These are **not** user-managed configuration fields.

They are assigned in code by the runtime manager.

Current behavior:

- storage-backed runtime instances such as MySQL, RustFS, and Weaviate are not keep-alive by default
- IMS BotAdapter runtime instances are keep-alive
- IMS BotAdapter runtime instances send a heartbeat periodically

If `keep_alive = true`, the instance is not automatically closed by idle cleanup.

If `heartbeat_interval_secs` is set, the manager periodically sends a lightweight action to verify the connection is still responsive.

## 8. Force Close Behavior

Force closing a runtime instance:

- closes the live instance
- removes it from the runtime manager
- does not delete the saved configuration

The next time a node or agent needs that `config_id`, the manager can create a fresh instance again.

You will typically also see logs for:

- successful config loads with `config_id`
- successful instance creation with `instance_id` and `config_id`
- idle instance cleanup with `instance_id` and `config_id`
- user-triggered force close with `instance_id` and `config_id`

## 9. Practical Example

Suppose you create one saved configuration:

- name: `Main Weaviate`
- `config_id`: `abc123`

Later:

- a graph node uses `config_id = abc123`
- the runtime manager creates a live Weaviate client
- that live client gets its own `instance_id`

If you open the connection manager page, you will see the runtime instance.

If you disable the configuration in `连接配置`, future use of `abc123` fails, and existing runtime instances for that configuration are cleaned up.
