# Code Conventions

Naming rules, file organization patterns, common utilities, error handling, and logging standards used throughout zihuan-next.

---

## Naming conventions

### Rust

| Item | Convention | Example |
|------|-----------|---------|
| Types, traits, enums | `UpperCamelCase` | `FormatStringNode`, `DataType`, `NodeGraph` |
| Functions, methods, fields | `snake_case` | `execute()`, `input_ports()`, `node_type` |
| Constants | `SCREAMING_SNAKE_CASE` | `GRID_SIZE`, `NODE_WIDTH_CELLS` |
| Modules / files | `snake_case` | `format_string.rs`, `node_graph_view_vm.rs` |
| Node struct names | `<Purpose>Node` | `FormatStringNode`, `BrainNode`, `BotAdapterNode` |

### Node type IDs (registry keys)

Node `type_id` values are `snake_case` strings matching the node's primary purpose:

```
format_string       json_extract        llm_api
conditional         brain               bot_adapter
switch_gate         message_content     send_friend_message
```

These values appear in the `node_type` field of graph JSON files. Once published, changing them breaks existing graphs.

### Port names

Port names are `snake_case`, describe the data they carry, and must be unique within their direction (inputs or outputs can share a name as they are separate namespaces):

```
input:  text, template, json, enabled, messages, sender_id, llm_model
output: result, output, message, response, assistant_message
```

### Node categories (registry label)

Categories use Chinese strings by convention (to match the existing UI palette groupings):

| Chinese label | English meaning |
|--------------|-----------------|
| `工具` | Utility / control flow |
| `AI` | LLM / AI inference |
| `消息` | Message handling |
| `数据` | Data sources / constants |
| `数据库` | Database connections |
| `消息存储` | Message storage |
| `适配器` | Bot platform adapters |

---

## File organization

### One node per file

Each node struct lives in its own file. Simple utility nodes go under `src/node/util/`:

```
src/node/util/
├── mod.rs               ← re-exports all util nodes
├── format_string.rs     ← FormatStringNode
├── json_extract.rs      ← JsonExtractNode
├── conditional.rs       ← ConditionalNode
└── ...
```

Bot-adapter nodes live under `src/bot_adapter/`, LLM nodes under `src/llm/`.

After creating a new file, add it to the parent `mod.rs` and register it in `src/node/registry.rs → init_node_registry()`.

### Module structure

```
src/
├── main.rs
├── error.rs                    ← crate-wide error type and Result alias
├── node/
│   ├── mod.rs                  ← Node trait, NodeGraph, Port, execution engine
│   ├── data_value.rs           ← DataType, DataValue, reference config structs
│   ├── graph_io.rs             ← NodeGraphDefinition, serde, auto-fix
│   ├── registry.rs             ← NODE_REGISTRY, register_node! macro
│   ├── util/                   ← utility and control-flow nodes
│   ├── database.rs             ← RedisNode, MySqlNode
│   ├── message_nodes.rs        ← MySQL message persistence
│   └── message_cache.rs        ← in-memory message cache
├── llm/
│   ├── mod.rs                  ← LLM types (OpenAIMessage, FunctionTool, etc.)
│   ├── brain_node.rs
│   ├── llm_api_node.rs
│   └── llm_infer_node.rs
├── bot_adapter/
│   ├── mod.rs
│   ├── adapter.rs              ← SharedBotAdapter, WebSocket loop
│   ├── node_impl.rs            ← BotAdapterNode (EventProducer)
│   └── ...
└── ui/
    └── ...                     ← see ui-architecture.md
```

---

## Common utilities

### Error handling

The crate uses a single `Error` enum and `Result` alias in `src/error.rs`:

```rust
use crate::error::{Error, Result};

// Return an error
return Err(Error::ValidationError("message here".into()));

// Propagate with ?
let value = some_fn()?;
```

Common `Error` variants:
- `Error::ValidationError(String)` — invalid input, type mismatch, missing required port
- `Error::ExecutionError(String)` — runtime failure during node execution
- `Error::IoError(std::io::Error)` — file I/O

### Logging

The crate uses the `log` crate with macros:

```rust
log::error!("Node {} failed: {}", node_id, e);
log::warn!("Port type mismatch, coercing: {:?}", data_type);
log::info!("Graph loaded from {}", path.display());
log::debug!("Executing node {} with {} inputs", id, inputs.len());
```

The log backend is configured in `main.rs`. In GUI mode, log lines are captured and displayed in the overlay log panel. In headless mode, they go to stdout.

### `inline_port_key(node_id, port_name)`

Located in `src/ui/node_render.rs`. Generates the HashMap key used for inline port values:

