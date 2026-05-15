# zihuan-next

> English | [简体中文](README.zh-CN.md)

`zihuan-next` is a Rust-based Agent service platform built around two ideas:

- Agents run as persistent services.
- Node graphs define reusable workflows and tools.

The graph stays focused on data flow. Long-lived behavior such as chat agents, HTTP-facing agents, task hosting, connection reuse, and runtime orchestration is hosted by the service layer.

<img width="1248" height="880" alt="zihuan-next" src="https://github.com/user-attachments/assets/3b781e53-1fcf-4b77-91ba-2d63299181c4" />

## Quick Start

### Requirements

- Rust stable
- Node.js 18+
- `pnpm`

Optional services, depending on your setup:

- MySQL
- Redis
- Weaviate
- RustFS

### Build

```bash
git clone https://github.com/FredYakumo/zihuan-next.git
cd zihuan-next
git submodule update --init --recursive

cd webui
pnpm install
cd ..

cargo build --release
```

The main binary embeds the frontend bundle from `webui/dist/`.

### Run

```bash
docker compose -f docker/docker-compose.yaml up -d
./target/release/zihuan_next
```

Default address:

```text
http://127.0.0.1:9951
```

Custom bind:

```bash
./target/release/zihuan_next --host 0.0.0.0 --port 9000
```

### First-time usage flow

After startup, the usual order is:

1. Open `/` and create `connections`.
2. Create `llm_refs`.
3. Create or import workflow graphs in `/editor`.
4. Mount graph-backed tools into an Agent.
5. Start the Agent or run the graph as a task.

## Highlights

- Simple Agent capabilities are available out of the box.
- Node graphs are used to design and reuse more complex workflows.
- The same workflow can run directly as a task or be exposed as an Agent tool.
- Connections and model refs are configured once and reused across Agents and graphs.

### CLI graph runner

```bash
cargo build -p zihuan_graph_cli --release

./target/release/zihuan_graph_cli --file workflow_set/qq_agent_example.json
./target/release/zihuan_graph_cli --workflow qq_agent_example
```

## How You Use It

### 1. Configure shared resources

In the admin UI, create:

- `connections`
- `llm_refs`
- `agents`

These are stored in the system config file under a unified config center.

### 2. Build a workflow graph

Use `/editor` to define:

- workflow steps
- node parameters
- function subgraphs
- tool subgraphs
- graph-local variables and inline values

### 3. Attach the workflow to an Agent

Use graph-backed tools in an Agent definition so the Agent can call them during inference. Simple Agent behavior can stay lightweight, while more complex multi-step logic can be moved into reusable graph workflows.

### 4. Operate the runtime

From the admin UI you can:

- inspect tasks
- watch logs
- manage saved connections
- inspect runtime connection instances
- start or stop agents

## Screenshots

<img width="1248" height="880" alt="main-ui" src="https://github.com/user-attachments/assets/01fae35b-3284-4081-b7f6-f5be5881dc1f" />
<img width="1248" height="880" alt="graph-editor" src="https://github.com/user-attachments/assets/d407db1c-2d5c-472e-8689-0ab636dbd7b8" />
<img width="1248" height="880" alt="workflow" src="https://github.com/user-attachments/assets/40e9d5dc-7383-4f7f-aded-52640edeed8e" />
<img width="1248" height="880" alt="qq" src="https://github.com/user-attachments/assets/7cc1f27d-9556-4bd7-8741-05904c536490" />
<img width="1248" height="880" alt="agent" src="https://github.com/user-attachments/assets/6d56ffd6-846f-4ced-9d98-0f57bb8f7d31" />
<img width="2382" height="1647" alt="editor-large" src="https://github.com/user-attachments/assets/2409f7a6-94a9-46a1-aca8-d21c0fa4347c" />
<img width="600" alt="shot-1" src="https://github.com/user-attachments/assets/0d25ce93-0f97-4d8c-8375-63b99f6dcd14" />
<img width="1080" alt="shot-2" src="https://github.com/user-attachments/assets/60b3b145-7ce7-4a76-9742-b975578a9556" />
<img width="1080" alt="shot-3" src="https://github.com/user-attachments/assets/137e4808-5ce3-4714-a0e3-6f5ddaf9f9cb" />
<img width="1440" alt="shot-4" src="https://github.com/user-attachments/assets/994472eb-2d37-4160-811d-c5b4856e3239" />
<img width="600" alt="shot-5" src="https://github.com/user-attachments/assets/12c27199-2b1e-41ab-8215-0baced40dff9" />
<img width="600" alt="shot-6" src="https://github.com/user-attachments/assets/b30bcef5-cb81-4173-8aa9-cefa5da9e690" />
<img width="600" alt="shot-7" src="https://github.com/user-attachments/assets/91da8e34-6feb-4c7b-be45-efd8bf599d1f" />

## What This Project Is

`zihuan-next` combines:

