//! Brain tools backed by `brain_tool` plugins.
//!
//! A plugin declares the LLM-facing function in its manifest (`tool`) and implements
//! `execute(args, config)`; the host only transports the call. All domain logic — encoding,
//! HTTP, formatting — stays inside the plugin, so a new capability needs no host change.

use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use crate::agent::tools::Tool;
use crate::error::Result;
use crate::model_inference::llm::tooling::FunctionTool;

/// One tool per installed `brain_tool` plugin, named after the plugin's declared tool.
pub struct PluginTool {
    plugin: dynamic_script_engine::PluginCatalogEntry,
    spec: Arc<dyn FunctionTool>,
    /// Runner watchdog budget: larger than the plugin's own HTTP timeouts so a slow upstream
    /// surfaces as the plugin's error message instead of a killed process.
    limit: Duration,
}

impl PluginTool {
    /// Builds the tool from a catalog entry, or `None` when the entry cannot be a tool
    /// (no declared spec, or no `execute` method to call).
    pub fn from_plugin(plugin: dynamic_script_engine::PluginCatalogEntry) -> Option<Self> {
        let tool = plugin.tool.as_ref()?;
        let name = tool.name.trim();
        if name.is_empty() || !plugin.methods.iter().any(|method| method == "execute") {
            return None;
        }
        let spec = Arc::new(PluginFunctionToolSpec {
            name: name.to_string(),
            description: tool.description.clone(),
            parameters: tool.parameters.clone(),
        });
        Some(Self { plugin, spec, limit: PLUGIN_TOOL_TIMEOUT })
    }
}

/// Upper bound on one plugin tool call, matching the default plugin timeout budget.
const PLUGIN_TOOL_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Debug)]
struct PluginFunctionToolSpec {
    name: String,
    description: String,
    parameters: Value,
}

impl FunctionTool for PluginFunctionToolSpec {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters(&self) -> Value {
        self.parameters.clone()
    }

    fn call(&self, _arguments: Value) -> Result<Value> {
        // The engine executes tools through `Tool::execute`; the plugin is the executor here.
        Ok(Value::Null)
    }
}

impl Tool for PluginTool {
    fn spec(&self) -> Arc<dyn FunctionTool> {
        Arc::clone(&self.spec)
    }

    fn execute(&self, _call_content: &str, arguments: &Value) -> Result<String> {
        let response = crate::script_plugins::call_plugin_method(
            &self.plugin,
            "execute",
            arguments.clone(),
            // A brain tool has no connection, so it gets an empty config object.
            Value::Object(Default::default()),
            self.limit,
        )?;
        Ok(match response {
            // A string result is passed through verbatim so the plugin can report its own
            // structured errors; anything else is handed to the model as JSON.
            Value::String(text) => text,
            other => other.to_string(),
        })
    }
}

/// Every installed `brain_tool` plugin, in catalog order.
pub fn plugin_brain_tools() -> Vec<PluginTool> {
    crate::script_plugins::plugins_of_type(crate::script_plugins::PLUGIN_TYPE_BRAIN_TOOL)
        .into_iter()
        .filter_map(PluginTool::from_plugin)
        .collect()
}

/// The installed `brain_tool` plugins a service selected, in catalog order.
///
/// A selected name with no installed plugin, or one whose manifest cannot act as a tool, is
/// skipped with a warning: a stale selection must not stop the service from starting.
pub fn selected_plugin_brain_tools(names: &[String]) -> Vec<PluginTool> {
    if names.is_empty() {
        return Vec::new();
    }
    let installed =
        crate::script_plugins::plugins_of_type(crate::script_plugins::PLUGIN_TYPE_BRAIN_TOOL);
    let mut tools = Vec::new();
    for name in names {
        match installed.iter().find(|plugin| &plugin.name == name) {
            Some(plugin) => match PluginTool::from_plugin(plugin.clone()) {
                Some(tool) => tools.push(tool),
                None => log::warn!(
                    "brain_tool plugin '{name}' is selected but declares no usable tool; skipped"
                ),
            },
            None => log::warn!("brain_tool plugin '{name}' is selected but not installed; skipped"),
        }
    }
    tools
}

/// One installed `brain_tool` plugin by name, for a configured tool entry that needs its spec.
pub fn plugin_brain_tool(name: &str) -> Option<PluginTool> {
    crate::script_plugins::plugins_of_type(crate::script_plugins::PLUGIN_TYPE_BRAIN_TOOL)
        .into_iter()
        .find(|plugin| plugin.name == name)
        .and_then(PluginTool::from_plugin)
}
