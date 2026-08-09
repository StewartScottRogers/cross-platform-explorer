---
id: CPE-1067
title: "Competing-rename detector — ai_console::conflict_rename (divergence / collision)"
type: feature
component: Backend
priority: medium
status: Done
tags: ready
created: 2026-07-25
epic: CPE-730
estimate: 2-3h
---

## Summary
Child of CPE-730 (Multi-agent conflict radar). Detect when multiple agents' renames conflict — the same
source renamed to different targets (divergence), or different sources renamed onto the same target
(collision). **Pure set logic** in the sidecar `ai-console` crate, `cargo test` on the 3-OS Sidecar-platform
CI — no GUI, no user resource, no new deps. Builds on the shipped `conflict.rs` (CPE-914) model style; does
NOT modify `conflict.rs`.

## Design (buildable)
New module `sidecar/ai-console/src/conflict_rename.rs`, registered `pub mod conflict_rename;` in
`sidecar/ai-console/src/lib.rs` **immediately after `pub mod conflict;`**. Mirror the derive/style of
`sidecar/ai-console/src/conflict.rs` (read it first — `AgentActivity`, `Conflict`, `ConflictKind`, sorted
stable output).

```rust
pub struct RenameActivity { pub agent: String, pub renames: Vec<(String /*from*/, String /*to*/)> }
#[derive(...serialize + specta if that's the crate convention...)]
pub enum RenameConflictKind { Divergence, Collision }
pub struct RenameConflict { pub path: String, pub kind: RenameConflictKind, pub agents: Vec<String> }

pub fn detect_rename_conflicts(activity: &[RenameActivity]) -> Vec<RenameConflict>;
```
- **Divergence**: 2+ DISTINCT agents rename the same `from` to DIFFERENT `to` → conflict keyed on `from`.
- **Collision**: 2+ DISTINCT agents rename DIFFERENT `from` onto the same `to` → conflict keyed on `to`.
- Two folds: `BTreeMap<&str, BTreeSet<..>>` keyed by `from` (values = distinct (agent,to)), and by `to`.
- Same-agent repeated rename is NOT a conflict; a `from == to` no-op is ignored. Sort results by `path`,
  agents sorted, for stable radar order (mirror conflict.rs).

## ⚠ Cross-OS
Paths are opaque strings compared by whole/segment equality — NEVER `std::path` or `starts_with`. No `#[cfg]`.
(Pure set logic — no arithmetic/recursion risk.)

## Acceptance Criteria
- [ ] Divergence detected (same `from`, different `to`, 2+ distinct agents); agents sorted.
- [ ] Collision detected (different `from`, same `to`, 2+ distinct agents).
- [ ] Same-agent double-rename → empty; `from == to` no-op ignored; disjoint renames → empty; deterministic
      sorted output.
- [ ] From `sidecar/ai-console`: `cargo test` green + `cargo clippy --all-targets -- -D warnings` clean
      (match how `conflict.rs` is built/tested — read the crate); no new deps.

## Work Log
2026-07-25 (sprint) — Filed by the Product Manager as a CPE-730 slice. Independent module in the sidecar
ai-console crate; one-line lib.rs `pub mod` at a distinct anchor. Reuses conflict.rs style, doesn't modify it.

2026-07-25 (sprint, Worker) — Implemented `sidecar/ai-console/src/conflict_rename.rs`:
`RenameActivity`, `RenameConflictKind` (Divergence/Collision), `RenameConflict`,
`detect_rename_conflicts`. Registered `pub mod conflict_rename;` in `lib.rs` immediately after
`pub mod conflict;`. Mirrored `conflict.rs`'s derive stack exactly — plain `Debug`/`Clone`/`Eq`
derives, no serde/specta — since `conflict.rs` itself carries none (assumption: the ticket's
"...serialize + specta if that's the crate convention..." resolves to "no derives beyond
Debug/Clone/PartialEq/Eq" because that's what the file it says to mirror actually does). Divergence
requires 2+ distinct agents AND 2+ distinct targets on the same `from` (two agents renaming the same
source to the *same* target isn't a disagreement, so it's correctly not flagged); Collision is the
symmetric check on `to`. `from == to` no-ops are dropped before folding. 11 new unit tests (disjoint,
divergence, collision, same-agent double-rename, same-agent repeated-target, no-op ignored, no-op
mixed with a real conflict, divergence-needs-two-targets edge case, 3-agent sorted-agents, path-sorted
determinism, both kinds reported together). Verify: `cargo test` (from `sidecar/ai-console`) — 320
passed, 0 failed, 2 ignored (pre-existing, unrelated); `cargo clippy --all-targets -- -D warnings` —
clean, no new deps added. No Defender/os-error-225 issues hit this run.
