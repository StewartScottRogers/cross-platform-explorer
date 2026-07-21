---
id: CPE-830
title: Native bridge orchestration — pull/push wiring native_meta + codecs + reconcile
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
Fourth child of CPE-717. The per-OS integration glue that turns the three isolated pieces (CPE-826
`native_meta` I/O, CPE-827 `native_tags` reconcile + portable codec, CPE-829 `finder_tags`) into a working
bridge — a `cpe-server::native_bridge` module with `pull` / `push` / `native_name`:

- **`native_name()`** — the attribute CPE reads/writes a path's tags under: on macOS the **Finder tags
  xattr** (`_kMDItemUserTags`, so tags interoperate with Finder); on Windows/Linux CPE's own namespaced
  ADS/xattr blob (no OS tag convention exists there).
- **`pull(store, path)`** — read the path's native metadata, decode with the OS-appropriate codec (Finder
  bplist on macOS → tag names; CPE JSON elsewhere → tags+label), and reconcile into the store
  non-destructively. Missing / unsupported metadata is a graceful no-op.
- **`push(store, path)`** — write the path's internal tags out to native metadata (internal
  authoritative), removing the native blob when the path has no tags. Unsupported filesystems degrade
  silently.

macOS carries only tag **names** (Finder has no separate CPE "label" colour); Windows/Linux carry
tags+label. Fully cargo-tested end-to-end with **real native I/O** on the running OS. Leaves CPE-828 as
just the thin Tauri commands + Properties UI + opt-in toggle.

## Acceptance Criteria
- [x] `native_name()`, `pull`, `push` in `cpe-server::native_bridge` with the per-OS codec/name policy.
- [x] End-to-end round-trip test on the running OS: set store tags for a temp file → `push` → `pull` into a
      fresh store → the tag set is recovered (guarded to a graceful skip when the temp fs can't store
      native metadata, so no flake).
- [x] `push` on an untagged path removes the native blob (round-trips to "no native tags").
- [x] Missing base path errors; unsupported filesystem / absent metadata are graceful no-ops, never fatal.
- [x] `cargo test` green in `crates/server` (130 passed); `cargo clippy --all-targets -D warnings` clean.
      App untouched.

## Resolution
Added **`cpe-server::native_bridge`** — the per-OS integration glue that turns CPE-826/827/829 into a
working bridge:

- `native_name()` — macOS → the Finder tags xattr (`_kMDItemUserTags`); Windows/Linux → CPE's namespaced
  `native_meta::cpe_name("tags")`.
- `pull(store, path)` — `native_meta::read` → `decode_native` (Finder bplist → names on macOS; CPE JSON
  `{tags,label}` elsewhere) → `native_tags::pull_into_store` (non-destructive union). Absent / unsupported
  metadata → `Ok(false)`; missing base path → `Err`.
- `push(store, path)` — `native_tags::push_from_store` (internal authoritative) → `encode_native` →
  `native_meta::write`, or `native_meta::remove` when the path has no tags. Unsupported fs degrades
  silently.

Per-OS codec selection (`decode_native`/`encode_native`) is `#[cfg]`-branched: macOS carries only tag
names (Finder has no CPE label); Windows/Linux carry tags+label via CPE's JSON blob.

Files: `crates/server/src/native_bridge.rs` (impl + 3 tests), registered in `lib.rs`.

Verification (local, Windows): the **end-to-end round-trip test passes through real NTFS alternate-data-
stream I/O** — `push` writes the ADS, `pull` reads+decodes+reconciles, tags recovered, base file contents
untouched. Plus untagged-clears-the-blob and missing-path-errors. `cargo test` in `crates/server` → **130
passed** (was 127); `cargo clippy --all-targets -D warnings` clean. On CI the same test drives real xattr
on Linux/macOS (Finder path on macOS). Tests skip gracefully on an xattr-less fs so no flake. App untouched
(CPE-828 exposes `pull`/`push` as Tauri commands + the Properties UI + opt-in toggle).

## Work Log
- 2026-07-21 — Picked up (dayshift, autonomous). Estimate 1-2h. Built `native_bridge` wiring native_meta +
  the codecs + reconcile with a per-OS name/codec policy (macOS Finder xattr; else CPE JSON blob).
  End-to-end round-trip verified locally through real NTFS ADS I/O. 130 tests green, clippy clean.
  Closing — CPE-717 is now backend-complete; only CPE-828 (thin Tauri commands + Properties UI, attended)
  remains.
