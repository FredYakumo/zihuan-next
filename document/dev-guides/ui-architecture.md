# UI Architecture

This document describes how the Slint frontend and Rust backend are structured, how they communicate, and how the node graph is rendered and interacted with.

---

## Layering principle

```
Slint (.slint files)      ← owns presentation, layout, bindings, animations
Rust (src/ui/*.rs)        ← owns state, logic, callbacks, persistence, graph execution
```

Slint never holds authoritative state. Every user action fires a Slint callback, which calls into Rust. Rust updates its state, then pushes a new view model back to Slint. Slint re-renders.

---

## File organization and naming conventions

All UI code lives under `src/ui/`.

### Slint files

- Root component and top-level layout: directly in `src/ui/` (e.g. `graph_window.slint`, `theme.slint`, `types.slint`, `dialogs.slint`).
- Extracted sub-components (reusable visual pieces): in `src/ui/components/`, one component per file, named after the component in `snake_case` (e.g. `graph_canvas.slint`, `node_item.slint`).

### Rust files

- One main view file per major view (e.g. `node_graph_view.rs`): owns tab lifecycle, graph load/save, UI wiring, and callback binding.
- Suffix `_vm` for view model conversion logic (e.g. `node_graph_view_vm.rs`).
- Suffix `_geometry` for coordinate math, node sizing, edge routing (e.g. `node_graph_view_geometry.rs`).
- Suffix `_inline` for inline value extraction and port value helpers (e.g. `node_graph_view_inline.rs`).

### Callbacks directory

- One subdirectory per major view: `node_graph_view_callbacks/` with a `mod.rs`.
- One file per interaction domain inside the directory. Name the file after the domain in `snake_case` (e.g. `canvas.rs`, `inline_ports.rs`, `tabs.rs`, `window.rs`).
- Node-specific dialog editors are named after the node type with an `_editor` suffix (e.g. `tool_editor.rs`, `json_extract_editor.rs`, `format_string_editor.rs`).

---

## View model structs (types.slint)

These are the stable public API between Rust and Slint. Names must not be changed without updating both sides simultaneously.

### NodeVm

Represents one node card on the canvas:

```slint
export struct NodeVm {
    id: string,
    label: string,
    preview_text: string,         // e.g. last output shown on node card
    node_type: string,
    string_data_text: string,     // for string_data node inline display
    message_event_filter_type: string,
    message_list: [MessageItemVm],
    x: float, y: float,           // canvas-space position (top-left corner)
    width: float, height: float,  // canvas-space size
    input_ports: [PortVm],
    output_ports: [PortVm],
    is_selected: bool,
    has_error: bool,
    is_event_producer: bool,
}
```

### PortVm

Represents one port on a node:

```slint
export struct PortVm {
    name: string,
    is_input: bool,
    is_connected: bool,           // true if an edge connects to this port
    is_required: bool,
    has_value: bool,              // true if port has inline value or is connected
    data_type: string,            // display string, e.g. "String", "Vec<OpenAIMessage>"
    inline_text: string,          // current inline value as text (for string/int/float)
    inline_bool: bool,            // current inline value for boolean ports
    bound_hyperparameter: string, // "" if not bound
}
```

### EdgeVm

Represents one edge connecting two ports:

```slint
export struct EdgeVm {
    from_node_id: string, from_port: string,
    to_node_id: string, to_port: string,
    from_x: float, from_y: float,   // canvas-space coordinates of source dot
    to_x: float, to_y: float,       // canvas-space coordinates of target dot
    is_selected: bool,
    color: color,
}
```

### Other VMs

| Struct | Purpose |
|--------|---------|
| `MessageItemVm` | One message in a message list preview (role + content) |
| `ToolDefinitionVm` | One tool entry in BrainNode's tool editor |
| `ToolParamVm` | One parameter inside a ToolDefinitionVm |
| `JsonExtractFieldVm` | One field in JsonExtractNode's field editor |
| `HyperParameterVm` | One hyperparameter binding entry |
| `NodeTypeVm` | Node type metadata for the add-node palette |
| `LogEntryVm` | One log line in the overlay log panel |
| `ValidationIssueVm` | One issue from graph validation (severity + message) |

