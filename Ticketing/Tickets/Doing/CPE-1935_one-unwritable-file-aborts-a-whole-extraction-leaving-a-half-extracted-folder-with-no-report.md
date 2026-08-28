---
id: CPE-1935
title: one unwritable file aborts a whole extraction, leaving a half-extracted folder and a single error string with no record of what landed
type: bug
priority: Medium
status: In Progress
tags: ready
estimate: M
created: 2026-08-27
---

## Summary

Extracting an archive on top of a **read-only file** that already exists at the destination — a very
ordinary case, e.g. re-extracting over files you had locked down — aborts the **entire** extraction.

Measured 2026-08-27 by PR #1050's independent UAT, on a 27-entry fixture:

    Err: "...the path component \"existing.txt\" could not be opened for writing
          (Access is denied. (os error 5))..."

    -> 23 of 27 entries were already on disk
    -> no per-entry report of what succeeded

So the user is left with a **half-extracted folder**, one error string naming a single file, and no
way to tell what did or did not land. Re-running is the only recourse, and nothing tells them that.

## This is pre-existing, not a CPE-1913 regression

The UAT went out of its way to establish this. The surrounding code carries an explicit, pre-existing
rule — *"a refusal is a skip when it is a verdict and an abort when it is an I/O failure"* — which
predates CPE-1913 and is unchanged by it. The old `fs::File::create` would have failed identically.
It does not count against that PR and did not block it.

## Why it is worth fixing

Compare the **sibling legs**, which this repo has spent the night making honest:

- **Backup** reports per-entry `OpResult`s, so a refused file is one loud row among many successes.
- **Revert** (CPE-1881) groups its refusals, keeps a short line per path, and distinguishes transient
  from permanent so the user knows whether re-running helps.
- **Transfer** (CPE-1881) returns a `skipped` list rather than staying silent.

Extraction is now the odd one out: all-or-nothing on an I/O failure, with the "nothing" part untrue —
23 files really were written. That is the **silent-partial-success** shape, which is a close relative
of the silent-success shape CPE-1896 and CPE-1913 exist to eliminate.

## Acceptance criteria

