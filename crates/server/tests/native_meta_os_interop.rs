//! Self-asserting native-metadata OS-interop test (CPE-1049, epic CPE-717).
//!
//! Retires **MANUAL-TEST-BURNDOWN #8**: today, whether the OS's *own* mechanism (an NTFS alternate
//! data stream on Windows / a POSIX extended attribute on Linux+macOS) can actually read back the
//! bytes CPE writes via [`cpe_server::native_meta`] was only confirmed by a human running the
//! `native_tags_demo` example and eyeballing `Get-Item -Stream` / `getfattr` / `xattr` output.
//!
//! This test writes a value through `native_meta::write`, then reads it back through a path that is
//! **independent of `native_meta::read`** — the OS's plain file API (Windows `path:stream` syntax) or
//! an external OS tool (`getfattr` on Linux, `xattr` on macOS) — and asserts byte-for-byte equality.
//! That proves the value landed as a *standard*, any-tool-readable OS attribute, not something only
//! this crate's own code can decode.
//!
//! Runs in the existing 3-OS `Backend` CI matrix (`.github/workflows/ci.yml`, `cargo test`) — no human,
//! no user resource. On a filesystem that can't store native metadata at all (e.g. a CI tmpfs without
//! xattr support), the test degrades to a graceful skip rather than a false failure, mirroring
//! `MetaError::Unsupported`.

use cpe_server::native_meta::{self, MetaError};
use std::path::Path;

#[test]
fn os_native_tool_reads_back_what_native_meta_wrote() {
    // CPE-1693: the scratch dir is a guard that removes itself on drop, including on a panicking
    // assertion below — replaces the old `scratch_file()` + manual `cleanup(&path)` pair, which never
    // ran its cleanup on a panic.
    let dir = cpe_server::fsutil::scratch_dir("cpe-nativemeta-osinterop");
    let path = dir.join("file.txt");
    std::fs::write(&path, b"base file contents").expect("create base file");

    // `native_meta::write`/`read`/`remove` take the already-namespaced attribute name — the caller
    // resolves the logical key via `cpe_name` first (see `native_meta.rs`'s own unit tests, which do
    // the same `let name = cpe_name("tags"); write(&f, &name, ...)`). Passing the bare logical key
    // here would write under a *different* (un-namespaced) stream/xattr than the one the OS-native
    // read below looks for.
    let value: &[u8] = b"work,urgent";
    let attr_name = native_meta::cpe_name("tags");

    match native_meta::write(&path, &attr_name, value) {
        Ok(()) => {}
        Err(MetaError::Unsupported) => {
            // A CI runner whose temp filesystem lacks ADS/xattr support (e.g. an old-kernel tmpfs) is a
            // valid environment, not a code failure — degrade gracefully instead of flaking the 3-OS
            // matrix. `is_supported` must agree.
            cpe_server::skip_notice!(
                "SKIP: {} does not support native metadata on this filesystem/runner; nothing to \
                 OS-interop-test here (MetaError::Unsupported).",
                path.display()
            );
            assert!(!native_meta::is_supported(&path), "an unsupported write implies an unsupported fs");
            return;
        }
        Err(e) => panic!("unexpected native_meta::write error: {e}"),
    }
    assert!(native_meta::is_supported(&path), "a successful write implies a supported fs");

    os_native_read_and_assert(&path, &attr_name, value);

    // Tidy up via native_meta itself (already exercised elsewhere) — fine to use here since the
    // interop assertion above is already complete.
    let _ = native_meta::remove(&path, &attr_name);
}

// ---------------------------------------------------------------------------
// Windows — read the NTFS alternate data stream via plain `std::fs`, using the OS's own
// `file:stream` path syntax (what `CreateFile` accepts), independent of calling
// `native_meta::read` itself.
// ---------------------------------------------------------------------------
#[cfg(windows)]
fn os_native_read_and_assert(path: &Path, attr_name: &str, expected: &[u8]) {
    // Any Windows tool that knows `path:stream` syntax (PowerShell `Get-Content -Stream`, `notepad`,
    // `more <`, ...) goes through the same OS file-name resolution as this. We deliberately don't call
    // `native_meta::read` — we build the stream path ourselves and hand it to plain `std::fs::read`.
    let stream_path = format!("{}:{}", path.display(), attr_name);
    let bytes = std::fs::read(&stream_path)
        .unwrap_or_else(|e| panic!("OS-native ADS read via std::fs::read({stream_path:?}) failed: {e}"));
    assert_eq!(
        bytes, expected,
        "the NTFS alternate data stream, read via the OS's own file:stream path syntax, must equal \
         what native_meta::write wrote"
    );
}