---

## State management: GraphTabState

Each open graph tab has a `GraphTabState` (Rust struct, not visible to Slint):

```rust
pub(crate) struct GraphTabState {
    pub(crate) id: u64,
    pub(crate) title: String,
    pub(crate) file_path: Option<PathBuf>,
    pub(crate) graph: NodeGraphDefinition,   // authoritative graph data
    pub(crate) selection: SelectionState,    // selected nodes/edges
    pub(crate) inline_inputs: HashMap<String, InlinePortValue>,  // per-port inline state
    pub(crate) hyperparameter_values: HashMap<String, serde_json::Value>,
    pub(crate) is_dirty: bool,
    pub(crate) is_running: bool,
    pub(crate) stop_flag: Option<Arc<AtomicBool>>,
}
```

When any part of this state changes, the Rust side calls `apply_graph_to_ui()` / `refresh_active_tab_ui()` to rebuild the view models and push them to Slint.

---

## Data flow: graph → view model → Slint

```
GraphTabState.graph  (NodeGraphDefinition)
       ↓
apply_graph_to_ui()            in node_graph_view_vm.rs
       ↓
build_node_vm() × N            converts each NodeDefinition → NodeVm
build_input_port_vm() × N      fills PortVm fields (inline values, connectivity, etc.)
build_edges()                  converts EdgeDefinition[] → EdgeVm[] with coordinates
build_edge_segments()          decomposes edges into horizontal/vertical segments
build_grid_lines()             generates GridLineVm[] for the canvas background
       ↓
ui.set_nodes(...)              push ModelRc<VecModel<NodeVm>> into Slint
ui.set_edges(...)
ui.set_edge_segments(...)
etc.
```

Slint then re-renders everything. There is no partial update — the entire model is rebuilt and replaced each time.

---

## Callback flow: user action → Rust → re-render

All interaction starts with a Slint callback and ends with a full re-render:

```
User clicks / drags / types in Slint
       ↓
Slint fires a callback (e.g. on-node-drag-end)
       ↓
bind_*_callbacks() registered handler runs (in node_graph_view_callbacks/)
       ↓
Rust updates GraphTabState (mutates graph / selection / inline_inputs)
       ↓
refresh_active_tab_ui() called
       ↓
apply_graph_to_ui() rebuilds all VMs
       ↓
Slint re-renders
```

### Callback binding

All callbacks are bound during `show_graph()` in `node_graph_view.rs`. Each domain has its own binding function:

```rust
bind_canvas_callbacks(&ui, tabs.clone(), active_index.clone(), ...);
bind_inline_port_callbacks(&ui, tabs.clone(), active_index.clone(), ...);
bind_tool_editor_callbacks(&ui, tabs.clone(), active_index.clone(), ...);
bind_json_extract_editor_callbacks(&ui, ...);
bind_format_string_editor_callbacks(&ui, ...);
bind_tab_callbacks(&ui, ...);
bind_window_callbacks(&ui, ...);
bind_hyperparameter_callbacks(&ui, ...);
```

The `tabs` and `active_index` are `Rc<RefCell<...>>` shared references, giving each callback closure mutable access to the shared tab state.

---

## Coordinate systems

There are two coordinate spaces:

| Space | Description | Origin |
|-------|-------------|--------|
| **Canvas space** | The virtual 4000×4000 coordinate system where nodes live | Top-left of the canvas |
| **Screen space** | Pixels on screen, affected by pan and zoom | Top-left of the window |

Conversions:

```rust
// canvas → screen
screen_x = (canvas_x - pan_x) * zoom
screen_y = (canvas_y - pan_y) * zoom

// screen → canvas
canvas_x = screen_x / zoom + pan_x
canvas_y = screen_y / zoom + pan_y
```

