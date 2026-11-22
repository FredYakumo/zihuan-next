use salvo::prelude::*;
use serde_json::json;

#[handler]
pub async fn template(req: &mut Request, res: &mut Response) {
    let kind = req.param::<String>("kind").unwrap_or_default();
    if ![
        "zihuan",
        "mysql",
        "sqlite",
        "redis",
        "weaviate",
        "elasticsearch",
        "rustfs",
        "napcat",
        "wechat_pad",
    ]
    .contains(&kind.as_str())
    {
        return super::components::render_error(
            res,
            StatusCode::BAD_REQUEST,
            "未知组件类型".into(),
        );
    }
    let secret = uuid::Uuid::new_v4().simple().to_string();
    let deployment = |image: &str, port: u16, name: &str| {
        json!({
            "image": image, "port": port, "data_dir": format!("./data/{name}"),
            "container_name": format!("zihuan-{name}-{}", &secret[..8]), "restart_policy": "unless-stopped"
        })
    };
    let mut config = json!({
        "install_method": "docker", "target_machine_address": "", "expose_public_access": false, "use_target_machine_address": false,
        "relational": {"enabled": kind == "mysql", "source": "install", "type": "mysql",
            "deployment": deployment("mysql:8.4", 3306, "mysql"), "host": "127.0.0.1", "username": "root", "password": secret,
            "database": "zihuan", "sqlite_path": "", "max_connections": 32, "acquire_timeout_secs": 30},
        "rustfs": {"enabled": kind == "rustfs", "source": "install", "deployment": deployment("rustfs/rustfs:latest", 9000, "rustfs"),
            "endpoint": "http://127.0.0.1:9000", "bucket": "zihuan", "region": "us-east-1", "access_key": "zihuan",
            "secret_key": secret, "public_base_url": null, "path_style": true},
        "search": {"enabled": kind == "weaviate" || kind == "elasticsearch", "source": "install", "type": "weaviate",
            "deployment": deployment("cr.weaviate.io/semitechnologies/weaviate:1.30.5", 8080, "search"), "base_url": "http://127.0.0.1:8080",
            "username": null, "password": null, "api_key": secret, "auth_method": "api_key", "vector_dimensions": 1024},
        "redis": {"enabled": kind == "redis", "source": "install", "deployment": deployment("redis:7", 6379, "redis"),
            "url": "redis://127.0.0.1:6379", "username": null, "password": secret}
    });
    if kind == "elasticsearch" {
        config["search"] = json!({"enabled": true, "source": "install", "type": "elasticsearch",
            "deployment": deployment("docker.elastic.co/elasticsearch/elasticsearch:8.17.0", 9200, "search"), "base_url": "http://127.0.0.1:9200",
            "username": "elastic", "password": secret, "api_key": null, "auth_method": "password", "vector_dimensions": 1024});
    }
    let mut request = json!({
        "name": format!("{kind}-{}", &secret[..8]), "version": "latest", "component_type": kind,
        "install_method": if kind == "sqlite" {"embedded"} else {"docker"}, "detailed_config": config, "extra_install_metadata": {}
    });
    if kind == "zihuan" {
        request["detailed_config"] = serde_json::Value::Null;
        request["extra_install_metadata"] = json!({"image": "zihuan-next:latest", "host": "127.0.0.1", "port": 9951, "source": "", "http_proxy": ""});
    }
    match serde_yaml::to_string(&request) {
        Ok(yaml) => res.render(Json(json!({"yaml": yaml}))),
        Err(error) => super::components::render_error(
            res,
            StatusCode::INTERNAL_SERVER_ERROR,
            error.to_string(),
        ),
    }
}
