# 代码约定

面向当前 `zihuan-next` 代码库的命名、文件放置与开发规则。

## 命名

### Rust

| 对象 | 约定 | 示例 |
|---|---|---|
| 类型 / 枚举 / trait | `UpperCamelCase` | `NodeGraph`, `AgentConfig` |
| 函数 / 方法 / 字段 | `snake_case` | `build_router`, `load_agents` |
| 模块 / 文件 | `snake_case` | `local_candle_embedding.rs` |
| 常量 | `SCREAMING_SNAKE_CASE` | `SYSTEM_CONFIG_FILE` |
| 节点结构体 | `<Purpose>Node` | `FormatStringNode`, `TavilySearchNode` |

### 节点 type_id

注册表 `type_id` 使用稳定的 `snake_case` 标识，例如：

- `format_string`
- `function`
- `llm_infer`
- `qq_chat_agent`
- `mysql`
- `rustfs`

已发布的 `type_id` 不应随意变更；如需变更，必须提供迁移支持以兼容已有图 JSON。

## 文件放置

### 一文件一节点

每个节点实现应独立放在单独文件中。

常见位置：

- `zihuan_graph_engine/src/util/`：通用运行时/工具节点
- `zihuan_graph_engine/src/`：引擎自有功能模块
- `model_inference/src/nodes/`：AI 与 Agent 相关节点
- `storage_handler/src/`：存储/连接相关节点
- `ims_bot_adapter/src/`：适配器相关节点
- `zihuan_service/src/nodes/`：Brain 与 Agent 节点

### 注册

新增节点后：

1. 在父级 `mod.rs` 导出
2. 在所属 crate 注册表中注册

当前入口：

- `zihuan_graph_engine::registry::init_node_registry()` — 内置工具节点
- `storage_handler::init_node_registry()`
- `ims_bot_adapter::init_node_registry()`
- `model_inference::init_node_registry()`
- `zihuan_service::init_node_registry()`
- 合并引导：`src/init_registry.rs` 调用 `init_node_registry_with_extensions()` 统一注册

## 包职责（高层）

| 包 | 职责 |
|---|---|
| `zihuan_core` | 核心类型 |
| `zihuan_agent` | Brain 工具调用循环引擎 |
| `zihuan_graph_engine` | 节点图运行时 |
| `model_inference` | LLM、Embedding、AI 节点 |
| `storage_handler` | 连接型节点与存储辅助 |
| `ims_bot_adapter` | IMS 适配集成 |
| `zihuan_service` | 长生命周期服务与任务托管、Brain/Agent 节点 |
| `node_macros` | 端口定义的过程宏 |
| `src/api` | Web API 与任务编排 |
| `webui/` | Web UI |

## 类型放置

- 通用/共享类型定义，以及可能引入循环引用的类型定义，统一放在 `zihuan_core`。
- 其余代码与类型定义必须放在功能职责所属的包内。
- 跨包设计优先高内聚、低耦合。

## 图运行时规则

- 图必须保持 DAG
- 从图运行时视角看，新节点行为应保持同步语义
- 动态端口应在 `apply_inline_config()` 中按配置重建
- 新增工具节点前优先复用现有辅助逻辑
- 复杂性应封装在节点或服务中，而不是增加画布拓扑复杂度

## 服务边界

不要把长生命周期执行模型重新放回图运行时。

如果行为需要：

- 监听循环
- 托管 HTTP 端点
- 后台消息消费
- 自动启动生命周期

应放入 `zihuan_service` 及其 API/配置配套，而不是新增图执行模式。

## 连接职责归属

- 运行时连接的创建、健康检查、重连与失效淘汰，应集中放在拥有该连接职责的连接/存储包中。
- 对 Redis 来说，共享重连行为应放在 `storage_handler` 的 helper 中，例如 `storage_handler::redis`，而不是散落在 `zihuan_service::agent` 这类业务模块里。
- service 层可以决定上层 fallback 策略，例如从 Redis 切到内存队列，但不应复制底层 `redis_cm` 生命周期管理逻辑。

## 日志与任务 Trace

- 普通日志继续使用 `log` 宏，保持简洁、可搜索。
- 对于带阶段耗时、时间线汇总、工具调用统计这类复杂任务日志，不要把格式化逻辑散落在业务主流程里。
- 应优先抽到独立模块，业务层只负责在关键边界打点，日志模块负责统一输出格式。
- 当前参考实现是 `zihuan_service/src/agent/qq_chat_agent_logging.rs`，对应说明文档见 [`qq-chat-agent-logging.zh-CN.md`](qq-chat-agent-logging.zh-CN.md)。
