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
