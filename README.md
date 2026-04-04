# zihuan-next

**Node-graph dataflow engine** for building event-driven bot pipelines with composable, self-contained processing nodes.

<img width="1248" height="880" alt="QQ_1774524965499" src="https://github.com/user-attachments/assets/ac06d18e-bf36-4ae8-893a-45ee9e36f475" />

## Overview

zihuan-next uses a **node graph** to describe data flow between processing steps. The key design principle is:

> **The graph describes what data flows where. Complexity lives inside individual nodes.**

Each node in the graph is a self-contained unit with clearly typed input and output ports. The graph topology stays simple and readable — it shows the high-level flow of data through the pipeline. All algorithms, agentic reasoning loops, control flow, and other complex logic are fully encapsulated within individual nodes. When you encounter a new complex problem, the answer is to build a new dedicated node rather than to introduce complex wiring on the graph canvas.

While currently featuring strong support for QQ bots, the engine is designed as a **universal workflow engine**. Workflows are stored as JSON and can run on **desktop**, **server**, or **edge devices** without modification.

### Key Capabilities

1.  **Simple Data Flow**: The graph is intentionally kept flat and readable. Routing, filtering, and composition happen through port connections, not nested logic on the canvas.
2.  **Self-Contained Nodes**: Complex behaviors such as LLM inference, agentic tool-use loops, chat session management, and RAG retrieval are each fully encapsulated in a single node. Adding capability means adding a node.
3.  **Function Subgraphs**: A `function` node embeds a private subgraph and exposes a typed call signature. This allows reusable sub-pipelines to be packaged as a single callable node, keeping the outer graph uncluttered.
4.  **Chat Memory**: Maintains conversational context for coherent multi-turn interactions.
5.  **Hybrid Knowledge Retrieval**: Augments responses using vector database knowledge graphs fused with real-time group chat context.
6.  **Web-Augmented Dialogue**: Enhances conversations with live web search results via Tavily.
7.  **Rich Content Rendering**: Generates display-ready images from Markdown, code blocks, and LaTeX formulas.
8.  **Multi-Platform Support**: Workflows run headless on servers or edge devices by loading a JSON file. No GUI required.

## Architecture

### Crate Structure

The engine is split into focused library crates:

| Crate | Contents |
|---|---|
| `crates/zihuan_core` | Error types, config loading, URL utilities |
| `crates/zihuan_bot_types` | `MessageEvent`, QQ message models, bot handle type |
| `crates/zihuan_llm_types` | `OpenAIMessage`, `LLMBase` trait, `FunctionTool` trait |
| `crates/zihuan_node` | `Node` trait, `DataType`/`DataValue`, DAG execution engine, general-purpose utility nodes, base node registry |
| `crates/zihuan_bot_adapter` | `BotAdapterNode`, QQ message send/receive nodes |
| `crates/zihuan_llm` | `LLMApiNode`, `LLMInferNode`, `BrainNode`, RAG nodes |
| `node_macros` | `node_input!`, `node_output!`, `port!` procedural macros |
| `src/` | Main binary: Slint UI, combined node registry (`init_registry.rs`) |

### Node Graph Engine (`crates/zihuan_node`)
- **DAG Execution**: Nodes execute in topological order. Data flows through typed ports.
- **Port-Based Connections**: Input and output ports carry a declared `DataType`. Connections require matching types.
- **Two Node Models**: `Simple` nodes run once per activation; `EventProducer` nodes run a persistent loop (e.g. WebSocket listener).
- **Function Subgraphs**: A `function` node owns a private embedded subgraph executed as a child graph, enabling reusable encapsulated pipelines.

