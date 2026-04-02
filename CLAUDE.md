# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

zihuan-next is a Rust + Slint UI node-graph workflow engine for building event-driven bot pipelines. Nodes are composable DAG components connecting via typed ports. Workflows are saved/loaded as JSON and can run headless or in a GUI editor.

Detailed implementation guidance lives under `document/`.

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

# Validate graph JSON (exits 0=ok/warn-only, 1=errors, 2=load failure)
cargo run -- --graph-json input.json --validate

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
- Preserve DAG graph behavior and current JSON conventions.
- Register new node types in `src/node/registry.rs`.
- Keep Slint responsible for presentation and Rust responsible for orchestration.
- Keep bot parsing lenient and message storage resilient.

## Detailed References

- `document/dev-guides/README.md`
- `document/dev-guides/node-system.md`
- `document/dev-guides/ui-architecture.md`
- `document/dev-guides/node-types.md`
- `document/dev-guides/event-handlers.md`
- `document/dev-guides/qq-message.md`
- `document/dev-guides/qq_message_storage.md`
- `document/dev-guides/runtime-utils.md`
- `document/node/node-development.md`
- `document/node/dynamic-port-nodes.md`
- `document/node/node-graph-json.md`
