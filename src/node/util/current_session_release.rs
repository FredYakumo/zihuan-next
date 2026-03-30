use crate::error::Result;
use crate::node::data_value::CurrentSessionLeaseRef;
use crate::node::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;
use std::sync::Arc;

pub struct CurrentSessionReleaseNode {
    id: String,
    name: String,
}

impl CurrentSessionReleaseNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for CurrentSessionReleaseNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("释放当前 sender 会话锁租约")
    }

    node_input![
        port! { name = "lease_ref", ty = CurrentSessionLeaseRef, desc = "try_acquire 成功后返回的会话锁租约" },
    ];

    node_output![
        port! { name = "released", ty = Boolean, desc = "是否成功释放会话锁" },
    ];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let lease_ref: Arc<CurrentSessionLeaseRef> = inputs
            .get("lease_ref")
            .and_then(|value| match value {
                DataValue::CurrentSessionLeaseRef(lease_ref) => Some(lease_ref.clone()),
                _ => None,
            })
            .ok_or_else(|| crate::error::Error::InvalidNodeInput("lease_ref is required".to_string()))?;

        let mut outputs = HashMap::new();
        outputs.insert(
            "released".to_string(),
            DataValue::Boolean(lease_ref.release()),
        );
        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use super::CurrentSessionReleaseNode;
    use crate::error::Result;
    use crate::node::util::{CurrentSessionListProviderNode, CurrentSessionTryAcquireNode, SenderIdInCurrentSessionNode};
    use crate::node::{DataValue, Node};
    use std::collections::HashMap;
    use std::sync::Arc;

    fn provider_ref() -> Arc<crate::node::data_value::CurrentSessionRegistryRef> {
        let mut provider = CurrentSessionListProviderNode::new("provider", "Provider");
        match provider
            .execute(HashMap::new())
            .expect("provider should execute")
            .get("session_registry_ref")
        {
            Some(DataValue::CurrentSessionRegistryRef(registry_ref)) => registry_ref.clone(),
            other => panic!("unexpected provider output: {other:?}"),
        }
    }

    #[test]
    fn release_unlocks_sender_id() -> Result<()> {
        let registry_ref = provider_ref();
        let mut acquire = CurrentSessionTryAcquireNode::new("acquire", "Acquire");
        let mut contains = SenderIdInCurrentSessionNode::new("contains", "Contains");
        let mut release = CurrentSessionReleaseNode::new("release", "Release");

        let acquired = acquire.execute(HashMap::from([
            (
                "session_registry_ref".to_string(),
                DataValue::CurrentSessionRegistryRef(registry_ref.clone()),
            ),
            (
                "sender_id".to_string(),
                DataValue::String("user-1".to_string()),
            ),
        ]))?;

        let lease_ref = acquired
            .get("lease_ref")
            .cloned()
            .expect("lease_ref should exist after acquire");

        let contains_before = contains.execute(HashMap::from([
            (
                "session_registry_ref".to_string(),
                DataValue::CurrentSessionRegistryRef(registry_ref.clone()),
            ),
            (
                "sender_id".to_string(),
                DataValue::String("user-1".to_string()),
            ),
        ]))?;
        assert!(matches!(
            contains_before.get("in_session"),
            Some(DataValue::Boolean(true))
        ));

        let released = release.execute(HashMap::from([(
            "lease_ref".to_string(),
            lease_ref,
        )]))?;
        assert!(matches!(released.get("released"), Some(DataValue::Boolean(true))));

        let contains_after = contains.execute(HashMap::from([
            (
                "session_registry_ref".to_string(),
                DataValue::CurrentSessionRegistryRef(registry_ref),
            ),
            (
                "sender_id".to_string(),
                DataValue::String("user-1".to_string()),
            ),
        ]))?;
        assert!(matches!(
            contains_after.get("in_session"),
            Some(DataValue::Boolean(false))
        ));
        Ok(())
    }
}
