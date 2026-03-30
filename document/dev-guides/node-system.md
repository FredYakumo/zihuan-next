# Node System

This document is the detailed reference for the node system: architecture, node lifecycle, port and data types, registration, execution, and the standard node development workflow.

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

Use this document in two ways:

- To understand how the runtime works end-to-end
- To implement or review a new node against the actual system contracts

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
| `Simple` | Stateless transform, runs once per input set | `execute(inputs) -> outputs` |
| `EventProducer` | Stateful event source, runs in a loop | `on_start -> loop { on_update } -> on_cleanup` |

### Identity and construction expectations

- Each node lives in its own file
- Each node should expose `new(id: String, name: String) -> Self`
- `id()` must return the stable runtime node id
- `name()` should return the display label shown in the UI

---

## Ports and Data Types

### Port

```rust
pub struct Port {
    pub name: String,
    pub data_type: DataType,
    pub required: bool,
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
| `ty = ...` | yes | Type shorthand; `type = ...` is also accepted |
| `desc = "..."` | no | Tooltip shown in the GUI |
| `optional` | no | Sets `required = false`; default is required |

### Macro type syntax

| Macro syntax | Expands to |
|-------------|-----------|
| `ty = String` | `DataType::String` |
| `ty = Integer` | `DataType::Integer` |
| `ty = Float` | `DataType::Float` |
| `ty = Boolean` | `DataType::Boolean` |
| `ty = Json` | `DataType::Json` |
| `ty = Password` | `DataType::Password` |
| `ty = MessageEvent` | `DataType::MessageEvent` |
| `ty = OpenAIMessage` | `DataType::OpenAIMessage` |
| `ty = LLModel` | `DataType::LLModel` |
| `ty = BotAdapterRef` | `DataType::BotAdapterRef` |
| `ty = RedisRef` | `DataType::RedisRef` |
| `ty = MySqlRef` | `DataType::MySqlRef` |
| `ty = Vec(OpenAIMessage)` | `DataType::Vec(Box::new(DataType::OpenAIMessage))` |
| `ty = Vec(String)` | `DataType::Vec(Box::new(DataType::String))` |
| `ty = Custom("my_type")` | `DataType::Custom("my_type".to_string())` |
| `ty = DataType::String` | `DataType::String` |

### Dynamic-port nodes

For dynamic-port nodes, the dynamic direction must be implemented as a regular `fn input_ports` / `fn output_ports` method instead of the macro, because the port list is rebuilt from config at runtime. See [dynamic-port-nodes.md](../node/dynamic-port-nodes.md).

The `Port::new` builder is still available when the macro is impractical:

```rust
Port::new("port_name", DataType::String)
Port::new("port_name", DataType::String).optional()
Port::new("port_name", DataType::String).with_description("help text")
```

### DataType

Defined in `src/node/data_value.rs`:

```rust
pub enum DataType {
    Any,
    String,
    Integer,
    Float,
    Boolean,
    Json,
    Binary,
    Password,

    Vec(Box<DataType>),

    MessageEvent,
    OpenAIMessage,
    QQMessage,
    FunctionTools,

    BotAdapterRef,
    RedisRef,
    MySqlRef,
    OpenAIMessageSessionCacheRef,
    CurrentSessionRegistryRef,
    CurrentSessionLeaseRef,
    LLModel,
    LoopControlRef,

