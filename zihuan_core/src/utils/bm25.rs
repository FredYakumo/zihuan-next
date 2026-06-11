use std::collections::{HashMap, HashSet};

const BM25_K1: f64 = 1.5;
const BM25_B: f64 = 0.75;
const MAX_ASCII_NGRAM: usize = 8;

#[derive(Debug, Clone, PartialEq)]
pub struct Bm25Match {
    pub index: usize,
    pub score: f64,
}

pub fn rank_bm25_matches(query: &str, documents: &[String]) -> Vec<Bm25Match> {
    let query_tokens = tokenize_bm25_text(query);
    if query_tokens.is_empty() || documents.is_empty() {
        return Vec::new();
    }

    let corpus_tokens: Vec<Vec<String>> = documents.iter().map(|value| tokenize_bm25_text(value)).collect();
    let scores = bm25_scores(&query_tokens, &corpus_tokens);

    let mut matches: Vec<Bm25Match> = scores
        .into_iter()
        .enumerate()
        .filter_map(|(index, score)| {
            if score.is_finite() && score > 0.0 {
                Some(Bm25Match { index, score })
            } else {
                None
            }
        })
        .collect();

    matches.sort_by(|left, right| right.score.total_cmp(&left.score).then_with(|| left.index.cmp(&right.index)));
    matches
}

pub fn tokenize_bm25_text(text: &str) -> Vec<String> {
    let normalized = normalize_bm25_text(text);
    if normalized.is_empty() {
        return Vec::new();
    }

    let mut tokens = Vec::new();
    let mut ascii_run = String::new();
    for ch in normalized.chars() {
        if ch.is_ascii_alphanumeric() {
            ascii_run.push(ch);
            continue;
        }

        flush_ascii_run(&mut ascii_run, &mut tokens);
        if !ch.is_whitespace() {
            tokens.push(ch.to_string());
        }
    }
    flush_ascii_run(&mut ascii_run, &mut tokens);
    tokens
}

pub fn normalize_bm25_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

fn flush_ascii_run(run: &mut String, tokens: &mut Vec<String>) {
    if run.is_empty() {
        return;
    }

    let current = std::mem::take(run);
    tokens.push(current.clone());

    let chars: Vec<char> = current.chars().collect();
    let max_ngram = chars.len().min(MAX_ASCII_NGRAM);
    for window_len in 1..=max_ngram {
        for start in 0..=chars.len() - window_len {
            tokens.push(chars[start..start + window_len].iter().collect());
        }
    }
}

fn bm25_scores(query_tokens: &[String], corpus_tokens: &[Vec<String>]) -> Vec<f64> {
    if query_tokens.is_empty() || corpus_tokens.is_empty() {
        return vec![0.0; corpus_tokens.len()];
    }

    let doc_count = corpus_tokens.len() as f64;
    let avg_doc_len = corpus_tokens.iter().map(|tokens| tokens.len() as f64).sum::<f64>() / doc_count.max(1.0);

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
                let denominator = frequency + BM25_K1 * (1.0 - BM25_B + BM25_B * (doc_len / avg_doc_len.max(1.0)));
                acc + idf * (frequency * (BM25_K1 + 1.0)) / denominator
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{rank_bm25_matches, tokenize_bm25_text};

    #[test]
    fn tokenization_keeps_cjk_chars_and_ascii_ngrams() {
        let tokens = tokenize_bm25_text("Alice 张三 123456");
        assert!(tokens.contains(&"alice".to_string()));
        assert!(tokens.contains(&"ali".to_string()));
        assert!(tokens.contains(&"张".to_string()));
        assert!(tokens.contains(&"三".to_string()));
        assert!(tokens.contains(&"3456".to_string()));
    }

    #[test]
    fn rank_matches_supports_partial_numeric_keyword() {
        let docs = vec!["123456789".to_string(), "987654321".to_string()];
        let matches = rank_bm25_matches("3456", &docs);
        assert_eq!(matches.first().map(|item| item.index), Some(0));
    }
}
