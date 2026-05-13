# Node System

This document describes the current node runtime in `zihuan-next`.

## Runtime Model

The graph runtime is a synchronous DAG executor.

`NodeGraph` owns:

- instantiated nodes
- parsed inline values
- runtime variable store
- stop flag
- optional execution callback
- graph edges and loaded definition metadata

The executor sorts nodes topologically and runs them synchronously.

## The `Node` Trait

Defined in `zihuan_graph_engine/src/lib.rs`.

Important methods:

- `input_ports()`
- `output_ports()`
- `execute(...)`
- `on_graph_start()`
- `apply_inline_config(...)`
- `set_function_runtime_values(...)`
- `set_runtime_variable_store(...)`

`NodeType` still exists, but the only active variant is:

```rust
pub enum NodeType {
    Simple,
}
```

## Build-Time Graph Preparation

When a graph definition is turned into a runtime graph:

1. node instances are created from the registry
2. inline values are parsed into typed `DataValue`s
3. `apply_inline_config(...)` is called so nodes can restore config-driven state
4. dynamic-port nodes may expose additional ports after config restoration
5. runtime variable store is attached to nodes

This happens in `zihuan_graph_engine::registry::build_node_graph_from_definition(...)`.

## Execution Flow

At execution time:

1. `prepare_for_execution()` resets the stop flag and runtime variables
2. each node receives `on_graph_start()`
3. the graph is topologically ordered
4. inputs are collected from:
   - edges
   - bound runtime variables
   - inline values
5. inputs are validated
6. `execute(...)` is called
7. outputs are validated and stored
8. optional execution callbacks are emitted

## Dynamic Ports

Nodes with config-driven ports should rebuild them in `apply_inline_config(...)`.

Examples include:

- function nodes
- format string nodes
- JSON extract nodes
- some connection/config nodes

The UI and runtime should derive the same visible ports from the same stored config.

## Runtime Variable Store

`NodeGraph` maintains a run-scoped shared variable store.

It is used for graph variables and for nodes such as:

- `set_variable`
- session state helpers
- function boundary helpers

Variable initial values come from the loaded graph definition and are reset at the start of each run.

## Stop Flag

`NodeGraph` still exposes a graph-level stop flag so task orchestration can request cancellation. It is not a separate node lifecycle model.

## What Does Not Belong Here

The graph runtime does not host:

- bot listener loops
- HTTP service entrypoints
- auto-start lifecycle management
- concurrent agent hosting

Those concerns belong to `zihuan_service` and the main server API/runtime layer.

## Registry Entry Points

Current registry bootstrap path:

- `zihuan_graph_engine::registry::init_node_registry()` — built-in utility nodes
- extended by `storage_handler::init_node_registry()`
- extended by `ims_bot_adapter::init_node_registry()`
- extended by `zihuan_llm::init_node_registry()`
- extended by `zihuan_service::init_node_registry()`
- combined via `init_node_registry_with_extensions()` in `src/init_registry.rs`

## Design Rule

If a feature needs to stay alive independently of a single graph run, it should move into the service runtime. If it can complete within one graph invocation, keep it as a normal synchronous node or subgraph.
