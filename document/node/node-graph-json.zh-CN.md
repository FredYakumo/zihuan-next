# 节点图 JSON 规范

> 🌐 [English](node-graph-json.md) | 简体中文


本文档描述用于保存和加载节点图的 JSON 格式。浏览器编辑器读写此格式；运行时通过 `zihuan_graph_engine/src/registry.rs` 中的 `build_node_graph_from_definition()` 从中重建可执行的 `NodeGraph`。

---

## 运行前验证图

当前可通过以下方式验证图：

- Web 编辑器 / Web API
- `zihuan_graph_engine::graph_io` 中的 Rust 验证辅助函数

**检查内容：**

| 检查项 | 严重程度 |
|-------|----------|
| JSON 解析 / schema 错误 | error（退出码 2） |
| `node_type` 未在注册表中找到 | error |
| 节点缺少必要输入端口 | error |
| 无效边（未知节点 ID 或端口名） | error |
| 图中存在循环依赖 | error |
| JSON 中存在但已从注册表移除的端口 | warning |
| `inline_values` 键无匹配端口 | warning |
| `function` / `brain` 节点中的子图 | 递归检查 |

**Rust API**（编程使用）：

```rust
use crate::node::graph_io::{validate_graph_definition, find_cycle_node_ids, ValidationIssue};

let definition = node::load_graph_definition_from_json("my_graph.json")?;
let issues: Vec<ValidationIssue> = validate_graph_definition(&definition);
let cycles = find_cycle_node_ids(&definition); // Tarjan SCC 算法；为空则为 DAG
for issue in &issues {
    println!("[{}] {}", issue.severity, issue.message);
}
let has_errors = issues.iter().any(|i| i.severity == "error") || !cycles.is_empty();
```

---

## 根结构

```jsonc
{
  "nodes": [ /* NodeDefinition[] */ ],
  "edges": [ /* EdgeDefinition[] */ ],
  "hyperparameters": [ /* HyperParameter[] */ ],  // 可选
  "variables": [ /* GraphVariable[] */ ],          // 可选
  "metadata": {                                    // 可选
    "name":        "我的工作流",
    "description": "描述此图的功能。",
    "version":     "1.0.0"
  }
}
```

`execution_results` 在内存中用于 UI 显示，但**不会**写入磁盘。

---

## GraphMetadata（节点图元数据）

可通过 **Zihuan Next → 编辑节点图信息** 菜单项进行编辑。

| 字段 | 类型 | 说明 |
|---|---|---|
| `name` | `string \| null` | 人类可读的显示名称（可与文件名不同）。 |
| `description` | `string \| null` | 描述该图功能的自由文本。 |
| `version` | `string \| null` | Semver 风格的版本字符串，如 `"1.0.0"`。 |

所有字段均为可选，默认为 `null`。在浏览工作流集（`workflow_set/`）时，卡片会展示 `name`、`description`、`version`、文件名，以及封面图片（若存在）。

---

## NodeDefinition

```jsonc
{
  "id":           "node_1",
  "name":         "Format String",
  "description":  "可选的工具提示",
  "node_type":    "format_string",        // 必须匹配已注册的 type_id
  "input_ports":  [ /* Port[] */ ],
  "output_ports": [ /* Port[] */ ],
  "dynamic_input_ports":  false,          // 可选；默认 false
  "dynamic_output_ports": false,          // 可选；默认 false
  "position":     { "x": 40.0, "y": 40.0 },
  "size":         { "width": 200.0, "height": 120.0 },  // null = 自动计算尺寸
  "inline_values": {
    "template": "Hello ${name}"           // 端口名 → JSON 值
  },
  "port_bindings": {
    "text": { "kind": "variable", "name": "api_key" }
  },
  "has_error":    false                   // 运行时标志，可省略/忽略
}
```

