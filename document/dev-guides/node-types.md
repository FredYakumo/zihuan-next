# Node Types

This document covers the two node execution models — **Simple** and **EventProducer** — their lifecycles, and when to use each. It uses `BotAdapterNode` as a concrete example of an EventProducer.

> **See also:** [node-system.md](node-system.md) for the full execution engine, and [node-development.md](../node/node-development.md) for a step-by-step creation guide.

---

## Overview

Every node declares its execution model by overriding `node_type()`:

```rust
fn node_type(&self) -> NodeType { NodeType::Simple }        // default
fn node_type(&self) -> NodeType { NodeType::EventProducer } // opt-in
```

| Type | When to use | Entry point |
|------|-------------|-------------|
| `Simple` | Stateless transform, runs once per activation | `execute(inputs) → outputs` |
| `EventProducer` | Long-running event source (WebSocket, timer, poll loop) | `on_start → loop { on_update } → on_cleanup` |

---

## Simple Nodes

A Simple node is a pure function over its inputs. The engine calls `execute()` once per activation and writes the returned map into the data pool for downstream nodes.

### Lifecycle

```
[graph starts]
  │
  ├─ prepare_for_execution()
  │     on_graph_start()         ← one-time setup (optional)
  │     apply_inline_config()    ← read inline/static port values
  │
  └─ for each node in topological order:
        validate_inputs(inputs)
        outputs = execute(inputs)   ← your logic lives here
        validate_outputs(outputs)
        → outputs written to data pool for downstream nodes
```

### Required trait methods

| Method | Must implement | Notes |
|--------|---------------|-------|
| `execute()` | Yes | Core logic; return `Ok(outputs)` or `Err(...)` |
| `input_ports()` / `output_ports()` | Yes | Via `node_input!` / `node_output!` macros |
| `node_type()` | No | Default is `NodeType::Simple` |
| `apply_inline_config()` | Only if node reads inline values | Called before first `execute()` |

### Example

```rust
impl Node for UppercaseNode {
    fn node_type(&self) -> NodeType { NodeType::Simple } // implicit default

    node_input![
        port! { name = "text", ty = String, desc = "Input text" },
    ];
    node_output![
        port! { name = "result", ty = String, desc = "Uppercased text" },
    ];

    fn execute(&mut self, inputs: HashMap<String, DataValue>)
        -> Result<HashMap<String, DataValue>>
    {
        let text = match inputs.get("text") {
            Some(DataValue::String(s)) => s.clone(),
            _ => return Err(Error::ValidationError("missing 'text'".into())),
        };
        let mut out = HashMap::new();
        out.insert("result".into(), DataValue::String(text.to_uppercase()));
        Ok(out)
    }
}
```

---

## EventProducer Nodes

An EventProducer owns a long-running loop and emits a new output map on each iteration. The engine runs it in a dedicated thread and re-executes all downstream Simple nodes on each emission.

### Lifecycle

```
[graph starts]
  │
  ├─ prepare_for_execution()
  │     on_graph_start()
  │     apply_inline_config()
  │     set_stop_flag(Arc<AtomicBool>)  ← store this; check it in on_update
  │
  ├─ Execute "base layer" (Simple nodes upstream of, or unreachable from, this producer)
  │
  └─ on_start(base_layer_outputs)       ← one-time init; connect, allocate channels
        │
        loop:
        │   if stop_flag is set → break
        │   outputs = on_update()
        │   if outputs is None  → break  ← natural end
        │   merge outputs into data_pool
        │   execute all downstream Simple nodes
        │
        on_cleanup()                     ← always called; drop channels, close sockets
```

### Required trait methods

| Method | Must implement | Notes |
|--------|---------------|-------|
| `node_type()` | Yes | Return `NodeType::EventProducer` |
| `set_stop_flag()` | Yes | Store the flag; the engine calls this before `on_start` |
| `on_start()` | Yes | Open connections, spawn background tasks, init channels |
| `on_update()` | Yes | Return `Ok(Some(map))` to emit; `Ok(None)` to exit cleanly |
| `on_cleanup()` | Yes | Drop channels, close connections, release runtime |
| `execute()` | Yes (stub) | Never called by the engine but must compile; return `Ok(HashMap::new())` |
| `input_ports()` / `output_ports()` | Yes | Via macros as usual |

