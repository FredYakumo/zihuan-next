# AGENTS.md

This file provides project-level instructions for coding agents working in this repository.

## Working Style

- Keep changes focused. Do not mix unrelated refactors into feature or bug-fix work.
- Preserve existing architecture and naming unless the task requires a deliberate change.
- Prefer small, local edits over broad rewrites.
- When instructions conflict, prefer the behavior described by the current code and `document/` over older agent notes.

## Rust Style Preferences

When writing Rust in this repository, follow these style preferences unless the local module already has a stronger convention:

- Group imports in three blocks: `std`, third-party crates, then `crate`/workspace imports, separated by blank lines.
- Prefer direct, domain-specific names. Function names should describe the action or conversion being performed.
- Prefer explicit control flow over clever chaining for business logic. Use `match`, `if let`, and intermediate local variables freely.
- Keep data-loading and transformation code linear and readable. Prefer straightforward loops over dense iterator pipelines when the loop carries business meaning.
- Extract repeated parsing or conversion logic into small helpers close to the call site. Do not duplicate similar logic inline.
- Build structs with explicit named fields. Use `..Default::default()` only when the defaulted fields are intentional.
- Error messages should carry concrete business context (field names, node inputs, source values). Avoid vague failure text.
- Prefer pragmatic readability over abstraction. Do not introduce a generic helper, trait, or macro unless it removes real duplication.
- **Prefer macros for pattern elimination.** When the same structural pattern appears across multiple types or functions, prefer a `macro_rules!` macro over repeating the pattern manually. Macros are preferred over generic helpers or trait abstractions when the duplication is about code structure rather than type-level polymorphism.
- **Error handling:** Prefer the `?` operator for propagating errors. Avoid verbose `if let Err(` patterns.

## General Engineering Practices

- **Don't repeat yourself.** Reuse existing functionality whenever possible. Search before writing a new helper; do not duplicate logic.
- **Comments.** Skip comments when the code is self-explanatory. Write a comment only when the *why* is non-obvious (hidden constraint, subtle invariant, deliberate workaround). Never use ASCII-art separator comments (`// ----`, `// ====`, etc.).
- **Tests.** Do not write unit tests by default. Only add tests when the feature is complex enough to warrant them.
- **One node per file.** The graph must remain a DAG.
- **Common types** that may cause circular references go in `zihuan_core`. Otherwise, keep code and types in the package that owns the responsibility.

## Code Search

When navigating the codebase, prefer LSP-based tools over text-only grep:

- **Rust**: `rust-analyzer` MCP server.
- **TypeScript**: TypeScript MCP server.

All architecture, crate layout, and command references live under `document/`.
