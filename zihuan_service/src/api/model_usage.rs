use chrono::{DateTime, Duration, Utc};
use salvo::prelude::*;
use salvo::writing::Json;
use serde::Serialize;
use zihuan_core::model_inference::model_usage::{load_usage, ModelUsageRecord};

use crate::api::config::render_internal_error;

#[derive(Default, Serialize)]
struct ModelUsageSummary {
    config_id: String,
    last_used_at: Option<String>,
    input_tokens: u64,
    output_tokens: u64,
    cached_input_tokens: u64,
    total_tokens: u64,
    average_cache_hit_rate: Option<f64>,
    average_tokens_per_second: Option<f64>,
    average_first_token_ms: Option<f64>,
    recent_records: Vec<ModelUsageRecord>,
}

fn average(values: impl Iterator<Item = f64>) -> Option<f64> {
    let values: Vec<_> = values.collect();
    (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
}

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
        let models = crate::system_config::load_llm_refs()?;
        let records = load_usage()?;
        let cutoff = Utc::now() - Duration::days(32);
        let summaries = models
            .into_iter()
            .map(|model| {
                let config_id = model.canonical_config_id().to_string();
                let matching: Vec<_> = records
                    .iter()
                    .filter(|record| {
                        record.config_id == config_id
                            && range.is_none_or(|(start, end)| {
                                record.used_at >= start && record.used_at < end
                            })
                    })
                    .collect();
                let mut summary = ModelUsageSummary {
                    config_id,
                    last_used_at: matching
                        .iter()
                        .map(|record| record.used_at)
                        .max()
                        .map(|date| date.to_rfc3339()),
                    average_cache_hit_rate: average(matching.iter().filter_map(|record| {
                        let usage = record.usage.as_ref()?;
                        let input = usage.prompt_tokens?;
                        let cached = usage.cached_prompt_tokens?;
                        (input > 0).then(|| cached.min(input) as f64 / input as f64)
                    })),
                    average_tokens_per_second: average(
                        matching.iter().filter_map(|record| record.output_tokens_per_second),
                    ),
                    average_first_token_ms: average(
                        matching.iter().filter_map(|record| record.time_to_first_token_ms),
                    ),
                    ..Default::default()
                };
                for record in matching {
                    if let Some(usage) = &record.usage {
                        summary.input_tokens += usage.prompt_tokens.unwrap_or(0) as u64;
                        summary.output_tokens += usage.completion_tokens.unwrap_or(0) as u64;
                        summary.cached_input_tokens +=
                            usage.cached_prompt_tokens.unwrap_or(0) as u64;
                        summary.total_tokens += usage.total_tokens.unwrap_or_else(|| {
                            usage.prompt_tokens.unwrap_or(0) + usage.completion_tokens.unwrap_or(0)
                        }) as u64;
                    }
                    if range.is_some() || record.used_at >= cutoff {
                        summary.recent_records.push(ModelUsageRecord {
                            config_id: record.config_id.clone(),
                            used_at: record.used_at,
                            usage: record.usage.clone(),
                            output_tokens_per_second: record.output_tokens_per_second,
                            time_to_first_token_ms: record.time_to_first_token_ms,
                        });
                    }
                }
                summary
            })
            .collect::<Vec<_>>();
        Ok::<_, zihuan_core::error::Error>(summaries)
    })
    .await;
    match result {
        Ok(Ok(summaries)) => res.render(Json(summaries)),
        Ok(Err(error)) => render_internal_error(res, error),
        Err(error) => render_internal_error(res, error),
    }
}
