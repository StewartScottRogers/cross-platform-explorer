---
id: CPE-1980
title: '`download_tree` returns `Ok` for a file it did not deliver, when a planted link is what stopped it — decide whether a link verdict is a delivery failure'
type: bug
priority: Medium
status: Open
tags: ready
estimate: M
created: 2026-08-28
---

## Summary

Filed out of CPE-1961 (PR #1089), at the Reviewer's request: round 5 wrote the finding as a **code
comment saying "a separate ticket's"**, and a note that names a ticket which does not exist is a note
that will be read as closed. This is that ticket.

`transfer::download_tree` classifies a **link verdict** — a directory component that is (or becomes) a
symlink/junction — as `DownloadReport::skipped`, and returns `Ok`. Only entries that reach
`undelivered` change the call's verdict. So:

> **A file the user asked for, did not get, and that `download_tree` reports as success.**

The reason *is* named — the entry lands in `DownloadReport::skipped` with its wording, and on stderr —
but the caller's `Result` says the download worked.

## Scope: this is pre-existing, and CPE-1961 is not what to revert

Measured, not assumed. `git show 8c9ddb60:crates/server/src/transfer.rs` — the merge base of PR #1089 —
carries the identical silent-`Ok`, byte for byte. The behaviour is **CPE-1709 / CPE-1881's standing
contract** for this leg: a link verdict is *"not writing is the correct, safe outcome"*
(`DownloadReport::skipped`'s own doc), i.e. neither delivered nor a delivery failure. It has produced
exactly this `skipped` + `Ok` for a link found at **claim** time since long before CPE-1961.

What CPE-1961 changed is only that a link found at **commit** time now answers the same way a link
found at **claim** time always did — the two moments stopped disagreeing. It did not create the silent
`Ok`, and fencing it off was the right call for that PR's scope.

## The actual question

Is *"we refused to write because the path was booby-trapped"* a **skip** or a **delivery failure**?

Arguments both ways, and neither is obviously right, which is why this is its own ticket rather than a
line in someone else's:

- **Skip** (today): the refusal is the safe and correct outcome; a caller that treats it as an error
  learns nothing it can act on, and a tree with one hostile component would fail wholesale.
- **Failure**: the user named a file and does not have it. A programmatic caller — a sync, a restore, a
  scripted mirror — that reads `Ok` will proceed as though the tree is complete. That is the failure
  mode worth pricing: not a person reading a panel, but code trusting a verdict.

A third answer is likely the real one: keep `Ok`, but make the *shape* of the report force the caller
to look — e.g. `DownloadReport::is_complete()`, or splitting `skipped` into "we chose not to" and "we
were prevented", so a caller cannot ignore the second by accident.

## Sites to read first

- `crates/server/src/transfer.rs` — the `record!` fork and the long note above it (the bucket table:
  `policy: false → undelivered → Err`, `policy: true → skipped → Ok`).
- `crates/server/src/transfer.rs:595` and `:19` — `undelivered`'s own doc, and the CPE-1881 history of
  *"the delivered count silently one lower"*, which is the same defect one level down and was fixed.
- `crates/server/src/archive.rs` — the sibling leg, for contrast: it returns `Ok(report)` with counted
  skips **either** way, because its report shape is different. Whatever is decided here should say
  explicitly whether the two legs are meant to agree in consequence or only in classification.

## Definition of done

- A decision, written down at the call site with its reasoning — including if the decision is "keep it,
  this is correct", which is a legitimate outcome and is better recorded than re-litigated.
- If the verdict changes: every in-tree caller of `download_tree` audited for what it does with a
  now-`Err`, and the UI told apart "refused for your safety" from "could not write".
- A test that drives a real planted link through `download_tree` and asserts the caller-visible verdict,
  whichever way it lands — there is no such leg-level test today.

## Notes

- CPE-1961's round 6 added `archive::tests::cpe_1961_a_link_planted_mid_write_skips_that_entry_and_writes_its_neighbours`,
  which plants a real link mid-write on the **archive** leg by polling for the `.cpe-tmp` staging
  sibling rather than by sleeping. That is a working recipe for the `download_tree` fixture this ticket
  needs; it is Unix-only, because NTFS refuses to rename a directory with our staging handle open
  inside it.
