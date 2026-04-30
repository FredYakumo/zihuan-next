# Brain

This document describes the `brain` node as an agentic runtime with embedded tool subgraphs.

Brain tool subgraphs reuse the same embedded subgraph and boundary-node mechanism described in [../node/function-subgraphs.md](../node/function-subgraphs.md). This document focuses on the Brain-specific data model, tool contract, and runtime loop.

---

## Overview

The `brain` node owns a private tool set:

- Each tool owns its own embedded subgraph.
- Tool subgraphs are stored inside the parent Brain node's inline JSON config.
- Brain runs an internal agentic tool loop and returns an `output` message trace for the current run.

This replaces the old Brain execution style where each tool was exposed as a separate dynamic output port.

---

## Core Data Model

Brain tools use these definitions:

```rust
pub struct BrainToolDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub parameters: Vec<ToolParamDef>,
    pub outputs: Vec<FunctionPortDef>,
    pub subgraph: NodeGraphDefinition,
}
```

Brain also owns a shared input signature:

```rust
pub type BrainSharedInputs = Vec<FunctionPortDef>;
```

### Storage locations

| Owner | Inline key | Meaning |
|------|------------|---------|
| `brain` node | `tools_config` | Array of `BrainToolDefinition` |
| `brain` node | `shared_inputs` | Shared runtime inputs injected into every Brain tool subgraph |

The embedded tool subgraph itself uses the same `function_inputs` and `function_outputs` boundary nodes documented in [../node/function-subgraphs.md](../node/function-subgraphs.md).

---

## Runtime

`brain` no longer exposes one output port per tool. Its output is now:

- `output: Vec<OpenAIMessage>`

### Tool loop behavior

1. Read `tools_config`.
2. Read `shared_inputs`.
3. Convert each tool definition into an LLM tool schema using `parameters`.
4. Send the current conversation to the model.
5. If the model returns tool calls:
   - find the matching embedded tool subgraph by tool `name`
   - run that tool subgraph with `shared_inputs + fixed content + tool arguments`
   - package all declared outputs as a JSON object string
   - append a `tool` role message to the conversation
6. Repeat until there are no tool calls or the max iteration limit is reached.
7. Return the current-run output trace as `output`, in execution order:
   - assistant messages that request tool calls
   - each tool result message
   - the final assistant message without tool calls

This makes Brain closer to a standard agentic loop: tool execution stays internal to the Brain node instead of being modeled as external graph wiring.

---

## Tool Contracts

### Tool input contract

- Brain shared inputs are fixed by `shared_inputs`.
- Brain injects a fixed reserved input `content: String` for each tool call, sourced from the triggering assistant message `content` and defaulting to `""` when absent.
- Tool-local LLM-callable inputs are fixed by `parameters`.
- Tool subgraph boundary inputs are `shared_inputs + content + parameters`.
- Tool parameter names must not use the reserved name `content`.
- `shared_inputs` are runtime-only and are not exposed in the LLM tool schema.

### Tool result contract

- Tool outputs are fixed by `outputs`.
- Tool return content is always a JSON object string.
- A tool with zero declared outputs returns `{}`.

---

## Graph JSON Behavior

Brain tool subgraphs are recursive graph payloads stored inside Brain inline config.

### Brain node shape

```jsonc
{
  "node_type": "brain",
  "dynamic_input_ports": true,
  "output_ports": [
    { "name": "output", "data_type": { "Vec": "OpenAIMessage" } }
  ],
  "inline_values": {
    "shared_inputs": [
      { "name": "context", "data_type": "Json" },
      { "name": "sender_id", "data_type": "String" }
    ],
    "tools_config": [
      {
        "id": "tool_1",
        "name": "search",
        "description": "Search docs",
        "parameters": [
          { "name": "query", "data_type": "String", "desc": "query text" }
        ],
        "outputs": [
          { "name": "results", "data_type": "Json" }
        ],
        "subgraph": {
          "nodes": [ ... function_inputs ..., ... function_outputs ... ],
          "edges": [ ... ]
        }
      }
    ]
  }
}
```

### Refresh and auto-fix

`refresh_port_types()` and `auto_fix_graph_definition()` recurse into:

- `tools_config[*].subgraph`

They also:

- rebuild Brain shared input ports from `shared_inputs`
- keep boundary node signatures in sync
- prune edges that reference removed legacy ports
- normalize old Brain nodes back to static `output` message-trace output

---

## Brain Engine (`crates/zihuan_llm/src/agent/brain.rs`)

