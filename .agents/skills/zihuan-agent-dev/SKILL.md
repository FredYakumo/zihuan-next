---
name: zihuan-agent-dev
description: Develop complete agents in zihuan-next. Use this skill when asked to create, configure, or modify agents — including Brain setup, LLM binding, built-in tool loading, node graph tool definitions, and agent lifecycle management.
---

# Agent Development in zihuan-next

Agents are the top-level runtime units: each agent has a name, a type, an LLM binding, a set of tools, and connection resources (MySQL, Redis, Weaviate, S3, etc.). Agents are defined in YAML configuration and loaded by `model_inference::system_config`.

## Agent types

| Type | Config struct | Purpose |
|------|--------------|---------|
| `qq_chat` | `QqChatAgentConfig` | QQ bot chat agent — handles group/friend messages, session management, steer (插嘴), reply building |
| `http_stream` | `HttpStreamAgentConfig` | HTTP SSE streaming agent — OpenAI-compatible `/chat/completions` endpoint |

Both types go through the same loading pipeline: resolve LLM config → build LLM model → resolve connections → build tools → construct `Brain`.

## Agent loading pipeline

```
AgentConfig (YAML / admin UI)
  ↓ resolve_llm_service_config()
  ↓ build_llm_model() → Arc<dyn LLMBase>
  ↓ build_inference_tool_provider() → Arc<dyn InferenceToolProvider>
  ↓ LoadedInferenceAgent { agent, model_name, llm, tools }
  ↓ spawn agent runtime (QQ chat handler or HTTP SSE handler)
  ↓ Brain::new(llm).with_tool(...).run(messages)
```

### Key entry points

| File | Purpose |
|------|---------|
| `zihuan_service/src/agent/qq_chat_agent.rs` | Spawns QQ chat agents with bot adapter, session claiming, inbox, steer |
| `zihuan_service/src/agent/http_stream_agent.rs` | Spawns HTTP stream agents with SSE endpoint |
| `zihuan_service/src/agent/inference.rs` | `LoadedInferenceAgent::load()` — resolves LLM + tools from config |
| `zihuan_agent/src/brain.rs` | `Brain` struct — tool-calling loop, observer hooks, iteration hooks |

## Brain construction

The `Brain` is the core agent runtime. It wraps an LLM and orchestrates tool-calling:

```rust
use zihuan_agent::brain::{Brain, BrainTool, BrainObserver, ToolRunDuration};

let llm: Arc<dyn LLMBase> = build_llm_model(&llm_config)?;

let mut brain = Brain::new(llm)
    .with_system_prompt("You are a helpful assistant.")
    .with_tool(web_search_tool)
    .with_tool(image_search_tool)
    .with_observer(my_observer);

let (messages, stop_reason) = brain.run(conversation_messages);
```

### Brain builder API

| Method | Purpose |
|--------|---------|
| `Brain::new(llm)` | Create with LLM |
| `.with_system_prompt(prompt)` | Set the system prompt |
| `.with_tool(tool)` | Add a single `BrainTool` |
| `.add_tool(tool)` | Add tool after construction |
| `.with_observer(observer)` | Add a `BrainObserver` for hooks |
| `.with_long_task_context(ctx)` | Enable long-running task tracking |
| `.run(messages)` | Execute tool-calling loop → `(messages, BrainStopReason)` |

### Stop reasons

| Reason | Meaning |
|--------|---------|
| `Done` | Final assistant message with no tool calls |
| `TransportError` | LLM response error |
| `MaxIterationsReached` | 25 iterations hit (set via `set_max_iterations()`) |

### Key traits

| Trait | Purpose | Location |
|-------|---------|----------|
| `BrainTool` | A tool the Brain can invoke. Wraps `FunctionTool` + execution logic | `zihuan_agent::brain` |
| `FunctionTool` | LLM-facing tool spec (name, description, parameters JSON Schema) | `zihuan_core::llm::tooling` (re-exported from `zihuan_agent`) |
| `BrainObserver` | Hooks: `on_assistant_tool_request`, `on_tool_start`, `on_tool_finish`, `on_final_assistant` | `zihuan_agent::brain` |
| `BrainIterationHook` | Pre-iteration hook for injecting messages (e.g., steer) | `zihuan_agent::brain` |
| `ToolRunDuration` | `Short` (default) or `Long` (enables task lifecycle tracking) | `zihuan_agent::brain` |

## Inference tool provider

`InferenceToolProvider` is the trait that supplies tools to an agent. Each agent type has its own implementation:

```rust
pub trait InferenceToolProvider: Send + Sync {
    /// Modify messages before inference (e.g., inject system prompt)
    fn augment_messages(&self, messages: &mut Vec<OpenAIMessage>, context: &InferenceToolContext);

    /// Build built-in (Rust) BrainTool instances
    fn build_default_tools(&self, context: &InferenceToolContext) -> Vec<Box<dyn BrainTool>>;

    /// Return node graph (subgraph) tool definitions
    fn tool_definitions(&self) -> Vec<BrainToolDefinition>;
}
```

The inference pipeline merges both tool sources:
1. `build_default_tools()` → built-in Rust tools wrapped as `BrainTool`
2. `tool_definitions()` → node graph tools wrapped via `ToolSubgraphRunner` → `ServiceSubgraphBrainTool`

## Built-in tools

Built-in tools are Rust structs implementing `BrainTool`. They are assembled in `zihuan_service/src/agent/tools/mod.rs`:

