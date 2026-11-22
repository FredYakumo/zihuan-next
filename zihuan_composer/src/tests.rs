use std::sync::Arc;

use salvo::http::StatusCode;
use salvo::test::{ResponseExt, TestClient};
use serde_json::{json, Value};

#[test]
fn yaml_storage_replaces_documents_and_rejects_invalid_content() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("nested").join("components.yaml");
    let missing: Vec<String> = crate::config::read_yaml(&path).unwrap();
    assert!(missing.is_empty());
    crate::config::write_yaml(&path, &vec!["Redis", "Weaviate"]).unwrap();
    crate::config::write_yaml(&path, &vec!["MySQL"]).unwrap();
    let loaded: Vec<String> = crate::config::read_yaml(&path).unwrap();
    assert_eq!(loaded, ["MySQL"]);
    assert!(!path.with_extension("yaml.tmp").exists());
    std::fs::write(&path, "components: [").unwrap();
    assert!(crate::config::read_yaml::<Vec<String>>(&path).is_err());
}

#[tokio::test]
async fn concurrent_component_scopes_keep_artifacts_and_connection_ids_separate() {
    let temporary = tempfile::tempdir().unwrap();
    let first = temporary.path().join("first");
    let second = temporary.path().join("second");
    let first_scope = first.clone();
    let second_scope = second.clone();
    let first_task =
        tokio::spawn(crate::config::COMPONENT_SCOPE.scope(("first".into(), first_scope), async {
            tokio::task::yield_now().await;
            (crate::config::data_dir(), crate::config::connection_id("redis"))
        }));
    let second_task = tokio::spawn(crate::config::COMPONENT_SCOPE.scope(
        ("second".into(), second_scope),
        async {
            tokio::task::yield_now().await;
            (crate::config::data_dir(), crate::config::connection_id("redis"))
        },
    ));
    assert_eq!(first_task.await.unwrap(), (first, "first-redis".into()));
    assert_eq!(second_task.await.unwrap(), (second, "second-redis".into()));
}

#[tokio::test]
async fn local_api_requires_authentication_and_supports_component_templates() {
    let temporary = tempfile::tempdir().unwrap();
    let _lock = crate::config::initialize(temporary.path().join("composer")).unwrap();
    let state = Arc::new(crate::api::state::AppState::new());
    let token = format!("Bearer {}", state.token);
    let service = salvo::Service::new(crate::api::router(state));

    let response = TestClient::get("http://localhost/api/components").send(&service).await;
    assert_eq!(response.status_code, Some(StatusCode::UNAUTHORIZED));

    let mut response = TestClient::get("http://localhost/api/components")
        .add_header("authorization", &token, true)
        .send(&service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::OK));
    assert_eq!(response.take_json::<Value>().await.unwrap(), json!([]));

    for kind in [
        "zihuan",
        "mysql",
        "sqlite",
        "redis",
        "weaviate",
        "elasticsearch",
        "rustfs",
        "napcat",
        "wechat_pad",
    ] {
        let mut response = TestClient::get(format!("http://localhost/api/template/{kind}"))
            .add_header("authorization", &token, true)
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::OK));
        let document = response.take_json::<Value>().await.unwrap();
        let request: crate::api::components::InstallComponentRequest =
            serde_yaml::from_str(document["yaml"].as_str().unwrap()).unwrap();
        assert_eq!(request.component_type, kind);
        if !["zihuan", "sqlite", "napcat", "wechat_pad"].contains(&kind) {
            crate::installation::validate_detailed_config(
                request.detailed_config.as_ref().unwrap(),
            )
            .unwrap();
        }
    }

    let mut response = TestClient::get("http://localhost/api/template/unknown")
        .add_header("authorization", &token, true)
        .send(&service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::BAD_REQUEST));
    assert!(response.take_json::<Value>().await.unwrap()["error"].is_string());

    let mut response = TestClient::post("http://localhost/api/parse-install")
        .add_header("authorization", &token, true)
        .json(&json!({"yaml": "name: ["}))
        .send(&service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::BAD_REQUEST));
    assert!(response.take_json::<Value>().await.unwrap()["error"].is_string());
}

#[tokio::test]
async fn progress_history_retains_events_emitted_before_the_ui_connects() {
    let state = crate::api::state::AppState::new();
    let (sender, _) = tokio::sync::broadcast::channel(16);
    state.register_task("install".into(), sender.clone());
    crate::orchestrator::emit_progress(Some(&sender), "finished", "success", "完成".into());
    for _ in 0..100 {
        if state
            .history
            .lock()
            .unwrap()
            .get("install")
            .is_some_and(|events| !events.is_empty())
        {
            let history = state.history.lock().unwrap();
            assert_eq!(history["install"][0].step, "finished");
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("task event was not retained");
}
