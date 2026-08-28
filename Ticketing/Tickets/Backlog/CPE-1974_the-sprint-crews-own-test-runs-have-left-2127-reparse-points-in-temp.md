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

## Re-measured 2026-08-28 (PR #1089 round 7) — **297,338 directories**, two orders of magnitude past the filing

Noticed incidentally by PR #1089's round-6 Reviewer and re-taken independently by its round-7 worker on
the same machine's real `%TEMP%` (`C:\Users\Stewart Rogers\AppData\Local\Temp`), which reproduced the
Reviewer's total **exactly**. Recorded here rather than as a new ticket: this ticket already owns the
defect, and a duplicate would split the count from the policy question.

- **297,338** top-level `cpe-*` directories (of **327,965** top-level directories in `%TEMP%` overall —
  the crew's fixtures are **91%** of everything in the user's temp directory)
- **287** distinct fixture prefixes
- Creation timestamps span **2026-07-11 20:40** → **2026-08-28 09:59**, so this is seven weeks of
  accumulation, not one bad shift

**Enumerated by shape**, per this ticket's first acceptance criterion — and the shape is the finding.
Four prefixes are **238,412 of the 297,338 (80.2%)**:

| prefix | count |
|---|---:|
| `cpe-binprev-pe-trunc-*` | 90,632 |
| `cpe-dotnetmeta-trunc-*` | 67,640 |
| `cpe-binprev-elf-trunc-*` | 60,530 |
| `cpe-binprev-macho-trunc-*` | 19,610 |
| `cpe-dispatch-base-*` | 5,742 |
| `cpe-copilot-cpe-*` | 3,187 |
| `cpe-webdav-*` | 2,837 |
| `cpe-dotnetmeta-fuzz-*` | 2,682 |
| `cpe-ftp-srv-*` | 2,195 |
| `cpe-snapprune-restore-m-*` | 1,847 |
| *(277 more prefixes)* | *(40,436 total)* |

**Read the shape before hunting fixtures.** The top four are all **truncation sweeps** — a test that
loops over every truncation length of a binary and makes a scratch directory *per iteration*. That is a
different defect from "a test planted a link and died before its cleanup": these are not crashed runs,
they are a per-iteration allocation that was never per-iteration-freed, and one such loop can out-produce
every panicking fixture in the suite combined. The remedy differs accordingly — hoist one directory
outside the loop, or `Drop` it inside it — so the AC "find the leaking fixtures" should be **split**:
per-iteration allocators first (80% of the volume, four call sites), panicking fixtures second.

**No reparse points at the top level, so the original 2,127 are nested.** A top-level attribute scan of
all 327,965 entries finds **0** `FILE_ATTRIBUTE_REPARSE_POINT`, which is consistent with the planting
fixtures putting their links *inside* their scratch directory rather than at `%TEMP%` root. Confirming
the 2,127 therefore needs a **recursive** walk of ~300k directories; that was not run here, and the
figure in this ticket's title should be treated as the recursive count it was, not as something a
top-level scan can reproduce.

**Not the archive family, and not this PR.** `cpe-archive-*` totals **114** — *unchanged* from this
ticket's filing, i.e. PR #1089's whole six-round run added none — and `cpe-archive-*` directories newer
than 6 hours: **0**. The new `#[cfg(unix)]` fixture from that PR cleans up after itself.

Commands used, for whoever picks this up:

```powershell
$di = New-Object System.IO.DirectoryInfo $env:TEMP
# total
@($di.EnumerateDirectories('cpe-*')).Count
# histogram by prefix (strip the trailing numeric suffix)
$h=@{}; foreach($d in $di.EnumerateDirectories('cpe-*')){
  $p=[regex]::Replace($d.Name,'[0-9].*$','').TrimEnd('-'); $h[$p]=1+$h[$p] }
$h.GetEnumerator() | Sort-Object Value -Descending | Select-Object -First 25
# reparse points, top level only
@($di.EnumerateDirectories('*') | Where-Object { ($_.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 }).Count
```

Each of those completes in **under 3 seconds** over 328k entries, which is worth saying: *counting was
never the expensive part*. Nobody was counting because nobody looked, not because looking was costly —
and that is an argument for the sweep being cheap enough to run at the **start** of every sprint.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1084's round-2 worker, which counted the debris while
verifying its own cleanup, established that none of it was its own, and correctly declined to remove
anyone else's.

Related: **CPE-1952** (the catalog staging leak, and the round-2 red-proof that leaked its own junction),
**CPE-1964** (the `cpe-swarm` leak, 55 dirs), **CPE-1929** (why these fixtures plant at real paths),
**CPE-1972** (an absence of information must never license a delete), and the janitor rule about never
`rm -rf`-ing a possibly-live worktree.
