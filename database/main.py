"""
Database and data management maintenance tasks.

DEPRECATED: Alembic has been removed. Database tables are now created automatically
by the Rust backend when a MySQL or SQLite connection is added via the Connections UI.
This module is retained as a schema reference only.
"""

from utils.logging_config import logger


def main():
    """Deprecated entry point — schema is now managed by the Rust backend."""
    logger.info("Database schema is now managed by the Rust backend. Nothing to do.")


if __name__ == "__main__":
    main()
