//! Semantic index pipeline (CPE-983, epic CPE-976): the document-level layer over the vector core.
//!
//! [`crate::vector_index`] (CPE-981) stores + searches vectors and [`crate::embedder`] (CPE-982) turns text
//! into them; this joins them into a **document** pipeline: chunk a document, embed each chunk, index it, and
//! search per document (the best-matching chunk represents the doc) — so a natural-language query returns the
//! right *files*, not raw fragments. Pure and tested with the dependency-free `FakeEmbedder`; the text
//! *extraction* (`doc_text`/`content_search`) and change-driven re-index (CPE-833) are wiring the caller
//! supplies.

use std::collections::BTreeMap;

use crate::embedder::Embedder;
use crate::vector_index::VectorIndex;

/// Separator between a document id and its chunk index in the vector index's item ids. NUL never appears in
/// a path/id, so `split_once('\0')` recovers the document id unambiguously.
const CHUNK_SEP: char = '\u{0}';

/// Default chunking: how many words per chunk, and how many overlap between consecutive chunks.
const DEFAULT_MAX_WORDS: usize = 120;
const DEFAULT_OVERLAP: usize = 20;

/// One document-level search result: the document id and its best chunk's cosine score.
#[derive(Debug, Clone, PartialEq)]
pub struct DocHit {
    pub doc_id: String,
    pub score: f32,
}

/// Split `text` into overlapping word windows. Each chunk has up to `max_words` words; consecutive chunks
/// share `overlap` words so a phrase straddling a boundary is still captured whole in one chunk. Text with
/// no words yields nothing; text shorter than `max_words` yields a single chunk.
pub fn chunk_text(text: &str, max_words: usize, overlap: usize) -> Vec<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return Vec::new();
    }
    let max_words = max_words.max(1);
    // Step forward by (max_words - overlap), but always at least 1 so we can't loop forever.
    let step = max_words.saturating_sub(overlap).max(1);
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < words.len() {
        let end = (start + max_words).min(words.len());
        chunks.push(words[start..end].join(" "));
        if end == words.len() {
            break;
        }
        start += step;
    }
    chunks
}

/// A document-level semantic index: a [`VectorIndex`] of chunk embeddings plus the [`Embedder`] that made
/// them and the per-document chunk bookkeeping needed for precise re-index/removal.
pub struct SemanticIndex {
    index: VectorIndex,
    embedder: Box<dyn Embedder>,
    /// doc_id → number of chunks currently indexed for it (so removal is exact, not a prefix scan).
    docs: BTreeMap<String, usize>,
    max_words: usize,
    overlap: usize,
}

impl SemanticIndex {
    /// A new index using `embedder` (its `dim` sizes the vector index) and the default chunking.
    pub fn new(embedder: Box<dyn Embedder>) -> Self {
        let dim = embedder.dim();
        SemanticIndex {
            index: VectorIndex::new(dim),
            embedder,
            docs: BTreeMap::new(),
            max_words: DEFAULT_MAX_WORDS,
            overlap: DEFAULT_OVERLAP,
        }
    }

    /// Override the chunking (words per chunk, overlap). `overlap` is clamped below `max_words`.
    pub fn with_chunking(mut self, max_words: usize, overlap: usize) -> Self {
        self.max_words = max_words.max(1);
        self.overlap = overlap.min(self.max_words.saturating_sub(1));
        self
    }

    /// How many documents are indexed.
    pub fn document_count(&self) -> usize {
        self.docs.len()
    }

    /// Whether `doc_id` is indexed.
    pub fn contains(&self, doc_id: &str) -> bool {
        self.docs.contains_key(doc_id)
    }

    /// Add or replace a document: drop any prior chunks for `doc_id`, then chunk + embed + index `text`.
    /// A document whose text has no words is recorded with zero chunks (so `contains` is true but it never
    /// matches) — the caller can still track that it was seen.
    pub fn upsert_document(&mut self, doc_id: &str, text: &str) {
        self.remove_document(doc_id);
        let chunks = chunk_text(text, self.max_words, self.overlap);
        let refs: Vec<&str> = chunks.iter().map(|s| s.as_str()).collect();
        let vectors = self.embedder.embed_batch(&refs);
        for (i, vec) in vectors.iter().enumerate() {
            // Ignore an add error (only a dim mismatch, which a well-formed embedder can't produce).
            let _ = self.index.add(format!("{doc_id}{CHUNK_SEP}{i}"), vec);
        }
        self.docs.insert(doc_id.to_string(), chunks.len());
    }

    /// Remove a document and all its chunks. Returns whether it was present.
    pub fn remove_document(&mut self, doc_id: &str) -> bool {
        let Some(count) = self.docs.remove(doc_id) else { return false };
        for i in 0..count {
            self.index.remove(&format!("{doc_id}{CHUNK_SEP}{i}"));
        }
        true
    }

