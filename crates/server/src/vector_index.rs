//! Pure vector index (CPE-981, epic CPE-976): the backend/model-agnostic core of AI semantic search.
//!
//! Where [`crate::index_query`] is the lexical query brain for CPE-703, this is the *vector* brain for
//! CPE-976: store per-item embeddings, find the nearest by **cosine similarity** (top-k), and persist the
//! index. It commits to **no embedding model** — vectors arrive already computed as `Vec<f32>` — so it is
//! reused whatever embedder [`CPE-982`] lands, needs no attended decision, and is fully cargo-testable.
//!
//! Vectors are **L2-normalised on insert**, so cosine similarity is a plain dot product (fast, scores in
//! `[-1, 1]`). The store is a hand-rolled versioned binary layout (zero new deps, mirroring the CPE-832
//! index discipline): a format mismatch is a transparent rebuild ([`VectorIndexError::Stale`]), never a hard
//! error. Pure std and tiny — zero runtime cost unless an index is actually built (like
//! [`crate::restore_plan`] / [`crate::snapshot`]); the heavy *embedder* is the feature-gated part (CPE-982).

use std::path::Path;

/// On-disk magic + format version. A mismatch → [`VectorIndexError::Stale`] (rebuild), so bumping this (or
/// changing the embedding dimensionality/model) transparently re-indexes rather than erroring.
const MAGIC: &[u8; 8] = b"CPEVEC\x00\x00";
const FORMAT_VERSION: u32 = 1;

/// One search result: the item id and its cosine similarity to the query (higher = closer, in `[-1, 1]`).
#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    pub id: String,
    pub score: f32,
}

/// Why a [`VectorIndex::load`] didn't return a usable index.
#[derive(Debug)]
pub enum VectorIndexError {
    /// The file couldn't be read / is short or garbled. Carries the OS or parse message.
    Io(String),
    /// The file is a different magic/format version than this build understands — rebuild (don't surface an
    /// error). The transparent-rebuild path (also how an embedding-dim/model change is absorbed).
    Stale,
}

impl std::fmt::Display for VectorIndexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VectorIndexError::Io(m) => write!(f, "vector index io error: {m}"),
            VectorIndexError::Stale => write!(f, "vector index is stale (format/dim mismatch); rebuild"),
        }
    }
}
impl std::error::Error for VectorIndexError {}

/// A dense vector index over unit-normalised embeddings, addressed by string id. Build it in memory
/// ([`VectorIndex::new`] + [`VectorIndex::add`]), query it ([`VectorIndex::search`]), and persist it
/// ([`VectorIndex::save`] / [`VectorIndex::load`]).
#[derive(Debug, Clone)]
pub struct VectorIndex {
    /// Embedding dimensionality — every vector must match this.
    dim: usize,
    /// Item ids, parallel to `vectors` rows.
    ids: Vec<String>,
    /// Row-major `dim × ids.len()` matrix of **L2-normalised** vectors.
    vectors: Vec<f32>,
}

impl VectorIndex {
    /// An empty index for `dim`-dimensional embeddings. `dim` must be non-zero.
    pub fn new(dim: usize) -> Self {
        assert!(dim > 0, "vector index dimensionality must be non-zero");
        VectorIndex { dim, ids: Vec::new(), vectors: Vec::new() }
    }

    /// The embedding dimensionality this index holds.
    pub fn dim(&self) -> usize {
        self.dim
    }

    /// How many items are indexed.
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    /// Whether the index holds no items.
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// Whether `id` is present.
    pub fn contains(&self, id: &str) -> bool {
        self.ids.iter().any(|i| i == id)
    }

    /// Add (or replace) the embedding for `id`. The vector's length must equal [`VectorIndex::dim`]; it is
    /// L2-normalised before storage (a zero vector is stored as-is and never matches anything). Re-adding an
    /// existing id overwrites its vector in place.
    pub fn add(&mut self, id: impl Into<String>, vector: &[f32]) -> Result<(), VectorIndexError> {
        if vector.len() != self.dim {
            return Err(VectorIndexError::Io(format!(
                "embedding length {} != index dim {}",
                vector.len(),
                self.dim
            )));
        }
        let normalised = l2_normalise(vector);
        let id = id.into();
        match self.ids.iter().position(|i| *i == id) {
            Some(row) => {
                let start = row * self.dim;
                self.vectors[start..start + self.dim].copy_from_slice(&normalised);
            }
            None => {
                self.ids.push(id);
                self.vectors.extend_from_slice(&normalised);
            }
        }
        Ok(())
    }