| 字段 | 必填 | 说明 |
|-------|----------|-------|
| `id` | 是 | 图内唯一。约定：`node_1`、`node_2`、... |
| `name` | 是 | GUI 中节点卡片上显示的标签 |
| `description` | 否 | 工具提示文本 |
| `node_type` | 是 | 必须是 `NODE_REGISTRY` 中已注册的 `type_id` |
| `input_ports` | 是 | 输入端口对象的有序列表 |
| `output_ports` | 是 | 输出端口对象的有序列表 |
| `dynamic_input_ports` | 否 | `true` = 输入端口由配置驱动；跳过此方向的自动修复和兼容性检查 |
| `dynamic_output_ports` | 否 | 输出方向同上 |
| `position` | 否 | 画布空间中的左上角位置。省略时 GUI 在加载时自动布局 |
| `size` | 否 | `null` 或省略 = 根据端口数量自动计算 |
| `inline_values` | 否 | 输入端口的默认值；键为端口名 |
| `port_bindings` | 否 | 输入端口绑定元数据。旧版字符串值仍作为超参数绑定加载 |
| `has_error` | 否 | 由运行时在执行失败时设置；加载时忽略 |

---

## Port

```jsonc
{
  "name":        "template",
  "data_type":   "String",
  "description": "含 ${variable} 占位符的格式模板",
  "required":    true
}
```

