# 节点系统

本文档描述 `zihuan-next` 当前的节点运行时。

## 运行时模型

图运行时是一个同步 DAG 执行器。

`NodeGraph` 拥有：

- 已实例化节点
- 已解析的 inline value
- 运行时变量存储
- 停止标记
- 可选执行回调
- 图边与已加载定义元数据

执行器会对节点进行拓扑排序，并同步执行。

## `Node` Trait

定义在 `zihuan_graph_engine/src/lib.rs`。

重要方法：

- `input_ports()`
- `output_ports()`
- `execute(...)`
- `on_graph_start()`
- `apply_inline_config(...)`
- `set_function_runtime_values(...)`
- `set_runtime_variable_store(...)`

`NodeType` 仍然存在，但当前唯一有效变体是：

```rust
pub enum NodeType {
    Simple,
}
```

## 构建期图准备

当图定义转换成运行时图时：

1. 从注册表创建节点实例
2. 将 inline value 解析成带类型的 `DataValue`
3. 调用 `apply_inline_config(...)`，让节点恢复配置驱动状态
4. 动态端口节点可以在配置恢复后暴露额外端口
5. 将运行时变量存储挂到节点上

该过程发生在 `zihuan_graph_engine::registry::build_node_graph_from_definition(...)`。

## 执行流程

执行时：

1. `prepare_for_execution()` 重置停止标记和运行时变量
2. 每个节点收到 `on_graph_start()`
3. 图按拓扑顺序排序
4. 输入从以下位置收集：
   - 边
   - 绑定的运行时变量
   - inline value
5. 校验输入
6. 调用 `execute(...)`
7. 校验并存储输出
8. 发出可选执行回调

## 动态端口

配置驱动端口的节点应在 `apply_inline_config(...)` 中重建端口。

例子包括：

- function 节点
- format string 节点
- JSON extract 节点
- 一些连接/配置节点

UI 与运行时应从同一份已存储配置推导出同样可见的端口。

## 运行时变量存储

`NodeGraph` 维护一次运行范围内共享的变量存储。

它用于图变量，以及以下节点：

- `set_variable`
- session state helper
- function boundary helper

变量初始值来自已加载的图定义，并会在每次运行开始时重置。

## 停止标记

`NodeGraph` 仍然暴露图级停止标记，用于任务编排请求取消。它不是独立的节点生命周期模型。

## 不属于这里的内容

图运行时不托管：

- bot 监听循环
- HTTP 服务入口
- 自动启动生命周期管理
- 并发 Agent 托管

这些职责属于 `zihuan_service` 和主服务 API/运行时层。

## 注册入口

当前注册表引导路径：

- `zihuan_graph_engine::registry::init_node_registry()` — 内置工具节点
- 由 `storage_handler::init_node_registry()` 扩展
- 由 `ims_bot_adapter::init_node_registry()` 扩展
- 由 `model_inference::init_node_registry()` 扩展
- 由 `zihuan_service::init_node_registry()` 扩展
- 通过 `src/init_registry.rs` 调用 `init_node_registry_with_extensions()` 组合

存储型节点如 `mysql`、`qq_message_list_mysql_persistence`、`message_mysql_get_user_history`、`message_mysql_get_group_history` 和 `message_mysql_search` 由 `storage_handler` 注册。它们的运行时连接句柄是 `DataValue` 引用，例如 `MySqlRef`，由连接 provider 节点产生，并由存储/搜索节点消费。当前 MySQL 消息存储与读取链路见 [`qq_message_storage.zh-CN.md`](qq_message_storage.zh-CN.md)。

## 设计规则

如果某个功能需要独立于一次图运行长期存活，应放入服务运行时。如果它能在一次图调用内完成，应保持为普通同步节点或子图。
