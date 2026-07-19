---
id: CPE-737
title: "EPIC: Integrity guard (bitrot detection)"
type: Task
status: Proposed
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
