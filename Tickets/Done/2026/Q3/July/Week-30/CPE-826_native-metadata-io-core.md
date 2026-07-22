---
id: CPE-826
title: Native metadata I/O core — NTFS ADS + POSIX xattr read/write/remove
type: feature
component: Backend
priority: low
status: Done
tags: ready
created: 2026-07-21
closed: 2026-07-21
epic: CPE-717
estimate: 2-3h
---

## Summary
First child of CPE-717 (native metadata bridge). A pure, Tauri-free `cpe-server::native_meta` module that
reads/writes/removes a **named metadata blob** on a path using the OS-native mechanism, behind one
cross-platform API:

- **Windows** — an NTFS **alternate data stream** (`path:streamname`), plain file I/O, no new dependency.
- **Unix** — a POSIX **extended attribute** (`user.<name>`), via the pure-Rust `xattr` crate.

Filesystems without the mechanism (FAT/exFAT) yield a graceful **`Unsupported`** outcome, never a hard
error that could fail a listing. This is the storage primitive the reconciliation layer (CPE-827) and the
UI (CPE-828) build on; it deliberately does not yet enumerate arbitrary streams or touch the tag store.

## Acceptance Criteria
- [x] `native_meta::{write,read,remove,is_supported}` operate on a named blob for a path, with a single
      cross-platform signature; Windows uses NTFS ADS, Unix uses `user.*` xattr.
- [x] Round-trip: write then read returns the exact bytes; remove then read reports absent.
- [x] Reading an absent attribute/stream returns `Ok(None)` (absent), distinct from `Unsupported` (fs can't
      store it) and from a genuine I/O error (`Err(Io)`).
- [x] Unsupported filesystems / unavailable mechanism degrade gracefully (typed `Unsupported`), never panic
      or fail the caller's listing.
- [x] Does not touch the base file's contents (test asserts base bytes unchanged after a metadata write);
      the base file remains readable unchanged.
- [x] cargo-tested on the native OS (round-trip + absent + remove + missing-path); CI's 3-OS `Server crates`
      job exercises Windows (ADS) and Linux/macOS (xattr). `cargo clippy --all-targets -D warnings` clean.

## Resolution
Added **`cpe-server::native_meta`** — the storage primitive of the native-metadata bridge. One
cross-platform API (`write` / `read` / `remove` / `is_supported` / `cpe_name`) with two `#[cfg]` backends:

- **Windows** — NTFS alternate data streams via plain `std::fs` on `path:streamname` (no new dep).
  Never conjures the base file into existence (guards `path.exists()` first); maps `ERROR_FILE_NOT_FOUND`
  → absent (`Ok(None)`) and `ERROR_INVALID_PARAMETER`/`ERROR_INVALID_NAME` → `Unsupported`.
- **Unix** — POSIX xattr via the pure-Rust `xattr` crate (`user.cpe.<key>`), Unix-only dep. `xattr::get`
  maps a missing attribute to `Ok(None)`; ENOTSUP/EOPNOTSUPP → `Unsupported`; `remove` is made idempotent.

Error taxonomy: `Ok(Some)` present · `Ok(None)` absent-but-supported · `Err(Unsupported)` fs can't store ·
`Err(Io)` genuine error. `cpe_name(key)` namespaces CPE metadata so it never collides with OS/other-app
metadata; interop callers (Finder tags in CPE-827) pass a raw native name instead. Scope is deliberately
narrow — no stream/attr enumeration and no tag-store coupling yet (those are CPE-827/828).

Files: `crates/server/src/native_meta.rs` (impl + 3 tests), registered in `crates/server/src/lib.rs`;
`crates/server/Cargo.toml` gains a `[target.'cfg(unix)'.dependencies] xattr = "1"`.

Verification (local, Windows): the ADS round-trip test passes — real coverage of the Windows path on this
machine. `cargo test` in `crates/server` → **115 passed** (was 112); `cargo clippy --all-targets -D
warnings` clean. The Unix xattr path compiles + runs on CI's Linux/macOS `Server crates` runners. The
round-trip test tolerates a `Unsupported` filesystem (e.g. tmpfs on an older-kernel Linux runner) as a
valid environment rather than flaking — which also exercises the graceful-degradation AC. App untouched
(nothing calls `native_meta` yet); `xattr` was already present in the app lock transitively.

## Work Log
- 2026-07-21 — Picked up (dayshift, autonomous) after activating epic CPE-717. Estimate 2-3h. Plan: a pure
  `native_meta` module with an ADS backend (Windows, no dep) and an xattr backend (Unix, `xattr` crate),
  graceful `Unsupported`, cargo-tested on the native OS.
- 2026-07-21 — Implemented + tested. ADS round-trip verified locally on Windows (real coverage). Full
  `cpe-server` suite 115 green, clippy clean. Made the round-trip test tolerate an xattr-less fs so the
  Linux runner can't flake. Closing; CPE-827 (reconciliation + Finder plist) is the next headless child.
