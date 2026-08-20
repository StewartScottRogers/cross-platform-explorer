---
id: CPE-1775
title: A refused archive entry is invisible — the toast still says "extracted" and the count is quietly lower
type: bug
priority: High
status: Done
tags: ready
estimate: M
created: 2026-08-17
closed:
---

## Problem

Found independently by both the Security Auditor and the UAT tester on PR #926 (CPE-1758), tracing what a
person actually sees when an archive entry is refused.

The backend does the right thing. `extract_zip_archive_stream` records
`"{name}: unsafe entry name, skipped"` into `ArchiveReport::errors`, which reaches `TransferReport.errors`
(`src-tauri/src/lib.rs:2883`) and the `transfer://done` event.

The frontend then throws it away in the common case. `src/App.svelte:6167`:

```js
if (!r.cancelled && r.failed === 0) { showNotice("N item(s) extracted"); }
```

and the errors are only surfaced when `r.failed > 0` (`App.svelte:6176`). **A skipped entry increments
neither `failed` nor `skipped`.** So the user gets:

- a success toast — *"1 item extracted"*
- a count that is quietly lower than the archive's contents
- the real message reachable only by hovering a small `· N error` annotation in the operations panel
  (`TransferPanel.svelte:52`), which they have no reason to open

That is the same user experience as the bug CPE-1758 was written to fix: **extraction succeeded, the file
is not there, nothing says why.** The mechanism changed from "written to an invisible place" to "refused
and not mentioned". The outcome did not.

## Why this is High

1. **It converts a security refusal into a silent failure.** A guard the user cannot observe cannot be
   acted on. They will not know to go back to the sender, or that the archive was hostile at all.
2. **CPE-1758 sharply increased how often it fires.** Colons, reserved device names, trailing dot and space
   are now refused where they previously extracted. This path went from rare to routine without the UI
   changing.
3. **It enables a partial-extraction deception.** An archive with a benign-named malicious entry A and a
   refused entry B extracts A, drops B, and reports success. The user believes they received both.
4. Its own documentation over-promised: the PR cited `extract_plan::plan_extract`'s `skipped_unsafe` as
   *"the plan the user reviews before extracting"*. Both legs independently grepped for it — **zero callers
   anywhere in `src-tauri/` or `src/`.** There is no plan-review UI. That claim was corrected in #926, and
   this ticket is what actually has to exist for it to become true.

## What to do

- Make a refused entry **visible in the primary notice**, not only in a tooltip. The headline should say
  something happened, e.g. *"3 items extracted · 2 entries skipped"*, with the reasons one click away.
- Introduce the count the UI is missing. `failed` is wrong (nothing failed) and reusing it would misreport
  a genuine failure. A distinct `skipped` — carried from `ArchiveReport` through `TransferReport` to the
  event — is the honest shape. Check whether `ArchiveReport` already has one and it is simply not
  populated for this case.
- **Say why, in the user's terms.** "unsafe entry name" is developer language. The user needs to know the
  archive contained an entry that could not be written safely, and which one.
- Apply it to every skip reason, not only unsafe names — traversal skips are documented as **silent** today
  too (`explorer-archives.md`), and they have exactly the same problem.
- While in here: the one-shot sinks (`extract_archive` at `archive.rs:1090`, `extract_zip_encrypted`,
  `extract_7z_safe` at `:1226`) `continue`/`return Ok(true)` with **no record at all**. They have no live
  frontend caller, but they should not be the quiet ones if they ever get one.

## Acceptance criteria

- [ ] Extracting an archive with one refused entry produces a notice that states, without hovering
      anything, that an entry was skipped and how many.
- [ ] The reason and the entry name are reachable in one obvious step, in language a non-developer
      understands.
- [ ] A genuine failure and a skip are distinguishable in the UI — a skip must not be reported as a failure
      or vice versa.
- [ ] Traversal skips and unsafe-name skips are both surfaced; neither stays silent.
- [ ] An extraction with nothing skipped is **unchanged** — no new noise on the normal path.
- [ ] A test asserts the count and the message that reach the frontend, so a future refactor cannot quietly
      drop them again. Assert on the event payload, not on a helper's return value.
- [ ] `explorer-archives.md` describes what the user will now see, and the "silently skipped" wording is
      corrected.

## Notes

Found by the Security Auditor and UAT independently on **PR #926 / CPE-1758**, 2026-08-17, during the
batched sprint. Related: CPE-1758 (which widened how often this fires), CPE-1773 (tar, where the skip does
not even happen yet), CPE-1774, CPE-1055 / `extract_plan` (the unwired plan preview).
