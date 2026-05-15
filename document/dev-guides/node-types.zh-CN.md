# 节点类型

本文档说明 `zihuan-next` **当前**的节点执行模型。

简短结论：图执行现在只支持 **Simple**。

---

## 当前状态

运行时仍然暴露 `NodeType`，但只剩一个变体：

```rust
pub enum NodeType {
    Simple,
}
```

每个节点都作为同步转换执行：

```rust
inputs -> execute() -> outputs
```

---

## 现在的 “Simple” 含义

Simple 节点：

- 当图执行器按拓扑顺序到达它时运行一次
- 接收所有已解析输入值，形式为 `HashMap<String, DataValue>`
- 同步返回一个输出 map
- 可以通过 `on_graph_start()` 维护单次运行内状态
- 可以在 `apply_inline_config()` 中重建配置驱动端口

典型例子：

- 字符串/JSON 转换
- 分支/router 节点
- LLM 请求节点
- Brain/tool 节点
- 数据库/对象存储引用节点
- 持久化节点

## 长生命周期行为迁移到哪里

如果你需要以下行为：

- 长时间运行的 bot 消息消费
- 后台网络监听器
- 并发 Agent 托管
- OpenAI-compatible HTTP 服务
- 自动启动生命周期

这些现在属于**服务运行时**，不是节点图。

相关代码：

- `zihuan_service/src/agent/mod.rs` (`AgentManager`)
- `src/system_config/`
- `src/api/system_config.rs`

---

## 新开发的实际规则

新增行为时，在下面几条路径中选择。

### 路径 A：同步节点

当行为能在一次图执行调用中完成时，使用普通节点。

例子：

- 解析/转换数据
- 调用一次 HTTP API
- 运行一个工具子图
- 持久化一批消息/图片

### 路径 B：服务运行时组件

当行为必须独立于图运行长期存活时，使用服务/运行时组件。

例子：

- QQ chat agent 事件循环
- HTTP stream agent
- 启动生命周期管理器

### 路径 C：由服务调用的同步工具节点/子图

当 Agent 需要可复用工作流行为时，把该行为保持在同步节点图或工具子图中，由服务调用它。

这是保持图拓扑简单，同时支持丰富 Agent 工作流的首选方式。

---

## 检查清单

对于任何新节点：

- 它应能作为基于 `execute()` 的同步节点正确工作
- 它不应需要自己的后台生命周期
- 它不应依赖图级异步调度

如果这些条件有任意一项不成立，该行为很可能应放到节点图之外。