### Stop flag contract

The stop flag (`Arc<AtomicBool>`) is the engine's only mechanism to interrupt a running producer. **You must check it** or the graph will hang when the user presses Stop.

```rust
fn set_stop_flag(&mut self, flag: Arc<AtomicBool>) {
    self.stop_flag = Some(flag);
}

fn on_update(&mut self) -> Result<Option<HashMap<String, DataValue>>> {
    if self.stop_flag.as_ref().map_or(false, |f| f.load(Ordering::Relaxed)) {
        return Ok(None); // exits the loop
    }
    // ... normal logic
}
```

For async producers using `tokio::select!`, the stop flag check should be one of the select branches (see [BotAdapterNode example](#botadapternode-example) below).

---

## BotAdapterNode — A Concrete EventProducer

`BotAdapterNode` (registered as `"bot_adapter"`, source: `src/bot_adapter/bot_adapter.rs`) connects to a QQ bot server over WebSocket, receives incoming `MessageEvent`s, and emits one output map per event.

### Ports

**Input:**

| Port | Type | Notes |
|------|------|-------|
| `qq_id` | `String` | QQ account ID to log in as |
| `bot_server_url` | `String` | WebSocket URL of the bot server |
| `bot_server_token` | `Password` | Optional auth token |

**Output (emitted on every event):**

| Port | Type | Notes |
|------|------|-------|
| `message_event` | `MessageEvent` | The raw incoming message event |
| `bot_adapter` | `BotAdapterRef` | Shared handle to the live connection (for send nodes) |

### Lifecycle walkthrough

#### `on_start` — connect and spawn

```
on_start(inputs):
  1. Extract qq_id, bot_server_url, bot_server_token from inputs
     (fall back to env vars QQ_ID / BOT_SERVER_URL / BOT_SERVER_TOKEN)
  2. Create mpsc channels: event_tx/event_rx and error_tx/error_rx
  3. Build an EventHandler closure that sends each MessageEvent to event_tx
  4. Spawn async task: BotAdapter::new(config) → register handler → BotAdapter::start()
  5. Block until adapter_handle is returned via oneshot channel
  6. Store event_rx, error_rx, adapter_handle in self
```

The Tokio runtime is managed here: if a current Tokio handle is available, the task is spawned on it; otherwise a new runtime is created and stored in `self.runtime`.

#### `on_update` — wait for next event

```
on_update():
  Block on tokio::select!:
    ┌─ event_rx.recv()   → Ok(Some(outputs))    emit { message_event, bot_adapter }
    ├─ error_rx.recv()   → Err(...)             propagate connection error
    └─ stop_flag check   → Ok(None)             exit loop cleanly
```

Because `on_update` blocks the engine thread until an event arrives, there is no busy-wait — the loop idles at near-zero CPU.

#### `on_cleanup` — release resources

```
on_cleanup():
  self.event_rx     = None   // drops channel
  self.error_rx     = None
  self.adapter_handle = None // drops WebSocket connection
  self.runtime      = None   // shuts down Tokio runtime if we owned it
  self.stop_flag    = None
```

### Execution flow in a bot pipeline

```
[graph start]
  │
  ├─ Base layer: string_data, llm_api, etc.
  │
  └─ BotAdapterNode.on_start(base_layer_outputs)
        │
        loop:
        │   [QQ server → WebSocket → event_tx → event_rx]
        │   on_update() → { message_event, bot_adapter }
        │   ↓
        │   message_event_type_filter
        │   extract_message_from_event
        │   openai_message_session_cache_get
        │   brain (LLM inference)
        │   send_group_message / send_friend_message
        │   [repeat on next event]
        │
        on_cleanup()
```

---

## Choosing the Right Type

Use **Simple** when:
- The node transforms or routes data synchronously
- It has no background I/O and holds no persistent connections
- Examples: `format_string`, `conditional`, `json_extract`, `brain`

Use **EventProducer** when:
- The node is the *source* of events in the pipeline
- It must maintain a connection or run a background loop indefinitely
- It naturally terminates when the connection closes or a condition is met
- Examples: `bot_adapter` (WebSocket), a timer node, a file-watcher node

> **Rule of thumb:** if your node calls `recv()`, `accept()`, or `sleep()` in a loop — it is an EventProducer.
