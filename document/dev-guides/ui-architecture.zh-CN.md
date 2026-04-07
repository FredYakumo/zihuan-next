# UI 架构

> 🌐 [English](ui-architecture.md) | 简体中文

本文档描述 Slint 前端与 Rust 后端的结构、通信方式，以及节点图的渲染和交互方式。

---

## 分层原则

```
Slint（.slint 文件）      ← 负责展示、布局、绑定、动画
Rust（src/ui/*.rs）       ← 负责状态、逻辑、回调、持久化、图执行
```

Slint 从不持有权威状态。每次用户操作都会触发 Slint 回调，再调入 Rust。Rust 更新其状态后，将新的视图模型推送回 Slint，由 Slint 重新渲染。

---

## 文件组织与命名约定

所有 UI 代码位于 `src/ui/` 下。

### Slint 文件

- 根组件和顶层布局：直接位于 `src/ui/`（例如 `graph_window.slint`、`theme.slint`、`types.slint`、`dialogs.slint`）。
- 提取出的子组件（可复用的视觉片段）：位于 `src/ui/components/`，每个组件一个文件，以 `snake_case` 命名（例如 `graph_canvas.slint`、`node_item.slint`）。

### Rust 文件

- 每个主要视图对应一个主视图文件（例如 `node_graph_view.rs`）：负责标签页生命周期、图加载/保存、UI 连线和回调绑定。
- 视图模型转换逻辑使用 `_vm` 后缀（例如 `node_graph_view_vm.rs`）。
- 坐标计算、节点尺寸、边路由使用 `_geometry` 后缀（例如 `node_graph_view_geometry.rs`）。
- 内联值提取和端口值辅助函数使用 `_inline` 后缀（例如 `node_graph_view_inline.rs`）。

### 回调目录

- 每个主要视图对应一个子目录：`node_graph_view_callbacks/`，包含 `mod.rs`。
- 目录内每个交互域对应一个文件，以 `snake_case` 命名（例如 `canvas.rs`、`inline_ports.rs`、`tabs.rs`、`window.rs`）。
- 节点特定的对话框编辑器以节点类型加 `_editor` 后缀命名（例如 `tool_editor.rs`、`json_extract_editor.rs`、`format_string_editor.rs`）。

---

## 视图模型结构体（types.slint）

这些是 Rust 与 Slint 之间稳定的公共 API。名称不能在未同步更新两侧的情况下更改。

### NodeVm

表示画布上的一个节点卡片：

```slint
export struct NodeVm {
    id: string,
    label: string,
    preview_text: string,         // 例如：节点卡片上显示的最后一次输出
    node_type: string,
    string_data_text: string,     // 用于 string_data 节点的内联显示
    message_event_filter_type: string,
    message_list: [MessageItemVm],
    x: float, y: float,           // 画布空间位置（左上角）
    width: float, height: float,  // 画布空间尺寸
    input_ports: [PortVm],
    output_ports: [PortVm],
    is_selected: bool,
    has_error: bool,
    is_event_producer: bool,
}
```

### PortVm

表示节点上的一个端口：

```slint
export struct PortVm {
    name: string,
    is_input: bool,
    is_connected: bool,           // 如果有边连接到此端口则为 true
    is_required: bool,
    has_value: bool,              // 如果端口有内联值或已连接则为 true
    data_type: string,            // 显示字符串，例如 "String"、"Vec<OpenAIMessage>"
    inline_text: string,          // 当前内联值的文本形式（用于 string/int/float）
    inline_bool: bool,            // 布尔端口的当前内联值
    bound_hyperparameter: string, // 未绑定时为 ""
}
```

### EdgeVm

表示连接两个端口的一条边：

```slint
export struct EdgeVm {
    from_node_id: string, from_port: string,
    to_node_id: string, to_port: string,
    from_x: float, from_y: float,   // 源点的画布空间坐标
    to_x: float, to_y: float,       // 目标点的画布空间坐标
    is_selected: bool,
    color: color,
}
```

### 其他 VM

| 结构体 | 用途 |
|--------|---------|
| `MessageItemVm` | 消息列表预览中的一条消息（角色 + 内容） |
| `ToolDefinitionVm` | BrainNode 工具编辑器中的一个工具条目 |
| `ToolParamVm` | ToolDefinitionVm 中的一个参数 |
| `JsonExtractFieldVm` | JsonExtractNode 字段编辑器中的一个字段 |
| `HyperParameterVm` | 一个超参数绑定条目 |
| `NodeTypeVm` | 添加节点面板的节点类型元数据 |
| `LogEntryVm` | 覆盖层日志面板中的一行日志 |
| `ValidationIssueVm` | 图验证的一个问题（严重程度 + 消息） |

---

## 状态管理：GraphTabState

每个打开的图标签页都有一个 `GraphTabState`（Rust 结构体，对 Slint 不可见）：

```rust
pub(crate) struct GraphTabState {
    pub(crate) id: u64,
    pub(crate) title: String,
    pub(crate) file_path: Option<PathBuf>,
    pub(crate) graph: NodeGraphDefinition,   // 权威图数据
    pub(crate) selection: SelectionState,    // 选中的节点/边
    pub(crate) inline_inputs: HashMap<String, InlinePortValue>,  // 每端口的内联状态
    pub(crate) hyperparameter_values: HashMap<String, serde_json::Value>,
    pub(crate) is_dirty: bool,
    pub(crate) is_running: bool,
    pub(crate) stop_flag: Option<Arc<AtomicBool>>,
}
```

当此状态的任何部分发生变化时，Rust 端调用 `apply_graph_to_ui()` / `refresh_active_tab_ui()` 重新构建视图模型并推送到 Slint。

---

## 数据流：图 → 视图模型 → Slint

