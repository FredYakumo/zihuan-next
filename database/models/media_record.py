from sqlalchemy import Column, String, DateTime, Text
from datetime import datetime
from database.base import Base


class MediaRecord(Base):
    """存储媒体文件元数据"""
    __tablename__ = "media_record"

    media_id = Column(String(256), primary_key=True)
    source = Column(String(32), nullable=False, comment="媒体来源：upload / qq_chat / web_search / agent_save")
    original_source = Column(Text, nullable=False, comment="原始 URL 或路径")
    rustfs_path = Column(Text, nullable=False, comment="RustFS 保存路径")
    name = Column(String(512), nullable=True, comment="文件名")
    description = Column(Text, nullable=True, comment="图片文字描述")
    mime_type = Column(String(128), nullable=True, comment="MIME 类型，如 image/jpeg")
    created_at = Column(DateTime, nullable=False, default=datetime.utcnow, comment="记录创建时间")
