---
id: CPE-1320
title: analyze_archive_safety can't distinguish corrupt-zip from empty → false "safe" report
type: bug
component: Backend
priority: medium
tags: ready
created: 2026-08-05
epic: CPE-1002
estimate: 1-2h
---

## Summary
Found by CPE-1318's UAT. `crates/server/src/archive_safety_scan.rs::analyze_archive_safety` is designed to
NEVER return `Err` — a garbage/corrupt/unreadable file yields a graceful `empty_report`
(`entries_scanned: 0, dangerous: false`), same as a genuinely-empty valid zip. So the Archive-Safety dialog
(CPE-1318) renders a corrupt-but-`.zip`-extensioned file as "No zip-bomb risk detected · 0 entries scanned" —
a silently-misleading "safe" result, the exact shape the frontend zip-only gate guards against for non-zip
extensions, but which slips through for a corrupt zip.

## Acceptance Criteria
- [ ] `analyze_archive_safety` distinguishes "opened fine, 0 entries" from "failed to open / not a valid zip".
      Either return `Err` on open-failure, or add a field to `ArchiveSafetyReport` (e.g. `opened: bool` /
      `unreadable: bool`) the dialog can surface as an error/unknown state instead of "safe".
- [ ] The Archive-Safety dialog (src/lib/components/ArchiveSafetyDialog.svelte) surfaces the corrupt/unreadable
      case as a visible error/unknown state, NOT "safe" (small frontend follow-up in the same ticket).
- [ ] Round-trip `cargo test` (a corrupt/truncated zip → the new signal) + a jsdom test for the dialog's
      unreadable state. clippy clean (3 cpe-server modes); `npm run check` + test:unit green.

## Notes
Pre-existing backend design (CPE-1281/1287) — analyze_archive_safety's "never panics/never errs" contract was
intentional for the batch scan path; this ticket adds a distinguishable signal without reintroducing panics.
Cargo-testable headless (backend) + a small jsdom frontend follow-up.

## Work Log
2026-08-05 (workshift run 2) — Filed by the Foreman from CPE-1318's UAT (corrupt-zip false-safe). Backlog.
