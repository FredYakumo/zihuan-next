pub mod command;
pub mod nodes;
pub mod role;

#[cfg(test)]
mod tests;

pub use role::{RoleServiceManager, RoleServiceRuntimeInfo, RoleServiceRuntimeStatus};

use zihuan_core::error::Result;

pub fn init_node_registry() -> Result<()> {
    use zihuan_core::register_node;

    use nodes::tool_calling_node::ToolCallingNode;

    register_node!(
        "tool_calling",
        "ToolCallingEngine",
        "AI",
        "使用 LLM + system prompt + user message 触发带可编辑 Tools 的函数调用推理",
        ToolCallingNode
    );

    Ok(())
}
