# QQMessage

本文档详细说明项目中的 `QQMessage` 模型，包括数据结构、serde 兼容规则、运行时 hydration 行为，以及它如何被 QQ Agent 和节点系统消费。

代码入口：

- `zihuan_core/src/ims_bot_adapter/models/message.rs`

相关运行时路径：

- `ims_bot_adapter/src/adapter.rs`
- `ims_bot_adapter/src/extract_message_from_event.rs`
- `zihuan_service/src/agent/qq_chat_agent.rs`
- `zihuan_service/src/agent/qq_chat_agent_core.rs`

## 系统中的角色

`QQMessage` 对应 Rust 代码中的 `crate::ims_bot_adapter::models::message::Message`。它表示一个 QQ 消息 segment，而不是完整消息事件。

当前实现支持这些 segment 类型：

- `text`
- `at`
- `reply`
- `image`
- `forward`

一个完整入站消息通常表示为 `Vec<QQMessage>`，例如：

```json
[
  { "type": "text", "data": { "text": "hello" } },
  { "type": "at", "data": { "qq": "123456" } },
  { "type": "reply", "data": { "id": "987654321" } }
]
```

在节点系统中，该类型映射为：

- `DataType::QQMessage`
- `DataValue::QQMessage(QQMessage)`
- `Vec<QQMessage>`

## Rust 结构

核心 enum 定义为：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum Message {
    #[serde(rename = "text")]
    PlainText(PlainTextMessage),
    #[serde(rename = "at")]
    At(AtTargetMessage),
    #[serde(rename = "reply", alias = "replay")]
    Reply(ReplyMessage),
    #[serde(rename = "image")]
    Image(ImageMessage),
    #[serde(rename = "forward")]
    Forward(ForwardMessage),
}
```

这意味着 JSON payload 使用外层 `type` 字段选择变体，变体自己的字段放在 `data` 下。

在文档和节点类型名中，这个 enum 通常称为 `QQMessage`，用于明确平台含义。阅读代码时需要记住：

- 文档中的 `QQMessage`
- 节点类型名中的 `QQMessage`
- Rust 实现中的 `Message`

都指同一个概念类型。

## Segment 类型

### `text`

```rust
pub struct PlainTextMessage {
    pub text: String,
}
```

用途：

- 表示纯文本内容
- `Display` 输出原始 `text`
- `get_type()` 返回 `"text"`

示例：

```json
{ "type": "text", "data": { "text": "hello" } }
```

### `at`

```rust
pub struct AtTargetMessage {
    #[serde(rename = "qq", alias = "target")]
    #[serde(default, deserialize_with = "deserialize_option_string_from_string_or_number")]
    pub target: Option<String>,
}
```

用途：

- 表示 @ 提及目标
- 输入 JSON 接受 `target` 或 `qq`
- 接受字符串或数字输入
- 内部统一规范化为 `Option<String>`

兼容行为：

- `null` 或字段缺失 -> `None`
- `"123"` -> `Some("123".to_string())`
- `123` -> `Some("123".to_string())`

显示行为：

- `Display` 输出 `@{target}`
- 如果 target 缺失，输出 `@null`

示例：

```json
{ "type": "at", "data": { "qq": 123456 } }
```

### `reply`

```rust
pub struct ReplyMessage {
    #[serde(deserialize_with = "deserialize_i64_from_string_or_number")]
    pub id: i64,
    #[serde(skip)]
    pub message_source: Option<Vec<Message>>,
}
```

用途：

- 表示对另一条消息的回复/引用
- `id` 接受字符串或数字输入
- `message_source` 是运行时 hydrated 上下文，会从 serde 中排除
- 当存在时，`message_source` 包含被引用消息的完整 segment 列表

兼容行为：

- `"123"` 和 `123` 都会反序列化为 `i64`
- 非数字字符串会反序列化失败

显示行为：

- `Display` 有意保持为 `[Reply of message ID 123]`
- 详细引用内容由更高层 helper 渲染，不由 `Display` 渲染

示例：

```json
{ "type": "reply", "data": { "id": "987654321" } }
```

### `image`

```rust
pub struct ImageMessage {
    pub media: PersistedMedia,
}
```

用途：

- 表示图片 segment
- 引用一个规范化后的持久化媒体对象，而不是一组松散 locator

运行时行为：

- 图片 segment 会递归归一化成 `PersistedMedia`
- 该归一化同时作用于顶层消息和转发消息中的图片
- 多模态 builder 优先使用 `PersistedMedia.rustfs_path` 与 `PersistedMedia.mime_type`

完整多媒体模型请见 `persisted-media.zh-CN.md`。

### `forward`

```rust
pub struct ForwardMessage {
    pub id: Option<String>,
    pub content: Vec<ForwardNodeMessage>,
}

