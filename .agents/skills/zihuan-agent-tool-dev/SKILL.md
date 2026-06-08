---
name: zihuan-agent-tool-dev
description: Develop tools and agent capabilities for the zihuan-next agent system. Use this skill when asked to create new tools for the QQ chat agent, implement FunctionTool traits, define embedded subgraph tools, or work with the Brain tool-calling loop.
---

# Agent Tool Development in zihuan-next

Tools are the mechanism by which agents interact with external systems. Each tool implements the `FunctionTool` trait and can be invoked by the Brain engine during a tool-calling loop. Tools can be standalone Rust functions or embedded DAG subgraphs.

## Tool architecture

```
User message → QQ Chat Agent
  → Brain::run() tool-calling loop (max 25 iterations)
    → LLM decides to call a tool
      → Find matching tool by name
        → tool.execute(call_content, arguments) → String result
          → Append tool_result to conversation
            → Continue loop or return final assistant message
```

## Creating a standalone tool

1. Create a new file in `zihuan_agent/src/tools/` (e.g., `my_tool.rs`)
2. Implement the `FunctionTool` trait
3. Register the tool in `zihuan_agent/src/tools/mod.rs`

```rust
use zihuan_agent::FunctionTool;

pub struct MyTool;

#[async_trait::async_trait]
impl FunctionTool for MyTool {
    fn name(&self) -> &str {
        "my_tool_name"
    }

    fn description(&self) -> &str {
        "Description shown to the LLM for tool selection"
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query"
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(
        &self,
        call_content: Option<String>,
        arguments: serde_json::Value,
    ) -> Result<String, String> {
        let query = arguments["query"].as_str().unwrap_or_default();
        // ... tool logic ...
        Ok("result string".to_string())
    }
}
```

## Existing default tools

| Tool | File | Purpose |
|------|------|---------|
| `web_search` | `zihuan_agent/src/tools/web_search.rs` | Tavily API web search and URL extraction |
| `get_agent_public_info` | `zihuan_agent/src/tools/info_tools.rs` | Returns agent name, git commit, repo (prevents prompt disclosure) |
| `get_function_list` | `zihuan_agent/src/tools/info_tools.rs` | Returns available functions and commands |
| `get_recent_group_messages` | `zihuan_agent/src/tools/recent_messages.rs` | MySQL query for recent group chat messages |
| `get_recent_user_messages` | `zihuan_agent/src/tools/recent_messages.rs` | MySQL query for recent private messages |
| `search_similar_images` | `zihuan_agent/src/tools/image_search.rs` | Weaviate embedding-based image similarity search |

## Embedded subgraph tools

Tools can also be DAG graphs loaded from files or workflow templates. Configure via `tool_definitions.rs`:

| Variant | Source |
|---------|--------|
| `FilePath` | Load graph from disk path |
| `WorkflowSet` | Load from `workflow_set/` directory |
| `InlineGraph` | Graph embedded directly in config |

### Validation requirements

When defining a subgraph tool:
- The graph must have declared **inputs and outputs**
- Tool **parameters** must match graph **inputs**
- Tool **outputs** must match graph **outputs**

## Brain observer pattern

Use `BrainObserver` to hook into the tool execution lifecycle without modifying the core loop:

```rust
trait BrainObserver {
    fn on_assistant_tool_request(&self, iteration: usize, content: &str, tool_calls: &[ToolCall]);
    fn on_tool_start(&self, tool_name: &str, call_id: &str, arguments: &Value);
    fn on_tool_finish(&self, tool_name: &str, call_id: &str, result: &str);
    fn on_final_assistant(&self, response: &str, stop_reason: StopReason);
}
```

| Hook | When it fires |
|------|---------------|
| `on_assistant_tool_request` | LLM requests tool calls this iteration |
| `on_tool_start` | Tool execution begins |
| `on_tool_finish` | Tool execution completes |
| `on_final_assistant` | Brain loop ends (Done / TransportError / MaxIterationsReached) |

## Stop reasons

| Reason | Meaning |
|--------|---------|
| `Done` | Final assistant message with no tool calls |
| `TransportError` | LLM response error detected |
| `MaxIterationsReached` | 25 iterations hit (configurable via `MAX_TOOL_ITERATIONS`) |

## Agent tips

- **Tools execute sequentially** within a single Brain iteration — one tool at a time.
- **Tool results are appended as `tool` role messages** in the OpenAI conversation format.
- **Validate graph I/O** for embedded subgraph tools — mismatched ports cause runtime failures.
- **Parameter schemas use JSON Schema** format — same as OpenAI function calling format.
- **Set reasonable `max_iterations`** — the default of 25 prevents infinite loops but may be too high for simple tools.
- **Search existing tools before creating new ones** — the 6 default tools cover web search, info disclosure, message history, and image search.
- **Tool descriptions matter** — the LLM uses them to decide which tool to call; be specific about when and why to use each tool.