- a persistent Agent runtime
- a browser-based workflow editor
- a synchronous DAG graph engine
- a shared tool-call loop for agents and graph tools
- a unified configuration center for connections, model refs, and agents

In practice, you use it in three connected ways:

1. Run agents as always-on services.
2. Build workflows with the node graph editor.
3. Expose those workflows as callable tools for agents.

This keeps graph topology simple while allowing complex behavior to live inside nodes, subgraphs, and agent tool loops.

## Core Model

### 1. Agent service is the primary runtime

The main binary hosts long-lived agents such as:

- `qq_chat`
- `http_stream`

Agents can be enabled, disabled, started, stopped, and auto-started from the admin UI. They are not one-shot scripts; they are hosted services managed by the server runtime.

### 2. Node graphs are workflows

The graph engine executes a DAG synchronously. A graph run is ideal for:

- data transformation
- message processing
- retrieval and storage steps
- calling models
- preparing tool results
- encapsulating business logic in reusable subgraphs

The graph is intentionally not the place for long-lived listeners or service lifecycles.

### 3. Workflows can also become Agent tools

This is a central design point of `zihuan-next`.

The same node-graph logic can be used in two roles:

- run directly as a workflow
- mounted into an Agent as a callable tool

Agents can call graph-backed tools through the shared Brain/tool loop. This makes workflows reusable across interactive agents, service endpoints, and graph-driven automations without rewriting the same logic twice.

## Unified Connections And Resources

Connections are first-class system configuration, not ad-hoc values hidden inside one workflow.

You define connection configs once in the admin UI, then reuse them from both:

- agents
- node graphs

Current resource types in the project include:

- MySQL
- Redis
- Weaviate
- RustFS / S3-style object storage
- IMS Bot Adapter connections
- Tavily

The runtime distinguishes between:

- persistent connection configuration identified by `config_id`
- live runtime connection instances identified by `instance_id`

Graphs and agents refer to `config_id`. The runtime creates or reuses live instances as needed. This makes database and service connections easy to manage centrally while still being directly consumable from graph nodes and agent runtimes.

## Model Access

`zihuan-next` supports several ways to use LLM and embedding capabilities:

- local inference with Candle-based models
- local or self-hosted inference through `llama.cpp`
- online model APIs
- OpenAI Chat Completions compatible endpoints
- OpenAI Responses compatible endpoints

Model endpoints are defined as reusable `llm_refs` in system configuration, then attached where needed by agents or graphs.

This allows one deployment to mix:

- local inference for cost control or privacy
- self-hosted inference for internal services
- hosted APIs for general-purpose reasoning

## Main Capabilities

- Browser admin UI at `/`
- Browser graph editor at `/editor`
- Persistent agent hosting
- Graph execution as task runs
- Graph-backed Agent tools
- Shared Brain tool-call loop
- Reusable connection and model configuration
- REST API and WebSocket event stream
- Task logs and runtime inspection
- Workflow-set loading and CLI execution

## Workspace Layout

| Package | Responsibility |
|---|---|
| `zihuan_core` | Shared types, config, errors |
| `zihuan_agent` | Brain tool-call loop engine |
| `zihuan_graph_engine` | Synchronous DAG graph runtime |
| `model_inference` | LLM, embedding, and inference-related nodes |
| `storage_handler` | Connection-backed resource nodes and runtime connection management |
| `ims_bot_adapter` | IMS / QQ adapter integration |
| `zihuan_service` | Long-lived agent hosting and agent-facing nodes |
| `zihuan_graph_cli` | CLI graph runner |
| `webui/` | Vue admin UI and LiteGraph editor |
| `src/` | Main Salvo web server, API, and app runtime |

## Configuration Model

System-level configuration is stored in:

- Windows: `%APPDATA%/zihuan-next_aibot/system_config/system_config.json`
- Linux/macOS: `$XDG_CONFIG_HOME` or `$HOME/.config/zihuan-next_aibot/system_config/system_config.json`

Current shape:

```json
{
  "version": 2,
  "configs": {
    "connections": [],
    "llm_refs": [],
    "agents": []
  }
}
```

Graph structure, inline values, variables, and embedded subgraphs live in graph JSON files or workflow-set files under `workflow_set/`.

`config.yaml` is only used by the Python Alembic migration flow for MySQL schema setup.

## Documentation

- [User Guide](document/user-guide.md)
- [Program Execution Flow](document/program-execute-flow.md)
- [Configuration And Connection Instances](document/config-and-connection-instances.md)
- [Node System](document/dev-guides/node-system.md)
- [Code Conventions](document/dev-guides/code-conventions.md)
- [UI Architecture](document/dev-guides/ui-architecture.md)
- [Function Subgraphs](document/node/function-subgraphs.md)
- [Node Development Guide](document/node/node-development.md)
- [Brain](document/llm/brain.md)

## License

AGPL-3.0. See [LICENSE](LICENSE).
