use crate::error::Result;
use crate::node::data_value::{SessionStateRef, SESSION_CLAIM_CONTEXT};
use crate::node::{node_input, node_output, DataType, DataValue, Node, Port};
use log::info;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::task::block_in_place;

pub struct SessionStateReleaseNode {
    id: String,
    name: String,
}

impl SessionStateReleaseNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for SessionStateReleaseNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("释放 sender_id 的会话占用状态")
    }

    node_input![
        port! { name = "session_ref", ty = SessionStateRef, desc = "运行时会话状态引用" },
        port! { name = "sender_id", ty = String, desc = "会话发送者 ID" },
    ];

    node_output![
        port! { name = "released", ty = Boolean, desc = "是否成功释放当前 sender_id 的占用" },
    ];

    fn execute(
        &mut self,
        inputs: HashMap<String, DataValue>,
    ) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let session_ref: Arc<SessionStateRef> = inputs
            .get("session_ref")
            .and_then(|value| match value {
                DataValue::SessionStateRef(session_ref) => Some(session_ref.clone()),
                _ => None,
            })
            .ok_or_else(|| {
                crate::error::Error::InvalidNodeInput("session_ref is required".to_string())
            })?;
        let sender_id = inputs
            .get("sender_id")
            .and_then(|value| match value {
                DataValue::String(sender_id) => Some(sender_id.clone()),
                _ => None,
            })
            .ok_or_else(|| {
                crate::error::Error::InvalidNodeInput("sender_id is required".to_string())
            })?;

        let claim_token = SESSION_CLAIM_CONTEXT
            .try_with(|context| {
                let token = context.claim_token_for(&session_ref.node_id, &sender_id);
                context.unregister_claim(&session_ref.node_id, &sender_id);
                token
            })
            .ok()
            .flatten();
        info!(
            "[SessionStateReleaseNode:{}] Releasing sender_id={} on session_ref={} with claim_token={:?}",
            self.id,
            sender_id,
            session_ref.node_id,
            claim_token
        );

        let session_ref_for_release = session_ref.clone();
        let sender_id_for_release = sender_id.clone();
        let release = async move {
            session_ref_for_release
                .release(&sender_id_for_release, claim_token)
                .await
        };
        let released = if let Ok(handle) = tokio::runtime::Handle::try_current() {
            block_in_place(|| handle.block_on(release))
        } else {
            tokio::runtime::Runtime::new()?.block_on(release)
        };
        info!(
            "[SessionStateReleaseNode:{}] Release result for sender_id={} on session_ref={}: released={}",
            self.id,
            sender_id,
            session_ref.node_id,
            released
        );

        let outputs = HashMap::from([("released".to_string(), DataValue::Boolean(released))]);
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