    Custom(String),
}
```

| Variant | Runtime value | Notes |
|---------|--------------|-------|
| `DataType::Any` | Any `DataValue` | Accepts all types |
| `DataType::String` | `DataValue::String(String)` | UTF-8 text |
| `DataType::Integer` | `DataValue::Integer(i64)` | 64-bit signed integer |
| `DataType::Float` | `DataValue::Float(f64)` | 64-bit float |
| `DataType::Boolean` | `DataValue::Boolean(bool)` | true / false |
| `DataType::Json` | `DataValue::Json(serde_json::Value)` | Arbitrary JSON |
| `DataType::Binary` | `DataValue::Binary(Vec<u8>)` | Raw bytes |
| `DataType::Password` | `DataValue::Password(String)` | Masked in UI |
| `DataType::Vec(inner)` | `DataValue::Vec(Vec<DataValue>)` | Homogeneous list |
| `DataType::MessageEvent` | `DataValue::MessageEvent(MessageEvent)` | Bot platform event |
| `DataType::OpenAIMessage` | `DataValue::OpenAIMessage(OpenAIMessage)` | LLM chat message |
| `DataType::QQMessage` | `DataValue::QQMessage(QQMessage)` | QQ message |
| `DataType::FunctionTools` | `DataValue::FunctionTools(...)` | LLM tool definitions |
| `DataType::BotAdapterRef` | `DataValue::BotAdapterRef(SharedBotAdapter)` | Shared bot connection |
| `DataType::RedisRef` | `DataValue::RedisRef(RedisConfig)` | Redis config |
| `DataType::MySqlRef` | `DataValue::MySqlRef(MySqlConfig)` | MySQL config + pool |
| `DataType::OpenAIMessageSessionCacheRef` | `DataValue::OpenAIMessageSessionCacheRef(...)` | Message cache |
| `DataType::CurrentSessionRegistryRef` | `DataValue::CurrentSessionRegistryRef(Arc<CurrentSessionRegistryRef>)` | Run-scoped sender session lock registry |
| `DataType::CurrentSessionLeaseRef` | `DataValue::CurrentSessionLeaseRef(Arc<CurrentSessionLeaseRef>)` | Lease used for precise session lock release |
| `DataType::LLModel` | `DataValue::LLModel(...)` | Language model config |
| `DataType::LoopControlRef` | `DataValue::LoopControlRef(Arc<LoopControl>)` | Loop break signal |

Session lock helpers:

- `current_session_list_provider` creates the shared run-scoped registry and a snapshot of active sender IDs.
- `sender_id_in_current_session` only observes whether a sender is currently active.
- `current_session_try_acquire` is the atomic operation that claims a sender lock and returns a lease.
- `current_session_release` releases by lease, so a stale worker cannot accidentally unlock a newer session.

**Compatibility rules:**

- `Any` is compatible with every type
- `Vec(A)` is compatible with `Vec(B)` iff `A` is compatible with `B`
- All other types must be strictly equal

### DataValue

`DataValue` is the runtime counterpart to `DataType`. Variants mirror `DataType`. A common access pattern:

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

## Validation and Inline Configuration

### Default validation behavior

The `Node` trait provides default `validate_inputs()` and `validate_outputs()` implementations that check:

- All required input ports have a value
- All values match their declared `DataType`

Override when the node has extra semantic rules:

```rust
fn validate_inputs(&self, inputs: &HashMap<String, DataValue>) -> Result<()> {
    if let Some(DataValue::Integer(n)) = inputs.get("count") {
        if *n < 1 {
            return Err(Error::ValidationError("count must be >= 1".into()));
        }
    }
    self.validate_inputs_default(inputs)
}
```

### `apply_inline_config`

Use `apply_inline_config()` when a node reads values configured directly on the node card rather than through incoming edges:

```rust
fn apply_inline_config(&mut self, inline_values: &HashMap<String, DataValue>) -> Result<()> {
    if let Some(DataValue::String(s)) = inline_values.get("mode") {
        self.mode = s.clone();
    }
    if let Some(DataValue::Integer(n)) = inline_values.get("max_retries") {
        self.max_retries = *n as u32;
    }
    Ok(())
}
```

This hook runs during `prepare_for_execution()`, before normal execution starts. For dynamic-port nodes, this is also where the dynamic port list is usually rebuilt.

### `on_graph_start`

Use `on_graph_start()` for one-time initialization that should happen after the graph is assembled but before node execution begins.

---

## Node Registry

All node types must be registered in `src/node/registry.rs -> init_node_registry()`.

The registry is a global thread-safe singleton:

```rust
pub static NODE_REGISTRY: Lazy<NodeRegistry> = Lazy::new(NodeRegistry::new);
```

### Registration macro

```rust
register_node!(
    "my_node",
    "My Node",
    "工具",
    "What it does",
    MyNodeStruct
);
```

The macro expects `MyNodeStruct::new(id: String, name: String)`.

### Manual registration

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
NODE_REGISTRY.create_node("format_string", "node_1", "Format")
NODE_REGISTRY.get_node_ports("format_string")
NODE_REGISTRY.get_node_dynamic_port_flags("format_string")
NODE_REGISTRY.is_event_producer("bot_adapter")
NODE_REGISTRY.get_all_types()
NODE_REGISTRY.get_categories()
```

The registry inspects port metadata by creating a temporary node instance with `id = "__probe__"`. That means `input_ports()` and `output_ports()` must stay deterministic when no config has been applied yet.

---

## Execution Engine

### Edge mode vs. implicit mode

**Edge mode** is the normal mode. When the `edges` array is non-empty, every connection is explicit:

```json
{ "from_node_id": "n1", "from_port": "output", "to_node_id": "n2", "to_port": "input" }
```

Rules enforced:

- Both nodes and ports must exist
- Port types must match
- Each input port may receive at most one incoming edge
- The graph must remain a DAG

**Implicit mode** is legacy behavior. When `edges` is empty, an output port named `"foo"` is automatically bound to any input port named `"foo"` on any other node. Do not use this for new graphs.

