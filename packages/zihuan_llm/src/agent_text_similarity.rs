use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use general_wheel_cpp::cosine_similarity;
use zihuan_core::error::Result;
use zihuan_llm_types::embedding_base::EmbeddingBase;

const BM25_K1: f64 = 1.5;
const BM25_B: f64 = 0.75;

#[derive(Debug, Clone)]
pub struct SimilarityCandidate {
    pub source: String,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct SimilarityMatch {
    pub source: String,
    pub text: String,
    pub bm25_score: f64,
    pub bm25_normalized: f64,
    pub cosine_score: Option<f64>,
    pub hybrid_score: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct HybridSimilarityConfig {
    pub bm25_weight: f64,
    pub cosine_weight: f64,
}

impl Default for HybridSimilarityConfig {
    fn default() -> Self {
        Self {
            bm25_weight: 0.45,
            cosine_weight: 0.55,
        }
    }
}

pub fn find_best_match(
    query: &str,
    candidates: &[SimilarityCandidate],
    embedding_model: Option<&Arc<dyn EmbeddingBase>>,
    config: HybridSimilarityConfig,
) -> Result<Option<SimilarityMatch>> {
    let normalized_query = normalize_similarity_text(query);
    if normalized_query.is_empty() || candidates.is_empty() {
        return Ok(None);
    }

    let prepared: Vec<_> = candidates
        .iter()
        .filter_map(|candidate| {
            let normalized = normalize_similarity_text(&candidate.text);
            if normalized.is_empty() {
                None
            } else {
                Some((candidate, normalized))
            }
        })
        .collect();
    if prepared.is_empty() {
        return Ok(None);
    }

    let corpus_tokens: Vec<Vec<String>> = prepared
        .iter()
        .map(|(_, text)| tokenize_for_bm25(text))
        .collect();
    let query_tokens = tokenize_for_bm25(&normalized_query);
    let bm25_scores = bm25_scores(&query_tokens, &corpus_tokens);
    let max_bm25 = bm25_scores.iter().copied().fold(0.0_f64, f64::max);

    let cosine_scores = if let Some(model) = embedding_model {
        let mut texts = Vec::with_capacity(prepared.len() + 1);
        texts.push(normalized_query.clone());
        texts.extend(prepared.iter().map(|(_, text)| text.clone()));
        let embeddings = model.batch_inference(&texts)?;
        if embeddings.len() == prepared.len() + 1 {
            let query_embedding = &embeddings[0];
            Some(
                embeddings[1..]
                    .iter()
                    .map(|embedding| {
                        cosine_similarity(query_embedding, embedding).unwrap_or(0.0) as f64
                    })
                    .collect::<Vec<_>>(),
            )
        } else {
            None
        }
    } else {
        None
    };

    let mut best: Option<SimilarityMatch> = None;
    for (index, (candidate, normalized_text)) in prepared.iter().enumerate() {
        let bm25_score = bm25_scores.get(index).copied().unwrap_or_default();
        let bm25_normalized = if max_bm25 > 0.0 {
            (bm25_score / max_bm25).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let cosine_score = cosine_scores
            .as_ref()
            .and_then(|scores| scores.get(index).copied());
        let hybrid_score = if let Some(cosine) = cosine_score {
            config.bm25_weight * bm25_normalized + config.cosine_weight * cosine.clamp(0.0, 1.0)
        } else {
            bm25_normalized
        };

        let current = SimilarityMatch {
            source: candidate.source.clone(),
            text: normalized_text.clone(),
            bm25_score,
            bm25_normalized,
            cosine_score,
            hybrid_score,
        };

        let should_replace = best
            .as_ref()
            .map(|prev| {
                current.hybrid_score > prev.hybrid_score
                    || (current.hybrid_score == prev.hybrid_score
                        && current.bm25_score > prev.bm25_score)
            })
            .unwrap_or(true);
        if should_replace {
            best = Some(current);
        }
    }

    Ok(best)
}

pub fn normalize_similarity_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn tokenize_for_bm25(text: &str) -> Vec<String> {
    let normalized = normalize_similarity_text(text).to_lowercase();
    if normalized.is_empty() {
        return Vec::new();
    }

    let mut tokens = Vec::new();
    let mut current = String::new();

    for ch in normalized.chars() {
        if ch.is_ascii_alphanumeric() {
            current.push(ch);
            continue;
        }

        if !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }

        if !ch.is_whitespace() {
            tokens.push(ch.to_string());
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

fn bm25_scores(query_tokens: &[String], corpus_tokens: &[Vec<String>]) -> Vec<f64> {
    if query_tokens.is_empty() || corpus_tokens.is_empty() {
        return vec![0.0; corpus_tokens.len()];
    }

    let doc_count = corpus_tokens.len() as f64;
    let avg_doc_len = corpus_tokens
        .iter()
        .map(|tokens| tokens.len() as f64)
        .sum::<f64>()
        / doc_count.max(1.0);

    let mut doc_freq: HashMap<&str, usize> = HashMap::new();
    for doc_tokens in corpus_tokens {
        let unique: HashSet<&str> = doc_tokens.iter().map(String::as_str).collect();
        for token in unique {
            *doc_freq.entry(token).or_default() += 1;
        }
    }

    corpus_tokens
        .iter()
        .map(|doc_tokens| {
            let mut tf: HashMap<&str, usize> = HashMap::new();
            for token in doc_tokens {
                *tf.entry(token.as_str()).or_default() += 1;
            }

            let doc_len = doc_tokens.len() as f64;
            query_tokens.iter().fold(0.0, |acc, token| {
                let frequency = tf.get(token.as_str()).copied().unwrap_or_default() as f64;
                if frequency == 0.0 {
                    return acc;
                }

                let df = doc_freq.get(token.as_str()).copied().unwrap_or_default() as f64;
                let idf = (((doc_count - df + 0.5) / (df + 0.5)) + 1.0).ln();
                let denominator = frequency
                    + BM25_K1 * (1.0 - BM25_B + BM25_B * (doc_len / avg_doc_len.max(1.0)));
                acc + idf * (frequency * (BM25_K1 + 1.0)) / denominator
            })
        })
        .collect()
}
