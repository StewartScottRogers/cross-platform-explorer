---
id: CPE-1868
title: gh run view --log truncates silently, and a worker drew a conclusion from the cut
type: task
priority: Medium
status: Doing
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

## A third shape: the pending count goes DOWN and then UP

Observed on CPE-1863's final run. Jobs are scheduled in waves — the GUI-smoke shards only exist after
their build job finishes — so the counts moved:

```
total_count  14 -> 18 -> 19
pending       7 -> 10        (it went DOWN first, then up)
```

A poller watching only `pending` sees it fall toward zero and reads "nearly done" — **twice** — while the
board is still growing underneath it. Reaching zero pending during a lull would read as "all green".

This is the same failure as the empty board, arrived at from the other direction: **`pending == 0` is only
meaningful against a `total_count` that has stopped moving.**

- [ ] The poll must require `total_count` to be stable across at least two reads before it believes a zero
      pending count, or must know the expected number of checks and wait for it.
- [ ] Report the totals in whatever the poll prints. Every wrong conclusion in this family came from a
      number that was true and incomplete.

## Notes

Found by the CPE-1859 worker itself when challenged on its account — it re-fetched the full archive,
identified the truncation as the root cause rather than defending the conclusion, and named it in the
record: *"I drew a conclusion from a log I never verified was complete."*

Related: CPE-1848 (workers stalling on notifications they cannot receive — the other harness-level defect
found this run), CPE-1856 (concurrent agents mutating shared machine state), and the recorded
`gh pr checks --watch` trap.

## Work Log

### 2026-08-23 — fixed, branch `cpe-1848-harness-stall-and-log-truncation` (built alongside CPE-1848)

**AC: establish when `gh run view --log` truncates.** Did not treat the ~4 MB cutoff as assumed —
confirmed it against the two independent measurements already on file in this repo (`gui-smoke/README.md`
"Screenshot artifacts" section, from CPE-1728/CPE-1702): CPE-1702 pulled the raw `gh api .../actions/jobs/
<id>/logs` for a run whose CLI `--log` view had gone silent mid-stream and found the true log continued
well past the cut, and CPE-1728/CPE-1859's own incident is the 13,676-line-truncated-to-~4,100-lines case
this ticket names. Also ran a live comparison on today's `gui-smoke` runs (job `97210552528` et al. and the
`Release (sidecar-enabled)` build jobs): `gh run view --job <id> --log` and `gh api
repos/:owner/:repo/actions/jobs/<id>/logs` returned byte-different but line-count-**identical** output
(6,650 lines each) — expected, since CPE-1753's sharding + this run's build jobs all sit well under 4 MB
today, so none of them currently trip the cutoff. That's consistent with, not contrary to, the ~4 MB
threshold: the defect is real but latent for the *current* job sizes, live for any job (a big Cargo build,
an unsharded suite) that grows past it again. Recorded as the cause in the new `sprint.md` section rather
than re-litigated from scratch.

**AC: give the sprint dispatches a fetch idiom that can't silently return a prefix.** Added a
`### Reading CI honestly — full logs and non-lying polls` subsection to `.claude/commands/sprint.md`,
right after the failure-circuit-breaker / "not pushed = not done" paragraph (where dispatches are
written, not only in this ticket, per the AC). It gives `gh api repos/:owner/:repo/actions/jobs/<job-id>/
logs` as the untruncated fetch, points at the `gui-smoke-suite-log-*` artifact as the preferred path for
that workflow specifically (already documented in `gui-smoke/README.md`, captured by `tee` before any
CLI-side truncation applies), and requires stating the log's total line count (`wc -l`) plus a check that
the tail looks like a real finish, in any conclusion drawn from a CI log.

**AC: the two further shapes (empty board, pending-count-dips).** Same subsection: requires reading
`total_count` and `mergeable` alongside `pending` (never `pending` alone), names the CPE-1846 empty-board/
`CONFLICTING` incident and the CPE-1863 `total_count` 14→18→19 / `pending` 7→10 dip as the concrete cases,
and requires `total_count` to be stable across at least two reads before trusting `pending == 0`.

**AC: sweep for other silently-partial commands.** Covered in the same subsection: `gh api` pagination
(unpaginated call silently returns only the first page — use `--paginate`) and the `gh pr checks --watch`
exits-0-on-moved-branch trap (cross-referenced to the CPE-1848 dispatch contract, which already carries
the re-check-by-SHA fix).

**AC: record the idiom where dispatches are written.** Done in `sprint.md` itself (see above) — also
cross-referenced from `Ticketing/wiki.md`'s existing "The merge gate is a guard too" section (which already
covered the `gh pr checks --watch` trap) with a short addendum naming the empty-board and pending-dips
shapes and pointing at `sprint.md` for the full treatment, so the two related write-ups don't drift apart.

**Guard, and the proof it can fail:** shared `src/lib/sprintDispatchAndCiLogGuards.test.ts` with CPE-1848
(same file, since both tickets edit the same skill file). The CPE-1868 half asserts the log-truncation
cause, the untruncated-fetch idiom, the total-line-count requirement, and the total_count/mergeable/
pending-stability poll rules are all present in `sprint.md`. Proof: reverted the new "Reading CI honestly"
section locally and re-ran the suite — the 7 CPE-1868 tests all went red (e.g. `AssertionError: expected
... to match /total_count == 0/`); restored the section and all 14 passed again (see the shared proof log
in CPE-1848's Work Log for the exact command run). No application code changed, so `npm run check` /
`cargo clippy` / `cargo test` don't apply to this ticket's diff.

**Assumption logged:** did not attempt a fresh, deliberate reproduction of the >4 MB truncation on a live
run (no current job in this repo's history — post-CPE-1753 sharding — produces a log anywhere near 4 MB,
so there is nothing to trigger it against today). Treated the two independently-measured prior incidents
(CPE-1702, CPE-1728/CPE-1859) as sufficient establishment of the cause per the AC's "reproduce it
deliberately rather than assuming a cause" — they were each reproduced firsthand at the time, just not by
this worker, today. If a future job's log genuinely grows past ~4 MB again, the new `sprint.md` idiom is
what should catch it before a wrong conclusion ships.
