# QQMessage 存储

本文档描述当前 QQ 消息存储与 MySQL 读取路径。

规范运行时表是 `message_record`。MySQL 后端消息存储是可选的；只有当图或服务路径提供 `MySqlRef` 时才会启用。

## 相关实现

连接与运行时句柄：

- `zihuan_core/src/data_refs.rs` 定义 `MySqlConfig`
- `zihuan_graph_engine/src/data_value.rs` 定义 `DataType::MySqlRef` 与 `DataValue::MySqlRef`
- `storage_handler/src/mysql.rs` 定义当前基于系统配置的 `MySqlNode`
- `storage_handler/src/connection_manager.rs` 创建并缓存 live MySQL pool
- `storage_handler/src/resource_resolver.rs` 为 API 与图绑定解析 MySQL 连接

消息持久化与读取：

- `zihuan_graph_engine/src/message_persistence.rs` 持久化 `MessageEvent`
- `zihuan_graph_engine/src/qq_message_list_mysql_persistence.rs` 持久化调用方提供的 `Vec<QQMessage>`
- `zihuan_graph_engine/src/message_mysql_history_common.rs` 包含共享 MySQL 查询辅助函数
- `zihuan_graph_engine/src/message_mysql_get_user_history.rs` 读取某个发送者的最近消息
- `zihuan_graph_engine/src/message_mysql_get_group_history.rs` 读取某个群的最近消息
- `zihuan_graph_engine/src/message_mysql_search.rs` 搜索 `message_record`
- `zihuan_graph_engine/src/message_restore.rs` 从运行时缓存或 MySQL 还原被引用消息
- `src/api/explorer.rs` 暴露管理端资源浏览器的 MySQL 查询接口
- `storage_handler/src/message_store.rs` 包含 Redis/MySQL 辅助 store，但它不是主要图节点路径

Schema 参考：

- `database/models/message_record.py`
- `migrations/versions/6d101e418d9b_add_message_record_table.py`
- `migrations/versions/e8c7d6f2b123_make_at_target_nullable.py`
- `migrations/versions/4f2a8c1d9e3b_add_media_json_to_message_record.py`
- `migrations/versions/9b7f4c2d1a6e_add_raw_message_json_to_message_record.py`

## MySQL 连接链路

当前首选路径使用已保存的系统连接配置：

1. MySQL 连接保存在系统配置 `connections` 集合中。
2. 图使用 `storage_handler/src/mysql.rs` 中的 `mysql` 节点。
3. `MySqlNode` 将选中的 `config_id` 存入 inline config。
4. 执行 `execute` 时，`MySqlNode` 调用：

```rust
RuntimeStorageConnectionManager::shared().get_or_create_mysql_ref(config_id)
```

5. `RuntimeStorageConnectionManager` 加载连接定义，创建 `sqlx::MySqlPool`，保留一个 Tokio runtime 供 pool 后台工作使用，并返回 `Arc<MySqlConfig>`。
6. `MySqlNode` 输出 `DataValue::MySqlRef(config)`。
7. 下游存储/搜索节点读取 `mysql_ref` 输入，并通过 pool 查询。

## 存储对象

存储层不会直接持久化单个 `QQMessage` segment。它存储规范化消息元数据、渲染后的内容，以及可选的序列化消息树：

```rust
pub struct MessageRecord {
    pub message_id: String,
    pub sender_id: String,
    pub sender_name: String,
    pub send_time: NaiveDateTime,
    pub group_id: Option<String>,
    pub group_name: Option<String>,
    pub content: String,
    pub at_target_list: Option<String>,
    pub media_json: Option<String>,
    pub raw_message_json: Option<String>,
}
```

字段含义：

- `message_id`：平台消息 ID
- `sender_id` / `sender_name`：发送者身份
- `send_time`：持久化时间戳
- `group_id` / `group_name`：群消息上下文
- `content`：渲染后的可读消息文本
- `at_target_list`：逗号分隔的 @ 目标
- `media_json`：序列化的直接媒体元数据，主要用于 fallback 重建
- `raw_message_json`：当写入方拥有完整 `MessageEvent` 时，序列化的 `Vec<Message>` 树

## MySQL 表结构

当前 ORM 模型是：

```python
class MessageRecord(Base):
    __tablename__ = 'message_record'

    id = Column(Integer, primary_key=True, autoincrement=True)
    message_id = Column(String(64), nullable=False)
    sender_id = Column(String(64), nullable=False)
    sender_name = Column(String(128), nullable=False)
    send_time = Column(DateTime, nullable=False)
    group_id = Column(String(64), nullable=True)
    group_name = Column(String(128), nullable=True)
    content = Column(String(2048), nullable=False)
    at_target_list = Column(String(512), nullable=True)
    media_json = Column(String(4096), nullable=True)
    raw_message_json = Column(String(65535), nullable=True)
```

重要约束：

- `id` 是主键。
- `message_id` 没有声明唯一约束。
- ORM 模型没有声明索引。
- 长内容会被拆分为多行，并使用相同的 `message_id`；需要完整消息的消费者必须按 `id` 排序聚合分片。

## 写入路径

### MessageEvent 持久化

`zihuan_graph_engine::message_persistence::persist_message_event(...)` 是通用事件持久化入口。

它会执行：

