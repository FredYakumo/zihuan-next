# AGENTS.md

This file provides project-level instructions for Codex and other coding agents working in this repository.

## Overview

`zihuan-next` is a Rust + Slint UI node-graph workflow engine for building event-driven bot pipelines. Nodes are composable DAG components connected through typed ports. Workflows are saved and loaded as JSON and can run headless or in the GUI editor.

## Working Style

- Keep changes focused. Do not mix unrelated refactors into feature or bug-fix work.
- Preserve existing architecture and naming unless the task requires a deliberate change.
- Prefer small, local edits over broad rewrites.
- When instructions conflict, prefer the behavior described by the current code and docs over older Copilot notes.

## Build And Run

```bash
# Build
cargo build
cargo build --release

# Run GUI mode
cargo run
cargo run -- --graph-json example.json

# Run headless
cargo run -- --graph-json input.json --no-gui
cargo run -- --graph-json input.json --save-graph-json output.json --no-gui

# Tests
cargo test
cargo test test_name
cargo test -- --ignored

# Infra
docker compose -f docker/docker-compose.yaml up -d
alembic upgrade head
alembic revision --autogenerate -m "description"
```

## Node Graph Rules

- One node per file. Never place multiple node structs in the same `.rs` file.
- The core node system lives under `src/node/`.
- Nodes implement the `Node` trait and typically use `node_input![]`, `node_output![]`, and `port!{...}` macros.
- Register all new node types in `src/node/registry.rs` inside `init_node_registry()`.
- Registry category names use Chinese labels such as `"工具"`, `"AI"`, and `"适配器"`.
- The graph must remain a DAG.
- Workflow JSON now uses explicit `edges` as the source of truth. If `edges` is empty, the engine may fall back to implicit binding by matching port names.
- Each input port accepts at most one incoming edge.
- `inline_values` act as defaults when an input is not connected. Supported scalar defaults are `String`, `Integer`, `Float`, and `Boolean`.

## Adding Or Changing Nodes

1. Create a dedicated file for the node in the relevant module.
2. Implement `Node`.
3. Register the node in `src/node/registry.rs`.
4. If the node should appear in the GUI, keep its metadata consistent with existing registry usage.

For event sources:

- Use `NodeType::EventProducer`.
- Preserve the lifecycle model: `on_start() -> on_update() -> on_cleanup()`.

## Bot Adapter And Message Models

- Bot adapter code lives in `src/bot_adapter/`.
- Keep lenient serde behavior for incoming message parsing. Unsupported message elements should be skipped instead of crashing the whole event.
- New message types belong in `src/bot_adapter/models/message.rs`.
- New platforms require:
  - a handler in `src/bot_adapter/event.rs`
  - registration in `BotAdapter::new()`
  - a new `MessageType` variant in `src/bot_adapter/models/event_model.rs`

## Message Store Rules

- The message store is a three-tier system: Redis cache -> MySQL persistence -> in-memory fallback.
- Keep graceful degradation behavior. Failures should not take down the bot pipeline.
- Message store logs should use the `[MessageStore]` prefix.
- Redis is flushed on startup; do not change this casually.
- Wrap shared store access in `Arc<TokioMutex<...>>` where required by current architecture.
- Prefer spawning storage writes with `tokio::spawn` so message persistence does not block the event loop.

## UI Rules

- Slint owns presentation, layout, and bindings. Rust owns graph state, VM projection, callbacks, persistence, and orchestration.
- Keep `src/ui/graph_window.slint` as the stable root entry point consumed by Rust.
- Keep exported VM struct names in `src/ui/types.slint` stable unless Rust integration is updated in the same change.
- Prefer extracting reusable components instead of turning `graph_window.slint` into a catch-all file.
- If callback families grow large, move them under dedicated Rust submodules rather than expanding one giant file.
- `GraphCanvas` must keep `clip: true`.
- Preserve canvas-space and screen-space conversion logic for pan, zoom, drag, resize, and selection.
- Dialogs and selectors must remain above the canvas layer.

## Database And Schema Changes

- Database schema models live in `database/models/`.
- After schema changes, generate migrations with Alembic instead of hand-waving the DB state.
- Do not change persistence schema without updating the relevant migration path and examples.

## Validation Expectations

After Rust changes:

- Run `cargo check` for lightweight verification when possible.
- Run `cargo test` when the change affects behavior covered by tests.

After UI changes:

- Run `cargo check`.
- Manually verify menu, tabs, pan/zoom, node drag/resize, box selection, edge selection, dialogs, and these node UIs:
  - `string_data`
  - `message_list_data`
  - `qq_message_list_data`
  - `message_event_type_filter`
  - `preview_message_list`

## Useful References

- `README.md`
- `document/node/node-graph-json.md`
- `document/node/node-development.md`
- `document/program-execute-flow.md`
- `example.json`
