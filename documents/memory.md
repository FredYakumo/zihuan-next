# 记忆读写接口

Planer 任务的记忆来源为 `{"kind":"planer","access_level":1}`，写入时来源权限等级固定为 `1`。Role Service 自身独立反思使用 `ROLE_SELF` 来源，等级为 `2`；用户来源使用经过校验的发送者权限等级。更新已有记忆时保留原访问等级。

Rust 业务读写接口位于 `zihuan_core/src/storage/memory/`，按人物、概念和行为分别定义记录、写入参数与操作。`MemoryBackend` 只选择本地文件或检索数据库，三个类别共享底层连接和文档编解码。

| 类别 | 记录 / 写入参数 | 读取 | 写入 | 检索 |
| --- | --- | --- | --- | --- |
| 人物 | `PersonMemoryRecord` / `PersonMemoryWrite` | `get_person(qq_id)` | `save_person` | `search_people` |
| 概念 | `ConceptMemoryRecord` / `ConceptMemoryWrite` | `get_concept(key, senders, groups)` | `save_concept` | `search_concept` |
| 行为 | `BehavioralMemoryRecord` / `BehavioralMemoryWrite` | `get_behavioral(key, senders, groups)` | `save_behavioral` | `search_behavioral` |

人物记录只携带人物档案、访问等级、对象 ID 和时间。`get_person` 是精确身份读取，不依赖 embedding，也不会在没有对应账号时返回其他人物。昵称、别名和语义检索使用 `search_people`。

概念记录携带内容、作用域和事实时间，搜索按事实时间排序。行为记录携带内容、作用域和过期时间，实际检索会延长有效期；只读检索设置 `skip_expiry_extend`。

搜索工具必须显式提供 `category`。`query` 是 Rust 的 `MemoryQuery` 枚举，JSON 使用外部标签：

```json
{"category":"person","query":{"id":"916436517"}}
```

`id` id

```json
{"category":"person","query":{"query":"紫幻"}}
```

`query` 始终模糊检索，即使内容全为数字也不会切换为精确查询。人物模糊文本字段为 `qq_id`、`name`、`preferred_name`、`aliases` 和 `impression`；人物 embedding 也仅由这些字段生成。`guideline` 是返回档案的一部分，不作为新向量的检索文本。

`FullSearchConceptMemory` 同样要求显式分类和枚举查询。管理界面的跨类别浏览在内部逐类调用；Dream 使用人物分类和 `id` 查询
