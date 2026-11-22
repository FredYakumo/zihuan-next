use std::sync::Arc;

use salvo::prelude::*;
use serde::Deserialize;

pub mod components;
pub mod state;
mod templates;

use components::render_error;
use state::AppState;

#[handler]
async fn authorize(req: &mut Request, depot: &mut Depot, res: &mut Response, ctrl: &mut FlowCtrl) {
    let state = depot.obtain::<Arc<AppState>>().unwrap();
    let expected = format!("Bearer {}", state.token);
    if req.header::<String>("authorization").as_deref() != Some(expected.as_str()) {
        render_error(res, StatusCode::UNAUTHORIZED, "Composer authentication required".into());
        ctrl.skip_rest();
    }
}

#[handler]
async fn build_info(res: &mut Response) {
    res.render(Json(serde_json::json!({
        "composer_version": env!("CARGO_PKG_VERSION"),
        "version": env!("ZIHUAN_BUILD_VERSION"),
        "build_type": env!("ZIHUAN_BUILD_TYPE"),
    })));
}

#[handler]
async fn environment(res: &mut Response) {
    res.render(Json(crate::orchestrator::detect_environment().await));
}

#[handler]
async fn progress(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let id = req.param::<String>("id").unwrap_or_default();
    let state = depot.obtain::<Arc<AppState>>().unwrap();
    let history = state.history.lock().unwrap();
    match history.get(&id) {
        Some(events) => res.render(Json(events)),
        None => render_error(res, StatusCode::NOT_FOUND, "任务不存在".into()),
    }
}

#[handler]
async fn connections(res: &mut Response) {
    match crate::storage::load_connections() {
        Ok(value) => res.render(Json(value)),
        Err(error) => render_error(res, StatusCode::INTERNAL_SERVER_ERROR, error),
    }
}

#[derive(Deserialize)]
struct YamlDocument {
    yaml: String,
}

#[handler]
async fn parse_install(req: &mut Request, res: &mut Response) {
    let result = async {
        let body: YamlDocument = req.parse_json().await.map_err(|error| error.to_string())?;
        let request: components::InstallComponentRequest =
            serde_yaml::from_str(&body.yaml).map_err(|error| error.to_string())?;
        Ok::<_, String>(request)
    }
    .await;
    match result {
        Ok(request) => match serde_yaml::to_string(&request) {
            Ok(yaml) => res.render(Json(serde_json::json!({"request": request, "yaml": yaml}))),
            Err(error) => render_error(res, StatusCode::BAD_REQUEST, error.to_string()),
        },
        Err(error) => render_error(res, StatusCode::BAD_REQUEST, error),
    }
}

#[handler]
async fn configuration(req: &mut Request, res: &mut Response) {
    let id = req.param::<String>("id").unwrap_or_default();
    let result = (|| {
        let component = components::find_component(&id)?;
        let path = configuration_path(&component)?;
        let yaml = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
        Ok::<_, String>(serde_json::json!({"yaml": yaml, "path": path}))
    })();
    match result {
        Ok(value) => res.render(Json(value)),
        Err(error) => render_error(res, StatusCode::BAD_REQUEST, error),
    }
}

fn configuration_path(
    component: &components::ComponentRecord,
) -> Result<std::path::PathBuf, String> {
    if component.installation_method == "docker" {
        return component
            .extra_install_metadata
            .get("compose_path")
            .and_then(serde_json::Value::as_str)
            .map(std::path::PathBuf::from)
            .ok_or_else(|| "组件没有 Compose 文件".to_string());
    }
    if component.component_type == "zihuan" && component.installation_method == "native" {
        return Ok(crate::config::root()
            .join("components")
            .join(&component.id)
            .join("runtime.yaml"));
    }
    Err("此组件的 native 配置由原生程序管理；可在连接配置中修改紫幻访问它的参数".into())
}

#[handler]
async fn save_configuration(req: &mut Request, res: &mut Response) {
    let result = async {
        let id = req.param::<String>("id").unwrap_or_default();
        let body: YamlDocument = req.parse_json().await.map_err(|error| error.to_string())?;
        let component = components::find_component(&id)?;
        let path = configuration_path(&component)?;
        let value: serde_yaml::Value =
            serde_yaml::from_str(&body.yaml).map_err(|error| error.to_string())?;
        if component.installation_method == "docker"
            && value.get("services").and_then(serde_yaml::Value::as_mapping).is_none()
        {
            return Err("Compose 配置需要 services 映射".to_string());
        }
        if component.installation_method == "native" {
            serde_yaml::from_str::<crate::self_install::NativeRuntime>(&body.yaml)
                .map_err(|error| error.to_string())?;
        }
        crate::config::write_yaml(&path, &value)?;
        Ok::<_, String>(serde_json::json!({"saved": true, "message": "重新启动组件以应用配置"}))
    }
    .await;
    match result {
        Ok(value) => res.render(Json(value)),
        Err(error) => render_error(res, StatusCode::BAD_REQUEST, error),
    }
}

#[handler]
async fn save_connections(req: &mut Request, res: &mut Response) {
    let result = async {
        let body: YamlDocument = req.parse_json().await.map_err(|error| error.to_string())?;
        let value: Vec<zihuan_core::storage::ConnectionConfig> =
            serde_yaml::from_str(&body.yaml).map_err(|error| error.to_string())?;
        crate::storage::save_connections(value)
    }
    .await;
    match result {
        Ok(()) => res.render(Json(serde_json::json!({"saved": true}))),
        Err(error) => render_error(res, StatusCode::BAD_REQUEST, error),
    }
}

#[handler]
async fn connection_yaml(res: &mut Response) {
    match crate::storage::load_connections()
        .and_then(|value| serde_yaml::to_string(&value).map_err(|error| error.to_string()))
    {
        Ok(yaml) => res.render(Json(serde_json::json!({"yaml": yaml}))),
        Err(error) => render_error(res, StatusCode::INTERNAL_SERVER_ERROR, error),
    }
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new().hoop(affix_state::inject(state)).push(
        Router::with_path("api")
            .hoop(authorize)
            .push(Router::with_path("build-info").get(build_info))
            .push(Router::with_path("environment").get(environment))
            .push(Router::with_path("template/<kind>").get(templates::template))
            .push(Router::with_path("parse-install").post(parse_install))
            .push(Router::with_path("tasks/<id>").get(progress))
            .push(Router::with_path("connections").get(connections))
            .push(Router::with_path("connections/yaml").get(connection_yaml).put(save_connections))
            .push(
                Router::with_path("components")
                    .get(components::list_components)
                    .push(Router::with_path("install").post(components::install_component))
                    .push(
                        Router::with_path("wechat_pad/login_qr")
                            .post(components::wechat_login::wechat_login_qr),
                    )
                    .push(
                        Router::with_path("wechat_pad/login_status")
                            .post(components::wechat_login::wechat_login_status),
                    )
                    .push(
                        Router::with_path("<id>")
                            .delete(components::delete_component)
                            .push(
                                Router::with_path("configuration")
                                    .get(configuration)
                                    .put(save_configuration),
                            )
                            .push(Router::with_path("enable").post(components::enable_component))
                            .push(Router::with_path("disable").post(components::disable_component)),
                    ),
            ),
    )
}
