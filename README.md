# zihuan-next
> 🌐 English | [简体中文](README.zh-CN.md)

**An AI agent runner engine that enable to describe simple or event-driven agentic workflows using a node graph — get an AI Agent running in just a few steps, you can easily and precisely define dataflow, and with clear visibility into how data flows through the pipeline.**

Ships with a collection of ready-to-use AI Agent templates.

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

### Option A: Download pre-built binary

Download the latest release from the [Releases page](https://github.com/FredYakumo/zihuan-next/releases/latest)., extract the archive, and run the executable directly — no build tools required.

### Option B: Build from source

Requirements: **Rust** (stable), **Node.js 18+** and **pnpm**.

```bash
# 1. Clone
git clone https://github.com/FredYakumo/zihuan-next.git
cd zihuan-next

# 2. Install frontend dependencies (first time only)
cd webui && pnpm install && cd ..

# 3. Build — pnpm build runs automatically via build.rs
cargo build --release
```

### Optional: Enable GPU acceleration for local Candle embedding models

The local text embedding loader can automatically use GPU acceleration when the binary is compiled with the matching feature and a supported runtime is available. If GPU initialization or inference fails, it falls back to CPU automatically.

- `candle-cuda`: Linux/Windows builds with CUDA toolkit and driver available.
- `candle-metal`: macOS builds with Metal available.
- No feature: CPU-only build.

Examples:

```bash
# CUDA build
cargo build --release --features candle-cuda

# Metal build (macOS)
cargo build --release --features candle-metal
```

Notes:

- `candle-cuda` requires a working CUDA toolchain; if `nvcc` is missing, Cargo will fail during dependency build.
- The runtime chooses `CUDA -> Metal -> CPU` based on compiled features and availability.

### Download a local text embedding model

Local embedding nodes load models from `models/text_embedding/<model_name>/`. The repository does not include model weights; you must download them separately.

Example — downloading **Qwen3-Embedding-0.6B** with the HuggingFace CLI:

```bash
# Install huggingface_hub (once)
pip install huggingface_hub

# Download the model into the expected directory
huggingface-cli download Qwen/Qwen3-Embedding-0.6B \
  --local-dir models/text_embedding/Qwen3-Embedding-0.6B \
  --local-dir-use-symlinks False
```

Alternative using `git lfs`:

```bash
git lfs clone https://huggingface.co/Qwen/Qwen3-Embedding-0.6B \
  models/text_embedding/Qwen3-Embedding-0.6B
```

The directory name under `models/text_embedding/` must match the `model_name` value you configure in the node graph (e.g. `Qwen3-Embedding-0.6B`).

### Run

```bash
docker compose -f docker/docker-compose.yaml up -d   # Redis + RustFS (+ optional MySQL)

./target/release/zihuan_next                         # http://127.0.0.1:8080
./target/release/zihuan_next --host 0.0.0.0 --port 9000
```

The bundled compose file also starts RustFS object storage on `127.0.0.1:9000` with the console on `127.0.0.1:9001`. Override the default credentials by exporting `RUSTFS_ACCESS_KEY` and `RUSTFS_SECRET_KEY` before `docker compose up`.

Configuration values (API keys, etc.) are managed as **hyperparameters** inside the node graph — open the Hyperparameters panel in the editor to set them.

### Initialize the MySQL schema (optional)

Required only if you plan to use the MySQL message store. Skip this step if you are running with Redis-only persistence.

1. Copy `config.yaml.example` to `config.yaml` and fill in the `MYSQL_*` fields with the credentials of your local MySQL instance.
2. Install the Python toolchain ([uv](https://docs.astral.sh/uv/) recommended) and create the virtualenv.
3. Run Alembic to create / upgrade the schema:

```bash
cp config.yaml.example config.yaml          # then edit MYSQL_* fields
uv sync                                     # install Python deps (alembic, sqlalchemy, pymysql)
uv run alembic upgrade head                 # apply all migrations to the configured database
```

The MySQL connection used by Alembic is built from the `MYSQL_*` fields in `config.yaml`; the Rust runtime itself does **not** read these fields and accepts a database URL through the relevant node input port at runtime.

### Build commands

```bash
cargo build
cargo build --release

# GPU-enabled local embedding builds
cargo build --features candle-cuda
cargo build --release --features candle-cuda
cargo build --features candle-metal
cargo build --release --features candle-metal

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