Functions `snap_to_grid(v)` and `snap_to_grid_center(v)` quantize canvas coordinates to the 20px grid:

```rust
pub const GRID_SIZE: f32 = 20.0;

fn snap_to_grid(value: f32) -> f32 {
    (value / GRID_SIZE).round() * GRID_SIZE
}
```

### Node sizing

Node dimensions are computed from port count in `node_dimensions()`:

```rust
// Default sizing constants (in grid cells)
NODE_WIDTH_CELLS = 10      →  200px wide
NODE_HEADER_ROWS = 2       →  2 rows for title area
NODE_MIN_ROWS    = 3       →  minimum total rows
NODE_PADDING_BOTTOM = 0.8  →  extra bottom padding

// Height = GRID_SIZE × max(NODE_MIN_ROWS, NODE_HEADER_ROWS + max(input_ports, output_ports))
```

Special overrides:
- `message_list_data` / `qq_message_list_data` nodes have a larger minimum height (`LIST_NODE_MIN_HEIGHT`)
- `brain` nodes have a larger minimum height (`BRAIN_NODE_MIN_HEIGHT`)

If `NodeDefinition.size` is set, that overrides the auto-calculated value (with the auto-calculated value as a minimum floor).

### Port center coordinates

Each port dot's center position is used for edge routing:

```rust
// Input ports: aligned to left edge of node
center_x = node.x + GRID_SIZE * 0.5
center_y = node.y + GRID_SIZE * (NODE_HEADER_ROWS + port_index + 0.5)

// Output ports: aligned to right edge of node
center_x = node.x + node_width - GRID_SIZE * 0.5
center_y = node.y + GRID_SIZE * (NODE_HEADER_ROWS + port_index + 0.5)
```

---

## Special node editors

Some node types require custom dialog editors that modify `inline_values` and rebuild dynamic ports:

### FormatStringNode editor

- Opened when the user edits the `template` inline field
- Extracts `${variable}` names from the template string
- Calls `apply_inline_config()` on the node definition
- Rebuilds `input_ports` in `NodeDefinition` to match the new variables
- Calls `refresh_active_tab_ui()` to re-render

### JsonExtractNode editor (`json_extract_editor.rs`)

- Dialog with a table of field definitions (name, data_type)
- On save: serializes field definitions to JSON, stores in `inline_values["fields_config"]`
- Rebuilds `output_ports` in `NodeDefinition` from the new field definitions
- Marks `dynamic_output_ports = true` on the node definition

### FunctionNode editor (`function_editor.rs`)

- Dialog edits function name, description, input signature, and output signature
- On save: serializes `function_config`, updates visible ports, and syncs boundary nodes inside the embedded subgraph
- The node also exposes an "enter subgraph" action that pushes a child page onto the current tab's page stack

### BrainNode tool editor (`tool_editor.rs`)

- Dialog with a table of tool definitions (id, name, description, parameters, outputs)
- On save: serializes tool config to JSON and stores it in `inline_values["tools_config"]`
- `brain` output ports stay static; only `output` remains visible, with type `Vec<OpenAIMessage>`
- Each tool row can open its own embedded subgraph editor page

The key invariant: whatever the editor saves into `inline_values` must produce exactly the same visible port list as the node's `apply_inline_config()` + `input_ports()` / `output_ports()` methods would compute at runtime.

---

## Subgraph page navigation

`GraphTabState` now manages a root page plus a stack of child subgraph pages.

Each page stores:

- the page-local `NodeGraphDefinition`
- selection state
- inline-input cache
- canvas pan/zoom state

This allows:

- entering a function node's private subgraph
- entering a Brain tool's private subgraph
- returning one level up or jumping back to `主图`
- preserving per-page pan/zoom and selection

Before save, save-as, open, tab switch, or leaving a subgraph page, Rust commits the current page stack back into the root graph's embedded configs.

