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

## Work Log

**2026-08-21** — Implemented.

- `trash_roundtrip_available()` (`src-tauri/src/lib.rs`) now returns `Result<(), &'static str>`
  instead of `bool`. Five sequential OS calls, six ways to come back `Err`: tempdir creation, the
  probe write, `trash::delete`, "listed back out of the trash" (a failed `list()` call and a
  successful one that just doesn't contain the probe are merged into this one cause — both mean
  "the delete didn't land somewhere `list()` can see", and telling them apart needs a second probe
  this function doesn't otherwise need; CPE-1815 explicitly didn't want the probe doing more work
  to produce detail), `restore_all`, and the final "is the file actually back" check.
- Added `cpe_server::fsutil::require_staged_reason(mechanism, supported_here, staged: Result<(), &str>)`
  alongside the existing `require_staged`, rather than changing `require_staged`'s signature — the
  bare-`bool` form has ~40 other call sites across `src-tauri` and every `crates/*` network crate,
  none of which have anything richer than "did this single symlink/deny probe succeed" to report.
  The new panic path folds the `Err` reason into the existing CPE-1717 panic message via a new
  `staging_failure_message_with_reason(mechanism, reason)`, both pure and unit-tested.
- All 9 call sites of `trash_roundtrip_available` switched from `require_staged` to
  `require_staged_reason` (mechanical rename, same "trash_roundtrip" mechanism string, no other
  logic change).
- Design question answered in the PR body: none of the 9 callers branch on *which* cause failed —
  they all do the same thing (print a generic skip notice, return) regardless. Under CI
  (`staging_is_strict()` true), the caller never even sees the `bool`/`Result` — `require_staged`/
  `require_staged_reason` panics first. So the only real "consumer" of the six-way distinction is a
  human reading that panic in the CI log, which is exactly where CPE-1815 threads it.
- Checked sibling `require_staged` call sites for the same six-cause shape (Notes below in the PR).
  Found two two-condition `&&` collapses (`src-tauri/src/lib.rs`'s CPE-1705 target+parent deny, and
  `crates/server/src/dispatch.rs`'s `deny_read`) that are a different kind of collapse — two
  redundant checks of ONE staging step (a deny verified two ways), already deliberately merged and
  documented as "one fact" by their own PR reviews, not several sequential steps with different
  diagnoses. Left those as-is; did not extend `require_staged_reason` to them.
- Gates: `cargo clippy -p cpe-server --all-targets -- -D warnings` clean (0 warnings). `cargo test`
  in `crates/server`: 2289 passed, 0 failed, 4 ignored (pre-existing, unrelated to this change).
  `src-tauri` `cargo clippy --all-targets -- -D warnings` clean in both feature modes (default and
  `--features sidecar-platform`). `src-tauri cargo test`: 212 passed / 0 failed (default), 267
  passed / 0 failed (`sidecar-platform`).
- Red-proofed both new `fsutil` unit tests by reverting the fix one line at a time, observing the
  assertion fail on the actual harm (not a message-shape decoy), then reverting back to green — see
  PR body for the exact lines.
- Live sanity check on this Windows dev machine: `CPE_STAGING_STRICT=1 cargo test
  list_trash_then_restore_trash_items_round_trips_a_probe_file` still passes (this machine's real
  Recycle Bin genuinely round-trips), confirming the refactor didn't change the probe's real
  behaviour, only its failure reporting.
