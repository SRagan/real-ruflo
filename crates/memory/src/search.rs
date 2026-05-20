//! Search modes + fusion. Designed so the public API is "just call search,"
//! and the system picks the right mix.

use serde::{Deserialize, Serialize};

/// Which search backend(s) to use.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchMode {
    /// Vector cosine similarity. Requires embeddings on stored entries and
    /// an embedding for the query.
    Vector,
    /// FTS5 lexical search. Always available.
    Lexical,
    /// Run both, fuse with Reciprocal Rank Fusion. Falls back gracefully:
    /// no query embedding → lexical only; no stored embeddings → lexical only.
    #[default]
    Hybrid,
}

/// Search request. Defaults are designed to "just work" — call with a query,
/// get sensible results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchRequest {
    /// Free-text query. Used for lexical search and (if no embedding given)
    /// passed to the configured [`crate::embed::Embedder`] for vector search.
    pub query: String,

    /// Pre-computed query embedding. Takes precedence over the embedder.
    #[serde(default)]
    pub embedding: Option<Vec<f32>>,

    /// Restrict to a namespace. None = search all.
    #[serde(default)]
    pub namespace: Option<String>,

    /// Restrict to entries that contain ALL listed tags.
    #[serde(default)]
    pub tags: Vec<String>,

    /// Maximum results returned (after fusion).
    #[serde(default = "default_limit")]
    pub limit: usize,

    /// Mode override. Default is hybrid with graceful fallback.
    #[serde(default)]
    pub mode: SearchMode,
}

fn default_limit() -> usize {
    10
}

/// Reciprocal Rank Fusion. Given two ranked lists of entry IDs, produce a
/// fused ranking. `k` is the RRF constant (60 is canonical).
pub fn rrf(lists: &[&[i64]], k: f32) -> Vec<(i64, f32)> {
    use std::collections::HashMap;
    let mut scores: HashMap<i64, f32> = HashMap::new();
    for list in lists {
        for (rank, &id) in list.iter().enumerate() {
            *scores.entry(id).or_insert(0.0) += 1.0 / (k + (rank as f32) + 1.0);
        }
    }
    let mut out: Vec<(i64, f32)> = scores.into_iter().collect();
    out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rrf_combines_two_rankings() {
        // id 10 is top in both lists → must be the top result.
        let a = vec![10i64, 20, 30];
        let b = vec![10i64, 30, 20];
        let fused = rrf(&[&a, &b], 60.0);
        assert_eq!(fused[0].0, 10);
        // All three ids must appear in the fused output.
        let ids: std::collections::HashSet<i64> = fused.iter().map(|(id, _)| *id).collect();
        assert!(ids.contains(&10));
        assert!(ids.contains(&20));
        assert!(ids.contains(&30));
    }

    #[test]
    fn rrf_score_is_higher_for_better_ranks() {
        // Single list: rank 0 must outscore rank 2.
        let only = vec![100i64, 200, 300];
        let fused = rrf(&[&only], 60.0);
        // fused is sorted by score desc, so first should be id 100.
        assert_eq!(fused[0].0, 100);
        assert!(fused[0].1 > fused[2].1);
    }
}
