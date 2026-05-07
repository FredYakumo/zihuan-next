# 配置与连接实例

本文说明 `zihuan-next` 中 **连接配置** 和 **运行时连接实例** 的区别。

## 核心概念

系统现在明确区分两层：

- **配置**
- **运行时实例**

它们有关联，但不是同一个东西。

## 1. 连接配置

连接配置是持久化保存到系统配置中的记录。

例如：

- 一个 MySQL 连接定义
- 一个 RustFS 对象存储定义
- 一个 Weaviate 定义
- 一个 IMS BotAdapter 定义

这些记录在管理界面的这里维护：

- `连接配置`

它们会保存到：

- Windows：`%APPDATA%/zihuan-next_aibot/system_config/system_config.json`
- Linux/macOS：`$XDG_CONFIG_HOME` 或 `$HOME/.config/zihuan-next_aibot/system_config/system_config.json`

当前磁盘结构已经统一到单一根对象：

```json
{
  "version": 2,
  "configs": {
    "connections": [],
    "llm_refs": [],
    "agents": []
  }
}
```

旧版根字段 `connections` / `llm_refs` / `agents` 在读取时会自动迁移到这个新结构，写回时只会写新结构。

每条配置都有一个稳定标识：

- `config_id`

内部仍会保留旧 `id` 作为迁移期兼容字段，但新的对外主键是：

- `config_id`

当前实现中，`connections`、`llm_refs`、`agents` 都已经接入统一的配置中心：

- 由 `ConfigCenter` 负责读取和写回用户数据目录中的配置文件
- 由 `config_id` 作为统一主键
- 旧版顶层结构会在读取时自动迁移

这意味着：

- 连接配置使用 `config_id`
- 模型配置使用 `config_id`
- Agent 配置也使用 `config_id`

## 2. 运行时连接实例

运行时连接实例是根据某个配置动态创建出来的、真实存活在内存中的连接。

例如：

- 一个正在工作的 MySQL pool
- 一个正在工作的 RustFS/S3 client
- 一个正在工作的 Weaviate client
- 一个正在工作的 IMS BotAdapter 会话

运行时实例具有这些特征：

- 按需创建
- 尽量复用
- 不持久化保存为系统配置
- 使用 `instance_id` 标识

每个运行时实例都通过下面的字段指向它来源的配置：

- `config_id`

## 3. 为什么要分成两层

这样拆分后，系统在行为上更清晰，也更容易运维。

配置层负责：

- 定义连接参数
- 决定连接是否启用
- 提供稳定名称和稳定 ID

运行时实例层负责：

- 只在真正使用时创建 live client
- 优先复用健康连接
- 在连接管理器页面展示当前活动连接
- 支持强制关闭，而不删除底层配置

## 4. 节点图如何使用它们

节点图不会保存运行时实例 ID。

节点图保存的是：

- `config_id`

执行时，节点会向对应的运行时连接管理器请求：

1. 查找这个 `config_id` 是否已有健康实例
2. 如果有，就复用
3. 如果没有，就新建

如果这个配置：

- 不存在
- 已禁用

执行就会报错。

为了兼容旧图，旧 JSON 里可能还保留：

- `connection_id`

图加载时会自动迁移成：

- `config_id`

## 5. 连接管理器页面

管理界面新增了一个专门页面：

- `连接管理器`

这个页面展示的是**运行时实例**，不是保存下来的配置。

它会显示：

- 连接名字
- `config_id`
- `instance_id`
- 开始时间
- 持续时间
- 是否维持长连接
- 心跳时间
- 状态
- 强制关闭按钮

当前界面会额外做两件事：

- 卡片或表格里的长 ID 会缩短显示，例如 `abcd1234...`
- 当某个配置对应多个运行时实例时，会显示成类似 `abcd1234..., 等3个`

如果你想查看或关闭当前 live 连接，看这里。

如果你想新建、编辑、启用、禁用某个配置，看：

- `连接配置`

## 6. 维持长连接与心跳

运行时实例还可能带两个只存在于运行时的行为字段：

- `keep_alive`
- `heartbeat_interval_secs`

它们**不是**用户在配置页里编辑的字段。

它们不会从用户配置读取，也不会写回配置文件，而是在代码中由运行时管理器赋值。

当前行为：

- MySQL、RustFS、Weaviate 这类存储实例默认不是长连接保活实例
- IMS BotAdapter 实例会被标记为长连接
- IMS BotAdapter 实例会定期发送心跳

如果 `keep_alive = true`，这个实例不会因为空闲清理而被自动关闭。

如果设置了 `heartbeat_interval_secs`，管理器会定期发送一个轻量动作来检查连接是否还可用。

## 7. 强制关闭是什么意思

强制关闭运行时实例会：

- 关闭当前 live 实例
- 把它从运行时管理器中移除
- 但不会删除对应的保存配置

后续如果节点或 Agent 再次需要这个 `config_id`，管理器仍然可以重新创建一个新的实例。

日志中通常会看到：

- 配置读取成功：输出 `config_id`
- 实例创建成功：输出 `instance_id` 和 `config_id`
- 实例空闲被清理：输出 `instance_id` 和 `config_id`
- 用户手动强制关闭：输出 `instance_id` 和 `config_id`

## 8. 一个实际例子

假设你创建了一条保存配置：

- 名称：`Main Weaviate`
- `config_id`：`abc123`

之后：

- 某个节点图使用了 `config_id = abc123`
- 运行时管理器创建了一个真正的 Weaviate client
- 这个 live client 会有它自己的 `instance_id`

你在连接管理器页面里看到的是这个运行时实例。

如果你在 `连接配置` 页面中禁用了这条配置，那么之后再使用 `abc123` 就会报错，并且现有相关运行时实例会被清理。