```rust
pub fn inline_port_key(node_id: &str, port_name: &str) -> String {
    format!("{}::{}", node_id, port_name)
}
```

Use this whenever you need to index into `GraphTabState.inline_inputs`.

### `snap_to_grid(value)` / `snap_to_grid_center(value)`

Located in `src/ui/node_graph_view_geometry.rs`:

```rust
snap_to_grid(45.0)        // → 40.0  (nearest multiple of GRID_SIZE = 20)
snap_to_grid_center(45.0) // → 50.0  (nearest grid center)
```

Use when computing or snapping canvas positions.

### `node_dimensions(node)`

Located in `src/ui/node_graph_view_geometry.rs`. Returns `(width, height)` in canvas pixels for a node, respecting port count and special node types.

### `get_port_center(graph, node_id, port_name, is_input)`

Located in `src/ui/node_graph_view_geometry.rs`. Returns `Option<(f32, f32)>` — the canvas-space center point of a port dot. Used for edge routing.

### `refresh_port_types(graph)`

Located in `src/node/graph_io.rs`. Re-synchronizes port types in a `NodeGraphDefinition` against the live registry. Called when loading a graph to fix stale type strings from old files.

### `build_node_graph_from_definition(def)`

Located in `src/node/registry.rs`. Creates an executable `NodeGraph` from a `NodeGraphDefinition`. Instantiates all nodes, applies inline configs, resolves edges.

### `validate_graph_definition(def)`

Located in `src/node/graph_io.rs`. Returns a list of `ValidationIssue` structs without executing the graph. Used by the UI's validate button and before execution.

---

## Patterns used in the codebase

### Lazy static singleton

```rust
use once_cell::sync::Lazy;
use std::sync::RwLock;

pub static MY_REGISTRY: Lazy<MyStruct> = Lazy::new(MyStruct::new);

pub struct MyStruct {
    data: RwLock<HashMap<String, Value>>,
}
```

`RwLock` allows concurrent reads and exclusive writes. This pattern is used by `NODE_REGISTRY`.

### Rc<RefCell<...>> for shared UI state

Callback closures in the UI layer share mutable state via `Rc<RefCell<T>>`:

```rust
let tabs: Rc<RefCell<Vec<GraphTabState>>> = Rc::new(RefCell::new(vec![]));
let active_index: Rc<Cell<usize>> = Rc::new(Cell::new(0));

// Clone the Rc for each callback
let tabs_clone = tabs.clone();
ui.on_some_action(move || {
    let mut tabs = tabs_clone.borrow_mut();
    // mutate tabs...
});
```

Use `Rc<Cell<T>>` for `Copy` types (like `usize`), `Rc<RefCell<T>>` for non-`Copy` types.

### Arc<AtomicBool> for cross-thread signaling

```rust
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

// Create
let stop_flag = Arc::new(AtomicBool::new(false));

// Signal (from UI thread)
stop_flag.store(true, Ordering::Relaxed);

// Check (from worker thread / EventProducer)
if stop_flag.load(Ordering::Relaxed) { break; }
```

### NodeFactory type

```rust
pub type NodeFactory = Arc<dyn Fn(String, String) -> Box<dyn Node> + Send + Sync>;
//                                  ↑id       ↑name
```

All node constructors must accept `(id: String, name: String)`. This is enforced by the `register_node!` macro which calls `<T>::new(id, name)`.

---

## Testing conventions

### Unit tests

Place unit tests in the same file as the code under test:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_something() {
        // ...
    }
}
```

### Integration tests requiring live services

Tests that need Redis, MySQL, or a live LLM API are marked `#[ignore]`:

```rust
#[test]
#[ignore]  // requires REDIS_URL env var
fn test_redis_store() { ... }
```

Run them explicitly: `cargo test -- --ignored`

### Test naming

Tests follow the pattern `test_<what>_<condition>`:

```rust
test_format_string_basic()
test_format_string_missing_variable()
test_json_extract_nested_path()
```

---

## Common mistakes to avoid

1. **Don't hardcode node type names in compatibility validation** — use the registry instead.
2. **Don't rely on auto-fix to rebuild dynamic ports** — dynamic ports must be restored from `inline_values` and editor logic.
3. **Don't use `edges = []` in new graphs** — always provide explicit edges for clarity and type safety.
4. **Don't store authoritative state in Slint** — push state from Rust to Slint, never read it back.
5. **Don't skip `set_stop_flag` in EventProducers** — without it, the UI stop button won't work.
6. **Don't change a `type_id` in the registry without a migration** — it breaks existing graph JSON files.
