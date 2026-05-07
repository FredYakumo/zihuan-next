# User Guide

This guide explains how to run `zihuan-next` in its current form: a web application plus an optional CLI graph runner.

## What You Actually Run

There are two user-facing binaries:

- `zihuan_next`: the main web application
- `zihuan_graph_cli`: a terminal graph executor

Most users will start with `zihuan_next`.

## 1. Build The Project

Requirements:

- Rust stable
- Node.js 18+
- `pnpm`

Build steps:

```bash
git clone <repository-url>
cd zihuan-next
git submodule update --init --recursive

cd webui
pnpm install
cd ..

cargo build --release
```

The main binary embeds the built frontend assets from `webui/dist/`.

## 2. Start Supporting Services

The bundled Docker Compose file starts the default local dependencies used by many graphs and agents:

```bash
docker compose -f docker/docker-compose.yaml up -d
```

Services included:

- Redis
- RustFS
- Weaviate

MySQL is not included in the compose file. If you need MySQL-backed message storage, start MySQL separately.

## 3. Run The Web Application

Default:

```bash
./target/release/zihuan_next
```

By default the server binds to:

```text
127.0.0.1:9951
```

Custom host/port:

```bash
./target/release/zihuan_next --host 0.0.0.0 --port 9000
```

Environment equivalents:

- `ZIHUAN_HOST`
- `ZIHUAN_PORT`

## 4. Open The UI

The web app serves two browser entry points:

- `/` -> Vue 3 admin UI
- `/editor` -> graph editor

The admin UI is where you manage:

- connections
- LLM refs
- agents
- saved graph sessions
- task history and logs

The graph editor is where you build and run node graphs.

## 5. Understand The Configuration Model

### System configuration

The application stores system-level JSON config in:

- Windows: `%APPDATA%/zihuan-next_aibot/system_config/system_config.json`
- Linux/macOS: `$XDG_CONFIG_HOME` or `$HOME/.config/zihuan-next_aibot/system_config/system_config.json`

Current on-disk shape:

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

The core config collections are still:

- `connections`
- `llm_refs`
- `agents`

The shared primary key is:

- `config_id`

Legacy top-level `connections` / `llm_refs` / `agents` data is migrated automatically when read.

### Graph files

Graph structure, inline values, variables, metadata, and embedded subgraphs live in graph JSON files.

Workflow-set files are stored under:

```text
workflow_set/
```

### `config.yaml`

`config.yaml` is only for Python Alembic migrations. The Rust runtime does not use it.

## 6. Set Up Connections And Agents

In the admin UI:

1. Create connection records for Redis, MySQL, RustFS, Weaviate, Tavily, or bot adapters.
2. Create LLM refs for reusable model endpoints.
3. Create agents if you want long-lived service-hosted runtimes such as QQ chat or HTTP stream agents.

In the current UI:

- connection configs, model configs, and agents all use `config_id` as the primary key
- long IDs are shortened in cards, for example `abcd1234...`
- when one connection config owns multiple live instances, the UI may render it as `abcd1234..., and N total`

Enabled agents with `auto_start = true` are started automatically when `zihuan_next` boots.

If you need the distinction between saved connection definitions and live runtime instances, see:

- [Configuration And Connection Instances](./config-and-connection-instances.md)

## 7. Work With Graphs

You can:

- create and edit graphs in `/editor`
- open and save workflow-set graphs
- execute a graph through the web UI
- rerun saved-file tasks from the task list

Execution from the web app creates a task entry and streams logs/events over WebSocket.

## 8. Run A Graph From The CLI

Build the CLI:

```bash
cargo build -p zihuan_graph_cli --release
```

Run by file path:

```bash
./target/release/zihuan_graph_cli --file workflow_set/qq_agent_example.json
```

Run by workflow-set name:

```bash
./target/release/zihuan_graph_cli --workflow qq_agent_example
```

The CLI loads the graph, builds a `NodeGraph`, executes it once, and exits.

## 9. Optional: MySQL Schema Migration

Only required for MySQL-backed message storage.

```bash
cp config.yaml.example config.yaml
uv sync
uv run alembic upgrade head
```

The migration connection is built from `MYSQL_*` fields in `config.yaml`.

## 10. Optional: GPU Build For Local Embeddings

CUDA:

```bash
cargo build --release --features candle-cuda
```

Metal:

```bash
cargo build --release --features candle-metal
```

Windows helper:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\cargo-cuda.ps1 -Release build
```

At runtime the local embedding loader prefers `CUDA -> Metal -> CPU`.
