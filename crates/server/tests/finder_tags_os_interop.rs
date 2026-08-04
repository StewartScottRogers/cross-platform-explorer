//! Self-asserting macOS Finder-tag OS-interop test (CPE-828, epic CPE-717).
//!
//! Retires **MANUAL-TEST-BURNDOWN #5**: today, whether the macOS Finder actually reads back the tags
//! `cpe_server::native_bridge::push` writes onto `com.apple.metadata:_kMDItemUserTags` was only
//! confirmed by a human running the `native_tags_demo` example and eyeballing Finder's Get Info panel
//! (or `xattr -l`). This mirrors the sibling `native_meta_os_interop.rs` test (CPE-1049, which retired
//! row #8): it pushes tags through `native_bridge::push`, then reads the raw attribute bytes back
//! through the OS's own `xattr` CLI tool — deliberately **not** `native_meta::read` /
//! `native_bridge::pull` for that first read, to prove the bytes on disk are a standard, any-tool-
//! readable binary plist, not something only this crate's own decoder happens to accept — and decodes
//! that binary plist the same way Finder itself would (an array of `"Name"` / `"Name\n<color>"`
//! strings, per `finder_tags.rs`'s documented wire format). `native_bridge::pull` is then exercised
//! separately, asserting it round-trips the same tag names via its own decode path.
//!
//! We deliberately use `xattr -p` (a raw attribute read) rather than `mdls` — `mdls` reads through the
//! Spotlight index, which is asynchronous and can lag or be disabled on a CI VM, making it flaky. `xattr
//! -p` reads the on-disk attribute directly, so it is deterministic.
//!
//! Runs on the `macos-latest` leg of CI's 3-OS `Server crates` matrix (`.github/workflows/ci.yml`,
//! `cargo test -p cpe-server`) — no human, no user resource. Gated on `target_os = "macos"` for the
//! *whole file*: Windows/Linux have no Finder-tag concept (their bridge writes CPE's own JSON blob, not
//! a Finder bplist, per `native_bridge.rs`), so there is nothing for this test to assert there — the
//! file compiles away to nothing on those hosts rather than degrading at runtime.

#![cfg(target_os = "macos")]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use cpe_server::native_bridge;
use cpe_server::tags::{tag_store_set, TagStore};

/// A unique scratch file under the OS temp dir (APFS), mirroring the pattern used by
/// `native_meta_os_interop.rs` and `native_bridge`'s own unit tests.
fn scratch_file() -> PathBuf {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("cpe-findertags-osinterop-{}-{}", std::process::id(), n));
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir.join("file.txt")
}

fn cleanup(path: &Path) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// Decode a `_kMDItemUserTags` binary-plist blob into plain tag names, the way Finder itself would:
/// an array of plist strings, each either `"Name"` or `"Name\n<colorIndex>"` (see `finder_tags.rs`'s
/// `FinderTag::from_wire` for the same wire convention). This is intentionally a *fresh*, independent
/// decode here (using the `plist` crate directly) rather than a call into `cpe_server::finder_tags`,
/// so the assertion doesn't just check "our encoder and our decoder agree with each other" — it checks
/// that the bytes actually on disk parse as the standard bplist shape Finder consumes.
fn decode_finder_tag_names(raw: &[u8]) -> Vec<String> {
    let value = plist::Value::from_reader(std::io::Cursor::new(raw))
        .unwrap_or_else(|e| panic!("xattr -p output was not a valid binary plist: {e}\nraw bytes: {raw:?}"));
    let items = value
        .as_array()
        .unwrap_or_else(|| panic!("_kMDItemUserTags plist value must be an array, got: {value:?}"));
    items
        .iter()
        .map(|v| {
            let s = v
                .as_string()
                .unwrap_or_else(|| panic!("each Finder tag entry must be a plist string, got: {v:?}"));
            // Strip the "\n<colorIndex>" suffix Finder appends for a coloured tag, if present.
            match s.split_once('\n') {
                Some((name, _color)) => name.to_string(),
                None => s.to_string(),
            }
        })
        .collect()
}

#[test]
fn xattr_tool_reads_back_the_finder_tags_native_bridge_pushed() {
    let path = scratch_file();
    std::fs::write(&path, b"base file contents").expect("create base file");

    // Tag the path in an internal TagStore, then push it out to native (Finder) metadata. `push` is
    // CPE-828's bridge entry point — the same one the Properties UI's "Push" button calls.
    let mut tags = vec!["Work".to_string(), "Urgent".to_string()];
    tags.sort(); // TagStore stores/returns tags sorted; compare like-for-like below.
    let mut store = TagStore::new();
    tag_store_set(&mut store, &path.to_string_lossy(), tags.clone(), "red".to_string());

    if let Err(e) = native_bridge::push(&store, &path) {
        panic!("native_bridge::push failed: {e}");
    }

    let attr_name = native_bridge::native_name();
    assert_eq!(
        attr_name,
        cpe_server::finder_tags::FINDER_TAGS_XATTR,
        "on macOS the bridge must co-opt Finder's own tags attribute, not a CPE-private one"
    );

    // Independent OS-native read: shell out to the `xattr` CLI tool (NOT native_meta::read /
    // native_bridge::pull) and ask for the raw attribute bytes.
    let path_str = path.to_string_lossy().to_string();
    let output = Command::new("xattr")
        .args(["-p", &attr_name, &path_str])
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn `xattr -p {attr_name} {path_str}`: {e}"));
    assert!(
        output.status.success(),
        "`xattr -p {attr_name} {path_str}` failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut raw = output.stdout;
    // Tolerate a single trailing newline some `xattr` builds add to stdout.
    if raw.last() == Some(&b'\n') {
        raw.pop();
    }

    let names = decode_finder_tag_names(&raw);
    assert_eq!(
        names, tags,
        "the binary plist Finder itself would parse from {attr_name} must contain exactly the tag \
         names native_bridge::push wrote"
    );

    // native_bridge::pull, going through *this crate's own* decode path (finder_tags::decode), must
    // round-trip the same tag names — proving push/pull agree with each other AND with the OS tool.
    let mut fresh = TagStore::new();
    match native_bridge::pull(&mut fresh, &path) {
        Ok(changed) => assert!(changed, "pull should have imported the tags Finder now carries"),
        Err(e) => panic!("native_bridge::pull failed: {e}"),
    }
    let key = path.to_string_lossy().to_string();
    let pulled = fresh
        .get(&key)
        .unwrap_or_else(|| panic!("pull should have created a store entry for {key}"));
    let mut pulled_tags = pulled.tags().to_vec();
    pulled_tags.sort();
    assert_eq!(
        pulled_tags, tags,
        "native_bridge::pull must round-trip the same tag names Finder (and xattr -p) read"
    );

    let _ = native_bridge::push(&TagStore::new(), &path); // clear the native blob (untagged push removes it)
    cleanup(&path);
}
