# Copilot Instructions: utils/

## Purpose
Shared utilities for configuration loading, logging setup, and message storage.

---

## utils/config_loader.py

### Pydantic Configuration Model

```python
class Config(BaseModel):
    # Bot connection
    BOT_SERVER_URL: str = "ws://localhost:3001"
    BOT_SERVER_TOKEN: Optional[str] = None
    
    # Redis cache
    REDIS_HOST: str = "127.0.0.1"
    REDIS_PORT: int = 6379
    REDIS_DB: int = 0
    REDIS_PASSWORD: Optional[str] = None
    
    # MySQL persistent storage
    MYSQL_HOST: str = "127.0.0.1"
    MYSQL_PORT: int = 3306
    MYSQL_USER: str = "zihuan_user"
    MYSQL_PASSWORD: str = "your_mysql_password"
    MYSQL_DB: str = "zihuan_db"
    
    @property
    def SQLALCHEMY_DATABASE_URL(self) -> str:
        """Auto-generated from MYSQL_* fields"""
        return (
            f"mysql+pymysql://{self.MYSQL_USER}:{self.MYSQL_PASSWORD}"
            f"@{self.MYSQL_HOST}:{self.MYSQL_PORT}/{self.MYSQL_DB}"
        )
```

### Usage Pattern
```python
from utils.config_loader import config

# Access directly
print(config.BOT_SERVER_URL)
print(config.SQLALCHEMY_DATABASE_URL)  # Auto-generated property
```

### Initialization Flow
1. Looks for `config.yaml` in project root
2. If missing → Warning + uses defaults from `Config` model
3. Loads YAML into Pydantic model (validates types, applies defaults)
4. Global singleton: `config = ConfigLoader("config.yaml")`

**Key insight**: Never modify `alembic.ini` database URL. Use `config.yaml` instead (loaded dynamically in `migrations/env.py`).

---

## utils/logging_config.py

### Discovery Priority Chain

#### Log Level
1. `LOG_LEVEL` environment variable
2. `config.yaml:log_level` field
3. Default: `logging.DEBUG`

#### Log Directory
1. `ZIHUAN_LOG_DIR` environment variable
2. `LOG_DIR` / `LOGGER_PATH` / `LOGER_PATH` env variables
3. `config.yaml:loger_path` (or `logger_path`)
4. Default: `./logs` (project root)

### Features
- **Daily rotation**: Uses `TimedRotatingFileHandler`
- **Fallback handling**: If target directory fails → creates `./logs`
- **Global singleton**: Import as `from utils.logging_config import logger`

### Usage Pattern
```python
from utils.logging_config import logger

logger.debug("Detailed diagnostic info")
logger.info("Normal operation")
logger.warning("Non-critical issue")
logger.error("Operation failed")
```

### Configuration Examples

**Via environment**:
```bash
# Windows PowerShell
$env:LOG_LEVEL = "INFO"
$env:ZIHUAN_LOG_DIR = "C:\logs\zihuan"

# Linux/macOS
export LOG_LEVEL=INFO
export ZIHUAN_LOG_DIR=/var/log/zihuan
```

**Via config.yaml**:
```yaml
log_level: INFO
loger_path: /var/log/zihuan  # Note: typo "loger" is intentional (backward compat)
```

---

## utils/message_store.py

### Storage Strategy

**Dual backend**:
- **Production**: Redis (persistent, shared across processes)
- **Development fallback**: In-memory dict (single process only)

### Initialization

```python
def init_message_store(config, logger):
    """
    Call early in BotAdapter.__init__().
    Attempts Redis connection, falls back to memory on failure.
    """
    global _redis_client, _use_global_store
    
    # Check if Redis config exists
    if not all([config.REDIS_HOST, config.REDIS_PORT, config.REDIS_DB]):
        logger.warning("No Redis config. Using MEMORY Cache (NOT for production!)")
        _use_global_store = True
        return
    
    # Try connecting
    try:
        _redis_client = redis.Redis(
            host=config.REDIS_HOST,
            port=config.REDIS_PORT,
            db=config.REDIS_DB,
            password=config.REDIS_PASSWORD  # Optional
        )
        _redis_client.ping()
        logger.info("Connected to Redis")
        _use_global_store = False
    except Exception as e:
        logger.error(f"Redis connection failed: {e}")
        logger.warning("Falling back to MEMORY Cache")
        _use_global_store = True
```

### API

#### store_message(message_id, message, logger=None)
```python
# Store message (auto-detects Redis vs memory)
store_message(msg_id, json_string, logger=logger)
```

#### get_message(message_id)
```python
# Retrieve message (returns bytes from Redis, str from memory, or None)
original = get_message(reply_msg.message_id)
```

### Usage Pattern

```python
# In BotAdapter initialization
init_message_store(config, logger)  # MUST be first

# In event processing
store_message(message_id, raw_json, logger=logger)

# In handlers (e.g., processing replies)
from utils.message_store import get_message
original = get_message(reply_message_id)
if original:
    # Process original message context
```

### Production Considerations

**Redis required for**:
- Multi-process deployments
- Cross-server message sharing
- Persistence across restarts

**Memory cache acceptable for**:
- Local development
- Single-process testing
- Short-lived sessions

**Warning**: Memory cache logs `NOT suitable for production!` — ensure Redis is configured before deploying.

---

## Integration Points

- **Config**: Loaded once at startup, accessed globally via `utils.config_loader.config`
- **Logger**: Configured once at import, accessed globally via `utils.logging_config.logger`
- **Message Store**: Initialized in `BotAdapter.__init__()`, used in `adapter.py` and `event.py`
- **Database**: `config.SQLALCHEMY_DATABASE_URL` consumed by `database/db.py` and `migrations/env.py`
