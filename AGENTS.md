# Agent Instructions for zihuan-next

## Build, Test, and Lint

See the `/zihuan-build`, `/zihuan-test`, and `/zihuan-lint` skills (in `.agents/skills/`) for detailed instructions.

## Architecture Overview

zihuan-next is a multi-tier AI agent development and runtime framework built in Rust. It unifies local inference (Candle, Llama.cpp) and cloud model APIs (OpenAI, Anthropic) under a single abstraction, then layers on a **Brain** tool-calling agent runtime, a **visual node-graph engine** for workflow orchestration, and **IMS-native adapters** for real-world bot deployment. The core pipeline is: **Define graph → Topological sort → Execute nodes → Produce outputs**. It has two frontends served from the same Vite app: a **Vue 3 admin panel** (`/`) for configuration management and a **Litegraph.js editor** (`/editor`) for visual workflow editing.

### Key crates

- **`zihuan_core`** — Shared types and traits used across the workspace. Common types (like `DataType`, ports, config) that may cause circular references live here.
- **`zihuan_graph_engine`** — DAG execution engine: topological sort, node lifecycle, data pool, graph execution.
- **`zihuan_agent`** — Agentic runtime: `Brain` tool-calling loop, `FunctionTool` trait re-export, `BrainTool`, `BrainObserver` hooks.
- **`zihuan_service`** — Service layer: QQ chat agent, HTTP stream agent, REST API endpoints, command system.
- **`model_inference`** — LLM model inference integration: model configuration, API adapters, system configuration. Provider/wire-format compatibility lives under `src/llm_message/convert/`; for remote API styles, keep one convert entry file per concrete `LlmApiStyle` variant rather than multiplexing multiple wire dialects through a shared public format enum.
- **`ims_bot_adapter`** — QQ bot adapter: WebSocket connection, message extraction, event processing, bot adapter nodes. Message-structure labels and boundary markers (e.g. `CURRENT_MESSAGE_LABEL`, `REPLY_START_MARKER`, `QUOTE_CONTENT_LABEL`) are defined as constants in `src/lib.rs`; any code that renders or parses nested message text must reuse these constants rather than hard-coding strings.
- **`storage_handler`** — Storage abstractions: Redis, MySQL, S3, Weaviate connections.
- **`zihuan_nlp`** — NLP utilities: text segmentation, tokenization.
- **`node_macros`** — Proc macros: `node_input!`, `node_output!`, `node_input_flow!`, `node_output_flow!`, `return_with_node_output!`.
- **`general_wheel_cpp`** — C++ FFI bridge for native performance libraries.

### Non-Rust components

- **`webui/`** — Vite + TypeScript app with two subsystems:
  - **Admin UI** (`/`): Vue 3 + Vue Router — manage connections, agents, LLMs, graphs, tasks, commands, settings, data
  - **Graph Editor** (`/editor`): Litegraph.js — visual node-graph canvas for workflow editing
- **`database/`** — Python database utilities (SQLAlchemy models for message records, task entries, task logs).
- **`workflow_set/`** — Pre-built workflow template JSON files.

### Key architectural concepts

**Node System** — Every node implements the `Node` trait. Only one node type exists: `Simple` — every node executes as a synchronous transformation `execute(inputs) -> outputs`. Nodes receive all resolved inputs as a `NodeInputFlow` and return a `NodeOutputFlow`.

**Port & Data Type System** — Ports carry typed data between nodes. Defined with `node_macros` using `port!{ name = "...", ty = Type, desc = "..." }` entries. Types include primitives (`String`, `Integer`, `Float`, `Boolean`, `Json`, `Binary`), messages (`LLMMessage`, `QQMessage`, `MessageEvent`), infrastructure refs (`RedisRef`, `MySqlRef`, `WeaviateRef`), and LLM types (`LLModel`, `EmbeddingModel`, `FunctionTools`). See `/zihuan-node-dev`.

**Unified LLM Message Model** — Internal conversation/state/history flow uses `zihuan_core::llm::LLMMessage` as the project-wide message type. `LLMMessage` contains `role`, `parts`, `tool_calls`, `tool_call_id`, `reasoning_content`, and `usage`. Message content is carried by `zihuan_core::MessagePart`, which is the project-wide base payload for text and multimodal message content. Image/video parts must carry the existing project media model (`PersistedMedia`) rather than raw provider-specific URL structs. Provider-specific request/response shapes belong in convert modules, not in the core message type.

**Brain Agent Loop** — The `Brain` engine implements a tool-calling loop (max 25 iterations): send conversation to LLM → receive tool calls → execute matching tools → append results → repeat. Tools implement the `FunctionTool` trait and are defined in `zihuan_agent/src/tools/`. See `/zihuan-agent-tool-dev`.

**Admin UI Architecture** — Vue 3 SPA with Vue Router. `AdminApp.vue` provides the sidebar shell; each page is a single-file component under `admin/view/`. Shared types in `admin/model.ts`. The admin UI controls: connection configs (MySQL, Redis, Weaviate, S3, bot adapters), LLM service/model management, agent CRUD, graph/workflow management, task monitoring, command permissions, and global settings. See `/zihuan-frontend-dev`.

**Hyperparameter System** — Per-graph configuration values persisted as YAML. Nodes bind input ports to hyperparameter names, and values are applied before execution.

## Rust Conventions

### Style

- Group imports in three blocks: `std`, third-party crates, then `crate`/workspace imports, separated by blank lines.
- Prefer direct, domain-specific names. Function names should describe the action or conversion being performed.
- Max line length: aim for 120 characters.
- `UpperCamelCase` for types, `snake_case` for functions, methods, and file names.

