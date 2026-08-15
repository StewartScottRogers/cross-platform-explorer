---
id: CPE-1750
title: The Copilot's parent_confined walks past a dangling link, and it guards real production mutations
type: bug
priority: High
status: Backlog
tags: ready
estimate: M
created: 2026-08-14
closed:
---

## Problem

Found and reproduced by the PR #909 (CPE-1730) reviewer, 2026-08-14, while checking whether the repo now had
two containment primitives. It has **three** — and the weakest of them is the only one guarding **real
production mutations**.

`crates/server/src/copilot.rs:178` `parent_confined` guards the AI Copilot's actual `fs::rename`, `fs::copy`,
`create_dir_all` and `trash::delete`. Its doc at `copilot.rs:198-200` claims:

> no mutation ever lands outside the confirmed folder.

That claim is false. Replicated verbatim and run against the new `cpe_server::fsutil::confined_to` on
identical inputs:

```
dangling link root/dangling -> <outside>/not-created-yet
  parent_confined(root, root/dangling/x.txt) = true      confined_to = false
  parent_confined(root, root/dangling)       = true      confined_to = false
live link root/live -> <outside>
  parent_confined(root, root/live)           = true      confined_to = false
```

Two defects, either sufficient on its own:

1. Its `Err(_) => cur = dir.parent()` walks **past a dangling link** — the exact trap `confined_to`'s doc
   names and fails closed on.
2. It never inspects the **final component at all**, so a link at the leaf is invisible to it.

Consequence: `FileOp::Copy { dst: root/dangling }` reaches `fs::copy`, which **follows the link** and creates
its target outside the confirmed folder — and outside the undo checkpoint, so the operation is also not
reversible by the app's own undo.

## Why this is High

This is not a test rig. CPE-1730 confined the FTP/SFTP/WebDAV **test rigs**; this is the shipped Copilot
acting on a real user's filesystem, with a doc comment asserting the property it does not have. The
confirmed-folder boundary is the Copilot's entire safety story.

## The fix

`cpe_server::fsutil::confined_to` (added by CPE-1730, `fsutil.rs:1043`) already answers this question
correctly and was adversarially probed across 26 cases (nested links, relative links, junctions, drive
prefixes, drive-relative paths, extended-length and UNC paths, trailing dots, embedded NUL, 200- and
5000-deep missing tails, mutually-referential dangling links, and a sibling whose name is a string prefix of
the root). Route `parent_confined` through it rather than re-deriving the walk, and cross-reference it from
`confined_to` so the next reader finds one answer instead of three.

**Also**: `copilot.rs:760` has a private `make_dir_link` that duplicates the new `fsutil::make_dir_link` —
same crate, two files apart — which is precisely the duplication `fsutil::make_dir_link`'s own doc says it
exists to prevent. Fold it.

## Acceptance criteria

- [ ] A Copilot `Copy`/`Move`/`Delete`/`mkdir` whose destination is a **dangling** link inside the confirmed
      folder is refused, and nothing is created outside that folder.
- [ ] The same for a **live** link at the leaf, and for a link at an intermediate component.
- [ ] Breaking the guard turns a **distinct** test red, and the assertion names what landed outside the
      confirmed folder — asserted **before** the `Result` is unwrapped, since this defect fails by
      succeeding.
- [ ] `copilot.rs:198-200`'s claim is either true or rewritten to what actually holds.
- [ ] `copilot.rs:760`'s private `make_dir_link` is removed in favour of `fsutil::make_dir_link`.
- [ ] TOCTOU's residual is stated where a reader of the Copilot path will hit it, the way `confined_to`
      states its own.

## Notes

Related: CPE-1730 (PR #909, which added `confined_to` and surfaced this), CPE-1709, CPE-1733.
`fsutil::contained_under` is a *third* primitive that deliberately fails **open** on a not-yet-existing path
— correct for its two removal-side callers, wrong for anything create-side. Do not reuse it here.
