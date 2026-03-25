# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

zihuan-next is a Rust + Slint UI node-graph workflow engine for building event-driven bot pipelines. Nodes are composable DAG components connecting via typed ports. Workflows are saved/loaded as JSON and can run headless or in a GUI editor.

## Commands

```bash
# Build
cargo build
cargo build --release

# Run GUI mode
cargo run
cargo run -- --graph-json example.json      # Load existing graph in GUI

# Run headless
cargo run -- --graph-json input.json --no-gui
cargo run -- --graph-json input.json --save-graph-json output.json --no-gui  # Convert/validate

# Run tests
cargo test
cargo test test_name                         # Single test
cargo test -- --ignored                      # LLM integration tests (require live API)

# Integration tests for message store (require live services)
export REDIS_URL=redis://localhost:6379
cargo test message_store::tests::test_redis_store
export DATABASE_URL=mysql://user:pass@localhost:3306/zihuan_aibot
cargo test message_store::tests::test_mysql_store

# Infrastructure
docker compose -f docker/docker-compose.yaml up -d  # Redis (+ optional MySQL)
alembic upgrade head                                 # DB migrations (Python required)
alembic revision --autogenerate -m "description"     # Generate migration after schema changes
```

Config is read from `config.yaml` (copy from `config.yaml.example`). Only LLM API endpoints and keys live here. Bot server URL/token, Redis URL, and MySQL URL are configured as `inline_values` on the relevant nodes in the workflow JSON — not in `config.yaml`.

## Architecture

### Core execution model (`src/node/`)

- `mod.rs` — `Node` trait + `NodeGraph`. Two execution modes:
  - **Simple**: `execute(inputs) -> outputs` — runs once per input set
  - **EventProducer**: `on_start() → loop { on_update() } → on_cleanup()` — stateful event source (e.g. `BotAdapterNode`)
- `data_value.rs` — `DataType` and `DataValue` enums. All port data flows as `HashMap<String, DataValue>`.
- `registry.rs` — global `NODE_REGISTRY`. `init_node_registry()` registers all node types. `build_node_graph_from_definition()` rebuilds a `NodeGraph` from JSON.
- `graph_io.rs` — `NodeGraphDefinition` / `EdgeDefinition` serde structs — source of truth for the JSON format.

### Adding a new node

**Convention: one node per file.** Never put multiple node structs in the same `.rs` file.

1. Create `src/node/your_node.rs` (or a new file in the relevant module).
2. Implement `Node` trait. Use `node_input![]` / `node_output![]` macros with `port!{name=..., type=..., desc=...}` syntax to reduce boilerplate. The `[]` form requires a trailing `;`.
3. Register in `src/node/registry.rs` inside `init_node_registry()`:
   ```rust
   register_node!("type_id", "显示名称", "分类", "Description", YourNodeStruct);
   ```
   Category names in the registry use Chinese (e.g. `"工具"`, `"AI"`, `"适配器"`). For complex constructors use `NODE_REGISTRY.register(...)` with a closure.

Registered nodes appear in the GUI automatically.

### Port binding rules

Workflow JSON uses explicit `edges` (see `document/node/node-graph-json.md`). When `edges` is empty, the engine falls back to implicit auto-binding by matching port names. Each input port accepts **at most one** incoming edge; the graph must be a DAG.

Node `inline_values` supply default values when no edge is connected. Supported for `String`, `Integer`, `Float`, `Boolean`.

### Module layout

