---
id: CPE-412
title: "File checksum (SHA-256) in Properties"
type: Feature
status: Done
priority: Medium
component: Multiple
tags: [ready]
estimate: 1-2h
created: 2026-07-15
closed: 2026-07-15
---

## Summary

A good file explorer lets you verify a file's integrity. Add an on-demand **SHA-256** checksum to the
Properties dialog for a single file: a "Checksum" row with a **Compute** button (hashing is I/O-bound,
so it's opt-in, not automatic), showing the lowercase hex digest with a **Copy** button. Streamed on
the backend so a multi-GB file never loads into memory.

Filed during the Nightshift (research loop 1): no hash/checksum capability existed (`grep` for
sha256/checksum/hash_file found nothing outside the sidecar's catalog signing). Chosen as a clean,
self-contained, headlessly-testable feature that fills a real gap.

## Acceptance Criteria

- [x] Backend `hash_file(path)` command streams the file through SHA-256 and returns lowercase hex;
      errors (missing/unreadable/dir) come back as `Err`, never a panic.
- [x] Properties dialog (single **file** only) shows a Checksum row with a Compute button; while
      running shows a spinner/"computing…", then the hex with a Copy button; a failure shows an error.
- [x] No automatic hashing (opt-in per the fast/small/predictable tiebreaker) — the digest is only
      computed on click.
- [x] Verified: backend unit test (known vector), `npm run check` clean, JS suite green.

2026-07-15 — Implemented + verified. **Files:** `src-tauri/Cargo.toml` (+`sha2 = "0.10"`, pure-Rust);
`src-tauri/src/lib.rs` (streaming `hash_file` command, 64 KiB chunks, hex inline — no extra dep;
registered; +test against the canonical SHA-256("abc") vector + folder/missing → Err);
`src/lib/components/PropertiesDialog.svelte` (Checksum row for single files: Compute → computing… →
hex + Copy, error state; opt-in) and its new component test. Reachable via the existing Properties
action (context menu / Alt+Enter) — no new menu wiring. **Verified headlessly:** `cargo test hash_file`
pass, `cargo clippy` clean, `npm run check` 0/0, `npm test` **394 passed** (+2). GUI not driven
(Nightshift machine-share rule) — left for the user to eyeball.

## Resolution

Added an on-demand SHA-256 file checksum in Properties. Backend streams the file so multi-GB files
never load into memory; the UI computes only on click (fast/small/predictable) and offers a one-click
copy. Tradeoff: SHA-256 only (the ubiquitous integrity algorithm) — MD5/SHA-1/BLAKE3 could be added
later as a dropdown if a use case appears; not needed now.

## Work Log
2026-07-15 — Nightshift loop 1. Estimate: 1-2h. Plan: add `sha2` to `src-tauri`, a streaming
`hash_file` command (+test), and a Compute/Copy checksum row in `PropertiesDialog.svelte`. Verify
headlessly (cargo test + npm check + npm test); GUI left for the user (Nightshift avoids driving the
GUI while the machine may be in use).
