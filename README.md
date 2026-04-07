# zihuan-next
> 🌐 English | [简体中文](README.zh-CN.md)
**An AI agent runner engine that lets you describe simple or event-driven agentic workflows using a node graph — get an AI Agent running in just a few steps, with clear visibility into how data flows through the pipeline. Ships with a collection of ready-to-use AI Agent templates.**

<img width="1248" height="880" alt="image" src="https://github.com/user-attachments/assets/3b781e53-1fcf-4b77-91ba-2d63299181c4" />

## Overview

zihuan-next uses a **node graph** to describe how data moves through a workflow. You can think of it as a simple flowchart made of typed processing blocks: data comes in, passes through a few steps, and produces an output.

> **The graph describes what data flows where. Complexity lives inside individual nodes.**

A workflow might look like:

`receive message → extract text → call model → format reply → send message`

Each node has clear **inputs** and **outputs** with declared data types. The graph focuses on the **big picture**: what data enters, how it is transformed, and where it goes. Agentic behavior (LLM loops, tool calling, retrieval) is encapsulated inside dedicated nodes — not visible as graph topology. When a problem becomes too complex to express cleanly on the canvas, create a new node or function subgraph rather than making the main graph more complicated.

---

The editor runs in your browser. The backend is a single Rust binary ([Salvo](https://salvo.rs)) that serves the web UI and exposes a REST + WebSocket API. The frontend is a TypeScript SPA built with [Vite](https://vitejs.dev) and [Litegraph.js](https://github.com/Comfy-Org/litegraph.js).

### Key Capabilities

1. **Node Graphs for Data Flow** — Describe how data moves between steps; internal algorithms stay inside nodes.
2. **Strongly Typed Ports** — Every port has a declared type, giving each connection a clear contract.
3. **Single Binary** — Only One binary application.
4. **Function Subgraphs and Agent Tools** — A `function` node can package a private subgraph as one reusable step and expose it as a callable tool for LLM-driven nodes.
5. **Extensibility** — Add new nodes when a workflow needs new behavior; the graph stays simple.

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

---

## Getting Started

### Build from source

Requirements: **Rust** (stable), **Node.js 18+** and **pnpm**, **Redis**, **MySQL**.

```bash
# 1. Clone
git clone https://github.com/FredYakumo/zihuan-next.git
cd zihuan-next

# 2. Install frontend dependencies (first time only)
cd webui && pnpm install && cd ..

# 3. Build — pnpm build runs automatically via build.rs
cargo build --release
```

### Configure and run

```bash
cp config.yaml.example config.yaml
# Edit config.yaml: Bot Server URL, LLM endpoints, DB credentials

docker compose -f docker/docker-compose.yaml up -d   # Redis (+ optional MySQL)

./target/release/zihuan_next                         # http://127.0.0.1:8080
./target/release/zihuan_next --host 0.0.0.0 --port 9000
```

### Build commands

```bash
cargo build
cargo build --release

# Frontend only
cd webui && pnpm run build

# Tests
cargo test
```

---

## Documentation

- **[User Guide](document/user-guide.md)** — Configuration and running the application
- **[Program Execution Flow](document/program-execute-flow.md)** — Backend startup and request lifecycle
- **[UI Architecture](document/dev-guides/ui-architecture.md)** — Web frontend and API design
- **[Node Graph JSON Specification](document/node/node-graph-json.md)** — JSON format for saving and loading node graphs
- **[Node Lifecycle & Execution](document/node/node-lifecycle.md)** — Node execution model, scheduling, and data flow
- **[Function Subgraphs](document/node/function-subgraphs.md)** — Embedded function subgraphs and reusable sub-pipelines
- **[Node Development Guide](document/node/node-development.md)** — Creating custom nodes and extending the system

## License

AGPL-3.0 — See [LICENSE](LICENSE).
