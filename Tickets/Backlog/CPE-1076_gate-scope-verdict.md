---
id: CPE-1076
title: "Gate scope verdict — ai_console::gate_scope (allow-root + deny/secret)"
type: feature
component: Backend
priority: medium
status: Backlog
tags: ready
created: 2026-07-25
epic: CPE-729
depends-on: CPE-1075
---

## Summary
Child of CPE-729 (Intervene & approve — pure policy core). Given a filesystem action + a scope policy, return
a verdict (in-scope / out-of-scope / deny-listed / secret). **Pure** in the sidecar `ai-console` crate,
`cargo test` on the 3-OS Sidecar-platform CI — no GUI, no user resource, no new deps. **Depends on CPE-1075**
(reuses `gate_ignore::{IgnoreRule, matches}`) — dispatch after CPE-1075 merges.

## Design (buildable)
New module `sidecar/ai-console/src/gate_scope.rs`, registered `pub mod gate_scope;` in
`sidecar/ai-console/src/lib.rs` at a distinct anchor (e.g. **immediately after `pub mod gate_ignore;`** once
CPE-1075 has landed). Reuse `gate_ignore::{IgnoreRule, matches}`.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsOp { Create, Modify, Delete, Read }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsAction { pub path: String, pub op: FsOp }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopePolicy { pub allow_roots: Vec<String>, pub deny: Vec<IgnoreRule>, pub secret: Vec<IgnoreRule> }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeVerdict { InScope, OutOfScope, DenyListed(String), SecretPath(String) }   // reason = matched pattern

pub fn evaluate_scope(action: &FsAction, policy: &ScopePolicy) -> ScopeVerdict;
```
Precedence: **Secret > Deny > OutOfScope > InScope**. Allow-root containment is **segment-wise** (the path's
`/`-segments must start with the root's segments, compared segment-by-segment). Deny/secret reuse
`gate_ignore::matches`. Empty `allow_roots` → OutOfScope default.

## ⚠ Cross-OS — the prefix-collision guard is the whole point
Allow-root containment via **`/`-segment equality, NEVER `starts_with`** — `/repo-secrets/x` must NOT be in
scope of allow-root `/repo`. Normalize `\`→`/`; no `std::path`, no `#[cfg]` assertion.

## Acceptance Criteria
- [ ] `/repo/src/x.rs` under allow_root `/repo` → InScope; **`/repo-secrets/x` is NOT in scope of `/repo`**
      (segment guard, explicit test).
- [ ] `.env` → SecretPath even inside an allow root (Secret > everything); deny beats plain out-of-scope.
- [ ] Empty allow_roots → OutOfScope; verdict carries the matched pattern as reason.
- [ ] From `sidecar/ai-console`: `cargo test` green + `cargo clippy --all-targets -- -D warnings` clean; no new
      deps.

## Work Log
2026-07-25 (workshift) — Filed by the Product Manager as a CPE-729 pure-core slice. Held in Backlog: depends on
CPE-1075 (`gate_ignore`) landing first. CPE-1077 depends on this module's `ScopeVerdict`.
