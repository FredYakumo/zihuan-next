# 命令系统

命令系统为 `zihuan-next` 中的代理提供斜杠命令（`/command`）支持。命令由集中式注册表处理，可以触发副作用、与 LLM 对话交互，并强制执行细粒度的权限控制。

## 概述

命令是以 `/` 开头的基于文本的交互方式，为用户提供轻量级机制来：

- 控制代理行为（例如，开始新对话）
- 触发副作用（例如，更新配置、管理会话）
- 查询系统状态（例如，列出可用命令）

命令系统将解析、权限检查和执行分离到不同的层中。

## 架构

### 核心组件

位于 `zihuan_core::command`：

| 组件 | 职责 |
|-----------|----------------|
| `CommandRegistry` | 中央路由器：注册、调度、权限管理 |
| `CommandHandler` | 每个命令处理程序实现的 Trait |
| `CommandContext` | 传递给处理程序的上下文：代理类型、调用者 ID、通道信息 |
| `PermissionRegistry` | 无状态的权限规则评估器 |
| `CommandSideEffect` | 执行后副作用的 Trait |
| `parser` | 消息解析逻辑 |

### 命令生命周期

1. **输入**：从 QQ 聊天、HTTP 流或仪表板接收原始消息
2. **解析**：`parser::parse_command` 提取命令名称、参数和透传文本
3. **查找**：注册表按名称或别名查找命令
4. **范围检查**：验证命令是否适用于当前代理类型
5. **权限检查**：`PermissionRegistry::check` 根据调用者评估规则
6. **执行**：处理程序生成包含回复和可选副作用的 `CommandResult`
7. **副作用**：通过 `SideEffectContext` trait 执行效果

## 数据类型

### CommandDefinition

定义注册命令的元数据：

```rust
pub struct CommandDefinition {
    pub name: String,
    pub aliases: Vec<String>,
    pub description: String,
    pub scope: CommandScope,
    pub accepted_arg_count: u8,
}
```

- `name`：主命令标识符（不区分大小写）
- `aliases`：同一命令的替代名称
- `scope`：哪些代理类型可以使用此命令
- `accepted_arg_count`：要消耗的位置参数数量（0 = 无参数）

### CommandScope

控制命令的可用性：

```rust
pub enum CommandScope {
    All,                              // 对所有代理可用
    QqChat,                           // 仅 QQ 聊天代理
    HttpStream,                       // 仅 HTTP 流代理
    Specific { agent_ids: Vec<String> }, // 特定代理实例
}
```

### PermissionRule

权限规则按顺序评估，第一个匹配项生效：

```rust
pub enum PermissionRule {
    Everyone,                         // 无限制
    QqUsers { allowed_ids: Vec<String> },  // 特定 QQ 用户
    ApiKeys { allowed_keys: Vec<String> }, // 特定 API 密钥
    Custom { custom_type: String, allow_list: Vec<String> },
}
```

空规则列表默认为开放访问。

### CommandResult

处理程序返回此结构：

```rust
pub struct CommandResult {
    pub reply: String,                // 注入 LLM 对话的文本
    pub side_effects: Vec<BoxedCommandSideEffect>,
    pub echo_message: Option<String>, // 可选的用户可见消息
    pub inject_to_llm: bool,          // 回复是否进入 LLM 历史记录
}
```

## 解析

解析器处理命令语法：

```rust
pub struct ParsedCommand {
    pub command_name: String,         // 小写命令名称
    pub args: Vec<String>,            // 位置参数（最多 accepted_arg_count 个）
    pub passthrough_text: Option<String>, // 留给 LLM 的剩余文本
}
```

使用 `accepted_arg_count = 1` 的解析示例：

```
输入:  "/task abc123 analyze this"
结果: command_name = "task"
        args = ["abc123"]
        passthrough_text = Some("analyze this")
```

无参数命令（`accepted_arg_count = 0`）将所有剩余文本传递给 LLM。

## 副作用

命令可以通过 `SideEffectContext` trait 请求副作用：

```rust
pub trait SideEffectContext: Send + Sync {
    fn command_context(&self) -> &CommandContext;
    fn start_new_conversation(&self, request: &NewConversationRequest) -> Result<()>;
}
```

效果在处理程序返回后执行，允许命令：
- 开始新的对话轮次
- 修改会话状态
- 触发外部操作

## 注册

命令在系统初始化期间注册：

```rust
let mut registry = CommandRegistry::new();
registry.register(
    CommandDefinition {
        name: "new".to_string(),
        aliases: vec!["n".to_string()],
        description: "Start a new conversation".to_string(),
        scope: CommandScope::All,
        accepted_arg_count: 0,
    },
    Arc::new(NewCommandHandler),
);
```

权限可以在运行时更新：

```rust
registry.set_permissions("new", vec![
    PermissionRule::QqUsers { allowed_ids: vec!["12345".to_string()] }
]);
```

## 集成

命令调度通常发生在消息处理管道中：

1. **QQ 聊天**：传入消息在 LLM 处理前检查 `/` 前缀
2. **HTTP 流**：API 请求可能包含命令指令
3. **仪表板**：命令管理的管理界面

当消息不是命令时，调度方法返回 `None`，允许正常 LLM 处理继续。

## 错误处理

- 权限被拒绝：返回回复"你没有权限使用此命令。"，无副作用
- 命令未找到：返回 `None`，回退到正常处理
- 处理程序错误：作为 `reply` 文本返回
