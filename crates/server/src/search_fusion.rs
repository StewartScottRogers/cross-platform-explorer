//! Search result fusion (CPE-984, epic CPE-976): blend the **semantic** hits from
//! [`crate::semantic_index`] with the **lexical** hits (filename / [`crate::index_query`]) into one ranked
//! list.
//!
//! Semantic scores (cosine, ~`[0,1]`) and lexical scores (term/prefix points) live on incomparable scales,
//! so blending their raw magnitudes is guesswork. Instead this uses **Reciprocal Rank Fusion (RRF)** — each
//! result scores `Σ 1/(k + rank)` over the source rankings it appears in. It fuses *positions*, not
//! magnitudes, so it's scale-free, order-only, and robust: a document that ranks well in *both* sources
//! rises above one that's strong in only a single source. Pure + std-only.

use crate::semantic_index::DocHit;

/// The conventional RRF constant (Cormack et al.). Larger `k` flattens the contribution of top ranks.
pub const DEFAULT_RRF_K: f32 = 60.0;

/// One fused result from [`rrf`]: an id and its combined RRF score (higher = better).
#[derive(Debug, Clone, PartialEq)]
pub struct Fused {
    pub id: String,
    pub score: f32,
}

/// A blended search hit with provenance: the id, its fused score, and which source(s) it came from — so the
/// UI can badge *why* it matched (meaning, name, or both).
#[derive(Debug, Clone, PartialEq)]
pub struct BlendedHit {
    pub id: String,
    pub score: f32,
    pub in_semantic: bool,
    pub in_lexical: bool,
}

/// Reciprocal Rank Fusion over several ranked id lists (each best-first). An id's score is the sum of
/// `1/(k + rank)` across every ranking it appears in (rank is 0-based). Returned best-first with a
/// deterministic tiebreak (score desc, then id ascending). A non-positive `k` is clamped to a tiny positive
/// value to avoid division blow-ups.
pub fn rrf(rankings: &[Vec<String>], k: f32) -> Vec<Fused> {
    let k = if k > 0.0 { k } else { f32::EPSILON };
    let mut scores: std::collections::BTreeMap<String, f32> = std::collections::BTreeMap::new();
    for ranking in rankings {
        for (rank, id) in ranking.iter().enumerate() {
            *scores.entry(id.clone()).or_insert(0.0) += 1.0 / (k + rank as f32);
        }
    }
    let mut fused: Vec<Fused> = scores.into_iter().map(|(id, score)| Fused { id, score }).collect();
    fused.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.id.cmp(&b.id))
    });
    fused
}

/// Blend the semantic + lexical search results into one ranked list via [`rrf`] (with [`DEFAULT_RRF_K`]),
/// tagging each hit with the source(s) it came from. `semantic` is best-first `DocHit`s (only their id order
/// matters to the fusion); `lexical` is a best-first list of ids/paths.
pub fn blend_semantic_lexical(semantic: &[DocHit], lexical: &[String]) -> Vec<BlendedHit> {
    let sem_ids: Vec<String> = semantic.iter().map(|h| h.doc_id.clone()).collect();
    let fused = rrf(&[sem_ids.clone(), lexical.to_vec()], DEFAULT_RRF_K);

    let sem_set: std::collections::BTreeSet<&String> = sem_ids.iter().collect();
    let lex_set: std::collections::BTreeSet<&String> = lexical.iter().collect();
    fused
        .into_iter()
        .map(|f| BlendedHit {
            in_semantic: sem_set.contains(&f.id),
            in_lexical: lex_set.contains(&f.id),
            id: f.id,
            score: f.score,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn dochits(v: &[&str]) -> Vec<DocHit> {
        v.iter().enumerate().map(|(i, s)| DocHit { doc_id: s.to_string(), score: 1.0 - i as f32 * 0.1 }).collect()
    }

    #[test]
    fn agreement_across_sources_wins() {
        // "a" is rank 0 in both → highest; then the others.
        let out = rrf(&[ids(&["a", "b", "c"]), ids(&["a", "c", "b"])], DEFAULT_RRF_K);
        assert_eq!(out[0].id, "a");
        // b and c each appear once at rank1 and once at rank2 → equal → id tiebreak b before c.
        assert_eq!(out.iter().map(|f| f.id.as_str()).collect::<Vec<_>>(), vec!["a", "b", "c"]);
    }

    #[test]
    fn top_of_two_beats_top_of_one() {
        // "both" is rank0 in two lists; "solo" is rank0 in one → both outranks solo.
        let out = rrf(&[ids(&["both", "x"]), ids(&["both", "solo"]), ids(&["solo"])], DEFAULT_RRF_K);
        let order: Vec<&str> = out.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(order[0], "both");
        // "solo" (rank0 in list2's tail + rank0 in list3) vs "x" (rank1 in list1) — solo has two hits.
        assert!(order.iter().position(|&r| r == "solo") < order.iter().position(|&r| r == "x"));
    }

    #[test]
    fn deterministic_tiebreak_by_id() {
        // Perfectly symmetric: x@0,y@1 and y@0,x@1 → equal scores → id ascending.
        let out = rrf(&[ids(&["x", "y"]), ids(&["y", "x"])], DEFAULT_RRF_K);
        assert_eq!(out.iter().map(|f| f.id.as_str()).collect::<Vec<_>>(), vec!["x", "y"]);
        assert!((out[0].score - out[1].score).abs() < 1e-9);
    }

    #[test]
    fn blend_tags_provenance() {
        let semantic = dochits(&["shared", "sem_only"]);
        let lexical = ids(&["shared", "lex_only"]);
        let out = blend_semantic_lexical(&semantic, &lexical);
        // "shared" is in both sources and ranks first.
        assert_eq!(out[0].id, "shared");
        assert!(out[0].in_semantic && out[0].in_lexical);
        let sem_only = out.iter().find(|h| h.id == "sem_only").unwrap();
        assert!(sem_only.in_semantic && !sem_only.in_lexical);
        let lex_only = out.iter().find(|h| h.id == "lex_only").unwrap();
        assert!(!lex_only.in_semantic && lex_only.in_lexical);
    }

    #[test]
    fn single_source_degrades_to_its_order() {
        // Only lexical results → same order, all flagged lexical-only.
        let out = blend_semantic_lexical(&[], &ids(&["a", "b", "c"]));
        assert_eq!(out.iter().map(|h| h.id.as_str()).collect::<Vec<_>>(), vec!["a", "b", "c"]);
        assert!(out.iter().all(|h| h.in_lexical && !h.in_semantic));
    }

    #[test]
    fn empty_inputs_yield_empty() {
        assert!(rrf(&[], DEFAULT_RRF_K).is_empty());
        assert!(blend_semantic_lexical(&[], &[]).is_empty());
    }
}
