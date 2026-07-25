---
id: CPE-829
title: macOS Finder-tag bplist codec (_kMDItemUserTags)
type: feature
component: Backend
priority: low
status: Done
tags: ready
created: 2026-07-21
closed: 2026-07-21
epic: CPE-717
estimate: 1-2h
---

## Summary
Third child of CPE-717 (native metadata bridge), split from CPE-827. macOS Finder tags live in the
`com.apple.metadata:_kMDItemUserTags` extended attribute as a **binary property list** — an array of
strings, each `"Name"` or `"Name\n<colorIndex>"` (colour 0–7). A pure `cpe-server::finder_tags` module
that encodes/decodes `Vec<FinderTag> ⇄ bplist bytes` (via the pure-Rust `plist` crate), plus a helper to
project Finder tags down to CPE tag names for the CPE-827 reconciliation.

Cross-platform code (built + round-trip-tested on every OS), so it's fully verifiable headlessly. Note:
**byte-compat with real macOS Finder can only be verified on a Mac** — that interop check is deferred to
the attended CPE-828 wiring. This ticket proves the codec round-trips through the actual binary-plist
format and parses the documented Finder tag shape.

## Acceptance Criteria
- [x] `FinderTag { name, color }` + `encode(&[FinderTag]) -> Vec<u8>` (binary plist array of `"name\ncolor"`
      strings) and `decode(&[u8]) -> Vec<FinderTag>`.
- [x] `encode` → `decode` round-trips exactly (name + colour), including tags with no colour and
      multi-word names.
- [x] `decode` is lenient: a non-plist / foreign / empty blob yields an empty list, never an error.
- [x] A `names(&[FinderTag]) -> Vec<String>` projection for reconciliation, and the
      `FINDER_TAGS_XATTR` name constant are exposed.
- [x] `cargo test` green in `crates/server` (127 passed); `cargo clippy --all-targets -D warnings` clean;
      the `plist` dep is pure-Rust. App untouched.

## Resolution
Added **`cpe-server::finder_tags`** — the macOS Finder-tag codec:

- `FinderTag { name, color }` with the documented Finder palette (0 none · 1 grey · 2 green · 3 purple ·
  4 blue · 5 yellow · 6 red · 7 orange).
- `encode(&[FinderTag]) -> Vec<u8>` writes the `_kMDItemUserTags` **binary plist** (array of `"name\ncolor"`
  strings, or bare `"name"` when uncoloured) via the pure-Rust `plist` crate.
- `decode(&[u8]) -> Vec<FinderTag>` reads it back — **lenient**: a non-plist / foreign / empty blob → empty
  list, never an error; only string array elements are taken. A non-numeric colour degrades to 0.
- `names(&[FinderTag])` projects to plain CPE tag names for CPE-827 reconciliation; `FINDER_TAGS_XATTR`
  exposes the attribute name.

Files: `crates/server/src/finder_tags.rs` (impl + 5 tests), registered in `lib.rs`; `crates/server/
Cargo.toml` gains `plist = "1"` (pure-Rust, no system libs).

Verification (local, Windows): the round-trip test proves encode→decode is exact through the **genuine
binary-plist format** (the `plist` crate parses what it wrote). `cargo test` in `crates/server` → **127
passed** (was 122); `cargo clippy --all-targets -D warnings` clean. App untouched.

Deferred (honest): **byte-compat with real macOS Finder** — that Finder reads our bytes and we read
Finder's — can only be verified on a Mac. That interop check rides the attended CPE-828 wiring; this ticket
delivers and self-verifies the codec.

## Work Log
- 2026-07-21 — Picked up (dayshift, autonomous). Estimate 1-2h. Implemented `finder_tags` with the `plist`
  crate; encode/decode round-trip + lenient decode + wire-form + names projection tested. 127 tests green,
  clippy clean. Real-Finder byte-interop deferred to CPE-828 (mac). Closing.
