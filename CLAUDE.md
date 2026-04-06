# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

zihuan-next is a Rust node-graph workflow engine for building event-driven bot pipelines. The node graph describes **data flow** between processing steps — complexity (algorithms, agentic loops, control flow) is encapsulated inside individual nodes, keeping the graph topology simple and readable. When a new complex problem arises, build a new node rather than adding complexity to the graph canvas.

The editor runs in the browser. The backend is a single Rust binary (Salvo HTTP server) that serves the web UI and exposes a REST + WebSocket API.

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
| `src/` | Main binary: Salvo web server, REST/WebSocket API (`src/api/`), combined registry (`src/init_registry.rs`) |
| `webui/` | Frontend: Vite + TypeScript + Litegraph.js; embedded at compile time via rust-embed |

Detailed implementation guidance lives under `document/`.

## Commands

```bash
# Build (pnpm run build in webui/ runs automatically via build.rs)
cargo build
cargo build --release

# Frontend only (when iterating on web UI)
cd webui && pnpm run build

# Run (starts web server; open browser at http://127.0.0.1:8080)
cargo run
cargo run -- --host 0.0.0.0 --port 9000

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

Config is read from `config.yaml` where applicable. Detailed module rules are documented under `document/`.

## Core Rules

- Keep changes focused and local.
- One node per file.
- The graph must remain a DAG. Keep graph topology simple; encapsulate complexity in nodes.
- Node file placement:
  - General-purpose utility node → `crates/zihuan_node/src/util/`
  - Bot / QQ messaging node → `crates/zihuan_bot_adapter/src/`
  - LLM / AI node → `crates/zihuan_llm/src/`
- Node registration:
  - Nodes in `zihuan_node` → `crates/zihuan_node/src/registry.rs` (`init_node_registry()`)
  - Nodes in `zihuan_bot_adapter` or `zihuan_llm` → `src/init_registry.rs`
- Keep the web frontend (TypeScript/Litegraph.js) responsible for presentation; Rust backend responsible for graph execution and state.
- Keep bot parsing lenient and message storage resilient.

## Detailed References

- `document/dev-guides/README.md`
- `document/dev-guides/node-system.md`
- `document/dev-guides/ui-architecture.md`
- `document/dev-guides/node-types.md`
- `document/dev-guides/qq-message.md`
- `document/dev-guides/qq_message_storage.md`
- `document/node/node-development.md`
- `document/node/function-subgraphs.md`
- `document/node/dynamic-port-nodes.md`
- `document/node/node-graph-json.md`
