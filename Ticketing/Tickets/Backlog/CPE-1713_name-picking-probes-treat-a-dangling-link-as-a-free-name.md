---
id: CPE-1713
title: unique_target and resolve_conflict treat a dangling link as a free name, so a move renames onto it
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-13
closed:
---

## Problem

Found by CPE-1710's enumeration of every `fs::rename`-destructive site, 2026-08-13.

CPE-1710 closed the **refusal**-shaped sites: a slot that is about to be renamed onto now goes through
`fsutil::rename_slot_refusal`, which pairs the occupancy check with the dangling-link check. Two sites in
`src-tauri` are a **different shape** and were deliberately left out of that fix:

- `unique_target` (`src-tauri/src/lib.rs:520`), reached by `do_move_into` — the bulk move and the watch
  executor;
- `resolve_conflict` (`src-tauri/src/lib.rs:2807`) — the transfer engine's Skip/Keepboth/Overwrite policy.

Both *probe* a candidate name and, on "free", **advance past it** rather than refusing. Both probe with
`try_exists()`, which **follows links**. A dangling link at `dest/report.txt` therefore resolves to
nothing, reads as a free name, and `do_move_into`'s `fs::rename` — which does not follow the final
component — replaces the link. The user's link is gone and the operation reports success.

CPE-1696 hardened both against *stat failures* (an unknown now counts as occupied). Neither was hardened
against a link, because a dangling link is not a stat failure — `try_exists` answers `Ok(false)`
correctly, to the question it was asked.

## Why it is a separate ticket

The fix is not `rename_slot_refusal`. Refusing is the wrong verdict at a name-picking loop: the right
behaviour is to treat a **link slot as occupied** and pick the next candidate (`report - Copy.txt`), which
is what the user asked for. That is a change to `classify_copy_target`'s inputs, not a guard inserted in
front of a rename, and it wants its own tests over the `- Copy (n)` sequence.

## Acceptance criteria

- [ ] `unique_target` treats a slot occupied by a link — including a dangling one — as **occupied**, and
      picks the next candidate name instead of returning it as free.
- [ ] `resolve_conflict` does the same, so `Skip` skips, `Keepboth` renames, and `Overwrite` is the only
      arm that touches it.
- [ ] A test proves a dangling link at the destination **survives** a bulk move, asserted on the slot
      (`symlink_metadata(..).is_symlink()`), not on the returned `Result`.
- [ ] Platform-gated the way CPE-1710 did it: `fsutil::make_dangling_link` (symlink, junction fallback on
      Windows) and a loud `writeln!(stderr)` skip if neither can be created. It is `#[cfg(test)]
      pub(crate)` in `cpe-server`, so `src-tauri` needs its own copy or the helper needs promoting.
- [ ] Breaking the change turns a distinct test red, with real output pasted in the PR (Evidence Rules).

## Notes

Filed by the CPE-1710 worker. Related: **CPE-1710** (the refusal-shaped sites and the
`rename_slot_refusal` pairing), **CPE-1705**, **CPE-1696** (which hardened these two functions against
stat failures), **CPE-1461** family (symlink-following).
