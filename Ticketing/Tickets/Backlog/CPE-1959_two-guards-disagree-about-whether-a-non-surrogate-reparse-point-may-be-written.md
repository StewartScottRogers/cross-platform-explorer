---
id: CPE-1959
title: two guards disagree about whether a non-surrogate reparse point may be written — `fsutil` writes it, `batch_media` refuses it, and only the refusal is now pinned by a test
type: task
priority: Medium
status: Open
tags: ready
estimate: S
created: 2026-08-27
---

## Summary

After PR #1066 (CPE-1929) the codebase states **opposite doctrines** about the same input class:

- **`fsutil::overwrite_confirmed_no_follow` writes** a non-surrogate reparse point. That is CPE-1896's
  rule: a dehydrated cloud placeholder (OneDrive, dedup, WOF) is an ordinary file the user expects to
  be written, and refusing it was the regression CPE-1896 removed.
- **`batch_media::open_output_verified` refuses** it, on the bare `FILE_ATTRIBUTE_REPARSE_POINT` bit —
  and PR #1066's new test now **cements** that verdict.

**This is not a regression.** `batch_media` refused non-surrogates before CPE-1929 too; the reorder
changed which check says so, not the verdict. Both PR #1066's Reviewer and its Security Auditor
confirmed that independently, and the split is documented at the `batch_media` site as **deliberately
unresolved**.

## Why it needs a ticket of its own

PR #1066's site comment points at **CPE-1958** as the place to revisit this. CPE-1958 is scoped to the
`links > 1` TOCTOU race at a *neighbouring* guard — a different problem. Its Reviewer flagged the
mismatch:

> Everything else this ticket deferred got a ticket that *owns* it, with acceptance criteria
> (CPE-1957). The doctrine question gets a pointer at a ticket about something else, so a worker
> arriving via CPE-1958 is working the race and may never read this.

So this ticket exists to own the question.

## What the site comment already establishes, and what it does not

**Established:** the split is real, both halves are named, it is non-regressive, and there is one
substantive asymmetry — *a refused batch item is **skipped** and the user keeps their input, whereas a
refused restore has failed at its only job.* That argument is correctly labelled an argument.

**Not established, and this is the crux, in the comment's own words:** nobody has asked a user whose
batch output landed on a cloud placeholder what they expected to happen.

## Acceptance criteria

- [x] **Decide which doctrine is right for `batch_media`, and record the reasoning**, not just the
      outcome. The asymmetry above is the starting argument; it is not obviously decisive.
- [x] **Get the missing evidence, or state plainly that it was not gettable.** What does a user with a
      dehydrated placeholder at a batch-media output path actually experience today — a skipped item
      with a link-shaped message they cannot act on? That is answerable from the code and the message
      text without asking anyone, and it is the input the comment says is missing.
- [x] If `batch_media` should match `fsutil`, narrow it to `reparse_name_surrogate` **and update the
      test PR #1066 added**, which currently pins the opposite. Do not weaken the test into vagueness;
      replace it with one that pins the new verdict just as hard.
- [x] ~~If the split is right, say so at both sites~~ — the split is **not** right; it is closed. The
      "unresolved" framing is deleted at both `fsutil` sites and replaced with "settled, and here is
      where".
- [x] Either way, **fix the follow-up hook**: the `batch_media` comment should point here, not at
      CPE-1958.
- [x] Check whether any **third** site takes a position on this class. PR #1066 enumerated all seven
      `handle_facts` call sites; use that enumeration rather than recalling (CPE-1932).

## Work log — 2026-08-28

**Verdict: `batch_media` was wrong, and it now calls `reparse_name_surrogate` like every other write
path in the crate.** The bare-bit refusal is gone. The reasoning is at the site; the three headline
inputs:

1. **The mechanism the refusal defended stopped existing at CPE-1961.** Its stated property was "a batch
   never writes *through* one". `VerifiedOutput::write_all` hands the handle to
   `fsutil::stage_bytes_over_checked_handle`, whose own doc says it is "the same three steps
   `overwrite_confirmed_no_follow` performs after its own checks, in the same order, for the same
   reasons; only the checks in front differ". A placeholder is **replaced by name**, so CPE-1896's three
   unresolved worries about writing through a `FILE_OPEN_REPARSE_POINT` handle are unreachable here.
   This ticket removed the last difference in the checks in front.
2. **The asymmetry argument does not survive being measured.** Derived end to end rather than assumed:
   `execute_one` → `open_output_verified` → `Err(…)` → `execute_plan_walk` pushes `(input, reason)` into
   `BatchReport::skipped` → `App.svelte` renders `notice.convertedWithSkipped` =
   `"{written} converted, {failed} skipped: first \"{name}\" — {reason}"`, which shows **`skipped[0]`
   only**. So a user whose synced folder's outputs OneDrive has dehydrated — which on a re-run is *every*
   previous output — got a count and exactly one sentence, and until today that sentence was:

   > refusing at write time: "…\out.png" is a link or other reparse point (a symlink, a junction, or any
   > name that can stand in for another), and a batch never writes through one — such a name's target can
   > be re-pointed after any check, even a dangling link that happens to point back inside this same
   > folder. Nothing was written for this file

   For a placeholder every noun in that is false and no action is named. The cost of over-refusing was
   never "one item"; it was systematic, and invisible past the first row.
