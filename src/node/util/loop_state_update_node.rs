use crate::error::{Error, Result};
use crate::node::data_value::LoopControl;
use crate::node::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;
use std::sync::Arc;

/// Writes new state into the shared LoopControl so that the next iteration of
/// the enclosing LoopNode emits the updated value as `output`.
///
/// This breaks what would otherwise be a data-edge back-edge from the tail of
/// the loop body back to Brain/LLM nodes: instead of a graph edge, the updated
/// value travels through the `Arc<LoopControl>` side-channel, keeping the
/// DAG topology strictly acyclic.
pub struct LoopStateUpdateNode {
	id: String,
	name: String,
}

impl LoopStateUpdateNode {
	pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
		Self {
			id: id.into(),
			name: name.into(),
		}
	}
}

impl Node for LoopStateUpdateNode {
	fn id(&self) -> &str {
		&self.id
	}

	fn name(&self) -> &str {
		&self.name
	}

	fn description(&self) -> Option<&str> {
		Some("将 new_state 写入循环控制引用的状态，供下一轮迭代使用；无图返回边，保持 DAG 结构")
	}

	node_input![
		port! {
			name = "loop_control",
			ty = LoopControlRef,
			desc = "来自 LoopNode 的循环控制引用"
		},
		port! {
			name = "new_state",
			ty = Any,
			desc = "本轮迭代结束后写入循环的新状态值"
		},
	];

	node_output![];

	fn execute(
		&mut self,
		inputs: HashMap<String, DataValue>,
	) -> Result<HashMap<String, DataValue>> {
		let loop_control: Arc<LoopControl> = inputs
			.get("loop_control")
			.and_then(|v| match v {
				DataValue::LoopControlRef(r) => Some(r.clone()),
				_ => None,
			})
			.ok_or_else(|| Error::InvalidNodeInput("loop_control is required".into()))?;

		let new_state = inputs
			.get("new_state")
			.cloned()
			.ok_or_else(|| Error::InvalidNodeInput("new_state is required".into()))?;

		loop_control.update_state(new_state);

		Ok(HashMap::new())
	}
}
