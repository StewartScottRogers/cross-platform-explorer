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

- [x] Decide, and record the decision at `replace_file_contents` alongside the existing statement of the
      gap — including "we chose to leave it" if that is the answer.
- [x] If a sweeper is built: it must never remove a temp belonging to a live save (test it with a second
      temp created seconds earlier), must never follow or delete a link, and must assert on the
      **filesystem** (which files survive), never on the returned `Result`.
- [x] If a sweeper is built, update both doc pages to drop the "safe to delete" instruction they now give.

## Notes

Filed by CPE-1725 from PR #904's review round, 2026-08-14. Related: **CPE-1716** (created
`replace_file_contents`), **CPE-1725** (routed the second save path through it, doubling the exposure).
The stale-temp case is already anticipated in `stage_and_replace`'s own stamp comment ("a stale temp left
by an earlier crash") — the stamp exists so a stale file cannot cause a collision, which is why this is a
tidiness bug and not a correctness one.

**Correction to this ticket's own "two save paths" framing.** By the time this ticket was worked, PR #904
had already been narrowed (its own second round, "share the dangling-link classifier, not the write
strategy") so that `write_file_text_impl` (the preview-pane text editor) calls plain `fs::write`, not
`stage_and_replace` — only `resolve_write_target`'s symlink decision is shared with `metadata_write`. So
the preview editor never creates a `.cpe-tmp` at all today, and `src/docs/03-explorer.md` was checked and
does not mention `.cpe-tmp` anywhere (grepped). Only `metadata_write` (Metadata Studio) is exposed, and
only `src/docs/25-metadata-studio.md` needed the doc fix. Recorded so nobody goes looking for a change in
`03-explorer.md` that this ticket did not need to make.

## Work Log

2026-08-14 — Implemented (PR TBD, filled below). **Decision: built the sweep** (the ticket's middle
option), rejecting both "do nothing" and "sweep on startup":

- "Do nothing" was weighed against `PURPOSE.md`'s fast/small/predictable tiebreaker rather than dismissed
  as over-engineering: a file that silently accumulates forever in a folder the user curates themselves is
  not the small, predictable footprint that tiebreaker protects.
- "Sweep on startup" was rejected because the app tracks no list of folders it has ever saved into, and
  building one would be a second feature purely to support this one.
- The sweep (`sweep_stale_temp_siblings` in `crates/server/src/fsutil.rs`) runs from `stage_and_replace`
  **once, after its own rename succeeds** — never before staging, never on a failed save (which already
  removes its own temp on its existing error branches). It matches only `target`'s OWN
  `<name>.*.cpe-tmp` siblings — never a directory-wide glob — so it can never mistake a different file's
  in-flight temp for one of this file's leftovers. "Stale" is decided by an **age floor** (5 minutes, an
  order of magnitude above any plausible in-flight save), not by asking the OS whether the stamped pid is
  still alive — pid-liveness needs a process-enumeration dependency this crate does not otherwise carry,
  and is the wrong question besides (a pid can be reused, and a process can outlive one particular save it
  made). It never touches — never even opens — anything `symlink_metadata` reports as a link.
- Cost on the common/successful save path: one `read_dir` of the folder just written into, after the
  write has already completed, filtered to a name prefix that will almost always match nothing. Bounded to
  the successful-save case only; a failed save pays nothing extra. On a slow network share this is judged
  an acceptable, bounded addition — no worse than the folder-listing cost the Explorer's own browsing
  already tolerates there (`docs/design/STREAMING.md`), and it never turns an already-successful save into
  a reported failure (every internal error is swallowed).
- Tests (`crates/server/src/fsutil.rs`): a pure `should_sweep_temp` truth table (the link arm cannot be
  proven by real-IO ageing — a dangling symlink can't be opened to set its mtime, so real IO can only ever
  stage a YOUNG link, not an OLD one); a real-filesystem test that ages one orphaned temp past the floor
  via `File::set_modified` and leaves a second one fresh, asserting the fresh one survives (the AC's own
  "created seconds earlier" scenario) with the filesystem checked BEFORE the `Result` is unwrapped; a test
  that a different file's stale temp in the same directory is untouched; and a wiring test with a real
  (dangling, privilege-free) symlink at a matching name. Verified each guard breaks a DISTINCT test when
  neutralised (is-symlink check removed → only the pure table's link row reds; age check removed → the
  pure table's age rows AND the real-IO flagship test both red, with the flagship's message showing
  `result was Ok(())` — proving the save silently succeeded while sweeping the wrong file).
- Docs: `src/docs/25-metadata-studio.md` no longer tells the user to delete a `.cpe-tmp` by hand — both
  the "killed mid-save" bullet and the symlinked-files paragraph now say it cleans itself up on the next
  save of that file. `src/docs/03-explorer.md` needed no change (see the correction note above it).
