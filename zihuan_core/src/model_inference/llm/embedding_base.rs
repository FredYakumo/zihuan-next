/// Where an embedding model actually runs, for the live inference report in the Data Explorer.
#[derive(Debug, Clone)]
pub struct EmbeddingRuntimeInfo {
    /// Inference backend: `candle` for local inference, `http` for a remote embedding API.
    pub backend: &'static str,
    /// Device the backend runs on (`cpu`, `cuda`, `metal`); `None` when not applicable.
    pub device: Option<&'static str>,
    /// Output vector dimension, when the model configuration already knows it.
    pub dimension: Option<usize>,
}

impl Default for EmbeddingRuntimeInfo {
    fn default() -> Self {
        Self {
            backend: "unknown",
            device: None,
            dimension: None,
        }
    }
}

pub trait EmbeddingBase: std::fmt::Debug + Send + Sync {
    fn get_model_name(&self) -> &str;

    fn inference(&self, text: &str) -> crate::error::Result<Vec<f32>>;

    fn batch_inference(&self, texts: &[String]) -> crate::error::Result<Vec<Vec<f32>>>;

    /// Runtime description reported in the Data Explorer context flow.
    fn runtime_info(&self) -> EmbeddingRuntimeInfo {
        EmbeddingRuntimeInfo::default()
    }
}
