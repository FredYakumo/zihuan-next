# Developer Guides

Documentation for developing and extending zihuan-next — a Rust + Slint node-graph workflow engine for event-driven bot pipelines.

The node graph describes **data flow** between processing steps. Complexity (algorithms, agentic loops, control flow) is encapsulated inside individual nodes; the graph topology itself stays simple and readable. When you encounter a new complex problem, build a new node rather than adding complexity to the graph canvas.

---

## Package Structure

| Package | Contents |
|---|---|
| `packages/zihuan_core` | Error types, config loading, URL utilities |
| `packages/zihuan_bot_types` | `MessageEvent`, QQ message models, bot handle |
| `packages/zihuan_llm_types` | `OpenAIMessage`, `LLMBase` trait, `FunctionTool` trait |
| `packages/zihuan_node` | `Node` trait, `DataType`/`DataValue`, DAG execution engine, general-purpose utility nodes, base registry |
| `packages/zihuan_bot_adapter` | `BotAdapterNode`, QQ message send/receive nodes |
| `packages/zihuan_llm` | `LLMApiNode`, `LLMInferNode`, `BrainNode`, RAG nodes |
| `node_macros` | `node_input!`, `node_output!`, `port!` procedural macros |
| `src/` | Main binary: Slint UI, combined node registry (`init_registry.rs`) |

### Node registration

- Nodes in `zihuan_node` → `packages/zihuan_node/src/registry.rs` (`init_node_registry()`)
- Nodes in `zihuan_bot_adapter` or `zihuan_llm` → `src/init_registry.rs`

---

## Build Profiles And Features

Most development builds can use the default CPU-only configuration:

```bash
cargo check
cargo build
```

For local Candle embedding GPU acceleration, the root crate forwards these optional features to `packages/zihuan_llm`:

```bash
# CUDA build (requires CUDA toolkit / nvcc)
cargo build --features candle-cuda

# Metal build (macOS)
cargo build --features candle-metal
```

Runtime behavior for the local text embedding loader:

- Prefer `CUDA` when the binary was compiled with `candle-cuda` and device initialization succeeds.
- Otherwise prefer `Metal` when compiled with `candle-metal` and available.
- Otherwise fall back to `CPU`.
- If GPU inference fails at runtime, embedding execution falls back to CPU automatically.

---

## Where to start

| Goal | Read first |
|------|-----------|
| Understand the overall system | [node-system.md](./node-system.md) |
| Build a new node | [../node/node-development.md](../node/node-development.md) |
| Build a node with config-driven ports | [../node/dynamic-port-nodes.md](../node/dynamic-port-nodes.md) |
| Understand embedded function subgraphs | [../node/function-subgraphs.md](../node/function-subgraphs.md) |
| Understand the Brain agentic runtime and tool subgraphs | [../llm/brain.md](../llm/brain.md) |
| Understand the JSON graph file format | [../node/node-graph-json.md](../node/node-graph-json.md) |
| Understand how the UI talks to nodes | [ui-architecture.md](./ui-architecture.md) |
| Look up naming and coding conventions | [code-conventions.md](./code-conventions.md) |

---

## Guide index

### dev-guides/ (this directory)

| Document | Contents |
|----------|----------|
| [node-system.md](./node-system.md) | Node trait, DataType/DataValue, execution engine, topological sort, EventProducer lifecycle |
| [ui-architecture.md](./ui-architecture.md) | Slint/Rust layering, VM pattern, callback boundaries, coordinate systems, special node editors |
| [code-conventions.md](./code-conventions.md) | Naming rules, file layout, common utilities, error handling, logging |
| [qq-message.md](./qq-message.md) | QQMessage data model, serde compatibility, and MessageProp aggregation |
| [qq_message_storage.md](./qq_message_storage.md) | QQMessage storage path in Redis/MySQL and the current MySQL table schema |
| [logging.md](./logging.md) | Logging initialization, backends, GUI overlay buffers, and log level control |

### node/ (node-specific docs)

| Document | Contents |
|----------|----------|
| [../node/node-development.md](../node/node-development.md) | Node implementation outline and quick checklist; detailed contracts live in `node-system.md` |
| [../node/dynamic-port-nodes.md](../node/dynamic-port-nodes.md) | Dynamic-port nodes: implementation pattern, UI coordination, JSON markers |
| [../node/function-subgraphs.md](../node/function-subgraphs.md) | Embedded function graphs, boundary nodes, and subgraph UI navigation |
| [../llm/brain.md](../llm/brain.md) | Brain agentic loop, tool contracts, embedded tool subgraphs, and Brain-specific JSON behavior |
| [../node/node-graph-json.md](../node/node-graph-json.md) | Complete node graph JSON specification with all field and data type descriptions |
| [../node/node-lifecycle.md](../node/node-lifecycle.md) | Node lifecycle detail: on_graph_start, execute, on_start/update/cleanup |
