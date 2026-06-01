use zihuan_graph_engine::{node_output_flow, DataValue};

fn main() {
    let output = "result";
    let _ = node_output_flow![
        output => DataValue::Boolean(true),
    ];
}
