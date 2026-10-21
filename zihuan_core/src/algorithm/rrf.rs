use std::collections::{HashMap, HashSet};
use std::hash::Hash;

/// Rank constant from the original reciprocal rank fusion paper.
pub const DEFAULT_RRF_RANK_CONSTANT: f64 = 60.0;

/// Fuses multiple ranked result lists into a single ranking.
///
/// Each input list is one retrieval's results ordered best-first. An item's
/// fused score is the sum of `1 / (rank_constant + rank)` over every list that
/// returned it, with `rank` starting at 1. Only the position within a list
/// matters, so lists produced by incomparable scoring schemes can be combined
/// without normalizing their raw scores.
///
/// Duplicate entries inside one list count once, at their first position. Ties
/// in the fused score keep the order in which items were first encountered.
pub fn reciprocal_rank_fusion<T>(rankings: &[&[T]], rank_constant: f64) -> Vec<(T, f64)>
where
    T: Clone + Eq + Hash,
{
    let rank_constant = if rank_constant.is_finite() {
        rank_constant.max(0.0)
    } else {
        DEFAULT_RRF_RANK_CONSTANT
    };

    let mut scores: HashMap<T, f64> = HashMap::new();
    let mut first_seen: HashMap<T, usize> = HashMap::new();
    let mut next_index = 0usize;

    for ranking in rankings {
        let mut seen: HashSet<&T> = HashSet::new();
        for (position, item) in ranking.iter().enumerate() {
            if !seen.insert(item) {
                continue;
            }
            if !first_seen.contains_key(item) {
                first_seen.insert(item.clone(), next_index);
                next_index += 1;
            }
            *scores.entry(item.clone()).or_default() +=
                1.0 / (rank_constant + (position + 1) as f64);
        }
    }

    let mut fused = scores.into_iter().collect::<Vec<_>>();
    fused.sort_by(|(left_item, left_score), (right_item, right_score)| {
        right_score
            .total_cmp(left_score)
            .then_with(|| first_seen[left_item].cmp(&first_seen[right_item]))
    });
    fused
}
