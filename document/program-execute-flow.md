# Program Execution Flow

This document describes how the current application starts and runs, both for the web app and for the CLI executor.

## 1. Web Application Startup

Entry point: `src/main.rs`

Startup order:

1. Initialize the global logger through `src/log_forwarder.rs`
2. Initialize the node registry via `init_node_registry()` in `src/init_registry.rs`
3. Parse `--host` and `--port`
4. Create `AppState`
5. Create the WebSocket broadcast channel
6. Attach log forwarding to app state and broadcast
7. Load system config sections
8. Auto-start enabled agents marked `auto_start = true`
9. Build the Salvo router
10. Bind the TCP listener and serve HTTP/WebSocket traffic

## 2. What The Web Application Hosts

The main binary hosts all of these concerns together:

- admin UI at `/`
- graph editor at `/editor`
- REST API under `/api`
- WebSocket endpoint at `/api/ws`
- graph execution tasks
- agent lifecycle management
- system config persistence
- log forwarding to files, console, and WebSocket clients

## 3. Request And UI Flow

### Browser UI

- `/` loads the Vue 3 admin application
- `/editor` loads the browser graph editor

### API

Important route groups:

- `/api/system/connections`
- `/api/system/llm-refs`
- `/api/system/agents`
- `/api/graphs`
- `/api/tasks`
- `/api/themes`
- `/api/workflow_set`

### WebSocket

`/api/ws` broadcasts server-originated events such as:

- task started
- task finished
- task stopped
- log message
- graph validation result
- QQ message preview payloads

## 4. Graph Execution From The Web App

When a graph is executed from the web app:

1. The API reads the graph session from `AppState`
2. A task entry is created
3. Execution preparation resolves runtime context and runtime inline values
4. The graph is built through `zihuan_graph_engine::registry::build_node_graph_from_definition`
5. Execution runs inside `spawn_blocking`
6. Task-scoped logs are captured through `log_forwarder::scope_task(...)`
7. WebSocket messages are emitted for task lifecycle and preview updates
8. The task finishes as `success`, `failed`, or `stopped`

The graph runtime itself is synchronous; the web layer uses background task orchestration around it.

## 5. Agent Startup And Lifecycle

At process startup, the web app loads agent definitions from system config and automatically starts agents that are both:

- `enabled`
- `auto_start`

Agent start/stop can also be triggered through `/api/system/agents/<id>/start` and `/api/system/agents/<id>/stop`.

Current long-lived agent types are defined in `model_inference::system_config::AgentType`:

- `qq_chat`
- `http_stream`

The long-lived runtime is hosted by `zihuan_service`, not by the graph executor.

The current task model is important:

- starting an agent no longer creates a task entry
- task-list agent entries represent one concrete handled response/request, not agent uptime
- `qq_chat_agent` creates a task when it actually starts a reply flow, for example `回复[3507578481]的消息`
- `http_stream_agent` creates one task per handled HTTP request
- pure ignore/filter paths such as a group message that does not mention the bot do not create tasks

Each agent response task has its own:

- `task_id`
- `start_time`
- `end_time`
- `duration_ms`
- `status`
- `error_message`
- `result_summary`
- `log_path`

Task logs are persisted per task under:

- `logs/tasks/<task_id>.jsonl`

`qq_chat_agent` task logs also include per-response details such as:

- raw user message text
- expanded inference message text
- context message count
- estimated context token totals
- history compaction token before/after
- current request token usage information

For the dedicated QQ chat agent logging design document, see
[`dev-guides/qq-chat-agent-logging.md`](dev-guides/qq-chat-agent-logging.md).

The current LLM abstraction does not expose a unified exact usage object yet. When exact `prompt_tokens` / `completion_tokens` / `total_tokens` are unavailable, logs explicitly mark them unavailable and include estimates instead.

### Current QQ Chat Agent Message Handling Model

`qq_chat_agent` now uses a hybrid model: asynchronous ingress at the adapter boundary and inbox-driven synchronous execution in the business layer.

