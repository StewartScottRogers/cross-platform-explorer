---
id: CPE-737
title: "EPIC: Integrity guard (bitrot detection)"
type: Task
status: In Progress
priority: Low
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed:
---

## Goal
Persist checksums for watched folders and periodically re-verify to catch silent corruption, unexpected
changes, or missing files, then alert with a clear "these N files changed unexpectedly" report.

## Why
Builds directly on the existing checksum feature to turn it from one-shot into continuous monitoring —
protecting archives, photo libraries, and backups from silent bitrot the OS won't warn about.

## Rough scope (areas, not child tickets)
- A checksum baseline store for chosen folders.
- A background verifier (scheduled / on-demand) comparing against the baseline.
- Change classification: edited-legitimately vs. corrupted vs. deleted.
- An alerts/report view with acknowledge/rebaseline actions.

## Open questions (resolve at activation)
- Distinguishing intended edits from corruption (mtime heuristics, prompts).
- Verification scheduling cost and how it honours the fast-when-off rule.
- Reuse of the checksum store with backup verification ([[CPE-736]]).

## Definition of Done
- Users baseline chosen folders and the verifier flags files that changed unexpectedly or went missing.
- Reports classify changes and allow acknowledge / rebaseline.
- Opt-in; no background verification runs unless configured.

## Work Log
2026-07-20 (autonomous) — Activated. Open questions resolved: **edit-vs-corruption = mtime heuristic**
(hash changed + mtime changed → intended edit; hash changed + mtime UNCHANGED → silent bitrot); **on-demand
verify** for v1 (background scheduler is a later child; opt-in, fast-when-off); reuses the existing sha256
checksum backend. Pure classifier lands first.

## Child tickets
1. **CPE-790** — Pure integrity model (`src/lib/integrity.ts`): `verifyManifest(baseline, current)` →
   intact/edited/corrupted/missing/new via the mtime heuristic + tolerant serialize/parse + `hasIssues`.
   Unit-tested. **Foundation, headless.**
2. **CPE-791** — Baseline store + on-demand verify: persist a folder's checksum baseline (reuse the sha256
   backend), re-scan and diff via CPE-790. **Backend/integration.** *(prereq: 790)*
3. **CPE-792** — Report view: "N files changed unexpectedly" with acknowledge / rebaseline. **GUI.**
   *(prereq: 790, 791)*

2026-07-21 (autonomous) — **v1 COMPLETE.** All three children done: CPE-790 (integrity model), CPE-791 (baseline store + on-demand verify — reconciled Deferred → Done, its frontend glue shipped with 792), CPE-792 (report view). Users baseline a folder, verify on demand, see changes classified (intact/edited/corrupted/missing/new) via the mtime heuristic, and rebaseline — reachable from the palette (`tool.integrity`). Remaining epic scope is the OPTIONAL scheduled/background verifier (a later child; opt-in, fast-when-off) — file it if/when wanted.

2026-07-21 (autonomous) — **EPIC COMPLETE (incl. monitoring).** Added backend verify (CPE-870 #148), verify-all-at-once (CPE-871 #149), and opt-in verify-on-startup (CPE-872 #150). DoD met: baseline + flag unexpected/missing + classify + rebaseline + opt-in monitoring. Only a richer while-running scheduler remains as an optional future child.