    /// Remove `id`, returning whether it was present. O(n): the row is swap-removed (index order is
    /// irrelevant — search re-ranks by score).
    pub fn remove(&mut self, id: &str) -> bool {
        let Some(row) = self.ids.iter().position(|i| i == id) else { return false };
        let last = self.ids.len() - 1;
        // Swap the row's vector with the last row's, then truncate.
        if row != last {
            let (a, b) = (row * self.dim, last * self.dim);
            for k in 0..self.dim {
                self.vectors.swap(a + k, b + k);
            }
            self.ids.swap(row, last);
        }
        self.ids.pop();
        self.vectors.truncate(last * self.dim);
        true
    }

    /// The `k` items most similar to `query` by cosine similarity, best-first. Ties (equal score) break by
    /// id ascending, so the order is deterministic. An empty index, `k == 0`, a wrong-length query, or a
    /// zero query yields no hits.
    pub fn search(&self, query: &[f32], k: usize) -> Vec<SearchHit> {
        if k == 0 || self.is_empty() || query.len() != self.dim {
            return Vec::new();
        }
        let q = l2_normalise(query);
        // A zero (degenerate) query can't be a meaningful direction — no hits.
        if q.iter().all(|&x| x == 0.0) {
            return Vec::new();
        }
        let mut hits: Vec<SearchHit> = self
            .ids
            .iter()
            .enumerate()
            .map(|(row, id)| {
                let start = row * self.dim;
                let score = dot(&q, &self.vectors[start..start + self.dim]);
                SearchHit { id: id.clone(), score }
            })
            .collect();
        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.id.cmp(&b.id))
        });
        hits.truncate(k);
        hits
    }

    /// Serialise to the hand-rolled binary format: header (magic + version + dim + count) then, for each
    /// item, its id (`u32` len + utf8) and its `dim` little-endian `f32`s. Vectors are already normalised.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        out.extend_from_slice(&(self.dim as u32).to_le_bytes());
        out.extend_from_slice(&(self.ids.len() as u32).to_le_bytes());
        for (row, id) in self.ids.iter().enumerate() {
            out.extend_from_slice(&(id.len() as u32).to_le_bytes());
            out.extend_from_slice(id.as_bytes());
            let start = row * self.dim;
            for &f in &self.vectors[start..start + self.dim] {
                out.extend_from_slice(&f.to_le_bytes());
            }
        }
        out
    }

    /// Parse the format from [`VectorIndex::to_bytes`]. Wrong magic/version → [`VectorIndexError::Stale`]
    /// (rebuild); short/garbled body → [`VectorIndexError::Io`] (never a panic).
    pub fn from_bytes(bytes: &[u8]) -> Result<VectorIndex, VectorIndexError> {
        let mut r = Reader::new(bytes);
        let magic = r.take(8).ok_or(VectorIndexError::Stale)?;
        if magic != MAGIC {
            return Err(VectorIndexError::Stale);
        }
        if r.u32().ok_or(VectorIndexError::Stale)? != FORMAT_VERSION {
            return Err(VectorIndexError::Stale);
        }
        let dim = r.u32().ok_or_else(|| VectorIndexError::Io("truncated header".into()))? as usize;
        if dim == 0 {
            return Err(VectorIndexError::Io("zero dimensionality".into()));
        }
        let count = r.u32().ok_or_else(|| VectorIndexError::Io("truncated header".into()))? as usize;
        let mut ids = Vec::with_capacity(count);
        let mut vectors = Vec::with_capacity(count * dim);
        for _ in 0..count {
            let len = r.u32().ok_or_else(|| VectorIndexError::Io("truncated id len".into()))? as usize;
            let raw = r.take(len).ok_or_else(|| VectorIndexError::Io("truncated id".into()))?;
            let id = std::str::from_utf8(raw).map_err(|_| VectorIndexError::Io("non-utf8 id".into()))?;
            ids.push(id.to_string());
            for _ in 0..dim {
                vectors.push(r.f32().ok_or_else(|| VectorIndexError::Io("truncated vector".into()))?);
            }
        }
        Ok(VectorIndex { dim, ids, vectors })
    }

    /// Persist to `path` via a temp sibling + rename, so a crash never leaves a half-written index.
    pub fn save(&self, path: &Path) -> Result<(), VectorIndexError> {
        let tmp = path.with_extension("cpevec.tmp");
        std::fs::write(&tmp, self.to_bytes()).map_err(|e| VectorIndexError::Io(e.to_string()))?;
        std::fs::rename(&tmp, path).map_err(|e| VectorIndexError::Io(e.to_string()))?;
        Ok(())
    }

    /// Load from `path`. Missing/unreadable → [`VectorIndexError::Io`]; format mismatch →
    /// [`VectorIndexError::Stale`] so the caller rebuilds transparently.
    pub fn load(path: &Path) -> Result<VectorIndex, VectorIndexError> {
        let bytes = std::fs::read(path).map_err(|e| VectorIndexError::Io(e.to_string()))?;
        VectorIndex::from_bytes(&bytes)
    }
}

