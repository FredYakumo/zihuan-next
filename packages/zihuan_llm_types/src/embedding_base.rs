pub trait EmbeddingBase: std::fmt::Debug + Send + Sync {
    fn get_model_name(&self) -> &str;

    fn embed_text(&self, text: &str) -> zihuan_core::error::Result<Vec<f32>>;

    fn embed_texts(&self, texts: &[String]) -> zihuan_core::error::Result<Vec<Vec<f32>>>;
}
