# Cold storage — 2026-08-15 17:21 MST

Work is **paused indefinitely**, deliberately, at a clean stopping point. This file is the handoff:
it assumes you remember nothing about the session that wrote it.

**Nothing is in flight.** No agents running, no scheduled wake-ups armed, no sprint lock held, no
ticket in `Doing/`, no unpushed commits. `main` is green and every branch that mattered is merged.

## Where things stand

| | |
|---|---|
| `main` | `86888aed` — clean tree, 0 unpushed commits |
| Latest release | **v0.57.66-sidecar**, published, installers on all three OSes |
| Installed locally | 0.57.66 (Sidecar), verified host + sidecar timestamps match |
| Ticket queue | **30** open in `Backlog/`, **0** in `Doing/`, **3** in `Blocked/` |
| Open PRs | **#738** only (gource visualisation) — pre-existing, not from this sprint |

## What the last sprint did (13–15 Aug, 36 batches)

Fifteen tickets merged. Fourteen of them are the same defect wearing different clothes: **the app
doing something wrong and reporting success.** A truncated download written to disk as the finished
file; an archive writing outside the folder the user chose; the Copilot trashing the folder the user
confirmed; a filename drawing as a different name; a save stripping a file's permissions and its
downloaded-from-the-internet mark.

The number worth remembering: **five of the worst defects were introduced by a fix and caught by an
independent check before merge.** Not one reached the user. That is the argument for the two-check
gate, and it is why the gate should not be economised when work resumes.

Full write-up, built to be read cold:
<https://claude.ai/code/artifact/55ce24fd-baed-49e0-92a3-4bc49bb5e1ed>

## Pick these up first when work resumes

Ranked. The top two are the ones that get worse on their own while nobody is looking.

1. **CPE-1693 — the temp directory is now at 1,217,268 leftover folders.** It grew ~27,000 during a
   single day of test runs and **starved the extraction path once**, failing a test spuriously. The
   ticket records 145,000; it is an order of magnitude past its own writeup. A one-line purge clears
   the symptom; the leak is the ticket.
2. **CPE-1762 — the release pins an ffmpeg build upstream deletes.** Filed today because it blocked
   release 0.57.66 outright, on all three OSes. It is re-pinned and the download now fails loudly
   naming the URL, but the pin will rot again in weeks. The ticket asks the real question: mirror it,
   or build from source as the macOS arm already does.
3. **CPE-1761 — the new spoof guard fails OPEN on a stray brace.** A lone `{` in ordinary prose
   silently ends the scan and reports the file **clean**. Two-line fix, and the worst possible failure
   direction for a guard.
4. **CPE-1758 — an archive entry can still hide bytes in an alternate data stream**, reporting full
   success while the file is absent from the folder. Deliberately deferred, but it is **not**
   fail-safe; a PR body said it was and had to be corrected before merge.
5. **CPE-1760, CPE-1754, CPE-1755, CPE-1759** — small, well-specified residuals from the same work.

Every one of these was written by whoever measured it, with the measurement in the ticket. Trust the
numbers in them more than any summary, including this one.

## Two lessons this sprint paid for, in blood

- **Assert the effect BEFORE unwrapping the `Result`.** Every defect above fails by *succeeding*, so
  an assertion after an `unwrap` is unreachable exactly when it matters.
- **Assert the reason, not only the effect, when two guards overlap.** With both in place the damage
  cannot occur, so a damage-only test lets one guard be deleted with the whole suite green. This bit
  twice.

## Known-untidy, left deliberately

- **72 git worktrees** under `.claude/worktrees/`. Registrations pruned; the directories are left
  alone on purpose — this repo has a scar from an over-eager cleanup that deleted a live worker's
  work. Safe to remove when nothing is running, one at a time, checking each branch is merged first.
- **`.claude/sprint-metrics/visual-evidence/`** — two PNGs from 11 Aug, untracked. Pre-dates this
  sprint and belongs to another session; left untouched rather than committed or deleted.
- Several unmerged branches (`audit-*`, `CPE-1268-green-ci`) predating this sprint. Not investigated.
- Desktop scheduled tasks (`cpe-daily-status`, `cpe-weekly-deps`) still run on their own schedule.
  They are not mine and will keep going.

## Resuming

There is no sprint to resume — the last one ended at its bound, cleanly. Start fresh: read this file,
`git pull`, then either `/ticketing-list` to see the queue or `/sprint-batched N` to run another
supervised batch. Do not treat the list above as a plan; re-read the tickets, because the measurements
in them will have aged.
