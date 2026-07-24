//! Native bridge orchestration (CPE-830, epic CPE-717): the per-OS glue that wires the storage
//! primitive ([`crate::native_meta`]), the codecs ([`crate::native_tags`] / [`crate::finder_tags`]),
//! and the reconciliation policy into a working `pull` / `push` between a path's native file metadata
//! and CPE's internal [`crate::tags`] store.
//!
//! **Per-OS policy.** macOS **co-opts the Finder tags xattr** (`_kMDItemUserTags`) so a path's tags
//! interoperate with Finder — but Finder has no equivalent of CPE's single colour *label*, so only tag
//! **names** cross on macOS. Windows/Linux have no OS-wide tag convention, so CPE stores its own
//! namespaced ADS/xattr blob carrying tags **and** label.
//!
//! Everything degrades gracefully: a filesystem that can't store native metadata, or a path with none,
//! is a no-op — never an error that could fail a listing. The internal store is authoritative on push;
//! pull is a non-destructive union. The Tauri commands + Properties UI + opt-in toggle are CPE-828.

use std::path::Path;

use crate::ctx::ServerCtx;
use crate::native_meta::{self, MetaError};
use crate::native_tags::{self, NativeTags};
use crate::tags::{read_tags_from, write_tags_to, TagStore};

/// The native attribute/stream name CPE reads and writes a path's tags under, per OS.
pub fn native_name() -> String {
    #[cfg(target_os = "macos")]
    {
        crate::finder_tags::FINDER_TAGS_XATTR.to_string()
    }
    #[cfg(not(target_os = "macos"))]
    {
        native_meta::cpe_name("tags")
    }
}

/// Decode a native metadata blob into CPE's [`NativeTags`], using the OS-appropriate codec. On macOS the
/// blob is a Finder bplist (tag names only, no CPE label); elsewhere it's CPE's own JSON `{tags,label}`.
fn decode_native(bytes: &[u8]) -> NativeTags {
    #[cfg(target_os = "macos")]
    {
        let names = crate::finder_tags::names(&crate::finder_tags::decode(bytes));
        NativeTags::new(names, String::new())
    }
    #[cfg(not(target_os = "macos"))]
    {
        NativeTags::decode(bytes)
    }
}

/// Encode CPE's [`NativeTags`] into the OS-appropriate native blob. On macOS this is a Finder bplist
/// (tags become uncoloured Finder tags — CPE's label doesn't map onto Finder); elsewhere CPE's JSON.
fn encode_native(tags: &NativeTags) -> Vec<u8> {
    #[cfg(target_os = "macos")]
    {
        let finder: Vec<crate::finder_tags::FinderTag> = tags
            .tags
            .iter()
            .map(|n| crate::finder_tags::FinderTag::new(n.clone(), 0))
            .collect();
        crate::finder_tags::encode(&finder)
    }
    #[cfg(not(target_os = "macos"))]
    {
        tags.encode()
    }
}

/// **Pull**: read `path`'s native tags and merge them into `store` non-destructively. Returns whether
/// the store changed. A path with no native metadata, or a filesystem that can't store it, is a no-op
/// (`Ok(false)`); a genuinely missing/unreadable base path is an `Err`.
pub fn pull(store: &mut TagStore, path: &Path) -> Result<bool, String> {
    let bytes = match native_meta::read(path, &native_name()) {
        Ok(Some(b)) => b,
        Ok(None) => return Ok(false),
        Err(MetaError::Unsupported) => return Ok(false),
        Err(MetaError::Io(e)) => return Err(e),
    };
    let native = decode_native(&bytes);
    Ok(native_tags::pull_into_store(
        store,
        &path.to_string_lossy(),
        &native,
    ))
}

/// **Push**: write `path`'s internal tags out to native metadata (the internal entry is authoritative).
/// A path with no tags/label has its native blob removed so the file's metadata matches the empty
/// internal state. A filesystem that can't store native metadata degrades silently (`Ok(())`); a
/// genuinely missing/unwritable base path is an `Err`.
pub fn push(store: &TagStore, path: &Path) -> Result<(), String> {
    let name = native_name();
    let outcome = match native_tags::push_from_store(store, &path.to_string_lossy()) {
        Some(tags) => native_meta::write(path, &name, &encode_native(&tags)),
        None => native_meta::remove(path, &name),
    };
    match outcome {
        Ok(()) | Err(MetaError::Unsupported) => Ok(()),
        Err(MetaError::Io(e)) => Err(e),
    }
}

// ---------------------------------------------------------------------------
// Command entry points (CPE-828) — resolve the tag store's config dir via `ServerCtx`, run the
// pull/push, and persist. The Tauri `#[tauri::command]` fns are one-line dispatchers over these,
// mirroring `crate::tags`'s command-entry pattern.
// ---------------------------------------------------------------------------

