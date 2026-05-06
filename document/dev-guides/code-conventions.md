# Code Conventions

Naming, file placement, and development rules for the current `zihuan-next` codebase.

## Naming

### Rust

| Item | Convention | Example |
|---|---|---|
| types / enums / traits | `UpperCamelCase` | `NodeGraph`, `AgentConfig` |
| functions / methods / fields | `snake_case` | `build_router`, `load_agents` |
| modules / files | `snake_case` | `local_candle_embedding.rs` |
| constants | `SCREAMING_SNAKE_CASE` | `SYSTEM_CONFIG_FILE` |
| node structs | `<Purpose>Node` | `FormatStringNode`, `TavilySearchNode` |

### Node type IDs

Registry `type_id` values are stable `snake_case` identifiers such as:

```text
format_string
function
llm_infer
qq_chat_agent
mysql
rustfs
```

Changing a published `type_id` breaks existing graph JSON unless you also provide migration support.

### Categories

The node palette currently uses Chinese category labels such as:

- `工具`
- `AI`
- `消息`
- `数据`
- `数据库`
- `消息存储`
- `Bot适配器`
- `内部`

## File Placement

### One node per file

Keep one node implementation per file.

Typical locations:

- `zihuan_graph_engine/src/util/` for general runtime/utility nodes
- `zihuan_graph_engine/src/` for engine-owned feature modules
- `zihuan_llm/src/` for AI and agent-related nodes
- `storage_handler/src/` for storage/connection nodes
- `ims_bot_adapter/src/` for adapter-facing nodes

### Registration

After adding a node:

1. Export it from the parent `mod.rs`
2. Register it in the owning crate registry

Current registry entry points:

- `zihuan_graph_engine::registry::init_node_registry()`
- `storage_handler::init_node_registry()`
- `ims_bot_adapter::init_node_registry()`
- `zihuan_llm::init_node_registry()`
- combined bootstrap: `src/init_registry.rs`

## High-Level Package Roles

| Package | Role |
|---|---|
| `zihuan_core` | Shared core types and helpers |
| `zihuan_graph_engine` | Synchronous graph runtime |
| `zihuan_llm` | LLM, Brain, embeddings, agent config models |
| `storage_handler` | Connection-backed nodes and storage helpers |
| `ims_bot_adapter` | QQ/IMS adapter integration |
| `zihuan_service` | Long-lived agent hosting |
| `src/api` | Web API and task orchestration |
| `webui/` | Browser UI |

## Error Handling

Use the shared result types from `zihuan_core`:

```rust
use zihuan_core::error::{Error, Result};
```

Prefer:

- `ValidationError` for invalid graph inputs, missing bindings, or type mismatches
- `ExecutionError` for runtime failures during work
- regular `?` propagation for I/O and integration errors

When the graph runtime wraps node failures, it already adds node ID and stage context. Avoid duplicating noisy prefixes unless they add real value.

## Logging

Use `log` macros:

```rust
log::info!("starting agent {}", agent_id);
log::warn!("connection missing, using fallback");
log::error!("graph execution failed: {}", err);
```

The logger is initialized by the main binary and forwarded to:

- console
- `./logs/`
- WebSocket clients
- task log storage when a task scope is active

Prefer concise, searchable messages.

## Browser UI Rules

Current UI code lives under `webui/src/`.

Structure:

- `admin/` for the Vue admin application
- `graph/` for LiteGraph editor/runtime helpers
- `api/` for browser-side API clients
- `ui/` for shared browser UI utilities
- `app/` for graph editor application state helpers

Do not write new docs or code as if `src/ui/` or Slint were the active frontend.

## Graph Runtime Rules

- The graph must remain a DAG
- New node behavior should be synchronous from the graph runtime's point of view
- Dynamic ports should be rebuilt from config in `apply_inline_config()`
- Reuse existing helpers before adding a new utility node
- Complexity should live inside nodes or services, not in graph topology

## Service Boundary

Do not reintroduce graph-owned long-lived execution models.

If behavior needs:

- a listener loop
- a hosted HTTP endpoint
- background message consumption
- auto-start lifecycle

it belongs in `zihuan_service` plus API/config plumbing, not in a new graph execution mode.
