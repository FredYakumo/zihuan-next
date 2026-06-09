use zihuan_core::error::Result;
use zihuan_core::llm::{InferenceParam, LLMMessage};
use zihuan_graph_engine::{node_input, node_output, DataType, DataValue, Node, Port};

pub struct LLMInferNode {
    id: String,
    name: String,
}

impl LLMInferNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for LLMInferNode {
    fn id(&self) -> &str {
        &self.id
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn description(&self) -> Option<&str> {
        Some("使用LLModel引用对消息列表进行一次推理，返回模型回复")
    }

    node_input![
        port! { name = "llm_model", ty = LLModel, desc = "LLM模型引用，由LlmNode提供" },
        port! { name = "messages",  ty = Vec(LLMMessage), desc = "输入消息列表，包含系统消息和用户消息" },
    ];

    node_output![port! { name = "response", ty = Vec(LLMMessage), desc = "LLM返回的消息列表" },];

    fn execute(&mut self, inputs: zihuan_graph_engine::NodeInputFlow) -> Result<zihuan_graph_engine::NodeOutputFlow> {
        self.validate_inputs(&inputs)?;

        let model = match inputs.get("llm_model") {
            Some(DataValue::LLModel(m)) => m.clone(),
            _ => {
                return Err(zihuan_core::error::Error::ValidationError(
                    "Missing required input: llm_model".to_string(),
                ));
            }
        };

        let messages: Vec<LLMMessage> = match inputs.get("messages") {
            Some(DataValue::Vec(_, items)) => items
                .iter()
                .filter_map(|item| {
                    if let DataValue::LLMMessage(m) = item {
                        Some(m.clone())
                    } else {
                        None
                    }
                })
                .collect(),
            _ => {
                return Err(zihuan_core::error::Error::ValidationError(
                    "Missing required input: messages".to_string(),
                ));
            }
        };

        let param = InferenceParam {
            messages: &messages,
            tools: None,
        };
        let response_message = model.inference(&param);

        zihuan_graph_engine::return_with_node_output![self;
            "response" => DataValue::Vec(
                Box::new(DataType::LLMMessage),
                vec![DataValue::LLMMessage(response_message)],
            ),
        ]
    }
}
