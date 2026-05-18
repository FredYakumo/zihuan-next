# QQ Chat Agent Steer 技术手册

本文档是 QQ Chat Agent 中 steer 机制的唯一正式技术说明。

它说明：当某个发送者已经有一个尚未结束的 QQ Chat 回复流程时，同一发送者后续重叠到来的消息会如何被处理。

## 设计目的

steer 机制的目标，是让在线 QQ Chat Agent 能在“同一用户上一轮回复尚未完成”时接受中途纠偏，而不是为同一发送者并发打开第二条回复流程。

也就是说，当前运行时不再把这种重叠输入建模成“Agent 忙”，而是建模成“用户正在调整一条尚未完成的回复方向”。

## 范围

本文档覆盖：

- 活跃回复流期间的同发送者重叠消息识别
- pending-steer 队列行为
- steer 注入尚未结束的 Brain 运行过程
- 错过注入窗口后如何自动进入下一轮 follow-up
- 会话历史持久化规则
- steer 数量限制与配置
- 日志与可观测性边界

本文档不重新定义：

- 群聊触发规则
- QQ 消息初始接入与 inbox 架构
- Brain observer 的通用设计

这些内容仍分别保留在各自专题文档中。

## 运行时模型

对于同一个发送者，QQ Chat Agent 仍然只允许同一时刻存在一条活跃回复流。

当该发送者在当前流程结束前又发来一条新消息时：

1. 这条新消息会被识别为 steer 候选。
2. 它会先进入 QQ Chat Agent 运行时持有的、该发送者专属的 pending-steer 缓冲队列。
3. 当前回复流随后再决定：这条 steer 还能否进入尚未结束的 Brain 对话，还是应该变成下一轮 follow-up 的输入。

这样既保留了单发送者串行语义，又允许进行中的回复被用户中途修正。

## Steer 生命周期

### 1. 接受

只有当该发送者当前已经存在一条活跃回复流时，同一发送者的重叠消息才会被接受为 steer。

被接受的 steer 不会立刻启动第二个任务，而是先附着在当前活跃流程上。

### 2. 入队

被接受的 steer 会进入与发送者 session 关联的 pending-steer 队列。

队列按到达顺序保存。

### 3. 注入窗口

如果当前 Brain 在 tool call 之后还会进入下一轮推理，运行时会在那一轮推理开始前取出队列中的 steer。

当同一个注入窗口里等待中的 steer 不止一条时，它们会按到达顺序合并成一条显式 `user` 消息。

因此模型看到的是一条正常的用户插话，而不是隐藏控制指令。

### 4. 错过注入

如果 steer 到达时，当前回复流已经越过了最后一个可用的推理注入边界，那么它不会因为错过这一轮而直接丢失。

相反，在当前回复流完成后，这条排队中的 steer 会成为下一轮自动 follow-up 对话的主输入。

### 5. 消费与持久化

当 steer 被实际消费后，被消费的输入会写入保存的会话历史。

如果多条 steer 在同一注入窗口内被合并成一条插话，那么历史里记录的是这条合并后的显式用户消息。

## 合并语义

steer 的合并是一个刻意收窄的行为：

- 只有在活跃回复流内部才会发生
- 只有在即将进入下一轮 Brain 推理前才会发生
- 合并保持消息到达顺序
- 合并结果是一条显式注入的 `user` 消息

运行时不会在同一注入窗口里把多条 steer 逐条分别注入。

## 重要边界

steer 机制不是“首条回复前先做一段静默等待”的 debounce 窗口。

第一条用户消息仍会立刻启动当前回复流程。

运行时不会为了看看用户是否还会继续补发内容，而延迟首轮推理。

因此，多消息合并只会发生在回复流程已经进行中、并且后面确实还剩下一轮 Brain 推理可进入的情况下。

## 限制与配置

一条活跃回复流只接受有限数量的 steer 消息。

这个上限由 `QqChatAgentConfig.max_steer_count` 控制。

当前默认值：

- `max_steer_count = 4`

当当前活跃流程已接受的 steer 数量达到上限后，后续新的同发送者重叠消息会被丢弃。

## 任务与 Follow-Up 语义

steer 改变的是“重叠输入如何解释”，但不改变整体任务模型：

- 一条活跃回复流仍然对应一次具体处理的回复任务
- steer 在第一阶段仍然属于这条活跃回复流
- 只有当当前流程结束、且排队 steer 触发续轮时，它才会成为下一轮自动 follow-up 的输入

这也是为什么当前实现不再把同发送者重叠输入归类为“忙碌冲突”。

## 日志与可观测性

steer 主要有两层观测面：

- 任务 trace 中的 steer 接收、注入、续轮事件
- service 级队列管理日志中的入队、丢弃、出队决策

关键日志字段包括：

- `message_id`
- `steer_count`
- `injected_messages`
- 配置的 `max_steer_count`
- 需要时记录的剩余队列长度
- 截断后的 steer payload 预览
- 注入事件上的 `merged=true/false`

当多条 steer 被折叠成一次注入时，`steer_count` 会大于 `injected_messages`。

更细的 trace 格式约定仍放在 [`qq-chat-agent-logging.zh-CN.md`](qq-chat-agent-logging.zh-CN.md)。

## 相关文档

- [`qq-message.zh-CN.md`](qq-message.zh-CN.md)：QQ 消息展开与用户可见行为背景
- [`qq-chat-agent-logging.zh-CN.md`](qq-chat-agent-logging.zh-CN.md)：任务 trace 事件设计
- [`../program-execute-flow.zh-CN.md`](../program-execute-flow.zh-CN.md)：整体运行时与服务边界
