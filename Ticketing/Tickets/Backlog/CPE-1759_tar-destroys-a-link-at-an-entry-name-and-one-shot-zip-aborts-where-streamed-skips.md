---
id: CPE-1759
title: tar destroys a link at an entry name, and one-shot zip aborts where streamed zip skips
type: task
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-15
closed:
---

## Why this exists

Split out of **CPE-1744** by the worker that closed its containment half. Both behaviours below are
**already pinned by characterization tests** and both were re-aimed at this ticket in that PR, so the code
does not name a closed ticket. Neither is a leak — each fails *safe* — but each is a real defect of a
different shape, and the in-app docs can only describe one behaviour per format.

## 1. TAR destroys a pre-existing link at an entry's name

Measured (CPE-1733 UAT, reproduced independently), live link at `dest/a.txt` pointing at a victim outside
`dest`:

```text
[tar ONE-SHOT and STREAMED]  outcome = Ok(..)   victim bytes = Some("VICTIM ORIGINAL")
                             slot is link = Ok(false)   slot is file = Ok(true)
```

tar does not *follow* the link — it **unlinks it and writes a regular file in its place**. The victim's
bytes are safe; the **user's link is silently gone** and the call reports success. ZIP and 7z both skip
the entry and leave the link alone (CPE-1733/CPE-1746), so tar is the odd one out.

**Why CPE-1744 did not close it, stated so the next worker does not re-derive it:** `extract_tar_stream`
has a per-entry hook (`Entry::unpack_in`) and would take the same `entry_slot_action` check in about five
lines — but the one-shot path is `tar::Archive::unpack`, which has **no** per-entry hook. Guarding only
the streamed path would *manufacture* a fresh one-shot/streamed divergence in order to fix item 2 below.
Guarding both means either reimplementing `unpack` (including the deferred directory-mtime pass it runs
after its entry loop) or sweeping `dest` before handing off. Which of those is right is a design decision
CPE-1744's own checklist flagged as needing confirmation, and it is not the same decision as the
containment guard — so the whole item moved rather than being half-shipped.

**Pinned by** `archive::tests::tar_extraction_destroys_a_link_at_an_entry_name_rather_than_following_it`
(both tar paths). Fixing this turns it red — **that is the intended signal**. Re-point it in the same
commit; never delete it. Three descriptions move with it: `archive.rs`'s section comment, the format
bullet under *Safety limits* in `src/docs/explorer-archives.md`, and that file's "covers ZIP and 7-Zip,
but not TAR" bullet under *Limits / notes*.

## 2. One-shot ZIP aborts the whole extraction where streamed ZIP skips one entry

```text
[zip ONE-SHOT]        outcome = Err("invalid Zip archive: Invalid symlink target path")
                      victim bytes = Some("VICTIM ORIGINAL")   b.txt extracted = false
[zip STREAMED]        outcome = Ok(ArchiveReport { done: 1, errors: ["a.txt: … is a link …"] })
                      b.txt extracted = true
```

Two shipped paths, opposite answers to the same input, and the same divergence shows up again on the
CPE-1744 containment case (one-shot zip errors `"invalid Zip archive: Invalid symlink target path"`,
streamed zip skips-and-records). `extract_archive` is a registered Tauri command in `bindings.gen.ts` with
**no current Svelte caller**, so this is an API/doc inconsistency rather than a live UI regression.

**The measurement CPE-1744 added, and the reason it recorded rather than closed this:** the alignment can
only run one way — `zip::ZipArchive::extract` has no progress or cancel hook, so the streamed loop would
have to take over the one-shot path. But `ZipArchive::extract` is **not** a plain loop over the same work:
it restores unix permission bits and materialises a stored symlink entry as a real symlink, neither of
which `extract_zip_archive_stream` does. So "align them" is not a no-op refactor — it silently downgrades
the *more* capable path, on a general file explorer, to fix a divergence that currently fails safe.

**That is the decision this ticket owes an answer to**, and it is a real fork:

- (a) route `extract_archive`'s zip branch through `extract_zip_archive_stream` **and** teach that loop
      permission bits + symlink entries first, so nothing is lost; or
- (b) keep both and document two behaviours per format honestly; or
- (c) make the streamed path abort too — the safest and the least useful.

**Pinned by** `archive::tests::one_shot_zip_extraction_aborts_everything_when_an_entry_lands_on_a_link`
(`b.txt` absent is what separates "skipped an entry" from "abandoned the run"; the assertion requires the
zip crate's *symlink* refusal rather than any I/O error). Fixing this turns it red — intended. Re-point it
and the *Safety limits* format bullet in `src/docs/explorer-archives.md` in the same commit.

## What to do

- [ ] Answer item 2's fork explicitly and write the answer down before coding.
- [ ] Confirm `tar::Archive::unpack`'s post-loop work before replacing it — reimplementing it and quietly
      dropping the directory-mtime pass would be a regression traded for a guard.
- [ ] Every guard broken on its own, a **distinct** test red, real output pasted, restored with
      `git checkout --`. Assert on the filesystem and the bytes **before** unwrapping the `Result`.
- [ ] Pin a **distinctive** refusal, not `is_err()` — on an unprivileged Windows runner a dangling
      junction makes `File::create` fail by itself (`Access is denied`, os error 5, measured for
      CPE-1733), so an `is_err()`-only leg passes straight through a deleted guard.
- [ ] Re-point **both** characterization tests named above and move every description each names, in the
      same commit.

## Notes

Filed by the CPE-1744 worker, 2026-08-15. Related: **CPE-1744** (the containment half + two wording
defects, closed), **CPE-1733** (the enumeration and the leaf-link guards), **CPE-1746** (the 7z half),
**CPE-1758** (the other CPE-1744 remainder).
