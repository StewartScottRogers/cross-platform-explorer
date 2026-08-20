---
id: CPE-1814
title: archive.rs carries a dead Skip|Abort collapse, a staging failure that returns instead of continues, and dangling cfg-gated doc links
type: task
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-20
closed:
---

## Problem

Four small defects in `crates/server/src/archive.rs`, all found while reviewing PR #958 and all deliberately
left out of it rather than widening a PR that already took three rework rounds:

1. **A dead `Skip(m) | Abort(m) => Some(m)` collapse at `:3186`.** Its only feeder returns `Write`/`Skip`,
   so the `Abort` arm is unreachable **today**. This is the *exact* construct that became CPE-1759's first
   blocking finding — it was dead there too, until a new feeder made it live and an entry started silently
   skipping where it should have aborted. It is a loaded gun with the safety on.
2. **A staging failure `return`s instead of `continue`s at `:6187`.** A fixture that fails to stage one
   entry abandons the rest of the setup, so the test runs against a **partially built** archive and passes,
   having exercised far less than its name claims. Third sighting of this shape in this file.
3. **Three dangling `cfg`-gated intra-doc links** — they resolve only on the platform whose item is
   compiled, so `cargo doc` on the other platform emits a broken link.
4. **A false comment at `:3081`** claiming a consolidated loop "is now the only zip extractor". See
   CPE-1807 — `extract_zip_encrypted` is a fourth, unmerged loop.
5. **An unqualified taxonomy entry at `:451-455`.** The decision-kind taxonomy CPE-1759 added as its own
   checkable rule still lists "a link this platform will not create" among refusals **without the ZIP/TAR
   qualification** the rest of that PR spent three rounds getting right. It is new in CPE-1759 and its
   final delta did not reach it. Non-blocking there because it is an internal comment rather than shipped
   help, it describes *kinds of decision* rather than a format promise, and the same comment block
   corrects it 33 lines down at `:488` — but a taxonomy that contradicts itself within one block is worth
   one line of work. **Fix it together with CPE-1813**, whose whole subject is that split.

## Why it matters

Item 1 is the interesting one and the reason this is `Medium`-worthy despite the `Low` priority: **a dead
arm is not a harmless arm.** CPE-1759 demonstrated the failure mode end to end within a single PR — the
construct sat inert on `main`, a change added a feeder, and it immediately produced `Ok` with an entry
missing. Removing it (or making it abort) costs nothing now and forecloses that.

Item 2 is the recurring one. It has now been found three separate times in this file, which suggests
copy-paste rather than coincidence — worth a sweep, not just three fixes.

## What to do

- For item 1: either delete the unreachable arm or make it abort. **Do not** leave it collapsed with a
  comment saying it is unreachable — that is precisely the state it was in before it became a bug.
- For item 2: **fail loudly** on a staging failure rather than `continue`; a fixture that cannot be staged
  is a broken test, not a smaller one. Then grep the file for every `return` inside a setup loop and report
  what the sweep found, even if it found nothing.
- Items 3 and 4 are text; fix by re-reading the code they describe.
- Red-proof item 2 by making a stage fail deliberately and confirming the test now fails rather than
  quietly shrinking.

## Notes

Filed by the Foreman from the round-2 and round-3 re-reviews of PR #958, 2026-08-20.

Related: **CPE-1759** (where the dead-collapse shape became a live bug), **CPE-1807** (the fourth zip loop),
**CPE-1809** (the earlier sighting of the staging-failure shape).
