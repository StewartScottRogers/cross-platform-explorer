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
