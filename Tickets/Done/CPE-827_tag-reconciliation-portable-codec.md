---
id: CPE-827
title: Tag reconciliation policy + portable metadata codec (native ⇄ internal TagStore)
type: feature
component: Backend
priority: low
status: Done
tags: ready
created: 2026-07-21
closed: 2026-07-21
epic: CPE-717
estimate: 2h
---

## Summary
Second child of CPE-717 (native metadata bridge). A pure, Tauri-free `cpe-server::native_tags` module
that (a) defines CPE's **portable metadata representation** — `{tags, label}` serialized as a JSON blob
suitable for storage in an NTFS ADS / POSIX xattr via CPE-826's `native_meta`, so a path's labels survive
as file metadata outside `tags.json` — and (b) implements the **push/pull reconciliation policy** between
that native representation and the internal [`crate::tags`] `TagStore`:

- **pull** (native → internal): non-destructive union of tag names into the path's entry; a non-empty
  native label is taken only when the internal label is empty (mirrors the existing `import` precedent).
- **push** (internal → native): the internal entry is authoritative; its normalized `{tags, label}` is the
  native representation to write.

Pure and fully cargo-tested; no new dependency. The macOS Finder-tag bplist codec is split to CPE-829 (its
real-Finder byte-compat needs a Mac); the command/UI wiring is CPE-828.

## Acceptance Criteria
- [x] A `NativeTags { tags, label }` portable representation with `encode`/`decode` (JSON bytes); decode is
      lenient — a malformed/foreign blob yields empty, never an error that could fail a listing.
- [x] `encode` → `decode` round-trips exactly; tags are normalized (trimmed, de-duped, sorted) on the way in.
- [x] Pull merges native tags into a `TagStore` entry non-destructively (existing internal tags kept),
      applying the label policy; returns whether the store changed.
- [x] Push produces the native representation from a store entry (internal authoritative), normalized.
- [x] Round-trip through both directions preserves the tag set; `TagEntry` gains the read accessors needed
      without breaking existing `tags` behaviour (122 tests pass, incl. all pre-existing tag tests).
- [x] `cargo test` green in `crates/server` (122 passed); `cargo clippy --all-targets -D warnings` clean.
      App untouched.

## Resolution
Added **`cpe-server::native_tags`** — the pure policy layer bridging the internal tag store and native
file metadata:

- `NativeTags { tags, label }` — CPE's portable representation, with `new` (normalizes tags: trim/dedupe/
  sort + trim label), `encode` (JSON bytes for an ADS/xattr blob), `decode` (lenient — garbage/foreign/
  empty → default, never an error), and `is_empty`.
- `pull_into_store(store, path, &NativeTags) -> bool` — native → internal: non-destructive union of tag
  names (existing internal tags kept), native label taken only when internal is empty; returns whether the
  store changed (idempotent re-pull is a no-op). Mirrors the `import`/`tag_store_merge` precedent.
- `push_from_store(store, path) -> Option<NativeTags>` — internal → native: the internal entry is
  authoritative; `None` when empty so the caller removes the native blob.

Also added `TagEntry::tags()` / `TagEntry::label()` read accessors (the struct's fields stay private).

Files: `crates/server/src/native_tags.rs` (impl + 7 tests, incl. a full internal→push→encode→decode→pull
round-trip), registered in `lib.rs`; `crates/server/src/tags.rs` gains the two accessors. No new
dependency; nothing in the app calls it yet (CPE-828 wires native_meta + native_tags into commands + UI).

Verification (local, Windows): `cargo test` in `crates/server` → **122 passed** (was 115); `cargo clippy
--all-targets -D warnings` clean. Fully headless — pure functions, no platform or I/O dependence.

Note: the macOS Finder-tag bplist codec was split to **CPE-829** (its byte-compat with real Finder can only
be verified on a Mac); the epic child list was updated accordingly.

## Work Log
- 2026-07-21 — Picked up (dayshift, autonomous). Estimate 2h. Scoped tight for full headless verifiability:
  pure reconciliation policy + CPE's portable JSON metadata codec, no new dep. Split the Finder bplist codec
  to CPE-829 (needs a Mac for real-Finder byte-interop).
- 2026-07-21 — Implemented `native_tags` + `TagEntry` accessors. 7 new tests (incl. full round-trip) pass;
  full `cpe-server` suite 122 green; clippy clean. Closing; CPE-828 (commands + UI, attended) and CPE-829
  (Finder bplist) remain.
