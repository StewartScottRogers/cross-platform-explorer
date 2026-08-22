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

**2026-08-21 (later)** — Independent Reviewer BLOCKED PR #986 with two findings. Both addressed.

- **Blocker 1 (zero coverage on the six/seven reasons; the "distinct reasons" test was tautological).**
  The reviewer showed two mutations that gutted the fix in substance while every existing gate stayed
  green: (a) collapsing two reasons to the same text (leaving four distinct), and (b) collapsing ALL
  reasons to `"staging failed"` — restoring the pre-fix bare fact. Fixed by lifting the reasons out of
  inline string literals into a `const TRASH_ROUNDTRIP_REASONS: [&str; 7]` in `src-tauri/src/lib.rs`,
  indexed from `trash_roundtrip_available`'s call sites, plus two new tests that hold BOTH halves of the
  fix: `trash_roundtrip_reasons_are_pairwise_distinct` (the array's values are genuinely different) and
  `trash_roundtrip_available_indexes_every_reason_exactly_once` (a source-scan of the function's own
  body via `include_str!`, in the same family as `half_applied_rename_guards_are_rejected`, proving
  every slot is actually wired to its call site and not just declared). Removed the tautological
  `fsutil::cpe_1815_distinct_probe_steps_produce_distinct_reasons` test (it fed `staged_fail_reason` —
  an identity function on its `Err` arm — two string literals and asserted they differed, which is true
  by construction and says nothing about the real probe); kept only its non-trivial part (the
  `Ok(())`-under-sabotage fallback-text check) as its own test. Re-ran both of the reviewer's exact
  mutations against the new code: PARTIAL COLLAPSE (two `TRASH_ROUNDTRIP_REASONS` entries made equal)
  reds `trash_roundtrip_reasons_are_pairwise_distinct`; TOTAL COLLAPSE (all seven set to `"staging
  failed"`) reds the same test. Also checked the OTHER half explicitly: pointing a call site at the
  wrong array index while leaving the array itself distinct reds
  `trash_roundtrip_available_indexes_every_reason_exactly_once` instead — confirming the two tests
  together cover both ways the fix can be gutted, not just the one the reviewer demonstrated.
- **Blocker 2 (the `list()` collapse's cost justification was false).** The PR body, this Work Log, and
  the doc comment all claimed splitting "list() errored" from "list() succeeded but the probe isn't in
  it" needed a second probe call. It doesn't — both branches read the SAME `trash::os_limited::list()`
  result already being fetched; splitting is a `?` plus an `.ok_or` on data already in hand, not an
  extra syscall. Split into two of the seven `TRASH_ROUNDTRIP_REASONS` slots (indices 3 and 4). This is
  also the pair with the most different real-world diagnoses: `list()` erroring outright means the trash
  backend itself is broken; `list()` succeeding without the probe in it is the signature of
  `lock_real_trash`'s `XDG_DATA_HOME` redirect (CPE-1785) *half*-applying — delete lands in one trash
  directory, list reads another. Different owners, different fixes; merging them would have been exactly
  the collapse this ticket exists to undo. Total distinguishable causes is now **seven**, not six —
  ticket title undercounts by one now that the free split is done; not renaming the ticket file since
  folder location/`status:` is the authoritative field, not the title.
- **Correction to the sibling-probe survey** (the review corrected this, not a self-catch): the earlier
  Work Log entry treated `src-tauri/src/lib.rs`'s CPE-1705 `target + parent deny`
  (`victim.try_exists().is_err() && !victim.exists()`) and `crates/server/src/dispatch.rs`'s
  `deny_read` (`std::fs::read(path).is_err() && std::fs::metadata(path).is_ok()`) as the same kind of
  legitimate two-probes-as-one-fact collapse. They are NOT the same. The `lib.rs` one genuinely does
  hold as one fact, and more strongly than originally argued: `try_exists().is_err()` logically implies
  `!exists()` (a path that errors on `try_exists` cannot simultaneously report `exists() == true`), so
  the second conjunct is redundant and there is truly only one condition being checked. `dispatch.rs`'s
  `deny_read` does NOT hold the same way: `read(path).is_err()` and `metadata(path).is_ok()` are two
  INDEPENDENT syscalls with independent outcomes and different diagnoses — `read` succeeding means the
  deny did not bind; `metadata` failing means the fixture itself is gone (a different bug entirely from
  a deny that never took effect). This is a real, if smaller, instance of the shape CPE-1815 exists to
  fix. **Not corrected in code** — `dispatch.rs` is out of scope for this ticket (CPE-1815 targeted
  `trash_roundtrip_available` specifically); worth its own follow-up ticket if/when that leg's failure
  mode needs to be diagnosable.
- **Two non-blocking fold-ins, done because they were cheap and in the same files:**
  - `require_staged_reason`'s `StagingVerdict::LegitimateSkip` path previously discarded the reason
    silently. That path is the one Windows actually hits (per `trash_roundtrip_available`'s own doc
    comment: five `CPE-1268` notices per measured CI run, zero on Linux) — the one platform observed to
    fail was getting no diagnosis at all. All 9 `trash_roundtrip_available` call sites in `lib.rs` now
    bind the probe `Result` to a local (`let trash_staged = trash_roundtrip_available(&trash_guard);`)
    and thread `trash_staged.err()` into their `skip_notice!` text as `(cause: {})`, so the Windows skip
    notice now names the step too, not just the Linux panic.
  - `staging_failure_message_with_reason` used to append `"Which step failed: {reason}"` AFTER the
    ~14-line boilerplate, which sits outside the window CI's guard step excerpts with
    `grep -m1 -A6 'CPE-1717'` (`.github/workflows/ci.yml:279,302,328`) once the panic hook's own
    "run with RUST_BACKTRACE=1" line is counted. Restructured so the reason is woven into the SAME first
    line as `[CPE-1717]` (`... could not stage its condition on {os} (failing step: {reason}), a
    platform where...`), which `grep -m1` always captures since it's the matched line itself. Added a
    positional assertion (`msg.lines().next()` must contain both `CPE-1717` and the reason) to
    `cpe_1815_the_failure_message_with_reason_names_which_step_failed` so a regression back to
    end-appending reds.
  - Added a doc-comment caution on `require_staged_reason`: the reason is an arbitrary `&str`, not
    `'static`-restricted, so a future caller could interpolate a runtime value (e.g. a filesystem path)
    into a message that reaches a public CI log. Every reason today is a `'static` literal — prevention,
    not a live defect.
  - `.github/workflows/ci.yml` still said `require_staged("trash_roundtrip", ..)` and "7 call sites" —
    stale even before this ticket (CPE-1770 made it 9), and this ticket both renamed the function on
    that exact path and touched all nine call sites, so it should have been caught here. Updated both
    the function name and the site count (7 → 9) in the two comments that named them, plus a third
    nearby "seven tests" mention.
- Gates re-run after all of the above: `crates/server` clippy clean, `cargo test` 2289 passed / 0 failed
  / 4 ignored (unchanged — one tautological test removed, one non-trivial one added, net zero). `src-tauri`
  clippy clean in both feature modes. `src-tauri cargo test`: 214 passed / 0 failed (default, +2 for the
  two new guard tests), 269 passed / 0 failed (`sidecar-platform`, +2). See PR body for the exact
  mutation-red transcripts.
