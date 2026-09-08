use std::any::Any;
use std::sync::Arc;

/// 连接类资源种类，由引擎统一识别，具体取值逻辑由业务方实现。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionKind {
    Rdb,
    S3,
    ImageWeaviate,
    WebSearch,
}

/// Agent 运行时资源契约：引擎（子图工具、脚本节点）只通过该抽象
/// 获取当前 Agent 的模型与连接资源，不感知具体服务配置类型。
///
/// 业务方在需要取回完整配置（如限流规则、情绪维度等引擎无关字段）时，
/// 通过 [`AgentResourceProvider::as_any`] 下钻到具体类型。
pub trait AgentResourceProvider: Send + Sync {
    /// 按用途返回 LLM 引用 ID，业务回退链（如缺省回落主模型）由实现方处理。
    fn llm_ref_id(&self, kind: &str) -> Option<String>;
    fn embedding_model_ref_id(&self) -> Option<String>;
    fn connection_id(&self, kind: ConnectionKind) -> Option<String>;

    /// 下钻到具体配置类型，供业务代码取回引擎无关的完整配置。
    fn as_any(&self) -> &dyn Any;
}

pub type SharedAgentResourceProvider = Arc<dyn AgentResourceProvider>;