```
GraphTabState.graph  (NodeGraphDefinition)
       ↓
apply_graph_to_ui()            在 node_graph_view_vm.rs 中
       ↓
build_node_vm() × N            将每个 NodeDefinition 转换为 NodeVm
build_input_port_vm() × N      填充 PortVm 字段（内联值、连通性等）
build_edges()                  将 EdgeDefinition[] 转换为带坐标的 EdgeVm[]
build_edge_segments()          将边分解为水平/垂直线段
build_grid_lines()             生成画布背景的 GridLineVm[]
       ↓
ui.set_nodes(...)              将 ModelRc<VecModel<NodeVm>> 推送到 Slint
ui.set_edges(...)
ui.set_edge_segments(...)
等等
```

Slint 随后重新渲染所有内容。没有局部更新——每次都会重新构建并替换整个模型。

---

## 回调流：用户操作 → Rust → 重新渲染

所有交互从 Slint 回调开始，以完整重新渲染结束：

```
用户在 Slint 中点击 / 拖拽 / 输入
       ↓
Slint 触发回调（例如 on-node-drag-end）
       ↓
bind_*_callbacks() 注册的处理器运行（在 node_graph_view_callbacks/ 中）
       ↓
Rust 更新 GraphTabState（修改图 / 选择 / inline_inputs）
       ↓
调用 refresh_active_tab_ui()
       ↓
apply_graph_to_ui() 重新构建所有 VM
       ↓
Slint 重新渲染
```

### 回调绑定

所有回调在 `node_graph_view.rs` 的 `show_graph()` 期间绑定。每个域有其独立的绑定函数：

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

`tabs` 和 `active_index` 是 `Rc<RefCell<...>>` 共享引用，为每个回调闭包提供对共享标签页状态的可变访问。

---

## 坐标系

存在两个坐标空间：

| 空间 | 描述 | 原点 |
|-------|-------------|--------|
| **画布空间** | 节点所在的 4000×4000 虚拟坐标系 | 画布左上角 |
| **屏幕空间** | 屏幕上的像素，受平移和缩放影响 | 窗口左上角 |

转换公式：

```rust
// 画布 → 屏幕
screen_x = (canvas_x - pan_x) * zoom
screen_y = (canvas_y - pan_y) * zoom

// 屏幕 → 画布
canvas_x = screen_x / zoom + pan_x
canvas_y = screen_y / zoom + pan_y
```

`snap_to_grid(v)` 和 `snap_to_grid_center(v)` 函数将画布坐标量化到 20px 网格：

```rust
pub const GRID_SIZE: f32 = 20.0;

fn snap_to_grid(value: f32) -> f32 {
    (value / GRID_SIZE).round() * GRID_SIZE
}
```

### 节点尺寸

节点尺寸根据 `node_dimensions()` 中的端口数量计算：

```rust
// 默认尺寸常量（以网格单元为单位）
NODE_WIDTH_CELLS = 10      →  宽 200px
NODE_HEADER_ROWS = 2       →  标题区域占 2 行
NODE_MIN_ROWS    = 3       →  最小总行数
NODE_PADDING_BOTTOM = 0.8  →  额外底部填充

// 高度 = GRID_SIZE × max(NODE_MIN_ROWS, NODE_HEADER_ROWS + max(输入端口数, 输出端口数))
```

特殊覆盖：
- `message_list_data` / `qq_message_list_data` 节点有更大的最小高度（`LIST_NODE_MIN_HEIGHT`）
- `brain` 节点有更大的最小高度（`BRAIN_NODE_MIN_HEIGHT`）

如果 `NodeDefinition.size` 已设置，将覆盖自动计算值（以自动计算值为最小下限）。

### 端口中心坐标

每个端口点的中心位置用于边路由：

```rust
// 输入端口：对齐到节点左边缘
center_x = node.x + GRID_SIZE * 0.5
center_y = node.y + GRID_SIZE * (NODE_HEADER_ROWS + port_index + 0.5)

// 输出端口：对齐到节点右边缘
center_x = node.x + node_width - GRID_SIZE * 0.5
center_y = node.y + GRID_SIZE * (NODE_HEADER_ROWS + port_index + 0.5)
```

---

## 特殊节点编辑器

某些节点类型需要自定义对话框编辑器来修改 `inline_values` 并重建动态端口：

### FormatStringNode 编辑器

- 当用户编辑 `template` 内联字段时打开
- 从模板字符串中提取 `${variable}` 名称
- 对节点定义调用 `apply_inline_config()`
- 重建 `NodeDefinition` 中的 `input_ports` 以匹配新变量
- 调用 `refresh_active_tab_ui()` 重新渲染

### JsonExtractNode 编辑器（`json_extract_editor.rs`）

- 带有字段定义表格（名称、数据类型）的对话框
- 保存时：将字段定义序列化为 JSON，存储在 `inline_values["fields_config"]` 中
- 从新字段定义重建 `NodeDefinition` 中的 `output_ports`
- 在节点定义上标记 `dynamic_output_ports = true`

### FunctionNode 编辑器（`function_editor.rs`）

- 对话框编辑函数名称、描述、输入签名和输出签名
- 保存时：序列化 `function_config`，更新可见端口，并同步嵌入子图内的边界节点
- 节点还暴露"进入子图"操作，将子页面推入当前标签页的页面栈

### BrainNode 工具编辑器（`tool_editor.rs`）

- 带有工具定义表格（id、名称、描述、参数、输出）的对话框
- 保存时：将工具配置序列化为 JSON，存储在 `inline_values["tools_config"]` 中
- `brain` 输出端口保持静态；只有 `output` 保持可见，类型为 `Vec<OpenAIMessage>`
- 每个工具行都可以打开其嵌入的子图编辑器页面
