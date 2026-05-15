use std::collections::HashMap;

use model_inference::agent_config_support::{build_llm_from_ref_id, LLM_KIND_FIELD};
use zihuan_core::agent_config::{
    current_qq_chat_agent_config, llm_ref_id_for_kind, normalize_llm_kind, LLM_KIND_MAIN,
};
use zihuan_core::error::Result;
use zihuan_graph_engine::{
    node_output, DataType, DataValue, Node, NodeConfigField, NodeConfigWidget, Port,
};

pub struct AgentLlmNode {
    id: String,
    name: String,
    llm_kind: Option<String>,
}

impl AgentLlmNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            llm_kind: Some(LLM_KIND_MAIN.to_string()),
        }
    }
}

impl Node for AgentLlmNode {
    fn id(&self) -> &str {
        &self.id
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn description(&self) -> Option<&str> {
        Some("从当前 Agent 工具调用上下文中读取指定 LLM，并输出 LLModel 引用")
    }
    fn input_ports(&self) -> Vec<Port> {
        Vec::new()
    }

    node_output![
        port! { name = "llm_model", ty = LLModel, desc = "当前 Agent 选定类型的 LLM 引用" },
    ];

    fn config_fields(&self) -> Vec<NodeConfigField> {
        vec![NodeConfigField::new(
            LLM_KIND_FIELD,
            DataType::String,
            NodeConfigWidget::AgentLlmKindSelect,
        )
        .with_description("选择读取主模型、意图分类模型或数学编程模型")]
    }

    fn apply_inline_config(&mut self, inline_values: &HashMap<String, DataValue>) -> Result<()> {
        self.llm_kind = inline_values
            .get(LLM_KIND_FIELD)
            .and_then(|value| match value {
                DataValue::String(value) => Some(value.clone()),
                _ => None,
            });
        Ok(())
    }

    fn execute(
        &mut self,
        _inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        let config = current_qq_chat_agent_config()?;
        let llm_kind = normalize_llm_kind(self.llm_kind.as_deref())?;
        let llm = build_llm_from_ref_id(llm_ref_id_for_kind(&config, llm_kind))?;
        Ok(HashMap::from([(
            "llm_model".to_string(),
            DataValue::LLModel(llm),
        )]))
    }
}
