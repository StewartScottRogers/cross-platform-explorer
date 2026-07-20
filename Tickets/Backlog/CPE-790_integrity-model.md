---
id: CPE-790
title: Pure integrity model (checksum manifest verify + bitrot classify)
type: feature
status: Open
priority: low
component: Frontend
tags: ready
created: 2026-07-20
closed:
epic: CPE-737
estimate: 1-2h
---

## Summary
Foundation for the integrity guard (epic CPE-737). A pure module (`src/lib/integrity.ts`) that compares a
checksum baseline against a fresh scan and classifies each path — the key value being the **bitrot
heuristic** that separates silent corruption from legitimate edits.

## Scope
- `ChecksumEntry { path; sha256; size; modified }`.
- `verifyManifest(baseline, current): IntegrityReport` with lists: `intact`, `edited` (hash changed AND
  mtime changed → intended), `corrupted` (hash changed but mtime UNCHANGED → silent bitrot), `missing`
  (baseline-only), `new` (current-only).
- Tolerant `parseManifest` / `serializeManifest`, and `hasIssues(report)` (corrupted or missing present).
- Pure + total (empty sides, matched paths).

## Acceptance Criteria
- [ ] Each status classified correctly; the mtime heuristic distinguishes corrupted vs edited.
- [ ] `hasIssues` true iff corrupted/missing; parse tolerant of malformed input; serialize round-trips.
- [ ] Pure + dependency-free; unit tests cover all; check + suite green.

## Notes
Reuses the existing sha256 checksum backend (CPE-412) for the actual hashes. Foundation for CPE-791/792. Headless.
