---
id: CPE-1717
title: join_files follows a symlink at the output path — writing through it on success, deleting it on failure
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-13
closed:
---

Related: **CPE-1710** (the `fs::rename`-destructive slots and the `rename_slot_refusal` pairing) and
**CPE-1715** (the name-picking probes). Same root hazard — a probe that *follows* a link standing in front
of an operation that does not — at a third shape: a `File::create` output path rather than a rename
destination or an auto-renamed candidate.

## Problem

Found by the PR #895 (CPE-1710) UAT, 2026-08-13, while re-checking the sites that PR classified as
out-of-class.

`crates/server/src/split_join.rs` (`join_files`, the `clobber_refusal` at :327) guards its `out_path` with
`clobber_refusal` alone. That check **follows links**, so a link at `out_path` whose target does not exist
reads as a free name, and the operation proceeds. Two distinct wrong outcomes, both measured by the UAT:

- **Failure path — the link is deleted.** Any error past the guard (missing part, short part, checksum
  mismatch, I/O error) runs `let _ = std::fs::remove_file(out_path)` to clear the partial output. That
  removes the user's **link**, not a partial file this operation created:

  ```
  [UAT] join_files (failing) -> Err("part 4 missing: ...")
  [UAT] AFTER: the user's link still exists = false
  ```

- **Success path — the bytes land somewhere the user never named.** `join_into` opens `out_path` with
  `File::create`, which **follows** the final component, so the reconstructed file is written **through**
  the link to its target. `Ok(())` is returned and the user is told the join succeeded, while the bytes
  are at a path they did not choose (and whatever was at the link's target is truncated).

CPE-1710 guards exactly this follow-through-link case at `copilot::transfer_entry`'s copy branch and its
comment calls it "a different surprise, equally unasked-for" — then classified this site as safe three
modules over, on the strength of "it precedes `File::create`, not `fs::rename`". The `File::create` is
what makes it *worse*, not safer.

## Scope

`join_files` in `crates/server/src/split_join.rs`. While there, check `split_file`'s two sibling
`clobber_refusal` calls (the manifest path and each numbered part path) — they have the same
`File::create` follow-through-link exposure, and a split writes a whole numbered *series*, so one link in
the output directory is one file written somewhere unnamed per part.

## Acceptance criteria

- [ ] A link at `out_path` — dangling or live — is refused before anything is created or removed, with a
      message that names the link (`fsutil::symlink_slot_refusal`'s wording, or `rename_slot_refusal` if
      the refusal shape fits).
- [ ] The same for `split_file`'s manifest and part paths, or a written-down reason why they differ.
- [ ] A test proves the **failure** path no longer deletes the user's link, asserted on the slot
      (`symlink_metadata(..).is_symlink()`), not on the returned `Result`.
- [ ] A test proves the **success** path no longer writes through the link to a path the user never named.
- [ ] Platform-gated with `cpe_server::fsutil::make_dangling_link` (symlink, junction fallback on Windows)
      and a loud `writeln!(stderr)` skip if a link cannot be created; the Linux and macOS legs assert for
      real.
- [ ] Each guard broken on its own turns a distinct test red, real output pasted in the PR (Evidence
      Rules, `Ticketing/wiki.md`).

## Notes

Filed by the CPE-1710 worker from the PR #895 UAT's findings, as a separate ticket rather than folded into
CPE-1715: that one is about **name-picking** probes (`unique_target`, `resolve_conflict`), whose fix is
"treat a link slot as occupied and pick the next name". This one is a refusal-shaped site whose fix is a
refusal, and it also has a delete-on-failure path the name-picking sites do not.
