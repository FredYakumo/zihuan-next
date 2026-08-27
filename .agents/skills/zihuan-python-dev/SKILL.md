---
name: zihuan-python-dev
description: Develop ZiHuan Next Python utilities and database code. Use for files in database/ or utils/, Python dependencies, virtual environments, formatting, and Python tests.
---

# Python Development

- Keep Python code in `database/` or `utils/` unless an existing package establishes another location.
- Manage the environment and dependencies with `uv`; use the repository `.venv` when available.
- Follow `pyproject.toml`, use type hints where they clarify boundaries, and keep error handling contextual.
- Format and lint with `ruff` when it is installed for the environment. Run the narrowest relevant test or script after edits.
