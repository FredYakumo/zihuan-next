//! Scan state for text that is being split into chunks.
//!
//! A segmenter cuts text wherever a chunk boundary fits; it does not know about markdown
//! code fences or quotation marks. Feeding each produced chunk through
//! [`analyze_chunk_state`] tracks whether the split landed inside a fenced block or a quote,
//! so the caller can close (and later re-open) them at the chunk edges instead of leaving
//! dangling fences across messages.

/// Fence and quote state carried across the chunks of one split.
#[derive(Debug, Clone, Copy, Default)]
pub struct SplitRepairState {
    pub in_code_fence: bool,
    pub in_double_quote: bool,
    pub in_cn_quote: bool,
}

/// Folds one chunk into `state`, returning the state after it.
///
/// A run of three or more backticks toggles the code-fence state; inside a fence every
/// other character is skipped, so quotes in code do not disturb the quote tracking.
pub fn analyze_chunk_state(chunk: &str, mut state: SplitRepairState) -> SplitRepairState {
    let mut iter = chunk.chars().peekable();
    while let Some(ch) = iter.next() {
        if ch == '`' {
            let mut count = 1usize;
            while matches!(iter.peek(), Some('`')) {
                iter.next();
                count += 1;
            }
            if count >= 3 {
                state.in_code_fence = !state.in_code_fence;
                continue;
            }
        }

        if state.in_code_fence {
            continue;
        }

        match ch {
            '"' => state.in_double_quote = !state.in_double_quote,
            '“' => state.in_cn_quote = true,
            '”' => state.in_cn_quote = false,
            _ => {}
        }
    }
    state
}
