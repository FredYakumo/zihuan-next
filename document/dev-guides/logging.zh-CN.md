# 日志

本文档描述当前日志管线。

## 概览

项目使用 `log` crate 进行日志调用，主 Web 应用使用 `src/log_forwarder.rs` 作为全局 logger 包装层。

logger 会把日志分发到：

- 控制台输出
- `./logs/` 下的文件
- WebSocket 广播消息
- 当执行处于某个 task 作用域内时，写入该 task 的日志存储

## 主 Web 应用初始化

`src/main.rs` 首先执行：

1. 使用 `LogUtil::new_with_path("zihuan_next", "logs")` 创建 `BASE_LOG`
2. 调用 `log_forwarder::init(&BASE_LOG)`
3. 后续通过以下函数附加应用状态与 WebSocket 广播：
   - `log_forwarder::set_app_state(...)`
   - `log_forwarder::set_broadcast(...)`

这个顺序很重要，因为启动失败和自动启动 Agent 的日志也应该可见。

## `log_forwarder` 增加了什么

`src/log_forwarder.rs` 包装 `LogUtil`，并增加两个运行时行为。

### WebSocket 转发

每条日志记录都会转换成 `ServerMessage::LogMessage`，并广播给连接到 `/api/ws` 的客户端。

### Task 作用域日志捕获

当代码运行在 `log_forwarder::scope_task(task_id, || { ... })` 中时，日志行也会追加到 `AppState` 中该 task 的已存储日志列表。

这就是图执行日志和 Agent 响应日志能够出现在 task UI 与 task 日志 API 中的原因。

## Task 日志流程

图执行期间：

1. API 创建一条 task 记录
2. 执行在 `scope_task(task_id, ...)` 中运行
3. 作用域内每次 `log::*` 调用都会追加到 task 日志列表
4. 同一条日志仍然会写入控制台/文件，并通过 WebSocket 广播

也就是说，一次日志调用会同时进入所有观察通道。

Agent 处理期间：

1. 启动 Agent **不会** 创建 task 记录
2. 只有当 Agent 开始处理一个具体输入/请求时，才会创建 task 记录
3. QQ chat 每个回复流程对应一个 task，例如 `回复[123456]的消息`
4. HTTP stream 每个请求对应一个 task
5. 处理代码在该响应 task ID 下运行，因此每次 `log::*` 调用都会持久化到 `logs/tasks/<task_id>.jsonl`

这样 task 列表会聚焦于具体工作单元，而不是长期存活的 Agent 在线时间。

## 持久化 Task 日志

Task 日志以 JSONL 文件形式持久化到：

- `logs/tasks/<task_id>.jsonl`

每个 task 都有自己的日志文件。task 日志 API 从这些持久化文件读取；UI 不是读取只存在于内存中的临时日志。

当前 task 记录还保存：

- `start_time`
- `end_time`
- `duration_ms`
- `status`
- `error_message`
- `result_summary`
- `log_path`

对于 Agent 响应 task，这意味着每一次单独回复/请求都有自己的持久日志轨迹。

## 日志级别

最大日志级别从 `RUST_LOG` 推导。如果未设置，则回退为 `info`。

示例：

```bash
RUST_LOG=debug ./target/release/zihuan_next
RUST_LOG=trace cargo run
```

## CLI 说明

`zihuan_graph_cli` 不初始化 Web 应用的 `log_forwarder` 管线。WebSocket/task 分发行为属于主服务运行时。

## 使用建议

日志适用于：

- 启动/关闭里程碑
- 连接与服务生命周期状态
- fallback 启用
- 重要执行检查点
- 可恢复异常

不要用日志替代向调用方返回结构化错误。
