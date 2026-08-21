---
id: CPE-1837
title: a refused link entry vanishes from a one-shot extraction with no note to the user
type: bug
priority: Medium
status: Backlog
tags: ready
estimate: M
created: 2026-08-20
closed:
---

## Problem

`extract_zip_encrypted` and the other **one-shot** extraction entry points return a bare `Result` with
nowhere to put a per-entry notice. When the shared loop refuses an entry — a link whose target escapes
the extraction folder, say — the entry is skipped and the extraction returns `Ok`. The user is told the
archive extracted. The file is simply not there.

The **streamed** variants do not have this problem: they carry an `ArchiveReport` with `skipped` and
`errors`, so the same refusal surfaces as
`errors: ["evil_link: this entry is a link pointing at \"…\", which is outside the extraction folder …"]`.

Measured during the CPE-1807 review, same archive, same refusal, two entry points: streamed reports it,
one-shot does not.

CPE-1807 made this reachable on a path where it previously could not happen. Before that merge,
`extract_zip_encrypted` wrote every entry — including a link-flagged one — as an ordinary file, so
nothing ever vanished. After it, the path can refuse, and a refusal on a one-shot call is silent. The
merge was still right; this is the consequence it exposed, documented there and split out here.

## Why it matters

This is the "successful-looking operation, missing file" shape the archive module elsewhere treats as a
hazard, and the same shape as CPE-1803/1804/1805 in the Trash view and CPE-1816's partial listing: an
operation that reports success while quietly withholding part of the truth is worse than one that
fails, because it stops the user looking.

It is also the shape a malicious archive would rely on. An entry crafted to be refused disappears with
no trace visible to the person who extracted it.

## Acceptance criteria

- [ ] A one-shot extraction that refuses one or more entries surfaces that fact to the caller. Decide the
      mechanism and record why: return the same `ArchiveReport` the streamed variants already produce, or
      return a distinct `Ok`-with-warnings shape, or fail the extraction outright. Reusing the existing
      report type is the obvious first candidate — check what the callers in `src-tauri/src/lib.rs` do
      with the current `Result` before assuming it is free.
- [ ] Every one-shot entry point is covered, not only `extract_zip_encrypted`. Enumerate them first and
      list them in the work log; a partial sweep presented as complete is this repo's most-repeated defect.
- [ ] The user-facing surface actually shows it. A report nobody renders is the same silence with more
      steps — check the Tauri command and the frontend, and make the notice visible the way the Trash
      view's degraded notice is.
- [ ] Tests pin that a refused entry produces a visible notice on the one-shot path, red-proofed by
      removing the notice and observing the test fail.
- [ ] Wording follows the precedent already in this module: name the entry, say what was refused and why,
      and say the rest of the archive still extracted.

## Notes

Filed by the Foreman from the CPE-1807 review. The doc comment added at `crates/server/src/archive.rs`
around `:2428` refers to this ticket.

Related, same "reports success while withholding the truth" family: CPE-1803, CPE-1804, CPE-1805 (Trash
degraded listings), CPE-1816 (partial listing rendered as complete). The wording and mechanism choices
made there are the precedent to follow rather than re-derive.
