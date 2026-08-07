---
id: CPE-1415
title: "Security (defensive): catch_unwind around sevenz-rust parse + track upstream overflow panic"
type: Task
status: Backlog
priority: Low
component: Backend
tags: [ready]
epic: CPE-705
created: 2026-08-07
---

## Problem (CPE-1411 / PR #689 — already contained, defensive hardening)
`sevenz-rust` 0.6.1 panics ("attempt to add with overflow" / "capacity overflow") on a crafted `.7z`
(unchecked `SIGNATURE_HEADER_SIZE + next_header_offset` u64 add on attacker bytes; no-op size bound on 64-bit).
Pinned by `#[should_panic]` tests in `archive_panic_safety.rs`. The #689 reviewer confirmed this is ALREADY
CONTAINED: all Tauri call sites (`read_archive_entries`/`extract_archive*` in src-tauri/lib.rs) use
`spawn_blocking`, so the panic is caught at the Tokio task boundary → `Err(String)`, not a process crash (and no
`panic="abort"` override exists). So real-world impact = a failed 7z listing, not a dead app.

## Fix direction (low priority — nice-to-have)
Wrap the `sevenz-rust` parse in `archive.rs` (7z listing/extraction) in `std::panic::catch_unwind` to convert the
upstream panic into a clean `Err` at the call site (cleaner error, avoids burning a blocking-pool thread's unwind
machinery), rather than relying on the task boundary. AND track the upstream `sevenz-rust` bug: on a future crate
upgrade the `#[should_panic]` tests flip red — revisit + remove the workaround then. Consider whether a newer
`sevenz-rust` (or `sevenz-rust2`) fixes it. Defensive only; the current containment is adequate.
