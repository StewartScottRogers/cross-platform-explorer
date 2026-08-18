//! Self-asserting macOS Finder-tag OS-interop test (CPE-1307, epic CPE-717).
//!
//! Note: this ticket was originally filed as CPE-828, which collided with an already-Done ticket
//! (`CPE-828_native-tags-commands.md`, the native-tags bridge command layer). It was renumbered to
//! CPE-1307; the code and test behaviour below are unchanged by the renumbering.
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
//! ## Binary-safe readback
//! `xattr -p <attr> <file>` writes the attribute's **raw binary** bytes to stdout. A binary plist is
//! not valid UTF-8 (its 32-byte trailer in particular is arbitrary binary), so capturing that stdout as
//! a lossy/UTF-8 `String` (`String::from_utf8_lossy`, or anything that round-trips through `char`)
//! corrupts or truncates those trailer bytes — which is exactly what made this test flaky in CI: it
//! panicked decoding a plist that looked like `bplist00` + the array payload with the trailer chopped
//! off. The fix is to never let the binary attribute value pass through a `String` at all. We use
//! `xattr -p -x` (equivalently `-px`), which asks `xattr` itself to print the value as a hex dump —
//! this dump *is* plain ASCII (safe to capture as a `String`), and we parse it back into the original
//! bytes ourselves via [`hex_dump_to_bytes`]. `hex_dump_to_bytes` and its sibling
//! [`decode_finder_tag_names`] are pure functions with no OS dependency, exercised on every platform by
//! `hex_dump_to_bytes_decodes_a_known_bplist_tag_array` below (built with the `plist` crate itself, so
//! it can't drift from what real macOS `xattr -px` output looks like: whitespace/newline-separated hex
//! byte pairs).
//!
//! Runs on the `macos-latest` leg of CI's 3-OS `Server crates` matrix (`.github/workflows/ci.yml`,
//! `cargo test -p cpe-server`) — no human, no user resource. The OS-interop test itself is gated on
//! `target_os = "macos"`: Windows/Linux have no Finder-tag concept (their bridge writes CPE's own JSON
//! blob, not a Finder bplist, per `native_bridge.rs`), so there is nothing for it to assert there. The
//! pure hex-dump/plist-decode helpers and their test are **not** macOS-gated, so this parse path is
//! proven on every CI leg (and locally on Windows), not just verified after the fact on macOS.

use std::io::Cursor;

/// Parse an `xattr -x`-style hex dump (whitespace/newline-separated pairs of hex digits, all
/// printable ASCII — e.g. `"62 70 6c 69 73 74 30 30\nA2 01 02 ..."`) into the raw bytes it represents.
///
/// This is deliberately tolerant of any amount/kind of whitespace between byte tokens (`xattr`'s exact
/// line-wrapping is not part of its documented contract), but panics on a malformed token so a broken
/// capture fails loudly here rather than silently mis-decoding downstream.
fn hex_dump_to_bytes(hex: &str) -> Vec<u8> {
    hex.split_whitespace()
        .map(|tok| {
            u8::from_str_radix(tok, 16)
                .unwrap_or_else(|e| panic!("invalid hex byte token {tok:?} in xattr hex dump: {e}"))
        })
        .collect()
}