/// L2-normalise a vector (unit length). A zero vector is returned unchanged (norm 0 → can't normalise), so
/// it never spuriously matches.
fn l2_normalise(v: &[f32]) -> Vec<f32> {
    let norm = v.iter().map(|&x| x * x).sum::<f32>().sqrt();
    if norm == 0.0 || !norm.is_finite() {
        return v.to_vec();
    }
    v.iter().map(|&x| x / norm).collect()
}

/// Dot product of two equal-length slices.
fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(&x, &y)| x * y).sum()
}

/// A tiny bounds-checked cursor over the index bytes — every read returns `None` past the end, so a
/// truncated file degrades to an error instead of panicking.
struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}
impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Reader { buf, pos: 0 }
    }
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(n)?;
        let s = self.buf.get(self.pos..end)?;
        self.pos = end;
        Some(s)
    }
    fn u32(&mut self) -> Option<u32> {
        self.take(4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn f32(&mut self) -> Option<f32> {
        self.take(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids_of(hits: &[SearchHit]) -> Vec<&str> {
        hits.iter().map(|h| h.id.as_str()).collect()
    }

    #[test]
    fn search_returns_nearest_by_cosine() {
        let mut idx = VectorIndex::new(3);
        idx.add("x", &[1.0, 0.0, 0.0]).unwrap();
        idx.add("y", &[0.0, 1.0, 0.0]).unwrap();
        idx.add("xy", &[1.0, 1.0, 0.0]).unwrap();
        // A query along +x is closest to "x", then "xy" (45°), then "y" (orthogonal, score 0).
        let hits = idx.search(&[2.0, 0.0, 0.0], 3);
        assert_eq!(ids_of(&hits), vec!["x", "xy", "y"]);
        assert!((hits[0].score - 1.0).abs() < 1e-6);
        assert!((hits[1].score - (0.5f32).sqrt()).abs() < 1e-6); // cos 45° = 1/√2
        assert!(hits[2].score.abs() < 1e-6);
    }

    #[test]
    fn magnitude_does_not_matter_only_direction() {
        let mut idx = VectorIndex::new(2);
        idx.add("a", &[3.0, 4.0]).unwrap(); // same direction as (0.6,0.8), any scale
        let big = idx.search(&[30.0, 40.0], 1);
        let small = idx.search(&[0.3, 0.4], 1);
        assert_eq!(big[0].id, "a");
        assert!((big[0].score - 1.0).abs() < 1e-6);
        assert!((small[0].score - big[0].score).abs() < 1e-6);
    }

    #[test]
    fn top_k_limits_and_breaks_ties_by_id() {
        let mut idx = VectorIndex::new(2);
        // Three identical vectors → equal scores → deterministic id-ascending order.
        for id in ["c", "a", "b"] {
            idx.add(id, &[1.0, 0.0]).unwrap();
        }
        let hits = idx.search(&[1.0, 0.0], 2);
        assert_eq!(ids_of(&hits), vec!["a", "b"]); // capped at 2, ids ascending
    }

    #[test]
    fn add_replaces_existing_id_in_place() {
        let mut idx = VectorIndex::new(2);
        idx.add("p", &[1.0, 0.0]).unwrap();
        idx.add("p", &[0.0, 1.0]).unwrap(); // replace, not duplicate
        assert_eq!(idx.len(), 1);
        let hits = idx.search(&[0.0, 1.0], 1);
        assert_eq!(hits[0].id, "p");
        assert!((hits[0].score - 1.0).abs() < 1e-6);
    }

    #[test]
    fn remove_takes_an_item_out() {
        let mut idx = VectorIndex::new(2);
        idx.add("a", &[1.0, 0.0]).unwrap();
        idx.add("b", &[0.0, 1.0]).unwrap();
        assert!(idx.remove("a"));
        assert!(!idx.remove("a")); // already gone
        assert_eq!(idx.len(), 1);
        assert!(!idx.contains("a"));
        // The survivor is still searchable + correctly placed.
        let hits = idx.search(&[0.0, 1.0], 5);
        assert_eq!(ids_of(&hits), vec!["b"]);
    }

    #[test]
    fn dim_mismatch_is_rejected() {
        let mut idx = VectorIndex::new(3);
        assert!(idx.add("bad", &[1.0, 2.0]).is_err());
        assert!(idx.search(&[1.0, 2.0], 1).is_empty()); // wrong-length query → no hits, no panic
    }

    #[test]
    fn empty_and_degenerate_queries_yield_nothing() {
        let mut idx = VectorIndex::new(2);
        assert!(idx.search(&[1.0, 0.0], 1).is_empty()); // empty index
        idx.add("a", &[1.0, 0.0]).unwrap();
        assert!(idx.search(&[1.0, 0.0], 0).is_empty()); // k == 0
        assert!(idx.search(&[0.0, 0.0], 1).is_empty()); // zero query
    }

    #[test]
    fn zero_vector_item_never_matches() {
        let mut idx = VectorIndex::new(2);
        idx.add("zero", &[0.0, 0.0]).unwrap();
        idx.add("real", &[1.0, 0.0]).unwrap();
        let hits = idx.search(&[1.0, 0.0], 5);
        assert_eq!(hits[0].id, "real");
        // The zero item scores 0 (dot with anything) and sorts last.
        assert_eq!(hits.last().unwrap().id, "zero");
        assert!(hits.last().unwrap().score.abs() < 1e-6);
    }

    #[test]
    fn save_load_roundtrips_and_still_searches() {
        let dir = std::env::temp_dir().join(format!("cpe-vec-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("idx.cpevec");
        let mut idx = VectorIndex::new(3);
        idx.add("x", &[1.0, 0.0, 0.0]).unwrap();
        idx.add("y", &[0.0, 1.0, 0.0]).unwrap();
        idx.save(&file).unwrap();
        let re = VectorIndex::load(&file).unwrap();
        assert_eq!(re.dim(), 3);
        assert_eq!(re.len(), 2);
        assert_eq!(re.search(&[1.0, 0.0, 0.0], 1)[0].id, "x");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_rejects_bad_magic_and_version_as_stale() {
        assert!(matches!(VectorIndex::from_bytes(b"nope"), Err(VectorIndexError::Stale)));
        let mut b = Vec::new();
        b.extend_from_slice(MAGIC);
        b.extend_from_slice(&(FORMAT_VERSION + 1).to_le_bytes());
        assert!(matches!(VectorIndex::from_bytes(&b), Err(VectorIndexError::Stale)));
    }

    #[test]
    fn truncated_body_is_io_not_panic() {
        let mut idx = VectorIndex::new(4);
        idx.add("a", &[1.0, 0.0, 0.0, 0.0]).unwrap();
        let full = idx.to_bytes();
        for cut in 0..full.len() {
            // Any prefix either parses a smaller-but-valid header or errors — never panics.
            let _ = VectorIndex::from_bytes(&full[..cut]);
        }
        // A body that claims an item but omits its vector → Io.
        let mut b = Vec::new();
        b.extend_from_slice(MAGIC);
        b.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        b.extend_from_slice(&2u32.to_le_bytes()); // dim 2
        b.extend_from_slice(&1u32.to_le_bytes()); // 1 item
        b.extend_from_slice(&1u32.to_le_bytes()); // id len 1
        b.push(b'a'); // id, but no vector floats follow
        assert!(matches!(VectorIndex::from_bytes(&b), Err(VectorIndexError::Io(_))));
    }
}
