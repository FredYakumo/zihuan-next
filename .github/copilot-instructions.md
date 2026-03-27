# Copilot Instructions

## Overview

`zihuan-next` is a Rust + Slint node-graph workflow engine for event-driven bot pipelines.

## High-Level Rules

- Keep changes focused.
- Preserve current architecture and naming unless the task requires otherwise.
- One node per file.
- Preserve DAG-based graph behavior.
- Register new node types in `src/node/registry.rs`.
- Keep UI responsibilities split between Slint presentation and Rust orchestration.
- Keep message parsing and storage behavior resilient.

## Build And Validation

```bash
cargo build
cargo run
cargo test
```

## Detailed References

Detailed guidance has moved under `document/`.

- `document/dev-guides/README.md`
- `document/dev-guides/node-system.md`
- `document/dev-guides/ui-architecture.md`
- `document/dev-guides/bot-adapter.md`
- `document/dev-guides/event-handlers.md`
- `document/dev-guides/message-models.md`
- `document/dev-guides/message-store.md`
- `document/dev-guides/runtime-utils.md`
- `document/node/node-development.md`
- `document/node/dynamic-port-nodes.md`
- `document/node/node-graph-json.md`