See [../node/function-subgraphs.md](../node/function-subgraphs.md) for the full subgraph model.

---

## Rules and constraints

- `GraphCanvas` must keep `clip: true` — all nodes outside the viewport must be clipped.
- `graph_window.slint` is the stable root component. Do not turn it into a catch-all.
- Extracted components live in `src/ui/components/`.
- Dialogs and the node-type selector must render above the canvas layer (z-order).
- The Rust side is the single source of truth. Slint holds no persistent state.
- Hyperparameter values are stored in a separate YAML file, not in the graph JSON.

---

## WebUI — LiteGraph inline widget rendering (webui/)

> This section applies to the browser-based canvas in `webui/src/graph/`. It is separate from the Slint system described above.

### Background: LiteGraph draw order

LiteGraph draws a node in this fixed order:

1. Node body (background shape)
2. `onDrawForeground` (binding badges for non-widget slots)
3. **Input slot dots and labels** — labels at `x ≈ slotHeight + 2` (left side)
4. **Output slot dots and labels** — labels right-aligned near the right dot
5. **`drawNodeWidgets()`** — widget backgrounds and text, drawn **on top of everything above**

Step 5 always overwrites whatever was drawn in steps 3–4. For nodes that have inline widgets sharing a row with visible ports, this creates conflicts that must be resolved with custom overdraw.

### Inline widget layout model

An **inline widget** is a widget pinned to a specific input-port row via `widget.y`, so it renders on the same horizontal row as the port dot (not stacked below all ports). This is set up by `setupSimpleInlineWidgets` in `webui/src/graph/widgets.ts`.

Key properties set per widget during setup:

| Property | Purpose |
|----------|---------|
| `input.label = ""` | Suppress LiteGraph's native slot label (the widget background covers it anyway; we repaint it later) |
| `input.widget = { name: key }` | Links the slot to the widget for click detection and right-click binding |
| `widget.y = getInlineWidgetTopY(node, inputIdx)` | Pins the widget to its slot row — avoids LiteGraph's default `+4 px` per-widget drift |
| `widget._inlineInputIndex = inputIdx` | Cache for fast slot-index lookup during drawing |
| `node._hasInlineWidgets = true` | Flag that enables the custom inline rendering path |
| `node.widgets_start_y` | Set so LiteGraph's `computeSize()` calculates the correct node height |

The canonical Y formula (in `webui/src/graph/inline_layout.ts`):

```
rowCenterY = slot_start_y + (slotIndex + 0.7) × NODE_SLOT_HEIGHT
widgetTopY = rowCenterY − NODE_WIDGET_HEIGHT / 2
```

This matches LiteGraph's own `getConnectionPos` formula, so widget tops, slot dots, and drawn text all share the same vertical baseline.

### Custom draw override — drawNodeWidgets

After calling `origDrawNodeWidgets` (which renders full-width widget backgrounds and covers port labels), the override does the following for inline nodes:

1. **Erase** the full-width LiteGraph widget background with `nodeBg` so it no longer covers the port area.
2. **Redraw the value** right-aligned as plain text. When an output label occupies the same row, the value is pushed left to avoid overlap; the output label's erase+redraw in step 5 then cleans up any residual overlap.
3. **`drawWidgetBindingBadges()`** — draw hyperparameter / variable binding pill badges on top.
4. **`drawInlineInputLabels()`** — repaint input slot names (cleared in step 1 above) left-aligned at `x = SLOT_H + 2`.
5. **`drawInlineOutputLabels()`** — repaint output labels; erases residual content in the label region first, then draws the label on top.

**The draw order (erase → value → badges → input labels → output labels) must not be changed.** Each step depends on the previous one having run.

### Invariants — do not break

