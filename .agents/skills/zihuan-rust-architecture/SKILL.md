---
name: zihuan-rust-architecture
description: Apply ZiHuan Next Rust architecture and conventions. Use for cross-crate types, shared configuration, inference messages, adapters, or Rust changes outside a more specific skill.
---

# Rust Architecture

- Put types shared between workspace crates in `zihuan_core`; keep feature-owned types in their owner crate.
- Use `zihuan_core::system_config::application_data_dir()` for application data and `SystemConfigSection` with `load_section`/`save_section` for persisted settings.
- Use the internal LLM message types from `zihuan_core`; keep provider-specific request and response conversions near their inference integration.
- Reuse QQ rendering labels and boundary markers from their defining module. Do not duplicate literal protocol markers.
- Group imports as standard library, third-party, then workspace imports. Prefer `?`, named fields, direct control flow, and contextual error messages.
- Search for an existing local pattern before introducing a shared abstraction. Add focused tests for non-trivial behavior.
