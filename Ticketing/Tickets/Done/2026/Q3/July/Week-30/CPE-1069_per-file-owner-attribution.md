---
id: CPE-1069
title: "Per-file attribution + owner — ai_console::conflict_owner (who else is here + heat-map owner)"
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
Child of CPE-730 (Multi-agent conflict radar). Produce the per-file "who else is here" contributor feed plus a
deterministic owner per file for the heat-map. **Pure fold** in the sidecar `ai-console` crate, `cargo test`
on the 3-OS Sidecar-platform CI — no GUI, no user resource, no new deps. Reuses `conflict::AgentActivity`;
does NOT modify `conflict.rs`.

## Design (buildable)
New module `sidecar/ai-console/src/conflict_owner.rs`, registered `pub mod conflict_owner;` in
`sidecar/ai-console/src/lib.rs` **immediately after `pub mod usage;`**. Read `conflict.rs` for `AgentActivity`
(has per-agent `edited`/`deleted` path sets — confirm exact field names) + the derive/style convention.

```rust
pub struct PathAttribution {
    pub path: String,
    pub contributors: Vec<(String /*agent*/, u32 /*edits*/, u32 /*deletes*/)>,
    pub owner: String,
}
pub fn attribute(activity: &[conflict::AgentActivity]) -> Vec<PathAttribution>;
```
Fold every agent's edited/deleted paths into per-path per-agent counts. `owner` rule (deterministic): most
edits wins → tie: most deletes → tie: **lexically-least agent** (so heat-map colouring is stable/reproducible).
`contributors` sorted (e.g. by agent). Results sorted by `path`.

## ⚠ Checked arithmetic
Per-agent edit/delete counts accumulate with `u32::saturating_add` so a pathological stream can't overflow.
No recursion.

## ⚠ Cross-OS
String paths only — no `std::path`, no `#[cfg]` assertion.

## Acceptance Criteria
- [x] Single-agent path → owned by that agent; contested path owner = top editor.
- [x] Deterministic lexical tie-break on equal edit/delete counts; contributors sorted; path-sorted output.
- [x] Empty input → empty; counts use saturating_add (no overflow panic).
- [x] From `sidecar/ai-console`: `cargo test` green + `cargo clippy --all-targets -- -D warnings` clean; no new
      deps.

## Work Log
2026-07-25 (workshift) — Filed by the Product Manager as a CPE-730 slice. Independent module (reuses
conflict::AgentActivity, doesn't modify it); distinct lib.rs anchor. CPE-1070 depends on this module's
`PathAttribution`.

2026-07-25 (workshift, Worker) — Built end-to-end on branch `cpe-1069-conflict-owner`. New module
`sidecar/ai-console/src/conflict_owner.rs`: `PathAttribution { path, contributors: Vec<(String, u32, u32)>,
owner: String }` and `pub fn attribute(activity: &[conflict::AgentActivity]) -> Vec<PathAttribution>`.
Registered `pub mod conflict_owner;` in `lib.rs` immediately after `pub mod usage;`, per the ticket anchor.
Reused `AgentActivity`'s exact fields (confirmed in `conflict.rs`): `agent: String`, `edited: BTreeSet<String>`,
`deleted: BTreeSet<String>` — no changes to `conflict.rs`.

Fold: `BTreeMap<&str /*path*/, BTreeMap<&str /*agent*/, (u32 edits, u32 deletes)>>`, both counts bumped with
`u32::saturating_add`. Owner rule implemented as `min_by_key(|(agent, (edits, deletes))| (Reverse(edits),
Reverse(deletes), agent))` — i.e. most edits wins, tie → most deletes, tie → lexically-least agent name.
`contributors` sorted by agent (falls out of `BTreeMap` iteration), results sorted by path (outer
`BTreeMap` iteration) — no separate sort step needed. No recursion, no `std::path`, no `#[cfg]` assertions,
no new deps (`Cargo.toml`/`Cargo.lock` untouched).

Assumption: per the ticket's example type signature, `contributors` sorts by agent name (not by
edit count) — matches `conflict.rs`'s existing convention of sorting agent lists lexically for a stable
radar/heat-map order.

Verify (from `sidecar/ai-console`): `cargo test` — full crate suite green (8 new `conflict_owner` unit
tests + all pre-existing tests, including integration tests, unaffected). `cargo clippy --all-targets --
-D warnings` clean. `git diff --stat` confirms only `lib.rs` (+1 line) and the new file changed —
`Cargo.lock` untouched. PR opened; see PR link in repo history.