    /// The top-`k` documents most similar to `query`, best-first. Every chunk is scored; the score for a
    /// document is its **best** chunk's cosine similarity. Deterministic tiebreak by doc_id. An empty query
    /// or empty index yields nothing.
    pub fn search(&self, query: &str, k: usize) -> Vec<DocHit> {
        if k == 0 || self.index.is_empty() {
            return Vec::new();
        }
        let q = self.embedder.embed(query);
        // Score every chunk (len() = all), then collapse to the best chunk per document.
        let chunk_hits = self.index.search(&q, self.index.len());
        let mut best: BTreeMap<String, f32> = BTreeMap::new();
        for hit in chunk_hits {
            let doc_id = hit.id.split_once(CHUNK_SEP).map(|(d, _)| d).unwrap_or(&hit.id);
            best.entry(doc_id.to_string())
                .and_modify(|s| {
                    if hit.score > *s {
                        *s = hit.score;
                    }
                })
                .or_insert(hit.score);
        }
        // Only positive cosine similarity is a match — a zero/negative score means no shared direction, so
        // an unrelated document is dropped rather than padded into the results.
        let mut docs: Vec<DocHit> = best
            .into_iter()
            .filter(|(_, score)| *score > 0.0)
            .map(|(doc_id, score)| DocHit { doc_id, score })
            .collect();
        docs.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.doc_id.cmp(&b.doc_id))
        });
        docs.truncate(k);
        docs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embedder::FakeEmbedder;

    fn fake_index() -> SemanticIndex {
        SemanticIndex::new(Box::new(FakeEmbedder::new(1024)))
    }

    fn doc_ids(hits: &[DocHit]) -> Vec<&str> {
        hits.iter().map(|h| h.doc_id.as_str()).collect()
    }

    #[test]
    fn chunk_text_windows_words_with_overlap() {
        assert_eq!(chunk_text("a b c d e", 2, 0), vec!["a b", "c d", "e"]);
        // Step 1 with 2-word windows: every word is covered; the final window "d e" already contains "e".
        assert_eq!(chunk_text("a b c d e", 2, 1), vec!["a b", "b c", "c d", "d e"]);
        assert_eq!(chunk_text("only three words", 10, 3), vec!["only three words"]); // shorter than a chunk
        assert!(chunk_text("   ", 5, 1).is_empty()); // no words
    }

    #[test]
    fn search_returns_the_most_relevant_document() {
        let mut si = fake_index();
        si.upsert_document("fox.txt", "the quick brown fox jumps over things");
        si.upsert_document("dog.txt", "a lazy sleeping dog in the sun");
        si.upsert_document("hound.txt", "the fox outran the hound");
        let hits = si.search("quick fox", 3);
        assert_eq!(doc_ids(&hits)[0], "fox.txt"); // shares quick+fox
        assert!(hits.iter().any(|h| h.doc_id == "hound.txt")); // shares fox
        // The dog doc shares nothing → zero similarity → filtered out entirely.
        assert!(!hits.iter().any(|h| h.doc_id == "dog.txt"));
    }

    #[test]
    fn upsert_replaces_and_leaves_no_stale_chunks() {
        let mut si = fake_index();
        si.upsert_document("d", "alpha beta gamma");
        assert!(si.search("alpha", 5).iter().any(|h| h.doc_id == "d"));
        // Re-upsert with unrelated text: the old tokens must no longer match.
        si.upsert_document("d", "delta epsilon");
        assert_eq!(si.document_count(), 1);
        assert!(si.search("alpha", 5).is_empty(), "stale chunk survived re-upsert");
        assert!(si.search("delta", 5).iter().any(|h| h.doc_id == "d"));
    }

    #[test]
    fn remove_document_clears_it() {
        let mut si = fake_index();
        si.upsert_document("keep", "shared token here");
        si.upsert_document("drop", "shared token here");
        assert!(si.remove_document("drop"));
        assert!(!si.remove_document("drop")); // already gone
        let hits = si.search("shared token", 5);
        assert_eq!(doc_ids(&hits), vec!["keep"]);
    }

    #[test]
    fn best_chunk_represents_a_multi_chunk_document() {
        // A long doc: the relevant phrase sits only in a later chunk; the doc must still rank on it.
        let mut si = fake_index().with_chunking(4, 0);
        let long = "aaa bbb ccc ddd eee fff ggg hhh volcano eruption lava ash";
        si.upsert_document("geo", long);
        si.upsert_document("misc", "totally unrelated filler words here now");
        let hits = si.search("volcano lava", 2);
        assert_eq!(hits[0].doc_id, "geo");
        assert!(hits[0].score > 0.0);
    }

    #[test]
    fn empty_query_or_index_yields_nothing() {
        let mut si = fake_index();
        assert!(si.search("anything", 5).is_empty()); // empty index
        si.upsert_document("d", "some words");
        assert!(si.search("", 5).is_empty()); // empty query → zero vector → no hits
        assert!(si.search("words", 0).is_empty()); // k == 0
    }

    #[test]
    fn tokenless_document_is_tracked_but_never_matches() {
        let mut si = fake_index();
        si.upsert_document("empty", "   ");
        assert!(si.contains("empty"));
        assert_eq!(si.document_count(), 1);
        assert!(si.search("empty", 5).is_empty());
    }
}
