# Copilot Instructions: src/ui/

## Purpose
Slint-based visual editor for node graph creation, interaction, execution triggers, and persistence. The UI layer is intentionally split into small Slint composition modules while Rust owns graph state, VM assembly, callback wiring, and persistence.

## Architecture Overview

### Slint / Rust split
- **Slint files** handle presentation, layout, and reactive bindings.
- **Rust files** handle selection state, graph mutations, VM projection, file I/O, run/stop orchestration, and window persistence.

### Recommended UI structure
- `src/ui/graph_window.slint` — root `NodeGraphWindow`, top-level property/callback contract, layout composition
- `src/ui/types.slint` — shared exported VM structs used by Rust and Slint
- `src/ui/components/buttons.slint` — shared text/button components
- `src/ui/components/menubar.slint` — menu bar row components
- `src/ui/components/tabs.slint` — tab UI components
- `src/ui/components/node_item.slint` — node card rendering and per-node inline editing
- `src/ui/components/graph_canvas.slint` — grid, pan/zoom, edges, drag line, box selection, node hosting
- `src/ui/dialogs.slint` — overlays such as errors, save confirmation, running confirmation, node selector
- `src/ui/node_graph_view.rs` — main Rust-side UI orchestration and VM refresh
- If a callback family grows beyond a couple of handlers, place it under a dedicated submodule folder such as `src/ui/node_graph_view_callbacks/` instead of keeping it inline in `node_graph_view.rs`
- Node-specific callback families should each get their own file, for example `message_list.rs` for `on_message_list_*` and `qq_message_list.rs` for `on_qq_message_list_*`
- `src/ui/node_graph_view_geometry.rs` — grid/edge geometry, snap helpers, node sizing, port type label resolution
- `src/ui/node_graph_view_inline.rs` — inline port state, message-list editing helpers, node creation helper
- `src/ui/node_graph_view_vm.rs` — `NodeGraphDefinition` → Slint VM projection and node-type filtering
- `src/ui/selection.rs` — selection synchronization and selected-item projection
- `src/ui/window_state.rs` — persistent window size/position
- `src/ui/node_render/` — Rust helpers for node previews and inline value rendering

## Design Rules
- Keep `NodeGraphWindow` as the stable public entry point consumed by Rust.
- Keep exported VM struct names stable unless Rust imports are updated in the same change.
- Prefer extracting reusable Slint components before adding more conditional UI to `graph_window.slint`.
- `graph_window.slint` should remain a composition/root-contract file, not a catch-all implementation file.
- Avoid circular Slint imports; shared types belong in `types.slint`, shared styling stays in `theme.slint`.
- If a node-specific UI becomes large, prefer another component file over extending `NodeItem` indefinitely.

## Callback and state contract
- Callback names and parameter shapes in `NodeGraphWindow` are effectively part of the Rust integration contract.
- First-choice refactors should preserve existing callback/property names to avoid churn in `node_graph_view.rs` and `selection.rs`.
- `GraphCanvas` and `NodeItem` may proxy callbacks internally, but the root window should remain the canonical contract boundary.

## Critical rendering constraints
- `GraphCanvas` must keep `clip: true`; otherwise grid lines, edges, and nodes can overflow over the menu bar or tab strip.
- Grid `Path` rendering must stay clipped and continue using the current pan/zoom viewbox math.
- Preserve the existing coordinate conversions between canvas space and zoomed screen space when moving or resizing nodes.
- Overlay dialogs/selectors must remain above the canvas and menu layers.

## Refactor guidance
- **Safe first step**: split Slint files while keeping Rust imports and UI behavior stable.
- **Follow-up step**: split `node_graph_view.rs` by responsibility (window lifecycle, VM building, geometry, filtering, run control).
- Current split keeps callback orchestration in `node_graph_view.rs` while moving geometry, inline-input transforms, and UI projection helpers to dedicated sibling modules.
- Do not mix a large Rust-side architectural rewrite into the first Slint modularization change unless required by compiler constraints.

## Verification checklist
- `cargo check`
- Launch GUI and verify menu, tabs, pan/zoom, node drag, node resize, box selection, edge selection, dialogs
- Re-test special node UIs:
  - `string_data`
  - `message_list_data`
  - `qq_message_list_data`
  - `message_event_type_filter`
  - `preview_message_list`

## Common pitfalls
- Forgetting to import a shared exported struct into `graph_window.slint`
- Accidentally changing callback signatures while moving components
- Breaking zoom/pan math by mixing screen-space and canvas-space coordinates
- Reintroducing a giant Slint file under a new filename — same spaghetti, different plate
