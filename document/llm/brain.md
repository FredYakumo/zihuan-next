# Brain

This document describes the `brain` node as an agentic runtime with embedded tool subgraphs.

Brain tool subgraphs reuse the same embedded subgraph and boundary-node mechanism described in [../node/function-subgraphs.md](../node/function-subgraphs.md). This document focuses on the Brain-specific data model, tool contract, and runtime loop.

---

## Overview

The `brain` node owns a private tool set:

- Each tool owns its own embedded subgraph.
- Tool subgraphs are stored inside the parent Brain node's inline JSON config.
- Brain runs an internal agentic tool loop and returns an `output` message trace for the current run.

This replaces the old Brain execution style where each tool was exposed as a separate dynamic output port.

---

## Core Data Model

Brain tools use these definitions:

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

Brain also owns a shared input signature:

```rust
pub type BrainSharedInputs = Vec<FunctionPortDef>;
```

### Storage locations

| Owner | Inline key | Meaning |
|------|------------|---------|
| `brain` node | `tools_config` | Array of `BrainToolDefinition` |
| `brain` node | `shared_inputs` | Shared runtime inputs injected into every Brain tool subgraph |

The embedded tool subgraph itself uses the same `function_inputs` and `function_outputs` boundary nodes documented in [../node/function-subgraphs.md](../node/function-subgraphs.md).

---

## Runtime

`brain` no longer exposes one output port per tool. Its output is now:

- `output: Vec<OpenAIMessage>`

### Tool loop behavior

1. Read `tools_config`.
2. Read `shared_inputs`.
3. Convert each tool definition into an LLM tool schema using `parameters`.
4. Send the current conversation to the model.
5. If the model returns tool calls:
   - find the matching embedded tool subgraph by tool `name`
   - run that tool subgraph with `shared_inputs + fixed content + tool arguments`
   - package all declared outputs as a JSON object string
   - append a `tool` role message to the conversation
6. Repeat until there are no tool calls or the max iteration limit is reached.
7. Return the current-run output trace as `output`, in execution order:
   - assistant messages that request tool calls
   - each tool result message
   - the final assistant message without tool calls

This makes Brain closer to a standard agentic loop: tool execution stays internal to the Brain node instead of being modeled as external graph wiring.

---

## Tool Contracts

### Tool input contract

- Brain shared inputs are fixed by `shared_inputs`.
- Brain injects a fixed reserved input `content: String` for each tool call, sourced from the triggering assistant message `content` and defaulting to `""` when absent.
- Tool-local LLM-callable inputs are fixed by `parameters`.
- Tool subgraph boundary inputs are `shared_inputs + content + parameters`.
- Tool parameter names must not use the reserved name `content`.
- `shared_inputs` are runtime-only and are not exposed in the LLM tool schema.

### Tool result contract

- Tool outputs are fixed by `outputs`.
- Tool return content is always a JSON object string.
- A tool with zero declared outputs returns `{}`.

---

## Graph JSON Behavior

Brain tool subgraphs are recursive graph payloads stored inside Brain inline config.

### Brain node shape

```jsonc
{
  "node_type": "brain",
  "dynamic_input_ports": true,
  "output_ports": [
    { "name": "output", "data_type": { "Vec": "OpenAIMessage" } }
  ],
  "inline_values": {
    "shared_inputs": [
      { "name": "context", "data_type": "Json" },
      { "name": "sender_id", "data_type": "String" }
    ],
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

- `tools_config[*].subgraph`

They also:

- rebuild Brain shared input ports from `shared_inputs`
- keep boundary node signatures in sync
- prune edges that reference removed legacy ports
- normalize old Brain nodes back to static `output` message-trace output

---

## UI Model

Brain tool subgraph editing uses the same page-stack navigation model as function subgraphs.

When editing a Brain tool subgraph, the file tab shows a breadcrumb-like bar:

- `返回`
- clickable `主图`
- current tool name

The shared subgraph editor behavior is documented in [../node/function-subgraphs.md](../node/function-subgraphs.md).
