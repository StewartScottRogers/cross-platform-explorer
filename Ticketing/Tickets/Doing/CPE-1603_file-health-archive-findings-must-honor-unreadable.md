---
id: CPE-1603
title: "File Health's archive path must honour unreadable/unreadable_entries before an archive tab ships"
type: Task
status: Backlog
priority: Low
component: Frontend
tags: [ready]
created: 2026-08-10
closed:
---

## Why
Flagged by the independent reviewer on CPE-1591 (PR #809). **Not a live bug — it is dead code today**, and
that is exactly why it is worth recording before it wakes up.

`src/lib/safetyReport.ts`'s `archiveFindings` / `buildFileHealth` (the CPE-1293 File Health unification
layer) reads only `report.report.flagged` from an `ArchiveSafetyReport`. It never inspects `unreadable`
(CPE-1320, whole-archive open failure) or the new `unreadable_entries` (CPE-1591, per-entry read failure,
typically an encrypted zip).

So if an encrypted or corrupt archive were fed through that path, it would contribute **zero findings and
no truncation signal** — presenting as clean. That is precisely the "safe-looking but never actually
scanned" failure mode CPE-1591 just eliminated in the Archive Safety dialog, waiting to be reintroduced
through a different door.

## Why it is not a bug yet
Confirmed dead for archives: `FileHealthDialog.svelte`'s `TabId` union is
`"dangling" | "mismatch" | "orphan" | "empty"` — there is no archive/zip-bomb tab, and no production caller
populates `FileHealthInputs.archive` (a grep found zero non-test occurrences). The module's own doc comment
says "future slices add tabs for the explorer's remaining file-health detectors", so this is a planned
future slice, not an oversight.

## Scope
Whoever wires the archive tab into File Health must make `archiveFindings` honour the same tri-state the
dialog now does: an archive that could not be read, in whole or in part, must surface as
**"couldn't be checked"** — never as an absence of findings. Suggested location: `src/lib/safetyReport.ts`
around the `archiveFindings` projection.

Add a test asserting that a report with `unreadable_entries > 0` and no flagged entries does **not** produce
an empty/clean File Health result.

## Notes
Small. Do this as part of the archive-tab slice rather than on its own if that slice is imminent — a fix
with no caller is hard to verify. Conflict surface: `src/lib/safetyReport.ts` and its test,
`FileHealthDialog.svelte`. Related: [[CPE-1602]] (the scan itself trusts archive metadata). Model: sonnet.

## Work Log
- `archiveFindings` in `src/lib/safetyReport.ts` now honours the full tri-state, mirroring
  `ArchiveSafetyDialog`'s already-fixed logic: alongside mapping `report.report.flagged` to `"zip-bomb"`
  findings (unchanged), it now ALSO emits a distinct `"archive-unreadable"` finding when
  `report.unreadable === true` ("archive could not be opened — safety not checked", the CPE-1320
  whole-archive-open-failure case) or when `report.unreadable_entries > 0` ("`N` entries could not be read
  — archive safety not fully checked", the CPE-1591/CPE-1602 per-entry case, singular/plural worded).
  These two signals are mutually exclusive per the backend's own contract (see the doc comments on
  `ArchiveSafetyReport` in `bindings.gen.ts`), so at most one extra finding is added — but it is always
  ADDED alongside any real flagged entries, never substituted for them.
- Added a new `Category` value `"archive-unreadable"` (distinct from `"zip-bomb"` — a confirmed bomb vs.
  "we don't know" must never share a bucket, per the CPE-1591/1612 lesson that an unreadable/unknown state
  has to be structurally distinct from "safe") with severity `"medium"` in `CATEGORY_SEVERITY`, and wired
  it into `buildFileHealth`'s `byCategory` initializer.
- `status`/`findings.length === 0` in `buildFileHealth` were already correct once `archiveFindings`
  returns a non-empty array — no change needed there; the whole fix is contained to the projection, as the
  ticket suggested.
- The report type carries no archive path of its own yet, so the two new findings use an empty `path` —
  documented in a code comment that the future archive-tab wiring should thread the real path through when
  it lands (that slice is still not wired — `FileHealthDialog.svelte`'s `TabId` union is unchanged, still
  no archive/zip-bomb tab, confirmed by grep — this ticket only fixes the projection ahead of that, as
  scoped).
- Updated `src/lib/safetyReport.test.ts`: added the new category key (count 0) to the two existing
  `byCategory` `toEqual` assertions that would otherwise now be missing a key, and added a new describe
  block "buildFileHealth archive unreadable/unreadable_entries (CPE-1603)" with 5 tests: whole-archive
  `unreadable: true` with zero flagged entries is NOT healthy; `unreadable_entries > 0` with zero flagged
  entries is NOT healthy/empty (the ticket's explicit acceptance criterion); singular "1 entry" wording;
  a real flagged zip-bomb entry alongside `unreadable_entries` produces BOTH findings (one doesn't replace
  the other); and a fully-clean, fully-assessed archive (`unreadable: false`, `unreadable_entries: 0`)
  still correctly reports healthy — no false positive introduced.
- No Rust changed (only consumed the existing `ArchiveSafetyReport` binding), so no `bindings.gen.ts`
  regen or `cargo` steps were needed. No user-facing doc update needed either — the archive tab still
  isn't wired into the UI, so there is nothing user-visible to document yet (CPE-579 only applies to
  shipped, user-facing surfaces).
- Verification (all run synchronously in the worktree):
  - `npx vitest run src/lib/safetyReport.test.ts` — 15 tests passed.
  - `npm run check` — 0 errors, 0 warnings.
  - Full `npx vitest run` — 272 files / 3316 tests passed, 0 failures.
