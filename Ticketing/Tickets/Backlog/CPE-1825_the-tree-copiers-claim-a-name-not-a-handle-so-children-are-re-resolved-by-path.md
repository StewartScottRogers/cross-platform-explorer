---
id: CPE-1825
title: the tree copiers claim a name, not a handle, so every child is re-resolved by path
type: bug
priority: Medium
status: Backlog
tags: big-design
estimate: L
created: 2026-08-20
closed:
---

## Problem

CPE-1765 closed the name-pick-to-write gap for the single-file and rename paths: `create_new` /
`create_dir` make the claim atomic, and `rename` never follows its final component, so no byte can
land outside the chosen folder there.

The two **tree** copiers are not closed by the same argument. `copy_tree_into_claimed_slot`
(`crates/server/src/fsutil.rs:676-690`) and `copy_tree_streamed` (`src-tauri/src/lib.rs:3495-3509`)
claim a **name**, not a handle, and then write every child by re-resolving
`dst.join(entry.file_name())`. An actor that `rmdir`s the empty directory we just created — an easier
primitive than deleting a file, and exactly the cleanup-tool / sync-client actor the CPE-1765 ticket
cites — and plants a directory link at that name redirects every subsequent byte:

```
[A1] child write -> Ok(12)
[A1] outside/child.txt = Ok("USER CONTENT")
```

The intermediate-component variant is worse, because the redirect survives the whole subtree:

```
[F1] copier returned: Ok(())
[F1] evil/Copy of src/sub/b.txt = Ok("USER CONTENT B")
```

Bytes outside the chosen folder, with the operation reporting success.

## Why it is deferred rather than fixed in CPE-1765

**Honest severity.** The Security Auditor could **not** win the race in-process on local NVMe — 0/100
across two harnesses (40 rounds with an `rmdir` spinner, 60 rounds with a 4000-entry source to slow
`read_dir`). The primitive is deterministic once the swap lands, but the window is small. It is
per-interior-directory, though, so a deep tree on a slow or network source (the QNAP target) offers
thousands of attempts rather than one.

**The real fix needs an API `std` does not have.** Closing it properly means resolving each child
*relative to an open directory handle* — `openat`-style — so a swapped parent cannot redirect the
write. `std` exposes no `*at` call on any platform, and on Windows `File::open` cannot even open a
directory (measured: os error 5). So this is a genuine design task, not a patch: either a hand-rolled
platform layer (`openat`/`NtCreateFile` with a relative root) or a vetted dependency, which collides
with the lean-core guardrail and needs a decision.

## Acceptance criteria

- [ ] Decide and record the approach: hand-rolled per-platform relative-open layer, a dependency (name
      it and justify against lean-core), or an accepted-and-documented residual with a narrower bound.
      This decision is the first deliverable — do not start coding before it is written down.
- [ ] If closing it: every child in both tree copiers is created relative to a directory handle held
      across the whole subtree, so replacing a parent by name cannot redirect a write.
- [ ] A test stages the swap deterministically at the seam rather than depending on winning a live
      race — CPE-1813 established the injectable-seam pattern for exactly this problem; reuse it.
- [ ] The abort-on-first-error `?` in `copy_tree_into_claimed_slot` stays. The Auditor measured it as
      **load-bearing**: in its race harness it fired in the gap between the attacker's `rename` and
      their `symlink`, and zero files escaped. Do not relax it to skip-and-continue without re-running
      that scenario.
- [ ] CPE-1765's per-site residual wording is updated to match whatever lands here.

## Notes

Found by the independent Security Auditor (F2) during the PR #968 review, and confirmed by the code
Reviewer from the other direction. Split out of CPE-1765 deliberately: that ticket's own fix is
airtight for the sites it covers, and bundling an unbounded design task into it would have blocked a
shipped safety improvement.
