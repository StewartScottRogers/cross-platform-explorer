---
id: CPE-1344
title: "file_type: add OLE2/CFBF signature (legacy .doc/.xls/.ppt, .msi, .msg) — currently invisible to mismatch detection"
type: Task
status: Done
priority: Medium
component: cpe-server
tags: [ready]
epic: CPE-1000
created: 2026-08-05
closed: 2026-08-05
---

## Problem

`crates/server/src/file_type.rs` has no signature for the **OLE2 / Compound File Binary
Format (CFBF)** container: magic `D0 CF 11 E0 A1 B1 1A E1` at offset 0. This container backs
several common, still-widely-seen formats:

- legacy Microsoft Office: `.doc`, `.xls`, `.ppt`
- Windows Installer `.msi` (a well-known malware-disguise vector)
- Outlook `.msg`, Visio `.vsd`

`type_class.rs` already lists `doc`/`xls`/`ppt`/`msi` as legitimate expected extensions
elsewhere in the app, but `detect_type` returns `None` for all of them — so a malicious `.msi`
renamed to `.pdf` (or a `.doc` renamed to `.jpg`) is **silently invisible** to both the
true-type column and the mismatch tree-sweep. This is the same class of common-container hole
the file already handles for ZIP (docx/xlsx/pptx/odt/…).

## Fix

Add one `FileType::Ole2` (or `Cfbf`) variant, its signature check, label, and canonical
extensions — following the exact pattern of the existing ZIP container arm:

- signature: `matches_at(bytes, 0, &[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1])`
- extensions: `["doc", "xls", "ppt", "msi", "msg", "vsd"]` (all share the one container, so —
  like ZIP's list — none of them false-flags against the others)
- label: e.g. "Microsoft OLE2 / legacy Office document"
- place the check with the other exact offset-0 signatures; it cannot shadow or be shadowed
  (8-byte unique magic).

## Acceptance criteria

- `detect_type` of the 8-byte CFBF magic returns the new variant; `mismatch` returns `None`
  under each of its listed extensions and `Some` when the bytes are renamed to a foreign
  extension (e.g. an `.msi`-container renamed `.pdf` mismatches).
- New unit tests in the same style (detect + mismatch + label/extensions); short/empty input
  still never panics.
- `cargo test` + `clippy --all-targets -D warnings` (both feature modes) green. No new deps.

## Notes

Pure `cpe-server`, headless, cargo-testable. Touches `file_type.rs` — **no overlap** with
CPE-1343 (`type_mismatch_scan.rs`), so the two run in parallel. Surfaced by the 2026-08-05
frontier scan. Feeds epic CPE-1000.

## Work Log
- 2026-08-05 (workshift): PR #639 squash-merged to main (782cd700). Worker(sonnet); Reviewer APPROVE + UAT PASS (independent probe: OLE2 magic detects, 6 container exts not flagged, disguised .pdf flagged, no panic). All CI green 3 OS. FileType::Ole2 (doc/xls/ppt/msi/msg/vsd); no new deps. Surfaced by the frontier scan.
