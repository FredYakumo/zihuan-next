use std::path::{Path, PathBuf};

use once_cell::sync::OnceCell;
use tokenizers::Tokenizer;
use zihuan_core::error::Result;

use crate::punctuation_segmenter::split_text_by_punctuation;

pub struct BpeSegmenter {
    tokenizer_path: PathBuf,
    tokenizer: OnceCell<Tokenizer>,
}

impl BpeSegmenter {
    pub fn try_new(tokenizer_path: &Path) -> Result<Self> {
        if !tokenizer_path.is_file() {
            return Err(zihuan_core::string_error!(
                "tokenizer file not found: {}",
                tokenizer_path.display()
            ));
        }
        Ok(Self {
            tokenizer_path: tokenizer_path.to_path_buf(),
            tokenizer: OnceCell::new(),
        })
    }

    fn tokenizer(&self) -> Result<&Tokenizer> {
        self.tokenizer.get_or_try_init(|| {
            Tokenizer::from_file(&self.tokenizer_path)
                .map_err(|err| {
                    zihuan_core::string_error!(
                        "failed to load tokenizer from '{}': {}",
                        self.tokenizer_path.display(),
                        err
                    )
                })
                .map_err(Into::into)
        })
    }
}

impl crate::TextSegmenter for BpeSegmenter {
    fn segment(&self, text: &str, max_chars: usize) -> Vec<String> {
        let max_chars = max_chars.max(1);
        if text.is_empty() {
            return Vec::new();
        }
        if text.chars().count() <= max_chars {
            return vec![text.to_string()];
        }

        let tokenizer = match self.tokenizer() {
            Ok(tokenizer) => tokenizer,
            Err(_) => return split_text_by_punctuation(text, max_chars),
        };

        let encoding = match tokenizer.encode(text, false) {
            Ok(encoding) => encoding,
            Err(_) => return split_text_by_punctuation(text, max_chars),
        };

        let mut byte_to_char: Vec<(usize, usize)> = text
            .char_indices()
            .enumerate()
            .map(|(char_idx, (byte_idx, _))| (byte_idx, char_idx))
            .collect();
        byte_to_char.push((text.len(), text.chars().count()));

        let to_char_index = |byte_pos: usize| -> usize {
            match byte_to_char.binary_search_by_key(&byte_pos, |(byte, _)| *byte) {
                Ok(idx) => byte_to_char[idx].1,
                Err(idx) => {
                    if idx == 0 {
                        0
                    } else {
                        byte_to_char[idx - 1].1
                    }
                }
            }
        };

        let token_end_char_positions: Vec<usize> = encoding
            .get_offsets()
            .iter()
            .map(|(_, end)| to_char_index(*end))
            .collect();

        let chars: Vec<char> = text.chars().collect();
        let mut chunks = Vec::new();
        let mut start = 0usize;
        while start < chars.len() {
            let hard_end = (start + max_chars).min(chars.len());
            if hard_end == chars.len() {
                let chunk = chars[start..hard_end]
                    .iter()
                    .collect::<String>()
                    .trim()
                    .to_string();
                if !chunk.is_empty() {
                    chunks.push(chunk);
                }
                break;
            }

            let min_split_index = start + (hard_end - start) * 2 / 3;
            let token_boundary = token_end_char_positions
                .iter()
                .copied()
                .filter(|idx| *idx > start && *idx <= hard_end)
                .filter(|idx| *idx >= min_split_index)
                .max();

            let split_end = token_boundary.unwrap_or(hard_end);
            let chunk = chars[start..split_end]
                .iter()
                .collect::<String>()
                .trim()
                .to_string();
            if chunk.is_empty() {
                start = hard_end;
                continue;
            }
            chunks.push(chunk);
            start = split_end;
        }

        if chunks.is_empty() {
            split_text_by_punctuation(text, max_chars)
        } else {
            chunks
        }
    }
}
