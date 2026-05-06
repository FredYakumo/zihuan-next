# Node Lifecycle & Execution Flow

This document describes the **current** node lifecycle in `zihuan-next`.

The node graph runtime is now synchronous and DAG-based. There is no separate `EventProducer` lifecycle anymore.

---

## Execution Model

Every node executes under the same model:

| Model | Core method | Stateful across one run? | Long-lived background loop? |
|-------|-------------|--------------------------|-----------------------------|
| `Simple` | `execute()` | Optional, via `on_graph_start()` | No |

---

## Lifecycle Phases

### 1. Graph assembly

Before execution, the runtime:

- loads graph JSON
- creates node instances from the registry
- restores inline values
- restores dynamic ports via `apply_inline_config()`

### 2. Run initialization

At the start of each graph run:

```text
for each node:
  on_graph_start()
for each node:
  apply_inline_config(...)
inject runtime variable store
```

Use `on_graph_start()` to reset run-scoped state such as counters or caches that should persist only for the current execution.

### 3. Topological execution

The graph is topologically sorted and then executed node by node:

```text
for node in topological order:
  collect inputs
  validate_inputs(inputs)
  outputs = execute(inputs)
  validate_outputs(outputs)
  store outputs
```

### 4. Optional execution callback

After a node runs, the graph may emit an execution callback so the UI or tooling layer can update previews or task logs.

---

## Data Flow

### Input collection

Inputs come from:

- upstream edges
- inline/default values when applicable
- runtime function boundary injection for special nodes

### Output storage

Outputs are stored in the graph execution result pool and become available to downstream nodes.

### Validation

Per node, the runtime checks:

- required inputs are present
- input/output types are compatible with declared ports

---

## Dynamic-Port Nodes

Dynamic-port nodes still follow the same lifecycle, but they usually rely on `apply_inline_config()` to rebuild their ports before execution.

Common examples:

- function boundary nodes
- Brain/tool config nodes
- JSON-extract style nodes

Rule of thumb:

- use `apply_inline_config()` to rebuild structure
- use `execute()` to process runtime values

---

## Function And Tool Subgraphs

Tool/function subgraphs are still synchronous from the graph runtime’s point of view.

The runtime may inject:

- function runtime values
- shared runtime variable store

This allows services such as QQ agents or HTTP stream agents to reuse graph logic safely without making the graph engine itself asynchronous.

---

## Stop / Cancellation

There is still a graph-level stop flag owned by `NodeGraph`, mainly for task control integration.

What changed:

- nodes do not implement `set_stop_flag()`
- nodes do not run internal lifecycle loops controlled by the graph
- cancellation is now mostly a graph/task concern, not a node-type concern

---

## Removed Lifecycle

The following lifecycle no longer exists in the graph engine:

```text
on_start -> loop { on_update } -> on_cleanup
```

That older model has been removed from node execution and replaced by:

```text
on_graph_start -> apply_inline_config -> execute
```

---

## Service Runtime Boundary

Anything that must remain alive independently of one graph run now lives outside the graph lifecycle:

- QQ agent message subscription
- HTTP stream serving
- auto-start and stop
- concurrent agent hosting

These are handled in the Rust service runtime, especially:

- `src/service/agent_manager.rs`

The service runtime may call back into the graph engine to execute synchronous subgraphs or tools.

---

## Developer Guidance

When implementing a node, think in this order:

1. What state must reset once per graph run? Put that in `on_graph_start()`.
2. What structure comes from inline config? Rebuild it in `apply_inline_config()`.
3. What work happens at runtime? Do it in `execute()`.

If the behavior instead needs:

- subscriptions
- sockets that remain open
- a standalone server
- continuous background processing

do not model it as a graph lifecycle. Move it into the service runtime.
