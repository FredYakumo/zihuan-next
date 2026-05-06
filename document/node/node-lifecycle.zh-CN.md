# 节点生命周期与执行流程

本文档描述 `zihuan-next` **当前** 的节点生命周期。

节点图运行时现在是同步、基于 DAG 的执行模型，不再存在独立的 `EventProducer` 生命周期。

---

## 执行模型

所有节点都遵循同一种模型：

| 模型 | 核心方法 | 是否可在单次运行内保留状态 | 是否存在图级后台循环 |
|-------|-------------|--------------------------|-----------------------------|
| `Simple` | `execute()` | 可选，通过 `on_graph_start()` | 否 |

---

## 生命周期阶段

### 1. 图装配

执行前，运行时会：

- 加载图 JSON
- 从注册表创建节点实例
- 恢复 inline values
- 通过 `apply_inline_config()` 恢复动态端口

### 2. 运行初始化

每次图执行开始时：

```text
for each node:
  on_graph_start()
for each node:
  apply_inline_config(...)
inject runtime variable store
```

`on_graph_start()` 适合做“单次运行作用域”的状态重置，例如计数器、缓存、临时累积器。

### 3. 拓扑执行

图会先做拓扑排序，然后按顺序逐个执行：

```text
for node in topological order:
  collect inputs
  validate_inputs(inputs)
  outputs = execute(inputs)
  validate_outputs(outputs)
  store outputs
```

### 4. 可选执行回调

节点执行完成后，图可能会触发执行回调，供 UI 或任务系统更新预览和日志。

---

## 数据流

### 输入收集

输入来源包括：

- 上游边连接
- inline/default 值
- 特殊函数边界节点的运行时注入

### 输出存储

输出会写入图的执行结果池，供下游节点继续读取。

### 校验

每个节点执行前后，运行时会检查：

- 必填输入是否存在
- 输入/输出类型是否与端口声明兼容

---

## 动态端口节点

动态端口节点仍然遵循同一生命周期，但通常依赖 `apply_inline_config()` 在执行前重建端口结构。

常见场景：

- function 边界节点
- Brain/tool 配置节点
- JSON 提取类节点

经验法则：

- 结构恢复放在 `apply_inline_config()`
- 运行时逻辑放在 `execute()`

---

## Function 与 Tool 子图

tool/function 子图从图运行时视角看仍然是同步执行的。

运行时可能注入：

- function runtime values
- 共享 runtime variable store

这样服务层中的 QQ Agent 或 HTTP Stream Agent 就可以复用节点图逻辑，而不需要把图引擎改回异步生命周期。

---

## 停止 / 取消

`NodeGraph` 仍然持有图级别的 stop flag，主要用于任务控制集成。

变化点：

- 节点不再实现 `set_stop_flag()`
- 节点不再由图运行时驱动内部生命周期循环
- 取消现在主要是图/任务层的能力，而不是节点类型分支

---

## 已移除的生命周期

下面这套生命周期已经不再存在：

```text
on_start -> loop { on_update } -> on_cleanup
```

现在的节点执行流程已经收敛为：

```text
on_graph_start -> apply_inline_config -> execute
```

---

## 服务运行时边界

凡是需要独立于单次图执行长期存活的能力，现在都放在图生命周期之外：

- QQ Agent 消息订阅
- HTTP Stream 服务
- auto-start / stop
- 多 Agent 并发托管

这些职责位于 Rust 服务运行时，尤其是：

- `src/service/agent_manager.rs`

服务运行时可以反过来调用图引擎，执行同步工具子图或工作流子图。

---

## 开发建议

实现节点时按下面顺序思考：

1. 哪些状态要在每次图运行开始时重置？放进 `on_graph_start()`。
2. 哪些结构来自 inline config？放进 `apply_inline_config()`。
3. 哪些工作属于真正的运行时逻辑？放进 `execute()`。

如果某个能力需要：

- 订阅
- 长连接
- 独立 HTTP 服务
- 持续后台处理

就不要再把它建模成图生命周期，请移动到服务运行时。
