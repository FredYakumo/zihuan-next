use std::collections::BTreeMap;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};

use dynamic_script_engine::PluginCatalogEntry;

use crate::error::Result;
use crate::validation_error;

use super::web_search_engine::{WebSearchEngine, WebSearchImage};

const METHOD_SEARCH: &str = "search";
const METHOD_EXTRACT_URL: &str = "extract_url";
const METHOD_FETCH_URL_DIRECT: &str = "fetch_url_direct";
const METHOD_SEARCH_IMAGES: &str = "search_images";

/// A web search engine backed by one plugin directory under `plugins/`.
///
/// Each trait call runs the plugin entry in its own dependency environment with `(args,
/// config)`, where `config` carries the connection's credentials and timeout. The plugin's
/// own HTTP timeout uses the configured `timeout_secs`, while the runner watchdog adds grace
/// so a plugin can report its timeout as a normal error instead of being killed mid-request.
pub struct PluginWebSearchEngine {
    plugin: PluginCatalogEntry,
    api_token: String,
    timeout_secs: u64,
    params: BTreeMap<String, String>,
}

impl PluginWebSearchEngine {
    pub fn new(
        plugin: PluginCatalogEntry,
        api_token: String,
        timeout_secs: u64,
        params: BTreeMap<String, String>,
    ) -> Self {
        Self { plugin, api_token, timeout_secs, params }
    }

    fn implements(&self, method: &str) -> bool {
        self.plugin.methods.iter().any(|candidate| candidate == method)
    }

    fn call(&self, method: &str, args: Value) -> Result<Value> {
        let limit = Duration::from_secs(self.timeout_secs.saturating_add(10));
        // Plugin params flatten into the config object; the connection's own columns are
        // written last so a param named like them can never shadow the real value.
        let mut config = serde_json::Map::new();
        for (key, value) in &self.params {
            config.insert(key.clone(), Value::String(value.clone()));
        }
        config.insert("api_token".to_string(), Value::String(self.api_token.clone()));
        config.insert("timeout_secs".to_string(), Value::Number(self.timeout_secs.into()));
        crate::script_plugins::call_plugin_method(
            &self.plugin,
            method,
            args,
            Value::Object(config),
            limit,
        )
    }

    fn result_strings(&self, method: &str, args: Value) -> Result<Vec<String>> {
        let response = self.call(method, args)?;
        let results = response.get("results").and_then(Value::as_array).ok_or_else(|| {
            validation_error!(
                "web search 插件 '{}' 的 {method} 返回缺少 results 数组",
                self.plugin.name
            )
        })?;
        Ok(results.iter().filter_map(|value| value.as_str().map(str::to_string)).collect())
    }
}

#[derive(Deserialize)]
struct PluginImageList {
    #[serde(default)]
    images: Vec<WebSearchImage>,
}

impl WebSearchEngine for PluginWebSearchEngine {
    fn search(&self, query: &str, search_count: i64) -> Result<Vec<String>> {
        self.result_strings(METHOD_SEARCH, json!({"query": query, "search_count": search_count}))
    }

    fn extract_url(&self, url: &str) -> Result<Vec<String>> {
        if self.implements(METHOD_EXTRACT_URL) {
            self.result_strings(METHOD_EXTRACT_URL, json!({"url": url}))
        } else {
            // Plugins without a dedicated extract API (for example Brave) degrade the same
            // way their native implementations did: fall back to the direct web request.
            self.fetch_url_direct(url)
        }
    }

    fn fetch_url_direct(&self, url: &str) -> Result<Vec<String>> {
        self.result_strings(METHOD_FETCH_URL_DIRECT, json!({"url": url}))
    }

    fn search_images(&self, query: &str, max_results: i64) -> Result<Vec<WebSearchImage>> {
        if !self.implements(METHOD_SEARCH_IMAGES) {
            return Ok(Vec::new());
        }
        let response =
            self.call(METHOD_SEARCH_IMAGES, json!({"query": query, "max_results": max_results}))?;
        let parsed: PluginImageList = serde_json::from_value(response).map_err(|error| {
            validation_error!(
                "web search 插件 '{}' 的 search_images 返回无效: {error}",
                self.plugin.name
            )
        })?;
        Ok(parsed.images)
    }
}

impl std::fmt::Debug for PluginWebSearchEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginWebSearchEngine")
            .field("plugin", &self.plugin.name)
            .field("api_token", &"<redacted>")
            .field("timeout_secs", &self.timeout_secs)
            .finish()
    }
}
