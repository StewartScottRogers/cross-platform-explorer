---
id: CPE-1868
title: gh run view --log truncates silently, and a worker drew a conclusion from the cut
type: task
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-22
closed:
---

## Problem

`gh run view --job <id> --log` returned **~4,100 lines of a 13,676-line job log** and exited cleanly.
No warning, no error, no truncation marker — the output simply stopped.

The worker on CPE-1859 read the stopping point as the end of the run and concluded that the wdio process
had ended early with no assertion ever firing. An independent reviewer fetched the full archive and found
the opposite: seven specs passed, **one genuinely failed**, two more passed after it, and the truncated
reporter JSON belonged to the *failing* worker rather than to one that never ran.

The conclusion — an environment flake, not a regression — survived. The *evidence* did not.

## Why it matters beyond one wrong paragraph

The whole review discipline in this run rests on **reading the actual log rather than the summary**. A
fetch that silently returns a prefix defeats that at the source: every downstream inference is honest, and
wrong. It is the same shape as the shell heredoc that ate a backslash and turned a sabotage into a syntax
error — a tool that fails by returning something plausible instead of failing.

It is also specifically dangerous for flake attribution. "No assertion fired" is exactly the conclusion a
prefix produces, and it is exactly the conclusion that excuses a red.

## Acceptance criteria

- [ ] Establish when `gh run view --log` truncates — size, line count, rate limit, or the run being
      archived. Reproduce it deliberately rather than assuming a cause.
- [ ] Give the sprint dispatches a fetch idiom that cannot silently return a prefix. The one that worked
      here is downloading the full log **archive** rather than streaming a job; whatever is chosen, it must
      fail loudly on a partial fetch.
- [ ] Any conclusion drawn from a CI log must be able to state the log's **total line count** and that the
      fetch reached the end. Cheap, and it is precisely the check that was missing.
- [ ] Sweep the sprint's own tooling for other commands whose output can be silently partial — `gh api`
      pagination is the obvious neighbour, and `gh pr checks` has a recorded trap of its own (it exits 0
      when the branch moves under it).
- [ ] Record the idiom where dispatches are written, not only in a ticket. The knowledge that did not
      reach the prompt is the knowledge that failed here.

## A second shape, same family: an empty board reads as a green board

CPE-1846 hit this while the first ticket was still open. Its PR went **CONFLICTING** after a sibling
merged underneath it, and GitHub cannot build a merge commit for a conflicting PR — so **zero check runs
were scheduled**. `total_count` stayed **0 for eight minutes**.

A poller that counts only *pending* jobs sees zero pending and concludes the board is green. **An empty
board and a passing board are identical to it.** Every merge in this run polls exactly that way.

- [ ] Every CI poll must read `total_count` (or the equivalent) and refuse to conclude anything from an
      empty board. Zero checks is a state to report, never a state to pass.
- [ ] Check the `mergeable` field alongside it — `CONFLICTING` is the usual cause, and a poller that reads
      it would have named the real problem in seconds rather than after eight minutes of silence.

## Notes

Found by the CPE-1859 worker itself when challenged on its account — it re-fetched the full archive,
identified the truncation as the root cause rather than defending the conclusion, and named it in the
record: *"I drew a conclusion from a log I never verified was complete."*

Related: CPE-1848 (workers stalling on notifications they cannot receive — the other harness-level defect
found this run), CPE-1856 (concurrent agents mutating shared machine state), and the recorded
`gh pr checks --watch` trap.