### Integration Components
- **Bot Adapter** (`crates/zihuan_bot_adapter`): WebSocket handling for QQ bot servers. Inbound events become `MessageEvent` values flowing into the graph.
- **LLM Integration** (`crates/zihuan_llm`): `LLMInferNode` for one-shot inference; `BrainNode` for agentic tool-use loops with embedded tool subgraphs.
- **Message Store**: Three-tier storage architecture (Redis cache → MySQL persistence → in-memory fallback).
- **Visual Editor** (`src/ui/`): Slint-based drag-and-drop workflow editor.

## Screenshots
<img width="1248" height="880" alt="QQ_1774524965499" src="https://github.com/user-attachments/assets/ac06d18e-bf36-4ae8-893a-45ee9e36f475" />

<img width="1248" height="880" alt="QQ_1774525104876" src="https://github.com/user-attachments/assets/406cce0d-7a19-41ae-8ce3-650de04b1409" />

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

### Prerequisites
- **Python 3.10+**: For database migrations (alembic).
- **Redis**: For caching and message queue.
- **MySQL**: For persistent storage.

### Installation

1.  **Download the Application**
    - Go to the [Releases page](https://github.com/FredYakumo/zihuan-next/releases)
    - Download the latest version for your OS
    - Extract the archive to your preferred location

2.  **Configuration**
    Copy the example config and edit it with your environment details (Bot Server URL, LLM endpoints, DB credentials).
    ```bash
    cp config.yaml.example config.yaml
    ```

3.  **Start Infrastructure**
    Start Redis using Docker:
    ```bash
    docker compose -f docker/docker-compose.yaml up -d
    ```

4.  **Database Setup**
    Initialize the MySQL schema using Python and Alembic:
    ```bash
    # Install dependencies (recommend using a venv)
    pip install alembic sqlalchemy mysqlclient
    
    # Run migrations
    alembic upgrade head
    ```

### Usage

**Visual Node Editor (GUI Mode)**
Launch the visual editor to design your bot logic.

- **Windows:** Double-click `zihuan_next.exe`
- **Linux/macOS:**
  ```bash
  ./zihuan_next
  ```

**Headless/Edge Mode**
Run the engine without the GUI (suitable for servers, edge devices, or web backends). You can execute complex workflows by simply loading a JSON file.

- **Windows:**
  ```powershell
  .\zihuan_next.exe --graph-json my_workflow.json --no-gui
  ```
- **Linux/macOS:**
  ```bash
  ./zihuan_next --graph-json my_workflow.json --no-gui
  ```

## Documentation

- **[User Guide](document/user-guide.md)** — Installation, configuration, and running the application
- **[Program Execution Flow](document/program-execute-flow.md)** — Internal execution details for GUI and Headless modes
- **[Node Graph JSON Specification](document/node/node-graph-json.md)** — JSON format for saving and loading node graphs
- **[Node Lifecycle & Execution](document/node/node-lifecycle.md)** — Node execution model, scheduling, and data flow
- **[Function Subgraphs](document/node/function-subgraphs.md)** — Embedded function subgraphs and reusable sub-pipelines
- **[Node Development Guide](document/node/node-development.md)** — Creating custom nodes and extending the system

## Development

### Creating Custom Nodes

Nodes are the primary extension point. When you encounter a new complex problem, build a new node rather than adding complexity to the graph canvas.

1. Decide which crate the node belongs to:
   - General-purpose utility node → `crates/zihuan_node/src/util/`
   - Bot / QQ messaging node → `crates/zihuan_bot_adapter/src/`
   - LLM / AI node → `crates/zihuan_llm/src/`
2. Create a file for the node (one node per file) and implement the `Node` trait.
3. Export the node from the crate's `lib.rs` (or the parent `mod.rs`).
4. Register the node:
   - Nodes in `zihuan_node` → register in `crates/zihuan_node/src/registry.rs` inside `init_node_registry()`.
   - Nodes in `zihuan_bot_adapter` or `zihuan_llm` → register in `src/init_registry.rs`.

For detailed instructions, see the [Node Development Guide](document/node/node-development.md).

## License

AGPL-3.0 license // See [LICENSE](LICENSE) file.
