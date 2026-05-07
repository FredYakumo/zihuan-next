# zihuan-next
> 🌐 English | [简体中文](README.zh-CN.md)

**A Rust-based node-graph workflow platform for AI agents, synchronous graph execution, and service-hosted runtimes such as QQ chat agents and HTTP stream agents.**

The graph focuses on **typed data flow**. Long-lived agent behavior is hosted by services, while reusable workflow logic stays in nodes and subgraphs.

<img width="1248" height="880" alt="image" src="https://github.com/user-attachments/assets/3b781e53-1fcf-4b77-91ba-2d63299181c4" />

## Overview

`zihuan-next` is now a multi-crate Rust workspace with two primary ways to use it:

- `zihuan_next`: the main web application. It serves the admin UI and graph editor, exposes REST/WebSocket APIs, manages system configuration, runs graphs as tasks, and hosts long-lived agents.
- `zihuan_graph_cli`: a CLI runner for executing a graph JSON file or a workflow-set graph directly from the terminal.

At the architecture level:

- `zihuan_graph_engine` executes graphs **synchronously** in DAG order.
- `zihuan_service` hosts long-lived agents such as QQ chat agents and HTTP stream agents.
- `zihuan_llm` provides LLM, Brain/tool, embedding, and retrieval-related nodes and agent helpers.
- `storage_handler` provides connection-backed nodes for MySQL, Redis, RustFS, Weaviate, and related persistence utilities.
- `webui/` contains the browser UI: Vue 3 admin pages at `/` and the LiteGraph-based editor at `/editor`.

## Current Capabilities

- Visual node-graph editor in the browser
- Synchronous DAG execution with typed ports
- Function subgraphs and Brain tool subgraphs
- Service-hosted agents with start/stop/auto-start lifecycle
- System-managed connections, LLM refs, and agent definitions
- Task execution, task logs, and WebSocket log streaming
- Local and remote embedding support
- QQ message storage and vector persistence helpers

## Screenshots

<img width="1248" height="880" alt="image" src="https://github.com/user-attachments/assets/01fae35b-3284-4081-b7f6-f5be5881dc1f" />
<img width="1248" height="880" alt="image" src="https://github.com/user-attachments/assets/d407db1c-2d5c-472e-8689-0ab636dbd7b8" />
<img width="1248" height="880" alt="image" src="https://github.com/user-attachments/assets/40e9d5dc-7383-4f7f-aded-52640edeed8e" />
<img width="1248" height="880" alt="QQ_1774525136280" src="https://github.com/user-attachments/assets/7cc1f27d-9556-4bd7-8741-05904c536490" />
<img width="1248" height="880" alt="6e9a6276770f6a190161b14577ebeb7f" src="https://github.com/user-attachments/assets/6d56ffd6-846f-4ced-9d98-0f57bb8f7d31" />
<img width="2382" height="1647" alt="c5872ca13db7d67512a625e9dae1a601" src="https://github.com/user-attachments/assets/2409f7a6-94a9-46a1-aca8-d21c0fa4347c" />
<img width=600 src="https://github.com/user-attachments/assets/0d25ce93-0f97-4d8c-8375-63b99f6dcd14" />
<img width="1080" src="https://github.com/user-attachments/assets/60b3b145-7ce7-4a76-9742-b975578a9556" />
<img width="1080" src="https://github.com/user-attachments/assets/137e4808-5ce3-4714-a0e3-6f5ddaf9f9cb" />
<img width="1440" src="https://github.com/user-attachments/assets/994472eb-2d37-4160-811d-c5b4856e3239" />
<img width=600 src="https://github.com/user-attachments/assets/12c27199-2b1e-41ab-8215-0baced40dff9" />
<img width=600 src="https://github.com/user-attachments/assets/b30bcef5-cb81-4173-8aa9-cefa5da9e690" />
<img width=600 src="https://github.com/user-attachments/assets/91da8e34-6feb-4c7b-be45-efd8bf599d1f" />

## Workspace Layout