The message path is:

1. `ims_bot_adapter::adapter::BotAdapter::start()` keeps reading WebSocket messages.
2. Each incoming text/binary frame is dispatched via `tokio::spawn(...)` into its own `BotAdapter::process_event(...)` task.
3. `process_event(...)` parses JSON, enriches images, hydrates reply/forward message segments, then `tokio::spawn(...)` calls `ims_bot_adapter::event::process_message(...)`.
4. `process_message(...)` clones the registered event handlers and `await`s them one by one for the same event.
5. The handler registered by `qq_chat_agent` builds an inbox item, tries to enqueue it into Redis first through shared `storage_handler::redis` helpers, and falls back to an in-memory queue when Redis is unavailable.
6. Background inbox consumers dequeue Redis or in-memory items and dispatch them through `tokio::task::spawn_blocking(...)`.
7. The actual business logic still runs inside `QqChatAgentService::handle_event(...)`.

The concurrency semantics of the current model are:

- **Different inbound messages**: already concurrent at the adapter layer, because each message is spawned into its own task before `process_event(...)` and `process_message(...)`.
- **Multiple handlers for the same message**: still serial within `process_message(...)`.
- **Messages from different users**: allowed to run concurrently, and global ordering is not preserved.
- **Messages from the same user**: serialized by the session claim/release mechanism inside `qq_chat_agent_core`, not by the adapter queue.
- **Redis unavailable at enqueue time**: the handler first relies on `storage_handler` to invalidate the failed Redis connection and reconnect once, then falls back to the local in-memory queue and still returns quickly if Redis remains unavailable.
- **Process restart while using the in-memory fallback**: accepted to lose queued in-memory work that has not started yet.

The current single-user serialization points are:

- `try_claim_session(...)`
- `release_session(...)`

They wrap `SessionStateRef::try_claim(...)` / `release(...)` so one sender does not enter multiple active reply flows at the same time.

### Current QQ Chat Agent Steer Model

While a sender already has an active QQ chat reply flow, additional messages from the same sender
are treated as **steer** messages instead of causing a busy reply.

The detailed runtime behavior, merge rules, history persistence, and `QqChatAgentConfig.max_steer_count`
limit are documented in
[`dev-guides/qq-chat-agent-steer.md`](dev-guides/qq-chat-agent-steer.md).

At the execution-flow level, the important point is only that same-user overlap stays attached to the
current sender flow first, and may either be injected into a later Brain round or become the next
automatic follow-up turn.

The architectural boundary is therefore:

- the adapter layer accepts, parses, and dispatches messages asynchronously
- the `qq_chat_agent` handler only enqueues work and returns quickly
- Redis is the preferred inbox backend, with an in-memory fallback when Redis enqueue fails or no Redis connection is configured
- Redis connection lifecycle for the inbox path is owned by `storage_handler`, while `zihuan_service` only decides whether to keep using Redis or degrade to memory
- inbox consumers move queued items into blocking business execution
- single-user serialization is enforced inside the service layer session lock
- the graph engine itself remains synchronous and does not own adapter ingress concurrency

For the distinction between runtime instance ownership, Redis helper-managed reconnect, and business-level fallback, see [`config-and-connection-instances.md`](config-and-connection-instances.md).

## 6. CLI Execution Flow

Entry point: `zihuan_graph_cli/src/main.rs`

CLI order:

1. Parse `--file` or `--workflow`
2. Initialize the node registry
3. Resolve the graph path
4. Load graph JSON with migration support
5. Build `NodeGraph`
6. Execute the graph once
7. Exit with success or failure

The CLI does not start the web server, task system, admin UI, or agent manager.

## 7. Execution Boundary

The important architectural boundary is:

- `zihuan_graph_engine` handles synchronous DAG graph execution
- `zihuan_service` handles long-lived service runtimes
- `src/api` coordinates HTTP, WebSocket, task records, and browser-facing state

This is the model all current docs and new development should follow.