/// Decode a `_kMDItemUserTags` binary-plist blob into plain tag names, the way Finder itself would:
/// an array of plist strings, each either `"Name"` or `"Name\n<colorIndex>"` (see `finder_tags.rs`'s
/// `FinderTag::from_wire` for the same wire convention). This is intentionally a *fresh*, independent
/// decode here (using the `plist` crate directly) rather than a call into `cpe_server::finder_tags`,
/// so the assertion doesn't just check "our encoder and our decoder agree with each other" — it checks
/// that the bytes actually on disk parse as the standard bplist shape Finder consumes.
fn decode_finder_tag_names(raw: &[u8]) -> Vec<String> {
    let value = plist::Value::from_reader(Cursor::new(raw))
        .unwrap_or_else(|e| panic!("xattr output was not a valid binary plist: {e}\nraw bytes: {raw:?}"));
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

/// Cross-platform, non-macOS-gated proof of the parse path used above. Builds a genuine binary plist
/// (with its full, correct trailer) for `["Urgent", "Work"]` via the `plist` crate — the same crate
/// `finder_tags.rs` and this test's OS leg use — hex-encodes it exactly the shape `xattr -px` emits
/// (whitespace-separated hex byte pairs), then runs it through `hex_dump_to_bytes` +
/// `decode_finder_tag_names` and asserts the round-trip. This is what actually caught (and now guards
/// against) the truncated-trailer bug: a lossy/UTF-8 capture of these same bytes fails this test too if
/// swapped in, because the trailer contains non-UTF-8 bytes.
#[test]
fn hex_dump_to_bytes_decodes_a_known_bplist_tag_array() {
    let tags = vec!["Urgent".to_string(), "Work".to_string()];
    let value = plist::Value::Array(tags.iter().cloned().map(plist::Value::String).collect());

    let mut bytes = Vec::new();
    plist::to_writer_binary(&mut bytes, &value).expect("encode known bplist array");

    // Simulate `xattr -px`'s hex-dump stdout: whitespace-separated pairs of hex digits, wrapped across
    // lines (the exact wrap width doesn't matter — hex_dump_to_bytes tolerates any whitespace).
    let hex_dump: String = bytes
        .chunks(16)
        .map(|line| {
            line.iter()
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n");

    let decoded_bytes = hex_dump_to_bytes(&hex_dump);
    assert_eq!(decoded_bytes, bytes, "hex_dump_to_bytes must exactly reconstruct the original bplist bytes");

    let names = decode_finder_tag_names(&decoded_bytes);
    assert_eq!(names, tags, "decode_finder_tag_names must recover the original tag names from the reconstructed bplist");
}

#[cfg(target_os = "macos")]
mod macos_interop {
    use super::{decode_finder_tag_names, hex_dump_to_bytes};
    use std::process::Command;

    use cpe_server::native_bridge;
    use cpe_server::tags::{tag_store_set, TagStore};

    #[test]
    fn xattr_tool_reads_back_the_finder_tags_native_bridge_pushed() {
        // CPE-1693: the scratch dir is a guard that removes itself on drop, including on a panicking
        // assertion below — replaces the old `scratch_file()` + manual `cleanup(&path)` pair, which
        // never ran its cleanup on a panic.
        let dir = cpe_server::fsutil::scratch_dir("cpe-findertags-osinterop");
        let path = dir.join("file.txt");
        std::fs::write(&path, b"base file contents").expect("create base file");

        // Tag the path in an internal TagStore, then push it out to native (Finder) metadata. `push` is
        // the bridge entry point — the same one the Properties UI's "Push" button calls.
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
        // native_bridge::pull) and ask for the raw attribute bytes AS A HEX DUMP (`-x`), not raw stdout.
        // A binary plist's trailer bytes are not valid UTF-8; capturing raw stdout as a `String` (even
        // lossily) corrupts/truncates those bytes. `-x` makes `xattr` itself emit a plain-ASCII hex
        // dump, which is safe to capture as a `String` and then parse back into bytes ourselves via
        // `hex_dump_to_bytes` — see the module doc comment above for the full story.
        let path_str = path.to_string_lossy().to_string();
        let output = Command::new("xattr")
            .args(["-px", &attr_name, &path_str])
            .output()
            .unwrap_or_else(|e| panic!("failed to spawn `xattr -px {attr_name} {path_str}`: {e}"));
        assert!(
            output.status.success(),
            "`xattr -px {attr_name} {path_str}` failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let hex_dump = String::from_utf8(output.stdout)
            .unwrap_or_else(|e| panic!("xattr -x hex-dump stdout was not valid UTF-8 (unexpected — it should be plain ASCII hex): {e}"));
        let raw = hex_dump_to_bytes(&hex_dump);

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
            "native_bridge::pull must round-trip the same tag names Finder (and xattr -px) read"
        );

        let _ = native_bridge::push(&TagStore::new(), &path); // clear the native blob (untagged push removes it)
    }
}
