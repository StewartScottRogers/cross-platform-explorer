---
id: CPE-1710
title: copilot's rename and transfer sites destroy a dangling symlink at the destination
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-13
closed:
---

## Problem

Found by the PR #893 (CPE-1705) reviewer, 2026-08-13, while enumerating that ticket's sites rather than
spot-checking them.

`copilot::apply_op` (the `Rename` arm) and `copilot::transfer_entry` are both **`fs::rename`-destructive**
at the destination. Both received the `clobber_refusal` guard in CPE-1705 — but **neither got
`symlink_slot_refusal`**, which `rename_entry_impl` and `move_exact_impl` both have.

The consequence: **a dangling symlink sitting at the destination is silently destroyed.** `clobber_refusal`
answers "is something already here?" using a stat that follows the link; for a link whose target does not
exist, that answers *no*, the slot reads as free, and the rename replaces the link itself.

The PR's own helper doc comment states that a `rename`-destructive site needs the extra symlink check.
These two sites are the exceptions to a rule the same PR wrote down.

## Why it is Medium and not High

A dangling symlink is a less common thing to lose than a file with contents, and the loss is of the link
rather than of data — the link's target was already absent. It is still a silent destruction of something
the user created, at a site whose two siblings guard against exactly this.

## Scope

`copilot::apply_op`'s `Rename` arm and `copilot::transfer_entry`. Compare against `rename_entry_impl` and
`move_exact_impl`, which are the correct shape.

## Acceptance criteria

- [ ] Both sites apply `symlink_slot_refusal` alongside `clobber_refusal`, matching `rename_entry_impl`.
- [ ] A test proves a **dangling** symlink at the destination survives, for each of the two sites, and that
      removing the check turns a **distinct** test red. Assert on the slot still being a symlink after the
      call — not on the returned `Result`, which was `ok: true` in the reviewer's reproduction.
- [ ] Check whether any **other** `fs::rename`-destructive site is missing the pairing. The reviewer found
      these two by enumeration; enumerate again rather than fixing only the two reported. If the pairing is
      always required, consider making it structurally impossible to apply one without the other rather
      than relying on every future author remembering.
- [ ] Platform-gate correctly. Symlink creation on Windows needs either Developer Mode or elevation, so a
      test that silently no-ops on an unprivileged runner proves nothing — detect and skip **loudly** with a
      `writeln!(stderr)` notice, and make sure the Linux and macOS legs assert something real. CI runs a
      3-OS matrix.
- [ ] Each guard broken **on its own** turns a **distinct** test red, real output pasted in the PR, per the
      Evidence Rules in `Ticketing/wiki.md`.

## Notes

Filed by the Foreman from the PR #893 review, 2026-08-13, on the reviewer's recommendation to handle it as
a follow-up rather than widening that PR.

**Useful technique, measured on this sprint:** a slot whose stat is genuinely refused can be staged locally
on Windows two independent ways — deny `(R)` on the target **plus `RD` on its parent** (which kills
`fs::metadata`'s `FindFirstFileW` fallback), or a **symlink whose resolution target is denied**. The second
exercises the reparse path and is the more natural fit here. See CPE-1705's "CORRECTION 4" section; that
ticket's guidance was wrong four times before this was understood, so read it before writing an ACL test.

Related: **CPE-1705** (which added `clobber_refusal` to these sites), **CPE-1687** (the honest-refusal
wording pattern), **CPE-1696** (the sibling stat-collapse round).
