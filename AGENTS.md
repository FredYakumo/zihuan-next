# AGENTS.md

This file provides project-level instructions for Codex and other coding agents working in this repository.

## Overview

`zihuan-next` is a Rust + Slint UI node-graph workflow engine for building event-driven bot pipelines. The node graph describes **data flow** between processing steps — complexity (algorithms, agentic loops, control flow) is encapsulated inside individual nodes, keeping the graph topology simple and readable. When a new complex problem arises, build a new node rather than adding complexity to the graph canvas.

The engine is split into focused library crates:

| Crate | Contents |
|---|---|
| `crates/zihuan_core` | Error types, config loading, URL utilities |
| `crates/zihuan_bot_types` | `MessageEvent`, QQ message models, bot handle |
| `crates/zihuan_llm_types` | `OpenAIMessage`, `LLMBase` trait, `FunctionTool` trait |
| `crates/zihuan_node` | `Node` trait, `DataType`/`DataValue`, DAG execution engine, general-purpose nodes, base registry |
| `crates/zihuan_bot_adapter` | `BotAdapterNode`, QQ message send/receive nodes |
| `crates/zihuan_llm` | `LLMApiNode`, `LLMInferNode`, `BrainNode`, RAG nodes |
| `node_macros` | `node_input!`, `node_output!`, `port!` procedural macros |
| `src/` | Main binary: Slint UI, combined registry (`init_registry.rs`) |

## Working Style

- Keep changes focused. Do not mix unrelated refactors into feature or bug-fix work.
- Preserve existing architecture and naming unless the task requires a deliberate change.
- Prefer small, local edits over broad rewrites.
- When instructions conflict, prefer the behavior described by the current code and docs over older Copilot notes.
- Detailed module-specific rules live under `document/`.

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

# Validate graph JSON before running (exits 0=ok/warn, 1=errors, 2=load failure)
cargo run -- --graph-json input.json --validate

# Tests
cargo test
cargo test test_name
cargo test -- --ignored

# Infra
docker compose -f docker/docker-compose.yaml up -d
alembic upgrade head
alembic revision --autogenerate -m "description"
```

## Core Rules

- One node per file.
- The graph must remain a DAG. Keep the graph topology simple; encapsulate complexity in nodes.
- Node file placement:
  - General-purpose utility node → `crates/zihuan_node/src/util/`
  - Bot / QQ messaging node → `crates/zihuan_bot_adapter/src/`
  - LLM / AI node → `crates/zihuan_llm/src/`
- Node registration:
  - Nodes in `zihuan_node` → `crates/zihuan_node/src/registry.rs` (`init_node_registry()`)
  - Nodes in `zihuan_bot_adapter` or `zihuan_llm` → `src/init_registry.rs`
- Keep Slint responsible for presentation and Rust responsible for orchestration.
- Keep message parsing and storage resilient.

## Detailed References

- `document/dev-guides/README.md`
- `document/dev-guides/node-system.md`
- `document/dev-guides/ui-architecture.md`
- `document/dev-guides/qq-message.md`
- `document/dev-guides/qq_message_storage.md`
- `document/node/node-development.md`
- `document/node/function-subgraphs.md`

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
- `document/dev-guides/README.md`
- `document/node/dynamic-port-nodes.md`
- `document/node/node-graph-json.md`
- `document/node/node-development.md`
- `document/program-execute-flow.md`
- `example.json`
