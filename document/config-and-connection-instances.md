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

Each configuration has a stable identifier:

- `config_id`

For compatibility, the current implementation still stores this canonical ID in:

- `ConnectionConfig.id`

The API may return both:

- `id`
- `config_id`

In practice, treat them as the same configuration key.

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

## 5. Connection Manager UI

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

Use this page when you want to inspect or terminate live connections.

Use `连接配置` when you want to create, edit, enable, or disable configurations.

## 6. Keep-Alive And Heartbeat

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

## 7. Force Close Behavior

Force closing a runtime instance:

- closes the live instance
- removes it from the runtime manager
- does not delete the saved configuration

The next time a node or agent needs that `config_id`, the manager can create a fresh instance again.

## 8. Practical Example

Suppose you create one saved configuration:

- name: `Main Weaviate`
- `config_id`: `abc123`

Later:

- a graph node uses `config_id = abc123`
- the runtime manager creates a live Weaviate client
- that live client gets its own `instance_id`

If you open the connection manager page, you will see the runtime instance.

If you disable the configuration in `连接配置`, future use of `abc123` fails, and existing runtime instances for that configuration are cleaned up.

## 9. Short Version

- `config_id` identifies a saved connection definition
- `instance_id` identifies a live runtime connection
- graph nodes bind to `config_id`
- runtime managers create and reuse instances
- the connection manager page shows live instances
- the connection config page manages persistent definitions
