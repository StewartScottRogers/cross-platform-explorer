---
id: CPE-1070
title: "Region heat-map rollup — ai_console::conflict_region (folder-level owner + contention)"
type: feature
component: Backend
priority: medium
status: Backlog
tags: ready
created: 2026-07-25
epic: CPE-730
depends-on: CPE-1069
---

## Summary
Child of CPE-730 (Multi-agent conflict radar). Roll per-file attribution up to folder regions so the heat-map
can colour a directory by its dominant agent + contention count. **Pure aggregation** in the sidecar
`ai-console` crate, `cargo test` on the 3-OS Sidecar-platform CI — no GUI, no user resource, no new deps.
**Depends on CPE-1069** (consumes its `PathAttribution`) — dispatch after CPE-1069 merges. Does NOT modify
`conflict.rs`.

## Design (buildable)
New module `sidecar/ai-console/src/conflict_region.rs`, registered `pub mod conflict_region;` in
`sidecar/ai-console/src/lib.rs` at a distinct anchor (e.g. **immediately after `pub mod conflict_owner;`**
once CPE-1069 has landed). Reuses `conflict_owner::PathAttribution`.

```rust
pub struct RegionOwner { pub region: String, pub owner: String, pub contended_files: u32 }
pub fn roll_up(attributions: &[PathAttribution], max_depth: usize) -> Vec<RegionOwner>;
```
For each directory prefix (up to `max_depth` segments deep), aggregate the dominant owner (majority of files'
owners; deterministic tie-break — reuse the lexically-least rule) + a `contended_files` count (files with 2+
contributors). Deterministic sorted-by-region output.

## ⚠ Cross-OS — CRITICAL (a sibling ticket red the Linux/macOS CI; another had a prefix-collision bug)
Compute directory prefixes by **splitting on `/` segments and comparing segment-by-segment** (segment
equality) — NEVER `std::path::Path` and NEVER string `starts_with`. Normalize `\`→`/` at the input boundary.
A sibling dir whose name is a prefix of another (**`a/` vs `ab/`**) must NOT be merged — add an explicit
probe test for this exact case. No `#[cfg]` assertion.

## ⚠ Bounded + checked
Honour `max_depth` so a pathological deep path can't run unbounded; **iterative, no recursion**. Counts via
`u32::saturating_add`.

## Acceptance Criteria
- [ ] Files under a dir roll up to that dir's majority owner (deterministic tie-break); `contended_files`
      counts multi-contributor files.
- [ ] Prefix-collision guard: `a/x` and `ab/y` roll to SEPARATE regions `a` and `ab`, never merged (explicit
      test).
- [ ] Depth capped at `max_depth`; empty input → empty; counts saturating (no overflow).
- [ ] From `sidecar/ai-console`: `cargo test` green + `cargo clippy --all-targets -- -D warnings` clean; no new
      deps.

## Work Log
2026-07-25 (workshift) — Filed by the Product Manager as the CPE-730 rollup slice. Held in Backlog: depends on
CPE-1069 (`PathAttribution`) landing first.
