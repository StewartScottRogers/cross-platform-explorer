---
id: CPE-1815
title: the trash staging probe collapses six failure causes into one bare false, so a red says nothing about why
type: task
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-20
closed:
---

## Problem

`trash_roundtrip_available()` in `src-tauri/src/lib.rs` answers a single `bool`, but **six distinct things
can make it false.** So when CPE-1806's new strictness turns a staging failure into a loud CI panic, the
panic will say *that* staging failed and nothing about *which step*.

## Why it matters

CPE-1806 changed a silent skip into a failure precisely so a Linux staging problem cannot hide behind a
green tick. That is the right trade — but the failure it now produces lands on a runner **nobody can log
into**, for a probe with six candidate causes, on the one platform this crew cannot reproduce locally.

The likely outcome is a red CI leg that someone can only diagnose by adding instrumentation and pushing
again, repeatedly. A guard that fires usefully but reports uselessly still costs a morning.

## What to do

- Make the probe return **which step failed**, not just that one did. A small enum or a `Result<(), &'static str>`
  is enough — this does not want a new error type.
- Thread the reason into the message `require_staged` panics with, so the CI log names it.
- **Do not** make the probe do more work to produce the detail. If a cause is expensive to distinguish, say
  so and leave it merged with a note — the goal is a legible red, not an exhaustive taxonomy.
- Check the sibling probes routed through `require_staged` for the same shape; if they also collapse
  several causes, apply the same treatment or explain why they do not need it.

## Notes

Filed by the Foreman from the independent review of PR #961, 2026-08-20, which flagged it as non-blocking
and out of that PR's scope — correctly, since CPE-1806's job was to stop the skip being silent, not to
explain it.

Worth doing **before** the first Linux red rather than after, since the whole point of the change is that
such a red is now possible.

Related: **CPE-1806** (the strictness that makes this reachable), **CPE-1717** (`require_staged`),
**CPE-1724** (the batched routing of the remaining staging mechanisms).
