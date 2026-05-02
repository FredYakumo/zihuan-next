# 函数子图

> 🌐 [English](function-subgraphs.md) | 简体中文

本文档描述 `function` 节点使用的嵌入式子图系统。

关于 `brain` 工具子图和 Agentic Brain 运行时，请参见 [../llm/brain.md](../llm/brain.md)。

---

## 概述

`function` 节点拥有一个私有子图和函数签名：

- 子图嵌入在节点的内联 JSON 配置中。
- 节点可见的输入和输出端口从该嵌入签名重建。
- 嵌入子图在外层 `function` 节点运行期间作为子图执行。

这使可复用的函数逻辑保持自包含，而不是存储为顶层图。

---

## 核心数据模型

运行时和持久化使用以下共享定义：

```rust
pub struct FunctionPortDef {
    pub name: String,
    pub data_type: DataType,
}

pub struct EmbeddedFunctionConfig {
    pub name: String,
    pub description: String,
    pub inputs: Vec<FunctionPortDef>,
    pub outputs: Vec<FunctionPortDef>,
    pub subgraph: NodeGraphDefinition,
}
```

### 存储位置

| 所有者 | 内联键 | 含义 |
|------|------------|---------|
| `function` 节点 | `function_config` | 完整的嵌入函数定义 |
| `function_inputs` 节点 | `signature` / `runtime_values` | 声明的输入签名和注入的调用参数 |
| `function_outputs` 节点 | `signature` | 声明的输出签名 |

---

## 边界节点

每个函数子图包含两个特殊的内部节点：

| 节点类型 | 作用 |
|----------|------|
| `function_inputs` | 将运行时调用参数展开为动态输出端口 |
| `function_outputs` | 通过动态输入端口收集子图结果 |

重要规则：

- 它们作为普通 `NodeDefinition` 持久化在嵌入子图中。
- 它们的位置和尺寸与子图一起保存。
- 它们仅供内部使用，不得从面板创建。
- 它们不得从编辑器中删除或复制。
- `signature` 等隐藏配置端口不在画布上渲染。

子图通过 `packages/zihuan_node/src/function_graph.rs` 中的 `sync_function_subgraph_signature()` 保持一致。

---

## 函数节点运行时

`function` 节点是一个动态端口节点：

- `dynamic_input_ports = true`
- `dynamic_output_ports = true`

其可见端口从 `function_config` 重建。

### 执行流程

1. 读取并验证 `function_config`。
2. 克隆嵌入子图。
3. 将运行时参数注入 `function_inputs` 节点。
4. 将声明的输出签名注入 `function_outputs`。
5. 构建并执行子图。
6. 读取 `function_outputs` 的执行结果。
7. 根据外层函数签名验证每个声明的输出。
8. 将验证后的输出映射返回给调用者。

错误会用外层函数节点 id 包装，以便 UI 将失败归因于调用节点，而不仅是边界节点。

---

## 图 JSON 行为

嵌入子图是存储在节点内联配置中的递归图载荷。

### 函数节点结构

```jsonc
{
  "node_type": "function",
  "dynamic_input_ports": true,
  "dynamic_output_ports": true,
  "inline_values": {
    "function_config": {
      "name": "MyFunction",
      "description": "",
      "inputs": [{ "name": "text", "data_type": "String" }],
      "outputs": [{ "name": "result", "data_type": "String" }],
      "subgraph": {
        "nodes": [ ... function_inputs ... , ... function_outputs ... ],
        "edges": [ ... ]
      }
    }
  }
}
```

### 刷新与自动修复

`refresh_port_types()` 和 `auto_fix_graph_definition()` 会递归处理：

- `function_config.subgraph`

它们还会：

- 从嵌入配置重建动态端口
- 保持边界节点签名同步
- 清理引用已删除旧端口的边

---

## UI 模型

子图编辑使用每个文件标签页的页面栈。

### GraphTabState 页面栈

每个标签页存储：

- 主图的一个根页面
- 嵌套子图的零个或多个子页面

每个页面保持其独立的：

- 图
- 选中状态
- 内联输入缓存
- 画布平移/缩放状态

在保存、打开、切换标签或导航回根之前，当前页面状态会提交回所有者节点的嵌入配置。

### 导航

编辑子图时，文件标签页显示面包屑式导航栏：

- `返回`
- 可点击的 `主图`
- 当前函数名称

函数节点子图和 Brain 工具子图共用相同的导航机制，但 Brain 特有的行为记录在 [../llm/brain.md](../llm/brain.md) 中。
