import redis

# Global fallback dict for message storage if Redis is unavailable
_GLOBAL_MESSAGE_STORE = {}
_redis_client = None
_use_global_store = True


def init_message_store(config, logger):
    global _redis_client, _use_global_store
    redis_config_missing = not all([
        getattr(config, "REDIS_HOST", None),
        getattr(config, "REDIS_PORT", None),
        getattr(config, "REDIS_DB", None)
    ])
    if redis_config_missing:
        _redis_client = None
        _use_global_store = True
        logger.warning("No Redis connection info provided. Using MEMORY Cache to store messages. This is NOT suitable for production!")
    else:
        redis_kwargs = {
            "host": config.REDIS_HOST,
            "port": config.REDIS_PORT,
            "db": config.REDIS_DB
        }
        if getattr(config, "REDIS_PASSWORD", None):
            redis_kwargs["password"] = config.REDIS_PASSWORD
        try:
            _redis_client = redis.Redis(**redis_kwargs)
            _redis_client.ping()
            logger.info("Connect to Redis.")
            logger.info("Connect to Redis succeeded.")
            _use_global_store = False
        except Exception as e:
            logger.error(f"Failed to connect to Redis: {e}")
            _redis_client = None
            _use_global_store = True
            logger.warning("Falling back to MEMORY Cache for message storage.")


def store_message(message_id, message, logger=None):
    if not message_id:
        if logger:
            logger.warning("No message_id provided, cannot store message.")
        return
    if not _use_global_store and _redis_client:
        try:
            _redis_client.set(message_id, message)
            if logger:
                logger.debug(f"Message stored in Redis with key: {message_id}")
        except Exception as re:
            if logger:
                logger.error(f"Failed to store message in Redis: {re}")
            # Fallback to memory if Redis fails at runtime
            _GLOBAL_MESSAGE_STORE[message_id] = message
            if logger:
                logger.warning(f"Message stored in MEMORY Cache with key: {message_id}")
    else:
        _GLOBAL_MESSAGE_STORE[message_id] = message
        if logger:
            logger.debug(f"Message stored in MEMORY Cache with key: {message_id}")


def get_message(message_id):
    if not message_id:
        return None
    if not _use_global_store and _redis_client:
        try:
            return _redis_client.get(message_id)
        except Exception:
            return _GLOBAL_MESSAGE_STORE.get(message_id)
    else:
        return _GLOBAL_MESSAGE_STORE.get(message_id)