/// Pull `path`'s native tags into the persisted tag store (non-destructive) and save if anything
/// changed. Returns the updated whole store (so the frontend can refresh its tag view in one round-trip).
pub fn pull_ctx(ctx: &dyn ServerCtx, path: &Path) -> Result<TagStore, String> {
    let dir = ctx.app_config_dir()?;
    let mut store = read_tags_from(&dir);
    if pull(&mut store, path)? {
        write_tags_to(&dir, &store)?;
    }
    Ok(store)
}

/// Push `path`'s internal tags out to native file metadata (the internal store is authoritative). Reads
/// the persisted store via `ctx`; a filesystem that can't store native metadata degrades to a no-op.
pub fn push_ctx(ctx: &dyn ServerCtx, path: &Path) -> Result<(), String> {
    push(&read_tags_from(&ctx.app_config_dir()?), path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tags::tag_store_set;

    fn scratch(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-bridge-{}-{}-{}", tag, std::process::id(), n));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn push_then_pull_round_trips_tags_through_native_metadata() {
        let dir = scratch("rt");
        let f = dir.join("file.txt");
        std::fs::write(&f, b"contents").unwrap();

        // Skip gracefully if the temp filesystem can't store native metadata (e.g. old-kernel tmpfs):
        // a valid environment, not a code failure. `native_meta` is covered by its own tests.
        if !native_meta::is_supported(&f) {
            let _ = std::fs::remove_dir_all(&dir);
            return;
        }

        // Tag the path in the internal store, then push out to native metadata.
        let mut src = TagStore::new();
        tag_store_set(&mut src, &f.to_string_lossy(), vec!["report".into(), "q3".into()], "red".into());
        push(&src, &f).unwrap();
        // The base file's contents are untouched by the metadata write.
        assert_eq!(std::fs::read(&f).unwrap(), b"contents");

        // Pull into a fresh store recovers the tag names (label too, off-macOS).
        let mut dst = TagStore::new();
        assert!(pull(&mut dst, &f).unwrap(), "pull should import the pushed tags");
        assert_eq!(dst[&f.to_string_lossy().to_string()].tags(), &["q3".to_string(), "report".to_string()]);
        // A second pull is a no-op (already reconciled).
        assert!(!pull(&mut dst, &f).unwrap());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pull_ctx_and_push_ctx_round_trip_through_the_config_store() {
        let dir = scratch("ctx");
        let f = dir.join("file.txt");
        std::fs::write(&f, b"x").unwrap();
        if !native_meta::is_supported(&f) {
            let _ = std::fs::remove_dir_all(&dir);
            return;
        }

        // A ctx whose config dir holds the tag store: tag the file, then push out to native metadata.
        let ctx = crate::ctx::HeadlessCtx::new(dir.join("home1"));
        crate::tags::set(&ctx, &f.to_string_lossy(), vec!["report".into(), "q3".into()], "red".into()).unwrap();
        push_ctx(&ctx, &f).unwrap();

        // A DIFFERENT (empty) config store pulls the native tags back in and persists them.
        let ctx2 = crate::ctx::HeadlessCtx::new(dir.join("home2"));
        let store = pull_ctx(&ctx2, &f).unwrap();
        assert_eq!(
            store[&f.to_string_lossy().to_string()].tags(),
            &["q3".to_string(), "report".to_string()]
        );
        // Persisted: reloading ctx2's store sees them without re-reading native.
        assert_eq!(crate::tags::load(&ctx2).unwrap().len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn push_untagged_path_removes_the_native_blob() {
        let dir = scratch("clear");
        let f = dir.join("file.txt");
        std::fs::write(&f, b"x").unwrap();
        if !native_meta::is_supported(&f) {
            let _ = std::fs::remove_dir_all(&dir);
            return;
        }

        // Push tags, then clear them and push again → the native blob is gone → pull finds nothing.
        let mut store = TagStore::new();
        tag_store_set(&mut store, &f.to_string_lossy(), vec!["temp".into()], "".into());
        push(&store, &f).unwrap();
        tag_store_set(&mut store, &f.to_string_lossy(), vec![], "".into()); // now empty → entry removed
        push(&store, &f).unwrap();

        let mut fresh = TagStore::new();
        assert!(!pull(&mut fresh, &f).unwrap(), "cleared tags leave no native blob to pull");
        assert!(native_meta::read(&f, &native_name()).unwrap().is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pull_on_a_missing_path_errors() {
        let dir = scratch("missing");
        let nope = dir.join("nope.txt");
        assert!(pull(&mut TagStore::new(), &nope).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
