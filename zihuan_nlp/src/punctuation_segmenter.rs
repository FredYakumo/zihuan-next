#[derive(Default)]
pub struct PunctuationSegmenter;

const STRONG_SEPARATORS: [char; 11] = ['\n', '。', '！', '？', '；', '：', '.', '!', '?', ';', ':'];
const WEAK_SEPARATORS: [char; 4] = ['，', ',', ' ', '\t'];

impl crate::TextSegmenter for PunctuationSegmenter {
    fn segment(&self, text: &str, max_chars: usize) -> Vec<String> {
        split_text_by_punctuation(text, max_chars)
    }
}

pub fn split_text_by_punctuation(text: &str, max_chars: usize) -> Vec<String> {
    let max_chars = max_chars.max(1);
    if text.is_empty() {
        return Vec::new();
    }

    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_chars {
        return vec![text.to_string()];
    }

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

        let split_end = find_split_end(&chars, start, hard_end);
        let chunk = chars[start..split_end].iter().collect::<String>().trim().to_string();

        if chunk.is_empty() {
            let fallback_end = (start + max_chars).min(chars.len());
            let fallback_chunk = chars[start..fallback_end].iter().collect::<String>().trim().to_string();
            if !fallback_chunk.is_empty() {
                chunks.push(fallback_chunk);
            }
            start = fallback_end;
        } else {
            chunks.push(chunk);
            start = split_end;
        }
    }

    chunks
}

fn find_split_end(chars: &[char], start: usize, hard_end: usize) -> usize {
    if hard_end <= start {
        return start;
    }

    let min_split_index = start + (hard_end - start) * 2 / 3;

    if let Some(boundary) = find_split_boundary_from_right(chars, start, hard_end, min_split_index, &STRONG_SEPARATORS)
    {
        return boundary;
    }

    if let Some(boundary) = find_split_boundary_from_right(chars, start, hard_end, min_split_index, &WEAK_SEPARATORS) {
        return boundary;
    }

    hard_end
}

fn find_split_boundary_from_right(
    chars: &[char],
    start: usize,
    hard_end: usize,
    min_split_index: usize,
    separators: &[char],
) -> Option<usize> {
    (min_split_index..hard_end)
        .rev()
        .find(|&idx| separators.contains(&chars[idx]) && is_safe_split_separator(chars, idx))
        .map(|idx| idx + 1)
        .filter(|&boundary| boundary > start)
}

fn is_safe_split_separator(chars: &[char], separator_index: usize) -> bool {
    let separator = chars[separator_index];

    match separator {
        '.' => {
            let prev_is_digit = separator_index > 0 && chars[separator_index - 1].is_ascii_digit();
            let next_is_digit = separator_index + 1 < chars.len() && chars[separator_index + 1].is_ascii_digit();
            !(prev_is_digit && next_is_digit)
        }
        ':' => {
            let prev_is_digit = separator_index > 0 && chars[separator_index - 1].is_ascii_digit();
            let next_is_digit = separator_index + 1 < chars.len() && chars[separator_index + 1].is_ascii_digit();
            if prev_is_digit && next_is_digit {
                return false;
            }

            let prev_is_protocol = separator_index + 2 < chars.len()
                && chars[separator_index + 1] == '/'
                && chars[separator_index + 2] == '/';
            !prev_is_protocol
        }
        ',' => {
            let prev_is_digit = separator_index > 0 && chars[separator_index - 1].is_ascii_digit();
            let next_is_digit = separator_index + 1 < chars.len() && chars[separator_index + 1].is_ascii_digit();
            !(prev_is_digit && next_is_digit)
        }
        _ => true,
    }
}
