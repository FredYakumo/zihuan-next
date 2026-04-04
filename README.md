# zihuan-next

**Node-graph engine** for building simple, event-driven AI pipelines — describe data flow on the graph, encapsulate complexity inside nodes.

<img width="1248" height="880" alt="image" src="https://github.com/user-attachments/assets/3b781e53-1fcf-4b77-91ba-2d63299181c4" />


## Overview

zihuan-next uses a **node graph** to describe how data moves through a workflow. You can think of it as a simple flowchart made of typed processing blocks: data comes in, passes through a few steps, and produces an output.

> **The graph describes what data flows where. Complexity lives inside individual nodes.**

That means the graph itself should stay easy to read. A workflow might look like:

`receive message → extract text → call model → format reply → send message`

Each node has clear **inputs** and **outputs**, and every port has a declared data type. The graph therefore focuses on the **big picture**: what data enters, how it is transformed, and where it goes next.

This is also where the project differs from putting all logic directly into an agent loop. **Agentic behavior is supported, but it should be encapsulated inside nodes.** In other words, the node graph is responsible for describing the outer data flow, while complex inner behavior — such as LLM reasoning, tool calling, message memory, retrieval, or other multi-step control logic — should live inside dedicated nodes.

So the graph can remain simple, while nodes can remain powerful. When a new problem becomes too complex to express cleanly on the canvas, the preferred solution is to create a new node or package the logic as a function subgraph rather than making the main graph more complicated.

The project currently has strong support for QQ bots, but the overall idea is broader: use a node graph to build **simple workflows** and **event-driven interactions** that can run in a desktop app or directly from JSON in headless mode.

### Key Capabilities

1.  **Node Graphs for Data Flow**: The graph is used to describe how data moves between steps. It is not meant to expose every internal algorithm or control detail on the canvas.
2.  **Strongly Typed Inputs and Outputs**: Every node works through typed ports, so the graph has a clear contract for what kind of data each step receives and produces.
3.  **Headless / No-GUI Execution**: Workflows can run directly from JSON with `--no-gui`, which makes them suitable for service deployment, command-line execution, and being invoked by other AI systems as tools or skills.
4.  **Infrastructure Nodes**: Nodes can provide reusable infrastructure for algorithms, linear algebra, neural network models, LLMs, and other higher-level intelligent systems.
5.  **Simple Flows and Event-Driven Interaction**: The same graph model can describe straightforward one-pass workflows as well as event-driven interactions triggered by messages, sockets, or other incoming events.
6.  **Function Subgraphs and Agent Tools**: A `function` node can package a private subgraph as one reusable step, which helps simplify the main graph. The same function logic can also be exposed to LLM-driven nodes as callable tools.
7.  **Extensibility**: When a workflow needs new behavior, the preferred pattern is to add a new node or function subgraph rather than pushing more complexity into the top-level graph.

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

### How the graph runtime works
- **Nodes are connected by ports**: Each node has typed inputs and outputs, and data moves along those connections.
- **The graph stays simple**: The canvas is for showing the workflow structure, not for expressing complicated logic step by step.
- **Two execution styles**: Some nodes run once when triggered, while others stay active and keep producing events.
- **Function subgraphs**: A `function` node can wrap a small private subgraph and present it as one clean step in the main graph.

### Integration Components
- **Bot Adapter** (`crates/zihuan_bot_adapter`): Connects to QQ bot servers and turns incoming messages into workflow input.
- **LLM Integration** (`crates/zihuan_llm`): Provides nodes for model calls, tool-using AI behaviors, and retrieval features.
- **Message Store**: Supports caching and persistent history with Redis, MySQL, and in-memory fallback.
- **Visual Editor** (`src/ui/`): Lets you build and inspect workflows as a node graph.

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
Open the editor when you want to build or inspect a workflow as a graph.

- **Windows:** Double-click `zihuan_next.exe`
- **Linux/macOS:**
  ```bash
  ./zihuan_next
  ```

**Headless/Edge Mode**
Run the engine without the GUI when you only need to execute a saved workflow file.

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

Nodes are the main extension point. If a workflow needs a new complex behavior, the preferred approach is to build a new node for it instead of making the graph itself more complicated.

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
