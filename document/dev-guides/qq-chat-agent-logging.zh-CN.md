# QQ Chat Agent 日志手册

本文档说明当前在线 QQ Chat Agent 的任务级日志设计。

主要实现文件：

- `zihuan_service/src/agent/qq_chat_agent_logging.rs`
- `zihuan_service/src/agent/qq_chat_agent_core.rs`
- `zihuan_service/src/agent/classify_intent.rs`
- `zihuan_agent/src/brain.rs`

## 目标

这套日志设计同时解决两个问题：

- 让任务日志对开发者和运维可读
- 保留足够的阶段耗时信息，用于排查慢响应和错误回复

因此当前实现不再默认倾倒大量原始调试状态，而是输出：

- 一串高信号的关键阶段日志
- 一个固定格式的任务结束时间线汇总

## 设计概览

`qq_chat_agent_logging.rs` 是 QQ Chat Agent 任务 trace 格式的唯一归属模块。

它提供两块能力：

- `QqChatTaskTrace`：单次任务的内存 trace 聚合器
- `QqChatBrainObserver`：把 Brain 循环事件转成任务 trace 事件的 `BrainObserver`

`qq_chat_agent_core.rs` 继续只负责业务编排。它应当只做这些事：

- 在任务开始时创建 trace
- 在关键业务边界记录阶段
- 把 observer 注入 `Brain`
- 在任务结束前调用 `finish_with_summary()`

这个拆分是刻意的：业务层决定“什么时候发生”，日志模块决定“如何输出”。

## 当前记录的阶段

当前 trace 会记录以下阶段族：

- 入站消息收到
- 任务创建
- 意图分类
- 意图分类中的 embedding 参与情况
- 发给主大模型的最终消息列表
- Brain 的工具请求轮次
- 工具调用开始与结束
- 主大模型最终返回
- 最终 assistant 结果解析
- 出站 QQ message batch 发送
- 任务结束汇总

每条关键日志都包含：

- 阶段标题
- 毫秒级耗时
- 截断后的高信号 payload 预览

任务结束时会再输出一段固定格式的时间线，使用绝对时间点和阶段耗时。

## 意图分类接入方式

`classify_intent.rs` 现在暴露 `classify_intent_with_trace(...)`。

返回的 trace 信息包括：

- 最终分类结果
- 分类路径：`local_guard`、`similarity_guard`、`llm`
- 是否使用 embedding
- 是否使用意图分类 LLM
- embedding 耗时（如适用）
- 分类总耗时
- 到达 LLM 时的原始标签文本

QQ Chat 任务 trace 会保存这份结果，并用于输出：

- 实时的“意图识别完成”日志
- 任务结束时间线中的意图/embedding 段

## Brain 接入方式

`zihuan_agent::brain::Brain` 现在支持可选 observer。

observer 能接收：

- assistant 发起工具请求的轮次
- 工具开始
- 工具结束
- 最终 assistant 返回 / stop reason

因此 QQ Chat Agent 不再依赖 Brain 自己的文本日志去反推耗时，而是通过 `QqChatBrainObserver` 把工具调用和最终结果统一落到同一个任务 trace 里。

其他 Brain 调用方不受影响，因为 observer 是可选的。

## 输出策略

QQ Chat 任务日志只应保留高信号内容：

- 用户消息
- 意图结果
- 发给大模型的消息列表
- 模型返回内容
- 工具参数
- 工具结果
- 最终发送的 QQ message list
- 最终任务汇总

不要重新引入以下顶层噪音日志：

- raw / expanded 双份消息转储
- 大段历史消息 payload 快照
- 大段 system prompt banner
- 多处重复的 token unavailable 提示
- 工具实现内部和任务 trace 双份工具调用日志

一个简单判断标准是：如果一条日志不能帮助回答“收到了什么、怎么判断、调了什么、返回了什么、最终发了什么”，它通常就不应进入主任务 trace。

## 如何扩展

如果要新增一个可观测阶段，建议按这个顺序做：

1. 在 `QqChatTaskTraceInner` 里加状态或时间字段
2. 在 `QqChatTaskTrace` 上加一个公开方法
3. 在 `qq_chat_agent_core.rs` 的业务边界调用这个方法
4. 如果它必须出现在最终固定汇总里，再更新 `finish_with_summary()`

优先扩展 trace 模块，不要在业务主流程里散落临时 `info!`。

## 边界说明

这个模块不是通用 metrics 系统。

它不负责：

- 在现有任务日志之外额外持久化结构化任务元数据
- 修改 task JSONL schema
- 为所有 agent 类型抽象一套统一 tracing 框架

它当前有意只服务于 QQ 在线回复路径。若未来其他 service agent 也需要同类模式，应先复制设计，再只抽取真正共享的部分。
