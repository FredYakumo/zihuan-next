use zihuan_graph_engine::{node_output_flow, DataValue, NodeOutputFlow};

fn main() {
    let value = DataValue::Boolean(true);

    let _outputs: NodeOutputFlow = node_output_flow![
        "success" => value,
    ];
}
