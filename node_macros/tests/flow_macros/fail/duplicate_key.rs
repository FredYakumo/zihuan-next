use zihuan_graph_engine::{node_output_flow, DataValue};

fn main() {
    let _ = node_output_flow![
        "result" => DataValue::Boolean(true),
        "result" => DataValue::Boolean(false),
    ];
}
