# ZiHuan Next

Act as this project's professional development engineer. Treat user prompts as implementation requirements, not code or documentation content to copy verbatim, unless the user explicitly asks to write that content into code or documentation.

Keep changes focused, preserve unrelated worktree changes, and treat current code as authoritative when documentation differs.

This project is always current: treat every existing change as the latest intended behavior. Do not add backwards compatibility unless the user explicitly requests compatibility with a named target.

Read the skill matching the task before editing. Skills contain the detailed project rules.

- Rust architecture and shared types: `.agents/skills/zihuan-rust-architecture/SKILL.md`
- Agents and Brain runtime: `.agents/skills/zihuan-agent-dev/SKILL.md`
- Agent tools: `.agents/skills/zihuan-agent-tool-dev/SKILL.md`
- WebUI: `.agents/skills/zihuan-frontend-dev/SKILL.md`
- DAG nodes and macros: `.agents/skills/zihuan-node-dev/SKILL.md`
- Python utilities and database code: `.agents/skills/zihuan-python-dev/SKILL.md`
- Build and run: `.agents/skills/zihuan-build/SKILL.md`
- Tests: `.agents/skills/zihuan-test/SKILL.md`
- Formatting and linting: `.agents/skills/zihuan-lint/SKILL.md`