---
id: CPE-1974
title: the sprint crew's own containment tests have left **2,127 reparse points** and 114 `cpe-archive-*` directories in the user's `%TEMP%`
type: task
priority: Medium
status: Open
tags: ready
estimate: S
created: 2026-08-27
---

## Summary

Counted by PR #1084's round-2 worker on this machine's real `%TEMP%`:

- **2,127 reparse points** (junctions / symlinks)
- **114 `cpe-archive-*` directories**

It confirmed **none are its own** — its tests clean up, and a scoped search for its fixtures returned 0
— and it **deliberately did not mass-delete**, per the standing rule about never `rm -rf`-ing anything
that might be live. Correct call; that rule exists because a cleanup glob once clobbered a live worker
and dirtied `main`.

## Where this came from

The 2026-08-27 sprint ran a long sequence of containment tickets — **CPE-1913, CPE-1937, CPE-1929,
CPE-1952, CPE-1958, CPE-1925, CPE-1938** — whose entire method is planting junctions and symlinks at
real paths and measuring where bytes land. That method is right and produced most of the shift's real
findings. **The debris is the cost of it, and nobody was counting.**

It compounds with two product leaks the same shift measured and fixed or filed:
**9** `cpe-catalog-stage-<pid>` directories (fixed by CPE-1952) and **55** `cpe-swarm-<millis>`
directories (**CPE-1964**). Those are the *app* leaking; this is the *test suite* leaking, and it is
two orders of magnitude larger.

## Why it is worth a ticket rather than a one-off `rm`

1. **A reparse point is not inert.** A stray junction pointing at a real directory is a hazard for the
   next agent — and several of these tests *plant links at predictable paths* precisely because a
   stand-in inside a `tempfile::tempdir()` would be unfalsifiable (CPE-1929). So the debris sits at
   exactly the paths the next run will reach for.
2. **It is on a real user's machine**, growing, and nobody chose it.
3. **The fix is a policy, not a delete.** A one-time cleanup leaves the next sprint in the same place.

## Acceptance criteria

- [ ] **Count first, and enumerate by shape** — how many, of which prefixes, from which tickets, and how
      many are reparse points versus plain directories. A number without a breakdown cannot tell a
      leaking fixture from a crashed run.
- [ ] **Find the leaking fixtures.** A test that plants a link and dies before its cleanup is a fixture
      defect, and it is the same class as CPE-1952's round-2 finding: **its own red-proof leaked a live
      junction because the panic fired before the `Scene` existed and therefore before its `Drop` was
      armed.** Expect more of exactly that — a guard armed *after* the thing it guards.
- [ ] **Prefer RAII over sweeps.** A `Drop` guard that survives a panic is the shape; a startup sweep is
      the fallback. If a sweep is needed, it must **refuse anything not plainly ours** — CPE-1972's rule
      applies: *an absence of information must never license a delete.*
- [ ] **Do not mass-delete blind.** Junctions must be removed as links, never followed —
      `remove_dir_all` on a planted link removes the **link**, not the target (measured in CPE-1952), but
      that is a property to verify per-platform before relying on it, not to assume.
- [ ] **Check for anything that is NOT ours** before deleting anything, and leave it. `%TEMP%` is shared.
- [ ] Consider whether the containment tests should plant under a single sprint-owned root that can be
      swept as one unit — without losing the real-path realism that makes them falsifiable in the first
      place. That tension is the actual design question here.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1084's round-2 worker, which counted the debris while
verifying its own cleanup, established that none of it was its own, and correctly declined to remove
anyone else's.

Related: **CPE-1952** (the catalog staging leak, and the round-2 red-proof that leaked its own junction),
**CPE-1964** (the `cpe-swarm` leak, 55 dirs), **CPE-1929** (why these fixtures plant at real paths),
**CPE-1972** (an absence of information must never license a delete), and the janitor rule about never
`rm -rf`-ing a possibly-live worktree.