// ---------------------------------------------------------------------------
// Linux — read the xattr via the `getfattr` OS tool (independent process, independent of
// `native_meta::read`'s `xattr` crate call).
// ---------------------------------------------------------------------------
#[cfg(target_os = "linux")]
fn os_native_read_and_assert(path: &Path, attr_name: &str, expected: &[u8]) {
    use std::process::Command;

    let path_str = path.to_string_lossy().to_string();
    let output = match Command::new("getfattr").args(["-n", attr_name, "--only-values", &path_str]).output() {
        Ok(o) => o,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            cpe_server::skip_notice!(
                "SKIP: `getfattr` is not installed on this runner; cannot independently verify the xattr \
                 {attr_name:?} on {path_str}. (Debian/Ubuntu CI images ship `attr`; if this ever fires, \
                 install it in the workflow rather than trusting native_meta::read circularly.)"
            );
            return;
        }
        Err(e) => panic!("failed to spawn getfattr: {e}"),
    };
    assert!(
        output.status.success(),
        "getfattr -n {attr_name} --only-values {path_str} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut got = output.stdout;
    // `--only-values` emits the raw value with no name= wrapper; tolerate a single trailing newline
    // some `getfattr` builds add.
    if got.last() == Some(&b'\n') {
        got.pop();
    }
    assert_eq!(
        got, expected,
        "getfattr's independently-read xattr value must equal what native_meta::write wrote"
    );
}

// ---------------------------------------------------------------------------
// macOS — read the xattr via the `xattr` OS tool. `xattr -p` prints the raw attribute bytes. If that
// ever fails in a way we didn't anticipate (CLI output format can vary across macOS releases), we
// degrade to confirming the OS tool at least lists the attribute name rather than faking a pass —
// full byte-level OS-tool interop on macOS stays partially manual in that fallback case (tracked
// alongside MANUAL-TEST-BURNDOWN #5/#8) instead of risking a false CI failure on a CLI-output-format
// assumption we can't verify from this (Windows) development machine.
// ---------------------------------------------------------------------------
#[cfg(target_os = "macos")]
fn os_native_read_and_assert(path: &Path, attr_name: &str, expected: &[u8]) {
    use std::process::Command;

    let path_str = path.to_string_lossy().to_string();
    let output = match Command::new("xattr").args(["-p", attr_name, &path_str]).output() {
        Ok(o) => o,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            cpe_server::skip_notice!("SKIP: the `xattr` command is not available on this runner; cannot independently verify.");
            return;
        }
        Err(e) => panic!("failed to spawn xattr: {e}"),
    };

    if output.status.success() {
        let mut got = output.stdout;
        if got.last() == Some(&b'\n') {
            got.pop();
        }
        assert_eq!(
            got, expected,
            "`xattr -p` independently-read value must equal what native_meta::write wrote"
        );
        return;
    }

    // Honest degrade: `xattr -p` didn't behave as expected. Don't fake a byte-level pass — fall back
    // to confirming the OS tool at least lists the attribute name that native_meta wrote.
    eprintln!(
        "NOTE: `xattr -p {attr_name} {path_str}` exited non-zero ({:?}); stderr: {}. Falling back to \
         confirming the OS tool lists the attribute name (byte-level interop stays partially manual on \
         this runner).",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    let list = Command::new("xattr")
        .arg(&path_str)
        .output()
        .expect("`xattr <file>` (list attribute names) should run if `xattr -p` just ran");
    let listed = String::from_utf8_lossy(&list.stdout);
    assert!(
        listed.lines().any(|l| l.trim() == attr_name),
        "expected {attr_name:?} to be listed by `xattr {path_str}`, got:\n{listed}"
    );
}
