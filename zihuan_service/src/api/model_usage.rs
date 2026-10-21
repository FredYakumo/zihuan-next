use chrono::{DateTime, Utc};
use salvo::prelude::*;
use salvo::writing::Json;
use zihuan_core::model_inference::model_usage::load_usage_summaries;

use crate::api::config::render_internal_error;

#[handler]
pub async fn get_model_usage(req: &mut Request, res: &mut Response) {
    let range = match (req.query::<String>("start"), req.query::<String>("end")) {
        (None, None) => None,
        (Some(start), Some(end)) => {
            match (DateTime::parse_from_rfc3339(&start), DateTime::parse_from_rfc3339(&end)) {
                (Ok(start), Ok(end)) if start < end => {
                    Some((start.with_timezone(&Utc), end.with_timezone(&Utc)))
                }
                _ => {
                    res.status_code(StatusCode::BAD_REQUEST);
                    res.render(Json(serde_json::json!({ "error": "日期范围无效" })));
                    return;
                }
            }
        }
        _ => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(serde_json::json!({ "error": "请同时提供开始和结束时间" })));
            return;
        }
    };
    let result = tokio::task::spawn_blocking(move || {
        let config_ids = crate::system_config::load_llm_refs()?
            .into_iter()
            .map(|model| model.canonical_config_id().to_string())
            .collect::<Vec<_>>();
        load_usage_summaries(&config_ids, range)
    })
    .await;
    match result {
        Ok(Ok(summaries)) => res.render(Json(summaries)),
        Ok(Err(error)) => render_internal_error(res, error),
        Err(error) => render_internal_error(res, error),
    }
}
