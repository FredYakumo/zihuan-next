use crate::data_value::{SessionClaim, SessionStateRef, SESSION_CLAIM_CONTEXT};
use crate::{node_input, node_output, DataType, DataValue, Node, Port};
use log::info;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::task::block_in_place;
use zihuan_core::error::Result;

pub struct SessionStateTryClaimNode {
    id: String,
    name: String,
}

impl SessionStateTryClaimNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for SessionStateTryClaimNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("原子检查并占用 sender_id 会话状态")
    }

    node_input![
        port! { name = "session_ref", ty = SessionStateRef, desc = "运行时会话状态引用" },
        port! { name = "sender_id", ty = String, desc = "会话发送者 ID" },
        port! { name = "state_json", ty = Json, desc = "成功占用时写入的附加 JSON 状态", optional },
    ];

    node_output![
        port! { name = "claimed", ty = Boolean, desc = "本次是否成功占用" },
        port! { name = "in_session", ty = Boolean, desc = "当前 sender_id 是否正在处理中" },
        port! { name = "state_json", ty = Json, desc = "当前 sender_id 的附加 JSON 状态" },
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
                zihuan_core::error::Error::InvalidNodeInput("session_ref is required".to_string())
            })?;
        let sender_id = inputs
            .get("sender_id")
            .and_then(|value| match value {
                DataValue::String(sender_id) => Some(sender_id.clone()),
                _ => None,
            })
            .ok_or_else(|| {
                zihuan_core::error::Error::InvalidNodeInput("sender_id is required".to_string())
            })?;
        let desired_state = inputs.get("state_json").and_then(|value| match value {
            DataValue::Json(value) => Some(value.clone()),
            _ => None,
        });

        let task_context = SESSION_CLAIM_CONTEXT.try_with(Arc::clone).ok();
        info!(
            "[SessionStateTryClaimNode:{}] Trying claim for sender_id={} on session_ref={}",
            self.id, sender_id, session_ref.node_id
        );
        let session_ref_for_try = session_ref.clone();
        let sender_id_for_try = sender_id.clone();
        let try_claim = async move {
            session_ref_for_try
                .try_claim(&sender_id_for_try, desired_state)
                .await
        };
        let (state, claimed) = if let Ok(handle) = tokio::runtime::Handle::try_current() {
            block_in_place(|| handle.block_on(try_claim))
        } else {
            tokio::runtime::Runtime::new()?.block_on(try_claim)
        };

        info!(
            "[SessionStateTryClaimNode:{}] Claim result for sender_id={} on session_ref={}: claimed={}, in_session={}, state_claim_token={:?}",
            self.id,
            sender_id,
            session_ref.node_id,
            claimed,
            state.in_session,
            state.claim_token
        );
        if claimed {
            if let (Some(context), Some(claim_token)) = (task_context, state.claim_token) {
                context.register_claim(SessionClaim {
                    session_ref: session_ref.clone(),
                    sender_id: sender_id.clone(),
                    claim_token,
                });
                info!(
                    "[SessionStateTryClaimNode:{}] Registered claim for sender_id={} on session_ref={} with claim_token={}",
                    self.id,
                    sender_id,
                    session_ref.node_id,
                    claim_token
                );
            }
        }

        let outputs = HashMap::from([
            ("claimed".to_string(), DataValue::Boolean(claimed)),
            (
                "in_session".to_string(),
                DataValue::Boolean(state.in_session),
            ),
            (
                "state_json".to_string(),
                DataValue::Json(match state.state_json {
                    Value::Null => Value::Null,
                    other => other,
                }),
            ),
        ]);
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}
