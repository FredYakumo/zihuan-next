# QQ Chat Agent 回复与发送流程

本文档描述 `qq_chat_agent` 当前如何把 Brain 的最终输出转换为 QQ 消息，并统一通过 `zihuan_service/src/agent/qq_chat_agent_msg_send.rs` 发送。

主要实现文件：

- `zihuan_service/src/agent/qq_chat_agent_msg_send.rs`
- `zihuan_service/src/agent/qq_chat_agent_claimed.rs`
- `zihuan_service/src/agent/tools/reply_message.rs`

## 概览

统一后的链路分为三层：

1. Brain 结束后，从 `brain_output` 里提取最后一条可发送的 Assistant 文本。
2. 收集本轮工具调用返回的图片 `media_id`，并读取本轮 `reply_message` 工具留下的 reply 指令。
3. 由 `qq_chat_agent_msg_send.rs` 统一完成：
   - 文本后处理
   - `@sender` / `@QQ号` 解析
   - `[Image media_id=...]` 解析
   - `[no reply]` 抑制发送
   - 文本拆分
   - 单图 / 多图 / 混合文本图片规则
   - forward 规划
   - reply 挂载
   - 最终发送与持久化

上层模块不再直接拼接 QQ batch，只负责决定“要发什么”。

## 模型输出契约

模型当前允许直接输出的特殊标记只有：

- `@sender`
- `@123456`
- `[Image media_id=media-xxxx]`
- `[no reply]`

模型不再允许输出：

- `[Reply his_message]`
- `[Reply message_id=...]`

如果模型需要引用消息，必须调用内置工具 `reply_message`。

## `reply_message` 工具

`reply_message` 是 `qq_chat_agent` 的默认内置工具之一，可在前端开关启用状态。

参数：

- `message_id?: integer`

行为：

- 传入 `message_id`：引用指定的 QQ 消息。
- 不传 `message_id`：默认引用触发当前 Agent 处理流程的那条消息。
- 工具本身不直接发消息，只把 reply 指令写入本轮运行时上下文。
- 最终真正发送时，由 `qq_chat_agent_msg_send.rs` 统一消费这条指令。

## 统一发送规划

### 阶段 1：Assistant 文本标准化

- 群聊里先把 `@sender` 替换为 `@<sender_id>`。
- 提取 `[no reply]`，若存在则直接返回 `suppress_send=true`。
- 提取 `[Image media_id=...]`。
- 提取正文里的 `@QQ号`。

### 阶段 2：文本拆分

文本仍然使用现有的分词 + 字数限制逻辑拆分，保持代码块和引号修复能力。

规则：

- 文本拆成 1 段或 2 段：允许普通消息发送。
- 文本拆成 3 段及以上：正文必须转成 forward。

### 阶段 3：图片规则

- 单张图片：直接发送。
- 多张图片：必须转成 forward。

### 阶段 4：混合文本 / 图片 / 文本

统一模块会保留原始顺序。

如果同时满足以下任一条件：

- 文本总拆分段数 >= 3
- 图片数量 > 1

则正文 remainder 统一转为一个 `ForwardMessage`。

### 阶段 5：`@` 与 reply 外提

forward 场景中：

- `@` 消息不会放进 forward。
- `reply_message` 产生的 reply 也不会放进 forward。

统一模块会先寻找一个可发送的 carrier 批次来承载 reply / @：

1. 优先抽取 forward 里的第一段可单独发送文本。
2. 如果没有文本，但有图片，则使用第一张图片。
3. 如果既没有文本也没有图片，则发送 reply-only carrier。

最终效果通常是：

- `Reply + @ + text`
- `Reply + image`
- 然后再发送剩余的 forward

## 发送入口

下列场景都已统一改为经过 `qq_chat_agent_msg_send.rs`：

- Brain 最终回复
- 命令 echo 文本
- `/task` 详情 side effect
- 长任务开始通知
- 长任务完成通知
- 内置工具进度通知
- editable tool / `tool_subgraph` 进度通知

底层实际发包仍然复用 `ims_bot_adapter::message_helpers`，但 QQ Chat 的拆分、forward、reply、mention 策略只保留在 agent 层。

## 历史持久化

- 发送成功后，仍然通过现有 outbound persistence 写入消息记录。
- 会话历史里不再保存旧的 `[Reply ...]` 文本协议。
- 可见文本以统一发送模块处理后的结果为准，至少不再保留旧 Reply 标记。

## 关键数据结构

### `QqReplyDirective`

- `Explicit { message_id }`
- `TriggerMessage`

表示本轮最终回复要引用哪条消息。

### `QqSendContext`

统一发送上下文，包含：

- adapter
- target_id
- is_group
- group_name
- bot_id / bot_name
- mention_target_id
- persistence
- max_text_chars

### `QqOutboundPlan`

统一规划结果，包含：

- `batches: Vec<Vec<Message>>`
- `suppress_send: bool`
- `visible_text: Option<String>`

## 维护规则

- 新的 QQ Chat 出站逻辑优先放到 `qq_chat_agent_msg_send.rs`。
- 不要再在 `qq_chat_agent_claimed.rs`、`tools/common.rs`、`tool_subgraph.rs` 里各自维护不同的拆分 / forward / reply 规则。
- 如果未来要增加新的 QQ 特殊发送行为，优先扩展 `QqOutboundPlan` 与统一规划函数，而不是新增旁路发送逻辑。
