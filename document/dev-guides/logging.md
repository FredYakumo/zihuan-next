# Logging

This document explains the logging system used by `zihuan-next`.

## Overview

The project uses the `log` crate for log calls and a custom composite logger for output routing.

- Rust code emits logs with `error!`, `warn!`, `info!`, `debug!`, and `trace!`.
- `src/main.rs` initializes logging before node registry setup, graph loading, or UI startup.
- The base backend is `LogUtil`, which writes logs to the console and to files under `./logs/`.
- In GUI mode, the logger also mirrors log lines into an in-memory buffer so Slint can display them.

## Initialization Flow

Logging is initialized at process startup in [`src/main.rs`](/c:/Users/fredyakumo/zihuan-next/src/main.rs#L27).

1. `BASE_LOG` is created with `LogUtil::new_with_path("zihuan_next", "logs")`.
2. `CompositeLogger::init(&BASE_LOG)` installs the global logger.
3. The maximum log level is derived from `RUST_LOG`.
4. All later `log` macros flow through the same logger.

This order matters because startup failures, graph loading errors, and runtime warnings all depend on logging already being available.

## Logger Structure

The GUI-aware logger lives in [`src/ui/log_overlay.rs`](/c:/Users/fredyakumo/zihuan-next/src/ui/log_overlay.rs).

`CompositeLogger` has two responsibilities:

- Delegate every record to `LogUtil` so standard console and file logging still works.
- Capture enabled log records into in-memory queues for the GUI.

The in-memory side is split into two buffers:

- `LOG_RING_BUFFER`: short-lived queue for newly arrived entries waiting to be shown in the overlay.
- `LOG_HISTORY`: longer history used by the log history dialog.

Current limits:

- Overlay buffer: `MAX_ENTRIES = 5`
- History buffer: `MAX_HISTORY = 1000`

Each stored entry contains:

- `level: log::Level`
- `message: String`

## GUI Log Display

In GUI mode, the Slint view polls for new log entries from [`src/ui/node_graph_view.rs`](/c:/Users/fredyakumo/zihuan-next/src/ui/node_graph_view.rs#L299).

- A timer polls `drain_new_entries()` every 100 ms.
- New entries are converted into `LogEntryVm` values and pushed into the overlay model.
- The overlay fades out after a short idle period.
- The full history dialog reads from `get_history()`.

This means GUI logging is pull-based on the UI thread, while log production can happen from runtime code on other threads.

## Headless Behavior

In headless mode there is no Slint consumer, but the same global logger is still used.

- Logs continue to go to `stdout`.
- Logs continue to be written to `./logs/`.
- The GUI-only in-memory buffers may still receive records, but they are not surfaced unless the UI is running.

## Log Level Control

The active max log level comes from the `RUST_LOG` environment variable in [`src/ui/log_overlay.rs`](/c:/Users/fredyakumo/zihuan-next/src/ui/log_overlay.rs#L77).

Supported values:

- `error`
- `warn`
- `info`
- `debug`
- `trace`
- `off`

Any other value currently falls back to `info`.

## Usage Conventions

Use the `log` macros directly in Rust code:

```rust
log::error!("Node {} failed: {}", node_id, err);
log::warn!("Configuration missing, using fallback");
log::info!("Graph loaded from {}", path.display());
log::debug!("Executing node {} with {} inputs", id, inputs.len());
```

Follow the existing module prefix style when a subsystem benefits from easy filtering in mixed logs. Existing examples include:

- `[MessageStore]`
- `[MySqlNode]`
- `[MessageCacheNode]`
- `[OpenAIMessageSessionCacheNode]`

Prefer logs for:

- startup and shutdown milestones
- external service connection state
- fallback activation
- recoverable runtime anomalies
- high-value execution checkpoints

Avoid using logs as a substitute for returning structured errors.

## Practical Notes

- If you add a new subsystem with frequent logs, keep messages concise because GUI overlay space is limited.
- If you need the log history dialog to retain more records, update `MAX_HISTORY` deliberately and consider UI cost.
- If you change initialization code, preserve the guarantee that logging is ready before other startup work begins.