The `Brain` struct is the shared runtime engine that powers both `BrainNode` and higher-level handler nodes such as `QqMessageHandlerNode`. It lives in `crates/zihuan_llm/src/agent/brain.rs` and is the single canonical implementation of the LLM ↔ tool call loop.

### Architecture

```
Brain
├── llm: Arc<dyn LLMBase>          ← the language model
└── tools: Vec<Box<dyn BrainTool>> ← registered tools
```

`Brain` is decoupled from the node system — it has no knowledge of `DataValue`, ports, or graphs. Callers assemble a `Brain`, hand it a `Vec<OpenAIMessage>`, and receive back the new messages produced during that run.

### BrainTool trait

```rust
pub trait BrainTool: Send + Sync + 'static {
    /// LLM-facing function spec (name, description, JSON schema of parameters).
    fn spec(&self) -> Arc<dyn FunctionTool>;
    /// Execute the tool. `call_content` is the assistant message text for this
    /// turn (useful for sending a progress notification before doing the work).
    fn execute(&self, call_content: &str, arguments: &Value) -> String;
}
```

Implement `BrainTool` to wrap any callable resource — a function subgraph, a REST API, a Rust function — without touching the loop logic.

**Current implementations**

| Type | Location | What it does |
|---|---|---|
| `SubgraphBrainTool` | `brain_node.rs` | Executes a `BrainToolDefinition` function subgraph |
| `TavilyBrainTool` | `qq_message_handler_node.rs` | Calls the Tavily search API and sends a progress notification |

### BrainStopReason

```rust
pub enum BrainStopReason {
    Done,                          // last response had no tool calls
    TransportError(String),        // LLM returned a transport-level error string
    MaxIterationsReached,          // hit MAX_TOOL_ITERATIONS without a final message
}
```

### Brain API

```rust
// Construction
let brain = Brain::new(llm_arc);

// Builder-style tool registration (takes ownership)
let brain = brain.with_tool(MyTool { ... });

// In-place tool registration
brain.add_tool(MyTool { ... });

// Run the loop
let (output_messages, stop_reason) = brain.run(conversation_messages);
```

`Brain::run` returns all **new** messages produced during the run (assistant turns + tool results + final assistant message). The caller's input messages are not repeated in the output.

### Loop behavior

1. `sanitize_messages_for_inference` — drop dangling tool-call segments from the history before the first inference.
2. Call `llm.inference(messages, tools)`.
3. If the response content starts with a known transport-error prefix → return `TransportError`.
4. If the response has no tool calls → return `Done`.
5. For each tool call: find the matching `BrainTool` by `spec().name()`, call `execute(content, arguments)`, append a `tool` role result message.
6. Repeat from step 2 until `Done`, `TransportError`, or `MAX_TOOL_ITERATIONS` (default `25`) is reached.

### sanitize_messages_for_inference

```rust
pub fn sanitize_messages_for_inference(messages: Vec<OpenAIMessage>) -> Vec<OpenAIMessage>
```

Removes incomplete or orphaned tool-call sequences so the conversation passed to the LLM is always structurally valid:

- If an `assistant` message with `tool_calls` is followed by another `assistant+tool_calls` before all results arrived → the first segment is dropped.
- `tool` role messages whose `tool_call_id` has no matching pending call → dropped.
- A `tool_calls` segment still open at the end of history → dropped.

This guard runs once at the start of every `Brain::run` call.

### Adding a new BrainTool

1. Create a struct that holds whatever state the tool needs.
2. Implement `BrainTool`:
   - `spec()` — return an `Arc<dyn FunctionTool>` describing the tool name, description, and JSON-schema parameters.
   - `execute()` — do the work, return a JSON string (`"{\"key\": \"value\"}"`).
3. Pass the tool to `Brain::with_tool` or `Brain::add_tool` before calling `run`.

```rust
struct GreetTool;

impl BrainTool for GreetTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::new(SimpleSpec {
            name: "greet",
            description: "Say hello to someone",
            params: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Person's name" }
                },
                "required": ["name"]
            }),
        })
    }

    fn execute(&self, _call_content: &str, arguments: &Value) -> String {
        let name = arguments["name"].as_str().unwrap_or("stranger");
        serde_json::json!({ "greeting": format!("Hello, {name}!") }).to_string()
    }
}

// Usage
let (messages, reason) = Brain::new(llm)
    .with_tool(GreetTool)
    .run(conversation);
```

---

## UI Model

Brain tool subgraph editing uses the same page-stack navigation model as function subgraphs.

When editing a Brain tool subgraph, the file tab shows a breadcrumb-like bar:

- `返回`
- clickable `主图`
- current tool name

The shared subgraph editor behavior is documented in [../node/function-subgraphs.md](../node/function-subgraphs.md).
