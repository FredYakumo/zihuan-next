---
name: zihuan-python-dev
description: Develop Python utilities and database code in zihuan-next. Use when changing files under database/ or utils/, Python dependencies, formatting, or virtual environments.
---

# Python Development in zihuan-next

- Python code belongs in `database/` and `utils/`.
- Use `uv` for dependencies and environments:

```powershell
uv venv
.\.venv\Scripts\Activate.ps1
uv pip install -e .
```

- Follow PEP 8 with lines near 120 characters or shorter.
- Use `ruff` for formatting and linting, following the repository `pyproject.toml` configuration.
- Add type hints where they improve clarity; they are encouraged, not mandatory.
