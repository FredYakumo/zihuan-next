use salvo::prelude::*;

#[handler]
async fn assets(req: &mut Request, res: &mut Response) {
    {
        let path = req.uri().path().trim_start_matches('/');
        let path = if path.is_empty() { "index.html" } else { path };
        let Some(asset) = FrontendAssets::get(path) else {
            res.status_code(StatusCode::NOT_FOUND);
            return;
        };
        let mime = match path.rsplit('.').next() {
            Some("html") => "text/html; charset=utf-8",
            Some("js") => "application/javascript; charset=utf-8",
            Some("css") => "text/css; charset=utf-8",
            Some("png") => "image/png",
            Some("svg") => "image/svg+xml",
            _ => "application/octet-stream",
        };
        res.headers_mut().insert("content-type", mime.parse().unwrap());
        res.headers_mut().insert("content-security-policy", "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: https:; connect-src 'self'; frame-ancestors 'none'".parse().unwrap());
        res.headers_mut().insert("referrer-policy", "no-referrer".parse().unwrap());
        res.headers_mut().insert("cache-control", "no-store".parse().unwrap());
        res.write_body(asset.data.into_owned()).unwrap();
    }
}

#[derive(rust_embed::RustEmbed)]
#[folder = "ui/dist/"]
struct FrontendAssets;

pub fn router() -> Router {
    Router::new().get(assets).push(Router::with_path("<**path>").get(assets))
}

#[cfg(test)]
mod tests {
    use super::*;
    use salvo::test::{ResponseExt, TestClient};
    #[tokio::test]
    async fn desktop_assets_preserve_css_and_javascript_content_types() {
        let service = salvo::Service::new(router());
        let mut response = TestClient::get("http://localhost/").send(&service).await;
        assert_eq!(response.headers().get("content-type").unwrap(), "text/html; charset=utf-8");
        let html = response.take_string().await.unwrap();
        let assets: Vec<&str> =
            html.split('"').filter(|value| value.starts_with("/assets/")).collect();
        assert!(assets.iter().any(|path| path.ends_with(".css")));
        assert!(assets.iter().any(|path| path.ends_with(".js")));
        for path in assets {
            let mut response =
                TestClient::get(format!("http://localhost{path}")).send(&service).await;
            assert_eq!(response.status_code, Some(StatusCode::OK));
            let expected = if path.ends_with(".css") {
                "text/css; charset=utf-8"
            } else {
                "application/javascript; charset=utf-8"
            };
            assert_eq!(response.headers().get("content-type").unwrap(), expected);
            assert!(!response.take_string().await.unwrap().is_empty());
        }
        let response = TestClient::get("http://localhost/assets/missing.css").send(&service).await;
        assert_eq!(response.status_code, Some(StatusCode::NOT_FOUND));
    }
}
