---
name: zihuan-node-dev
description: Develop DAG nodes in ZiHuan Next. Use when creating, modifying, registering, or debugging graph nodes, ports, node macros, and node lifecycle behavior.
---

# DAG Nodes

## Architecture Decision

Dynamic script nodes are the primary node implementation mechanism. Prefer adding
ordinary business and composition nodes under `dag_nodes/` and registering them
through the dynamic script catalog.

When a capability is performance-sensitive, CPU-intensive, stateful at the host
boundary, or otherwise unsuitable for the script runtime, implement it as a Rust
SDK capability and expose it to dynamic scripts. Keep the Rust surface small and
stable: model reusable operators, primitives, and atomic operations as `util`
building blocks so multiple script nodes can share them. Do not move a node to
Rust merely because it is convenient to implement there.

1. Find a comparable node and keep one node implementation per file.
2. Define ports with `node_input!`, `node_output!`, and `port!`; use meaningful names and UI descriptions.
3. Return production outputs with `return_with_node_output!` so declared ports are validated.
4. Register the node in its owning crate's registry or initialization module.
5. Put cross-crate port types in `zihuan_core`; rebuild dynamic ports through the node lifecycle when configuration changes them.
6. Add unit tests for transformations and run the narrow crate test from `zihuan-test`.

Use `node_input_flow!` and `node_output_flow!` mainly for tests or direct node invocation.
