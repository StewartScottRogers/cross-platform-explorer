//! Tag reconciliation + portable metadata codec (CPE-827, epic CPE-717): the pure policy layer that
//! bridges CPE's internal [`crate::tags`] store and a path's **native file metadata**.
//!
//! CPE's tags normally live only in `tags.json`; this module defines a **portable representation** —
//! [`NativeTags`] (`{tags, label}` as a small JSON blob) — that CPE-828 stores in an NTFS alternate
//! data stream / POSIX xattr via [`crate::native_meta`], so a path's labels travel *with the file*
//! outside the app. It also implements the two-way **reconciliation policy**:
//!
//! - **pull** (native → internal): a non-destructive union — existing internal tags are never dropped,
//!   native tag names are added, and a native label is taken only when the internal one is empty
//!   (mirroring the internal `import` merge, [`crate::tags::tag_store_merge`]).
//! - **push** (internal → native): the internal entry is authoritative; its normalized `{tags, label}`
//!   is the native representation to write (or `None` when empty, so the caller removes the blob).
//!
//! Pure and Tauri-free — no new dependency, fully cargo-tested. The macOS Finder-tag bplist codec is a
//! separate child (CPE-829); the command/UI wiring is CPE-828.

use serde::{Deserialize, Serialize};

use crate::tags::{tag_store_set, TagStore};

/// CPE's portable native-metadata representation of a path's organisational labels: its tags + colour
/// label, stored as a JSON blob in native file metadata so they survive outside `tags.json`.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct NativeTags {
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub label: String,
}

impl NativeTags {
    /// Build from raw tags + label, normalizing tags (trim, drop empties, sort, de-dupe) and trimming
    /// the label — the same normalization the internal store applies.
    pub fn new(tags: Vec<String>, label: String) -> Self {
        Self {
            tags: normalize(tags),
            label: label.trim().to_string(),
        }
    }

    /// Encode as the portable JSON blob for native-metadata storage.
    pub fn encode(&self) -> Vec<u8> {
        // Serializing a struct of strings can't realistically fail; degrade to an empty object rather
        // than surface an error to a metadata write.
        serde_json::to_vec(self).unwrap_or_else(|_| b"{}".to_vec())
    }

    /// Decode a portable blob back to [`NativeTags`]. **Lenient**: a malformed or foreign blob (e.g. a
    /// stream some other tool wrote) yields the default empty value rather than an error, so a stray
    /// blob never fails a listing.
    pub fn decode(blob: &[u8]) -> Self {
        serde_json::from_slice::<NativeTags>(blob)
            .map(|n| Self::new(n.tags, n.label))
            .unwrap_or_default()
    }

    /// True when there is nothing to store (no tags, no label) — the caller removes the native blob.
    pub fn is_empty(&self) -> bool {
        self.tags.is_empty() && self.label.is_empty()
    }
}

/// Trim, drop empties, sort and de-dupe a tag list — CPE's canonical tag normalization.
fn normalize(tags: Vec<String>) -> Vec<String> {
    let mut t: Vec<String> = tags
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    t.sort();
    t.dedup();
    t
}

/// **Pull** a path's native tags into the store: union tag names into the existing entry
/// (non-destructive), and take the native label only when the internal one is empty. Returns whether
/// the store actually changed.
pub fn pull_into_store(store: &mut TagStore, path: &str, native: &NativeTags) -> bool {
    let (cur_tags, cur_label) = store
        .get(path)
        .map(|e| (e.tags().to_vec(), e.label().to_string()))
        .unwrap_or_default();

    let mut merged = cur_tags.clone();
    for t in &native.tags {
        if !merged.contains(t) {
            merged.push(t.clone());
        }
    }
    let merged = normalize(merged);
    let label = if cur_label.trim().is_empty() {
        native.label.clone()
    } else {
        cur_label.clone()
    };

    if merged == cur_tags && label == cur_label {
        return false;
    }
    tag_store_set(store, path, merged, label);
    true
}