### Error handling

Prefer the `?` operator for propagating errors. Avoid verbose `if let Err(` patterns.

Error messages should carry concrete business context — field names, node inputs, source values. Avoid vague failure text.

### Control flow & readability

- Prefer explicit control flow over clever chaining for business logic. Use `match`, `if let`, and intermediate local variables freely.
- Keep data-loading and transformation code linear and readable. Prefer straightforward loops over dense iterator pipelines when the loop carries business meaning.
- Extract repeated parsing or conversion logic into small helpers close to the call site. Do not duplicate similar logic inline.
- Don't use `else` after `return`.

### Structs & constructors

- Build structs with explicit named fields. Use `..Default::default()` only when the defaulted fields are intentional.
- Each node function signature: `pub fn new(id: String, name: String) -> Self`.

### Macros

Prefer macros for pattern elimination. When the same structural pattern appears across multiple types or functions, prefer a `macro_rules!` macro over repeating the pattern manually. Macros are preferred over generic helpers or trait abstractions when the duplication is about code structure rather than type-level polymorphism.

### Organization

- **Common types** that may cause circular references go in `zihuan_core`. Otherwise, keep code and types in the package that owns the responsibility.
- **One node per file.** The graph must remain a DAG.
- **Don't repeat yourself.** Reuse existing functionality whenever possible. Search before writing a new helper; do not duplicate logic.
- **Message text parsing constants.** When rendering or parsing nested QQ message structures (reply quotes, forward nodes, image references), always import and use the shared constants defined in `ims_bot_adapter/src/lib.rs` (e.g. `REPLY_START_MARKER`, `QUOTE_CONTENT_LABEL`). Do not hard-code Chinese labels or boundary markers locally.
- **LLM message naming.** Use `LLMMessage`, `MessagePart`, and `LLMMessageSessionCacheRef` in new Rust code. `MessagePart` is the shared base carrier for project message content. Do not introduce new `OpenAIMessage*` names for internal message flow.
- **Node naming.** New or renamed message utility nodes should use the `llm_message_*` prefix rather than `openai_message_*`. Keep node IDs, file names, and module names aligned with the `llm_message_*` terminology unless explicit backward compatibility is required.
- **API style conversion layout.** For remote LLM adapters, each non-local `LlmApiStyle` should map to a dedicated convert entry file under `model_inference/src/llm_message/convert/`. Shared parsing helpers are fine, but the top-level dispatch must remain one-style-to-one-file so wire dialects stay easy to audit.

### Comments

Skip comments when the code is self-explanatory. Write a comment only when the *why* is non-obvious (hidden constraint, subtle invariant, deliberate workaround). Never use ASCII-art separator comments (`// ----`, `// ====`, etc.).

### Tests

Do not write unit tests by default. Only add tests when the feature is complex enough to warrant them. Use `#[cfg(test)] mod tests` for unit tests and `tests/` directories at crate roots for integration tests. See `/zihuan-test`.

## Python Conventions

Python code lives in `database/` and `utils/`. The project uses **uv** for dependency management.

### Virtual environment

```bash
uv venv
.\.venv\Scripts\Activate.ps1  # Windows PowerShell
uv pip install -e .
```

### Python Style

- Follow [PEP 8](https://peps.python.org/pep-0008/) with 120-char max line length
- Use `ruff` for linting and formatting (configured in `pyproject.toml`)
- Type hints are encouraged but not strictly enforced

## Frontend Conventions

See `/zihuan-frontend-dev` for detailed development instructions.

- **Two separate UIs** — admin panel (`/`) vs graph editor (`/editor`), independent code paths
- **Vue 3 single-file components** — `<template>`, `<script setup>`, `<style scoped>` per view
- **TypeScript strict mode** — all types must be explicitly defined
- **pnpm 10** is the package manager — use `pnpm install`, not npm or yarn
- **Admin model types in `admin/model.ts`** — shared types for connection configs, agent types, LLM API styles

## Working Style

- Keep changes focused. Do not mix unrelated refactors into feature or bug-fix work.
- Preserve existing architecture and naming unless the task requires a deliberate change.
- Prefer small, local edits over broad rewrites.
- When instructions conflict, prefer the behavior described by the current code over older documentation.

## PR Guidelines

- Keep PRs small — aim for focused changes. Separate cosmetic changes from functional ones.
- Build and test locally before submitting: `cargo build --release` and `cargo test` should pass.
- Run `cargo clippy --all-targets --all-features` and `cargo fmt --all` before committing.
- The PR author is responsible for merging after approval.

## Code Search

When navigating the codebase, prefer LSP-based tools over text-only grep:

- **Rust**: `rust-analyzer` MCP server.
- **TypeScript**: TypeScript MCP server.

The workspace crate layout is described above in [Architecture Overview](#architecture-overview). For deeper architectural reference, see the codebase map in repository memory (`/memories/repo/zihuan-next-codebase-map.md`) and the QQ chat agent deep dive (`/memories/repo/qq-chat-agent-deep-dive.md`).

## Domain-Specific Skills

For detailed patterns and instructions on specific development areas:

- `/zihuan-build` — Build the project from source
- `/zihuan-test` — Run tests and debug test failures
- `/zihuan-lint` — Lint and format code
- `/zihuan-node-dev` — Create and modify graph nodes (port macros, flow macros, registration)
- `/zihuan-agent-dev` — Create and configure agents (Brain, LLM, built-in tools, node graph tools)
- `/zihuan-agent-tool-dev` — Develop individual agent tools and Brain integrations
- `/zihuan-frontend-dev` — Develop the admin UI (Vue 3) and graph editor (Litegraph.js)
