---
id: CPE-1955
title: `gui-smoke` shard 2 dies after one spec with "SUITE DID NOT COMPLETE" — four times today, on four unrelated PRs
type: bug
priority: Medium
status: Open
tags: ready
estimate: M
created: 2026-08-27
---

## Summary

`GUI smoke (ubuntu-latest) shard 2` has failed **four times on 2026-08-27**, across four PRs with
nothing in common, always the same way:

    [gui-smoke ratchet] SUITE DID NOT COMPLETE: expected 14 spec file(s)
    (globbed from specs/*.smoke.ts) but only 1 reported any result. A timeout,
    crash, or hang killed the job before it finished — this is treated as RED.
    [gui-smoke ratchet] 1/14 spec file(s) reported, 1 case(s) — 1 passed, 0 failed
    [gui-smoke ratchet] FAILED — 0 new failing case(s) … incomplete=true

**0 new failing cases every time.** One spec reports, thirteen never run.

Observed on **#1039** (twice), **#1056**, and **#1063** — release-plumbing, dialog copy, and catalog
trust-engine changes respectively. **None of the four diffs touches a GUI spec**, and shard 2's
fourteen specs are all main-explorer surfaces (archive-browse, trash, thumbnails, drive-menu,
instant-search, native-tags, …) unrelated to any of them.

Each occurrence was re-run and passed. That is why it has been treated as flake — but four times in
one day, always the same shard, always after exactly one spec, is a defect with a re-run as its
workaround.

## Why this is worth fixing rather than re-running

The ratchet is behaving **correctly** — CPE-1753's `incomplete=true` rule exists precisely so a
suite that dies is red rather than "everything else happened to pass". That is the right design and
must not be softened.

The cost is elsewhere: each occurrence blocks a merge for a full CI cycle, and it trains the crew to
reach for `gh run rerun` on a red GUI-smoke shard — which is exactly the habit that lets a **real**
regression through. A guard people learn to re-run is a guard that has stopped working.

## Acceptance criteria

- [ ] **Find out what dies.** The ratchet reports only the aftermath. Get the job's own log for a
      failing run and identify whether it is a timeout, a crash, a hang in `tauri-driver`, an OOM, or
      a spec that never returns. **Do not fix anything until you can name the cause** — this repo has
      spent the week finding that plausible explanations are not measurements.
- [ ] Establish whether it is **shard 2 specifically** or the second shard *whatever it contains*.
      Those have completely different fixes. The shard plan is deterministic, so this is answerable by
      changing the split and re-running.
- [ ] Check whether the **first spec that reports** is always the same one, and whether the dying spec
      is always the second in the shard's order. If so, it is a specific spec, not the shard.
- [ ] **Make the failure legible.** Whatever the cause, the job should say which spec it died in and
      why — "only 1 of 14 reported" is a symptom, not a diagnosis. That is most of the value here even
      if the underlying hang proves hard to fix.
- [ ] **Do not weaken the ratchet.** `incomplete=true ⇒ RED` stays. If the fix is a retry, it must be
      a bounded retry **inside** the job that still reds when it exhausts, never an exemption.
- [ ] Check the other shards' failure rates over the same period. If shard 2 is an outlier, that is
      evidence; if all four shards do this occasionally, the diagnosis changes completely.

## Notes

Filed 2026-08-27 by the sprint Foreman after the fourth occurrence, having re-run the first three.
The standing rule that produced this ticket: **re-run an infra-looking failure once, then investigate
rather than re-running a third time.**

Related: **CPE-1753** (the verdict-across-all-shards job and the `incomplete` rule — working as
designed), **CPE-1171** (the gui-smoke harness), **CPE-1679** (a prior gui-smoke timeout where
`this.timeout()` inside an `it()` body was found not to be honoured — documented at
`wdio.conf.ts:1358-1372`, and worth re-reading here).