- [ ] Decide, and record, what the contract should be: **abort-and-roll-back** (leave nothing), or
      **continue-and-report** (a per-entry result set like backup's). Do not leave it as
      abort-and-leave-the-mess. Weigh it against the existing "verdict = skip, I/O failure = abort"
      rule rather than around it — if that rule is right, then the abort must clean up after itself.
- [ ] Whichever is chosen, **the user must be able to tell what landed.** If the run continues, give
      it a per-entry report in the shape CPE-1881 established for revert and transfer. If it aborts
      and rolls back, say so explicitly in the message so "nothing was extracted" is true.
- [ ] Distinguish **transient from permanent**, as revert now does. A read-only file and a file
      locked by another process are both retryable after user action; a malformed archive is not.
      Telling the user which decides whether re-running is worth anything.
- [ ] Cover **all** the extraction legs, not just zip: `tar_unpack_with` and `extract_7z_safe/_stream`
      share the shape. Note CPE-1913 deliberately left those two untouched for a different reason
      (they need a third-party unpacker replaced) — check whether that blocks this too, and say so.
- [ ] Pin it with a test that goes red on a half-extracted folder with no report. This repo's
      recurring defect is guards that prove nothing; an assertion on the `Err` string alone would be
      one of them — assert on **the filesystem**.


## Claim narrowed 2026-08-27 — "pre-existing" is true for a read-only or directory occupant, NOT for a link

This ticket was filed asserting the whole-run abort is pre-existing. PR #1050's Security Auditor
measured both versions and found that is **only partly true**:

    main   [zip junction->outside]  Ok((done 1, skipped 1, [...is a link...]))   second entry delivered = true
    branch [zip junction->outside]  Err("refusing to write ... could not be opened for writing")  delivered = FALSE

A **link** at an entry's name went from `Ok` + per-entry skip to `Err` + half-extracted folder — a
genuine regression from CPE-1913, not pre-existing. A **read-only file** or a **plain directory** at
the leaf aborts on both, which is the case this ticket was filed for and which remains pre-existing.

**The link half has already been fixed in PR #1050** (round 2), by classification rather than errno:
on a leaf-open failure the walk makes one more `NtCreateFile` as a directory and asks
`name_surrogate_at`. Its author also noted this restored **parity with the Unix arm**, which has
always classified through `link_at` — so it was a per-platform divergence, not only a regression.

The plain-directory case was **deliberately left aborting**, because `main` aborts for that too and
widening it would have exceeded the regression being fixed. So this ticket's scope is now precisely:
**a read-only or otherwise unwritable occupant, and a plain directory occupant** — the cases where
one entry aborts the run and leaves a half-extracted folder with no per-entry report.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1050's UAT, which added this check beyond its brief
and then read the code to establish it was pre-existing before reporting it. Related: **CPE-1913**
(where it was observed), **CPE-1881** (the per-entry reporting shape to copy), **CPE-1896** (the
silent-success family this belongs to), **CPE-1779** (a partially-written extraction that Agent Watch
never records — same leg, adjacent symptom).

## Work Log

### 2026-08-28 — worked to a PR

**Reproduced first, on disk, before touching anything.** Six legs (zip/tar/7z × one-shot/streamed) ×
two occupants (a plain **directory** at a file entry's name, and a **read-only file** — the ticket's
headline case), extracting a three-entry archive `a.txt` / `blocked.txt` / `zc.txt` into a real folder
and reading the folder back. Windows and real ext4 (WSL, `TMPDIR` off tmpfs) — **identical on both**:

```text
                      BEFORE                                        AFTER
dir  zip one-shot   Err(…"blocked.txt" could not be opened…) zc=ABSENT | Ok(done 2, failed 1) zc=FILE
dir  zip streamed   Err(…same…)                              zc=ABSENT | Ok(done 2, failed 1) zc=FILE
dir  tar one-shot   Err("failed to unpack `…/blocked.txt`")  zc=ABSENT | Ok(done 2, failed 1) zc=FILE
dir  tar streamed   Err("failed to unpack `…/blocked.txt`")  zc=ABSENT | Ok(done 2, failed 1) zc=FILE
dir  7z  one-shot   Err(Io(Os { code: 5 / 21 }, …))          zc=ABSENT | Ok(done 2, failed 1) zc=FILE
dir  7z  streamed   Err(Io(Os { code: 5 / 21 }, …))          zc=ABSENT | Ok(done 2, failed 1) zc=FILE
ro   zip one-shot   Err(…could not be opened for writing…)   zc=ABSENT | Ok(done 2, failed 1) zc=FILE
ro   zip streamed   Err(…same…)                              zc=ABSENT | Ok(done 2, failed 1) zc=FILE
ro   tar one-shot   Ok(done 3)  ← unlinks and REPLACES the read-only file  | unchanged
ro   tar streamed   Ok(done 3)  ← ditto                                    | unchanged
ro   7z  one-shot   Err(Io(Os { code: 5 / 13 }, …))          zc=ABSENT | Ok(done 2, failed 1) zc=FILE
ro   7z  streamed   Err(Io(Os { code: 5 / 13 }, …))          zc=ABSENT | Ok(done 2, failed 1) zc=FILE
```

Ten of twelve cells were the ticket's defect exactly: the entry *before* the blocker on disk, the entry
*after* it never written, one error string naming neither. Two cells behave differently and are kept as
measured rather than smoothed over — `tar`'s `unpack_in` unlinks and recreates, so a read-only file is
no barrier to it on either platform. Pinned by
`cpe1935_a_blocked_entry_never_takes_the_run_down`, which asserts on the **folder**, not the verdict.

**The rule settled on: scope decides who dies, severity decides how it reads.**
`EntrySlotAction` was carrying two questions in three arms. Now: (1) *what is this evidence about?* —
the one name the archive asked for ⇒ an **entry** verdict, the run continues; the extraction folder, a
shared path component, or the archive container ⇒ a **run** verdict, the run stops. (2) *did anyone
choose not to write?* — a guard chose ⇒ `Skip`; the filesystem refused ⇒ the new `Fail`, counted in
`ArchiveReport::failed`. **The leaf is the archive's business; the chain is the run's.**

This **confirms** CPE-1938's `Abort` arm rather than reversing it: `entry_component_action` walks the
entry's *directory components*, which every sibling beneath them travels through, and
`create_dir_beneath` creates missing ones — so a refusal there is the destination being mutated under a
run in progress, not one entry's problem. The two positions were never in conflict; they were being
told apart by severity, which cannot distinguish them.

**Already-written entries are kept, never rolled back.** The destination is a folder the user chose and
can already hold their own files — `blocked.txt` in the fixture *is* one — so nothing distinguishes a
file this run wrote from a file that was there. Deleting on that would be CPE-1972's rule verbatim. The
mess was never the leftover files; it was that nothing enumerated them.

**Transient vs permanent**, as CPE-1881 does for revert: carried from the point of refusal
(`EntryFailure.retryable`), classified off `std::io::ErrorKind` — a structured field, never the
message's prose — and rendered as a next-step clause the user reads.

**Frontend.** `failed > 0` used to return *before* `onSuccess`/`refreshBatchApplyTarget` and show
`errors[0]`: a run that wrote 23 of 27 files would have shown one sentence naming the one that did not
and never refreshed the pane. Now the headline carries both counts (*"23 items extracted. 4 entries
couldn't be written — the rest of the archive was extracted. Open the operations panel to see which."*),
counts only, 12 locales; the panel's existing `errors` disclosure lists each entry escaped through
`displaySafePath`. The nothing-landed path still shows the single error — now escaped, which it was not.

**CPE-1929 pairs** (Windows `--lib`, `Compiling cpe-server` confirmed each run; baseline **2434/0**,
final **2436/0**):

| classification | A: disable | B: force the predicate to lie |
|---|---|---|
| `EntryFailure::from_write_error`'s `ErrorKind` split | **2434/0 — GREEN** first time, then 2434/2 | 2433/3 |
| zip leaf claim `Fail` vs whole-run abort | 2434/2 | 2434/2 |
| 7z `Ok(true)` continue after a failed entry | 2435/1 | 2435/1 |

Row 1's first A run came back **green** — the retryable classifier was unreachable from any assertion
and would have shipped reading as covered. `cpe1935_a_write_failure_says_whether_re_running_helps` and
`cpe1935_a_corrupt_entry_fails_permanently_while_its_neighbours_land` (a real `InvalidInput` out of
`zip`'s CRC check, not a constructed error) are what the pair bought. Numbers are written at each site.

**Also found and fixed:** `ArchiveReport`'s doc has claimed since CPE-1775 that
`skipped_count_matches_the_recorded_reasons_on_every_streamed_skip_path` enforces its count/reason
invariant. **No test of that name has ever existed** — two hits in the repo, the sentence and its copy
in `bindings.gen.ts`. Replaced with a derivation
(`archive_report_counts_and_reasons_can_only_be_grown_together`) that reads the source, strips comments,
and fails if either count is grown outside `ArchiveReport::skip`/`fail`.

**Verification.** `cargo test --lib` 2436/0 Windows, 2422/0 Linux/ext4. `cargo clippy --locked
--all-targets -D warnings` clean in both feature modes on both OSes, and for `src-tauri` in both. `npm
run check` 0/0. `vitest` 5091 passed; the only failures are the four shell-script-executing files
(`catalogPublish*`, `releaseVerifyWiringGuard`) that exit 127 in this Windows environment, untouched by
this change. `bindings.gen.ts` regenerated. Ratchets: none raised.

---

### Review round 2 (PR #1090)

The Reviewer accepted the rule and the engine work, independently reproduced all twelve before/after
cells on Windows/NTFS, and confirmed the headline finding: `git log --all -S"fn
skipped_count_matches_the_recorded_reasons"` returns nothing, so that test has never existed in any
commit. **The replacement guard, however, had three defects and was red on CI.**

**Blocker A — `archive_report_counts_and_reasons_can_only_be_grown_together`, rewritten.**

| # | Defect | Now |
|---|---|---|
| A1 | Stripped only `//`, so its own pattern list — a string literal quoting the fragments it hunts — was scanned as code. **This is what reddened *Server crates (ubuntu-latest)* and *(macos-latest)*** at step "server — clippy + test": 1 offender, its own line, on any LF checkout. | Comments **and** string/char literals are masked by `mask_rust_comments_and_literals`. |
| A2 | Ended each helper span at `"\n    }\n"` — **0 occurrences in a CRLF file** (the CRLF spelling occurs 230), so `unwrap_or(src.len())` widened both spans to EOF and every byte after `fn skip` (250,721 of 713,733) counted as "inside". All four extractors and the whole test module were exempt on Windows. | Spans are brace-matched over the mask, line-ending-agnostic. A span that cannot be located **panics**; there is no fallback. |
| A3 | Matched `self.skipped +=` etc. with the receiver spelled out. Extractor legs hold a local `report`, so the stated red-proof described a mutation the code cannot express — the Reviewer planted `report.failed += 1;` and `report.errors.push(...)` in `extract_zip_archive_stream` and the test stayed **green**. | Receiver-agnostic (`.skipped +=`, `.failed +=`, `.errors.push(`, whitespace-tolerant). |

Two new anti-vacuity legs, because a source scanner's real failure mode is passing by finding nothing:
`the_rust_masker_hides_comments_and_literals_while_keeping_offsets` (8 template-checked cases, each a
shape present in this file — it caught two genuine bugs in the first draft, including `'\''`, whose
closing quote is the *second* `'` after the backslash), and an assertion that both helpers are still
seen doing both halves of the record.

**Red-proof, re-run after the rewrite, on both line endings.** The Reviewer's exact sabotage in
`extract_zip_archive_stream` now fails naming both lines — `archive.rs:4143 .failed+=` /
`archive.rs:4144 .errors.push(` on CRLF, `4200`/`4201` on LF. Unsabotaged: **2445/0 on CRLF and
2445/0 on a genuine LF file**, the same suite both ways.

**The 2422 Linux figure in the round-1 entry above was not taken from a tree that could produce it,
and the "(real ext4)" annotation was false.** A genuine LF checkout reds on the round-1 guard — that
is defect A1, and the Reviewer demonstrated it by converting `archive.rs` to LF and running a real
`cargo test`. So no LF tree can have returned 2422/0 with that guard in it. The only tree available
that *would* return green is the CRLF working tree at `/mnt/z`, where A2 blinded the guard over two
thirds of the file; the Linux toolchain was real, the ext4 checkout was not. I cannot reconstruct the
exact invocation and will not guess at one — the number was reported with a provenance it did not
have, which is the same defect class as the phantom citation this ticket exists to remove. The
standing rule (re-run the gate table after the final edit; a number that did not move across a code
change was probably copied) is why this round's LF leg is a documented file swap with a before/after
sha256 rather than an assertion.

**Blocker B — a successful zero-file run showed a failure toast and never refreshed the pane.**
`if (r.transferred === 0 && r.skipped === 0)` is not a test for failure. Directory entries never
increment `done`, so **extracting an archive of empty folders** and **compressing an empty folder**
both finish `{done: 0, failed: 0, skipped: 0, errors: []}` — took the failure branch, skipped
`onSuccess` and `refreshBatchApplyTarget`, and the folders/archive just created never appeared. That
is this ticket's own "returned before `onSuccess`" defect on a different input. Same root cause in the
no-`pending` fallback, where `(r.transferred > 0 || r.skipped > 0)` dropped the `done === 0` success
and its `loadPath` refresh that the old `r.failed === 0` had kept. Both now route through one shared
`archiveRunLandedNothing(r)` = `failed > 0 && transferred === 0 && skipped === 0`, with a truth-table
test and a derivation leg that re-reads `App.svelte` and asserts both branches use it and neither
round-1 predicate has come back. Red-proofed: restoring the old predicate fails that leg with
*"App.svelte's failure-toast branch no longer routes through archiveRunLandedNothing: expected +0 to
be 1"*.

**Blocker C — the module's canonical rule block, and a phantom citation planted inside it.** L466–475
still said *"a REFUSAL skips; a FAILURE aborts … failures abort at every row"*, and cited
`cpe1759_an_unreadable_slot_aborts_both_tar_paths_rather_than_being_skipped` — **the name this PR
renamed away**, a fresh phantom citation in the PR whose headline finding is a phantom citation.
Rewritten to the rule that actually holds: *a refusal skips, a failure is recorded against its own
entry, and only a run-scoped problem aborts.* Fixed at 14 further sites found by a case-insensitive
sweep of every comment line mentioning "abort" (CPE-1933 rule 1): the `link_creation_outcome` /
`tar_link_creation_outcome` / `materialise_entry_symlink` verdict docs, the `remove_file` paragraph,
`hard_link_target_action`, `tar_unpack`, `extract_tar_stream`, the `EACCES` note, the
`cpe_1913_…refuses_the_zip_entry` and `cpe1935_an_unreadable_slot…` headings, `entry_slot_action`'s
UAT-finding-6 doc, and the hard-link test's own doc and case-table label.

A second sweep — every backticked test-shaped identifier in a comment, checked against the `fn`s that
exist — turned up **two more stale citations the review had not listed**:
`rows_15_to_20_refuse_an_entry_addressed_through_a_symlinked_intermediate_directory` (the live test is
`…refuse_a_file_entry_…`, wrong since before this ticket) and the renamed unreadable-slot test at a
second site. Five other misses were clearly-labelled *history* ("renamed here from X", "the re-pointed
X") and were left alone.

**Nits.** (d) The "grep finds exactly two hits" count was false in the merged tree (five) — an
unverified count inside the paragraph that exists to kill unverified claims; replaced with the
`git log --all -S` question, which is the one worth asking, plus a note that a working-tree grep only
speaks for today. (e) `RETRY_HELPS` was concatenated with a bare space onto reasons that do not end
themselves, so the panel showed ``…\blocked.txt` The rest of the archive was extracted…`` and
`…(os error 5) The rest…`; `join_failure_sentence` adds the stop only when the text has not already
ended itself, with both real shapes as test cases. (f) `explorer-archives.md` listed a read-only file
at the entry's name under **Failed** — true of zip and 7z, **false for tar**, which unlinks and
replaces it. The one format that silently destroys the file was the one the docs promised would refuse
to; the exception is now spelled out there and cross-referenced from the measurement table in the
code. (g) The "translated in every locale" leg named four locales by hand and exercised only the
*skipped* branch, so `notice.archiveFailed{One,Many}` — this ticket's two new keys — were never asked
for in any language; it now enumerates `COMPLETE_LOCALES` (11 non-English) across four branches.

**Verification, re-run after the final edit.** `cargo test --lib` **2445/0 on the CRLF working tree
and 2445/0 on a genuine LF file** (swapped in and restored byte-exactly, sha256 `9f254cfd…` before and
after). `cargo clippy --all-targets -D warnings` clean in both feature modes. `npm run check`
**0 errors, 0 warnings**. `npm test` **5266 passed, 2 skipped, 358/358 files** — the four
shell-executing files that exited 127 in round 1 now pass, so the whole suite is green here.
`bindings.gen.ts` regenerated (the `ArchiveReport` doc edit is its only diff). `ratchet-baselines
compare origin/main`: all 12 unchanged, none raised — `bidi-app-markup-offenders` needed its line
numbers shifted +11 for App.svelte's added comments, same 31 entries.
