---
name: zihuan-rust-architecture
description: Apply zihuan-next shared Rust architecture and conventions. Use when changing cross-crate types, LLM messages, configuration persistence, provider conversions, QQ adapter message rendering, or general Rust code outside a more specific domain skill.
---

# Shared Rust Architecture in zihuan-next

## Ownership

- Put types shared by crates or needed to avoid circular dependencies in `zihuan_core`.
- Keep feature-owned code and types in their owning crate; search for an existing utility before adding one.
- Keep one graph node per file and preserve the DAG structure. Use `/zihuan-node-dev` for node-specific work.
- Use `/zihuan-agent-dev` and `/zihuan-agent-tool-dev` for agent and tool changes.

## LLM messages and provider conversions

- Use `zihuan_core::llm::LLMMessage`, `MessagePart`, and `LLMMessageSessionCacheRef` for internal conversation flow. Do not introduce internal `OpenAIMessage*` types.
- Keep text and multimodal content in `MessagePart`; use `PersistedMedia` for image and video parts rather than provider-specific URL structures.
- Keep provider request/response shapes in `model_inference/src/llm_message/convert/`.
- Give every non-local `LlmApiStyle` its own top-level conversion entry file. Share private parsing helpers only when useful.
- Name new message utility nodes, IDs, files, and modules with `llm_message_*`, not `openai_message_*`.

## Configuration and application data

- Use `zihuan_core::system_config::application_data_dir()` as the root for application-owned files.
- Persist system settings by implementing `SystemConfigSection` and using `load_section` / `save_section`.
- Do not recreate path resolution, serialization, version initialization, or platform-specific data-directory behavior in feature crates.
- Graph hyperparameters are per-graph YAML values: nodes bind input ports to parameter names and receive the applied values before execution.

## QQ message boundaries

- When rendering or parsing nested QQ messages, import labels and boundary markers from `ims_bot_adapter/src/lib.rs`.
- Reuse constants such as `CURRENT_MESSAGE_LABEL`, `REPLY_START_MARKER`, and `QUOTE_CONTENT_LABEL`; never hard-code their text.

## Rust conventions

- Group imports as `std`, third-party, then crate/workspace imports, with blank lines between groups.
- Use direct domain names, `UpperCamelCase` types, `snake_case` functions/files, and lines near 120 characters or shorter.
- Propagate errors with `?`; make error messages identify the affected field, input, or source value.
- Prefer explicit, linear control flow. Avoid `else` after `return`; extract repeated parsing or conversions into small nearby helpers.
- Construct structs with named fields. Use `..Default::default()` only for intentional defaults.
- Prefer `macro_rules!` when repeated code has the same structural pattern.
- Skip self-evident comments. Document only non-obvious constraints, invariants, or workarounds; do not use ASCII-art separators.
- Add tests only when behavior is complex enough to warrant them. Use `/zihuan-test` to run or debug tests.
