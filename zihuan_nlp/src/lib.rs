mod bpe_segmenter;
mod punctuation_segmenter;

use std::path::Path;
use std::sync::Arc;

use log::warn;

pub use bpe_segmenter::BpeSegmenter;
pub use punctuation_segmenter::PunctuationSegmenter;

pub trait TextSegmenter: Send + Sync {
    fn segment(&self, text: &str, max_chars: usize) -> Vec<String>;
}

pub fn build_segmenter(tokenizer_path: Option<&Path>) -> Arc<dyn TextSegmenter> {
    if let Some(path) = tokenizer_path {
        match BpeSegmenter::try_new(path) {
            Ok(segmenter) => return Arc::new(segmenter),
            Err(err) => {
                warn!(
                    "[zihuan_nlp] failed to initialize BPE tokenizer segmenter from '{}': {}; fallback to punctuation segmenter",
                    path.display(),
                    err
                );
            }
        }
    }
    Arc::new(PunctuationSegmenter::default())
}
