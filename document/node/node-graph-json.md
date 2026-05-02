# Node Graph JSON Specification

This document describes the JSON format used to save and load node graphs. The GUI reads and writes this format; the runtime rebuilds an executable `NodeGraph` from it via `build_node_graph_from_definition()` in `packages/zihuan_node/src/registry.rs`.

---

## Validating a graph before running

Run the built-in validator to check a graph JSON file for safety:

```bash
cargo run -- --graph-json my_graph.json --validate
# or with a release build:
./zihuan_next --graph-json my_graph.json --validate
```

**What is checked:**

| Check | Severity |
|-------|----------|
| JSON parse / schema errors | error (exit 2) |
| `node_type` not found in registry | error |
| Required input port absent from node | error |
| Invalid edge (unknown node ID or port name) | error |
| Cycle dependency in the graph | error |
| Port present in JSON but removed from registry | warning |
| `inline_values` key with no matching port | warning |
| Subgraphs inside `function` / `brain` nodes | recursively checked |

**Exit codes:** `0` = pass or warnings only · `1` = one or more errors · `2` = file load failure

**Rust API** (for programmatic use):

```rust
use crate::node::graph_io::{validate_graph_definition, find_cycle_node_ids, ValidationIssue};

let definition = node::load_graph_definition_from_json("my_graph.json")?;
let issues: Vec<ValidationIssue> = validate_graph_definition(&definition);
let cycles = find_cycle_node_ids(&definition); // Tarjan's SCC; empty = DAG
for issue in &issues {
    println!("[{}] {}", issue.severity, issue.message);
}
let has_errors = issues.iter().any(|i| i.severity == "error") || !cycles.is_empty();
```

---

## Root structure

```jsonc
{
  "nodes": [ /* NodeDefinition[] */ ],
  "edges": [ /* EdgeDefinition[] */ ],
  "hyperparameters": [ /* HyperParameter[] */ ],  // optional
  "variables": [ /* GraphVariable[] */ ],          // optional
  "metadata": {                                    // optional
    "name":        "My Workflow",
    "description": "What this graph does.",
    "version":     "1.0.0"
  }
}
```

`execution_results` exists in memory for UI display purposes but is **never** written to disk.

---

## GraphMetadata

Editable from the **Zihuan Next → 编辑节点图信息** menu entry.

| Field | Type | Description |
|---|---|---|
| `name` | `string \| null` | Human-readable display name (may differ from the filename). |
| `description` | `string \| null` | Free-text description of what the graph does. |
| `version` | `string \| null` | Semver-style version string, e.g. `"1.0.0"`. |

All fields are optional and default to `null`. When the workflow browser lists entries from `workflow_set/`, it shows `name`, `description`, `version`, the filename, and a cover image (if present).

---

## NodeDefinition

```jsonc
{
  "id":           "node_1",
  "name":         "Format String",
  "description":  "Optional tooltip",
  "node_type":    "format_string",        // must match a registered type_id
  "input_ports":  [ /* Port[] */ ],
  "output_ports": [ /* Port[] */ ],
  "dynamic_input_ports":  false,          // optional; default false
  "dynamic_output_ports": false,          // optional; default false
  "position":     { "x": 40.0, "y": 40.0 },
  "size":         { "width": 200.0, "height": 120.0 },  // null = auto-size
  "inline_values": {
    "template": "Hello ${name}"           // port_name → JSON value
  },
  "port_bindings": {
    "text": { "kind": "variable", "name": "api_key" }
  },
  "has_error":    false                   // runtime flag, safe to omit/ignore
}
```

| Field | Required | Notes |
|-------|----------|-------|
| `id` | yes | Unique within the graph. Convention: `node_1`, `node_2`, ... |
| `name` | yes | Display label shown on the node card in the GUI |
| `description` | no | Tooltip text |
| `node_type` | yes | Must be a registered `type_id` in `NODE_REGISTRY` |
| `input_ports` | yes | Ordered list of input Port objects |
| `output_ports` | yes | Ordered list of output Port objects |
| `dynamic_input_ports` | no | `true` = input ports are config-driven; skip auto-fix and compatibility checks for this direction |
| `dynamic_output_ports` | no | Same for output direction |
| `position` | no | Top-left corner in canvas space. Omitting lets the GUI auto-layout on load |
| `size` | no | `null` or omitted = auto-calculated from port count |
| `inline_values` | no | Default values for input ports; keys are port names |
| `port_bindings` | no | Input port binding metadata. Legacy string values still load as hyperparameter bindings |
| `has_error` | no | Set by the runtime on execution failure; ignored on load |

---

## Port

```jsonc
{
  "name":        "template",
  "data_type":   "String",
  "description": "Format template with ${variable} placeholders",
  "required":    true
}
```

