# 动态节点开发参考

本文档说明 ZiHuan Next 动态脚本节点的开发规范与接口定义。接口定义以 `dynamic_script_engine/zihuan_sdk.d.ts` (TypeScript 定义) 或对应语言的 SDK 接口为准。节点通过 SDK 提供的 `zihuan` 对象与 Rust 引擎交互，底层通信由引擎实现，节点开发者无需关心。

## 节点文件结构

动态脚本节点文件位于 `dag_nodes/` 目录下。文件扩展名取决于所使用的脚本语言（如 `.mjs`、`.py`）。每个文件需要提供一个节点定义列表，具体导出方式由语言 SDK 决定。

## 节点定义字段

| 字段 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `type_id` | string | 是 | 节点类型的唯一标识，用于注册、序列化和运行时查找 |
| `display_name` | string | 是 | 在 WebUI 节点面板中显示的名称 |
| `category` | string | 是 | 节点在节点面板中的分类 |
| `description` | string | 否 | 节点功能的简要描述 |
| `input_ports` | port[] | 是 | 静态输入端口定义 |
| `output_ports` | port[] | 是 | 静态输出端口定义 |
| `dynamic_input_ports` | boolean | 否 | 标记是否使用动态输入端口（默认 false） |
| `dynamic_output_ports` | boolean | 否 | 标记是否使用动态输出端口（默认 false） |
| `resolve_ports` | function | 否 | 根据 inline_values 动态计算端口的函数 |
| `ui` | object | 否 | 节点 UI 元数据，包含 template_path 等字段 |
| `execute` | async function | 是 | 节点执行逻辑函数 |

## 端口定义

端口通过 SDK 提供的端口声明函数（如 `port()`）创建。具体调用方式取决于语言实现，但接口协议一致。

端口函数接收以下参数：

### port() 参数

| 参数 | 类型 | 说明 |
| --- | --- | --- |
| `name` | string | 端口名称，必须唯一 |
| `type` | string \| object | SDK 数据类型，如 "String"、"Number" 或复合类型如 { Vec: "LLMMessage" } |
| `options` | object | 可选配置对象 |

### options 字段

| 字段 | 类型 | 默认值 | 说明 |
| --- | --- | --- | --- |
| `required` | boolean | true | 是否必须连接输入 |
| `description` | string | - | 端口描述信息 |
| `hidden` | boolean | false | 是否隐藏端口 |

### 支持的数据类型

#### 基础类型
- `String` — 字符串
- `Number` — 数字
- `Boolean` — 布尔值
- `Any` — 任意类型

#### 复合类型
- `{ Vec: "TypeName" }` — 指定类型的数组
- `{ Map: "TypeName" }` — 指定类型的映射
- `{ Option: "TypeName" }` — 可选的指定类型

#### 资源类型
- `RdbRef` — 数据库连接引用
- `S3Ref` — S3 存储引用
- `LLModel` — 语言模型引用
- `EmbeddingModel` — 嵌入模型引用
- `LLMMessage` — LLM 消息对象
- `QQMessage` — QQ 消息对象

## resolve_ports 函数

用于动态计算端口定义的函数，根据节点的 inline_values 返回要添加的端口。

函数签名（伪代码表示）：
```
resolve_ports(inline_values) -> { input_ports?, output_ports? }
```

### 返回值字段

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `input_ports` | port[] | 要添加的输入端口（可选） |
| `output_ports` | port[] | 要添加的输出端口（可选） |

### 约束条件

- 该函数应只依赖 `inline_values` 参数
- 不能依赖外部状态或执行上下文
- 返回的端口会与静态端口合并

## execute 函数

节点的核心执行逻辑，接收执行上下文并返回输出结果。

函数签名（伪代码表示）：
```
async execute(context) -> { output_port_name: value, ... }
```

execute 必须是异步函数，支持 await 等待 SDK 调用结果。

