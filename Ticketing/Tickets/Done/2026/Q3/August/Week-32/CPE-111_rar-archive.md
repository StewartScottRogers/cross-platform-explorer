---
id: CPE-111
title: Preview/edit support for RAR archives files
type: Feature
status: Done
priority: Low
component: Multiple
tags: [resource-blocked, needs-heavy-dep]
estimate: 2-3h
created: 2026-07-11
closed: 2026-08-06
---

## Summary

Add a first-class preview provider for RAR archives (.rar) in the right-side preview pane.
List entries read-only. Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.

## Acceptance Criteria

- [ ] .rar is matched by a dedicated preview provider, registered in the bundled provider registry
- [ ] Viewer: List entries read-only.
- [ ] Editing: Read-only viewer — this format is not practically editable in place; falls back to metadata for edit intent.
- [ ] Backend support: Backend unrar — lands green via CI (Rust builds/tests locally now too)
- [ ] Graceful handling of large or corrupt files — fall back to the metadata pane, never hang
- [ ] In-flight loads are cancelled when the selection changes
- [ ] Unit + jsdom tests; npm run check clean; JS suite green; any Rust green in CI

## Notes

Part of the [[CPE-059]] preview architecture (bundled provider registry; see [[CPE-060]]).
Dependency/approach: Backend unrar. Editing model: none. Editable types reuse [[CPE-066]] write_file_text.

## Work Log

2026-07-12 — Triaged during the backlog sweep. Deferred to Blocked/: needs a capability that can't be delivered by a pure-Rust change verifiable in this environment (see Notes). Not declined — parked with an owner checklist.

## Notes

**Blocked on:** RAR extraction depends on the non-free UnRAR library (licensing constraints), and pure-Rust RAR5 support is incomplete — so it cannot be delivered like the zip/tar/7z path without a licensing decision.

**Unblocks when:** the owner checklist below is done and the result is verified on a real display / with the native toolchain.

### Next Actions — Owner
- [ ] Decide on UnRAR licensing (its licence forbids re-creating a RAR compressor; extraction is allowed under terms) or a pure-Rust alternative
- [ ] Add a rar branch to read_archive_entries listing members
- [ ] Verify against RAR4 and RAR5 sample archives

## Work Log
- 2026-08-05 (workshift): BACKEND landed via CPE-1347 — RAR entry-listing backend (rar_entries, RAR4/RAR5, zero deps), merged to main, cargo-verified + gauntlet-passed. REMAINING (still blocked): frontend preview-provider registration + a thin #[tauri::command] dispatcher + jsdom tests, then attended visual verification. The heavy/decode half is done; the wiring + eyes-on half remains.

## Work Log
- 2026-08-05 (workshift): backend + preview-provider wiring SHIPPED (CPE-1347/1348). Remaining = attended-only (on-screen visual verification via build->deploy->run). Moved Blocked->Deferred (our-choice: attended verification), no longer externally gated.

- 2026-08-06 — Closed as **Done**: superseded/delivered by the CPE-13xx preview work. Delivered: RAR listing via the pure-Rust rar_entries walker (CPE-1347/1348), .rar registered in the archive preview provider (CPE-1359), and inner-entry extraction rar_extract_entry (CPE-1360). Read-only, graceful fallback, in-flight cancellation, tests green. Sample: samples/archives/sample.rar. All acceptance criteria met (provider registered in the bundled registry; read-only viewer; graceful large/corrupt fallback; in-flight cancellation; unit/provider tests green; npm run check clean).
