# 程序执行流程

本文档描述项目**当前**的启动与运行方式，包括 Web 应用和 CLI 图执行器。

## 1. Web 应用启动流程

入口：`src/main.rs`

启动顺序：

1. 通过 `src/log_forwarder.rs` 初始化全局日志
2. 通过 `src/init_registry.rs` 中的 `init_node_registry_with_extensions()` 初始化节点注册表
3. 解析 `--host` 和 `--port`
4. 创建 `AppState`
5. 创建 WebSocket 广播通道
6. 把日志转发器接入 app state 和 broadcast
7. 加载系统配置各个 section
8. 自动启动所有 `enabled && auto_start` 的 Agent
9. 构建 Salvo Router
10. 绑定 TCP 监听并开始提供 HTTP/WebSocket 服务

## 2. Web 应用承载了什么

主二进制当前统一承载这些能力：

- `/` 管理界面
- `/editor` 图编辑器
- `/api` REST API
- `/api/ws` WebSocket
- 节点图任务执行
- Agent 生命周期管理
- 系统配置持久化
- 日志写文件、控制台输出和 WebSocket 推送

## 3. 请求与界面流转

### 浏览器界面

- `/` 加载 Vue 3 管理端
- `/editor` 加载浏览器版图编辑器

### API

主要路由分组：

- `/api/system/connections`
- `/api/system/llm-refs`
- `/api/system/agents`
- `/api/graphs`
- `/api/tasks`
- `/api/themes`
- `/api/workflow_set`

### WebSocket

`/api/ws` 会广播服务端事件，例如：

- task started
- task finished
- task stopped
- log message
- graph validation result
- QQ 消息预览数据

## 4. 从 Web 应用执行节点图

当用户从 Web 界面执行图时：

1. API 从 `AppState` 中取出图 session
2. 创建 task 记录
3. 准备运行时上下文与 runtime inline values
4. 通过 `zihuan_graph_engine::registry::build_node_graph_from_definition` 构建图
5. 在 `spawn_blocking` 中执行图
6. 使用 `log_forwarder::scope_task(...)` 采集任务级日志
7. 通过 WebSocket 推送任务生命周期和预览事件
8. 任务以 `success`、`failed` 或 `stopped` 状态结束

图运行时本身仍然是同步的；Web 层只是在外围做异步任务编排。

## 5. Agent 启动与生命周期

进程启动时，Web 应用会从系统配置中读取 Agent 定义，并自动启动同时满足以下条件的 Agent：

- `enabled`
- `auto_start`

也可以通过 `/api/system/agents/<id>/start` 和 `/api/system/agents/<id>/stop` 手动控制。

当前长期运行的 Agent 类型定义在 `model_inference::system_config::AgentType` 中：

- `qq_chat`
- `http_stream`

这些长期运行的服务逻辑由 `zihuan_service` 承载，而不是图执行器。

需要特别注意当前任务模型：

- 启动 Agent 本身**不再**创建任务记录
- Task 列表里的 Agent 任务表示“一次具体响应/请求处理”，而不是 Agent 的存活周期
- `qq_chat_agent` 在真正进入回复流程时创建一个任务，例如 `回复[3507578481]的消息`
- `http_stream_agent` 在每次处理一个 HTTP 请求时创建一个任务
- 群聊里未 `@` 机器人这类纯忽略路径不会创建任务

每个 Agent 响应任务都有独立的：

- `task_id`
- `start_time`
- `end_time`
- `duration_ms`
- `status`
- `error_message`
- `result_summary`
- `log_path`

任务日志按 task 维度持久化到：

- `logs/tasks/<task_id>.jsonl`

`qq_chat_agent` 的任务日志还会额外记录：

- 用户原始消息
- 展开后的推理消息文本
- 历史上下文消息数
- 上下文 token 估算值
- 历史压缩前后 token
- 当前请求 token 消耗信息

当前底层并没有统一的精确 usage 结构，因此 `prompt_tokens` / `completion_tokens` / `total_tokens` 在不可用时会明确记录为 unavailable，并补充估算值，而不是伪造精确数字。

## 6. CLI 执行流程

入口：`zihuan_graph_cli/src/main.rs`

CLI 执行顺序：

1. 解析 `--file` 或 `--workflow`
2. 初始化节点注册表
3. 解析图文件路径
4. 以带迁移兼容的方式加载图 JSON
5. 构建 `NodeGraph`
6. 执行一次图
7. 成功或失败后退出

CLI 不会启动 Web 服务、任务系统、管理界面或 Agent Manager。

## 7. 当前执行边界

最重要的架构边界是：

- `zihuan_graph_engine` 负责同步 DAG 图执行
- `zihuan_service` 负责长期运行的服务型 Agent
- `src/api` 负责 HTTP、WebSocket、任务记录与浏览器状态协调

后续文档和新开发都应以这个边界为准。
