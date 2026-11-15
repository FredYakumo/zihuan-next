//! Upstream/downstream dependency analysis between stored configurations.
//!
//! Role Services sit downstream of the resources they reference: connection
//! configs and LLM refs are referenced from the role service type payload by
//! `config_id`, and sub-agent definitions are referenced through the service's
//! tool list (`AgentToolType::SubAgent`). Sub-agent definitions may in turn
//! reference other sub-agent definitions through their `tool_ids`, so the
//! agent → service mapping is computed transitively.
//!
//! The analysis works on the generic [`RoleServiceConfig`] payload (raw JSON)
//! and [`AgentDefinition`]s, so `zihuan_core` keeps no reverse dependency on
//! the crates that own the concrete role service payloads.

use std::collections::{HashMap, HashSet, VecDeque};

use serde::Serialize;

use crate::agent::declarative_agent::AgentDefinition;
use crate::agent::tool_definitions::selected_sub_agent_ids;
use crate::role::service_config::RoleServiceConfig;

/// The kind of stored configuration whose save/delete triggered the analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigDependencyKind {
    Connection,
    LlmRef,
    SubAgent,
    RoleService,
}

/// One role service that depends on the analyzed configuration.
#[derive(Debug, Clone, Serialize)]
pub struct DependentRoleService {
    pub id: String,
    pub name: String,
    pub enabled: bool,
}

/// Collects the connection config ids a role service references.
///
/// The scan walks the raw payload JSON and picks every string value under a key
/// that reads as a connection reference (`connection_id`, `rdb_id`, or any
/// `*_connection_id`), which covers the flat fields of both role service kinds
/// as well as nested objects such as `retrieval_store`.
pub fn role_service_connection_ref_ids(role_service: &RoleServiceConfig) -> HashSet<String> {
    let payload = serde_json::to_value(&role_service.role_service_type).unwrap_or_default();
    collect_id_refs(&payload, is_connection_ref_key)
}

/// Collects the LLM ref ids a role service references (`*_llm_ref_id` / `*_model_ref_id`).
pub fn role_service_llm_ref_ids(role_service: &RoleServiceConfig) -> HashSet<String> {
    let payload = serde_json::to_value(&role_service.role_service_type).unwrap_or_default();
    collect_id_refs(&payload, is_llm_ref_key)
}

/// Collects the sub-agent ids a role service selects through its tools.
pub fn role_service_sub_agent_ids(role_service: &RoleServiceConfig) -> Vec<String> {
    selected_sub_agent_ids(&role_service.tools)
}

/// Computes the role services that depend on the given configuration.
///
/// For [`ConfigDependencyKind::SubAgent`] the reverse reference closure over
/// agent definitions is computed first: an agent referencing the analyzed one
/// through its `tool_ids` pulls in every role service that selects it too.
pub fn analyze_dependent_role_services(
    kind: ConfigDependencyKind,
    config_id: &str,
    role_services: &[RoleServiceConfig],
    agent_definitions: &[AgentDefinition],
) -> Vec<DependentRoleService> {
    role_services
        .iter()
        .filter(|role_service| {
            role_service_depends_on(kind, config_id, role_service, agent_definitions)
        })
        .map(|role_service| DependentRoleService {
            id: role_service.id.clone(),
            name: role_service.name.clone(),
            enabled: role_service.enabled,
        })
        .collect()
}

fn role_service_depends_on(
    kind: ConfigDependencyKind,
    config_id: &str,
    role_service: &RoleServiceConfig,
    agent_definitions: &[AgentDefinition],
) -> bool {
    match kind {
        ConfigDependencyKind::Connection => {
            role_service_connection_ref_ids(role_service).contains(config_id)
        }
        ConfigDependencyKind::LlmRef => role_service_llm_ref_ids(role_service).contains(config_id),
        ConfigDependencyKind::SubAgent => {
            let referencing = agent_reverse_reference_closure(config_id, agent_definitions);
            role_service_sub_agent_ids(role_service)
                .iter()
                .any(|agent_id| referencing.contains(agent_id))
        }
        ConfigDependencyKind::RoleService => {
            role_service.id == config_id || role_service.config_id == config_id
        }
    }
}

/// All agent ids that reference `target_id` directly or through other agents.
///
/// `tool_ids` also name plain tools, node-graph tools and plugins, so only ids
/// that resolve to an existing agent definition are treated as agent references.
fn agent_reverse_reference_closure(
    target_id: &str,
    agent_definitions: &[AgentDefinition],
) -> HashSet<String> {
    let known_ids: HashSet<&str> =
        agent_definitions.iter().map(|definition| definition.id.as_str()).collect();

    // Reverse edges: referencing agent -> referenced agent.
    let mut reverse_edges: HashMap<&str, Vec<&str>> = HashMap::new();
    for definition in agent_definitions {
        for tool_id in agent_tool_ids(definition, &known_ids) {
            reverse_edges.entry(tool_id).or_default().push(&definition.id);
        }
    }

    let mut closure = HashSet::new();
    let mut queue = VecDeque::from([target_id]);
    while let Some(current) = queue.pop_front() {
        if let Some(referencing) = reverse_edges.get(current) {
            for agent_id in referencing {
                if closure.insert((*agent_id).to_string()) {
                    queue.push_back(agent_id);
                }
            }
        }
    }
    closure
}

fn agent_tool_ids<'a>(definition: &'a AgentDefinition, known_ids: &HashSet<&str>) -> Vec<&'a str> {
    definition
        .tool_ids
        .iter()
        .map(String::as_str)
        .filter(|tool_id| known_ids.contains(tool_id))
        .collect()
}

fn is_connection_ref_key(key: &str) -> bool {
    key == "connection_id" || key == "rdb_id" || key.ends_with("_connection_id")
}

fn is_llm_ref_key(key: &str) -> bool {
    key.ends_with("llm_ref_id") || key.ends_with("model_ref_id")
}

fn collect_id_refs(
    value: &serde_json::Value,
    key_matches: impl Fn(&str) -> bool,
) -> HashSet<String> {
    let mut refs = HashSet::new();
    collect_id_refs_inner(value, &key_matches, &mut refs);
    refs
}

fn collect_id_refs_inner(
    value: &serde_json::Value,
    key_matches: &impl Fn(&str) -> bool,
    refs: &mut HashSet<String>,
) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, nested) in map {
                if key_matches(key) {
                    if let serde_json::Value::String(id) = nested {
                        if !id.trim().is_empty() {
                            refs.insert(id.clone());
                        }
                    }
                }
                collect_id_refs_inner(nested, key_matches, refs);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_id_refs_inner(item, key_matches, refs);
            }
        }
        _ => {}
    }
}
