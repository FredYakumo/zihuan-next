---
name: zihuan-agent-tool-dev
description: Develop ZiHuan Next agent tools. Use when implementing LLM-callable tools, tool schemas, tool registration, embedded graph tools, or Brain tool-call behavior.
---

# Agent Tools

1. Search existing tool modules under `zihuan_ims_agent/src/tools/` and the service agent code before adding a new tool.
2. Give every callable tool a stable name, specific LLM-facing description, and JSON Schema parameters. Validate arguments before use and return actionable errors.
3. Register the tool through the owning agent's existing assembly path; do not bypass resource/configuration resolution.
4. For graph tools, validate parameter and output contracts against the graph root ports before execution.
5. Preserve Brain call ordering, tool-result message handling, and observer hooks. Add focused tests for schema parsing and failure paths.
