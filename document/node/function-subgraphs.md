# Function Subgraphs

This document describes the embedded subgraph system used by `function` nodes.

For `brain` tool subgraphs and the agentic Brain runtime, see [../llm/brain.md](../llm/brain.md).

---

## Overview

A `function` node owns a private subgraph and function signature:

- The subgraph is embedded inside the node's inline JSON config.
- The node's visible input and output ports are rebuilt from that embedded signature.
- The embedded subgraph is executed as a child graph during the outer `function` node run.

This keeps reusable function logic self-contained instead of storing it as a top-level graph.

---

## Core Data Model

Runtime and persistence use these shared definitions:

```rust
pub struct FunctionPortDef {
    pub name: String,
    pub data_type: DataType,
}

pub struct EmbeddedFunctionConfig {
    pub name: String,
    pub description: String,
    pub inputs: Vec<FunctionPortDef>,
    pub outputs: Vec<FunctionPortDef>,
    pub subgraph: NodeGraphDefinition,
}
```

### Storage locations

| Owner | Inline key | Meaning |
|------|------------|---------|
| `function` node | `function_config` | Full embedded function definition |
| `function_inputs` node | `signature` / `runtime_values` | Declared input signature and injected call arguments |
| `function_outputs` node | `signature` | Declared output signature |

---

## Boundary Nodes

Each function subgraph contains two special internal nodes:

| Node type | Role |
|----------|------|
| `function_inputs` | Expands runtime call arguments into dynamic output ports |
| `function_outputs` | Collects subgraph results through dynamic input ports |

Important rules:

- They are persisted as normal `NodeDefinition`s inside the embedded subgraph.
- Their positions and sizes are saved with the subgraph.
- They are internal-only and must not be creatable from the palette.
- They must not be deletable or copyable from the editor.
- Hidden config ports such as `signature` are not rendered on the canvas.

Subgraphs are kept consistent by `sync_function_subgraph_signature()` in `zihuan_graph_engine/src/function_graph.rs`.

---

## Function Node Runtime

The `function` node is a dynamic-port node:

- `dynamic_input_ports = true`
- `dynamic_output_ports = true`

Its visible ports are rebuilt from `function_config`.

### Execute flow

1. Read and validate `function_config`.
2. Clone the embedded subgraph.
3. Inject runtime arguments into the `function_inputs` node.
4. Inject the declared output signature into `function_outputs`.
5. Build and execute the child graph.
6. Read the `function_outputs` execution results.
7. Validate each declared output against the outer function signature.
8. Return the validated output map to the caller.

Errors are wrapped with the outer function node id so the UI can attribute failures to the calling node instead of only the boundary node.

---

## Graph JSON Behavior

Embedded subgraphs are recursive graph payloads stored inside node inline config.

### Function node shape

```jsonc
{
  "node_type": "function",
  "dynamic_input_ports": true,
  "dynamic_output_ports": true,
  "inline_values": {
    "function_config": {
      "name": "MyFunction",
      "description": "",
      "inputs": [{ "name": "text", "data_type": "String" }],
      "outputs": [{ "name": "result", "data_type": "String" }],
      "subgraph": {
        "nodes": [ ... function_inputs ..., ... function_outputs ... ],
        "edges": [ ... ]
      }
    }
  }
}
```

### Refresh and auto-fix

`refresh_port_types()` and `auto_fix_graph_definition()` recurse into:

- `function_config.subgraph`

They also:

- rebuild dynamic ports from embedded config
- keep boundary node signatures in sync
- prune edges that reference removed legacy ports

---

## UI Model

Subgraph editing uses a page stack per file tab.

### GraphTabState page stack

Each tab stores:

- one root page for the main graph
- zero or more child pages for nested subgraphs

Every page keeps its own:

- graph
- selection
- inline input cache
- canvas pan/zoom state

Before save, open, tab switch, or navigation back to root, the current page state is committed back into the owner node's embedded config.

### Navigation

When editing a subgraph, the file tab shows a breadcrumb-like bar:

- `返回`
- clickable `主图`
- current function name

Function-node subgraphs and Brain tool subgraphs share the same navigation mechanism, but the Brain-specific behavior is documented in [../llm/brain.md](../llm/brain.md).
