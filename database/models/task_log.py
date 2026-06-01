from sqlalchemy import Column, Integer, String, Text, ForeignKey, Index
from database.base import Base


class TaskLog(Base):
    __tablename__ = "task_log"

    id = Column(Integer, primary_key=True, autoincrement=True)
    task_id = Column(String(64), ForeignKey("task_entry.id", ondelete="CASCADE"), nullable=False)
    timestamp = Column(String(40), nullable=False)
    level = Column(String(16), nullable=False)
    message = Column(Text, nullable=False)

    __table_args__ = (Index("ix_task_log_task_id", "task_id"),)
