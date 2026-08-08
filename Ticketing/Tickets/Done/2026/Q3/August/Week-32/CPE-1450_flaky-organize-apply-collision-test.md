---
id: CPE-1450
title: "Flaky test: organize_apply::tests::organize_apply_skips_on_name_collision_without_failing_the_rest fails under full-suite parallelism"
type: Bug
status: Done
priority: Low
component: Backend
tags: [ready]
epic: CPE-810
created: 2026-08-07
---
## Observation (seen repeatedly across the 2026-08-07 workshift)
`crates/server/src/organize_apply.rs`'s test `organize_apply_skips_on_name_collision_without_failing_the_rest`
intermittently FAILS on the first `cargo test` run under full-suite parallel load, then PASSES in isolation
(`--test-threads=1`) and on a clean rerun. Observed independently by ≥3 separate agents this shift (reviewers +
workers on unrelated PRs #709/#712/#713). Not a regression from any of those diffs — a pre-existing
parallelism/ordering flake in the test itself.

## Likely cause + fix direction
A name-collision test that almost certainly shares a fixed temp path / working directory or relies on
filesystem ordering that isn't stable when other tests run concurrently. Fix: give it a unique per-test temp
dir (e.g. `tempfile::tempdir()` if already a dep, or a uniquely-named dir under the OS temp), avoid any
process-wide `set_current_dir`, and make the collision setup deterministic. Confirm it passes reliably under
`cargo test` (full parallel) across several runs on the 3-OS matrix.

## Why it matters
A flaky test erodes the CI signal the whole crew relies on — every reviewer this shift had to manually
distinguish this flake from a real regression, and a genuinely-broken build could hide behind "oh that's just
the organize_apply flake." Cheap to fix, high signal-to-noise payoff.

## Notes
Low priority (doesn't block — passes on rerun) but worth doing. Epic CPE-810 (client/server contract +
test hygiene). Good QA-Architect candidate.

## Work Log

- 2026-08-08 — Investigated and fixed.

  **Root cause found:** `organize_apply.rs`'s test module had its own `scratch(tag)` helper that built each
  test's temp dir as `std::env::temp_dir().join(format!("cpe-organize-apply-{tag}-{pid}-{n}"))`, using the
  current process id plus a module-local `AtomicU64` counter for uniqueness — and never deleted the
  directory afterwards. That combination is **only unique within one `cargo test` process's lifetime**, not
  globally:
  - No test ever cleaned up its scratch dir, so every run left files behind under the OS temp dir forever
    (across the whole 2026-08-07 workshift's many `cargo test` invocations from many agents/worktrees on the
    same machine, this had been accumulating for a full day).
  - `(pid, counter)` is only collision-free while that exact process is alive. Windows recycles process ids
    quickly for short-lived processes like `cargo test` binaries; a *later* `cargo test` invocation that
    happens to be assigned a previously-used pid, whose internal counter reaches the same sequence number a
    prior run used for `scratch("collision")`, revisits the **same directory name** — now pre-populated with
    **stale leftover files from an earlier pass of the exact same test** (e.g. a `Documents/b.pdf` already
    sitting there from a previous run). The collision test's "the pdf move must still succeed" assertion
    (`assert!(pdf_result.ok)`) would then fail against that pre-existing state — spuriously, and only under
    the kind of sustained heavy parallel `cargo test` churn a full-suite run (or a whole workshift's worth of
    them) produces. This matches the reported signature exactly: fails on a full run, passes in isolation or
    on a fresh rerun (a rerun is far less likely to land on the same reused pid + counter value again).
  - No `set_current_dir` calls, no shared/global statics beyond that counter, and no ordering dependency were
    found in this file or its `checkpoint_store`/`snapshot_capture` dependencies — each test's checkpoint
    store path is keyed by a SHA-256 of the (already-unique) scratch root, so that part was already sound.

  **Fix:** replaced the hand-rolled `(pid, counter)` scratch helper with `tempfile::TempDir`
  (`tempfile::Builder::new().prefix(...).tempdir().unwrap()`), matching the pattern already used elsewhere in
  this crate (`secure_shred.rs`, `shell_menu.rs`, `ticket_board.rs`, `vault_crypto.rs`). `tempfile` names each
  directory with a random, collision-resistant suffix with no dependence on pid or a counter, and removes the
  directory automatically on `Drop` — so nothing is ever left behind to collide with a future run again. All
  6 tests in the file share this one helper, so all of them (not just the collision test) got the fix.
  Updated the ~15 call sites from `dir.join(...)` / `dir.to_string_lossy()` / `ctx_for(&dir)` to
  `dir.path().join(...)` / `dir.path().to_string_lossy()` / `ctx_for(dir.path())` to go through
  `TempDir::path()` explicitly (the codebase's established idiom — see `secure_shred.rs`'s `write_temp`
  helper — rather than relying on `Deref` coercion). No production logic in `organize_apply.rs` was touched.

  **Verification (multi-run, full parallel):**
  - `cargo build --lib` — clean.
  - `cargo test --lib organize_apply` — 6/6 passed, ×10 consecutive runs, all green.
  - `cargo test --lib` (full crate suite, default 32-thread parallelism, 1717 tests) — ×5 consecutive full
    runs, **0 failures every time** (66s / 52s / 10s / 40s / 23s — the varying wall time itself shows real
    contention/parallelism across runs, so this wasn't a low-load fluke).
  - `cargo clippy --all-targets -- -D warnings` — clean.
  - `cargo clippy --all-targets --features index -- -D warnings` — clean.
  - Could not reproduce the original flake even before the fix (7 full-suite runs on this machine, all
    green) — consistent with the root cause being tied to *accumulated cross-process leftover state* built
    up over a long workshift rather than a single-run race, which a short local reproduction window won't
    trigger. The fix removes the mechanism (stale shared-name leftovers) regardless of whether it
    reproduced locally.

  PR: branch `cpe-1450-flaky-organize-apply-test`.