pub struct ForwardNodeMessage {
    pub user_id: Option<String>,
    pub nickname: Option<String>,
    pub id: Option<String>,
    pub content: Vec<Message>,
}
```

用途：

- 表示合并转发/组合转发消息
- 入站事件可能只有 `id`
- 详细节点内容稍后通过 NapCat `get_forward_msg` hydration

运行时行为：

- forward hydration 递归发生在 `ims_bot_adapter/src/adapter.rs`
- 如果 `ForwardMessage` 只有 `id`，adapter 会调用 `get_forward_msg(message_id)`
- 返回 payload 会被宽松解析：
  - 节点 wrapper 可以是 `{ "data": ... }`，也可以是直接对象
  - 节点内容可以在 `content` 或 `message` 下
  - 内容可以是：
    - `Vec<Message>` 形状的数组
    - 单个 message 对象
    - CQ-code 字符串，如文本混合 `[CQ:image,...]` 与 `[CQ:forward,...]`
- 支持递归嵌套 forward

显示行为：

- 只有 `id`：`[Forward of message ID ...]`
- 已 hydrated `content`：`[Forward with N node(s)]`
- 可读展开由递归渲染 helper 处理，不由 `Display` 处理

## Serde 兼容策略

QQ 平台 payload 和 adapter 输入经常在字符串值、数字值、别名字段名、值形状上有差异。该文件有意采用“宽输入，窄内部表示”的策略。

### `deserialize_i64_from_string_or_number`

用于 `ReplyMessage.id`。

接受：

- JSON number
- JSON string

拒绝：

- object
- array
- bool
- 非数字字符串

目标是保证内部表示始终为 `i64`。

### `deserialize_option_string_from_string_or_number`

用于 `AtTargetMessage.target` 和 `ForwardMessage.id`。

接受：

- `null`
- number
- string

目标是保证内部为 `Option<String>`，从而简化 bot-id 比较、去重和显示逻辑。

### Value-level forward 解析

`ims_bot_adapter/src/adapter.rs` 包含额外的非 derive forward payload 解析：

- CQ-code 字符串会解析成 `Vec<Message>`
- `[CQ:forward,id=...]` 会变成 `Message::Forward { id, content: [] }`
- 嵌套 CQ forward 后续会递归 hydration

这有意比普通 `serde_json::from_value::<Vec<Message>>()` 更宽容。

## `Display` 语义

`Message` 及其所有变体都实现了 `Display`。这不只是调试或展示用途。一些更高层逻辑仍然直接使用这些字符串形式。

当前输出规则：

- `text` -> 原始文本
- `at` -> `@target`
- `reply` -> `[Reply of message ID ...]`
- `image` -> `[Image ...]`
- `forward` -> `[Forward of message ID ...]` 或 `[Forward with N node(s)]`

如果修改 `Display`，你修改的是运行时行为，不只是 UI 文案。它会直接影响：

- 日志中的 fallback 文本渲染
- 仍然使用 `to_string()` 的代码路径
- debug 输出

不过，当前更高层的用户可见渲染已经不只依赖 `Display`。对于 reply 和 forward，可读文本现在通过 `render_messages_readable()` 等递归 helper 构建。

## `MessageProp` 聚合

`MessageProp` 不是原始消息结构。它是从 `Vec<QQMessage>` 派生出的结构，用于意图检测和 LLM 消费：

```rust
pub struct MessageProp {
    pub content: Option<String>,
    pub ref_content: Option<String>,
    pub is_at_me: bool,
    pub at_target_list: Vec<String>,
}
```

字段含义：

- `content`：通过递归渲染构建的人类可读字符串，包含 hydrated forward 内容
- `ref_content`：从 `reply.message_source` 提取的上下文文本，也会递归渲染
- `is_at_me`：消息是否提及 bot 自己
- `at_target_list`：按首次出现顺序去重后的 @ 目标

### 构建规则

`MessageProp::from_messages(messages, bot_id)` 大致执行：

1. 通过 `render_messages_readable(messages)` 渲染完整消息列表
2. 收集所有 `AtTargetMessage.target` 值
3. 对每个 `reply`，如果存在 `message_source`，递归渲染为 `ref_content`
4. 如果提供 `bot_id`，检查它是否出现在 `at_target_list` 中

### 示例

输入：

```rust
vec![
    Message::PlainText(PlainTextMessage { text: "Hello".into() }),
    Message::At(AtTargetMessage { target: Some("42".into()) }),
]
```

输出大致为：

```text
content = Some("Hello @42")
ref_content = None
is_at_me = true   // 当 bot_id == "42"
at_target_list = ["42"]
```

该结构影响回复决策、上下文组装，以及持久化到 MySQL `content` 的内容。

## Hydration 与递归展开

当前 reply / forward 消息有三个不同运行时阶段：

1. adapter hydration
2. 可读渲染
3. QQ-agent inference 展开

### Adapter hydration

在 `ims_bot_adapter/src/adapter.rs` 中：

- `reply` segment 会尝试通过 `restore_message_snapshot()` 从运行时缓存或 MySQL 还原 `message_source`
- `forward` segment 通过 NapCat `get_forward_msg` hydration `content`
- 嵌套 forward 会递归 hydration
- hydrated forward 内部的图片也会递归缓存到对象存储

这意味着下游代码看到的内存中 `MessageEvent` 可能比原始平台事件丰富很多。

### 可读渲染

在 `zihuan_core/src/ims_bot_adapter/models/message.rs` 中，`render_messages_readable()` 提供递归文本视图：

- 纯文本和 @ 直接渲染
- reply 在原位置保持紧凑，但会为 `ref_content` 贡献完整引用文本
- forward 渲染为带发送者前缀节点内容的块

该可读渲染是以下内容的基础：

- `MessageProp.content`
- `MessageProp.ref_content`
- 新消息 MySQL `content` 持久化

### QQ-agent inference 展开

`zihuan_service/src/agent/qq_chat_agent_core.rs` 增加了一个 Agent 专用步骤：

- `expand_event_for_inference()` 克隆入站 `MessageEvent`
- `expand_messages_for_inference()` 递归把 reply/forward 壳替换为显式文本边界和内部内容
- 展开后的 event 用于：
  - `build_user_message()`
  - `extract_user_message_text()`
  - 传入可编辑 QQ-agent tool 子图的固定 `message_event` 输入

这很重要，因为 Agent 应该基于完全展开的引用和转发内容进行推理，而存储和原始日志仍可以保留更接近平台形状的消息列表。

## 多模态提取

以下两个路径现在都理解 hydrated forward：

- `ims_bot_adapter/src/extract_message_from_event.rs`
- `zihuan_service/src/agent/qq_chat_agent_core.rs`

在两个文件中：

- `Message::Forward` 会渲染为显式 `[转发内容]` 块
- 节点内容会递归遍历
- forward 节点内的图片可以变成多模态 `ContentPart`

如果所选 LLM 报告 `supports_multimodal_input = false`，Agent 仍然不能真正视觉检查图片内容。在这种情况下，它会收到展开文本和图片占位符，但不会收到真正的图片 part。

## 嵌套 Forward 支持

嵌套 forward 消息在运行时消息处理路径中端到端支持：

- CQ-code 解析可以生成内部 `Message::Forward` segment
- adapter hydration 会递归展开嵌套 forward
- 图片缓存会递归进入嵌套 forward 节点
- Agent inference 展开会在调用 brain/LLM 前递归进入嵌套 forward
- 当显式使用 MySQL `MessageEvent` 持久化路径时，会把 fully hydrated tree 存入 `raw_message_json`

当前实现的实际限制不是递归深度，而是上游 NapCat payload 是否仍能被 adapter 的宽松 value-level parser 解析。

## 扩展 `QQMessage` 时的要求

如果新增 QQ 消息 segment 类型，不要只扩展 enum。至少还要检查这些位置：

1. `zihuan_core/src/ims_bot_adapter/models/message.rs`
2. `fmt::Display for Message`
3. `MessageBase::get_type()`
4. `MessageProp::from_messages()`
5. 如果新 segment 可以出现在 forward 内部，检查 adapter hydration
6. `ims_bot_adapter` 与 `qq_chat_agent` 中的多模态 builder
7. 如果该 segment 需要 lossless reply reconstruction，检查持久化和还原行为
8. 每个声明 `DataType::QQMessage` 或 `Vec(QQMessage)` 的节点

建议约束：

- 对仍保留兼容性的消息类型，保持 serde 输入兼容宽松；图片 segment 现在是刻意严格的
- 不要让一个不支持的字段导致整条消息不可用
- 明确定义 `Display` 语义
- 决定新 segment 是否影响 `content`、`ref_content`、@ 提取、多模态提取或持久化

## 与节点系统的关系

文档提到 `QQMessage` 时，通常指节点层类型名，不一定是 Rust 精确类型名。

示例：

- `string_to_plain_text` 节点输出单个 `QQMessage`
- `qq_message_list_data` 节点输出 `Vec<QQMessage>`
- 一些节点会把 `MessageEvent.message_list` 转发给下游节点

实践中：

- bot adapter 层把平台输入转换为 `Vec<Message>`
- 节点层把同一份数据暴露为 `QQMessage` 或 `Vec<QQMessage>`

这是同一数据的两层命名。

## 当前限制

在当前代码状态下，`QQMessage` 覆盖：

- 纯文本
- @ 提及
- reply 引用
- 图片
- 合并转发

它尚未覆盖其他常见 QQ segment 类型，例如：

- emoji
- 文件
- 语音

如果上游输入开始包含这些类型，应先定义：

- JSON 结构
- LLM 应看到什么文本表示
- 节点 UI 是否需要编辑支持
- 运行时逻辑是否需要额外资源下载或存储行为

## 相关文件

- `zihuan_core/src/ims_bot_adapter/models/message.rs`
- `zihuan_core/src/ims_bot_adapter/models/event_model.rs`
- `ims_bot_adapter/src/adapter.rs`
- `ims_bot_adapter/src/extract_message_from_event.rs`
- `zihuan_service/src/agent/qq_chat_agent.rs`
- `zihuan_service/src/agent/qq_chat_agent_core.rs`
- `zihuan_graph_engine/src/message_restore.rs`

## 编程式发送 Helper

当节点图之外的代码需要发送 QQ 消息时，使用 `ims_bot_adapter/src/message_helpers.rs` 中的 helper，而不是重复编写 WebSocket action 调用。

```rust
use ims_bot_adapter::message_helpers::{
    get_bot_id,
    send_friend_text,
    send_group_text,
    send_friend_batches,
    send_group_batches,
    send_group_progress_notification,
    send_friend_progress_notification,
};
```

| 函数 | 用途 |
|---|---|
| `get_bot_id(adapter)` | 返回 bot 自己的 QQ ID，类型为 `String` |
| `send_friend_text(adapter, target_id, text)` | 向好友发送纯文本消息 |
| `send_group_text(adapter, target_id, text)` | 向群发送纯文本消息 |
| `send_friend_batches(adapter, target_id, batches)` | 向好友发送 `Vec<Vec<Message>>` 批次 |
| `send_group_batches(adapter, target_id, batches)` | 向群发送 `Vec<Vec<Message>>` 批次 |
| `send_group_progress_notification(adapter, group_id, mention_id, content)` | 向群发送 `@mention + text` |
| `send_friend_progress_notification(adapter, target_id, content)` | 向好友发送普通进度消息 |

所有函数都会自动处理 Tokio context，通过 `block_in_place` 在同步节点代码运行于 Tokio worker thread 时也可以安全调用。
