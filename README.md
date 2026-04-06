# zihuan-next

**Node-graph workflow engine** for building event-driven AI pipelines — describe data flow on the graph, encapsulate complexity inside nodes.

The editor runs in your browser. The backend is a single Rust binary that serves the web UI and exposes a REST + WebSocket API for graph management and execution.

## Overview

zihuan-next uses a **node graph** to describe how data moves through a workflow. You can think of it as a simple flowchart made of typed processing blocks: data comes in, passes through a few steps, and produces an output.

> **The graph describes what data flows where. Complexity lives inside individual nodes.**

A workflow might look like:

`receive message → extract text → call model → format reply → send message`

Each node has clear **inputs** and **outputs** with declared data types. The graph focuses on the **big picture**: what data enters, how it is transformed, and where it goes. Agentic behavior (LLM loops, tool calling, retrieval) is encapsulated inside dedicated nodes — not visible as graph topology. When a problem becomes too complex to express cleanly on the canvas, create a new node or function subgraph rather than making the main graph more complicated.

### Key Capabilities

1. **Node Graphs for Data Flow** — Describe how data moves between steps; internal algorithms stay inside nodes.
2. **Strongly Typed Ports** — Every port has a declared type, giving each connection a clear contract.
3. **Browser-based Editor** — Litegraph.js canvas served by the Rust backend; no desktop installation needed.
4. **REST + WebSocket API** — All graph operations (load, save, run, stop, hyperparams) are accessible via API, making the engine easy to script or embed.
5. **Single Binary Deployment** — The frontend is compiled into the Rust binary via `rust-embed`; deploy one file.
6. **Function Subgraphs and Agent Tools** — A `function` node can package a private subgraph as one reusable step and expose it as a callable tool for LLM-driven nodes.
7. **Extensibility** — Add new nodes when a workflow needs new behavior; the graph stays simple.

## Architecture

### Crate Structure

| Crate | Contents |
|---|---|
| `crates/zihuan_core` | Error types, config loading, URL utilities |
| `crates/zihuan_bot_types` | `MessageEvent`, QQ message models, bot handle type |
| `crates/zihuan_llm_types` | `OpenAIMessage`, `LLMBase` trait, `FunctionTool` trait |
| `crates/zihuan_node` | `Node` trait, `DataType`/`DataValue`, DAG execution engine, general-purpose utility nodes, base node registry |
| `crates/zihuan_bot_adapter` | `BotAdapterNode`, QQ message send/receive nodes |
| `crates/zihuan_llm` | `LLMApiNode`, `LLMInferNode`, `BrainNode`, RAG nodes |
| `node_macros` | `node_input!`, `node_output!`, `port!` procedural macros |
| `src/` | Main binary: Salvo web server, REST/WebSocket API (`src/api/`), combined node registry (`src/init_registry.rs`) |
| `webui/` | Frontend: Vite + TypeScript + Litegraph.js; built into `webui/dist/` and embedded in the binary |

### How the stack works

- **Backend** (`src/`): Salvo HTTP server exposes REST endpoints under `/api/` and a WebSocket at `/ws`. The frontend and all static assets are served from the root route.
- **Frontend** (`webui/`): TypeScript SPA built with Vite. Communicates with the backend via `fetch` (REST) and a WebSocket connection that receives live execution events.
- **Embedding**: `cargo build` runs `pnpm run build` automatically (via `build.rs`), then `rust-embed` bakes `webui/dist/` into the binary. No separate web server or file deployment is needed.
- **Execution**: Graph execution runs on the backend. The frontend sends run/stop commands and receives status updates through WebSocket messages.

### Integration Components

- **Bot Adapter** (`crates/zihuan_bot_adapter`): Connects to QQ bot servers and turns incoming messages into workflow input.
- **LLM Integration** (`crates/zihuan_llm`): Nodes for model calls, tool-using AI behaviors, and retrieval features.
- **Message Store**: Caching and persistent history with Redis, MySQL, and in-memory fallback.

---

## Getting Started

### Prerequisites

- **Rust** (stable toolchain)
- **Node.js 18+** and **pnpm** — for the frontend build
- **Python 3.10+** — for database migrations (Alembic)
- **Redis** — for caching and message queue
- **MySQL** — for persistent storage

### Build from source

```bash
# 1. Clone
git clone https://github.com/FredYakumo/zihuan-next.git
cd zihuan-next

# 2. Install frontend dependencies (first time only)
cd webui && pnpm install && cd ..

# 3. Build — pnpm build runs automatically via build.rs
cargo build --release
```

### Configuration

```bash
cp config.yaml.example config.yaml
# Edit config.yaml: Bot Server URL, LLM endpoints, DB credentials
```

### Start infrastructure

```bash
docker compose -f docker/docker-compose.yaml up -d   # Redis (+ optional MySQL)

# DB migrations (Python required)
pip install alembic sqlalchemy mysqlclient
alembic upgrade head
```

### Run

```bash
# Default: listen on 127.0.0.1:8080
./target/release/zihuan_next

# Custom host/port
./target/release/zihuan_next --host 0.0.0.0 --port 9000

# Environment variables also work
ZIHUAN_HOST=0.0.0.0 ZIHUAN_PORT=9000 ./target/release/zihuan_next
```

Open your browser at `http://127.0.0.1:8080` (or the address you configured).

---

## Documentation

- **[User Guide](document/user-guide.md)** — Configuration and running the application
- **[Program Execution Flow](document/program-execute-flow.md)** — Backend startup and request lifecycle
- **[UI Architecture](document/dev-guides/ui-architecture.md)** — Web frontend and API design
- **[Node Graph JSON Specification](document/node/node-graph-json.md)** — JSON format for saving and loading node graphs
- **[Node Lifecycle & Execution](document/node/node-lifecycle.md)** — Node execution model, scheduling, and data flow
- **[Function Subgraphs](document/node/function-subgraphs.md)** — Embedded function subgraphs and reusable sub-pipelines
- **[Node Development Guide](document/node/node-development.md)** — Creating custom nodes and extending the system

---

## Development

### Build commands

```bash
# Build (also runs pnpm build for the frontend automatically)
cargo build
cargo build --release

# Frontend only (when iterating on web UI without recompiling Rust)
cd webui && pnpm run build

# Tests
cargo test
cargo test test_name

# Infrastructure
docker compose -f docker/docker-compose.yaml up -d
alembic upgrade head
alembic revision --autogenerate -m "description"
```

### Creating Custom Nodes

Nodes are the main extension point. If a workflow needs a new complex behavior, build a new node rather than making the graph more complicated.

1. Decide which crate the node belongs to:
   - General-purpose utility node → `crates/zihuan_node/src/util/`
   - Bot / QQ messaging node → `crates/zihuan_bot_adapter/src/`
   - LLM / AI node → `crates/zihuan_llm/src/`
2. Create a file for the node (one node per file) and implement the `Node` trait.
3. Export the node from the crate's `lib.rs` or parent `mod.rs`.
4. Register the node:
   - Nodes in `zihuan_node` → `crates/zihuan_node/src/registry.rs` inside `init_node_registry()`.
   - Nodes in `zihuan_bot_adapter` or `zihuan_llm` → `src/init_registry.rs`.

For detailed instructions, see the [Node Development Guide](document/node/node-development.md).

## License

AGPL-3.0 — See [LICENSE](LICENSE).