| Package | Responsibility |
|---|---|
| `zihuan_core` | Shared error types, system config, adapter models, LLM model types |
| `zihuan_graph_engine` | Graph runtime, node registry, graph JSON loading, base nodes |
| `zihuan_llm` | LLM nodes, Brain/tool runtime, embeddings, RAG helpers, agent config models |
| `storage_handler` | Connection configs plus Redis/MySQL/RustFS/Weaviate nodes and helpers |
| `ims_bot_adapter` | QQ/IMS adapter client and adapter-facing nodes |
| `zihuan_service` | Long-lived agent runtime and scheduling support |
| `zihuan_graph_cli` | Terminal graph executor |
| `webui/` | Vue 3 admin UI and `/editor` graph editor |
| `src/` | Main Salvo server binary and HTTP/WebSocket API |

## Quick Start

### Requirements

- Rust stable
- Node.js 18+
- `pnpm`

Optional, depending on what you use:

- Redis
- RustFS
- Weaviate
- MySQL

### Build the web application

```bash
git clone https://github.com/FredYakumo/zihuan-next.git
cd zihuan-next
git submodule update --init --recursive

cd webui
pnpm install
cd ..

cargo build --release
```

`build.rs` embeds `webui/dist/` into the main binary, so the frontend build must succeed for `zihuan_next`.

### Run the web application

```bash
docker compose -f docker/docker-compose.yaml up -d

./target/release/zihuan_next
# serves http://127.0.0.1:9951 by default
```

Custom bind:

```bash
./target/release/zihuan_next --host 0.0.0.0 --port 9000
```

The bundled compose file starts:

- Redis on `127.0.0.1:6379`
- RustFS on `127.0.0.1:9000` with console on `127.0.0.1:9001`
- Weaviate on `127.0.0.1:8080`

It does **not** start MySQL.

### Build and run the CLI

```bash
cargo build -p zihuan_graph_cli --release

./target/release/zihuan_graph_cli --file workflow_set/qq_agent_example.json
./target/release/zihuan_graph_cli --workflow qq_agent_example
```

The CLI initializes the same node registry extensions as the web app's graph runtime, then loads the graph and executes it once.

## Configuration Model

The project currently has three distinct configuration layers:

### 1. System configuration

Managed by the web app and stored as JSON under the user config directory:

- Windows: `%APPDATA%/zihuan-next_aibot/system_config/system_config.json`
- Linux/macOS: `$XDG_CONFIG_HOME` or `$HOME/.config/zihuan-next_aibot/system_config/system_config.json`

Stored sections:

- `connections`
- `llm_refs`
- `agents`

### 2. Graph definition and graph-local values

Graph structure, inline values, variables, metadata, and subgraphs are stored in graph JSON files. Workflow-set files live under `workflow_set/`.

### 3. Alembic migration config

`config.yaml` is only used by the Python migration toolchain for MySQL schema setup. The Rust runtime does **not** read `config.yaml`.

## Optional GPU Build For Local Embeddings

Root features forward to `zihuan_llm`:

```bash
cargo build --release --features candle-cuda
cargo build --release --features candle-metal
```

Windows helper:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\cargo-cuda.ps1 -Release build
```

The local embedding loader prefers `CUDA -> Metal -> CPU` and falls back automatically if GPU initialization fails.

## Local Embedding Models

Local embedding nodes load from:

```text
models/text_embedding/<model_name>/
```

Example:

```bash
pip install huggingface_hub
hf download Qwen/Qwen3-Embedding-0.6B \
  --local-dir models/text_embedding/Qwen3-Embedding-0.6B
```

The directory name must match the `model_name` configured in the graph or node config.

## MySQL Schema Setup

Only needed if you use the MySQL-backed message store.

```bash
cp config.yaml.example config.yaml
uv sync
uv run alembic upgrade head
```

`config.yaml.example` documents the required `MYSQL_*` fields.

## Documentation

- [User Guide](document/user-guide.md)
- [Program Execution Flow](document/program-execute-flow.md)
- [Developer Guide Index](document/dev-guides/README.md)
- [UI Architecture](document/dev-guides/ui-architecture.md)
- [Node Lifecycle](document/node/node-lifecycle.md)
- [Function Subgraphs](document/node/function-subgraphs.md)
- [Node Development Guide](document/node/node-development.md)

## License

AGPL-3.0. See [LICENSE](LICENSE).
