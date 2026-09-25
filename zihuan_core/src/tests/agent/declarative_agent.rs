use std::collections::{HashMap, HashSet};

use serde_json::json;

use crate::agent::declarative_agent::{
    agent_input_from_tool_arguments, default_llm_kind, render_template, render_user_prompt,
    validate_agent_id, AgentDefinition, AgentOutputMode, AgentPromptPart,
};
use crate::graph::function_graph::FunctionPortDef;
use crate::graph::util::function::data_value_from_json_with_declared_type;
use crate::graph::{DataType, DataValue};
use crate::tool_runtime::ToolRunDuration;

fn available_tools() -> HashSet<String> {
    ["search_memory", "update_memory", "list_memory_keys"]
        .into_iter()
        .map(ToOwned::to_owned)
        .collect()
}

fn sample_definition(id: &str) -> AgentDefinition {
    AgentDefinition {
        id: id.to_string(),
        name: "Memory".to_string(),
        description: String::new(),
        builtin: false,
        inputs: vec![],
        outputs: vec![],
        system_prompt: String::new(),
        user_prompt: None,
        prompt_parts: vec![],
        output_mode: AgentOutputMode::JsonPorts,
        llm_kind: default_llm_kind(),
        progress_message: None,
        include_graph_tools: false,
        run_duration: ToolRunDuration::Short,
        tool_ids: vec!["search_memory".to_string()],
    }
}

#[test]
fn definition_round_trips_through_yaml() {
    let definition = sample_definition("memory");
    let yaml = serde_yaml::to_string(&definition).unwrap();
    let parsed: AgentDefinition = serde_yaml::from_str(&yaml).unwrap();
    assert_eq!(parsed, definition);
    parsed.validate(&available_tools()).unwrap();
}

#[test]
fn legacy_definition_without_new_fields_parses() {
    let yaml = "id: memory\nname: Memory\nsystem_prompt: hi\ntool_ids: [search_memory]\n";
    let parsed: AgentDefinition = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(parsed.output_mode, AgentOutputMode::JsonPorts);
    assert!(parsed.user_prompt.is_none());
    parsed.validate(&available_tools()).unwrap();
}

#[test]
fn validation_rejects_duplicate_ports_and_unauthorized_tools() {
    let mut definition = sample_definition("memory");
    definition.inputs = vec![FunctionPortDef {
        name: "content".to_string(),
        data_type: DataType::String,
        description: String::new(),
        required: true,
    }];
    definition.inputs.push(FunctionPortDef {
        name: "content".to_string(),
        data_type: DataType::String,
        description: String::new(),
        required: true,
    });
    assert!(definition.validate(&available_tools()).is_err());

    let mut definition = sample_definition("memory");
    definition.tool_ids.push("not_allowed".to_string());
    assert!(definition.validate(&available_tools()).is_err());
}

#[test]
fn validation_rejects_invalid_and_conflicting_ids() {
    assert!(validate_agent_id("Research").is_err());
    assert!(validate_agent_id("../research").is_err());
    let definition = sample_definition("search_memory");
    assert!(definition.validate(&available_tools()).is_err());
}

#[test]
fn declared_ports_convert_json_arguments() {
    let port = FunctionPortDef {
        name: "count".to_string(),
        data_type: DataType::Integer,
        description: String::new(),
        required: true,
    };
    let value = data_value_from_json_with_declared_type(&port, &json!(7)).unwrap();
    assert!(matches!(value, DataValue::Integer(7)));
    assert!(data_value_from_json_with_declared_type(&port, &json!("seven")).is_err());
}

#[test]
fn tool_arguments_follow_declared_input_ports() {
    let inputs = vec![FunctionPortDef {
        name: "content".to_string(),
        data_type: DataType::String,
        description: String::new(),
        required: true,
    }];
    let input = agent_input_from_tool_arguments(&inputs, &json!({ "content": "hello" })).unwrap();
    assert!(matches!(input.get("content"), Some(DataValue::String(value)) if value == "hello"));
    assert!(agent_input_from_tool_arguments(&inputs, &json!({})).is_err());
}

#[test]
fn template_renders_known_ports_and_drops_unknown() {
    let input = HashMap::from([
        ("problem".to_string(), DataValue::String("ping".to_string())),
        ("count".to_string(), DataValue::Integer(3)),
    ]);
    assert_eq!(render_template("{problem}/{count}/{missing}", &input), "ping/3/");
}

#[test]
fn prompt_parts_match_on_input_value() {
    let definition = AgentDefinition {
        user_prompt: Some("{chat_context}".to_string()),
        prompt_parts: vec![AgentPromptPart {
            port: "operation".to_string(),
            equals: Some("search_memory".to_string()),
            template: "\nSEARCH".to_string(),
        }],
        tool_ids: vec![],
        ..sample_definition("memory")
    };
    let input =
        HashMap::from([("operation".to_string(), DataValue::String("search_memory".to_string()))]);
    assert!(render_user_prompt(&definition, &input).ends_with("\nSEARCH"));

    let other =
        HashMap::from([("operation".to_string(), DataValue::String("update_memory".to_string()))]);
    assert!(!render_user_prompt(&definition, &other).contains("SEARCH"));
}
