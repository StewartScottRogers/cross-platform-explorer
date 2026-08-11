---
id: CPE-1623
title: "Batch Media's rename template accepts path separators, so `..\\..\\elsewhere\\name` silently overwrites an arbitrary file — from the normal UI, in \"non-destructive\" mode, with no confirmation"
type: Bug
status: Backlog
priority: High
component: Backend
tags: [ready]
created: 2026-08-11
closed:
---

## Why
Found by the independent Security Auditor while threat-modelling CPE-1613 (PR #818). **Not introduced by
that PR** — the flaw predates it (CPE-940 / CPE-723) — but CPE-1613's work disproves the module's own
claim that `plan()` "keeps output non-destructive" and that the engine "refuses" destructive writes. It
is filed High because it was **demonstrated end-to-end on a real Windows box** and is reachable by an
ordinary user typing into an ordinary text field.

## The bug
`plan()`'s rename path substitutes the user's template into the output stem with **zero sanitisation**:

- `crates/server/src/batch_media.rs:305-309` — `template.replace("{stem}", &stem).replace("{n}", ...).replace("{ext}", ...)`.
  Nothing rejects `/`, `\`, or `..`.
- `validate()` (`batch_media.rs:113-138`) only checks the template is **non-empty**.
- `join(&dir, &out_stem, &ext)` (`batch_media.rs:151-157`) is plain string concatenation — no lexical containment.
- `plan()`'s non-destructive collision guard (`batch_media.rs:332-343`) compares the computed output only
  against `used` — *this batch's own inputs and outputs* — via `same_file`. It never checks whether the
  computed output already exists on disk as an unrelated file outside the batch.
- `execute_one` (`batch_execute.rs:96-105`) then performs an unconditional `fs::write(&item.output, ...)`.

## Demonstrated reproduction (actually run, not hypothesised)
Input `traversal_workdir\innocuous.jpg` (a real, valid JPEG). Job: `MediaOp::Rename { template:
"..\\..\\cpe1613_traversal_victim\\important" }`, `non_destructive: true` — the **default, supposedly safe**
mode ("write to new files" checked). An unrelated pre-existing file sat at
`...\cpe1613_traversal_victim\important.jpg` containing `"VICTIM ORIGINAL CONTENT - must not be touched"`.

Result: `plan()` computed an output that `std::fs::canonicalize` resolved to a path **outside the input's
own directory**. `any_in_place(&items)` returned `false` — correctly, because they genuinely *are*
different files, so `same_file` is not even wrong here. `execute_plan(&items, &job)` returned
`Ok(BatchReport { written: 1, skipped: [] })` with **no error and no refusal**, and reading the victim file
back showed its contents replaced with re-encoded JPEG bytes. The original was gone.

## Why it matters
- **Reachable from the shipped UI, not just devtools.** `src/lib/components/BatchMediaDialog.svelte:451` is
  a bare `<input bind:value={renameTemplate}>` with no character restriction; the only frontend check is
  "non-empty" (`BatchMediaDialog.svelte:116-117`).
- **The confirmation never fires.** Because input and output are genuinely different files,
  `overwritesInPlace` / `any_in_place` never trigger, so the "Overwrite N files" dialog never appears. The
  write proceeds as an ordinary, un-flagged "Apply".
- The entire `confirmed_overwrite` guard machinery (CPE-1590/1599/1613) is scoped to *"does this collide
  with the batch's own input/output"* — **never** to *"does this stay inside the folder the user picked"*.

## Fix
1. **Constrain every computed output to the selected directory.** Lexically normalise the joined path and
   reject (or refuse the whole batch) if it does not remain under the batch's target dir. This is the real
   fix — do it in `plan()` so both the collect and streaming paths inherit it.
2. **Reject path separators and `..` in the rename template** at `validate()`, with a clear user-facing
   error, and mirror the restriction in `BatchMediaDialog.svelte` so the user is told before they click.
3. **Extend the destructive check beyond the batch's own set:** an output that resolves onto an existing
   file *not* part of this batch must be treated as an overwrite — requiring confirmation, and refused by
   the engine when unconfirmed, exactly as CPE-1599 established.
4. Correct the module docs' claim about `plan()` being non-destructive once the above holds.

## Acceptance criteria
- The demonstrated reproduction above **fails to escape**: the batch is refused (or the output is contained),
  the victim file is byte-for-byte untouched, and the user is told why.
- A test reproduces the traversal against the **old** code (negative control) and shows it blocked on the new.
- Ordinary rename templates (no separators) are unaffected — no new false alarms.
- An output landing on an existing unrelated file requires confirmation and is engine-refused without it.
- Frontend and engine agree on what a valid template is; the engine remains the enforcement point.

**Conflict surface:** `crates/server/src/batch_media.rs`, `crates/server/src/batch_execute.rs`,
`src/lib/components/BatchMediaDialog.svelte`, `src/lib/batchMedia.ts`, plus their tests.

## Notes
Ranked by the auditor as by far the most severe of three findings; the other two are filed as CPE-1624.
