use std::collections::HashMap;
use std::sync::Arc;

use zihuan_llm::nn::queued_embedding_model::QueuedEmbeddingModel;
use zihuan_core::error::{Error, Result};
use zihuan_core::llm::embedding_base::EmbeddingBase;
use zihuan_graph_engine::{node_input, node_output, DataType, DataValue, Node, Port};

pub struct LoadLocalTextEmbedderNode {
    id: String,
    name: String,
}

impl LoadLocalTextEmbedderNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self { id: id.into(), name: name.into() }
    }
}

impl Node for LoadLocalTextEmbedderNode {
    fn id(&self) -> &str { &self.id }
    fn name(&self) -> &str { &self.name }
    fn description(&self) -> Option<&str> {
        Some("从 models/text_embedding/<model_name> 加载本地 Candle embedding 模型，输出 EmbeddingModel 引用")
    }

    node_input![
        port! { name = "model_name", ty = String, desc = "models/text_embedding 下的模型目录名，例如 Qwen3-Embedding-0.6B" },
    ];

    node_output![
        port! { name = "embedding_model", ty = EmbeddingModel, desc = "Embedding 模型引用" },
    ];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let model_name = match inputs.get("model_name") {
            Some(DataValue::String(value)) if !value.trim().is_empty() => value.trim().to_string(),
            _ => return Err(Error::ValidationError("Missing required input: model_name".to_string())),
        };

        let model: Arc<dyn EmbeddingBase> = Arc::new(QueuedEmbeddingModel::new(model_name)?);

        let outputs = HashMap::from([("embedding_model".to_string(), DataValue::EmbeddingModel(model))]);
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
