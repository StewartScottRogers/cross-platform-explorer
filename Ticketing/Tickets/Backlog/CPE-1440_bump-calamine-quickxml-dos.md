---
id: CPE-1440
title: "Security: bump calamine 0.26→0.36 to close quick-xml High-severity DoS (RUSTSEC-2026-0194/0195)"
type: Bug
status: Backlog
priority: High
component: Backend
tags: [ready]
epic: CPE-705
created: 2026-08-07
---
## Problem (found by the shift-1 dependency audit)
`quick-xml 0.31.0` (pulled transitively via `calamine 0.26.1`, the XLSX preview reader pinned in
`crates/server/Cargo.toml`) carries TWO High-severity (CVSS 7.5) advisories:
- **RUSTSEC-2026-0194** — quadratic runtime on duplicate-attribute tags (DoS).
- **RUSTSEC-2026-0195** — unbounded namespace-declaration allocation (memory DoS).
Both are reachable by opening a **crafted `.xlsx`** (untrusted input) in the preview pane → real DoS surface.

## Fix
Bump `calamine 0.26 → 0.36` in `crates/server/Cargo.toml`. calamine 0.36.1 (latest, 2026-07-27) requires
`quick-xml = "0.41"`, which fixes both advisories. `cargo audit` confirmed this is the only path pulling the
vulnerable quick-xml.

## Watch for API breakage
calamine 0.26→0.36 is 10 minor versions — its API may have changed. Study how `crates/server` uses calamine
(grep `calamine` — the XLSX/ODS preview reader, likely in a spreadsheet/data-grid module) and fix any breaking
API changes (Reader trait, `open_workbook`, `worksheet_range`, cell/DataType enum renames are the usual churn).
Keep the existing XLSX preview behavior identical. Run the spreadsheet/data-grid tests + panic-safety battery.

## Acceptance
- `calamine` at 0.36.x; `cargo tree -i quick-xml` shows ≥0.41 (advisories gone; re-run `cargo audit` to confirm
  RUSTSEC-2026-0194/0195 no longer fire).
- XLSX preview still works (existing tests green); the spreadsheet panic-safety battery green.
- `cargo build` + `cargo clippy --all-targets -- -D warnings` (crates/server, both feature modes) green.
- No new advisory introduced by the bump; `Cargo.lock` updated.

## Notes
Dependency Steward finding, shift-1 audit 2026-08-07. Untrusted-input DoS → High priority.
