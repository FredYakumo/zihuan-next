# Node System

This document describes how the node system works end-to-end: the `Node` trait, data types, the execution engine, and the EventProducer lifecycle.

---

## Overview

A **NodeGraph** is a directed acyclic graph (DAG) whose vertices are nodes and whose edges are typed port connections. The engine executes nodes in topological order, passing data through ports.

```
NodeGraph
├── nodes: HashMap<id, Box<dyn Node>>   ← business logic units
├── edges: Vec<EdgeDefinition>           ← typed port connections
├── inline_values                        ← static default values per port
└── stop_flag: Arc<AtomicBool>           ← lets the UI interrupt execution
```

---

## The Node Trait

Defined in `src/node/mod.rs`. Every node type implements this trait:

```rust
pub trait Node: Send + Sync {
    // Identity
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn description(&self) -> Option<&str> { None }

    // Execution model
    fn node_type(&self) -> NodeType { NodeType::Simple }

    // Port declarations
    fn input_ports(&self) -> Vec<Port>;
    fn output_ports(&self) -> Vec<Port>;
    fn has_dynamic_input_ports(&self) -> bool { false }
    fn has_dynamic_output_ports(&self) -> bool { false }

    // Configuration hooks
    fn apply_inline_config(&mut self, inline_values: &HashMap<String, DataValue>) -> Result<()>;
    fn on_graph_start(&mut self) -> Result<()> { Ok(()) }
    fn set_stop_flag(&mut self, flag: Arc<AtomicBool>) {}

    // Simple node execution
    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>>;

    // EventProducer lifecycle
    fn on_start(&mut self, inputs: HashMap<String, DataValue>) -> Result<()> { Ok(()) }
    fn on_update(&mut self) -> Result<Option<HashMap<String, DataValue>>> { Ok(None) }
    fn on_cleanup(&mut self) -> Result<()> { Ok(()) }

    // Validation (default implementation covers most cases)
    fn validate_inputs(&self, inputs: &HashMap<String, DataValue>) -> Result<()>;
    fn validate_outputs(&self, outputs: &HashMap<String, DataValue>) -> Result<()>;
}
```

### Node types

```rust
pub enum NodeType { Simple, EventProducer }
```

| Type | Use case | Entry point |
|------|----------|-------------|
| `Simple` | Stateless transform, runs once per input set | `execute(inputs) → outputs` |
| `EventProducer` | Stateful event source, runs in a loop | `on_start → loop { on_update } → on_cleanup` |

---

## Ports and Data Types

### Port

```rust
pub struct Port {
    pub name: String,
    pub data_type: DataType,
    pub required: bool,           // only meaningful for input ports
    pub description: Option<String>,
}
```

Ports are declared using the `node_input!` / `node_output!` procedural macros (defined in the `node_macros` crate). They generate the `fn input_ports` / `fn output_ports` method and check for duplicate names at compile time.

```rust
use crate::node::{DataType, DataValue, Node, Port, node_input, node_output};

impl Node for MyNode {
    node_input![
        port! { name = "text",     ty = String,  desc = "Input text" },
        port! { name = "count",    ty = Integer, desc = "Repeat count", optional },
        port! { name = "messages", ty = Vec(OpenAIMessage), desc = "Chat history" },
    ];

    node_output![
        port! { name = "result",  ty = String },
        port! { name = "message", ty = OpenAIMessage },
    ];

    // ...
}
```

Each `port! { ... }` field:

| Field | Required | Notes |
|-------|----------|-------|
| `name = "..."` | yes | Port name string literal |
| `ty = ...` (alias `type = ...`) | yes | Type shorthand — bare identifier maps to `DataType::Ident`; `Vec(T)` maps to `DataType::Vec(Box::new(DataType::T))`; full `DataType::X` path also accepted |
| `desc = "..."` | no | Tooltip shown in the GUI |
| `optional` | no | Flag that sets `required = false`; default is required |

For **dynamic-port nodes** the dynamic direction must be implemented as a regular `fn input_ports` / `fn output_ports` method instead of the macro (because the port list is built at runtime from config). See [dynamic-port-nodes.md](../node/dynamic-port-nodes.md).

The `Port::new` builder is still available for manual use when the macro is impractical:

```rust
Port::new("port_name", DataType::String)             // required = true
Port::new("port_name", DataType::String).optional()  // required = false
Port::new("port_name", DataType::String).with_description("help text")
```

### DataType

Defined in `src/node/data_value.rs`:

