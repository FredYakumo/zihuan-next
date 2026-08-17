# zihuan-next Agent Guide

## Project map

zihuan-next is a Rust AI-agent framework: unified local/cloud inference, a Brain tool-calling runtime, a DAG workflow engine, and IMS adapters. The execution path is **define graph → topologically sort → execute nodes → produce outputs**.

- `zihuan_core`: cross-crate types and shared system facilities.
- `zihuan_graph_engine`: DAG execution and node lifecycle.
- `zihuan_agent`: Brain runtime and tool interfaces.
- `zihuan_service`: chat agents, HTTP streaming, REST API, and commands.
- `model_inference`: model configuration and provider adapters.
- `ims_bot_adapter`: QQ adapter and adapter nodes.
- `webui`: Vue admin UI at `/` and Litegraph editor at `/editor`.
- `database`: Python persistence utilities; `workflow_set`: workflow templates.

## Concepts and skills

Use the relevant skill before making changes in that area:

- `/zihuan-rust-architecture` — shared Rust architecture, messages, persistence, adapters, and conventions.
- `/zihuan-python-dev` — database and utility Python development.
- `/zihuan-node-dev` — DAG nodes, ports, macros, and registration.
- `/zihuan-agent-dev` — agent configuration, Brain wiring, and lifecycle.
- `/zihuan-agent-tool-dev` — `FunctionTool` implementations and embedded graph tools.
- `/zihuan-frontend-dev` — Vue admin UI and Litegraph editor.
- `/zihuan-build` — build, run, CUDA/Metal, and Docker workflows.
- `/zihuan-test` — test selection and debugging.
- `/zihuan-lint` — Rust formatting and linting.

## Working agreement

- This is greenfield software: breaking changes are acceptable unless requested otherwise.
- Keep changes focused; follow the current code when older documentation conflicts.
- Prefer LSP navigation (`rust-analyzer` for Rust, TypeScript MCP for TypeScript) before text search.
- Consult repository memory for deeper architecture only when needed.
