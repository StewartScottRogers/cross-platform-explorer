---
id: CPE-894
title: agent_memory::load_dir dedup is filesystem-order-dependent (flaky on Linux CI)
type: bug
component: Sidecar
priority: high
tags: ready
epic: CPE-259
created: 2026-07-22
closed: 2026-07-22
status: Done
---

## Summary
`ai-console`'s `agent_memory::load_dir` iterated `std::fs::read_dir` in **arbitrary OS order** and fed
notes to `MemoryGraph::add`, which keeps the **first** note seen for a given content hash (append-only
dedup). Notes are saved as `{id}-{hash}.md`, so two notes with identical content but different ids
(e.g. `a-….md` and `dup-….md`) produce two files; which one survives the dedup — and therefore the
surviving node's **id and links** — depended on the filesystem's directory order. On Linux CI `dup` was
read before `a`, so `a` vanished and `save_then_load_dir_round_trips` failed at
`neighbors("a").len() == 1` (it was 0). It passed on Windows only by luck of read order.

This surfaced as a red **Sidecar platform (ubuntu)** job blocking an unrelated PR (#196), since the job
runs regardless of what a PR touches.

Fix: sort the `*.md` paths before loading, so content-hash dedup keeps a deterministic survivor
(lexicographically-first filename) on every platform.

## Acceptance Criteria
- [x] `load_dir` sorts entries before loading; dedup survivor is deterministic cross-platform.
- [x] `save_then_load_dir_round_trips` passes on Linux (and Windows).
- [x] `cargo test --lib` (ai-console 292 pass) + `cargo clippy --all-targets -D warnings` clean.

## Work Log
- 2026-07-22 (nightshift) — Root-caused from CI: `read_dir` order + first-wins dedup = nondeterministic
  survivor. Sorted the collected `.md` paths before the load loop. Not caused by the PR whose CI caught
  it; fixed on its own branch.