1. 通过 `cache_message_snapshot(event)` 将事件写入进程内还原缓存。
2. 如果传入或全局注册了 Redis ref，则可选写入结构化 Redis 快照。
3. 如果传入或全局注册了 MySQL ref，则可选写入事件到 MySQL。

MySQL 插入路径是 `persist_message_to_mysql(...)`。

对 MySQL，它会存储：

- 规范化元数据
- 可读 `content`，按 `CONTENT_MAX_CHARS` 拆分
- `at_target_list` 只放在第一个分片
- `media_json` 只放在第一个分片
- `raw_message_json` 只放在第一个分片

当前直接调用方包括 `ims_bot_adapter/src/message_helpers.rs` 中的出站消息 helper，例如：

- `send_friend_text_with_persistence`
- `send_group_text_with_persistence`
- `send_friend_batches_with_persistence`
- `send_group_batches_with_persistence`

入站 adapter 事件不会由 `ims_bot_adapter/src/event.rs` 自动插入；它们会分发给已注册 handler 和 Brain agent。当 MySQL 历史是需求时，需要在图或服务路径中显式持久化入站消息。

### QQMessage 列表节点持久化

`QQMessageListMySQLPersistenceNode` 存储调用方提供的 `Vec<QQMessage>` 和必需元数据。

输入：

- `qq_message_list`
- `message_id`
- `sender_id`
- `sender_name`
- 可选 `group_id`
- 可选 `group_name`
- `mysql_ref`

该节点写入规范化元数据、渲染后的 `content`、`at_target_list` 和 `media_json`。

它当前不写 `raw_message_json`，因为它没有接收完整 `MessageEvent`，并且使用的是另一种 insert SQL 形状。

## 读取路径

### 图节点

消息历史图节点都接收 `mysql_ref`，并调用 `run_mysql_query(...)`：

- `message_mysql_get_user_history`
- `message_mysql_get_group_history`
- `message_mysql_search`

共享 helper：

- 应用 30 秒查询超时
- 当存在 `MySqlConfig.runtime_handle` 时在该 handle 上运行查询
- 按 `message_id` 聚合分片行
- 将消息格式化为可读字符串

查询模式：

- 用户历史：`sender_id = ?`
- 群内用户历史：`sender_id = ? AND group_id = ?`
- 群历史：`group_id = ?`
- 搜索：可选 `sender_id`、`group_id`、内容 `LIKE`、时间范围、排序方向和 limit

### Reply 快照还原

`restore_message_snapshot(message_id)` 按以下顺序还原被引用消息：

1. 进程内运行时缓存
2. Redis 结构化快照缓存
3. MySQL `raw_message_json`
4. 从拼接后的 `content` 加 `media_json` fallback 重建

`ims_bot_adapter/src/adapter.rs` 在填充 `reply.message_source` 时使用它。

MySQL lookup 读取：

```sql
SELECT content, media_json, raw_message_json
FROM message_record
WHERE message_id = ?
ORDER BY id ASC
```

这就是分片顺序和首分片元数据重要的原因。

当前 Redis 快照 payload 包含：

- `message_id`
- `content`
- `media_json`
- `raw_message_json`

这使得 Redis 命中时也能保留 `PersistedMedia.media_id` 和图片重建能力，而不是退化成纯文本。

### 管理端资源浏览器

`src/api/explorer.rs::query_mysql` 支撑 MySQL 资源浏览器接口。

它通过 `storage_handler::resource_resolver::build_mysql_ref(...)` 解析 `connection_id`，然后使用可选过滤条件查询 `message_record`：

- `message_id`
- `sender_id`
- `sender_name`
- `group_id`
- `content`
- `send_time_start`
- `send_time_end`

响应会分页，并将展示内容截断为 preview。

### MessageStore 辅助封装

`storage_handler::MessageStore` 是围绕以下存储的辅助封装：

- Redis cache
- MySQL `message_record`
- 内存 fallback map

可用方法包括：

- `load_messages_from_mysql(limit)`
- `get_messages_by_sender(sender_id, group_id, limit)`
- `store_message(message_id, message)`
- `store_message_record(record)`
- `get_message_record(message_id)`
- `get_message(message_id)`
- `get_message_with_mysql(message_id)`

该 helper 当前不驱动主要图节点持久化路径。新增消息存储行为时，优先使用图节点和 `message_persistence` 函数。

## 注册

存储相关节点由 `storage_handler::init_node_registry()` 注册：

- `mysql`
- `qq_message_list_mysql_persistence`
- `message_mysql_get_user_history`
- `message_mysql_get_group_history`
- `message_mysql_search`

`storage_handler` 通过 `document/dev-guides/node-system.zh-CN.md` 中描述的主注册表引导路径扩展基础图注册表。

## 实用说明

- 新图应使用基于系统配置的 `mysql` 节点。
- 将 `mysql_ref` 显式传给历史/搜索/持久化节点。
- 如果需要 reply/forward 重建，优先使用会持久化 `raw_message_json` 的写入路径。
- `QQMessageListMySQLPersistenceNode` 适合简单的图级持久化，但它不能像 `MessageEvent` 持久化那样准确还原完整嵌套结构。
- 当前 schema 允许重复 `message_id` 行，同时也用重复 `message_id` 表示分片。在决定分片消息如何表示前，不要加入去重逻辑。
- 在主 `MessageEvent` 持久化路径中，Redis 现在保存结构化消息快照；但从持久化和可恢复性的角度看，MySQL `raw_message_json` 仍然是更推荐的持久事实来源。
