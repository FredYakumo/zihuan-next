use model_inference::system_config::{AgentToolConfig, AgentToolType, PythonScriptAgentToolConfig};
use zihuan_core::tool_runtime::ToolRunDuration;
use zihuan_graph_engine::brain_tool_spec::BrainToolImplementation;
use zihuan_graph_engine::function_graph::FunctionPortDef;
use zihuan_graph_engine::DataType;
use zihuan_service::agent::tool_definitions::build_enabled_tool_definitions;

#[test]
fn python_script_tool_builds_definition() {
    let tool = AgentToolConfig {
        id: "python-echo".to_string(),
        name: "python_echo".to_string(),
        description: "python echo".to_string(),
        enabled: true,
        run_duration: ToolRunDuration::Short,
        tool_type: AgentToolType::PythonScript(PythonScriptAgentToolConfig {
            script_path: "utils/python_tools/echo_tool.py".to_string(),
            module_entry: Some("run_tool".to_string()),
            python_mode: None,
            python_runtime: None,
            timeout_secs: Some(30),
            parameters: vec![zihuan_graph_engine::brain_tool_spec::ToolParamDef {
                name: "text".to_string(),
                data_type: DataType::String,
                desc: "text".to_string(),
                required: true,
            }],
            outputs: vec![FunctionPortDef {
                name: "result".to_string(),
                data_type: DataType::String,
                description: "echo output".to_string(),
                required: true,
            }],
        }),
    };

    let definitions = build_enabled_tool_definitions(&[tool]).expect("build tool definitions");
    assert_eq!(definitions.len(), 1);
    let definition = &definitions[0];
    assert_eq!(definition.implementation, BrainToolImplementation::PythonScript);
    let python_config = definition.python_config.as_ref().expect("python config");
    assert_eq!(python_config.script_path, "utils/python_tools/echo_tool.py");
    assert_eq!(python_config.module_entry, "run_tool");
    assert_eq!(python_config.timeout_secs, 30);
    assert!(python_config.runtime_override().is_none());
}

#[test]
fn python_script_tool_rejects_reserved_parameter_name() {
    let tool = AgentToolConfig {
        id: "python-invalid".to_string(),
        name: "python_invalid".to_string(),
        description: "python invalid".to_string(),
        enabled: true,
        run_duration: ToolRunDuration::Short,
        tool_type: AgentToolType::PythonScript(PythonScriptAgentToolConfig {
            script_path: "utils/python_tools/echo_tool.py".to_string(),
            module_entry: Some("run_tool".to_string()),
            python_mode: None,
            python_runtime: None,
            timeout_secs: Some(30),
            parameters: vec![zihuan_graph_engine::brain_tool_spec::ToolParamDef {
                name: "content".to_string(),
                data_type: DataType::String,
                desc: "reserved".to_string(),
                required: true,
            }],
            outputs: vec![FunctionPortDef {
                name: "result".to_string(),
                data_type: DataType::String,
                description: "echo output".to_string(),
                required: true,
            }],
        }),
    };

    let error = build_enabled_tool_definitions(&[tool]).expect_err("reserved name should fail");
    let message = format!("{error}");
    assert!(message.contains("保留运行时输入"));
}
