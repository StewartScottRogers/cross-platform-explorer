---
id: CPE-1069
title: "Per-file attribution + owner — ai_console::conflict_owner (who else is here + heat-map owner)"
type: feature
component: Backend
priority: medium
status: Doing
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
- [ ] Single-agent path → owned by that agent; contested path owner = top editor.
- [ ] Deterministic lexical tie-break on equal edit/delete counts; contributors sorted; path-sorted output.
- [ ] Empty input → empty; counts use saturating_add (no overflow panic).
- [ ] From `sidecar/ai-console`: `cargo test` green + `cargo clippy --all-targets -- -D warnings` clean; no new
      deps.

## Work Log
2026-07-25 (workshift) — Filed by the Product Manager as a CPE-730 slice. Independent module (reuses
conflict::AgentActivity, doesn't modify it); distinct lib.rs anchor. CPE-1070 depends on this module's
`PathAttribution`.
