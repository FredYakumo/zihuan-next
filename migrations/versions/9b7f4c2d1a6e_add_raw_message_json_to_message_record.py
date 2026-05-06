"""add raw_message_json to message_record

Revision ID: 9b7f4c2d1a6e
Revises: 4f2a8c1d9e3b
Create Date: 2026-05-07 07:30:00.000000

"""
from typing import Sequence, Union

from alembic import op
import sqlalchemy as sa


# revision identifiers, used by Alembic.
revision: str = "9b7f4c2d1a6e"
down_revision: Union[str, Sequence[str], None] = "4f2a8c1d9e3b"
branch_labels: Union[str, Sequence[str], None] = None
depends_on: Union[str, Sequence[str], None] = None


def upgrade() -> None:
    """Add raw_message_json column to message_record."""
    op.add_column(
        "message_record",
        sa.Column("raw_message_json", sa.Text(), nullable=True),
    )


def downgrade() -> None:
    """Drop raw_message_json column from message_record."""
    op.drop_column("message_record", "raw_message_json")
