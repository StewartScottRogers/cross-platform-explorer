---
id: CPE-1756
title: The Copilot's recursive copy follows a child link and pulls outside content into the confirmed folder
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-15
closed:
---

## Problem

Stated by the CPE-1750 worker as a residual it deliberately did **not** close, rather than leaving it
implied. Filed so the boundary is on record.

CPE-1750 routes every `FileOp` path field through `cpe_server::fsutil::confined_to` before any primitive
runs, so no Copilot **mutation lands outside** the confirmed folder. `copy_recursive` then descends into
`src`'s children **without re-asking containment per entry**.

The destination side stays contained: `read_dir` file names carry no separators, and a pre-existing `dst`
is refused by `rename_slot_refusal`, so `dst` is always freshly created. The source side is the gap — a
child symlink inside a confined `src` is **followed by `fs::copy`**, pulling content from outside the
confirmed folder **in**.

## Why it is Medium, and why the ticket's own claim still holds

This is a read-**inflow**, not a mutation landing outside, so CPE-1750's rewritten doc claim ("every path
field must resolve within `canonical_root`, and a refusal never reaches a primitive") remains true as
written. Nothing of the user's is destroyed and nothing is created outside the folder they confirmed.

It is still a real hole. A user who confirms "tidy up this folder" does not expect the result to contain a
copy of a file from elsewhere on their disk — and on a shared or synced folder, that is an exfiltration
shape: content the user never chose is now sitting inside a directory they may share, back up, or hand to
someone else.

## What to do

Ask containment per entry as `copy_recursive` descends, or refuse a child that is a link, or copy the link
**as a link** rather than following it. Whichever is chosen, record the reasoning where a reader of
`copy_recursive` will find it, in the same "What this does NOT cover" style CPE-1750 established at
`apply_op`.

Consider the same question for any other recursive walk the Copilot performs.

## Acceptance criteria

- [ ] A `Copy` whose `src` is confined but contains a child symlink pointing **outside** the confirmed
      folder does not silently produce a copy of the outside content inside that folder — it refuses,
      copies the link as a link, or does whatever this ticket decides, with the choice recorded.
- [ ] The same for a **dangling** child link, and for a child link at a nested level rather than the top.
- [ ] An ordinary recursive copy — real files, real subfolders, no links — still works unchanged.
- [ ] A child link that resolves back **inside** the confirmed folder is still handled (the discrimination
      leg; without it a guard that refuses everything looks perfect).
- [ ] Breaking the guard turns a **distinct** test red, and the assertion names the outside content that
      landed inside — asserted **before** the `Result` is unwrapped, since this fails by succeeding.
- [ ] Any link staging uses the repo's loud `require_staged`/`skip_notice!` path so a runner that cannot
      stage a link goes red under CI rather than covering nothing quietly.

## Notes

Related: CPE-1750 (PR #916, which surfaced this), CPE-1710/CPE-1716 (what a primitive does to a link
resolving *inside* the root — a separate question), CPE-1730 (`confined_to`).
