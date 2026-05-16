# PersistedMedia

描述 `zihuan-next` 中的统一媒体处理模型。QQ 消息中的所有图片 segment 均引用一个规范化的 `PersistedMedia` 对象。消息持久化、多模态推理、图片发送与图片检索均消费同一份稳定的媒体事实。

## 本文内容

- [类型参考](#类型参考)
- [设计规则](#设计规则)
- [入站流程](#入站流程)
- [多模态推理](#多模态推理)
- [持久化模型](#持久化模型)
- [出站图片发送](#出站图片发送)
- [Weaviate 图片集合](#weaviate-图片集合)
- [错误行为](#错误行为)
- [开发准则](#开发准则)
- [相关文档](#相关文档)

## 类型参考

### PersistedMediaSource

指定媒体对象的来源类别。

```rust
pub enum PersistedMediaSource {
    Upload,
    QqChat,
    WebSearch,
}
```

| 变体 | 描述 | Serde 值 |
|------|------|----------|
| `Upload` | 本地系统或手动上传路径产生的媒体 | `"upload"` |
| `QqChat` | 来自 QQ / NapCat 消息事件的媒体 | `"qq_chat"` |
| `WebSearch` | 来自 Tavily 或其他联网搜图流程的媒体 | `"web_search"` |

Serde 序列化使用 `snake_case` 命名。

### PersistedMedia

主要的媒体描述符。所有下游图片逻辑均从该结构体读取。

```rust
pub struct PersistedMedia {
    pub media_id: String,
    pub source: PersistedMediaSource,
    pub original_source: String,
    pub rustfs_path: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub mime_type: Option<String>,
}
```

| 字段 | 类型 | 描述 |
|------|------|------|
| `media_id` | `String` | 稳定的媒体标识。运行时逻辑与 Weaviate 图片存储使用。 |
| `source` | `PersistedMediaSource` | 规范化来源类别。 |
| `original_source` | `String` | 源平台提供的原始上游定位字符串。 |
| `rustfs_path` | `String` | RustFS object key，系统内主要文件定位字段。持久化完成前为空字符串。 |
| `name` | `Option<String>` | 人类可读的媒体名称。 |
| `description` | `Option<String>` | 图片内容的文本描述。 |
| `mime_type` | `Option<String>` | 媒体的 MIME 类型。用于多模态构造和出站行为。优先于基于文件名的推断。 |

### ImageMessage

包装 `PersistedMedia` 以纳入消息树。

```rust
pub struct ImageMessage {
    pub media: PersistedMedia,
}
```

**备注：**

- 消息层图片语义通过 `Message::Image(ImageMessage)` 表达。
- 媒体层身份与存储事实通过 `PersistedMedia` 表达。
- 消息层与媒体层刻意分离。同一个 `PersistedMedia` 实例可被多条消息或多个工具路径引用。

## 设计规则

### 单一事实来源

归一化完成后，下游代码不得从以下旧字段重建图片身份：

- `file`
- `path`
- `url`
- `object_key`
- `object_url`
- `local_path`

下游代码必须仅从 `PersistedMedia` 读取媒体事实：

- `image.media.rustfs_path`
- `image.media.original_source`
- `image.media.name`
- `image.media.description`
- `image.media.mime_type`

### 严格的图片 JSON 格式

当前图片 JSON 格式要求包含 `media` 字段。仅包含 `file`、`url`、`object_key`、`object_url` 等旧字段的 legacy payload 被视为不兼容。

```json
{
  "type": "image",
  "data": {
    "media": {
      "media_id": "...",
      "source": "qq_chat",
      "original_source": "...",
      "rustfs_path": "...",
      "name": "...",
      "description": "...",
      "mime_type": "image/jpeg"
    }
  }
}
```

## 入站流程

### 1. QQ 事件解析

**模块：** `ims_bot_adapter/src/adapter.rs`

将平台 payload 解析为 `Vec<Message>`。对于图片 segment，创建初始 `PersistedMedia`，默认值如下：

| 字段 | 初始值 |
|------|--------|
| `source` | `QqChat` |
| `original_source` | 上游 QQ 定位字符串 |
| `rustfs_path` | `""`（空） |
| `name` | 事件中可用时提供 |
| `mime_type` | `None` |

此阶段媒体对象已存在，但尚未持久化。

### 2. RustFS 持久化与媒体归一化

**模块：** `storage_handler/src/object_storage/media_cache.rs`

此模块为图片持久化的归一化边界，执行以下步骤：

1. 从当前图片来源解析字节。
2. 推断或保留内容类型。
3. 上传字节至 RustFS。
4. 构造规范化 `PersistedMedia`。
5. 回写至 `ImageMessage.media`。

此步骤完成后，以下不变量成立：

- `rustfs_path` 已填充有效的 object key。
- `mime_type` 在内容类型检测成功时已设置。
- `media_id` 为稳定值。

### 3. 转发消息递归

图片归一化递归处理转发消息。hydrated forward 节点内的图片在任何下游推理或存储处理之前，均转换为 `PersistedMedia` 对象。

## 多模态推理

以下两个模块从图片消息构造多模态输入：

- `ims_bot_adapter/src/extract_message_from_event.rs`
- `zihuan_service/src/agent/qq_chat_agent_core.rs`

两者遵循相同的解析优先级：

| 优先级 | 来源 | 条件 |
|--------|------|------|
| 1 | `rustfs_path` | 默认路径，从 RustFS 读取字节。 |
| 2 | `original_source` | RustFS 不可用时的回退路径。 |
| 3 | `media.mime_type` | 构造 `data:` URL 时使用。 |
| 4 | `media.name` | 仅作为次级命名信号。 |

### MIME 类型处理

`PersistedMedia.mime_type` 是内容类型信息的权威来源。基于文件名的 MIME 推断不可靠，不得作为主要机制使用。

**旧代码的典型故障：**

- 实际字节：JPEG
- `name`：`"download"`（无扩展名）
- 基于扩展名的回退逻辑：错误地产生 `image/png`

当前模型通过在 `PersistedMedia` 上存储 `mime_type` 并在所有路径复用来避免此问题。

### Data URL 构造

启用多模态输入时，图片内容转换为 `ContentPart::ImageUrl`，使用以下格式之一：

- `data:<mime>;base64,...` — 从已解析字节和 `PersistedMedia.mime_type` 构造
- 直接 data URL — 来源已提供时

data URL 中的 MIME 类型必须优先取自 `PersistedMedia.mime_type`。

## 持久化模型

### 运行时内存缓存

**模块：** `zihuan_graph_engine/src/message_restore.rs`

维护以 `message_id` 为 key、`Vec<Message>` 为 value 的进程内缓存。完整保留图片消息及其 `PersistedMedia`。

### Redis 缓存

Redis 存储 `CachedMessageSnapshotPayload` 结构化快照，而非纯渲染文本：

```rust
pub struct CachedMessageSnapshotPayload {
    pub message_id: String,
    pub content: String,
    pub media_json: Option<String>,
    pub raw_message_json: Option<String>,
}
```

| 字段 | 内容 |
|------|------|
| `media_json` | 直接媒体元数据，包含 `media_id` |
| `raw_message_json` | 完整消息树，包含 `ImageMessage { media }` |

从 Redis 还原的消息可恢复包含完整 `PersistedMedia` 数据的图片 segment。

### MySQL

**模块：** `zihuan_graph_engine/src/message_persistence.rs`

持久化以下列：

| 列 | 内容 |
|----|------|
| `content` | 渲染文本 |
| `media_json` | 从 `PersistedMedia` 派生的序列化媒体记录 |
| `raw_message_json` | 完整序列化的 `Vec<Message>` 树 |

`collect_media_records(...)` 从 `PersistedMedia` 提取图片记录，包含：`media_id`、`source`、`original_source`、`rustfs_path`、`name`、`description`、`mime_type`。

### 还原顺序

`restore_message_snapshot(message_id)` 按以下优先级解析：

| 优先级 | 来源 | 保真度 |
|--------|------|--------|
| 1 | 进程内运行时缓存 | 完整消息树 |
| 2 | Redis `CachedMessageSnapshotPayload` | 完整消息树 |
| 3 | MySQL `raw_message_json` | 完整消息树 |
| 4 | MySQL `content` + `media_json` | 重建；部分保真度 |

`media_id` 在所有层级均可保留，包括 Redis 缓存命中。

## 出站图片发送

**模块：** `ims_bot_adapter/src/ws_action.rs`

将规范化后的 `ImageMessage` 转换为 QQ 出站 API 可发送的 payload。解析顺序：

| 优先级 | 来源 | 条件 |
|--------|------|------|
| 1 | `original_source` | 包含 base64 或 data URL |
| 2 | `original_source` | 为本地文件路径 |
| 3 | `rustfs_path` | RustFS object key |
| 4 | `original_source` | 远程 URL |

出站行为由 `PersistedMedia` 驱动，不使用旧图片字段。

## Weaviate 图片集合

image collection schema 表示持久化媒体记录，而非与消息绑定的图片行。

### 属性

| 属性 | 类型 | 来源 |
|------|------|------|
| `media_id` | string | `PersistedMedia.media_id` |
| `original_source` | string | `PersistedMedia.original_source` |
| `rustfs_path` | string | `PersistedMedia.rustfs_path` |
| `name` | string | `PersistedMedia.name` |
| `description` | string | `PersistedMedia.description` |
| `mime_type` | string | `PersistedMedia.mime_type` |
| `source` | string | `PersistedMedia.source` |

### Upsert 行为

`WeaviateRef::upsert_image_record(...)` 接收 `PersistedMedia` 与向量。Weaviate object 属性直接从 `PersistedMedia` 结构体派生，确保图片检索与 QQ 消息、多模态构造、出站发送使用同一份媒体事实。

## 错误行为

### 不兼容图片 JSON

反序列化器遇到缺少 `media` 字段的 legacy 图片 JSON 时：

1. 通过 `error!` 宏记录日志。
2. 返回反序列化错误。
3. 不执行静默转换。

### 重建回退

当 `raw_message_json` 缺失但 `media_json` 存在时，系统从 `MessageMediaRecord` 重建图片消息。此回退保留 `PersistedMedia` 身份，但保真度低于完整 `raw_message_json` 还原。

## 开发准则

1. **将 `PersistedMedia` 视为唯一事实来源。** 不得从旧字段推导图片身份。
2. **不得向 `ImageMessage` 引入独立定位字段。** 所有文件位置与身份信息必须驻留于 `PersistedMedia`。
3. **解析字节时保留 `mime_type`。** 不得依赖基于文件名的内容类型推断。
4. **无损还原时优先使用 `raw_message_json`。** 精确还原需要完整消息树。
5. **同步更新两条推理路径。** 影响多模态输入的媒体行为变更必须同时应用于 `extract_message_from_event.rs` 和 `qq_chat_agent_core.rs`。

## 相关文档

- [QQ 消息](qq-message.zh-CN.md)
- [QQ 消息存储](qq_message_storage.zh-CN.md)
