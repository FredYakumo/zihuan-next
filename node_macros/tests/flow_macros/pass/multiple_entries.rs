use zihuan_graph_engine::{node_input_flow, node_output_flow, DataValue, NodeInputFlow, NodeOutputFlow};

fn build_value(prefix: &str, count: i64) -> DataValue {
    DataValue::String(format!("{prefix}-{count}"))
}

fn main() {
    let text = "hello".to_string();

    let _inputs: NodeInputFlow = node_input_flow![
        "message" => DataValue::String(text.clone()),
        "count" => DataValue::Integer(1),
    ];

    let _outputs: NodeOutputFlow = node_output_flow![
        "result" => build_value("ok", 2),
        "success" => DataValue::Boolean(true),
    ];
}
