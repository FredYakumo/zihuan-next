# Developer Guides

This index describes the project as it exists now: a browser-based management/editor frontend, a synchronous graph runtime, and a separate service runtime for long-lived agents.

## Architecture Summary

| Layer | Responsibility |
|---|---|
| `zihuan_graph_engine` | Synchronous DAG graph execution, graph JSON loading, base registry, reusable runtime primitives |
| `zihuan_llm` | LLM nodes, Brain/tool runtime, embedding support, RAG helpers, agent config models |
| `storage_handler` | Connection config models, storage-backed nodes, object storage helpers, message storage helpers |
| `zihuan_service` | Long-lived agent runtime such as QQ chat and HTTP stream agents |
| `src/api` | REST API, WebSocket events, task orchestration, graph session management, system config endpoints |
| `webui/` | Vue 3 admin UI at `/` and LiteGraph-based editor at `/editor` |

## Current Workspace Packages

| Package | Contents |
|---|---|
| `zihuan_core` | Shared error types, system config helpers, adapter models, LLM model types |
| `zihuan_graph_engine` | `Node`, `NodeGraph`, graph JSON loading, utility nodes, execution runtime |
| `zihuan_llm` | LLM nodes, Brain, embeddings, RAG, `AgentType` and LLM ref config models |
| `storage_handler` | Connection config sections and nodes for Redis/MySQL/RustFS/Weaviate/Tavily |
| `ims_bot_adapter` | QQ/IMS adapter client and adapter-oriented nodes |
| `zihuan_service` | `AgentManager`, agent runtime status, scheduled task support |
| `zihuan_graph_cli` | Command-line graph execution binary |
| `src/` | Main Salvo server binary, API, router, log forwarding |
| `webui/` | Browser UI code |

## Runtime Entry Points

### Main web application

`src/main.rs`:

- initializes logging
- initializes the merged node registry
- creates `AppState`
- restores system config
- auto-starts enabled agents
- serves the browser UI and HTTP/WebSocket APIs

Auto-start restores agent runtime only. It does not create task-list entries by itself. Task entries for agents are created later per handled response/request.

### CLI graph runner

`zihuan_graph_cli/src/main.rs`:

- initializes the node registry
- loads a graph file or workflow-set graph
- builds `NodeGraph`
- executes once and exits

## Current UI Split

- `/` -> Vue 3 admin UI
- `/editor` -> LiteGraph-based graph editor

There is no desktop GUI runtime in the current architecture.

## System Configuration

System config is persisted as JSON in the user config directory and currently stores:

- `connections`
- `llm_refs`
- `agents`

Root helpers live in `zihuan_core::system_config`. Section-specific schemas live with their owning domains:

- `storage_handler` for `connections`
- `zihuan_llm::system_config` for `llm_refs` and `agents`

## Where To Start

| Goal | Read first |
|---|---|
| Understand the graph runtime | [node-system.md](./node-system.md) |
| Understand execution lifecycle | [../node/node-lifecycle.md](../node/node-lifecycle.md) |
| Build a new node | [../node/node-development.md](../node/node-development.md) |
| Understand graph JSON | [../node/node-graph-json.md](../node/node-graph-json.md) |
| Understand function/tool subgraphs | [../node/function-subgraphs.md](../node/function-subgraphs.md) |
| Understand Brain/tool behavior | [../llm/brain.md](../llm/brain.md) |
| Understand browser UI/API boundaries | [ui-architecture.md](./ui-architecture.md) |
| Look up naming and file layout rules | [code-conventions.md](./code-conventions.md) |
| Understand logging and task log forwarding | [logging.md](./logging.md) |

## Guide Index

| Document | Contents |
|---|---|
| [node-system.md](./node-system.md) | Current `Node` trait and synchronous runtime model |
| [node-types.md](./node-types.md) | What `NodeType::Simple` means now |
| [ui-architecture.md](./ui-architecture.md) | Browser UI, routes, API boundaries, and graph editor structure |
| [code-conventions.md](./code-conventions.md) | Naming, file placement, shared utilities, and architecture rules |
| [logging.md](./logging.md) | Global logger, WebSocket forwarding, and task-scoped logs |
| [qq-message.md](./qq-message.md) | QQ message model details |
| [qq_message_storage.md](./qq_message_storage.md) | Message storage schema and persistence path |

## Architectural Rule

Keep this boundary sharp:

- graph complexity belongs inside nodes and subgraphs
- long-lived behavior belongs in `zihuan_service`
- browser UI owns presentation, not authoritative execution state
