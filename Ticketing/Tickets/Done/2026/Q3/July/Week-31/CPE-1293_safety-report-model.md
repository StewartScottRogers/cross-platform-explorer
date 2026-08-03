---
id: CPE-1293
title: "Frontend File-Health safety-report model"
type: feature
component: frontend
priority: medium
status: Done
tags: ready
created: 2026-08-03
epic: CPE-1002
---

## Summary
Now that the five safety-scan commands are exposed with typed bindings (CPE-1287), add the pure
front-end presentation model that unifies their results into one sorted "File Health" view-model — the
headless contract a future cleanup/review panel will render. Pure TS, vitest-tested, zero Rust.

## Build
- New `src/lib/safetyReport.ts` (+ `src/lib/safetyReport.test.ts`): a pure function that takes the five
  scan result shapes from `bindings.gen.ts` (`ArchiveSafetyReport`, `EmptyDirsReport`,
  `OrphanSidecarResult`, `DanglingReport`, `MismatchReport` — import the generated types; be tolerant if a
  field is absent) and produces a unified `FileHealthReport`:
  - a flat list of findings, each `{ category, severity, path, summary }` with a human one-line summary;
  - **severity ranking** (highest→lowest): disguised-file (type mismatch) & zip-bomb ≈ security > dangling
    link > orphaned sidecar > empty folder — sort findings by severity then path;
  - grouping by category with per-category counts;
  - an overall status + empty-state handling (no findings → a clean "healthy" result, not an error);
  - carry through each scan's `truncated`/`scanned` so the UI can note an incomplete sweep.
- No new dependency. Import the result types from `./bindings.gen` (do NOT redefine them); if a type isn't
  exported conveniently, define a minimal structural interface matching the generated shape.

## Acceptance criteria
- Given sample results from all five scans, the model returns findings sorted by the severity order above,
  grouped with correct counts, with sensible one-line summaries; an all-empty input yields a healthy,
  non-error result; a `truncated` scan is reflected in the output.
- `src/lib/safetyReport.test.ts` (vitest) covers ranking, grouping, empty-state, and truncation.
- `npm run check` clean; `npx vitest run src/lib/safetyReport.test.ts` green.

## Notes
Headless frontend (vitest). The eventual "File Health" panel UI is a later attended ticket. Epic CPE-1002.
Disjoint from all Rust work + all other frontend files.

## Work Log
- 2026-08-03 — File-Health model merged (#589). Reviewer APPROVE, 10/10 vitest, npm check clean, real bindings field-access verified, severity high(mismatch/zipbomb)>med(dangling)>low(orphan)>info(empty) with deterministic ordinal sort. Non-blocking: archive finding path = in-archive entry name (documented follow-up), scanned not carried.
