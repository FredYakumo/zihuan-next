---
name: zihuan-agent-dev
description: Develop ZiHuan Next agents and Brain runtime behavior. Use for agent configuration, inference loading, Brain wiring, agent lifecycle, system prompts, and graph-backed tool definitions.
---

# Agent Development

- Start at `zihuan_service/src/agent/` for service loading and `zihuan_core/src/agent/` for shared runtime definitions; trace the actual config loader before changing configuration.
- Keep the load path explicit: resolve model and connection references, construct the runtime, load enabled tools, then start the transport-specific handler.
- Build Brain behavior through its existing builder and observer APIs. Preserve stop-reason and task-lifecycle handling.
- Treat YAML/admin configuration as the source of agent definition. Validate referenced IDs and keep new fields serializable and backwards-compatible unless a breaking change is intentional.
- For graph-backed tools, keep declared parameters and outputs synchronized with the graph's root inputs and outputs.
- Use `zihuan-agent-tool-dev` for an individual tool implementation and `zihuan-test` for verification.
