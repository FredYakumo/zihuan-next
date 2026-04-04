# Node Development Guide

This guide describes the usual workflow for adding a new node to the project. It is intended as the practical entry point for implementation work: where to place the file, what to implement, what to register, and what to verify before considering the node complete.

For detailed runtime contracts such as the `Node` trait, port and data type rules, execution order, and EventProducer lifecycle, see [../dev-guides/node-system.md](../dev-guides/node-system.md).

---

## When To Read This Guide

Read this document when you are:

- adding a brand-new node
- migrating an experimental node into the main registry
- checking whether a node implementation is complete
- reviewing a node PR for missing integration steps

If you need exact API semantics rather than development flow, switch to [../dev-guides/node-system.md](../dev-guides/node-system.md).

---

## Development Workflow

### 1. Decide the node type

Choose the execution model first:

- Use `Simple` for one-shot transforms and routing logic
- Use `EventProducer` for long-running event sources such as sockets, timers, or polling loops

If the distinction is unclear, read [../dev-guides/node-system.md](../dev-guides/node-system.md) and [../dev-guides/node-types.md](../dev-guides/node-types.md) before writing code.

### 2. Create the node file

Decide which crate the new node belongs to, then place it in one file per node:

| Node category | Crate | Directory |
|---|---|---|
| General-purpose utility or transform | `crates/zihuan_node` | `crates/zihuan_node/src/util/` |
| Bot / QQ messaging | `crates/zihuan_bot_adapter` | `crates/zihuan_bot_adapter/src/` |
| LLM / AI | `crates/zihuan_llm` | `crates/zihuan_llm/src/` |

Do not create a new directory unless the feature genuinely introduces a new area of responsibility.

The node struct should normally expose:

```rust
pub fn new(id: String, name: String) -> Self
```

### 3. Implement the `Node` trait

Implement the node against the chosen execution model:

- Simple nodes should focus on `execute()`
- EventProducer nodes should implement `node_type()`, `on_start()`, `on_update()`, `on_cleanup()`, and `set_stop_flag()`
- If the node reads values configured directly on the node card, implement `apply_inline_config()`

Keep the implementation narrow. A node should do one job clearly rather than absorb unrelated orchestration.

### 4. Define ports carefully

Declare ports with `node_input!` and `node_output!` unless the node has dynamic ports that must be rebuilt from configuration at runtime.

Check these points while defining ports:

- port names are descriptive and use `snake_case`
- required inputs are actually required
- output types are specific rather than overusing `Any`
- dynamic ports remain deterministic enough for registry probing and UI editing

For dynamic-port behavior, see [dynamic-port-nodes.md](./dynamic-port-nodes.md).

### 5. Export the node from its module

After adding the file, update the parent `mod.rs` so the node can be referenced from the registry and from tests.

### 6. Register the node

Register in the appropriate registry — this makes the node available to graph loading, the UI palette, and metadata queries.

- **Nodes in `crates/zihuan_node`** → `crates/zihuan_node/src/registry.rs` inside `init_node_registry()`.
- **Nodes in `crates/zihuan_bot_adapter` or `crates/zihuan_llm`** → `src/init_registry.rs`.

When registering:

- keep `type_id` stable once published
- use the existing category conventions
- make the display name and description understandable in the editor

### 7. Add tests

At minimum, add a unit test for the node's normal behavior. Add error-path tests when validation or parsing is part of the node's responsibility.

Use a `NodeGraph` integration test when:

- the node relies on graph wiring behavior
- inline values matter
- EventProducer downstream execution needs coverage
- registration or JSON loading is part of the risk

### 8. Verify the surrounding integration

Before finishing, verify that the node is not only implemented but also integrated into the rest of the system:

- the parent module exports it
- the registry entry exists in the correct registry file
- docs or examples are updated if the node adds a notable capability
- the node behaves correctly in the expected graph shape

---

## Common Pitfalls

- Forgetting to register the node after writing the implementation
- Using vague port names that make graph editing hard
- Marking optional inputs as required, or the reverse
- Returning loosely typed outputs where a concrete type should be enforced
- EventProducer nodes not checking the stop flag regularly
- Dynamic-port nodes exposing unstable port lists before config is applied
- Writing a node that mixes data transformation, transport, and persistence concerns in one place

---

## Completion Checklist

Use this checklist before considering a node finished:

- The node lives in its own file
- `new(id: String, name: String)` exists
- The `Node` trait implementation matches the intended execution model
- Ports are declared clearly and use stable naming
- Inline config handling is implemented when needed
- The node is exported from the parent `mod.rs`
- The node is registered in the correct registry (`crates/zihuan_node/src/registry.rs` or `src/init_registry.rs`)
- Unit tests cover the main behavior
- Error cases are tested when they are part of the contract
- Any EventProducer implementation stores and checks the stop flag

---

## Related Documents

- Detailed system contract: [../dev-guides/node-system.md](../dev-guides/node-system.md)
- Node execution model overview: [../dev-guides/node-types.md](../dev-guides/node-types.md)
- Dynamic-port nodes: [dynamic-port-nodes.md](./dynamic-port-nodes.md)
- Graph JSON format: [node-graph-json.md](./node-graph-json.md)
- Node lifecycle detail: [node-lifecycle.md](./node-lifecycle.md)
