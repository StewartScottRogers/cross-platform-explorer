---
id: CPE-1817
title: the skip-visibility guard covers two of seven sites while reading as if it covers the mechanism
type: task
priority: Medium
status: Done
tags: ready
estimate: S
created: 2026-08-20
closed: 2026-08-23
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

**2026-08-23 — round 2: gauntlet UAT FAIL, two findings in the new machinery itself, both fixed.**

Reviewer approved; UAT failed with two findings. Both are defects in the widening this ticket added, not
in the underlying `require_staged_reason` mechanism.

**Finding 1 (non-blocking per UAT, fixed anyway per Foreman's ruling) — the drift check false-fails on a
cosmetic reflow.** `grep -c 'require_staged_reason("trash_roundtrip"' src/lib.rs` requires the function
name and the string literal on the same physical line. Wrapping one call's args across several lines —
no rename, no logic change — dropped the count 9→8 and tripped `::error::CPE-1817: ... a call site was
added or removed`, which is false: nothing was added or removed. Fixed by collapsing whitespace (newlines
included) before counting: `tr -s '[:space:]' ' ' < src/lib.rs | grep -o 'require_staged_reason([[:space:]]*"trash_roundtrip"' | wc -l`.
Verified locally (grep only, no rebuild needed):
- Baseline: both old and new counting methods read 9.
- Reflowed one call site (`require_staged_reason(` / `"trash_roundtrip",` / `cfg!(...)` / `trash_staged,` /
  `)` each on their own line, `git diff -w` confirms no token changed): new method still reads 9 (no false
  trip); old method dropped to 8 (the bug, reproduced and confirmed before fixing). Reverted.
- Genuine deletion (one call's routing replaced with `if false`): new method correctly drops to 8.
- Genuine addition (a fabricated extra `require_staged_reason("trash_roundtrip", ...)` call injected):
  new method correctly rises to 10. Reverted.
Also softened the failure message: it no longer asserts "a call site was added or removed" as the sole
explanation — it says that is the most likely cause given the two counts have drifted, and names "the
counting pattern may itself need to change for a new call shape" as the alternative, rather than
overclaiming certainty about a cause the message can't actually verify.

**Finding 2 (blocking) — the pass check could be satisfied by an unrelated staging gate.**
`grep -q 'CPE-1717'` (bare substring) matches the panic prefix EVERY `require_staged`/`require_staged_reason`
call emits, not just `trash_roundtrip`'s. Sites 4 and 5 (`cpe_1770_restore_trash_items_refuses_...` /
`cpe_1770_restore_from_trash_refuses_...`) also call `cpe_server::fsutil::make_dangling_link`, which stages
independently via its own `require_staged("make_dangling_link", true, ...)`. Fixed by tightening both
matches (in `assert_linux_only_trash_canary` and `assert_shared_trash_canary`) to the mechanism-specific
text `` grep -qF 'CPE-1717] `trash_roundtrip`' ``, which only the trash_roundtrip mechanism's own panic
message contains.

**The two checkers' configurations contradicted each other; resolved with evidence, not a coin flip.**
Reproduced both, at site 4, with routing deleted (`if false`) and `make_dangling_link`'s own routing left
untouched, rebuilt, `cargo test cpe_1770_restore_trash_items_refuses_when_a_dangling_link_occupies_the_original_path`:

- **Config A — `CPE_STAGING_SABOTAGE=1 CI=true`** (this is what `.github/workflows/ci.yml`'s step
  ALWAYS sets — `env: CPE_STAGING_SABOTAGE: "1"` at the step level, and GitHub Actions always sets
  `CI=true` — so this is the config real CI actually runs under, always): test **FAILS** (exit 101),
  panicking at `make_dangling_link`'s own gate (`[CPE-1717] \`make_dangling_link\` could not stage its
  condition on windows...`), NOT trash_roundtrip's. Matches the reviewer's finding.
- **Config B — `CPE_STAGING_SABOTAGE` unset, `CI=true`**: test **PASSES** (exit 0) on this box — no
  panic at all, because `make_dangling_link_inner`'s privilege-free junction fallback genuinely succeeds
  here without sabotage forcing anything. Matches the UAT's finding in spirit (silent pass), though not
  byte-for-byte: the UAT's own box apparently could not even satisfy the junction fallback, so on their
  machine `make_dangling_link` panicked for a REAL reason instead of passing — either way the panic (real
  or sabotage-forced) carries the same mechanism-agnostic `[CPE-1717] \`make_dangling_link\`...` text.

Both are real and reproduce as described — they are not in conflict once separated by WHICH CODE PATH
each exercises, not by which config is "correct":
- On Windows, my `assert_shared_trash_canary`'s failing branch takes an unconditional
  `if RUNNER_OS = Windows: FALSE RED` path regardless of grep — so on a real Windows CI leg, this defect
  never actually produced a guard-green (any red at all is fail=1 there, just mislabeled). This is what a
  RUNNER_OS=Windows-labeled repro (the reviewer's) finds: FAIL.
- On Linux — where `supported_here = cfg!(target_os = "linux")` is genuinely true for `trash_roundtrip`,
  the actual case this whole mechanism exists to guard — the failing branch instead falls through to
  `elif grep -q 'CPE-1717'`, which (pre-fix) accepted ANY require_staged panic, including
  `make_dangling_link`'s. This is a REAL exposure on the real Linux CI leg, under the REAL Config A (since
  `CPE_STAGING_SABOTAGE=1` is always what CI sets): `make_dangling_link`'s `supported_here` argument is
  the literal `true` (not `cfg!(...)`, confirmed by reading `crates/server/src/fsutil.rs`), so it panics on
  ANY OS whenever sabotage is set and CI is strict — `staging_verdict`'s `staged && !sabotaged` branch is
  unreachable under sabotage regardless of whether the real construction would have succeeded. Proved this
  directly: took the real Config-A panic log from this Windows box (message text for the mechanism name is
  OS-independent) and ran the OLD grep against it with `RUNNER_OS=Linux` forced (matching the real Linux
  leg's branch) — it printed `OK: ... went red, and for the right reason` and returned 0: a confirmed
  guard-green on a genuine deletion, on the code path real Linux CI actually executes. Ran the NEW grep
  against the identical log: it correctly fell to the `else` branch and reported the mismatch. Then
  reproduced the same result through the actual (not hand-simulated) `assert_shared_trash_canary` function,
  `RUNNER_OS=Linux` forced, site 4 mutated, both Config A and Config B: both configurations now produce
  a guard failure (`fail=1`) — Config A via the corrected "not trash_roundtrip's own panic" message,
  Config B via the pre-existing (unchanged) "passed when it must be red" branch, plus the drift check
  (Finding 1's fix) independently catching the missing site too.

**Resolution:** the UAT is right that a guard-green was reachable — on the Linux leg, which its repro
(likely by not setting `RUNNER_OS=Windows`, landing on the same generic branch real Linux CI takes) happened
to exercise even though its physical machine is Windows. The reviewer's "FAIL" on a properly
`RUNNER_OS=Windows`-labeled repro is also correct, but was testing a different, already-safe branch (the
unconditional Windows catch-all) that this defect never touched. Both checkers were right about what they
tested; the fix (mechanism-specific grep) closes the gap on the branch that mattered — the Linux leg —
without needing to touch the already-safe Windows branch.

Verified after both fixes: full guard script green (`fail=0`) on the unmodified tree; the Finding-1 red/green
triad (reflow no-trip, deletion trips, addition trips) all reproduced; the Finding-2 site-4 deletion now
reds in both env configurations, confirmed via the real `assert_shared_trash_canary` function with
`RUNNER_OS=Linux` forced, not just the hand-simulated grep. `bash -n` clean, YAML parses. Pushed; watching
CI synchronously, then re-verifying by SHA per the round-2 instructions.
