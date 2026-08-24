---
id: CPE-1809
title: an archive test assertion cannot fail, and a staging failure returns where it should continue
type: bug
priority: Medium
status: Done
tags: ready
estimate: S
created: 2026-08-20
closed: 2026-08-23
---

## Problem

Two independent defects in `crates/server/src/archive.rs`, both found while reviewing PR #958:

1. **`err.contains("hard")` at `7237` cannot fail.** The scratch directory in that test is named
   `..._hardlink`, so the substring is present in the error text no matter what the error actually says.
   The assertion certifies nothing.
2. **A staging failure `return`s instead of `continue`s at `5605`.** A fixture that fails to stage one
   entry abandons the rest of the setup, so the test then runs against a **partially built archive** —
   and passes, having exercised far less than it claims.

## Why it matters

Both belong to the same family: a test that reports success while proving less than its name says. This
crew found **nine candidate cannot-fail tests in one sprint and eight were real** — the pattern is not
theoretical here, it is the dominant defect class in this file.

The second is the more insidious of the two, because the test still *does* something; it just silently
does less, and no output distinguishes the truncated run from the full one.

## What to do

- Fix `7237` to assert against something the fixture's own naming cannot supply. Then **red-proof it**:
  make the error say something else entirely and confirm the assertion now fails — the test as written
  would not have.
- Fix `5605` to `continue`, or fail the test outright on a staging failure. **Failing loudly is probably
  right**: a fixture that cannot be staged is a broken test, not a smaller test.
- Sweep the file for both shapes — a `contains` assertion whose needle appears in the fixture's own path,
  and an early `return` inside setup. Report what the sweep found even if it found nothing.

## Notes

Filed by the Foreman from the independent review of PR #958, 2026-08-20. Both pre-existing and explicitly
left out of that PR rather than widening it.

Related: **CPE-1759**, and the Evidence Rules in `Ticketing/wiki.md`.

## Work Log

2026-08-23 — Located both defects by content (the ticket's line numbers, 7237/5605, were from PR #958's
branch state at review time — 2026-08-20 06:13, between the first CPE-1759 commit and its round-2 review
— not the final merged file, and ~40 batches of unrelated tickets have landed in this file since).
Defect 1 = `cpe1759_an_escaping_tar_hard_link_is_skipped_while_a_missing_target_still_fails`'s
`err.contains("hard")`. Defect 2 = `one_shot_and_streamed_zip_answer_a_link_at_an_entry_name_identically`'s
`return;` inside its `for (label, run, records) in legs` loop.
2026-08-23 — Defect 1 fix: renamed the test's scratch dir from `cpe1759_hardlink` to
`cpe1759_tar_link_escape` (no "hard" substring anywhere in the path) and strengthened the assertion to
`err.contains("could not create the link") && err.contains("hard")` — the wrapper phrase is
`tar_link_creation_outcome`'s own fixed text, never suppliable by a fixture or entry name. Red-proofed by
replacing that wrapper's text with `"boom: {e}"`: the test went red, and the panic message itself proved
the OLD assertion (`contains("hard")` alone) would NOT have caught it — the entry's own name ("hard") is
still the final path component even under the broken wording. Evidence in the PR body.
2026-08-23 — Defect 2 fix: `return` → `continue` in `one_shot_and_streamed_zip_answer_a_link_at_an_entry_name_identically`.
2026-08-23 — Swept the whole file for both shapes per the ticket's ask. Found the SAME return-vs-continue
shape in six more places, all "for loop over independent legs/rows, each staging its own link, `return`
on the first staging failure" — fixed all seven to `continue`:
`every_guarded_row_refuses_a_live_link_without_touching_its_target` (loses 8 of 9 `GUARDED_ROWS` on one
bad stage — the worst instance found), `rows_15_and_16_refuse_a_live_link_and_still_extract_the_rest`,
`rows_21_and_22_tar_refuse_a_link_at_an_entry_name_and_still_extract_the_rest`,
`one_shot_and_streamed_zip_answer_a_link_at_an_entry_name_identically` (the named defect),
`rows_15_to_20_refuse_a_file_entry_addressed_through_a_symlinked_intermediate_directory` and
`row18_refuses_a_directory_entry_that_would_be_created_outside_the_extraction_folder` (both via
`stage_intermediate_dir_escape`'s `let-else`), and
`row17_a_dangling_link_at_the_extraction_destination_is_reported_as_a_link`. Left `assert_row_refuses_a_dangling_link`,
`cpe1759_a_link_entry_overwrites_an_ordinary_file_but_a_directory_is_a_failure`'s up-front probe, and the
other single-scenario `return`s alone — each stages once for its ONE test, not for a table of independent
legs, so there is nothing a `return` there could silently abandon. No other `contains`-on-fixture-naming
shape found beyond the one fixed (checked every scratch-dir name against its own assertions).
2026-08-23 — This coverage-loss defect (unlike defect 1) has no clean assertion-level red state to
demonstrate — a `return` firing on a legitimate skip does not fail the test, it just silently tests less,
which is the whole danger. Evidence offered instead: the mechanism is stated in each fix's inline comment,
and `cargo test` stayed green after all seven conversions (no coverage regression from the change itself).
2026-08-23 — Status: Doing → ready to close alongside CPE-1837/CPE-1812 in one PR.
