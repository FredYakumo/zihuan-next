# Function Subgraphs And Brain Tool Subgraphs

This document describes the embedded function-subgraph system introduced for `function` nodes and `brain` tool definitions.

---

## Overview

The system adds a private subgraph mechanism:

- A `function` node owns its own function signature and subgraph.
- Each `brain` tool owns its own tool subgraph.
- Subgraphs are embedded inside the parent node's inline JSON config instead of being stored as top-level graphs.

This replaces the old Brain "one dynamic output port per tool" execution style. `brain` now runs an internal tool loop and returns only a final `assistant_message`.

---

## Core Data Model

### Shared signature structs

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

For Brain tools:

```rust
pub struct BrainToolDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub parameters: Vec<ToolParamDef>,
    pub outputs: Vec<FunctionPortDef>,
    pub subgraph: NodeGraphDefinition,
}
```

### Storage locations

| Owner | Inline key | Meaning |
|------|------------|---------|
| `function` node | `function_config` | Full embedded function definition |
| `brain` node | `tools_config` | Array of `BrainToolDefinition` |
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

Subgraphs are kept consistent by `sync_function_subgraph_signature()` in [src/node/function_graph.rs](../../src/node/function_graph.rs).

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

## Brain Runtime

`brain` no longer exposes one output port per tool. Its output is now:

- `assistant_message`

### Tool loop behavior

1. Read `tools_config`.
2. Convert each tool definition into an LLM tool schema using `parameters`.
3. Send the current conversation to the model.
4. If the model returns tool calls:
   - find the matching embedded tool subgraph by tool `name`
   - run that tool subgraph with the tool arguments
   - package all declared outputs as a JSON object string
   - append a `tool` role message to the conversation
5. Repeat until there are no tool calls or the max iteration limit is reached.
6. Return the final assistant message as `assistant_message`.

### Tool result contract

- Tool inputs are fixed by `parameters`.
- Tool outputs are fixed by `outputs`.
- Tool return content is always a JSON object string.
- A tool with zero declared outputs returns `{}`.

This makes Brain closer to a standard agentic loop: tool execution stays internal to the Brain node instead of being modeled as external graph wiring.

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

### Brain node shape

```jsonc
{
  "node_type": "brain",
  "output_ports": [
    { "name": "assistant_message", "data_type": "OpenAIMessage" }
  ],
  "inline_values": {
    "tools_config": [
      {
        "id": "tool_1",
        "name": "search",
        "description": "Search docs",
        "parameters": [
          { "name": "query", "data_type": "String", "desc": "query text" }
        ],
        "outputs": [
          { "name": "results", "data_type": "Json" }
        ],
        "subgraph": {
          "nodes": [ ... function_inputs ..., ... function_outputs ... ],
          "edges": [ ... ]
        }
      }
    ]
  }
}
```

### Refresh and auto-fix

`refresh_port_types()` and `auto_fix_graph_definition()` recurse into:

- `function_config.subgraph`
- `tools_config[*].subgraph`

They also:

- rebuild dynamic ports from embedded config
- keep boundary node signatures in sync
- prune edges that reference removed legacy ports
- normalize old Brain nodes back to static `assistant_message` output

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
- current function or tool name

Both function-node subgraphs and Brain tool subgraphs use the same navigation mechanism.

### Editors

#### Function editor

The `function` node provides:

- `编辑函数`
- `进入子图`

The function editor dialog updates:

- function name
- description
- input signature
- output signature

Saving also:

- rebuilds the outer node's visible ports
- syncs the two boundary nodes inside the child subgraph
- removes invalid edges caused by deleted ports

#### Brain tool editor

The Brain tool editor updates:

- tool id
- name
- description
- input parameters
- output signature
- subgraph entry

Tool parameters are the fixed call schema presented to the LLM. Tool outputs define the JSON object returned from the tool subgraph.

---

## Restrictions

### Inside function subgraphs

- EventProducer nodes are not allowed.
- `function_inputs` and `function_outputs` are hidden from the add-node palette.
- Run is disabled from subgraph pages; users must return to the main graph to run.

### Compatibility

Old Brain graphs are not semantically migrated to the previous external tool-loop pattern.

What is migrated automatically:

- stale Brain tool output ports are removed
- edges connected to those removed ports are pruned
- the tab is marked dirty after load so the user can review and save

The old `tool_result` node still exists, but it is no longer required for the new Brain flow.

---

## Relevant Source Files

| Area | File |
|------|------|
| Shared embedded function model | [src/node/function_graph.rs](../../src/node/function_graph.rs) |
| Function node runtime | [src/node/util/function.rs](../../src/node/util/function.rs) |
| Function input boundary node | [src/node/util/function_inputs.rs](../../src/node/util/function_inputs.rs) |
| Function output boundary node | [src/node/util/function_outputs.rs](../../src/node/util/function_outputs.rs) |
| Brain tool definition model | [src/llm/brain_tool.rs](../../src/llm/brain_tool.rs) |
| Brain runtime loop | [src/llm/brain_node.rs](../../src/llm/brain_node.rs) |
| Recursive graph refresh / auto-fix | [src/node/graph_io.rs](../../src/node/graph_io.rs) |
| Subgraph page stack UI state | [src/ui/node_graph_view.rs](../../src/ui/node_graph_view.rs) |
| Function editor callbacks | [src/ui/node_graph_view_callbacks/function_editor.rs](../../src/ui/node_graph_view_callbacks/function_editor.rs) |
| Brain tool editor callbacks | [src/ui/node_graph_view_callbacks/tool_editor.rs](../../src/ui/node_graph_view_callbacks/tool_editor.rs) |
