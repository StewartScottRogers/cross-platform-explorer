---
id: CPE-1653
title: A refused vault lock leaves inert link debris in the vault-sessions root
type: bug
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-11
closed:
---

## Problem

Found by the independent UAT re-run on PR #838 (CPE-1647), step 4 — a side observation, not a blocker,
and correctly reported rather than fixed in place.

After CPE-1647, a tampered lock is refused: if the session directory has been swapped for a junction or
symlink pointing elsewhere, `lock` refuses, wipes nothing, drops the mapping, and returns `Err`. That is
the right call and the victim's files survive.

But **the planted link is left behind** at the old session path, inside the app's own `vault-sessions`
root. It is inert — never followed again, never shredded — and the user can immediately unlock into a
fresh session path and carry on, so nothing is blocked. It simply accumulates.

`sweep_orphan_sessions` does not clear it either: it filters children on `file_type().is_dir()`, which is
`false` for a junction/symlink, so a planted link is skipped rather than followed. That skip is exactly
what makes the sweep safe (audited twice now), so the fix is *not* to loosen the filter — it is to
recognise a link-shaped child as debris and unlink the link itself without ever traversing it.

## Acceptance criteria

- [ ] A link-shaped child of the vault-sessions root is removed as debris — the LINK is unlinked, never
      followed, and its target is never touched. Prove the target survives by hashing it before and after.
- [ ] The safety property `sweep_orphan_sessions` already has is preserved: no traversal through any
      reparse point, on any code path.
- [ ] Cleanup happens somewhere a user actually benefits — at the startup sweep and/or immediately after a
      refused lock. Decide which, and say why in the work log.
- [ ] A test plants a junction (and a symlink, skipping loudly if unavailable) in the sessions root, runs
      the cleanup, and asserts the link is gone and the victim's bytes are intact.
- [ ] `cargo clippy --all-targets -D warnings` clean in both feature modes; crates/server suite green.

## Notes

- Source: independent UAT re-run on PR #838, 2026-08-11.
- Related: [[CPE-1647]] vault session containment, [[CPE-1645]] locking a vault destroys edits,
  [[CPE-1651]] delete_permanent has no backend gate.
- Deliberately Low: it is untidiness inside an app-owned directory, reachable only after someone has
  already tampered with the session path. No data is at risk.

## Work Log

- 2026-08-11 — Filed by the Foreman from the PR #838 UAT re-run observation.
