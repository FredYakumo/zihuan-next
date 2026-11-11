use std::path::{Path, PathBuf};

use crate::connection::Connection;
use crate::error::Result;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, RwLock,
};
use tokenizers::Tokenizer;

use crate::nlp::punctuation_segmenter::split_text_by_punctuation;

pub struct BpeSegmenter {
    tokenizer_path: PathBuf,
    tokenizer: RwLock<Option<Arc<Tokenizer>>>,
    closed: AtomicBool,
}

impl BpeSegmenter {
    pub fn try_new(tokenizer_path: &Path) -> Result<Self> {
        if !tokenizer_path.is_file() {
            return Err(crate::string_error!(
                "tokenizer file not found: {}",
                tokenizer_path.display()
            ));
        }
        Ok(Self {
            tokenizer_path: tokenizer_path.to_path_buf(),
            tokenizer: RwLock::new(None),
            closed: AtomicBool::new(false),
        })
    }

    fn tokenizer(&self) -> Result<Arc<Tokenizer>> {
        if self.is_closed() {
            return Err(crate::string_error!("Tokenizer connection is closed"));
        }
        let mut current = self.tokenizer.write().unwrap_or_else(|e| e.into_inner());
        if self.is_closed() {
            return Err(crate::string_error!("Tokenizer connection is closed"));
        }
        if current.is_none() {
            let tokenizer = Tokenizer::from_file(&self.tokenizer_path).map_err(|error| {
                crate::string_error!(
                    "failed to load tokenizer '{}': {error}",
                    self.tokenizer_path.display()
                )
            })?;
            *current = Some(Arc::new(tokenizer));
        }
        Ok(current.as_ref().unwrap().clone())
    }
}

impl crate::nlp::TextSegmenter for BpeSegmenter {
    fn segment(&self, text: &str, max_chars: usize) -> Vec<String> {
        if self.is_closed() {
            return split_text_by_punctuation(text, max_chars.max(1));
        }
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

        let token_end_char_positions: Vec<usize> =
            encoding.get_offsets().iter().map(|(_, end)| to_char_index(*end)).collect();

        let chars: Vec<char> = text.chars().collect();
        let mut chunks = Vec::new();
        let mut start = 0usize;
        while start < chars.len() {
            let hard_end = (start + max_chars).min(chars.len());
            if hard_end == chars.len() {
                let chunk = chars[start..hard_end].iter().collect::<String>().trim().to_string();
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
            let chunk = chars[start..split_end].iter().collect::<String>().trim().to_string();
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

#[async_trait::async_trait]
impl Connection for BpeSegmenter {
    async fn connect(&mut self) -> Result<()> {
        self.closed.store(false, Ordering::Release);
        if let Err(error) = self.health_check().await {
            self.close().await?;
            return Err(error);
        }
        Ok(())
    }

    async fn health_check(&self) -> Result<()> {
        self.tokenizer().map(|_| ())
    }

    async fn close(&self) -> Result<()> {
        self.closed.store(true, Ordering::Release);
        self.tokenizer.write().unwrap_or_else(|e| e.into_inner()).take();
        Ok(())
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }
}

impl BpeSegmenter {
    pub async fn from_config(config: &crate::storage::TokenizerConnection) -> Result<Self> {
        let path = Path::new("models/tokenizer")
            .join(config.model_name.trim())
            .join("tokenizer.json");
        let mut connection = Self::try_new(&path)?;
        connection.connect().await?;
        Ok(connection)
    }
}
