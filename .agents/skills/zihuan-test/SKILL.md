---
name: zihuan-test
description: Test or debug ZiHuan Next. Use when selecting tests, running Rust tests, diagnosing failures, or adding regression coverage.
---

# Testing

```powershell
cargo test
cargo test -p zihuan_service
cargo test -p zihuan_service <test_name>
cargo test -- --nocapture
```

- Start with the narrow crate or named test that covers the changed behavior; use workspace-wide tests for shared contracts.
- Add tests beside the Rust unit under test or in that crate's integration-test directory.
- Include success and failure paths for parsing, configuration, and protocol boundary changes.
- For frontend edits, run `pnpm run build` in `webui/`; add behavioral coverage only where the existing test setup supports it.
