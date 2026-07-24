//! Embedding seam (CPE-982, epic CPE-976): the pluggable boundary between "turn text into a vector" and the
//! rest of AI semantic search.
//!
//! [`crate::vector_index`] (CPE-981) stores + searches vectors but commits to no model; this defines the
//! [`Embedder`] trait that produces those vectors, plus a dependency-free [`FakeEmbedder`] so the chunk→embed
//! pipeline (CPE-983) and query blend (CPE-984) can be built and tested **end-to-end today**, before any real
//! model is chosen. A **real** backend — a bundled local model or an opt-in external endpoint — is the
//! deferred big-design call and will live behind a feature gate; nothing here pulls in an ML stack.

/// Turns text into a fixed-dimensionality embedding vector. Object-safe, so a pipeline can hold a
/// `Box<dyn Embedder>` and swap the [`FakeEmbedder`] for a real model without touching call sites.
pub trait Embedder {
    /// The dimensionality of every vector this embedder produces (must match the [`crate::vector_index`] it
    /// feeds).
    fn dim(&self) -> usize;

    /// Embed a single piece of text. Same input → same output (deterministic).
    fn embed(&self, text: &str) -> Vec<f32>;

    /// Embed a batch of texts. Defaults to per-item [`Embedder::embed`]; a real backend may override this to
    /// batch calls to a model/endpoint.
    fn embed_batch(&self, texts: &[&str]) -> Vec<Vec<f32>> {
        texts.iter().map(|t| self.embed(t)).collect()
    }
}

/// A deterministic, dependency-free embedder for tests + local dev: a **feature-hashed bag-of-words**. It
/// tokenises text and hashes each token into one of `dim` buckets (counting occurrences), so identical text
/// yields an identical vector and texts that **share tokens have non-zero cosine similarity** — enough to
/// exercise the vector index + pipeline as if by a real semantic model, with zero model weight.
///
/// It is not a language model: it captures lexical overlap, not meaning. Its value is being deterministic,
/// fast, and dependency-free so CPE-983/984 have a real `Embedder` to build against.
#[derive(Debug, Clone, Copy)]
pub struct FakeEmbedder {
    dim: usize,
}

impl FakeEmbedder {
    /// A fake embedder producing `dim`-dimensional vectors. `dim` must be non-zero; larger `dim` means fewer
    /// hash collisions between distinct tokens.
    pub fn new(dim: usize) -> Self {
        assert!(dim > 0, "embedder dimensionality must be non-zero");
        FakeEmbedder { dim }
    }
}

impl Embedder for FakeEmbedder {
    fn dim(&self) -> usize {
        self.dim
    }

    fn embed(&self, text: &str) -> Vec<f32> {
        let mut v = vec![0.0f32; self.dim];
        for token in tokenize(text) {
            let bucket = (fnv1a(token.as_bytes()) as usize) % self.dim;
            v[bucket] += 1.0;
        }
        v
    }
}

/// Lowercase + split on non-alphanumeric runs into tokens (empties dropped).
fn tokenize(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| t.to_lowercase())
}

/// FNV-1a 32-bit hash — a small, **stable** (cross-run, cross-platform) hash, unlike `std`'s
/// `DefaultHasher`, so a persisted [`FakeEmbedder`] index stays valid between runs.
fn fnv1a(bytes: &[u8]) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for &b in bytes {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector_index::VectorIndex;

    #[test]
    fn embed_is_deterministic_and_correct_dim() {
        let e = FakeEmbedder::new(128);
        assert_eq!(e.dim(), 128);
        let a = e.embed("The Quick Brown Fox");
        let b = e.embed("the quick brown fox"); // case-insensitive → identical
        assert_eq!(a, b);
        assert_eq!(a.len(), 128);
        // Non-empty text produces a non-zero vector.
        assert!(a.iter().any(|&x| x != 0.0));
    }

    #[test]
    fn empty_or_tokenless_text_is_a_zero_vector() {
        let e = FakeEmbedder::new(32);
        assert!(e.embed("").iter().all(|&x| x == 0.0));
        assert!(e.embed("   ...  !!! ").iter().all(|&x| x == 0.0));
    }

    #[test]
    fn shared_tokens_give_higher_similarity() {
        // Cosine similarity (dim large enough to avoid collisions) should rank token-overlap correctly.
        let e = FakeEmbedder::new(1024);
        let mut idx = VectorIndex::new(1024);
        idx.add("fox", &e.embed("the quick brown fox")).unwrap();
        idx.add("dog", &e.embed("the lazy sleeping dog")).unwrap();
        idx.add("hound", &e.embed("a fox and a hound")).unwrap();
        // Query shares "quick"+"fox" with doc "fox", only "fox" with "hound", nothing with "dog".
        let hits = idx.search(&e.embed("quick fox"), 3);
        let order: Vec<&str> = hits.iter().map(|h| h.id.as_str()).collect();
        assert_eq!(order[0], "fox", "closest doc first");
        assert_eq!(order[1], "hound", "partial overlap second");
        assert!(hits[2].score.abs() < 1e-6, "unrelated doc scores ~0: {:?}", hits[2]);
    }

    #[test]
    fn embed_batch_matches_per_item_embed() {
        let e = FakeEmbedder::new(64);
        let texts = ["alpha beta", "gamma", ""];
        let batch = e.embed_batch(&texts);
        let per: Vec<Vec<f32>> = texts.iter().map(|t| e.embed(t)).collect();
        assert_eq!(batch, per);
    }

    #[test]
    fn usable_as_a_trait_object() {
        // The seam must be object-safe so a pipeline can hold Box<dyn Embedder>.
        let e: Box<dyn Embedder> = Box::new(FakeEmbedder::new(16));
        assert_eq!(e.dim(), 16);
        assert_eq!(e.embed("x y").len(), 16);
    }

    #[test]
    fn hash_matches_published_fnv1a_vectors() {
        // Pin the canonical FNV-1a-32 test vectors so an accidental swap to a different/non-stable hasher
        // fails here — a persisted FakeEmbedder index depends on this hash being identical across runs.
        assert_eq!(fnv1a(b""), 0x811c_9dc5); // offset basis (no bytes)
        assert_eq!(fnv1a(b"a"), 0xe40c_292c);
        assert_eq!(fnv1a(b"foobar"), 0xbf9c_f968);
    }
}
