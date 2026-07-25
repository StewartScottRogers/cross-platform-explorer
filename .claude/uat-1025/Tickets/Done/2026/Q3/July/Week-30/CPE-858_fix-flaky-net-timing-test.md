---
id: CPE-858
title: Fix flaky cpe-net local-fast timing guard (best-of-trials)
type: bug
component: Backend
priority: medium
status: Done
tags: ready
created: 2026-07-21
closed: 2026-07-21
---

## Summary
`cpe-net`'s `in_process_dispatch_is_not_taxed_by_the_remote_path` (CPE-825, CPE-810 tiebreaker) asserted
`in_process < loopback` from a single timed batch of each path. On a loaded CI runner a one-off scheduler
stall during the in-process batch could make it momentarily ≥ the loopback batch, failing CI on unrelated
PRs (observed: the Windows Server-crates job on PR #132). The structural invariant is real — loopback does
everything in-process does *plus* a TCP round-trip — but the single-sample comparison was noise-sensitive.

## Fix
Time each path **best-of-5 trials** and compare the *minimum* batch time (`best_in < best_loop`). The
minimum is a stable lower-bound estimator that filters one-off stalls, so only a genuine regression (the
local path consistently as slow as the networked one) trips the assert. Server/client and ctx/dispatcher
are built once and reused across trials.

## Acceptance Criteria
- [x] Test compares best-of-N minima instead of a single batch sum.
- [x] `cargo test -p cpe-net` passes; `cargo clippy --all-targets -D warnings` clean.

## Resolution
`crates/net/src/lib.rs` — added `TRIALS=5`, imported `Duration`, looped both timings taking `.min()`,
asserted on the minima. No production code changed (test-only robustness fix).
