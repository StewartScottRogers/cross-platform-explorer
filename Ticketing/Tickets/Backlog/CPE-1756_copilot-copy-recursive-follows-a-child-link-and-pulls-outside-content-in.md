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

- [x] A `Copy` whose `src` is confined but contains a child symlink pointing **outside** the confirmed
      folder does not silently produce a copy of the outside content inside that folder — it refuses,
      copies the link as a link, or does whatever this ticket decides, with the choice recorded.
- [x] The same for a **dangling** child link, and for a child link at a nested level rather than the top.
- [x] An ordinary recursive copy — real files, real subfolders, no links — still works unchanged.
- [x] A child link that resolves back **inside** the confirmed folder is still handled (the discrimination
      leg; without it a guard that refuses everything looks perfect).
- [x] Breaking the guard turns a **distinct** test red, and the assertion names the outside content that
      landed inside — asserted **before** the `Result` is unwrapped, since this fails by succeeding.
- [x] Any link staging uses the repo's loud `require_staged`/`skip_notice!` path so a runner that cannot
      stage a link goes red under CI rather than covering nothing quietly.

## Work log

### The design call: ask containment per entry, but only at a link

Of the three options the ticket floats, the chosen one is **ask [`fsutil::confined_to`] per entry as the
walk descends** — narrowed to the entries where the answer is not already known. The other two were
weighed and rejected:

- **Refuse a child that is a link.** Cheapest and most predictable, but it collapses the discrimination
  leg: a link resolving back *inside* the confirmed folder is ordinary in-folder content, and refusing it
  makes the guard indistinguishable from one that refuses everything — the exact shape the ticket's own AC
  warns about. Rejected.
- **Copy the link as a link.** Needs `symlink_file`, which an unprivileged Windows session cannot create,
  so an ordinary Windows user's copy would start failing where it used to work. It also leaves a link
  pointing *out* of a folder the user may go on to share — the exfiltration shape only slightly redressed.
  Rejected.

**Why "only at a link" is soundness and not a shortcut.** The ticket flags the cost of a `confined_to` per
entry on a deep tree against `PURPOSE.md`'s fast/small/predictable tiebreaker. It does not have to be paid,
because for a non-link entry the answer is already determined:

- `src` is confined — `apply_op` asked before the walk started.
- A `read_dir` name is a single component: never `.`/`..`, never separator-bearing. So for a child that is
  not a link, `canonicalize(child) == canonicalize(parent).join(name)`, which `starts_with` the real root
  exactly when the parent's does.
- The walk recurses only where `symlink_metadata().is_dir()` holds, which is false for every link (a
  junction included — Rust reports one as `is_symlink`). So every directory descended into is a non-link
  child of a confined directory, and the induction carries.

A link is therefore the only entry whose containment is undecided, and it is the only entry asked. **An
ordinary link-free tree pays zero extra `canonicalize` calls**, so the tiebreaker is honoured rather than
traded. The induction is written out on `copy_recursive` so a future reader can check it instead of
trusting it.

### What `copy_recursive` refused BEFORE (CPE-1750's "enumerate the accidental properties" lesson)

It refused nothing *by name*. Every stop was an incidental primitive error, which is why the messages were
opaque and why "some platform happens to stop it" was doing the work:

| entry | before | now |
|---|---|---|
| a **directory** link | not descended into (`symlink_metadata().is_dir()` is false for one) — so no unbounded copy of its target's tree — then `fs::copy` fails on a directory and aborts the whole copy | refused **by name** when it resolves outside; the same incidental abort when it resolves inside |
| a **dangling** link | `fs::copy` fails `NotFound`, aborting the copy | refused by name when the dangle leads outside (`confined_to` fails closed there); the same `NotFound` when it leads back inside |
| a **live file** link | **followed** — the hole this ticket closes | refused when it resolves outside; still followed when it resolves inside |
| an unreadable `src`/entry | `Err` from `symlink_metadata`/`read_dir` | unchanged |

Every row moves in one direction: something previously allowed is now refused, nothing previously refused is
now allowed. The `symlink_metadata` call is kept for its own sake — it is what stops a directory link being
*descended*, a separate property from this guard, and reusing it would have made that property look
incidental.

