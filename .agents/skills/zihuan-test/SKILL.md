---
name: zihuan-test
description: Run zihuan-next tests. Use this skill when asked to run tests, debug test failures, or find and execute specific test cases in the project.
---

# Running zihuan-next Tests

The project uses **Rust's built-in test framework** (`#[test]`). Tests are organized as:
- Inline unit tests within source files (`#[cfg(test)] mod tests`)
- Integration tests in `tests/` directories at crate roots

## Running tests

```bash
# Run all tests across the entire workspace
cargo test

# Run tests for a specific crate
cargo test -p zihuan_service
cargo test -p node_macros
cargo test -p zihuan_agent
cargo test -p ims_bot_adapter

# Run a specific test by name
cargo test -p zihuan_service test_name

# Run tests with stdout/stderr output visible
cargo test -- --nocapture

# Run tests, showing output and not capturing test results
cargo test -- --show-output

# Run ignored tests
cargo test -- --ignored
```

## Test file locations

| Crate | Integration tests |
|-------|-------------------|
| `zihuan_service` | `zihuan_service/tests/` — QQ agent tool inputs, image understanding, deep search workflow |
| `node_macros` | `node_macros/tests/` — flow and node macros |
| `ims_bot_adapter` | `ims_bot_adapter/tests/` — adapter and message tests |

## Test conventions

- Test functions are annotated with `#[test]`
- Naming convention: `test_<what>_<expected_behavior>` (e.g., `test_tool_input_expands_reply_source_images_into_top_level_message_list`)
- Integration tests live in `tests/` directories at the crate root
- Unit tests use `#[cfg(test)] mod tests { ... }` within source files

```rust
#[test]
fn tool_input_expands_reply_source_images_into_top_level_message_list() {
    let event = build_reply_image_event();
    let expanded = expand_message_event_for_tool_input(&event);
    assert!(/* ... */);
}
```

## Agent tips

- **Always run from the workspace root** — `cargo test` with `-p <crate>` will resolve correctly from there.
- **Use `-p <crate>` for fast feedback** — avoids compiling and testing unrelated crates.
- **Use `-- --nocapture`** when debugging to see `println!` / `dbg!` output.
- **Redirect output to a file** for large test suites: `cargo test > test_output.txt 2>&1`.
- **Integration tests in `zihuan_service/tests/`** test QQ chat agent tool inputs and workflow definitions — these are the most domain-relevant tests.
- **Tests in `node_macros/tests/`** validate the `node_input!` / `node_output!` proc macros — run these when changing port definitions or the macro crate.
