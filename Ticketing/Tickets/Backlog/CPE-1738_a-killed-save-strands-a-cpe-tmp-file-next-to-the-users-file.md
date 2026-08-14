---
id: CPE-1738
title: A killed save strands a .cpe-tmp file next to the user's file, and nothing ever collects it
type: bug
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-14
closed:
---

## Problem

Found by the PR #904 (CPE-1725) review, 2026-08-14, while checking that the user documentation matched
what `cpe_server::fsutil::replace_file_contents` actually does.

The atomic save stages a temp sibling and renames it into place. `stage_and_replace`
(`crates/server/src/fsutil.rs`) removes that temp on **two** paths, and both are error branches:

- the `write_all` / `sync_all` failure branch, and
- the `rename` failure branch.

So the temp is cleaned up when the save **fails**, and not when the save is **killed**. A force-quit, a
crash, an OOM kill or a power cut between the `create_new` and the `rename` leaves a
`<name>.<pid>-<nanos>.cpe-tmp` sitting in the user's own folder. **There is no sweeper anywhere in the
repo** — searched: `rg 'cpe-tmp'` across the whole tree, which finds only the construction site, its
tests, and the two doc pages.

Nothing is damaged: the original file is untouched (that is the whole point of staging), and the
pid+nanosecond stamp means a later save can never collide with the stale file. The defect is that the
user sees an unexplained file next to theirs, in a folder they curate, and it stays forever.

This is now **two** save paths' worth of exposure, not one: CPE-1725 routed the preview pane's text
editor through the same function the Metadata Studio uses, so any saved text file can strand one.

## What was already done rather than left implied

PR #904 did **not** fix this, and deliberately scoped itself to not over-claiming about it:

- `replace_file_contents`'s "What this does NOT do" list now states the gap at the site.
- Both user-facing pages (`src/docs/03-explorer.md`, `src/docs/25-metadata-studio.md`) previously said an
  interrupted save — explicitly listing "the app is closed" — left the file intact *and cleaned up the
  temporary file*. The second half was false for exactly the cause the sentence named. Both now say the
  original is safe either way, and that a killed app can leave a `.cpe-tmp` file which is safe to delete.

So the documentation is true today whichever way this ticket is decided. What is open is the behaviour.

## The decision to make

Is a sweeper wanted at all? Options, roughly in increasing cost:

1. **Nothing.** The docs explain it; stale temps are rare and harmless. Defensible.
2. **Opportunistic sweep on save.** Before staging, remove `*.cpe-tmp` siblings in the target's directory
   older than some age. Cheap, but it is a *delete* in the user's folder driven by a filename pattern,
   which is precisely the shape this repo has filed several tickets about — it would need
   `symlink_slot_refusal`-grade care, an age floor well above any plausible in-flight save, and it must
   never touch a temp another process is actively writing.
3. **Sweep on startup**, scoped to folders the app itself has saved into (it does not track those today).

Option 2's "delete by name pattern" hazard is the reason this is filed rather than done inline.

## Acceptance criteria

- [ ] Decide, and record the decision at `replace_file_contents` alongside the existing statement of the
      gap — including "we chose to leave it" if that is the answer.
- [ ] If a sweeper is built: it must never remove a temp belonging to a live save (test it with a second
      temp created seconds earlier), must never follow or delete a link, and must assert on the
      **filesystem** (which files survive), never on the returned `Result`.
- [ ] If a sweeper is built, update both doc pages to drop the "safe to delete" instruction they now give.

## Notes

Filed by CPE-1725 from PR #904's review round, 2026-08-14. Related: **CPE-1716** (created
`replace_file_contents`), **CPE-1725** (routed the second save path through it, doubling the exposure).
The stale-temp case is already anticipated in `stage_and_replace`'s own stamp comment ("a stale temp left
by an earlier crash") — the stamp exists so a stale file cannot cause a collision, which is why this is a
tidiness bug and not a correctness one.