### Any other recursive walk with this shape? No — checked, not assumed

- `snapshot_capture::scan_dir` (the pre-execute **checkpoint**) is the Copilot's only other recursive walk
  of the user's tree. It uses `DirEntry::metadata()`, which does not traverse, so a link is neither
  `is_dir` nor `is_file` there and is skipped outright — **no inflow of this shape, by construction.** The
  corollary is recorded on `copy_recursive`: a checkpoint contains no links either, so Undo cannot restore
  one.
- `list_plan_entries` is one level deep and reads names only.
- The **move** branch of `transfer_entry` does not need the guard: `fs::rename` moves the directory entry
  itself and never descends, so a link inside a moved subtree stays a link and nothing is dereferenced.
  `canonical_root` is threaded to `transfer_entry` for the copy branch alone, and that is said at the
  signature.

`copy_recursive` was the only walk with the shape.

### What is deliberately still not covered (recorded on `copy_recursive`, in CPE-1750's style)

- A link resolving back **inside** the folder is still **dereferenced** — the copy holds a regular file
  where the original held a link. Deliberate (see the rejected "copy as a link" option), and pinned by the
  discrimination test so it cannot drift silently.
- A refusal mid-walk **leaves the partial copy behind**. Pre-existing behaviour for any mid-walk error;
  everything in it came from inside the folder. Recorded, not changed.
- **Not atomic** with `fs::copy` — the same TOCTOU residual `apply_op` and `confined_to` both state, now
  once per link.

### Mutation results — each guard reds a DISTINCT test

Run against the finished tests, by editing the guard and re-running `cargo test --lib copilot::`:

1. **Guard disabled** (`if false && …`, i.e. the shipped pre-CPE-1756 behaviour restored) — 2 FAILED, 20
   passed; every CPE-1750 test stayed green, so the red is distinct:
   - `cpe_1756_copy_refuses_a_child_link_that_would_pull_outside_content_in` red on the **effect**
     assertion, above the `unwrap()`, naming the outside content and where it landed:
     `the bytes of "…\cpe1756-inflow-outside-27248-34\secret.txt" — a file OUTSIDE the confirmed folder
     "…\cpe1756-inflow-27248-19" — were copied INTO that folder at "…\copy-of-flat\leak.txt".`
   - `cpe_1756_copy_refuses_a_child_link_that_dangles_out_of_the_folder` red on the **reason**, which is
     the point of that leg: without the guard the only thing stopping it was `Got: Access is denied. (os
     error 5)`. A failure-only assertion would have stayed green.
2. **Guard widened to a blanket link ban** (`if meta.file_type().is_symlink()`) — 1 FAILED, 21 passed:
   `cpe_1756_ordinary_copy_and_an_in_root_child_link_still_work` red with *"a child link resolving back
   INSIDE the confirmed folder must not be refused as an inflow — the guard is containment, not a ban on
   links"*. The discrimination leg is load-bearing, and a guard that refuses everything does not look
   perfect.

### Staging

The inflow leg needs a **live file** symlink — the one construction this repo cannot fake (a junction is
directory-only; a hard link answers `is_symlink() == false`) — so it goes through
`fsutil::require_staged("live_file_symlink", true, …)`, the CPE-1717 path: red under CI on a runner that is
supposed to manage one, a loud `skip_notice!` locally. The dangling leg uses `fsutil::make_dir_link`
(junction fallback, no privilege needed), so it runs on every runner even when the inflow leg must skip.

### Verification

`cargo test` + `cargo clippy --all-targets -- -D warnings` green in `crates/server` (default, `index`,
`copilot`) and `src-tauri` (default, `sidecar-platform`). `fsutil.rs` is a one-line doc cross-reference
only; no behaviour change there. `src/docs/21-ai-copilot.md` gains a plain-language paragraph in the
existing "Links and shortcuts that lead out of the folder" section.

## Notes

Related: CPE-1750 (PR #916, which surfaced this), CPE-1710/CPE-1716 (what a primitive does to a link
resolving *inside* the root — a separate question), CPE-1730 (`confined_to`).
