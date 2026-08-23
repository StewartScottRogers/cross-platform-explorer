---
id: CPE-1817
title: the skip-visibility guard covers two of seven sites while reading as if it covers the mechanism
type: task
priority: Medium
status: Doing
tags: ready
estimate: S
created: 2026-08-20
closed:
---

## Problem

CPE-1806 routed all **seven** `trash_roundtrip_available()` call sites through `require_staged`, so a test
that cannot stage its precondition now fails loudly instead of skipping into a green log. It then added a CI
guard so that routing cannot be silently deleted again.

**The guard covers 2 of the 7 sites.** The surrounding comment at `.github/workflows/ci.yml:250-253` and
`:281-282` reads as covering the mechanism. The other five — including the second Linux-only test,
`restore_and_empty_trash_fail_loudly_instead_of_reporting_false_success_when_the_dependency_panics` —
remain deletable with CI green.

A second, narrower gap in the same block: the **Windows arm of block 2** (`ci.yml:309-312`) is itself
zero-match-vulnerable, because `grep ... || true` asserts nothing. Every scenario that could exploit it also
reds the Linux leg, and `fail-fast: false` guarantees that leg runs — so the exposure is real but covered.

## Why it matters

This is the third layer of the same problem and each layer was found by asking the same question. The test
could skip silently (CPE-1806). The guard against that could be deleted silently (CPE-1806's first review).
Now the guard's *coverage* is narrower than its own description.

None of these is a behaviour bug. Each is a claim that reads stronger than the evidence behind it — which is
the defect class this repo has spent a sprint learning to find.

The comment is the part that makes it worth fixing rather than noting: a future reader deleting the routing
from one of the five uncovered sites will read "the mechanism is guarded", see green, and be wrong.

## What to do

- Extend the guard from 2 sites to all 7, or **narrow the comment to say exactly which two are covered and
  why the other five are not**. Either is honest; the current pairing is not. Prefer extending — the
  asymmetric block already exists and the marginal cost per site is small.
- Assert the `CPE-1268` notice on the **Windows arm** so it cannot pass by matching nothing in isolation.
- Note the ordering constraint: the two Linux-only tests do not compile off Linux, so any added canary needs
  the same explicit per-OS skip the existing blocks use. **An implicit skip here would be the fourth layer
  of the joke.**

## Evidence

Per the Evidence Rules in `Ticketing/wiki.md`. Red-proof **per site**: delete the routing from each newly
covered site in turn and confirm the guard reds for that one. A single deletion proving a single red does not
establish coverage of seven.

## Notes

Filed by the Foreman from the round-2 re-review of PR #961, 2026-08-20, which approved the PR — this was
explicitly out of scope for the round it asked for.

Related: **CPE-1806** (the routing and the guard), **CPE-1717** (`require_staged`), **CPE-1724** (the batched
routing of the remaining staging mechanisms), **CPE-1815** (the probe's collapsed failure causes).

## Work Log

**2026-08-23 — widened the guard from 2 of 9 sites to all 9, fixed the Windows `|| true` gap.**

By the time this was picked up, `lib.rs` had grown from 7 `trash_roundtrip_available()` call sites (the
ticket's count at filing) to 9 — CPE-1770 added a second dangling-link pair after CPE-1806 landed, and the
`ci.yml` comment already said as much (`9 ... call sites ... CPE-1770 added an 8th and 9th after this
comment first said 7`). The **shape of the bug was unchanged**: the guard still only ever sabotage-tested
one site of each shape (2 of 9), so this ticket's fix targets "all 9", not "all 7" — the ticket's own
Evidence section asks to red-proof "each newly covered site", which is the operative instruction; the stale
"seven" in the title/Problem section is a snapshot, not a constraint.

Changes (`.github/workflows/ci.yml`, `skip-visibility guard (CPE-1717 / CPE-1806)` step, `backend` job):

- Replaced the two hand-written per-site blocks with two small bash functions
  (`assert_linux_only_trash_canary`, `assert_shared_trash_canary`) that encode the same pass/fail logic as
  before, then called once per site — 2 Linux-only-shape calls + 7 shared-shape calls, covering all 9.
- The shared-shape function's Windows arm now asserts `grep -q 'CPE-1268' "$log"` (was
  `grep -m1 'CPE-1268' "$log" || true`, which is documented in this ticket as asserting nothing) and reds
  with `::error::` if the notice is absent, rather than only logging it best-effort.
- Added a live drift guard: `grep -c 'require_staged_reason("trash_roundtrip"' src/lib.rs` compared against
  the number of sites this step actually sabotage-tests; a mismatch reds the step. This is the "prefer
  counting all seven sites over enumerating two" instruction, implemented as a real assertion rather than a
  comment promise — a 10th call site added later either gets a canary or fails this step on purpose.
- Updated the header comment to state what the step now covers (9 of 9) instead of the stale "2 of 9 while
  reading as if it covers the mechanism" shape this ticket exists to fix.

**Red-proof** (Evidence Rules, `Ticketing/wiki.md`): ran the guard's exact script locally
(`CPE_STAGING_SABOTAGE=1`, `CI=true` — `CI` is required to reproduce GitHub Actions' strict verdict locally;
without it `staging_is_strict()` is false and nothing ever reds) against the unmodified worktree first
(passed, `fail=0`), then deleted the routing (`if !require_staged_reason(...) { ...; return; }` →
`if false { ... }`) from one site at a time, rebuilt, reran the full guard, and reverted before the next
site. Confirmed RED, with only the tampered site's check firing and all others staying green, for:

- `macro_run_convert_step_then_undo_restores_the_original_bytes_via_trash` — new Windows CPE-1268 assertion
  fired (`passed under sabotage on Windows, but the log has no CPE-1268 notice`); drift check also fired
  (8 vs 9 sites).
- `restore_trash_items_reports_a_collision_as_a_distinguishable_per_item_error_without_aborting_the_batch`
  — same failure mode.
- `empty_trash_purges_only_the_selected_probe_item` — same failure mode.
- `list_trash_stream_flushes_batches_over_the_channel_and_matches_the_collect_variant` — same failure mode.
- `cpe_1770_restore_trash_items_refuses_when_a_dangling_link_occupies_the_original_path` — reds, but via a
  different, pre-existing `CPE-1717` panic from `make_dangling_link` failing to stage on this dev machine
  (no symlink privilege / Developer Mode) rather than the new CPE-1268 check; noted as an assumption below.
  Its twin, `cpe_1770_restore_from_trash_refuses_when_a_dangling_link_occupies_the_original_path`, is
  structurally identical (same `make_dangling_link` dependency) and not separately red-proofed.
- `restore_and_empty_trash_fail_loudly_instead_of_reporting_false_success_when_the_dependency_panics` — the
  Linux-only site the ticket names explicitly. This one doesn't compile on Windows
  (`#[cfg(target_os = "linux")]`) and this dev machine has no Linux toolchain (WSL has no `cc`; installing
  one is out of scope per the working rules' "no machine-global tooling" constraint, and a throwaway
  GitHub Actions run was judged unnecessary given the technique below). Proved instead by temporarily
  broadening *only this test's* `#[cfg]` to also compile on Windows (`any(target_os = "linux",
  target_os = "windows")`), deleting its routing the same way, running the guard with `RUNNER_OS=Linux`
  forced, and reverting both edits before rebuilding clean again. Result: the test panics at
  `trash_guard.scratch_path().expect("the Linux XDG_DATA_HOME redirect must be active")` — a deterministic,
  machine-independent consequence of running without the routing gate on a non-Linux target — and the guard
  correctly reds it (`red, but not with the require_staged_reason panic`). Never committed; `git checkout --
  src-tauri/src/lib.rs` after every mutation, confirmed clean via `git status --short` before proceeding.

Confirmed unmodified-tree green (`fail=0`) both before starting and again after the last revert.

**Assumption logged:** this machine's Windows dev account lacks symlink-creation privilege (no Developer
Mode / admin), which makes the pre-existing, unrelated `make_dangling_link` staging gate red under
`CI=true` regardless of this ticket's change (confirmed on the unmodified `cpe_1710_rename_entry_...` base
check too, which is not part of this ticket's scope). This is a local-environment fact, not a defect;
real CI runners have the privilege (per the existing comment: "if it still passes, a runner that quietly
lost symlink privilege ... would report green over zero coverage"). Documented rather than worked around,
per Evidence Rules.

YAML validity: `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))"` parses clean;
`bash -n` on the extracted step script has no syntax errors.