3. **CPE-1957's unprivileged-plantable finding cuts *toward* narrowing.** Anyone who can plant a
   non-surrogate tag can equally plant a surrogate one, which both doctrines refuse. All the bare bit
   denied was a tag that by definition does not redirect the name, on a name about to be replaced by
   rename inside an already-contained folder — i.e. it was an attacker-**triggerable** refusal (a
   denial-of-service on the batch item), not an attacker-blocking one.

**Per-site verdicts, derived at run time (CPE-1932), not from PR #1066's "seven".** Enumerating
`handle_facts(` in `crates/`+`src-tauri/`+`sidecar/` and splitting at each file's `mod tests` gives
**nine production call sites**, not seven — `fsutil.rs:4237`/`4335` (CPE-1961/1963's staged/reopened
identity reads) and `vault_manager.rs:1930` post-date that count. Of the nine, exactly **four** consult
`is_reparse_point` at all (derived by grepping the field, same split):

| site | position on this class | verdict |
|---|---|---|
| `batch_media::open_output_verified` | was bare bit → **now `reparse_name_surrogate`** | **changed here** |
| `fsutil::copy_file_onto_destination_handle` (2252) | narrow since CPE-1896 | fine; note added that the split is closed |
| `fsutil::overwrite_confirmed_no_follow` (3832) | narrow since CPE-1929 | fine; **its docblock claimed the opposite** — fixed |
| `vault_manager::overwrite_pinned_file` (1942) | bare bit; **CPE-1957 / PR #1101 narrows it** | not touched — that PR owns it |
| `backup.rs:296`, `fsutil.rs:2752`, `3103`, `4237`, `4335` | use `.id`/`links` only | no position; nothing to do |

A fifth consumer of the rule, `open_beneath::sys::name_surrogate_at`, is already narrow (CPE-1938).

**A false provenance claim found and fixed (CPE-1933).** `overwrite_confirmed_no_follow`'s docblock said
*"This site refuses a reparse point on the **bare bit**"*. It has narrowed since CPE-1929 and a green
test pins that it does. Prose about a check thirty lines away, with the suite reading as though it
vouched for it.

**Sabotages — three, all red, Windows 11 / NTFS, `cargo test -p cpe-server --lib`.** Baseline
**2,458 passed / 2 failed / 14 ignored** (the two are `ticket_board::…nearest_project_root…` and
`archive::…cpe1774…`, environmental to running from a worktree nested in the repo, red before and after);
the count did not move because this ticket replaced one test with one test. Disable → **2,457 / 3**;
predicate forced to lie → **2,457 / 3**; un-narrow to the bare bit → **2,457 / 3**. The extra failure is
the new test each time.

**And a correction worth more than the result.** Round 1 built the surrogate half of the two-halves
fixture from a GUID surrogate tag, asserted the handle refusal's `"stands in for another name"` wording,
and passed — *and went on passing with the handle refusal disabled*, i.e. a green sabotage that read as
safety. `classify_reparse_tag` sends an **unrecognised** surrogate to the path-side `WHY_SURROGATE_TAG`
before the open, and that constant contains the same phrase. So the one-bit-apart GUID pair cannot reach
the narrowed handle check at all. The test now has a **third leg** — a real symlink, the tag
`classify_reparse_tag` passes through as `Probe::Link` — asserting on `"(a symlink, a junction, or a
mount point)"`, which belongs to the handle refusal alone. That leg is what makes the disable sabotage
red.

**The test asserts filesystem state, never `Ok`** (CPE-1957's lesson). Half 1 drives the whole write and
reads the bytes back, plus asserts the reparse bit is **gone** — which is the staged replace, and the
direct evidence for reason 1 above. Half 2 asserts the refused object still carries its reparse bit and
its `$DATA` length (it cannot be `fs::read` — a driverless surrogate answers `ERROR_CANT_ACCESS_FILE`
(1920), measured). Half 3 asserts the link is still a link.

**Also corrected, because the decision inverts them:** `batch_execute::execute_one`'s step 4 still said
"Truncate + write **through the handle from step 2**", and its CPE-1725 note still said the bytes go
through that handle "plus the handle's reparse bit". Both false since CPE-1961/CPE-1959 respectively.
`src/docs/explorer-batch-media.md` gains a user-facing bullet and loses the same "writes through it".

`clippy --locked --all-targets -D warnings` clean in both feature modes (default and `--features index`).

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1066's Reviewer (round 2), which called the follow-up
hook "weaker than the rest of the PR's own standard" while approving.

Related: **CPE-1896** (the dehydrated-placeholder rule this rests on), **CPE-1929** (the reorder that
surfaced the split, PR #1066), **CPE-1957** (the three shadowed sites left unmeasured — the same guard
family), **CPE-1958** (the TOCTOU race at the neighbouring guard, which this is *not*).
