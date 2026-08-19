---
id: CPE-1784
title: "Ticketing/Tickets has 14 mojibake files and 12 BOM files, out of the CPE-1771 guard's scope"
type: task
priority: Low
status: Backlog
tags: ready
estimate: M
created: 2026-08-19
closed:
---

## Problem

Found while building the whole-repo mojibake guard for **CPE-1771**. `Ticketing/` was deliberately
excluded from that guard's scan (see `SCAN_EXCLUDE_DIRS` in `src/lib/mojibakeGuard.ts`), because a full
scan of it — measured, not estimated — turns up corruption far outside a single S-sized ticket's scope:

- **683** occurrences of the `â€…`-family mojibake signature across **14** files under
  `Ticketing/Tickets/**` (both `Backlog/` and `Done/`), e.g.
  `Ticketing/Tickets/Backlog/CPE-1685_route-s3-through-vfs-open-with-accesskey-credentials.md`.
- **12** of those (and others) additionally carry a UTF-8 BOM at byte 0 — the exact defect class CPE-1771's
  Problem statement traces to a PowerShell `Get-Content`/`Set-Content` round-trip, and the same failure
  mode that blocked release 0.57.66 (commit `86888aed`).

This is very likely the **same root cause** hitting the ticket-authoring path itself (tickets get written/
edited by the same PowerShell-adjacent tooling), not a one-off. It was excluded from CPE-1771's guard
because: (a) fixing 14 files / hundreds of occurrences is a much bigger job than the two named manifests,
(b) some `â€…` hits are almost certainly CPE-1771's/CPE-1752's own ticket text *quoting* the mojibake byte
signature as a literal example (self-referential, not corruption) and need per-file triage to tell those
apart from real corruption, and (c) `Ticketing/` churns constantly across concurrent sprint workers, so a
guard that reds on it repo-wide would be a standing false-alarm generator until this cleanup lands.

## What to do

- Enumerate every `Ticketing/**/*.md` file with the mojibake signature or a leading BOM (start from the
  684-occurrence/14-file, 12-BOM count above; re-measure at pickup time since the queue moves).
- For each occurrence, distinguish real corruption (repair byte-exact, no PowerShell round-trip) from a
  deliberate literal quotation of the mojibake pattern in ticket prose (e.g. CPE-1771's and CPE-1752's own
  tickets describing the byte signature) — leave the latter alone, they are not bugs.
- Strip the BOM from all 12 files (verify against the git blob, not just the checkout).
- Once `Ticketing/` is clean, either fold it into `src/lib/mojibakeGuard.ts`'s scan scope (remove the
  exclusion) or add a narrower, justified exclusion if some sub-path still can't be covered — don't leave
  the whole-repo guard permanently short of the one directory class most likely to reintroduce this bug.

## Acceptance criteria

- [ ] 0 mojibake sequences remain in `Ticketing/**`, verified against the git blob, excluding any location
      explicitly identified as a deliberate literal quotation (documented, not just skipped).
- [ ] 0 files under `Ticketing/**` start with a UTF-8 BOM, verified against the git blob.
- [ ] `git diff --numstat` per file shows a targeted edit, not a whole-file re-encode.
- [ ] `Ticketing/` is either included in `src/lib/mojibakeGuard.ts`'s scan (exclusion removed) or the
      remaining exclusion is justified in a code comment with a concrete reason.
- [ ] The ticket-system's own tests (`epicsQueueLayout.test.ts` and friends) and `/ticketing-*` tooling
      still parse every touched file correctly (frontmatter intact) after the repair.

## Notes

Found by the mojibake guard built for **CPE-1771**, 2026-08-19, while measuring the guard's blast radius
before wiring it into CI. Related: CPE-1752, CPE-1771 (root-cause pattern), CPE-1733 (a worker who caught
the same PowerShell round-trip live via `git diff --numstat`), commit `86888aed` (the BOM incident that
blocked a release).
