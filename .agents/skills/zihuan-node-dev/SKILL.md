---
name: zihuan-node-dev
description: Develop new node types for the zihuan-next DAG graph engine. Use this skill when asked to create, modify, or debug graph nodes — including port definitions, node registration, Node trait implementation, and lifecycle management.
---

# Node Development in zihuan-next

Nodes are the fundamental building blocks of the DAG graph engine. Every node implements the `Node` trait. There is only one node type: **Simple** — every node executes as a synchronous transformation `execute(inputs) -> outputs`.

The runtime still exposes `NodeType` for compatibility, but only one variant remains:

```rust
pub enum NodeType {
    Simple,
}
```

Each node runs once when the graph executor reaches it in topological order, receives all resolved input values as a `NodeInputFlow`, and returns one `NodeOutputFlow` synchronously. Nodes may keep per-run state via `on_graph_start()` and rebuild config-driven ports in `apply_inline_config()`.

## File placement

- **Utility/general nodes**: `zihuan_node/src/util/` — one file per node (e.g., `format_string_node.rs`, `conditional_node.rs`)
- **Bot/QQ adapter nodes**: `ims_bot_adapter/src/` — one file per node
- **LLM/AI nodes**: `zihuan_llm/src/` or `model_inference/src/`

Each node function: `pub fn new(id: String, name: String) -> Self`

## Port definition

Use the `port!{ ... }` entries inside `node_input!` and `node_output!` macros from `node_macros`. The macros generate `fn input_ports(&self) -> Vec<Port>` and `fn output_ports(&self) -> Vec<Port>` respectively, with compile-time duplicate name detection.

```rust
use node_macros::{node_input, node_output};

impl Node for MyNode {
    node_input![
        port! { name = "input_a", ty = String, desc = "Description shown in UI" },
        port! { name = "count",   ty = Integer, optional },
    ];

    node_output![
        port! { name = "result", ty = String, desc = "The output value" },
    ];
}
```

### Port field options

| Field | Description |
|-------|-------------|
| `name = "..."` | Port name (required) |
| `ty = Type` | Data type (required). Supports: `String`, `Integer`, `Float`, `Boolean`, `Json`, `Binary`, crate path types, `Vec(InnerType)`, `Custom("...")` |
| `desc = "..."` | Description shown in UI tooltips |
| `optional` | Mark port as optional (not required). Use `required = true` / `required = false` for explicit control |

## Flow macros

In addition to port definition macros, `node_macros` provides macros for constructing input/output flows in `execute()`:

### `node_input_flow!`

Build a `NodeInputFlow` from `"key" => value` pairs. Typically used in tests or when calling another node's `execute` directly.

```rust
use node_macros::node_input_flow;

let inputs = node_input_flow![
    "port_a" => some_data_value,
    "port_b" => another_value,
];
```

### `node_output_flow!`

Build a `NodeOutputFlow` from `"key" => value` pairs. **No validation** is performed against declared output ports.

```rust
use node_macros::node_output_flow;

let outputs = node_output_flow![
    "result" => computed_value,
];
Ok(outputs)
```

### `return_with_node_output!`

Build a validated `NodeOutputFlow` and wrap in `Ok(...)`. Calls `self.validate_outputs()` to ensure the output matches declared ports. This is the **preferred** way to return from `execute()`:

```rust
use node_macros::return_with_node_output;

fn execute(&mut self, inputs: NodeInputFlow) -> Result<NodeOutputFlow> {
    let value = inputs.get_required("input_a")?;
    let result = /* transform */;

    return_with_node_output![self;
        "result" => result,
    ]
}
```

> **Prefer `return_with_node_output!`** over `node_output_flow!` in production `execute()` methods — it catches mismatched output ports at runtime.

## Node registration

After creating a node, register it so the engine can discover it:

**For nodes in `zihuan_node`**: Add to `zihuan_node/src/registry.rs` using `register_node!` macro.

**For nodes in `ims_bot_adapter` or other crates**: Add to `src/init_registry.rs` in the root crate:

```rust
register_node!(
    registry,
    "type_id",
    "Display Name",
    "Category",
    "Description of what this node does",
    MyNode::new
);
```

The registry is a global lazy-static `NODE_REGISTRY` (`once_cell::sync::Lazy`).

## Dynamic ports

Nodes that need runtime-determined ports implement:

```rust
fn has_dynamic_input_ports(&self) -> bool { true }
fn has_dynamic_output_ports(&self) -> bool { true }
```

Then rebuild ports in response to configuration changes (e.g., inline config values). This is used by nodes like `FunctionNode` and `BrainNode` where subgraph boundaries determine port sets.

## Data types

Available port data types (defined in `zihuan_core`):

| Category | Types |
|----------|-------|
| Primitives | `String`, `Integer`, `Float`, `Boolean`, `Json`, `Binary` |
| Messages | `OpenAIMessage`, `QQMessage`, `MessageEvent` |
| Media | `Image`, `ContentPart` |
| Infrastructure | `BotAdapterRef`, `S3Ref`, `RedisRef`, `MySqlRef`, `WeaviateRef`, `TavilyRef`, `SessionStateRef` |
| LLM | `LLModel`, `EmbeddingModel`, `FunctionTools`, `OpenAIMessageSessionCacheRef` |
| Control | `LoopControlRef` |
| Security | `Password` |
| Collections | `Vec<T>` |
| Custom | `Custom(String)` |

## Agent tips

- **One node per file** — the graph must remain a DAG; keep node implementations isolated.
- **Use `port!{ ... }` macros** — `node_input!` and `node_output!` reduce boilerplate and provide compile-time duplicate detection.
- **Prefer `return_with_node_output!`** in `execute()` — it validates outputs against declared ports, catching mismatches at runtime.
- **Use `node_input_flow!` / `node_output_flow!`** for constructing flows in tests or direct node invocations.
- **Common types go in `zihuan_core`** — if your node needs a type shared across crates, put it there to avoid circular dependencies.
- **Search for similar nodes first** — check `zihuan_node/src/util/` before writing a new one. Reuse existing patterns.
- **Error messages should carry context** — include field names, node inputs, and source values in error text.
- **Use `#[cfg(test)] mod tests`** for unit tests on node logic, especially for data transformation nodes.
