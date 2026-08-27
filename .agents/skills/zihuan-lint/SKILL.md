---
name: zihuan-lint
description: Format or lint ZiHuan Next. Use when running rustfmt, Clippy, TypeScript checks, or fixing lint and formatting issues.
---

# Lint And Format

```powershell
cargo fmt --all -- --check
cargo fmt --all
cargo clippy --all-targets --all-features
```

- Prefer `cargo clippy -p <crate> --all-targets` for fast feedback on a focused change.
- Use `#[allow(...)]` only at the smallest justified scope.
- For WebUI changes, run `pnpm run build` in `webui/`; it includes TypeScript checking.
- Format only files affected by the requested work when unrelated worktree edits are present.
