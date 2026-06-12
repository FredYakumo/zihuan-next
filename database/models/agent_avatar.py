from sqlalchemy import Column, String, DateTime, LargeBinary, Index
from datetime import datetime
from database.base import Base


class AgentAvatar(Base):
    """存储 Agent 头像图片的数据表"""
    __tablename__ = "agent_avatar"

    id = Column(String(64), primary_key=True, comment="头像唯一ID")
    agent_id = Column(String(64), nullable=False, comment="关联的 Agent ID")
    file_name = Column(String(256), nullable=True, comment="原始文件名")
    mime_type = Column(String(64), nullable=False, comment="图片 MIME 类型，如 image/png")
    image_data = Column(LargeBinary, nullable=False, comment="图片二进制数据")
    created_at = Column(DateTime, nullable=False, default=datetime.utcnow, comment="创建时间")
    updated_at = Column(DateTime, nullable=False, default=datetime.utcnow, onupdate=datetime.utcnow, comment="更新时间")

    __table_args__ = (
        Index("ix_agent_avatar_agent_id", "agent_id"),
    )
