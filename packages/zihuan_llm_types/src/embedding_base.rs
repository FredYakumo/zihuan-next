pub trait EmbeddingBase: std::fmt::Debug + Send + Sync {
    fn get_model_name(&self) -> &str;

    fn inference(&self, text: &str) -> zihuan_core::error::Result<Vec<f32>>;

    fn batch_inference(&self, texts: &[String]) -> zihuan_core::error::Result<Vec<Vec<f32>>>;
}
