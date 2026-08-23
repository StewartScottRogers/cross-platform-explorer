# Checkpoint — batched run `batched-2026-08-20-1620`

**Status: COMPLETE. 40 of 40 batches. Nothing in flight.**
Written 2026-08-23 before a planned reboot.

## State at shutdown

- `main` clean, fully pushed, HEAD `ac1fc7b0` (`release: bump to 0.57.69`)
- Tags pushed: `v0.57.69`, `v0.57.69-sidecar`
- **0.57.69 sidecar build installed and verified running** at
  `%LOCALAPPDATA%\Cross-Platform Explorer (Sidecar)` — all processes stopped cleanly for the reboot
- No open PRs from this run (#738 is pre-existing and unrelated)
- `BATCH-COUNTER` and `SPRINT-LOCK` deleted — the run is torn down, not paused

## What shipped

40 tickets, each through an independent reviewer; the security-sensitive ones through a
dedicated attacker as well. 2,204 agent outcomes in `ledger.jsonl`.

The recurring finding, stated plainly: **the code was almost always right and the claims about it
were not.** Ten hollow guards — tests that passed while proving nothing. A dozen comments stating
things that were measurably false. Nearly all caught by *running* a sabotage rather than reading.

## Open items the next session should know

**1. The plain release build has been broken for 27 days.**
Every `Release` run since 2026-08-04 fails at *"Verify updater manifest + signatures (CPE-1058)"*,
on all three platforms, which is why the `catalog` job is permanently skipped. It does **not**
affect the sidecar build, which is what gets installed. **This was noticed during the run, recorded
as context inside other tickets, and never filed on its own.** It should be.

**2. Nothing in these 40 batches has been looked at by a human.**
The 0.57.69 build was installed and launched, but no attended visual check was made. The two
genuinely visible changes are both in the status bar:
- the free-space figure now anchors right in **every** folder (it sat 534px out of place in the
  majority of them since 2026-07-14)
- the git chip and disk figure now clear correctly on entering an archive / smart folder / saved
  search, and neither repaints from a slow previous folder

**3. One new UI surface has never been seen at all.**
CPE-1845's revert-outcome panel only appears when a revert holds deletions back, which needs a
checkpoint containing a filename this filesystem cannot write. Worth staging deliberately.

**4. ~180 agent worktrees, ~700 GB, under `.claude/worktrees/`.**
Disk is not tight (2.3 TB free) and they were left deliberately: this repo squash-merges, so a
merged branch does not *look* merged by commit ancestry, and the obvious "is this work landed?"
test gives the wrong answer. Removing a live or resumable worktree has caused loss here before.
A proper cleanup needs the PR-to-directory mapping and is a job in itself.
A full dirty-worktree scan was attempted at shutdown and **timed out partway** — no dirty tree was
found before it did, but it did not finish. `main` itself is clean and pushed.

## Bench

52 tickets open. The run filed 31 of them, almost all found by checkers attacking work that had
already passed its author's own tests. Highest-value next, all created by this run's own findings:

- **CPE-1871** — two prune-loop design decisions argued at length and pinned by nothing; each
  rejected alternative leaves the suite green. One fixture with an undeletable blob closes both.
- **CPE-1869** — the held-back list names 8 of 200 paths and says "delete these files yourself".
- **CPE-1862** — retention prunes manifests but nothing reconciles `checkpoints.json`.
- **CPE-1868** — three distinct ways to misread a CI board, all found on 2026-08-23: a silently
  truncated log fetch, a conflicting PR scheduling zero checks, and a pending count that dips
  before it rises.
- **CPE-1848** / **CPE-1856** — the two harness-level defects: workers stalling on notifications
  they cannot receive, and concurrent agents mutating shared machine state.
