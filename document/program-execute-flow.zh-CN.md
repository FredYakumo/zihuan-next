# 程序执行流程

本文档描述项目**当前**的启动与运行方式，包括 Web 应用和 CLI 图执行器。

## 1. Web 应用启动流程

入口：`src/main.rs`

启动顺序：

1. 通过 `src/log_forwarder.rs` 初始化全局日志
2. 通过 `src/init_registry.rs` 中的 `init_node_registry()` 初始化节点注册表
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

相关日志模式的独立说明文档见
[`dev-guides/qq-chat-agent-logging.zh-CN.md`](dev-guides/qq-chat-agent-logging.zh-CN.md)。

当前底层并没有统一的精确 usage 结构，因此 `prompt_tokens` / `completion_tokens` / `total_tokens` 在不可用时会明确记录为 unavailable，并补充估算值，而不是伪造精确数字。

### QQ Chat Agent 当前消息处理模型

`qq_chat_agent` 当前采用“接入层异步分发 + inbox 驱动的业务层同步执行”的混合模型。

消息进入路径如下：

1. `ims_bot_adapter::adapter::BotAdapter::start()` 持续读取 WebSocket 消息。
2. 每条入站文本/二进制消息都会 `tokio::spawn(...)` 一个独立任务执行 `BotAdapter::process_event(...)`。
3. `process_event(...)` 完成 JSON 解析、图片补全、reply/forward message hydration 后，再 `tokio::spawn(...)` 调用 `ims_bot_adapter::event::process_message(...)`。
4. `process_message(...)` 会先复制当前注册的 event handlers，然后对**同一条消息**按注册顺序逐个 `await` handler。
5. `qq_chat_agent` 注册的 handler 会构造 inbox item，并通过共享的 `storage_handler::redis` helper 优先尝试写入 Redis；当 Redis 不可用时回退到进程内内存队列。
6. 后台 inbox consumers 会持续从 Redis 或内存队列取出消息，并通过 `tokio::task::spawn_blocking(...)` 分发执行。
7. 真正的业务处理仍然最终在 `QqChatAgentService::handle_event(...)` 中执行。

这个模型的并发语义是：

- **不同入站消息**：适配器层已经是并发的，因为每条消息在进入 `process_event(...)` 和 `process_message(...)` 前都已经被 `tokio::spawn(...)` 分发到独立任务。
- **同一条消息的多个 handlers**：在 `process_message(...)` 内部仍然是串行 `await`。
- **不同用户的消息**：允许并发执行，也允许全局乱序；当前实现并不尝试做跨用户顺序保证。
- **同一用户的消息**：顺序性由 `qq_chat_agent_core` 内部的 session claim/release 机制保证，而不是由 adapter 层队列保证。
- **enqueue 阶段 Redis 不可用**：handler 会先依赖 `storage_handler` 将失败的 Redis 连接标记失效并尝试重连一次；若 Redis 仍不可用，再回退到本地内存队列，并尽快返回。
- **使用内存 fallback 时发生进程重启**：允许丢失尚未开始处理的内存排队消息。

当前单用户串行控制点在：

- `try_claim_session(...)`
- `release_session(...)`

它们围绕 `SessionStateRef::try_claim(...)` / `release(...)` 建立会话占用语义，确保同一发送者不会同时进入多个实际回复流程。

### QQ Chat Agent 当前的插嘴（steer）机制

当某个发送者已经有一个正在进行中的 QQ Chat 回复流程时，同一发送者后续继续发来的消息，不再被视为“忙碌冲突”，而是会被当作 **插嘴 / steer**。

当前 steer 行为如下：

- 这条重叠消息会先进入该发送者对应的 QQ Agent 私有 pending-steer 队列。
- 如果当前 Brain 在 tool call 之后还会继续进入下一轮推理，队列里的 steer 会在下一次推理前被一次性取出；当存在多条 steer 时，它们会按到达顺序合并成一条显式 `user` 消息注入当前对话，而不是逐条分别注入。
- 如果 steer 到达得太晚，没赶上当前 Brain 轮次，它会在当前回复流程结束后，自动成为下一轮 follow-up 对话的第一条输入。
- 被实际消费掉的 steer 会写入会话历史；当多条 steer 在同一轮被合并注入时，历史中会追加这条合并后的显式用户消息。
- 单次活跃回复流程里最多接受多少条 steer，由 `QqChatAgentConfig.max_steer_count` 控制；当前默认值为 `4`。超过上限的 steer 会被丢弃。

这里有一个重要边界：当前实现没有在首条消息前额外增加等待窗口，也不会为了“再等等用户是否继续发”而延迟首次推理。多条 steer 只会在“当前回复流程尚未结束，且 Brain 还来得及进入下一次 inference”时发生合并；如果已经错过这一边界，它们仍会按现有 follow-up 机制进入下一轮。

这意味着：同一用户的重叠输入，当前不再被建模成“Agent 忙”，而是被建模成“用户正在对尚未完成的回复插嘴调整方向”。

因此，当前模型的关键边界是：

- adapter 层负责接入、解析、异步分发消息；
- `qq_chat_agent` handler 只负责入队并尽快返回；
- Redis 是首选 inbox 后端；没有 Redis 连接或 Redis 入队失败时回退到进程内内存队列；
- inbox 路径中的 Redis 连接生命周期由 `storage_handler` 负责，`zihuan_service` 只负责决定继续使用 Redis 还是降级到内存；
- inbox consumer 负责把队列中的消息转入阻塞式业务执行；
- 单用户串行由 service 层 session 锁控制；
- 图引擎本身仍然保持同步执行，不直接参与 adapter 接入层并发。

关于“运行时实例归属”“Redis helper 管理的重连”“业务层 fallback”三者的区别，见 [`config-and-connection-instances.zh-CN.md`](config-and-connection-instances.zh-CN.md)。

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
