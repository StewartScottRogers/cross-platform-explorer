---
id: CPE-1874
title: six shipped releases went out without their updater signatures ever being checked — verify them retroactively
type: task
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-23
closed:
---

## Problem

CPE-1872 fixes the `Verify updater manifest + signatures (CPE-1058)` step going forward. It does
**not** look back. Between 2026-08-04 and 2026-08-23 the step failed on every run, so **at least six
tagged releases shipped an updater manifest that was never checked against the configured pubkey.**

The independent UAT tester on PR #1008 established the important half of this from the real job
logs: in each of those runs, `Uploading latest.json...` and the `.sig` / installer uploads all
**succeeded** before the doomed verify step ran. So the artefacts and their signatures exist and are
downloadable today — they were simply never automatically verified. This is a gap in assurance, not
evidence of a problem.

## Why it is worth doing, and why it is only Medium

The updater is the product's highest-trust surface, and "we never checked" is a different statement
from "we checked and it was fine". Six releases is a small, bounded, one-off set, and the check is
now a single command per tag. Cheap to close properly; unsatisfying to leave open.

It is not High because there is no indication anything is wrong — signing itself was working (the
runtime updater accepts these releases), only the *verification* step was broken.

## What to do

For each tag shipped in the outage window:

1. `gh release download <tag>` the `latest.json`, the installer(s), and their `.sig` assets.
2. Run the now-fixed verifier against them:
   `verify-release-artifacts --conf src-tauri/tauri.conf.json --manifest <downloaded latest.json> --search <download dir>`
3. Record the result per tag in this ticket's work log — pass or fail, with the output.

Note the check should be run with the pubkey **as configured at that tag**, not today's, or a key
rotation would read as tampering. Read each tag's own `src-tauri/tauri.conf.json`.

If it can be done cheaply, prefer a small script over six manual passes, so the same sweep can be
re-run if this ever happens again. Do **not** build a scheduled job — CPE-1872's watchdog already
covers "it went dark"; this ticket covers "it was dark and we need to look back".

## Depends on

CPE-1872 (the fixed verifier). Do not start until that merges.

## Acceptance criteria

- [ ] Every tag in the 2026-08-04 → 2026-08-23 window checked, with per-tag output recorded.
- [ ] Any failure escalated as its own ticket with the evidence.
- [ ] The exact command used is written down where the next person can re-run it.

## Notes

Related findings from the same audit: **CPE-1873** (nothing pins the updater pubkey, so a tag can
rotate the root of trust and the guard blesses it) — that one is the deeper trust question and is
independent of this sweep.

## Work Log

- **2026-08-23 14:05 USMST** — Filed by the Foreman during batched run `batched-2026-08-23-1124`,
  from the independent UAT tester's residual-risk statement on PR #1008. The UAT confirmed from raw
  job logs (fetched via the non-truncating `gh api .../logs` endpoint) that the artefacts and
  signatures were uploaded successfully in every failed run, which is what makes a retroactive check
  possible at all.