```rust
pub enum DataType {
    // Primitive
    Any,        // compatible with every other type (wildcard)
    String,
    Integer,    // i64
    Float,      // f64
    Boolean,
    Json,       // serde_json::Value
    Binary,     // Vec<u8>
    Password,   // String subtype, shown as masked input in the UI

    // Composite
    Vec(Box<DataType>),  // homogeneous list, e.g. Vec(OpenAIMessage)

    // Domain types
    MessageEvent,                   // bot platform message event
    OpenAIMessage,                  // LLM chat message {role, content, tool_calls}
    QQMessage,                      // QQ platform message
    FunctionTools,                  // LLM function-calling tool specs

    // Reference types (shared resources passed between nodes)
    BotAdapterRef,                  // shared bot WebSocket connection
    RedisRef,                       // Redis config + live connection manager
    MySqlRef,                       // MySQL config + live connection pool
    OpenAIMessageSessionCacheRef,   // per-sender message history cache
    LLModel,                        // language model config
    LoopControlRef,                 // loop break signal

    Custom(String),                 // extension point
}
```

**Compatibility rules:**
- `Any` is compatible with every type (bidirectional)
- `Vec(A)` is compatible with `Vec(B)` iff A is compatible with B
- All other types must be strictly equal

### DataValue

`DataValue` is the runtime counterpart to `DataType`. Variants mirror `DataType`. Common access pattern:

```rust
fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
    let text = match inputs.get("text") {
        Some(DataValue::String(s)) => s.clone(),
        _ => return Err(Error::ValidationError("missing or wrong-type 'text' input".into())),
    };

    let mut out = HashMap::new();
    out.insert("result".to_string(), DataValue::String(text.to_uppercase()));
    Ok(out)
}
```

---

## Node Registry

All node types must be registered in `src/node/registry.rs → init_node_registry()`.

The registry is a global thread-safe singleton:

```rust
pub static NODE_REGISTRY: Lazy<NodeRegistry> = Lazy::new(NodeRegistry::new);
// Internally: factories: RwLock<HashMap<type_id, NodeFactory>>
//             metadata:  RwLock<HashMap<type_id, NodeTypeMetadata>>
```

### Registration macro (preferred)

```rust
register_node!(
    "my_node",      // type_id used in the JSON `node_type` field
    "My Node",      // display name shown in the UI node palette
    "工具",         // category label (Chinese convention, see code-conventions.md)
    "What it does", // description shown on hover
    MyNodeStruct    // must implement new(id: String, name: String)
);
```

### Manual registration (for complex constructors)

```rust
NODE_REGISTRY.register(
    "my_node",
    "My Node",
    "工具",
    "What it does",
    Arc::new(|id: String, name: String| Box::new(MyNode::new(id, name))),
)?;
```

### Registry queries

```rust
NODE_REGISTRY.create_node("format_string", "node_1", "Format")  // instantiate
NODE_REGISTRY.get_node_ports("format_string")                   // (input_ports, output_ports)
NODE_REGISTRY.get_node_dynamic_port_flags("format_string")      // (has_dyn_in, has_dyn_out)
NODE_REGISTRY.is_event_producer("bot_adapter")                  // → bool
NODE_REGISTRY.get_all_types()                                    // → Vec<NodeTypeMetadata>
NODE_REGISTRY.get_categories()                                   // → Vec<String>
```

**Registry probe pattern:** The registry inspects port metadata by creating a temporary node instance with `id = "__probe__"`. This means `input_ports()` and `output_ports()` must return a deterministic result even with no configuration applied.

---

## Execution Engine

### Edge mode vs. implicit mode

**Edge mode (recommended):** when the `edges` array is non-empty, every connection is explicit:

```json
{ "from_node_id": "n1", "from_port": "output", "to_node_id": "n2", "to_port": "input" }
```

Rules enforced:
- Both nodes and ports must exist
- Port types must match
- Each input port may receive at most one incoming edge
- Graph must be a DAG (no cycles)

**Implicit mode (legacy):** when `edges` is empty, an output port named `"foo"` is automatically bound to any input port named `"foo"` on any other node. Do not use this for new graphs.

### Topological sort (Kahn's algorithm)

```
1. Compute in-degree of each node (number of incoming edges)
2. Enqueue all nodes with in-degree = 0
3. Dequeue a node, execute it, decrement in-degree of its downstream neighbors
4. Enqueue newly zero-in-degree nodes
5. Repeat until all nodes are processed; report an error if any remain (cycle)
```

### Execution flow: Simple-only graph

