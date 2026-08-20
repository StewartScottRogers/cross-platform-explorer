---
id: CPE-1759
title: tar destroys a link at an entry name, and one-shot zip aborts where streamed zip skips
type: task
priority: Medium
status: Done
tags: ready
estimate: M
created: 2026-08-15
closed: 2026-08-20
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

## Work Log

- 2026-08-20 — merged as **#958** (`232ad326`), batch 34. **Four rework rounds**, three independent reviews
  and one UAT.

### What "destroys" meant
Unlink-and-replace, not follow-and-overwrite. `tar-0.4.46` opens the destination with `create_new(true)`;
a symlink there yields `AlreadyExists`, so tar calls `remove_file` and retries. `remove_file` does not
follow a symlink, so the user's link was deleted and a regular file written in its place — and the call
returned `Ok`, which is why nothing could report the loss.

### Abort vs skip — skip, and the reason two tickets were wrong
**Abort's stated virtue did not exist.** CPE-1744 and the CPE-1773/1774 review both recorded the one-shot
zip abort as leaving an empty destination. Both were measuring archives poisoned at **entry 0**. Poison
entry two of three and `a.txt` is on disk, `c.txt` is not, and the error names neither. The reviewer
confirmed it at the source — `zip-2.4.2/src/read.rs:782-785` says outright: *"Extraction is not atomic. If
an error is encountered, some of the files may be left on disk."* The UAT then reproduced it independently
on its own fixtures. **A false premise had survived two tickets.**

So the real choice was "partial, with an error naming neither half" versus "complete-but-one, with a counted
refusal" — and skip was already the contract at 22 of 23 sinks.

### What else came out of it
- Symlink materialisation moved into the shared loop, so the **streamed** path — the one every shipping
  user hits — now creates real links instead of writing a text file containing the target's name.
- An escaping tar **hard**-link entry no longer kills the whole run; it is a counted skip, resolved against
  the base the crate itself uses.
- **A genuine same-`ErrorKind` collision on POSIX**: `EPERM` and `EACCES` share one kind, and `remove_file`
  answers `EPERM` on macOS for a directory — so without a direct return, this PR's own test would have
  flipped abort→refusal **on the macOS leg alone**.

### The rework pattern, recorded because it is the lesson
Rounds 2, 3 and 4 all found the same species of defect: **the code was right and the sentence about it was
false.** The author's own diagnosis: *"the measurement discipline stopped at the boundary of the thing under
test and didn't extend to the sentence about it."* The 1314/`PermissionDenied` claim was measured false
(1314 is `Uncategorized`); the in-app help promised a refusal the code did not deliver; three further
statements generalised from the mechanism's shape. The final round swept its own remaining claims and found
a fourth. The one claim that could not be checked — which Windows codes a FAT volume emits — is now labelled
documentation-derived with the direction of error stated.

### Left as tickets
**CPE-1812** (the leaf-link guard is not independently pinned — removing it alone leaves the suite green),
**CPE-1813** (TAR still does not deliver the no-link-support refusal ZIP does), **CPE-1814** (a dead
`Skip|Abort` collapse, a staging failure that `return`s, dangling cfg-gated doc links, an unqualified
taxonomy line), **CPE-1807** (the fourth, unmerged zip loop), **CPE-1808** (the flake fix's untouched twin),
**CPE-1809** (an unfailable `contains` assertion).
