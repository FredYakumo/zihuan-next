---
name: zihuan-lint
description: Lint and format zihuan-next code. Use this skill when asked to check for warnings, run clippy, format code, or fix lint issues.
---

# Linting and Formatting zihuan-next

The project uses **Clippy** for linting and **rustfmt** for formatting. There are no custom `clippy.toml` or `rustfmt.toml` configuration files — Rust's default settings apply.

## Quick checks

```bash
# Run clippy on all workspace crates with all features
cargo clippy --all-targets --all-features

# Run clippy on a specific crate
cargo clippy -p zihuan_service --all-targets

# Check formatting (dry-run — reports issues without modifying files)
cargo fmt --all -- --check

# Auto-format all code
cargo fmt --all
```

## Fixing issues

```bash
# Auto-fix clippy warnings where possible
cargo clippy --all-targets --all-features --fix --allow-dirty

# Auto-format (always run this before committing)
cargo fmt --all
```

## Frontend linting

For TypeScript/JavaScript in `webui/`:

```bash
cd webui
pnpm run lint        # if a lint script is configured in package.json
```

## Common clippy categories

| Lint group | What it catches |
|------------|-----------------|
| `clippy::all` | Default warn-by-default lints |
| `clippy::pedantic` | Stricter style lints (not enabled by default) |
| `clippy::nursery` | New lints still under development |
| `clippy::cargo` | Cargo.toml-specific lints |

## Agent tips

- **Run clippy before every commit** — `cargo clippy --all-targets --all-features` catches most issues early.
- **Fix warnings, not just errors** — warnings often indicate real bugs or style drift.
- **Run `cargo fmt --all` after making changes** — ensures consistent formatting across the workspace.
- **Clippy may produce false positives** — use `#[allow(clippy::lint_name)]` sparingly at the smallest scope possible (function or block, not module).
- **No custom config files exist** — if you need to adjust lint or format rules, add a `[lints]` section to the workspace `Cargo.toml` or create `clippy.toml` / `rustfmt.toml` at the workspace root.
