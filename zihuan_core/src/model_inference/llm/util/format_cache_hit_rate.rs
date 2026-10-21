pub fn format_cache_hit_rate(
    cached_prompt_tokens: Option<usize>,
    prompt_tokens: Option<usize>,
) -> String {
    match (cached_prompt_tokens, prompt_tokens) {
        (Some(cached), Some(prompt)) if prompt > 0 => {
            format!("{:.2}%", (cached as f64 / prompt as f64) * 100.0)
        }
        _ => "unavailable".to_string(),
    }
}
