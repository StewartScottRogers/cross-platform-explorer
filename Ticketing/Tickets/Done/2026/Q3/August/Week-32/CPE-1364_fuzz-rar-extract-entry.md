---
id: CPE-1364
title: "QA: fuzz/panic-safety coverage for rar_extract_entry (close the CPE-1360 gap)"
type: Task
status: Done
priority: Low
component: Backend
tags: [ready]
epic: CPE-1338
created: 2026-08-06
closed: 2026-08-06
---

## Problem

The panic-safety fuzz harness (`binary_data_preview_panic_safety.rs`, CPE-1354) covers the hand-rolled
RAR *listing* walker `rar::rar_entries`, but not `rar::rar_extract_entry` — the second hand-rolled RAR
path, added this session for CPE-1360. Extraction re-walks the header block AND slices the data area
(`data[data_start..next_pos]`) on untrusted offsets/sizes; the adversarial audit judged it bounds-safe,
but nothing pinned that against regression. A future edit to the offset math could reintroduce a slice
panic that crashes the app on a malformed `.rar` — the same class of bug as the PDF crash (CPE-1357).

## Fix

Added `rar_extract_entry_never_panics` to the fuzz harness: the same realistic RAR5 magic as the
`rar_entries` battery (a file header naming "hello.txt") plus a few STORED data bytes, run through the
shared adversarial `run_battery` + `catch_unwind` harness. It asks for both the named entry and a
non-matching name (exercising the walk-to-end-without-finding path), asserting no mutation panics and that
an empty file Errs. Pins the audit's "clean" finding as a permanent regression guard.

## Verification

`cargo test --test binary_data_preview_panic_safety` → 9 passed (was 8). Test-only change; no production
code touched.

## Work Log

- 2026-08-06 — QA Architect follow-up to the CPE-1363 adversarial audit: the audit found rar_extract_entry
  clean but unfuzzed; added the regression-pinning battery. Green locally.
