use crate::agent::brain::{BrainTool, ToolRunDuration};
use crate::graph_engine::brain_tool_spec::BrainToolDefinition;

/// Factory for creating tool implementations from tool definitions.
/// This allows zihuan_core to define the agent runtime without depending on
/// service-specific subgraph execution (which lives in zihuan_service).
pub trait BrainToolFactory: Send + Sync {
    /// Create a BrainTool from a tool definition.
    /// Returns None if this factory cannot handle the given definition.
    fn create_tool(&self, definition: &BrainToolDefinition) -> Option<Box<dyn BrainTool>>;

    /// Returns the run duration for a given tool definition.
    /// Defaults to the definition's own run_duration field.
    fn run_duration_for(&self, definition: &BrainToolDefinition) -> ToolRunDuration {
        definition.run_duration
    }
}

/// Composite factory that tries multiple factories in order.
pub struct CompositeBrainToolFactory {
    factories: Vec<Box<dyn BrainToolFactory>>,
}

impl CompositeBrainToolFactory {
    pub fn new() -> Self {
        Self { factories: Vec::new() }
    }

    pub fn add_factory(mut self, factory: Box<dyn BrainToolFactory>) -> Self {
        self.factories.push(factory);
        self
    }
}

impl Default for CompositeBrainToolFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl BrainToolFactory for CompositeBrainToolFactory {
    fn create_tool(&self, definition: &BrainToolDefinition) -> Option<Box<dyn BrainTool>> {
        for factory in &self.factories {
            if let Some(tool) = factory.create_tool(definition) {
                return Some(tool);
            }
        }
        None
    }
}
