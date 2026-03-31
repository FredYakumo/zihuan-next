# Developer Guides

Documentation for developing and extending zihuan-next — a Rust + Slint node-graph workflow engine for event-driven bot pipelines.

---

## Where to start

| Goal | Read first |
|------|-----------|
| Understand the overall system | [node-system.md](./node-system.md) |
| Build a new node | [../node/node-development.md](../node/node-development.md) |
| Build a node with config-driven ports | [../node/dynamic-port-nodes.md](../node/dynamic-port-nodes.md) |
| Understand embedded function subgraphs and Brain tool subgraphs | [../node/function-subgraphs.md](../node/function-subgraphs.md) |
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
| [../node/function-subgraphs.md](../node/function-subgraphs.md) | Embedded function graphs, boundary nodes, Brain internal tool loop, subgraph UI navigation |
| [../node/node-graph-json.md](../node/node-graph-json.md) | Complete node graph JSON specification with all field and data type descriptions |
| [../node/node-lifecycle.md](../node/node-lifecycle.md) | Node lifecycle detail: on_graph_start, execute, on_start/update/cleanup |
