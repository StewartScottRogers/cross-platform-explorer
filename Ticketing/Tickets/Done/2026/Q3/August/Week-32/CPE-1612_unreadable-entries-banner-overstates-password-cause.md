---
id: CPE-1612
title: "Archive safety: the \"couldn't be checked\" banner blames password protection even when the cause was the verification budget"
type: Bug
status: Backlog
priority: Low
component: Frontend
tags: [ready]
created: 2026-08-10
closed:
---

## Why
Found by the independent UAT tester on CPE-1602 (PR #811). Not a regression from that PR — the string is
pre-existing CPE-1591 copy — but CPE-1602 widened the set of causes that land in the same bucket, so the
wording is now wrong more often than it was.

## The problem
`ArchiveSafetyDialog.svelte` renders the `unreadable_entries` state as:

> **N entries couldn't be read (likely password-protected) — this archive's safety could not be checked.**

That is accurate for the overwhelmingly common cause. But per the scanner's own doc comments, the same
`unreadable_entries` bucket now also catches an entry that **looked suspicious and whose bounded
verification ran out of budget** before reaching a verdict — a legitimate, unencrypted archive with one
oddly-shaped entry. In that case the banner tells the user their archive is probably password-protected,
which is simply the wrong explanation.

The tester could not construct a real archive landing in that sub-case (every suspicious entry it built
resolved cleanly within budget), so this is established by code inspection rather than a live repro — worth
saying plainly. The docs already caveat the scenario honestly; only the UI string overstates.

## Fix
Distinguish the causes so the banner explains the real one. Options, cheapest first:
1. Soften the copy to name both possibilities without asserting either ("couldn't be read — some entries are
   encrypted, or were too complex to verify within the safety check's limits").
2. Better: have the backend distinguish *why* an entry was skipped (encrypted vs. verification-budget) and
   let the dialog say the accurate thing. This costs a DTO field, so weigh it against the value.

Whichever: the verdict itself must stay unchanged — both causes correctly mean **"could not be checked"**,
never "safe". Only the explanation is at issue.

New/updated strings must land in **all 12 locale catalogs** (guard-tested).

## Acceptance criteria
- A budget-exhausted entry no longer tells the user the archive is likely password-protected.
- An actually-encrypted archive still says so plainly.
- Both still render the "could not be checked" state, never the safe banner.

## Notes
Conflict surface: `ArchiveSafetyDialog.svelte`, `src/lib/i18n.ts`, and — only if option 2 is chosen —
`crates/server/src/archive_safety_scan.rs`, the report DTO and `bindings.gen.ts`.
Related: [[CPE-1602]], [[CPE-1591]]. Model: sonnet.
