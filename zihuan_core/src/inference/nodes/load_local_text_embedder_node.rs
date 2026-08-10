use crate::graph::NodeOutputFlow;
use std::collections::HashMap;
use std::sync::Arc;

use crate::inference::nn::queued_embedding_model::QueuedEmbeddingModel;
use crate::error::{Error, Result};
use crate::llm::embedding_base::EmbeddingBase;
use crate::graph::{node_input, node_output, DataType, DataValue, Node, Port};

pub struct LoadLocalTextEmbedderNode {
    id: String,
    name: String,
}

impl LoadLocalTextEmbedderNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for LoadLocalTextEmbedderNode {
    fn id(&self) -> &str {
        &self.id
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn description(&self) -> Option<&str> {
        Some("从 models/text_embedding/<model_name> 加载本地 Candle embedding 模型，输出 EmbeddingModel 引用")
    }

    node_input![
        port! { name = "model_name", ty = String, desc = "models/text_embedding 下的模型目录名，例如 Qwen3-Embedding-0.6B" },
    ];

    node_output![port! { name = "embedding_model", ty = EmbeddingModel, desc = "Embedding 模型引用" },];

    fn execute(&mut self, inputs: crate::graph::NodeInputFlow) -> Result<crate::graph::NodeOutputFlow> {
        self.validate_inputs(&inputs)?;

        let model_name = match inputs.get("model_name") {
            Some(DataValue::String(value)) if !value.trim().is_empty() => value.trim().to_string(),
            _ => {
                return Err(Error::ValidationError("Missing required input: model_name".to_string()));
            }
        };

        let model: Arc<dyn EmbeddingBase> = Arc::new(QueuedEmbeddingModel::new(model_name)?);

        crate::graph::return_with_node_output![self;
            "embedding_model" => DataValue::EmbeddingModel(model),
        ]
    }
}
