# Node Development Guide

> **Prerequisites:** Read [node-system.md](../dev-guides/node-system.md) first for a full picture of how the system works.

This guide walks through creating, registering, and testing a node from scratch.

---

## Table of contents

- [Creating a Simple node](#creating-a-simple-node)
- [Creating an EventProducer node](#creating-an-eventproducer-node)
- [Registering your node](#registering-your-node)
- [Declaring ports with node_input! / node_output!](#declaring-ports-with-node_input--node_output)
- [Data types reference](#data-types-reference)
- [Validation](#validation)
- [Testing](#testing)
- [Checklist](#checklist)

---

## Creating a Simple node

A Simple node runs once per input set and returns outputs. It is stateless.

### 1. Create the file

Put it in the appropriate module. Utility/transform nodes go in `src/node/util/`:

```rust
// src/node/util/uppercase.rs

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

### 2. Implement the Node trait

Use `node_input!` and `node_output!` to declare ports (see the [macro reference](#declaring-ports-with-node_input--node_output) for full syntax):

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

**Required ports are `required = true` by default.** Use `optional` to allow unconnected ports:

```rust
node_input![
    port! { name = "text",   ty = String, desc = "Input text" },
    port! { name = "suffix", ty = String, desc = "Optional suffix", optional },
];
```

### 3. Export from the module

Add to `src/node/util/mod.rs`:

```rust
mod uppercase;
pub use uppercase::UppercaseNode;
```

---

## Creating an EventProducer node

An EventProducer maintains a lifecycle loop and emits outputs on each iteration. Use this for nodes that receive external events (WebSocket, timer, polling).

```rust
// src/node/util/timer.rs

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
            id, name,
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
        // Check stop flag first
        if self.stop_flag.load(Ordering::Relaxed) {
            return Ok(None); // returning None exits the loop
        }

        // Wait for interval
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

    // execute() is unused for EventProducers but must compile
    fn execute(&mut self, _inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        Ok(HashMap::new())
    }
}
```

**EventProducer contract:**
- `on_start()` — called once before the loop; read inputs here
- `on_update()` — called in a loop; return `Ok(Some(outputs))` to emit, `Ok(None)` to exit
- `on_cleanup()` — called after the loop regardless of how it ended
- `set_stop_flag()` — must store the flag and check it in `on_update()`

---

## Registering your node

All nodes must be registered in `src/node/registry.rs → init_node_registry()`.

### Using the macro (preferred)

```rust
// In init_node_registry():
use crate::node::util::UppercaseNode;

register_node!(
    "uppercase",          // type_id — used in JSON node_type field; never change after publishing
    "Uppercase",          // display name shown in the node palette
    "工具",               // category (see code-conventions.md for category labels)
    "Converts text to uppercase",
    UppercaseNode
);
```

The macro requires `YourStruct::new(id: String, name: String)` to exist.

### Manual registration (complex constructors)

```rust
NODE_REGISTRY.register(
    "timer",
    "Timer",
    "工具",
    "Emits events at fixed intervals",
    Arc::new(|id: String, name: String| Box::new(TimerNode::new(id, name))),
)?;
```

---

## Declaring ports with node_input! / node_output!

`node_input!` and `node_output!` are procedural macros defined in the `node_macros` crate. They generate the `fn input_ports` and `fn output_ports` methods from a compact declarative syntax, and perform duplicate-name checking at compile time.

### Syntax

```rust
node_input![
    port! { name = "port_name", ty = TypeName, desc = "help text" },
    port! { name = "optional_port", ty = TypeName, optional },
];

node_output![
    port! { name = "result", ty = String },
];
```

Each `port! { ... }` entry supports these fields:

| Field | Alias | Required | Description |
|-------|-------|----------|-------------|
| `name = "..."` | | yes | Port name (must be a string literal) |
| `ty = ...` | `type = ...` | yes | Port data type (see below) |
| `desc = "..."` | | no | Tooltip text shown in the GUI |
| `optional` | `optional = true` | no | Makes the port optional (default: required) |

### Type syntax

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
| `ty = DataType::String` | `DataType::String` (full path also accepted) |

### Real example (BrainNode)

```rust
node_input![
    port! { name = "llm_model",    ty = LLModel,           desc = "LLM model reference from llm_api node" },
    port! { name = "messages",     ty = Vec(OpenAIMessage), desc = "Message list (system/user/assistant roles)" },
    port! { name = "tools_config", ty = Json,              desc = "Tools config managed by tool editor", optional },
];
```

### Trailing comma

A trailing comma after the last `port!` entry is allowed and recommended (matches Rust convention).

### When NOT to use the macros

Dynamic-port nodes must compute their port lists at runtime from configuration. For these nodes, do **not** use `node_input!` / `node_output!` for the dynamic direction — implement `input_ports()` / `output_ports()` manually instead. See [dynamic-port-nodes.md](./dynamic-port-nodes.md).

---

## Data types reference

Full list of `DataType` variants (defined in `src/node/data_value.rs`):

| Variant | Runtime value | Notes |
|---------|--------------|-------|
| `DataType::Any` | Any `DataValue` | Accepts all types (wildcard) |
| `DataType::String` | `DataValue::String(String)` | UTF-8 text |
| `DataType::Integer` | `DataValue::Integer(i64)` | 64-bit signed integer |
| `DataType::Float` | `DataValue::Float(f64)` | 64-bit float |
| `DataType::Boolean` | `DataValue::Boolean(bool)` | true / false |
| `DataType::Json` | `DataValue::Json(serde_json::Value)` | Arbitrary JSON |
| `DataType::Binary` | `DataValue::Binary(Vec<u8>)` | Raw bytes |
| `DataType::Password` | `DataValue::Password(String)` | Like String but masked in UI |
| `DataType::Vec(inner)` | `DataValue::Vec(Vec<DataValue>)` | Homogeneous list |
| `DataType::MessageEvent` | `DataValue::MessageEvent(MessageEvent)` | Bot platform event |
| `DataType::OpenAIMessage` | `DataValue::OpenAIMessage(OpenAIMessage)` | LLM chat message |
| `DataType::QQMessage` | `DataValue::QQMessage(QQMessage)` | QQ message segment |
| `DataType::FunctionTools` | `DataValue::FunctionTools(...)` | LLM tool definitions |
| `DataType::BotAdapterRef` | `DataValue::BotAdapterRef(SharedBotAdapter)` | Shared bot connection |
| `DataType::RedisRef` | `DataValue::RedisRef(RedisConfig)` | Redis config |
| `DataType::MySqlRef` | `DataValue::MySqlRef(MySqlConfig)` | MySQL config + pool |
| `DataType::OpenAIMessageSessionCacheRef` | `DataValue::OpenAIMessageSessionCacheRef(...)` | Message cache |
| `DataType::LLModel` | `DataValue::LLModel(...)` | Language model config |
| `DataType::LoopControlRef` | `DataValue::LoopControlRef(Arc<LoopControl>)` | Loop break signal |

---

## Validation

The `Node` trait provides default `validate_inputs()` and `validate_outputs()` implementations that check:
- All required input ports have a value
- All values match their declared `DataType`

Override for custom rules:

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

---

## Configuration: apply_inline_config

If your node reads from inline values (set directly on the node card without an incoming edge), override `apply_inline_config()`:

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

This is called before execution starts (`prepare_for_execution()`). For **dynamic-port nodes**, this is where you rebuild the dynamic port list. See [dynamic-port-nodes.md](./dynamic-port-nodes.md).

---

## Testing

### Unit test (in the node file)

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

    #[test]
    fn test_uppercase_missing_input() {
        let mut node = UppercaseNode::new("test".into(), "Test".into());
        let result = node.execute(HashMap::new());
        assert!(result.is_err());
    }
}
```

### Integration test with NodeGraph

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

---

## Checklist

Use this checklist before submitting a new node:

- [ ] Node struct in its own file under the appropriate module
- [ ] `new(id: String, name: String)` constructor
- [ ] All `Node` trait methods implemented
- [ ] Ports declared using `node_input!` / `node_output!` macros (except dynamic-port directions)
- [ ] `execute()` always returns `Ok(...)` or a meaningful `Err(...)`
- [ ] EventProducers: `set_stop_flag()` stored and checked in `on_update()`
- [ ] Registered in `src/node/registry.rs → init_node_registry()`
- [ ] Exported from parent `mod.rs`
- [ ] Unit tests for happy path and error cases
- [ ] `type_id` is unique and `snake_case`
- [ ] Port names are descriptive and `snake_case`
