use zihuan_core::error::Result;
use crate::data_value::LoopControl;
use crate::{node_input, node_output, DataType, DataValue, Node, NodeType, Port};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct LoopNode {
    id: String,
    name: String,
    loop_control: Arc<LoopControl>,
    stop_flag: Arc<AtomicBool>,
}

impl LoopNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            loop_control: Arc::new(LoopControl::new()),
            stop_flag: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Node for LoopNode {
    fn node_type(&self) -> NodeType {
        NodeType::EventProducer
    }

    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("重复执行，将 input 透传为 output，直到 LoopBreakNode 触发退出条件")
    }

    node_input![port! { name = "input", ty = Any, desc = "循环中透传的数据" },];

    node_output![
        port! { name = "output", ty = Any, desc = "透传的数据（与 input 相同）" },
        port! { name = "loop_control", ty = LoopControlRef, desc = "循环控制引用，连接到 LoopBreakNode" },
    ];

    fn execute(
        &mut self,
        _inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        Ok(HashMap::new())
    }

    fn set_stop_flag(&mut self, stop_flag: Arc<AtomicBool>) {
        self.stop_flag = stop_flag;
    }

    fn on_start(&mut self, inputs: HashMap<String, DataValue>) -> Result<()> {
        self.loop_control.reset();
        let data = inputs
            .get("input")
            .cloned()
            .unwrap_or(DataValue::Boolean(false));
        self.loop_control.init_state(data);
        Ok(())
    }

    fn on_update(&mut self) -> Result<Option<HashMap<String, DataValue>>> {
        if self.stop_flag.load(Ordering::Relaxed) || self.loop_control.should_break() {
            return Ok(None);
        }
        let data = self.loop_control.get_state();
        let mut outputs = HashMap::new();
        outputs.insert("output".to_string(), data);
        outputs.insert(
            "loop_control".to_string(),
            DataValue::LoopControlRef(self.loop_control.clone()),
        );
        Ok(Some(outputs))
    }

    fn on_error_request_stop(&self) {
        self.loop_control.request_break();
    }

    fn suppress_error_after_stop_request(&self) -> bool {
        true
    }
}
