# Logging

This document describes the current logging pipeline.

## Overview

The project uses the `log` crate for log calls and `src/log_forwarder.rs` as the global logger wrapper in the main web application.

The logger fans out to:

- console output
- files under `./logs/`
- WebSocket broadcast messages
- per-task log storage when execution is scoped to a task

## Main Web App Initialization

`src/main.rs` does this first:

1. creates `BASE_LOG` with `LogUtil::new_with_path("zihuan_next", "logs")`
2. calls `log_forwarder::init(&BASE_LOG)`
3. later attaches app state and WebSocket broadcast with:
   - `log_forwarder::set_app_state(...)`
   - `log_forwarder::set_broadcast(...)`

This ordering matters because startup failures and auto-start agent logs should already be visible.

## What `log_forwarder` Adds

`src/log_forwarder.rs` wraps `LogUtil` and extends it with two runtime behaviors:

### WebSocket forwarding

Every record is converted into `ServerMessage::LogMessage` and broadcast to connected clients on `/api/ws`.

### Task-scoped log capture

When code runs inside `log_forwarder::scope_task(task_id, || { ... })`, log lines are also appended to that task's stored log list in `AppState`.

This is how graph execution logs and agent-response logs become visible in the task UI and task log APIs.

## Task Logging Flow

During graph execution:

1. the API creates a task entry
2. execution runs under `scope_task(task_id, ...)`
3. every `log::*` call inside that scope is appended to the task log list
4. the same record is still written to console/files and broadcast over WebSocket

This means one log call can feed all observer channels at once.

During agent handling:

1. starting an agent does **not** create a task entry
2. a task entry is created only when the agent begins handling one concrete input/request
3. QQ chat uses one task per reply flow, for example `回复[123456]的消息`
4. HTTP stream uses one task per request
5. the handling code runs under that response task ID, so every `log::*` call is persisted into `logs/tasks/<task_id>.jsonl`

This keeps the task list focused on concrete work units instead of long-lived agent uptime.

## Persisted Task Logs

Task logs are persisted as JSONL files under:

- `logs/tasks/<task_id>.jsonl`

Each task has its own log file. The task log API reads from those persisted files; the UI is not reading transient in-memory-only logs.

The current task record also stores:

- `start_time`
- `end_time`
- `duration_ms`
- `status`
- `error_message`
- `result_summary`
- `log_path`

For agent response tasks this means every individual reply/request has its own durable log trail.

## Log Levels

The max log level is derived from `RUST_LOG`. If unset, it falls back to `info`.

Examples:

```bash
RUST_LOG=debug ./target/release/zihuan_next
RUST_LOG=trace cargo run
```

## CLI Note

`zihuan_graph_cli` does not initialize the web app's `log_forwarder` pipeline. The WebSocket/task fan-out behavior belongs to the main server runtime.

## Usage Guidance

Use logging for:

- startup/shutdown milestones
- connection and service lifecycle state
- fallback activation
- important execution checkpoints
- recoverable anomalies

Do not use logs as a replacement for returning structured errors to callers.
