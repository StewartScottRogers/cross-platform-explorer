---
id: CPE-1831
title: run_transfer still dereferences a shortcut moved across volumes, unlike do_move_into
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-20
closed:
---

## Problem

CPE-1765 fixed `do_move_into`: moving a shortcut across volumes now **refuses** rather than following
the link, copying out everything it points at, and deleting the original.

`run_transfer` — the **progress-dialog** transfer path — was not fixed and still does the old thing.
Measured C: to Z:, two real NTFS volumes, through production:

```
[REV3-d] transferred=1 failed=0 errors=[]
[REV3-d] landed_is_link=false  landed_bytes=Some("PRIVATE KEY")
[REV3-d] src_link_left=false   secret_intact=true
```

The link is dereferenced, the linked-to secret's bytes are written to the other volume as a real file,
the user's shortcut is deleted, and the report says `transferred=1, failed=0`.

`run_transfer` never calls `cross_volume_move_into_picked_slot`; its `RenameFailed` arm falls into
`copy_tree_streamed` -> `stream_copy_file`, whose `File::open` follows the link.

## Why it matters

The two paths now disagree about what "move a shortcut" means, and which one the user gets depends on
which UI affordance they used — not on anything they can see. The dangerous case is the one that still
misbehaves: a shortcut pointing at an SSH directory or a password vault, dragged to a USB stick through
the progress dialog, still puts the contents there.

**Pre-existing, not caused by CPE-1765** — `fs::rename` failed `EXDEV` and this same thing happened
before. What is new is that CPE-1765's user documentation promised the refusal without qualification;
that sentence was corrected to name only the paths that keep it.

## Acceptance criteria

- [ ] `run_transfer`'s cross-volume move path agrees with `do_move_into`: a symlink or junction source is
      refused, not dereferenced, with the same message.
- [ ] Both reparse shapes are covered. Note Rust reports an NTFS junction and a directory symlink both as
      `is_symlink()`, so one predicate serves both — verified during the CPE-1765 audit.
- [ ] The refusal leaves no partial state: nothing created, nothing staked that survives, source intact,
      link target intact. CPE-1765's `cross_volume_move_into_picked_slot` was measured at 0 litter across
      200 consecutive EXDEV attempts — match that, and test it the same way.
- [ ] Ordinary files and folders still move across volumes through `run_transfer`. Pin BOTH directions:
      CPE-1765's own extraction shipped with only the refusal pinned, and mutating the predicate to
      refuse everything left the whole suite green.
- [ ] `src/docs/03-explorer.md`'s shortcut-moving sentence is restored to an unqualified statement once
      both paths agree.

## Notes

Found by the independent Reviewer in CPE-1765's final round. Fixing it there was deliberately declined:
it would have been a new branch at the end of a chain whose lesson was that every round's new hole
arrived in the layer added to close the last one.
