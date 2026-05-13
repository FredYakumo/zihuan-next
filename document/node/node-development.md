# Node Development Guide

This guide describes the practical workflow for adding a new node under the **current Simple-only graph runtime**.

For the full runtime contract, see [../dev-guides/node-system.md](../dev-guides/node-system.md).

---

## Development Workflow

### 1. Decide whether it should be a node at all

Use a node when the behavior can complete synchronously during one graph execution.

Good fits:

- transforms
- parsing
- one-shot HTTP/database/object-storage work
- Brain/tool subgraph orchestration
- persistence helpers

Do **not** use a node for:

- long-lived subscriptions
- bot event loops
- independent HTTP servers
- background services that must outlive one graph run

Those belong in the Rust service runtime.

### 2. Create one file per node

Typical placement:

| Area | Directory |
|---|---|
| Utility / transform | `zihuan_graph_engine/src/util/` |
| Database / storage helpers | matching module under `zihuan_graph_engine/src/` |
| Storage / connection / search | `storage_handler/src/` |
| Bot-related synchronous helpers | `ims_bot_adapter/src/` |
| LLM / embedding / agent-config | `zihuan_llm/src/nodes/` |
| Brain / agent | `zihuan_service/src/nodes/` |

### 3. Expose a constructor

```rust
pub fn new(id: String, name: String) -> Self
```

### 4. Implement the `Node` trait

In the current runtime, most nodes only need:

- `id()`
- `name()`
- `input_ports()`
- `output_ports()`
- `execute()`

Optional hooks:

- `on_graph_start()` for run-scoped reset
- `apply_inline_config()` for node-card configuration and dynamic ports
- `set_function_runtime_values()` for special function boundary nodes
- `set_runtime_variable_store()` for nodes that use graph-scoped runtime variables

### 5. Define ports carefully

Prefer `node_input!` and `node_output!` unless the port list must be rebuilt dynamically.

Check:

- port names are `snake_case`
- requiredness is correct
- types are concrete where possible
- hidden ports are only used for internal plumbing

### 6. Export the node

Add it to the parent `mod.rs`.

### 7. Register the node

- `zihuan_graph_engine` nodes → `zihuan_graph_engine/src/registry.rs`
- `storage_handler` / `ims_bot_adapter` / `zihuan_llm` / `zihuan_service` nodes → add `register_node!` to the owning crate's `init_node_registry()`, which is called from `src/init_registry.rs`

### 8. Validate behavior

Do the smallest useful verification:

- `cargo check`
- load a graph that uses the node
- run a workflow that exercises the node

Add automated tests only when explicitly requested or clearly justified by complexity.

---

## Common Pitfalls

- implementing behavior that should be in the service runtime instead of a node
- forgetting to rebuild dynamic ports in `apply_inline_config()`
- using vague port names
- returning loosely typed outputs when a concrete type is known
- mixing unrelated responsibilities into one node

---

## Completion Checklist

- the node lives in its own file
- `new(id: String, name: String)` exists
- port names are clear and stable
- `execute()` matches the intended synchronous behavior
- optional hooks are used only when needed
- the node is exported from its module
- the node is registered in the correct registry
- behavior has been manually or automatically validated at an appropriate level

---

## Related Documents

- Runtime contract: [../dev-guides/node-system.md](../dev-guides/node-system.md)
- Node execution model: [../dev-guides/node-types.md](../dev-guides/node-types.md)
- Dynamic ports: [dynamic-port-nodes.md](./dynamic-port-nodes.md)
- Graph JSON: [node-graph-json.md](./node-graph-json.md)
- Lifecycle: [node-lifecycle.md](./node-lifecycle.md)
