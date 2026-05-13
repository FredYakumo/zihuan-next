# 节点开发指南

本指南描述在**当前 Simple-only 节点图运行时**下，如何向项目中添加一个新节点。

完整运行时契约请参见 [../dev-guides/node-system.md](../dev-guides/node-system.md)。

---

## 开发流程

### 1. 先判断它到底该不该是节点

当某个能力可以在一次图执行期间**同步完成**时，适合做成节点。

适合做节点的场景：

- 数据变换
- 解析
- 一次性 HTTP / 数据库 / 对象存储操作
- Brain/tool 子图编排
- 持久化辅助

不适合做节点的场景：

- 长时间存活的订阅
- bot 事件循环
- 独立 HTTP 服务
- 必须脱离单次图运行而持续存在的后台服务

这些职责应该放到 Rust 服务运行时。

### 2. 每个节点一个文件

典型放置位置：

| 区域 | 目录 |
|---|---|
| 通用工具 / 变换 | `zihuan_graph_engine/src/util/` |
| 数据库 / 存储辅助 | `zihuan_graph_engine/src/` 下对应模块 |
| 存储 / 连接 / 搜索 | `storage_handler/src/` |
| Bot 相关同步辅助 | `ims_bot_adapter/src/` |
| LLM / 向量化 / Agent 配置 | `model_inference/src/nodes/` |
| Brain / Agent | `zihuan_service/src/nodes/` |

### 3. 提供构造函数

```rust
pub fn new(id: String, name: String) -> Self
```

### 4. 实现 `Node` trait

在当前运行时中，大部分节点只需要实现：

- `id()`
- `name()`
- `input_ports()`
- `output_ports()`
- `execute()`

可选钩子：

- `on_graph_start()`：单次运行开始时重置状态
- `apply_inline_config()`：读取节点卡片配置、重建动态端口
- `set_function_runtime_values()`：供特殊 function 边界节点使用
- `set_runtime_variable_store()`：需要图级运行时变量时使用

### 5. 仔细定义端口

除非端口列表必须动态重建，否则优先使用 `node_input!` 和 `node_output!`。

检查点：

- 端口名使用 `snake_case`
- required / optional 设置正确
- 类型尽量具体
- hidden 端口只用于内部连线

### 6. 导出节点

将其加入父级 `mod.rs`。

### 7. 注册节点

- `zihuan_graph_engine` 中的节点 → `zihuan_graph_engine/src/registry.rs`
- `storage_handler` / `ims_bot_adapter` / `model_inference` / `zihuan_service` 中的节点 → 在所属 crate 的 `init_node_registry()` 中调用 `register_node!`，该函数由 `src/init_registry.rs` 统一引入

### 8. 验证行为

做最小但有效的验证：

- `cargo check`
- 加载包含该节点的图
- 执行覆盖该节点行为的工作流

只有在用户明确要求，或复杂度确实值得时，再补自动化测试。

---

## 常见陷阱

- 应该放进服务运行时的能力却硬塞进节点
- 动态端口没有在 `apply_inline_config()` 中重建
- 端口命名含糊
- 已知具体类型时却返回过于宽松的输出
- 一个节点混合了太多不相关职责

---

## 完成检查清单

- 节点有独立文件
- 存在 `new(id: String, name: String)`
- 端口名清晰且稳定
- `execute()` 与预期的同步行为一致
- 可选钩子只在需要时使用
- 节点已从模块导出
- 节点已注册到正确的注册表
- 行为已通过合适粒度的手工或自动验证

---

## 相关文档

- 运行时契约：[../dev-guides/node-system.md](../dev-guides/node-system.md)
- 节点执行模型：[../dev-guides/node-types.md](../dev-guides/node-types.md)
- 动态端口：[dynamic-port-nodes.md](./dynamic-port-nodes.md)
- 图 JSON：[node-graph-json.zh-CN.md](./node-graph-json.zh-CN.md)
- 生命周期：[node-lifecycle.zh-CN.md](./node-lifecycle.zh-CN.md)
