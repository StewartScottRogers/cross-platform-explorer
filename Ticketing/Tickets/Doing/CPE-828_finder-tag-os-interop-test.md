---
id: CPE-828
title: macOS Finder-tag OS-interop test (xattr reads back native_bridge tags) — retires MVD row 5
type: test
component: Backend
priority: medium
tags: ready
created: 2026-08-03
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
- [ ] Runs green on the existing `macos-latest` leg of the `Server crates` 3-OS matrix (`.github/workflows/ci.yml`).
      This is the real gate (offsite GitHub Actions).
- [ ] When green on macOS CI post-merge: flip burndown row 5 to ✅, name the pinning job, decrement MVD 7→6.

## Notes
Validated primarily OFFSITE on the GitHub Actions macOS runner — merge only after PR CI's macOS leg is green
so `main` stays green. Model: sonnet (small, precedented — copy CPE-1049's structure).

**ID collision flagged**: `Ticketing/Tickets/Done/2026/Q3/July/Week-30/CPE-828_native-tags-commands.md`
already occupies ID CPE-828 (a different, already-Done ticket — the bridge's command-layer + tag-editor
UI). This ticket was filed re-using CPE-828 rather than the next sequential ID (should have been ≥1304 per
`Ticketing/wiki.md`'s "scan all folders, highest NNN + 1" rule). Proceeding under the ID as filed by the
Foreman since a worker shouldn't unilaterally renumber a dispatched ticket; flagging here for the
QA-Architect/Foreman to reconcile (e.g. renumber one of the two on a future ticketing-organize pass).

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