### 执行上下文字段

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `nodeId` | string | 当前图实例中的节点 ID |
| `nodeName` | string | 当前节点的名称 |
| `inputs` | object | 按输入端口名组织的输入数据对象 |
| `inline_values` | object | 节点的配置值，从编辑界面设置的参数 |
| `zihuan` | object | Rust 宿主提供的 SDK 对象 |

### 返回值

返回一个对象，键为输出端口名称，值为对应的数据。

## SDK (zihuan 对象)

`zihuan` 对象提供了与 Rust 引擎交互的各种能力。所有 SDK 调用都是异步的，需要使用对应的异步等待机制（如 JavaScript 的 await、Python 的 await）。

### 命名空间概览

| 命名空间 | 功能 |
| --- | --- |
| `zihuan.variables` | 读写当前图运行期变量 |
| `zihuan.task` | 写入任务进度 |
| `zihuan.session` | 访问会话状态 |
| `zihuan.messageCache` | 操作消息缓存资源 |
| `zihuan.models` | 调用语言模型 |
| `zihuan.embeddings` | 使用嵌入模型 |
| `zihuan.storage` | 创建和使用存储资源 |
| `zihuan.search` | 执行向量检索 |
| `zihuan.agent` | 访问当前 Agent 资源 |
| `zihuan.bot` | 操作机器人适配器 |
| `zihuan.resources` | 校验和包装资源句柄 |
| `zihuan.ui` | 向 WebUI 发布状态并等待事件 |

### 资源句柄约束

资源类参数（RdbRef、S3Ref、LLModel 等）是 Rust 管理的不可变句柄，有如下约束：

- 只能将 SDK 返回的句柄传入接受同类型参数的 SDK 方法
- 不能自行构造或解析句柄的内部结构
- 不能跨运行保存句柄，它们只在当前执行上下文中有效
- 不能访问句柄的底层属性或方法

### 调用约定

- 所有 SDK 方法调用都是异步的，需使用语言对应的异步等待机制
- 参数必须是 JSON 可序列化的对象
- 返回值是 JSON 可序列化的对象

## UI 交互

### 发布状态

`zihuan.ui.publish(state)` — 发布完整状态到 WebUI

`zihuan.ui.update(patch)` — 更新状态（注意：当前实现不会在服务端合并 patch，需要传递完整状态）

### 状态对象结构

状态对象可以是任意 JSON 对象，常用字段：

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `status` | string | 节点状态，如 "running"、"waiting_confirm" |
| `progress` | number | 进度百分比（0-100） |
| 其他自定义字段 | any | 根据节点需要定义 |

### 等待用户事件

`zihuan.ui.waitEvent(eventName?, timeoutMs?)` — 等待用户交互事件

#### 参数

| 参数 | 类型 | 默认值 | 说明 |
| --- | --- | --- | --- |
| `eventName` | string | undefined | 事件名称，不指定则接收队列中第一个事件 |
| `timeoutMs` | number | 30000 | 超时时间（毫秒），最大值为 300000 |

#### 返回值

- 成功：返回事件 payload（JSON 对象）
- 超时：返回 `null`

#### 行为特性

- 事件队列按 `(task_id, node_id)` 隔离
- 队列最多保留 64 个事件，满时丢弃最早事件
- 任务清理时会按 task ID 清除事件队列
- 当前 renderer 将可点击元素的 `value` 作为 payload

## HTML 模板自定义 UI

### ui 对象字段

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `template_path` | string | 模板文件路径（相对于节点脚本文件），支持 `.html`、`.css`、`.scss` |

### 模板路径规范

- 路径相对于节点脚本文件所在目录
- 支持的扩展名：`.html`、`.css`、`.scss`
- 不能是绝对路径
- 不能包含 `..`（上级目录引用）
- 解析后的真实路径必须在节点脚本文件所在目录内
- 单个模板文件最大 512 KiB

### 数据绑定属性