| 字段 | 必填 | 说明 |
|-------|----------|-------|
| `name` | 是 | 在节点的输入或输出端口列表中唯一。使用 `snake_case`。 |
| `data_type` | 是 | 参见下方[数据类型](#数据类型) |
| `description` | 否 | 在 GUI 中显示为工具提示 |
| `required` | 是 | 仅对输入端口有意义。若为 `true`，此端口没有传入边且无 `inline_values` 条目时执行失败 |

---

## EdgeDefinition

```jsonc
{
  "from_node_id": "node_1",
  "from_port":    "output",
  "to_node_id":   "node_2",
  "to_port":      "text"
}
```

**运行时强制执行的验证规则：**
- 两个节点必须都存在于图中
- `from_port` 必须是源节点的输出端口
- `to_port` 必须是目标节点的输入端口
- 端口数据类型必须兼容
- 每个输入端口最多接收**一条**传入边
- 图必须是 **DAG**（无环）

> **传统模式：** 当 `edges` 为空数组时，引擎回退到隐式名称匹配：输出端口 `"foo"` 自动向任意其他节点上名为 `"foo"` 的输入端口供数据。新图请勿使用此模式。

---

## HyperParameter

超参数是图级变量，可绑定到输入端口，并在不编辑图的情况下在运行时覆盖：

```jsonc
{
  "name":        "api_key",
  "group":       "default",
  "data_type":   "Password",
  "description": "OpenAI API 密钥",
  "required":    true
}
```

| 字段 | 必填 | 说明 |
|-------|----------|-------|
| `name` | 是 | 图内唯一名称 |
| `group` | 否 | 共享存储组。默认为 `"default"` |
| `data_type` | 是 | 与端口相同的数据类型规则 |
| `description` | 否 | UI 提示文本 |
| `required` | 否 | 是否在没有值时阻止执行 |

超参数的*值*存储在共享的本地 YAML 文件中，而非图 JSON 里。  
图通过 `(group, name)` 复用值，因此重命名或移动图文件不会破坏值的查找。

---

## GraphVariable

变量是图级的运行时作用域状态，带有 JSON 定义的初始值：

```jsonc
{
  "name": "counter",
  "data_type": "Integer",
  "initial_value": 0
}
```

| 字段 | 必填 | 说明 |
|-------|----------|-------|
| `name` | 是 | 图内唯一名称 |
| `data_type` | 是 | 当前 UI 支持 String / Integer / Float / Boolean / Password |
| `initial_value` | 否 | 初始运行时值。每次图运行都会将变量重置为此值 |

---

## 数据类型

`data_type` 字段是一个字符串或 JSON 对象，对应 `zihuan_graph_engine/src/data_value.rs` 中的 `DataType` Rust 枚举。

### 基本类型

| JSON 值 | Rust 变体 | 内联值格式 |
|-----------|-------------|---------------------|
| `"String"` | `DataType::String` | `"hello"` |
| `"Integer"` | `DataType::Integer` | `42` |
| `"Float"` | `DataType::Float` | `3.14` |
| `"Boolean"` | `DataType::Boolean` | `true` / `false` |
| `"Json"` | `DataType::Json` | 任意 JSON 值 |
| `"Binary"` | `DataType::Binary` | *（不可内联编辑）* |
| `"Password"` | `DataType::Password` | `"secret"`（UI 中掩码显示） |
| `"Any"` | `DataType::Any` | 任意值 |

### Vec（同类列表）

序列化为带 `"Vec"` 键的 JSON 对象：

```json
{ "Vec": "OpenAIMessage" }
{ "Vec": "String" }
{ "Vec": "QQMessage" }
```

### 领域类型

| JSON 值 | 描述 |
|-----------|-------------|
| `"MessageEvent"` | 机器人平台消息事件 |
| `"OpenAIMessage"` | LLM 聊天消息 `{role, content, tool_calls}` |
| `"QQMessage"` | QQ 平台消息段 |
| `"FunctionTools"` | LLM 函数调用工具定义 |
| `"BotAdapterRef"` | 共享机器人 WebSocket 连接 |
| `"S3Ref"` | 对象存储配置 + 上传客户端 |
| `"RedisRef"` | Redis 配置 + 连接管理器 |
| `"MySqlRef"` | MySQL 配置 + 连接池 |
| `"OpenAIMessageSessionCacheRef"` | 每发送者消息历史缓存 |
| `"LLModel"` | 语言模型配置 |
| `"LoopControlRef"` | 循环中断信号 |

### 向后兼容别名

反序列化器接受这些旧名称并默默转换：

| 旧名称 | 解析为 |
|---------|-------------|
| `"Message"` | `"OpenAIMessage"` |
| `"MessageList"` | `{"Vec": "OpenAIMessage"}` |
| `"QQMessageList"` | `{"Vec": "QQMessage"}` |
| `"Vec<OpenAIMessage>"`（显示字符串） | `{"Vec": "OpenAIMessage"}` |

---

## 完整示例

三节点流水线：**Bot 适配器 → 提取消息 → 预览**

```json
{
  "nodes": [
    {
      "id": "node_1",
      "name": "QQ Bot Adapter",
      "description": "从 QQ 服务器接收消息",
      "node_type": "ims_bot_adapter",
      "input_ports": [
        { "name": "qq_id",            "data_type": "String", "required": true  },
        { "name": "bot_server_url",   "data_type": "String", "required": true  },
        { "name": "bot_server_token", "data_type": "Password", "required": false }
      ],
      "output_ports": [
        { "name": "message_event", "data_type": "MessageEvent", "required": true }
      ],
      "position": { "x": 40.0, "y": 40.0 },
      "inline_values": {
        "qq_id": "123456789",
        "bot_server_url": "ws://localhost:3001"
      }
    },
    {
      "id": "node_2",
      "name": "Extract Message",
      "node_type": "extract_message_from_event",
      "input_ports": [
        { "name": "message_event", "data_type": "MessageEvent", "required": true }
      ],
      "output_ports": [
        { "name": "message", "data_type": { "Vec": "QQMessage" }, "required": true }
      ],
      "position": { "x": 300.0, "y": 40.0 }
    },
    {
      "id": "node_3",
      "name": "Preview",
      "node_type": "preview_string",
      "input_ports": [
        { "name": "text", "data_type": "String", "required": false }
      ],
      "output_ports": [
        { "name": "text", "data_type": "String", "required": true }
      ],
      "position": { "x": 560.0, "y": 40.0 }
    }
  ],
  "edges": [
    { "from_node_id": "node_1", "from_port": "message_event", "to_node_id": "node_2", "to_port": "message_event" },
    { "from_node_id": "node_2", "from_port": "message",       "to_node_id": "node_3", "to_port": "text" }
  ]
}
```

数据流：

```
[ims_bot_adapter] --message_event--> [extract_message_from_event] --message--> [preview_string]
```

---

## 参见

- [节点开发指南](./node-development.zh-CN.md) — 创建和注册节点类型
- [动态端口节点指南](./dynamic-port-nodes.md) — 配置驱动的端口列表
- [node-system.md](../dev-guides/node-system.md) — 执行引擎和所有内置节点类型
