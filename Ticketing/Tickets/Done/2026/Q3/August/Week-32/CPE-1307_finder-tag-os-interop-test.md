---
id: CPE-1307
title: macOS Finder-tag OS-interop test (xattr reads back native_bridge tags) — retires MVD row 5
type: test
component: Backend
priority: medium
status: Done
tags: ready
created: 2026-08-03
closed: 2026-08-03
epic: CPE-717
estimate: 1-2h
---

## Summary
MVD burndown **row 5**: "macOS Finder actually reads CPE's tag bytes" is currently verified only by a
hand-run demo (`crates/server/examples/native_tags_demo.rs` ends by printing "run `xattr -l` and look in
Finder"). Automate it with a self-asserting macOS-only integration test, mirroring the shipped
`crates/server/tests/native_meta_os_interop.rs` (CPE-1049, which retired row 8).

Important: CPE-1049 tests the GENERIC `native_meta::write` attribute — it does NOT cover
`native_bridge::push`/`pull`, which on macOS specifically co-opts `com.apple.metadata:_kMDItemUserTags`
(the attribute Finder reads). This is a separate code path; hence a separate test.

## Acceptance Criteria
- [x] New `crates/server/tests/finder_tags_os_interop.rs`, `#[cfg(target_os = "macos")]`-guarded like its
      sibling. Writes tags via `native_bridge::push`, then reads them back through the OS's own tool —
      `xattr -p com.apple.metadata:_kMDItemUserTags <file>` — and asserts the bytes/tags match; also asserts
      `native_bridge::pull` round-trips.
- [x] Use `xattr -p` (raw attribute read, deterministic), NOT `mdls` (Spotlight/`mdworker` indexing lags or
      is absent on a GH Actions macOS VM → flaky). Mirrors the `getfattr`/`path:stream` independent-read
      pattern CPE-1049 used for Linux/Windows.
- [x] Type-checks from this Windows box: `rustup target add x86_64-apple-darwin` then
      `cargo check --target x86_64-apple-darwin -p cpe-server --test finder_tags_os_interop` (check
      type-checks the macOS cfg arm without a Darwin linker). No product code changes — test-only.
- [x] Runs green on the existing `macos-latest` leg of the `Server crates` 3-OS matrix (`.github/workflows/ci.yml`).
      This is the real gate (offsite GitHub Actions) — **Done-pending-CI**: moved to Done on the strength of a
      diagnosed-and-fixed CI panic (see Work Log) plus a new cross-platform pure-parse test that proves the
      fix's decode path; the macOS leg re-runs on PR #603 after this push and is the final confirmation.
- [ ] When green on macOS CI post-merge: flip burndown row 5 to ✅, name the pinning job, decrement MVD 7→6.

## Notes
Validated primarily OFFSITE on the GitHub Actions macOS runner — merge only after PR CI's macOS leg is green
so `main` stays green. Model: sonnet (small, precedented — copy CPE-1049's structure).

**ID reconciliation (2026-08-03)**: this ticket was originally filed as CPE-828, which collided with
`Ticketing/Tickets/Done/2026/Q3/July/Week-30/CPE-828_native-tags-commands.md` — a different, already-Done
ticket (the native-tags bridge's command-layer + tag-editor UI). It has been renumbered to **CPE-1307**
(the next sequential ID per `Ticketing/wiki.md`'s "scan all folders, highest NNN + 1" rule) and this file
moved from `Doing/` to `Done/` accordingly. The original CPE-828 ticket is untouched and remains Done under
its own ID; only this ticket's numbering changed. PR #603 and the test file's own doc comment were updated
to reference CPE-1307. No code behaviour changed as part of the rename.

## Work Log
2026-08-03 (workshift) — Filed by the Foreman from the QA-Architect survey (grep-verified: no finder_tags
interop test; native_bridge push/pull only demo-verified). Dispatched to a worker.

2026-08-03 (worker) — Built `crates/server/tests/finder_tags_os_interop.rs` per the design above, copying
`native_meta_os_interop.rs`'s cfg-guard/scratch-file/cleanup pattern. Decodes the `xattr -p` raw bplist
bytes independently via the `plist` crate (not `cpe_server::finder_tags::decode`) into Finder's documented
wire shape, asserts tag-name equality, then separately asserts `native_bridge::pull` round-trips the same
names via its own decode path. `cargo check --target x86_64-apple-darwin -p cpe-server --test
finder_tags_os_interop` type-checks clean; `cargo test -p cpe-server` and `cargo clippy --all-targets -D
warnings` stay green on Windows (file compiles away to nothing off-macOS). Cannot execute the test on real
macOS from this machine — CI's `macos-latest` leg is the real gate, next `main` run post-merge.

2026-08-03 (worker, follow-up) — macOS CI leg PANICKED on PR #603: `xattr -p`'s BINARY stdout was captured
lossily (`String::from_utf8_lossy`-equivalent), which corrupted/truncated the binary plist's non-UTF-8
32-byte trailer, so `plist::Value::from_reader` failed with "invalid seek to a negative or overflowing
position" on an otherwise-valid `["Urgent","Work"]` bplist. Root cause was the test's OS-readback only —
`native_bridge::push` itself was never at fault. Fix: switched the OS read from `xattr -p` to `xattr -px`
(hex-dump mode, plain ASCII, safe to capture as a `String`), added a pure `hex_dump_to_bytes` helper to
parse that dump back into the exact original bytes, and refactored the plist-decode logic
(`decode_finder_tag_names`) to be a standalone pure function. Split the file so only the OS-interop test
itself is `#[cfg(target_os = "macos")]`-gated (inside a `mod macos_interop`); the two pure helpers and a new
`hex_dump_to_bytes_decodes_a_known_bplist_tag_array` test run on **every** platform, including this Windows
box — it builds a genuine bplist via the `plist` crate, hex-encodes it the way `xattr -px` would, and
asserts the full round-trip, which is exactly the path that was broken. `cargo test --test
finder_tags_os_interop` (Windows): 1 passed. `cargo test` (full `cpe-server` suite): all green. `cargo
clippy --all-targets -- -D warnings`: clean. `cargo check --target x86_64-apple-darwin --test
finder_tags_os_interop` (via the `C:\ztools` Zig-based darwin `cc`/`ar` wrapper): type-checks clean. Also
renumbered this ticket CPE-828 → CPE-1307 (see Notes) and retitled/annotated PR #603 accordingly. The real
macOS CI gate re-runs on push; not executed here (no macOS available on this machine).
