# Command System

The command system provides slash-command (`/command`) support for agents in `zihuan-next`. Commands are processed by a centralized registry and can trigger side effects, interact with LLM conversations, and enforce fine-grained permissions.

## Overview

Commands are text-based interactions that start with `/`. They provide a lightweight mechanism for users to:

- Control agent behavior (e.g., start new conversations)
- Trigger side effects (e.g., update configuration, manage sessions)
- Query system state (e.g., list available commands)

The command system separates parsing, permission checking, and execution into distinct layers.

## Architecture

### Core Components

Located in `zihuan_core::command`:

| Component | Responsibility |
|-----------|----------------|
| `CommandRegistry` | Central router: registration, dispatch, permission management |
| `CommandHandler` | Trait implemented by each command handler |
| `CommandContext` | Context passed to handlers: agent type, caller ID, channel info |
| `PermissionRegistry` | Stateless evaluator for permission rules |
| `CommandSideEffect` | Trait for post-execution side effects |
| `parser` | Message parsing logic |

### Command Lifecycle

- **Input**: Raw message received from QQ Chat, HTTP Stream, or Dashboard
- **Parse**: `parser::parse_command` extracts command name, arguments, and passthrough text
- **Lookup**: Registry finds command by name or alias
- **Scope Check**: Verify command applies to current agent type
- **Permission Check**: `PermissionRegistry::check` evaluates rules against caller
- **Execute**: Handler produces `CommandResult` with reply and optional side effects
- **Side Effects**: Effects execute via `SideEffectContext` trait

## Data Types

### CommandDefinition

Defines a registered command's metadata:

```rust
pub struct CommandDefinition {
    pub name: String,
    pub aliases: Vec<String>,
    pub description: String,
    pub scope: CommandScope,
    pub accepted_arg_count: u8,
}
```

- `name`: Primary command identifier (case-insensitive)
- `aliases`: Alternative names for the same command
- `scope`: Which agent types can use this command
- `accepted_arg_count`: Number of positional arguments to consume (0 = parameterless)

### CommandScope

Controls command availability:

```rust
pub enum CommandScope {
    All,                              // Available to all agents
    QqChat,                           // Only QQ Chat agents
    HttpStream,                       // Only HTTP Stream agents
    Specific { agent_ids: Vec<String> }, // Specific agent instances
}
```

### PermissionRule

Permission rules are evaluated in order, first match wins:

```rust
pub enum PermissionRule {
    Everyone,                         // No restriction
    QqUsers { allowed_ids: Vec<String> },  // Specific QQ users
    ApiKeys { allowed_keys: Vec<String> }, // Specific API keys
    Custom { custom_type: String, allow_list: Vec<String> },
}
```

Empty rule list defaults to open access.

### CommandResult

Handlers return this struct:

```rust
pub struct CommandResult {
    pub reply: String,                // Text injected into LLM conversation
    pub side_effects: Vec<BoxedCommandSideEffect>,
    pub echo_message: Option<String>, // Optional user-visible message
    pub inject_to_llm: bool,          // Whether reply goes to LLM history
}
```

## Parsing

The parser handles command syntax:

```rust
pub struct ParsedCommand {
    pub command_name: String,         // Lowercased command name
    pub args: Vec<String>,            // Positional arguments (up to accepted_arg_count)
    pub passthrough_text: Option<String>, // Remaining text for LLM
}
```

Example parsing with `accepted_arg_count = 1`:

```
Input:  "/task abc123 analyze this"
Result: command_name = "task"
        args = ["abc123"]
        passthrough_text = Some("analyze this")
```

Parameterless commands (`accepted_arg_count = 0`) pass all remaining text to the LLM.

## Side Effects

Commands can request side effects through the `SideEffectContext` trait:

```rust
pub trait SideEffectContext: Send + Sync {
    fn command_context(&self) -> &CommandContext;
    fn start_new_conversation(&self, request: &NewConversationRequest) -> Result<()>;
}
```

Effects execute after the handler returns, allowing commands to:

- Start fresh conversation turns
- Modify session state
- Trigger external actions

## Registration

Commands register during system initialization:

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

Permissions can be updated at runtime:

```rust
registry.set_permissions("new", vec![
    PermissionRule::QqUsers { allowed_ids: vec!["12345".to_string()] }
]);
```

## Integration

Command dispatch typically happens in message processing pipelines:

1. **QQ Chat**: Incoming messages checked for `/` prefix before LLM processing
2. **HTTP Stream**: API requests may include command directives
3. **Dashboard**: Admin interface for command management

The dispatch method returns `None` for non-command messages, allowing normal LLM processing to continue.

## Error Handling

- Permission denied: Returns reply "你没有权限使用此命令。" with no side effects
- Command not found: Returns `None`, falls through to normal processing
- Handler errors: Returned as `reply` text
