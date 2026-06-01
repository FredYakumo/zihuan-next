from sqlalchemy import Column, String, Boolean, DateTime, BigInteger, Text, Index
from database.base import Base


class TaskEntry(Base):
    __tablename__ = "task_entry"

    id = Column(String(64), primary_key=True)
    task_type = Column(String(32), nullable=False)
    graph_name = Column(String(256), nullable=False)
    graph_session_id = Column(String(128), nullable=False)
    file_path = Column(String(512), nullable=True)
    is_workflow_set = Column(Boolean, nullable=False, default=False)
    start_time = Column(DateTime, nullable=False)
    is_running = Column(Boolean, nullable=False, default=True)
    end_time = Column(DateTime, nullable=True)
    duration_ms = Column(BigInteger, nullable=True)
    user_ip = Column(String(64), nullable=True)
    owner_id = Column(String(128), nullable=True)
    status = Column(String(32), nullable=False)
    error_message = Column(Text, nullable=True)
    result_summary = Column(Text, nullable=True)
    can_rerun = Column(Boolean, nullable=False, default=False)

    __table_args__ = (
        Index("ix_task_entry_owner_id", "owner_id"),
        Index("ix_task_entry_status_end_time", "status", "end_time"),
    )