| Field | Required | Notes |
|-------|----------|-------|
| `name` | yes | Unique within the node's input or output port list. `snake_case`. |
| `data_type` | yes | See [Data Types](#data-types) below |
| `description` | no | Shown as tooltip in the GUI |
| `required` | yes | Only meaningful for input ports. If `true`, execution fails if this port has no incoming edge and no `inline_values` entry |

---

## EdgeDefinition

```jsonc
{
  "from_node_id": "node_1",
  "from_port":    "output",
  "to_node_id":   "node_2",
  "to_port":      "text"
}
```

**Validation rules enforced at runtime:**
- Both nodes must exist in the graph
- `from_port` must be an output port on the source node
- `to_port` must be an input port on the target node
- Port data types must be compatible
- Each input port may receive **at most one** incoming edge
- The graph must be a **DAG** (no cycles)

> **Legacy mode:** when `edges` is an empty array, the engine falls back to implicit name-matching: an output port `"foo"` automatically feeds any input port `"foo"` on any other node. Do not use this for new graphs.

---

## HyperParameter

Hyperparameters are graph-level variables that can be bound to input ports and overridden at runtime without editing the graph:

```jsonc
{
  "name":        "api_key",
  "group":       "default",
  "data_type":   "Password",
  "description": "OpenAI API key",
  "required":    true
}
```

| Field | Required | Notes |
|-------|----------|-------|
| `name` | yes | Unique name within the graph |
| `group` | no | Shared storage group. Defaults to `"default"` |
| `data_type` | yes | Same data type rules as ports |
| `description` | no | UI hint text |
| `required` | no | Whether execution is blocked when no value is present |

Hyperparameter *values* are stored in a shared local YAML file, not in the graph JSON.  
Graphs reuse values by `(group, name)`, so renaming or moving the graph file will not break value lookup.

---

## GraphVariable

Variables are graph-level run-scoped state with JSON-defined initial values:

```jsonc
{
  "name": "counter",
  "data_type": "Integer",
  "initial_value": 0
}
```

| Field | Required | Notes |
|-------|----------|-------|
| `name` | yes | Unique name within the graph |
| `data_type` | yes | Current UI supports String / Integer / Float / Boolean / Password |
| `initial_value` | no | Initial runtime value. Each graph run resets variables back to this value |

---

## Data Types

The `data_type` field is a string or JSON object corresponding to the `DataType` Rust enum in `src/node/data_value.rs`.

### Primitive types

| JSON value | Rust variant | Inline value format |
|-----------|-------------|---------------------|
| `"String"` | `DataType::String` | `"hello"` |
| `"Integer"` | `DataType::Integer` | `42` |
| `"Float"` | `DataType::Float` | `3.14` |
| `"Boolean"` | `DataType::Boolean` | `true` / `false` |
| `"Json"` | `DataType::Json` | any JSON value |
| `"Binary"` | `DataType::Binary` | *(not inline-editable)* |
| `"Password"` | `DataType::Password` | `"secret"` (masked in UI) |
| `"Any"` | `DataType::Any` | any value |

### Vec (homogeneous list)

Serialized as a JSON object with key `"Vec"`:

```json
{ "Vec": "OpenAIMessage" }
{ "Vec": "String" }
{ "Vec": "QQMessage" }
```

### Domain types

| JSON value | Description |
|-----------|-------------|
| `"MessageEvent"` | Bot platform message event |
| `"OpenAIMessage"` | LLM chat message `{role, content, tool_calls}` |
| `"QQMessage"` | QQ platform message segment |
| `"FunctionTools"` | LLM function-calling tool definitions |
| `"BotAdapterRef"` | Shared bot WebSocket connection |
| `"RedisRef"` | Redis configuration + connection manager |
| `"MySqlRef"` | MySQL configuration + connection pool |
| `"OpenAIMessageSessionCacheRef"` | Per-sender message history cache |
| `"LLModel"` | Language model configuration |
| `"LoopControlRef"` | Loop break signal |

### Backward-compatible aliases

The deserializer accepts these old names and converts them silently:

| Old name | Resolves to |
|---------|-------------|
| `"Message"` | `"OpenAIMessage"` |
| `"MessageList"` | `{"Vec": "OpenAIMessage"}` |
| `"QQMessageList"` | `{"Vec": "QQMessage"}` |
| `"Vec<OpenAIMessage>"` (Display string) | `{"Vec": "OpenAIMessage"}` |

---

## Complete example

A 3-node pipeline: **Bot Adapter → Extract Message → Preview**

```json
{
  "nodes": [
    {
      "id": "node_1",
      "name": "QQ Bot Adapter",
      "description": "Receives messages from QQ server",
      "node_type": "bot_adapter",
      "input_ports": [
        { "name": "qq_id",            "data_type": "String", "required": true  },
        { "name": "bot_server_url",   "data_type": "String", "required": true  },
        { "name": "bot_server_token", "data_type": "Password", "required": false }
      ],
      "output_ports": [
        { "name": "message_event", "data_type": "MessageEvent", "required": true }
      ],
      "position": { "x": 40.0, "y": 40.0 },
      "inline_values": {
        "qq_id": "123456789",
        "bot_server_url": "ws://localhost:3001"
      }
    },
    {
      "id": "node_2",
      "name": "Extract Message",
      "node_type": "extract_message_from_event",
      "input_ports": [
        { "name": "message_event", "data_type": "MessageEvent", "required": true }
      ],
      "output_ports": [
        { "name": "message", "data_type": { "Vec": "QQMessage" }, "required": true }
      ],
      "position": { "x": 300.0, "y": 40.0 }
    },
    {
      "id": "node_3",
      "name": "Preview",
      "node_type": "preview_string",
      "input_ports": [
        { "name": "text", "data_type": "String", "required": false }
      ],
      "output_ports": [
        { "name": "text", "data_type": "String", "required": true }
      ],
      "position": { "x": 560.0, "y": 40.0 }
    }
  ],
  "edges": [
    { "from_node_id": "node_1", "from_port": "message_event", "to_node_id": "node_2", "to_port": "message_event" },
    { "from_node_id": "node_2", "from_port": "message",       "to_node_id": "node_3", "to_port": "text" }
  ]
}
```

Data flow:

```
[bot_adapter] --message_event--> [extract_message_from_event] --message--> [preview_string]
```

---

## See also

- [Node Development Guide](./node-development.md) — creating and registering node types
- [Dynamic Port Nodes Guide](./dynamic-port-nodes.md) — config-driven port lists
- [node-system.md](../dev-guides/node-system.md) — execution engine and all built-in node types
