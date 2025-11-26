

# Copilot Instructions for zihuan-next_aibot-800b

> **Documentation Principle:** Keep this file concise. Detailed descriptions are distributed in module-specific documentation.

> **Project Management:** This project uses [uv](https://github.com/astral-sh/uv) for dependency management.

---

## Quick Start

```bash
# Setup
uv sync
cp config.yaml.example config.yaml  # Edit: BOT_SERVER_URL, REDIS_*, MYSQL_*
cd docker/redis && docker-compose up -d
uv run alembic upgrade head
uv run python main.py
```

---

## Architecture

**Hybrid RAG chatbot** with event-driven design:
- **Event flow**: WebSocket → `BotAdapter` → `MessageEvent` → Platform handlers
- **Storage**: Redis (cache) + MySQL (persistent history)
- **Platforms**: QQ (primary), web, edge devices

---

## Key Conventions

### Configuration
- Pydantic-based config in `utils/config_loader.py`
- `config.SQLALCHEMY_DATABASE_URL` auto-generated from `MYSQL_*` fields
- Graceful fallbacks: Missing Redis → memory cache (dev only)

### Message Models
- All inherit from `MessageBase`: `PlainTextMessage`, `AtTargetMessage`, `ReplayMessage`
- Deserialization: `convert_message_from_json()` in `bot_adapter/models/message.py`

### Event Processing
```python
# BotAdapter dispatch pattern
self.event_process_func = {
    "private": event.process_friend_message,
    "group": event.process_group_message
}
```

### Database
- Alembic loads DB URL from `config.yaml` (see `migrations/env.py`)
- Never hardcode URLs in `alembic.ini`
- Migrations: `uv run alembic revision --autogenerate -m "msg"`

### Logging
- Priority: `LOG_LEVEL` env → `config.yaml:log_level` → `DEBUG`
- Log dir: `ZIHUAN_LOG_DIR` env → `config.yaml:loger_path` → `./logs`

---

## Module References

Detailed patterns and examples:
- [adapter.py](./copilot-instructions-adapter.md): Event loop, platform dispatch
- [event.py](./copilot-instructions-event.md): Handler implementations
- [models/](./copilot-instructions-models.md): Event and message schemas
- [utils/](./copilot-instructions-utils.md): Config and logging setup

---

## Common Tasks

**Add message type**: Subclass `MessageBase` → Update `convert_message_from_json()` → Handle in events  
**Add platform**: Extend `event_process_func` → Implement handler in `event.py`  
**Modify schema**: Edit `database/models/` → `uv run alembic revision --autogenerate -m "..."` → `uv run alembic upgrade head`
