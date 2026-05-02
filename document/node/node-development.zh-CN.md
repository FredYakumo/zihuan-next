# 节点开发指南

> 🌐 [English](node-development.md) | 简体中文

本指南描述向项目添加新节点的常规工作流程。它是实现工作的实践入口：文件放在哪里、实现什么、注册什么，以及在认为节点完成之前需要验证什么。

关于详细运行时契约（如 `Node` trait、端口和数据类型规则、执行顺序和 EventProducer 生命周期），请参见 [../dev-guides/node-system.md](../dev-guides/node-system.md)。

---

## 何时阅读本指南

在以下情况下阅读本文档：

- 添加全新节点
- 将实验性节点迁移到主注册表
- 检查节点实现是否完整
- 审查节点 PR 是否缺少集成步骤

如果需要精确的 API 语义而非开发流程，请切换到 [../dev-guides/node-system.md](../dev-guides/node-system.md)。

---

## 开发工作流

### 1. 决定节点类型

首先选择执行模型：

- 对于一次性变换和路由逻辑，使用 `Simple`
- 对于长时间运行的事件源（如套接字、定时器或轮询循环），使用 `EventProducer`

如果区别不清楚，在编写代码前先阅读 [../dev-guides/node-system.md](../dev-guides/node-system.md) 和 [../dev-guides/node-types.md](../dev-guides/node-types.md)。

### 2. 创建节点文件

决定新节点属于哪个 crate，然后将其放在每节点一个文件中：

| 节点类别 | Crate | 目录 |
|---|---|---|
| 通用工具或变换节点 | `packages/zihuan_node` | `packages/zihuan_node/src/util/` |
| Bot / QQ 消息节点 | `packages/zihuan_bot_adapter` | `packages/zihuan_bot_adapter/src/` |
| LLM / AI 节点 | `packages/zihuan_llm` | `packages/zihuan_llm/src/` |

除非功能确实引入了新的责任领域，否则不要创建新目录。

节点结构体通常应暴露：

```rust
pub fn new(id: String, name: String) -> Self
```

### 3. 实现 `Node` trait

针对所选执行模型实现节点：

- 简单节点应专注于 `execute()`
- EventProducer 节点应实现 `node_type()`、`on_start()`、`on_update()`、`on_cleanup()` 和 `set_stop_flag()`
- 如果节点读取直接配置在节点卡片上的值，实现 `apply_inline_config()`

保持实现简洁。一个节点应该清晰地做一件事，而不是吸收不相关的编排逻辑。

### 4. 仔细定义端口

使用 `node_input!` 和 `node_output!` 声明端口，除非节点具有必须在运行时从配置重建的动态端口。

定义端口时检查以下几点：

- 端口名具有描述性并使用 `snake_case`
- 必要输入确实是必要的
- 输出类型尽量具体，而不是过度使用 `Any`
- 动态端口对注册表探测和 UI 编辑来说保持足够确定性

关于动态端口行为，请参见 [dynamic-port-nodes.md](./dynamic-port-nodes.md)。

### 5. 从模块导出节点

添加文件后，更新父 `mod.rs`，使节点可以从注册表和测试中引用。

### 6. 注册节点

在合适的注册表中注册——这使节点可用于图加载、UI 面板和元数据查询。

- **`packages/zihuan_node` 中的节点** → `packages/zihuan_node/src/registry.rs` 的 `init_node_registry()` 中。
- **`packages/zihuan_bot_adapter` 或 `packages/zihuan_llm` 中的节点** → `src/init_registry.rs`。

注册时：

- `type_id` 一旦发布后保持稳定
- 使用现有的类别约定
- 使显示名称和描述在编辑器中易于理解

### 7. 添加测试

至少为节点的正常行为添加单元测试。当验证或解析是节点职责的一部分时，添加错误路径测试。

在以下情况使用 `NodeGraph` 集成测试：

- 节点依赖图连线行为
- 内联值很重要
- EventProducer 下游执行需要覆盖
- 注册或 JSON 加载是风险的一部分

### 8. 验证周边集成

完成之前，验证节点不仅已实现，还已集成到系统的其余部分：

- 父模块导出了它
- 正确的注册表文件中存在注册表条目
- 如果节点添加了值得注意的能力，文档或示例已更新
- 节点在预期图结构中行为正确

---

## 常见陷阱

- 编写完实现后忘记注册节点
- 使用模糊的端口名，使图编辑困难
- 将可选输入标记为必要，或反之
- 应该强制具体类型时返回松散类型的输出
- EventProducer 节点不定期检查停止标志
- 应用配置前动态端口节点暴露不稳定的端口列表
- 编写在一个地方混合数据变换、传输和持久化关注点的节点

---

## 完成检查清单

在认为节点完成之前使用此检查清单：

- 节点存在于其独立文件中
- `new(id: String, name: String)` 存在
- `Node` trait 实现与预期执行模型匹配
- 端口声明清晰且使用稳定命名
- 需要时实现了内联配置处理
- 节点从父 `mod.rs` 导出
- 节点在正确的注册表（`packages/zihuan_node/src/registry.rs` 或 `src/init_registry.rs`）中注册
- 单元测试覆盖主要行为
- 当错误情况是契约的一部分时，已测试错误情况
- 所有 EventProducer 实现都存储并检查停止标志

---

## 相关文档

- 详细系统契约：[../dev-guides/node-system.md](../dev-guides/node-system.md)
- 节点执行模型概述：[../dev-guides/node-types.md](../dev-guides/node-types.md)
- 动态端口节点：[dynamic-port-nodes.md](./dynamic-port-nodes.md)
- 图 JSON 格式：[node-graph-json.zh-CN.md](./node-graph-json.zh-CN.md)
- 节点生命周期详情：[node-lifecycle.zh-CN.md](./node-lifecycle.zh-CN.md)