### Topological sort

The engine uses Kahn's algorithm:

```
1. Compute in-degree of each node
2. Enqueue all nodes with in-degree = 0
3. Dequeue a node, execute it, decrement downstream in-degree
4. Enqueue newly zero-in-degree nodes
5. Repeat until all nodes are processed
```

If some nodes remain unprocessed, the graph contains a cycle and execution fails.

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
Execute base layer

For each root EventProducer:
  node.on_start(base_layer_outputs)
  loop:
    if stop_flag is set: break
    outputs = node.on_update()
    if outputs is None: break
    merge outputs into data_pool
    execute all downstream nodes reachable from this EventProducer
  node.on_cleanup()
```

### Stop flag

EventProducers receive an `Arc<AtomicBool>` via `set_stop_flag()` and should check it regularly:

```rust
fn on_update(&mut self) -> Result<Option<HashMap<String, DataValue>>> {
    if self.stop_flag.load(Ordering::Relaxed) {
        return Ok(None);
    }
    // normal logic...
}
```

The UI calls `graph.request_stop()` to set the flag.

### Execution callback

The graph can expose per-node execution state for live previews:

```rust
graph.set_execution_callback(|node_id, inputs, outputs| {
    // update UI preview, etc.
});
```

---

## Node Development Workflow

This is the recommended implementation path for a new node.

### 1. Create the node file

Put the node in the correct module. Utility or transform nodes usually go in `src/node/util/`.

```rust
use std::collections::HashMap;
use crate::error::{Error, Result};
use crate::node::{DataType, DataValue, Node, NodeType, Port, node_input, node_output};

pub struct UppercaseNode {
    id: String,
    name: String,
}

impl UppercaseNode {
    pub fn new(id: String, name: String) -> Self {
        Self { id, name }
    }
}
```

### 2. Implement a Simple node

```rust
impl Node for UppercaseNode {
    fn id(&self) -> &str { &self.id }
    fn name(&self) -> &str { &self.name }
    fn description(&self) -> Option<&str> { Some("Converts text to uppercase") }

    node_input![
        port! { name = "text", ty = String, desc = "Input text" },
    ];

    node_output![
        port! { name = "result", ty = String, desc = "Uppercased text" },
    ];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        let text = match inputs.get("text") {
            Some(DataValue::String(s)) => s.clone(),
            _ => return Err(Error::ValidationError("missing or wrong-type 'text'".into())),
        };

        let mut out = HashMap::new();
        out.insert("result".to_string(), DataValue::String(text.to_uppercase()));
        Ok(out)
    }
}
```

Use `optional` on input ports that may legally be left unconnected.

### 3. Implement an EventProducer node

Use `EventProducer` for nodes that emit values over time from timers, sockets, or polling:

```rust
use std::collections::HashMap;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::time::{Duration, Instant};
use crate::error::Result;
use crate::node::{DataType, DataValue, Node, NodeType, Port, node_input, node_output};

pub struct TimerNode {
    id: String,
    name: String,
    interval_ms: u64,
    tick: u64,
    last_tick: Option<Instant>,
    stop_flag: Arc<AtomicBool>,
}