| Tool | Purpose | Required resources |
|------|---------|-------------------|
| `WebSearchBrainTool` | Tavily web search + URL extraction | `WebSearchEngineRef` |
| `GetAgentPublicInfoBrainTool` | Returns agent metadata (name, commit, repo) | None |
| `GetFunctionListBrainTool` | Returns available slash commands + tools | None |
| `GetRecentGroupMessagesBrainTool` | MySQL query for recent group messages | `MySqlConfig` |
| `GetRecentUserMessagesBrainTool` | MySQL query for recent private messages | `MySqlConfig` |
| `SearchSimilarImagesBrainTool` | Weaviate embedding-based image search | `WeaviateRef` + `EmbeddingBase` |
| `ImageUnderstandBrainTool` | Vision LLM image understanding | `LLMBase` (with vision) |
| `CurrentTimeBrainTool` | Returns current timestamp | None |
| `SendNaturalLanguageReplyBrainTool` | Natural language reply (agent → user) | `Sender` |
| `ReplyMessageBrainTool` | QQ reply to specific message | `Sender` |
| `RunResearchSubagentBrainTool` | Research subagent (sub-Brain) | `LLMBase` + `WebSearchEngineRef` |
| `RunDeepResearchSubagentBrainTool` | Deep research subagent | `LLMBase` + connections |
| `UpdateAgentStateBrainTool` | Update persistent agent state | `SessionStateRef` |
| `RememberContentBrainTool` | Store memory content | `WeaviateRef` (agent_memory) |
| `SearchMemoryContentBrainTool` | Semantic memory search | `WeaviateRef` (agent_memory) |
| `ListAvailableMemoryKeysBrainTool` | List memory keys | `WeaviateRef` (agent_memory) |
| `EditableQqAgentTool` | User-defined tool from admin UI | Variable |

## Node graph tools

Tools can also be DAG graphs. Defined in agent YAML config under `tools[]`:

```yaml
tools:
  - id: "my_tool"
    name: "my_tool_name"
    description: "What this tool does"
    enabled: true
    run_duration: Short
    tool_type:
      node_graph:
        source: workflow_set  # or file_path, inline_graph
        name: "research"       # workflow_set/ name
        parameters:
          - name: "query"
            data_type: "String"
            description: "The search query"
            required: true
        outputs:
          - name: "result"
            data_type: "String"
            description: "Search results"
```

### Tool source variants

| Variant | Source |
|---------|--------|
| `workflow_set` | Load from `workflow_set/<name>.json` |
| `file_path` | Load from arbitrary disk path |
| `inline_graph` | Graph embedded directly in config |

### Tool loading flow

```
AgentConfig.tools[]
  ↓ build_enabled_tool_definitions()
  ↓ For each enabled node graph tool:
  ↓   load_graph → sync_root_graph_io()
  ↓   validate parameters ↔ graph_inputs
  ↓   validate outputs ↔ graph_outputs
  ↓   root_graph_to_tool_subgraph() → embedded subgraph
  ↓ Vec<BrainToolDefinition>
```

## Agent configuration

Agents are defined in YAML (loaded by `model_inference::system_config`):

```yaml
agents:
  - id: "my_agent"
    name: "My Agent"
    enabled: true
    auto_start: false
    agent_type:
      qq_chat:
        bot_name: "MyBot"
        bot_id: "123456789"
        llm_ref_id: "my_llm"
        system_prompt: "You are a helpful assistant."
        # Connection refs for tools & storage
        mysql_ref_id: "my_mysql"
        weaviate_image_ref_id: "my_weaviate_image"
        weaviate_memory_ref_id: "my_weaviate_memory"
        embedding_model_ref_id: "my_embedding"
        memory_llm_ref_id: "my_llm"
        s3_ref_id: "my_s3"
        web_search_engine_ref_id: "tavily"
        # Bot adapter & messaging
        bot_server_url: "ws://..."
        bot_server_token_id: "my_token"
        max_message_length: 250
        compact_context_length: 8000
      tools:
        - id: "web_search"
          name: "web_search"
          enabled: true
          tool_type:
            node_graph:
              source: workflow_set
              name: "research"
```

Key config sections:
- `llm_ref_id` → points to an LLM in the `llm_refs` config
- `*_ref_id` fields → point to connections (MySQL, Redis, Weaviate, S3, etc.)
- `tools[]` → node graph tools assigned to this agent
- Built-in tools are loaded automatically based on available connection resources

## Agent tips

- **Agents are loaded from YAML config** — `model_inference::system_config::load_agents()` parses the config, then each agent goes through `LoadedInferenceAgent::load()`.
- **Built-in tools auto-detect available resources** — if `weaviate_image_ref_id` is set, `SearchSimilarImagesBrainTool` is automatically available. No manual registration needed.
- **Node graph tools require explicit `tools[]` entries** — each must have `enabled: true` and a valid graph source.
- **Tool validation is strict** — `validate_tool_graph_contract()` checks that tool `parameters` match graph `graph_inputs` and tool `outputs` match graph `graph_outputs`.
- **Use `BrainObserver` for tracing** — attach an observer to log tool calls, iterations, and final responses without modifying the Brain loop.
- **Long-running tools** use `ToolRunDuration::Long` + `LongTaskContext` → task lifecycle events are emitted while the Brain waits synchronously for the real result.
- **Steer (插嘴)** is handled by `BrainIterationHook::InjectUserHook` — injects new user messages into the ongoing Brain loop.
