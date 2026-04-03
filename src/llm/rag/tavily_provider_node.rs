pub use zihuan_llm::rag::tavily_provider_node::*;
#[cfg(test)]
mod tests {
    use super::TavilyProviderNode;
    use crate::error::Result;
    use crate::node::{DataValue, Node};
    use std::collections::HashMap;
    use std::time::Duration;

    #[test]
    fn outputs_tavily_ref_with_default_timeout() -> Result<()> {
        let mut node = TavilyProviderNode::new("provider", "Provider");
        let outputs = node.execute(HashMap::from([(
            "api_token".to_string(),
            DataValue::Password("secret-token".to_string()),
        )]))?;

        match outputs.get("tavily_ref") {
            Some(DataValue::TavilyRef(tavily_ref)) => {
                assert_eq!(tavily_ref.api_token, "secret-token");
                assert_eq!(tavily_ref.timeout, Duration::from_secs(30));
            }
            other => panic!("unexpected output: {:?}", other),
        }

        Ok(())
    }

    #[test]
    fn respects_custom_timeout() -> Result<()> {
        let mut node = TavilyProviderNode::new("provider", "Provider");
        let outputs = node.execute(HashMap::from([
            (
                "api_token".to_string(),
                DataValue::Password("secret-token".to_string()),
            ),
            ("timeout_secs".to_string(), DataValue::Integer(12)),
        ]))?;

        match outputs.get("tavily_ref") {
            Some(DataValue::TavilyRef(tavily_ref)) => {
                assert_eq!(tavily_ref.timeout, Duration::from_secs(12));
            }
            other => panic!("unexpected output: {:?}", other),
        }

        Ok(())
    }

    #[test]
    fn rejects_empty_token() {
        let mut node = TavilyProviderNode::new("provider", "Provider");
        let err = node
            .execute(HashMap::from([(
                "api_token".to_string(),
                DataValue::Password("   ".to_string()),
            )]))
            .expect_err("empty token should be rejected");

        assert!(err.to_string().contains("api_token"));
    }
}