impl TimerNode {
    pub fn new(id: String, name: String) -> Self {
        Self {
            id,
            name,
            interval_ms: 1000,
            tick: 0,
            last_tick: None,
            stop_flag: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Node for TimerNode {
    fn id(&self) -> &str { &self.id }
    fn name(&self) -> &str { &self.name }
    fn node_type(&self) -> NodeType { NodeType::EventProducer }

    fn set_stop_flag(&mut self, flag: Arc<AtomicBool>) {
        self.stop_flag = flag;
    }

    node_input![
        port! { name = "interval_ms", ty = Integer, desc = "Interval in milliseconds (default 1000)", optional },
    ];

    node_output![
        port! { name = "tick", ty = Integer, desc = "Tick counter (starts at 1)" },
    ];

    fn on_start(&mut self, inputs: HashMap<String, DataValue>) -> Result<()> {
        if let Some(DataValue::Integer(ms)) = inputs.get("interval_ms") {
            self.interval_ms = (*ms).max(1) as u64;
        }
        self.tick = 0;
        self.last_tick = Some(Instant::now());
        Ok(())
    }

    fn on_update(&mut self) -> Result<Option<HashMap<String, DataValue>>> {
        if self.stop_flag.load(Ordering::Relaxed) {
            return Ok(None);
        }

        if let Some(last) = self.last_tick {
            if last.elapsed() < Duration::from_millis(self.interval_ms) {
                std::thread::sleep(Duration::from_millis(10));
                return self.on_update();
            }
        }

        self.tick += 1;
        self.last_tick = Some(Instant::now());

        let mut out = HashMap::new();
        out.insert("tick".to_string(), DataValue::Integer(self.tick as i64));
        Ok(Some(out))
    }

    fn on_cleanup(&mut self) -> Result<()> {
        log::info!("Timer '{}' completed {} ticks", self.id, self.tick);
        Ok(())
    }

    fn execute(&mut self, _inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        Ok(HashMap::new())
    }
}
```

EventProducer contract:

- `on_start()` runs once before the loop
- `on_update()` returns `Ok(Some(outputs))` to emit data
- `on_update()` returns `Ok(None)` to stop naturally
- `on_cleanup()` runs after the loop ends
- `set_stop_flag()` must persist the flag for cooperative stop

### 4. Export from the parent module

Add the node to the corresponding `mod.rs`:

```rust
mod uppercase;
pub use uppercase::UppercaseNode;
```

### 5. Register the node

Register it in `src/node/registry.rs -> init_node_registry()`:

```rust
register_node!(
    "uppercase",
    "Uppercase",
    "工具",
    "Converts text to uppercase",
    UppercaseNode
);
```

### 6. Test the node

Unit test in the node file:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uppercase_basic() {
        let mut node = UppercaseNode::new("test".into(), "Test".into());

        let mut inputs = HashMap::new();
        inputs.insert("text".to_string(), DataValue::String("hello world".into()));

        let outputs = node.execute(inputs).unwrap();
        assert_eq!(
            outputs.get("result"),
            Some(&DataValue::String("HELLO WORLD".into()))
        );
    }
}
```

Integration test with `NodeGraph`:

```rust
#[test]
fn test_uppercase_in_graph() {
    use crate::node::{NodeGraph, graph_io::NodeGraphDefinition};
    use crate::node::registry::{build_node_graph_from_definition, init_node_registry};

    init_node_registry().unwrap();

    let json = r#"{
      "nodes": [{
        "id": "n1", "name": "Upper", "node_type": "uppercase",
        "input_ports":  [{"name": "text",   "data_type": "String", "required": true}],
        "output_ports": [{"name": "result", "data_type": "String", "required": true}],
        "inline_values": {"text": "hello"}
      }],
      "edges": []
    }"#;

    let def: NodeGraphDefinition = serde_json::from_str(json).unwrap();
    let result = build_node_graph_from_definition(&def).unwrap()
        .execute_and_capture_results()
        .unwrap();

    let outputs = result.node_results.get("n1").unwrap();
    assert_eq!(outputs.get("result"), Some(&DataValue::String("HELLO".into())));
}
```

### 7. Final checklist

- Node struct is in its own file
- `new(id: String, name: String)` exists
- Ports are declared clearly and use `snake_case`
- `type_id` is unique and stable
- EventProducers check the stop flag
- Node is exported from parent `mod.rs`
- Node is registered in `src/node/registry.rs`
- Happy-path and failure-path tests are present

---

## Built-in Node Types

### Utility (工具)

| type_id | Description |
|---------|-------------|
| `format_string` | Template formatting via `${var}` syntax; dynamic input ports |
| `conditional` | Routes to `true_output` / `false_output` based on a Boolean |
| `switch_gate` | Passes input through when `enabled=true`, blocks otherwise |
| `loop` | Repeats execution; pair with `loop_break` |
| `loop_break` | Signals the loop to exit when `condition=true` |
| `array_get` | Gets element at index from a Vec; negative indices supported |
| `stack` | Wraps a single value in a single-element Vec |
| `concat_vec` | Concatenates two same-element-type Vecs |
| `json_parser` | Parses a JSON string to `Json` |
| `json_extract` | Extracts typed fields from `Json`; dynamic output ports |
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
| `openai_message_session_cache` | Per-sender message history cache node |
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
| `bot_adapter` | EventProducer; connects via WebSocket and emits `MessageEvent` |
| `send_friend_message` | Sends a message to a QQ friend |
| `send_group_message` | Sends a message to a QQ group |
| `message_event_type_filter` | Filters events by type |
| `extract_sender_id_from_event` | Extracts `sender_id` from a `MessageEvent` |
| `extract_group_id_from_event` | Extracts `group_id` from a `MessageEvent` |
| `extract_message_from_event` | Extracts message body from a `MessageEvent` |

### Database (数据库)

| type_id | Description |
|---------|-------------|
| `redis` | Creates a `RedisRef` |
| `mysql` | Creates a `MySqlRef` |

### Message Store (消息存储)

| type_id | Description |
|---------|-------------|
| `message_cache` | In-memory message storage |
| `message_mysql_persistence` | Persists messages to MySQL |
