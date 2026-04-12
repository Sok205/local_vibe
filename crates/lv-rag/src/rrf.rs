//! Reciprocal Rank Fusion: merges multiple ranked lists by summing
//! 1 / (k + rank) contributions across all lists each item appears in.
//!
//! Canonical k = 60 (Cormack et al., SIGIR '09).

use std::collections::HashMap;

const RRF_K: f32 = 60.0;

/// Fuse multiple ranked id lists into a single ranking.
///
/// Each input is a slice of ids in order (best first). Returns `(id, score)`
/// pairs sorted by descending fused score. Duplicate ids within a single
/// list are ignored (only the first occurrence counts).
pub fn fuse(ranked_lists: &[Vec<String>]) -> Vec<(String, f32)> {
    let mut scores: HashMap<String, f32> = HashMap::new();
    for list in ranked_lists {
        let mut seen_in_list: std::collections::HashSet<&str> =
            std::collections::HashSet::new();
        for (rank, id) in list.iter().enumerate() {
            if !seen_in_list.insert(id.as_str()) {
                continue;
            }
            let delta = 1.0 / (RRF_K + rank as f32 + 1.0);
            *scores.entry(id.clone()).or_insert(0.0) += delta;
        }
    }
    let mut out: Vec<(String, f32)> = scores.into_iter().collect();
    out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_list_preserves_order() {
        let fused = fuse(&[vec!["a".into(), "b".into(), "c".into()]]);
        let ids: Vec<&str> = fused.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b", "c"]);
    }

    #[test]
    fn two_lists_overlap_wins() {
        // "b" appears high in both -> should beat "a" (only in list 1) and "c" (only in list 2).
        let fused = fuse(&[
            vec!["a".into(), "b".into(), "x".into()],
            vec!["c".into(), "b".into(), "y".into()],
        ]);
        assert_eq!(fused[0].0, "b");
    }

    #[test]
    fn item_only_in_one_list_still_ranks() {
        let fused = fuse(&[
            vec!["a".into()],
            vec!["b".into()],
        ]);
        // Both at rank 0 -> tied.
        assert_eq!(fused.len(), 2);
        assert!((fused[0].1 - fused[1].1).abs() < 1e-6);
    }

    #[test]
    fn duplicate_within_list_ignored() {
        let fused = fuse(&[vec!["a".into(), "a".into(), "b".into()]]);
        let a_score = fused.iter().find(|(id, _)| id == "a").unwrap().1;
        let b_score = fused.iter().find(|(id, _)| id == "b").unwrap().1;
        // a at rank 0 only (duplicate ignored) -> 1/61
        // b at rank 2 -> 1/63 (original position in the list survives dedup)
        assert!(a_score > b_score);
    }
}