/// **Push**: the native representation to write for `path` (the internal entry is authoritative).
/// `None` when the path has no tags and no label — the caller should remove the native blob so the
/// file's metadata matches the (now empty) internal state.
pub fn push_from_store(store: &TagStore, path: &str) -> Option<NativeTags> {
    store
        .get(path)
        .map(|e| NativeTags::new(e.tags().to_vec(), e.label().to_string()))
        .filter(|n| !n.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tags::tag_store_set;

    #[test]
    fn encode_decode_round_trips_and_normalizes() {
        let n = NativeTags::new(vec!["  Zeta ".into(), "alpha".into(), "alpha".into()], " red ".into());
        assert_eq!(n.tags, vec!["Zeta".to_string(), "alpha".to_string()]);
        assert_eq!(n.label, "red");
        let back = NativeTags::decode(&n.encode());
        assert_eq!(back, n);
    }

    #[test]
    fn decode_is_lenient_on_garbage() {
        assert_eq!(NativeTags::decode(b"not json at all"), NativeTags::default());
        assert_eq!(NativeTags::decode(b""), NativeTags::default());
        // A foreign JSON shape decodes to empty (missing fields default), never errors.
        assert_eq!(NativeTags::decode(br#"{"other":1}"#), NativeTags::default());
    }

    #[test]
    fn pull_unions_non_destructively_and_reports_change() {
        let mut store = TagStore::new();
        tag_store_set(&mut store, "/a", vec!["keep".into()], "blue".into());

        // Native adds a tag; internal label already set, so native label is ignored.
        let native = NativeTags::new(vec!["added".into(), "keep".into()], "red".into());
        assert!(pull_into_store(&mut store, "/a", &native));
        assert_eq!(store["/a"].tags(), &["added".to_string(), "keep".to_string()]);
        assert_eq!(store["/a"].label(), "blue", "internal label wins when non-empty");

        // Pulling the same native again is a no-op (no change).
        assert!(!pull_into_store(&mut store, "/a", &native));
    }

    #[test]
    fn pull_takes_native_label_only_when_internal_empty() {
        let mut store = TagStore::new();
        tag_store_set(&mut store, "/a", vec!["t".into()], "".into());
        let native = NativeTags::new(vec![], "green".into());
        assert!(pull_into_store(&mut store, "/a", &native));
        assert_eq!(store["/a"].label(), "green");
    }

    #[test]
    fn pull_onto_untagged_path_creates_the_entry() {
        let mut store = TagStore::new();
        let native = NativeTags::new(vec!["fresh".into()], "".into());
        assert!(pull_into_store(&mut store, "/new", &native));
        assert_eq!(store["/new"].tags(), &["fresh".to_string()]);
    }

    #[test]
    fn push_is_authoritative_and_none_when_empty() {
        let mut store = TagStore::new();
        tag_store_set(&mut store, "/a", vec!["x".into(), "y".into()], "red".into());
        let n = push_from_store(&store, "/a").unwrap();
        assert_eq!(n.tags, vec!["x".to_string(), "y".to_string()]);
        assert_eq!(n.label, "red");
        // A path with no entry, or an empty one, pushes nothing.
        assert!(push_from_store(&store, "/absent").is_none());
    }

    #[test]
    fn full_push_encode_decode_pull_preserves_tags() {
        // internal → push → encode → (native metadata) → decode → pull into a fresh store
        let mut src = TagStore::new();
        tag_store_set(&mut src, "/f", vec!["report".into(), "q3".into()], "red".into());
        let blob = push_from_store(&src, "/f").unwrap().encode();

        let mut dst = TagStore::new();
        let decoded = NativeTags::decode(&blob);
        assert!(pull_into_store(&mut dst, "/f", &decoded));
        assert_eq!(dst["/f"].tags(), &["q3".to_string(), "report".to_string()]);
        assert_eq!(dst["/f"].label(), "red");
    }
}
