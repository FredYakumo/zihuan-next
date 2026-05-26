from sqlalchemy import Column, Integer, String, Text, ForeignKey, Index
from database.base import Base


class TaskProgress(Base):
    __tablename__ = "task_progress"

    id = Column(Integer, primary_key=True, autoincrement=True)
    task_id = Column(String(64), ForeignKey("task_entry.id", ondelete="CASCADE"), nullable=False)
    seq = Column(Integer, nullable=False)
    message = Column(Text, nullable=False)

    __table_args__ = (Index("ix_task_progress_task_id_seq", "task_id", "seq"),)
