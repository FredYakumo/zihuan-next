# Node Types

This document explains the **current** node execution model in `zihuan-next`.

Short version: graph execution is now **Simple-only**.

---

## Current State

The runtime still exposes `NodeType`, but only one variant remains:

```rust
pub enum NodeType {
    Simple,
}
```

Every node executes as a synchronous transformation:

```rust
inputs -> execute() -> outputs
```

---

## What “Simple” Means Now

A Simple node:

- runs once when the graph executor reaches it in topological order
- receives all resolved input values as a `HashMap<String, DataValue>`
- returns one output map synchronously
- may keep per-run state via `on_graph_start()`
- may rebuild config-driven ports in `apply_inline_config()`

Typical examples:

- string/JSON transforms
- branch/router nodes
- LLM request nodes
- Brain/tool nodes
- database/object-storage reference nodes
- persistence nodes

---

## Removed Execution Model

The graph runtime no longer supports `EventProducer`.

Removed concepts:

- `on_start`
- `on_update`
- `on_cleanup`
- `set_stop_flag` on nodes
- graph-owned message/event subscription loops
- graph-owned timers/pollers as a first-class execution model

Historical docs or compatibility helpers may still mention them. New code should not rely on them.

---

## Where Long-Lived Behavior Moved

If you need any of the following:

- long-running bot message consumption
- background network listeners
- concurrent agent hosting
- OpenAI-compatible HTTP serving
- auto-start lifecycle

that now belongs to the **service runtime**, not the node graph.

Relevant code:

- `src/service/agent_manager.rs`
- `src/system_config/`
- `src/api/system_config.rs`

---

## Practical Rule For New Development

When adding behavior, choose one of these paths:

### Path A: synchronous node

Use a normal node when the behavior can complete during one graph execution call.

Examples:

- parse/transform data
- call an HTTP API once
- run a tool subgraph
- persist one batch of messages/images

### Path B: service runtime component

Use a service/runtime component when the behavior must stay alive independently of a graph run.

Examples:

- QQ chat agent event loop
- HTTP stream agent
- startup lifecycle manager

### Path C: synchronous tool node/subgraph used by a service

When an agent needs reusable workflow behavior, keep that behavior in a synchronous node graph or tool subgraph, and let the service call it.

This is the preferred way to keep graph topology simple while still enabling rich agent workflows.

---

## Checklist

For any new node:

- it should behave correctly as a synchronous `execute()`-based node
- it should not require its own background lifecycle
- it should not depend on graph-level async scheduling

If any of those are false, the behavior probably belongs outside the node graph.
