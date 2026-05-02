"""add media_json to message_record

Revision ID: 4f2a8c1d9e3b
Revises: e8c7d6f2b123
Create Date: 2026-05-02 17:10:00.000000

"""
from typing import Sequence, Union

from alembic import op
import sqlalchemy as sa


# revision identifiers, used by Alembic.
revision: str = "4f2a8c1d9e3b"
down_revision: Union[str, Sequence[str], None] = "e8c7d6f2b123"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    """Add media_json column to message_record."""
    op.add_column(
        "message_record",
        sa.Column("media_json", sa.Text(), nullable=True),
    )


def downgrade() -> None:
    """Drop media_json column from message_record."""
    op.drop_column("message_record", "media_json")
