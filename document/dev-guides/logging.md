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

This is how graph execution logs become visible in the task UI and task log APIs.

## Task Logging Flow

During graph execution:

1. the API creates a task entry
2. execution runs under `scope_task(task_id, ...)`
3. every `log::*` call inside that scope is appended to the task log list
4. the same record is still written to console/files and broadcast over WebSocket

This means one log call can feed all observer channels at once.

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