```
prepare_for_execution():
  for each node: on_graph_start()
  for each node: apply_inline_config(inline_values[node_id])

for each node in topological order:
  inputs = collect from data_pool (via edges) + inline_values fallback
  node.validate_inputs(inputs)
  outputs = node.execute(inputs)
  node.validate_outputs(outputs)
  write outputs into data_pool
```

### Execution flow: graph with EventProducers

```
prepare_for_execution()  (same as above)

Identify "base layer": Simple nodes not reachable from any EventProducer
Execute base layer (same as Simple-only flow above)

For each root EventProducer (no upstream EventProducer):
  node.on_start(base_layer_outputs)
  loop:
    if stop_flag is set: break
    outputs = node.on_update()
    if outputs is None: break      ← EventProducer signals natural end
    merge outputs into data_pool
    execute all downstream nodes reachable from this EventProducer
  node.on_cleanup()
```

### Stop flag

EventProducers receive an `Arc<AtomicBool>` via `set_stop_flag()`. They must check it regularly:

```rust
fn on_update(&mut self) -> Result<Option<HashMap<String, DataValue>>> {
    if self.stop_flag.load(Ordering::Relaxed) {
        return Ok(None); // returning None exits the loop
    }
    // normal logic...
}
```

The UI calls `graph.request_stop()` which atomically sets the flag to `true`.

### Execution callback

Register a callback to observe each node's execution in real time (used by the UI for live previews):

```rust
graph.set_execution_callback(|node_id, inputs, outputs| {
    // update UI preview, etc.
});
```

---

## Built-in Node Types

### Utility (工具)

| type_id | Description |
|---------|-------------|
| `format_string` | Template formatting via `${var}` syntax — dynamic input ports |
| `conditional` | Routes to `true_output` / `false_output` based on a Boolean |
| `switch_gate` | Passes input through when `enabled=true`, blocks otherwise |
| `loop` | Repeats execution; pair with `loop_break` |
| `loop_break` | Signals the loop to exit when `condition=true` |
| `array_get` | Gets element at index from a Vec (negative indices supported) |
| `stack` | Wraps a single value in a single-element Vec |
| `concat_vec` | Concatenates two same-element-type Vecs |
| `json_parser` | Parses a JSON string to `Json` |
| `json_extract` | Extracts typed fields from a `Json` — dynamic output ports |
| `preview_string` | Displays a string value on the node card |
| `preview_message_list` | Displays a message list on the node card |

### Data (数据)

| type_id | Description |
|---------|-------------|
| `string_data` | User-provided string constant |
| `current_time` | Emits current local time as a string |
| `message_list_data` | User-provided OpenAIMessage list |
| `qq_message_list_data` | User-provided QQMessage list |

### Message (消息)

| type_id | Description |
|---------|-------------|
| `message_content` | Extracts `content` string from an `OpenAIMessage` |
| `string_to_openai_message` | Wraps a String + role into an `OpenAIMessage` |
| `string_to_plain_text` | Converts String to a QQ plain-text message segment |
| `openai_message_session_cache` | Per-sender message history cache node (manages cache lifecycle) |
| `openai_message_session_cache_get` | Retrieves cached messages for a sender |
| `openai_message_session_cache_clear` | Clears cached messages for a sender |
| `tool_result` | Wraps a tool call result as an `OpenAIMessage` |

### AI (AI)

| type_id | Description |
|---------|-------------|
| `llm_api` | Configures an OpenAI-compatible API endpoint and key |
| `llm_infer` | Calls an LLM to generate text |
| `brain` | LLM inference with dynamic tool-call output ports |

### Adapter (适配器)

| type_id | Description |
|---------|-------------|
| `bot_adapter` | EventProducer — connects via WebSocket, emits `MessageEvent` per message |
| `send_friend_message` | Sends a message to a QQ friend |
| `send_group_message` | Sends a message to a QQ group |
| `message_event_type_filter` | Filters events by type (group / private) |
| `extract_sender_id_from_event` | Extracts `sender_id` from a `MessageEvent` |
| `extract_group_id_from_event` | Extracts `group_id` from a `MessageEvent` |
| `extract_message_from_event` | Extracts message body from a `MessageEvent` |

### Database (数据库)

| type_id | Description |
|---------|-------------|
| `redis` | Creates a `RedisRef` (config + lazy connection) |
| `mysql` | Creates a `MySqlRef` (config + live connection pool) |

### Message Store (消息存储)

| type_id | Description |
|---------|-------------|
| `message_cache` | In-memory message storage |
| `message_mysql_persistence` | Persists messages to MySQL |