| 属性 | 语法 | 行为 |
| --- | --- | --- |
| `data-bind-text` | `data-bind-text="path"` | 将状态值作为纯文本显示到元素中 |
| `data-bind-attr` | `data-bind-attr="attr:path"` | 将状态值设置到指定的 DOM 属性 |
| `data-bind-if` | `data-bind-if="path"` | 根据状态值的真假设置元素的 `hidden` 属性 |
| `data-bind-each` | `data-bind-each="path"` | 循环渲染数组中的每一项 |

### 路径语法

- 使用点号分隔对象字段：`user.name`、`items.0.title`
- 不支持脚本语言表达式
- 不支持函数调用
- 不支持插值语法
- 不支持动态属性名

### 事件绑定属性

| 属性 | 语法 | 行为 |
| --- | --- | --- |
| `data-ui-event` | `data-ui-event="eventName"` | 为元素绑定点击事件，向执行中的节点发送指定事件 |

### 安全边界

模板内容会经过多层安全清洗：

#### Rust 端清洗
- 移除 `<script>`、`<iframe>`、`<object>`、`<embed>` 标签
- 移除 `on*` 事件属性
- 移除 `javascript:` 协议的属性值

#### 前端解析时清洗
- 再次移除危险标签
- 再次移除事件属性和危险协议

#### 当前限制
- 尚未实现完整的标签白名单
- 尚未实现完整的属性白名单
- 尚未实现外链 URL 白名单
- 尚未实现 CSS 白名单
- 尚未支持 `ui.card`/`ui.panel` 等声明式组件

#### 安全建议
- 模板文件应与节点脚本放在同一目录下，来自受信任的代码仓库
- `.html` 模板中不要使用内联样式、远程资源或依赖浏览器脚本
- `.css`、`.scss` 样式文件应仅包含样式定义，不包含脚本代码
- 节点逻辑必须在 `execute` 函数中实现，不能在模板中编写

## 图持久化与状态管理

### inline_values（配置状态）

节点配置值，在编辑界面设置。

- 随图 JSON 保存和加载
- 可以被撤销/重做
- 用于 `resolve_ports` 动态计算端口
- 适合存储编辑态配置参数

### ui_state（界面状态）

节点卡片的界面状态。

- 通过节点更新 API 读写
- 随图 JSON 保存
- 不影响节点执行逻辑
- 适合存储界面布局状态

### execute 中的状态（运行期状态）

通过 `zihuan.ui.publish` 发布的状态。

- 只在任务执行期间存在
- 不回写到图 JSON
- 任务结束后自动清理
- 适合存储执行时的临时状态

### template_path（类型元数据）

节点类型的模板路径定义。

- 不写入图实例
- 是节点类型的元数据
- 所有同类型节点共享

## WebSocket 通信协议

### NodeUiUpdate 消息

节点向 WebUI 广播状态时产生的 WebSocket 消息。

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `task_id` | string | 任务 ID |
| `graph_session_id` | string | 图会话 ID |
| `node_id` | string | 节点 ID |
| `revision` | number | 单调递增的版本号 |
| `state` | object | 节点状态数据 |

### NodeUiEvent 消息

WebUI 向节点发送的用户交互事件。

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `task_id` | string | 任务 ID |
| `node_id` | string | 节点 ID |
| `event_name` | string | 事件名称 |
| `payload` | any | 事件载荷 |

## 错误处理

### 模板加载错误

模板不存在或校验失败时：
- 不会阻止节点类型注册
- registry 返回 `ui_template_error`
- 当前 WebUI 尚未显示专用错误面板
- 需通过 registry 响应或服务日志排查

### SDK 调用错误

- SDK 方法调用失败会抛出异常
- 应在 `execute` 函数中适当捕获和处理异常
- 异常会导致节点执行失败

### 端口验证错误

- 输入端口缺少必需连接会导致执行失败
- 输出数据类型与端口声明不匹配会引发警告
