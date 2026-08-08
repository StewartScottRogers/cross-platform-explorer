---
id: CPE-1450
title: "Flaky test: organize_apply::tests::organize_apply_skips_on_name_collision_without_failing_the_rest fails under full-suite parallelism"
type: Bug
status: Backlog
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
