---
id: CPE-1065
title: "Replay folder view + transition diff — cpe_server::replay_view (children_at / diff_states)"
type: feature
component: Backend
priority: medium
status: Backlog
tags: ready
created: 2026-07-25
epic: CPE-728
depends-on: CPE-1063
---

## Summary
Child of CPE-728 (Activity replay & scrub). The folder-view projection: what direct children a folder shows
at moment T, and the add/remove/modify diff between two scrub cursors (to animate a scrub). Pure, backend-only,
`cargo test` on the 3-OS matrix — no GUI, no user resource, no new deps. **Depends on CPE-1063** (uses its
`FsState`) — dispatch after CPE-1063 merges.

## Design (buildable)
New module `crates/server/src/replay_view.rs`, registered `pub mod replay_view;` in `lib.rs` at a distinct
anchor (e.g. **immediately after `pub mod replay;`** once CPE-1063 has landed). Reuses
`crate::replay::{FsState, FsNode}`.

```rust
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ReplayEntry { pub name: String, pub path: String, pub ts: u64, pub kind: String }

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, Default)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct StateDiff { pub added: Vec<String>, pub removed: Vec<String>, pub modified: Vec<String> }

/// The DIRECT children (one level below `dir`) present in `state`.
pub fn children_at(state: &FsState, dir: &str) -> Vec<ReplayEntry>;
/// What changed from state `a` to state `b`.
pub fn diff_states(a: &FsState, b: &FsState) -> StateDiff;
```
`children_at`: return only entries exactly one level below `dir` (a nested grandchild is excluded).
`diff_states`: `added` = in b not a; `removed` = in a not b; `modified` = in both but node differs (ts/kind).
Deterministic (sorted) output.

**⚠ Cross-OS (learned this shift — a sibling PR red the Linux/macOS CI):** compute parent/child by splitting
the stored path on normalized `/` segments — **NEVER** `std::path::Path::parent()`/`components()` (platform
separators differ). Normalize `\`→`/` if needed. No `#[cfg(windows)]` assertions.

## Acceptance Criteria
- [ ] `children_at` returns only one level below `dir` (grandchildren excluded); works for root and nested
      dirs; deterministic ordering.
- [ ] `diff_states` classifies added/removed/modified correctly; identical states → empty diff.
- [ ] Cross-OS: `a/b/c.txt` under `a/b` computed via string segments, not `std::path`; no platform-specific
      test.
- [ ] `cargo test -p cpe-server` green; clippy `--all-targets -- -D warnings` clean in default AND
      `--features index` builds; no new deps.

## Work Log
2026-07-25 (workshift) — Filed by the Product Manager as a CPE-728 slice. Held in Backlog: depends on
CPE-1063 (`FsState`) landing first.