- `src/bot_adapter/` — `BotAdapterNode` (EventProducer), `MessageSenderNode`, `MessageEventToStringNode`; QQ WebSocket integration
- `src/bot_adapter/models/event_model.rs` — `MessageType` enum, `MessageEvent`, `Sender`
- `src/bot_adapter/models/message.rs` — `Message` enum (`PlainText`, `At`, `Replay`); lenient serde skips unknown variants
- `src/bot_adapter/event.rs` — per-platform handlers (`process_friend_message`, `process_group_message`)
- `src/llm/` — `LLMNode`, `AgentNode`, `TextProcessorNode`; `BrainAgent` for multi-tool reasoning
- `src/llm/function_tools/` — `FunctionTool` trait implementations (`MathTool`, `ChatHistoryTool`, `CodeWriterTool`)
- `src/ui/` — Slint visual editor (see UI section below)
- `src/node/util/` — utility nodes: `ConditionalNode`, `JsonParserNode`, etc. (one node per file)
- `src/node/database/` — Redis and MySQL connection nodes
- `src/config.rs` — config loading (priority: `config.yaml` → env vars → defaults); `pct_encode()` for Redis passwords with special chars
- `src/util/message_store.rs` — three-tier message storage
- `node_macros/` — proc-macro crate; `node_input!`, `node_output!`, `port!` macros

### Three-tier message storage (`src/util/message_store.rs`)

Redis cache → MySQL persistence → in-memory fallback. Graceful degradation with `[MessageStore]`-prefixed log output. Redis is flushed (`FLUSHDB`) on startup. Wrap in `Arc<TokioMutex<>>` for async access; always spawn storage writes with `tokio::spawn` to avoid blocking the event loop.

- `get_message()` — Redis → memory
- `get_message_with_mysql()` — Redis → MySQL → memory
- `get_message_record()` / `query_messages()` — MySQL only

### Extending the bot

**New message type**: Add struct + variant to `Message` enum in `src/bot_adapter/models/message.rs` (tagged serde `{"type": "...", "data": {...}}`). Lenient deserialization means unknown variants are skipped without crashing.

**New platform**: Add handler in `src/bot_adapter/event.rs`, register in `BotAdapter::new()` event_handlers map, add variant to `MessageType` enum.

**New LLM function tool**: Implement `FunctionTool` trait in `src/llm/function_tools/`, register in `src/main.rs` tools vec.

**Database schema change**: Edit `database/models/` (Python SQLAlchemy), then `alembic revision --autogenerate`.

## UI architecture (`src/ui/`)

Slint owns presentation/layout/bindings. Rust owns graph state, VM projection, callbacks, persistence.

**Slint file responsibilities:**
- `graph_window.slint` — root `NodeGraphWindow`; stable public entry point for Rust; composition only, not a catch-all
- `types.slint` — shared exported VM structs; keep names stable (they're part of the Rust integration contract)
- `components/*.slint` — canvas, node cards, buttons, menu, tabs
- `dialogs.slint` — overlay dialogs/selectors (must stay above canvas layer)

**Rust file responsibilities:**
- `node_graph_view.rs` — main orchestration; large callback families go in `node_graph_view_callbacks/` subdirectory
- `node_graph_view_vm.rs` — `NodeGraphDefinition` → Slint VM projection
- `node_graph_view_geometry.rs` — grid/edge geometry, snap helpers, coordinate conversions
- `node_graph_view_inline.rs` — inline port state and message-list editing
- `selection.rs` — selection sync
- `window_state.rs` — persistent window config (`~/.config/zihuan_next/` on Linux/macOS, `%APPDATA%/zihuan_next/window_config.json` on Windows)

**Critical constraints:**
- `GraphCanvas` must keep `clip: true` — without it, grid/edges/nodes overflow the menu bar
- Preserve pan/zoom coordinate conversions between canvas space and screen space
- Never mix Rust architectural rewrites into a Slint modularization change

**Common pitfalls:**
- Forgetting to import a shared exported struct into `graph_window.slint`
- Accidentally changing callback signatures while moving components
- Breaking zoom/pan math by mixing coordinate spaces

**Verification after UI changes:** `cargo check`, then manually test menu, tabs, pan/zoom, node drag/resize, box selection, edge selection, dialogs, and these special node UIs: `string_data`, `message_list_data`, `qq_message_list_data`, `message_event_type_filter`, `preview_message_list`.

## Workflow JSON format

See `document/node/node-graph-json.md` for full spec and `example.json` in the repo root for a working reference. Key structure: `{ "nodes": [...], "edges": [...] }`. Each node requires `id`, `name`, `node_type` (registry key), `input_ports`, `output_ports`.
