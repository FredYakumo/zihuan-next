use std::path::PathBuf;

use zihuan_graph_engine::load_graph_definition_from_json_with_migration;

fn workflow_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("workflow_set")
        .join(format!("{name}.json"))
}

#[test]
fn deep_search_workflow_uses_optional_group_id_node() {
    let loaded =
        load_graph_definition_from_json_with_migration(workflow_path("deep_search_qq_message"))
            .expect("workflow should load");

    let graph = loaded.graph;
    let prompt_node = graph
        .nodes
        .iter()
        .find(|node| node.id == "e2e1823e-2765-4c5c-86d6-0c8e8f68e270")
        .expect("workflow should contain prompt builder function");
    let prompt_json =
        serde_json::to_string(&prompt_node.inline_values).expect("inline values should serialize");

    assert!(
        prompt_json.contains("\"node_type\":\"extract_optional_group_id_from_event\""),
        "deep_search_qq_message should use the private-safe group id extractor"
    );
}

#[test]
fn deep_search_workflow_prompt_mentions_private_empty_group_id() {
    let loaded =
        load_graph_definition_from_json_with_migration(workflow_path("deep_search_qq_message"))
            .expect("workflow should load");

    let graph = loaded.graph;
    let prompt_node = graph
        .nodes
        .iter()
        .find(|node| node.id == "e2e1823e-2765-4c5c-86d6-0c8e8f68e270")
        .expect("workflow should contain prompt builder function");
    let prompt_json =
        serde_json::to_string(&prompt_node.inline_values).expect("inline values should serialize");

    assert!(
        prompt_json.contains("当前群号（私聊为空）"),
        "prompt should explain that private chats have an empty current group id"
    );
}