| Invariant | Where enforced | Why |
|-----------|---------------|-----|
| `input.label = ""` on every inline-widget-linked input | `widgets.ts` `setupSimpleInlineWidgets` | LiteGraph draws slot labels before widgets; clearing the label prevents a ghost label from bleeding through the erase |
| `widget.y` pinned to `getInlineWidgetTopY(node, idx)` | `widgets.ts` | Without this LiteGraph auto-increments `posY += H + 4` per widget, causing lower rows to drift `4 px × row-index` off their slot |
| `widget._inlineInputIndex` cache | `widgets.ts` | Used by `getInlineWidgetInputIndex()` in `canvas.ts` to resolve which slot owns a widget during drawing without a linear scan each frame |
| Erase before redrawing labels | `canvas.ts` draw override | If the erase is removed, the LiteGraph widget background covers input labels; if the repaint is removed, labels disappear entirely |
| Output labels redrawn last | `canvas.ts` draw override | Output labels sit over the widget area; they must be the final layer or they get erased by earlier steps |
| Value right-edge calculation respects the output label | `canvas.ts` draw override | When input[i] and output[i] share a row, the value text must stop left of the output label's start |
| `getInlineRowCenterY` used for **all** inline Y positions | `inline_layout.ts` | All rendering systems (widget draw, value text, input labels, output labels, badges) must use the same formula or they drift apart |

### Output label conflict on asymmetric nodes

LiteGraph positions output[i] at the same Y as input[i]. On nodes where inputs > outputs (e.g. MySQL node: 9 inputs, 1 output), output[0]'s label (`mysql_ref`) appears on the same visual row as input[0] (`mysql_host`). This is **expected** — the output dot is physically at that Y coordinate. The value text and output label coexist by the value being pushed left and the output label being erased+redrawn on the right.

Pass-through ports (same name on both an input and its corresponding output, e.g. a `String` passthrough node) are handled specially: `drawInlineOutputLabels` skips the output label for that row, and the input's value text occupies the full width instead.

### File map

| File | Responsibility |
|------|---------------|
| `webui/src/graph/link_layout.ts` | Connection geometry helpers for committed links: fan-out separation, curve control points, and label anchors |
| `webui/src/graph/inline_layout.ts` | Canonical Y-geometry helpers (`getInlineRowCenterY`, `getInlineWidgetTopY`, etc.) |
| `webui/src/graph/widgets.ts` — `setupSimpleInlineWidgets` | Creates widgets, sets `widget.y`, `_inlineInputIndex`, `input.label=""`, `_hasInlineWidgets` |
| `webui/src/graph/canvas.ts` — `drawNodeWidgets` override | Erase → value redraw → badges → input labels → output labels |
| `webui/src/graph/canvas.ts` — `drawInlineInputLabels` | Repaints suppressed input slot names after the erase step |
| `webui/src/graph/canvas.ts` — `drawInlineOutputLabels` | Repaints output slot labels (with local erase) at the end of the overdraw pass |
| `webui/src/graph/canvas.ts` — `drawWidgetBindingBadges` | Draws colored pill badges for hyperparameter/variable-bound ports |

### Custom link rendering invariants

Committed graph links in the browser canvas no longer rely entirely on LiteGraph's stock orthogonal path. `canvas.ts` uses `link_layout.ts` to render a **ComfyUI-style smooth Bezier curve** with these invariants:

- The curve must separate visibly from the source port early, using a strong horizontal pull on the source-side control point. This avoids the "vertical trunk" ambiguity where a downward wire looks detached from the node that emitted it.
- Multiple links that fan out from the same output port must diverge near the source side, not only near the targets. The fan-out offset is computed per `(origin_id, origin_slot)` group.
- Label anchors belong to individual rendered links, not to a deduplicated output-port midpoint. When an output fans out to several targets, each visible wire keeps its own anchor so users can tell which target a label belongs to.
- Temporary drag links use LiteGraph's spline renderer so preview wires stay visually close to the committed curved result.

If you touch browser link rendering in the future, preserve those invariants unless you deliberately redesign the canvas interaction model.
