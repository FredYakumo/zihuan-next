use crate::error::Result;
use crate::node::data_value::CurrentSessionRegistryRef;
use crate::node::{node_input, node_output, DataType, DataValue, Node, Port};
use std::collections::HashMap;
use std::sync::Arc;

pub struct CurrentSessionTryAcquireNode {
    id: String,
    name: String,
}

impl CurrentSessionTryAcquireNode {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
        }
    }
}

impl Node for CurrentSessionTryAcquireNode {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        Some("原子尝试获取 sender_id 会话锁，成功时输出 lease_ref，失败时不阻塞")
    }

    node_input![
        port! { name = "session_registry_ref", ty = CurrentSessionRegistryRef, desc = "当前运行期 sender 会话锁注册表引用" },
        port! { name = "sender_id", ty = String, desc = "要尝试获取锁的 sender_id" },
    ];

    node_output![
        port! { name = "acquired", ty = Boolean, desc = "是否成功拿到该 sender_id 的会话锁" },
        port! { name = "lease_ref", ty = CurrentSessionLeaseRef, desc = "成功获取会话锁时输出的租约引用" },
    ];

    fn execute(&mut self, inputs: HashMap<String, DataValue>) -> Result<HashMap<String, DataValue>> {
        self.validate_inputs(&inputs)?;

        let registry_ref: Arc<CurrentSessionRegistryRef> = inputs
            .get("session_registry_ref")
            .and_then(|value| match value {
                DataValue::CurrentSessionRegistryRef(registry_ref) => Some(registry_ref.clone()),
                _ => None,
            })
            .ok_or_else(|| crate::error::Error::InvalidNodeInput("session_registry_ref is required".to_string()))?;

        let sender_id = inputs
            .get("sender_id")
            .and_then(|value| match value {
                DataValue::String(sender_id) => Some(sender_id.clone()),
                _ => None,
            })
            .ok_or_else(|| crate::error::Error::InvalidNodeInput("sender_id is required".to_string()))?;

        let lease_ref = registry_ref.try_acquire(&sender_id);
        let mut outputs = HashMap::new();
        outputs.insert(
            "acquired".to_string(),
            DataValue::Boolean(lease_ref.is_some()),
        );
        if let Some(lease_ref) = lease_ref {
            outputs.insert(
                "lease_ref".to_string(),
                DataValue::CurrentSessionLeaseRef(lease_ref),
            );
        }

        self.validate_outputs(&outputs)?;
        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use super::CurrentSessionTryAcquireNode;
    use crate::error::Result;
    use crate::node::util::CurrentSessionListProviderNode;
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
    fn only_one_try_acquire_succeeds_for_same_sender() -> Result<()> {
        let registry_ref = provider_ref();
        let mut node = CurrentSessionTryAcquireNode::new("acquire", "Acquire");

        let first = node.execute(HashMap::from([
            (
                "session_registry_ref".to_string(),
                DataValue::CurrentSessionRegistryRef(registry_ref.clone()),
            ),
            (
                "sender_id".to_string(),
                DataValue::String("user-1".to_string()),
            ),
        ]))?;
        let second = node.execute(HashMap::from([
            (
                "session_registry_ref".to_string(),
                DataValue::CurrentSessionRegistryRef(registry_ref),
            ),
            (
                "sender_id".to_string(),
                DataValue::String("user-1".to_string()),
            ),
        ]))?;

        assert!(matches!(first.get("acquired"), Some(DataValue::Boolean(true))));
        assert!(matches!(second.get("acquired"), Some(DataValue::Boolean(false))));
        assert!(first.get("lease_ref").is_some());
        assert!(second.get("lease_ref").is_none());
        Ok(())
    }
}
