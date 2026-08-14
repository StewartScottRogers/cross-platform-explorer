---
id: CPE-1725
title: The two file-save paths answer the dangling-link question oppositely
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-13
closed:
---

## Problem

Found by the PR #899 (CPE-1716) UAT, 2026-08-13, while checking that ticket's sibling sweep.

The app now has two paths that write a whole file back over a path the user opened, and after CPE-1716
they **disagree about what a broken symlink means**:

| path | dangling link at the target | result |
|---|---|---|
| `metadata_write` (Metadata Studio save) | **refused**, naming the link | link survives, target not created |
| `write_file_text` (content editor save) | **`Ok(8)`** | link survives, **target created** |

`write_file_text` is `fs::write(&path, ..)`, which is `O_CREAT\|O_TRUNC` and *follows* the final
component — so through a link pointing at something that is no longer there, it silently **creates** the
missing file. Nothing is destroyed and the link is not harmed; the user simply gets a new file conjured at
the far end of a broken link and no indication that is what happened.

Measured by the PR #899 UAT (`Ok(8)`, target created). Corroborated independently on Windows by that PR's
own Probe E: swapping `create_new` for `create(true).truncate(true)` in `replace_file_contents` made the
staging open follow a **dangling junction** and create
`staging-dangling-target-that-does-not-exist` — the same `O_CREAT` follow-through, on the other platform
and through the other reparse type.

## Why file it rather than fix it in #899

Two reasons, both about blast radius:

1. It is **not the CPE-1716 bug**. Nothing is lost, the link is not destroyed, and the edit does land
   somewhere the link points. CPE-1716 was a data-loss bug with a false success report; folding a
   behaviour change to a second command into it would have widened a high-priority fix.
2. The other three `fs::write` siblings (`macro_convert_in_place`, `batch_execute`'s in-place overwrite,
   `forge_resolve_file`) have the **same** shape, so "make them consistent" is a four-command decision,
   not a one-line one. It deserves its own decision and its own tests.

## The decision to make

Which answer is right for a **whole-file save over a path the user opened**?

- **Refuse** (what `metadata_write` does): consistent with the Metadata Studio, and the user finds out
  their link is broken. Surprising for anyone who expects `fs::write` semantics.
- **Create the target** (what `write_file_text` does today): matches every editor that uses plain
  `fs::write`, and "save" arguably *should* create a file that isn't there.
- Whichever is chosen, **both paths must give the same answer**, and the answer must be written down
  where the next sweep will find it (at the site, per CPE-1710's convention).

Note that `write_file_text` also has the inverse trade-off from `metadata_write`: `fs::write` is not
atomic, so an interrupted save truncates the user's file. `cpe_server::fsutil::replace_file_contents`
(added by CPE-1716) already provides atomic-replace + link-resolution + the dangling refusal in one call,
so "route the content editor through it" is one candidate fix that settles both questions at once — but
it *is* a behaviour change to the dangling case, which is the thing to decide first.

## Acceptance criteria

- [ ] Decide, and record at the site, what a dangling link at a save destination means. One answer, both
      paths.
- [ ] `metadata_write` and `write_file_text` agree, asserted by a test that drives **both** and asserts on
      the far end of the link (created or not) — never on the `Result` alone.
- [ ] State explicitly what the other three `fs::write` siblings do, and either bring them into line or
      record why they are different.
- [ ] If `write_file_text` moves to `replace_file_contents`, the atomicity change is called out in the
      user docs (`src/docs/` content-editor page) alongside the dangling-link behaviour.
- [ ] Platform-gate the tests the way CPE-1710/CPE-1716 did — `fsutil::make_dangling_link`, whose junction
      fallback needs no privilege, so the leg asserts for real on every runner.

## Notes

Filed from PR #899's UAT round 2, 2026-08-13. Related: **CPE-1716** (the metadata save; created
`replace_file_contents`), **CPE-1710** (the rename-guard sweep), **CPE-1726** (the protocol crates'
unguarded renames — a different primitive, filed separately).

Through a **live** symlink `write_file_text` is clean: it writes through to the real file and the link
survives. This ticket is only about the **dangling** case.
