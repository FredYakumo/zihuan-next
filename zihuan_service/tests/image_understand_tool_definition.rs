use model_inference::system_config::{AgentToolConfig, AgentToolType, NodeGraphToolConfig};
use serde_json::Value;
use zihuan_graph_engine::DataType;
use zihuan_service::agent::tool_definitions::build_enabled_tool_definitions;
use zihuan_service::nodes::tool_subgraph::tool_parameters_to_json_schema;

#[test]
fn image_understand_workflow_derives_optional_message_id_parameter() {
    let tool = AgentToolConfig {
        id: "tool-image-understand".to_string(),
        name: "image_understand".to_string(),
        description: "test".to_string(),
        enabled: true,
        tool_type: AgentToolType::NodeGraph(NodeGraphToolConfig::FilePath {
            path: format!(
                "{}/../workflow_set/image_understand.json",
                env!("CARGO_MANIFEST_DIR")
            ),
            parameters: Vec::new(),
            outputs: Vec::new(),
        }),
    };

    let definitions = build_enabled_tool_definitions(&[tool]).expect("build tool definitions");
    assert_eq!(definitions.len(), 1);

    let definition = &definitions[0];
    assert_eq!(definition.outputs.len(), 1);
    assert_eq!(definition.outputs[0].name, "image_description");

    let message_id = definition
        .parameters
        .iter()
        .find(|param| param.name == "message_id")
        .expect("message_id parameter should be derived from workflow graph");
    assert_eq!(message_id.data_type, DataType::Integer);
    assert!(!message_id.required, "message_id should stay optional");

    let schema = tool_parameters_to_json_schema(&definition.parameters);
    let required = schema
        .get("required")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    assert!(
        !required
            .iter()
            .any(|item| item.as_str() == Some("message_id")),
        "message_id should not be required in tool JSON schema"
    );
}
