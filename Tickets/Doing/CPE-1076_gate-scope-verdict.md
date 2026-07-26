---
id: CPE-1076
title: "Gate scope verdict — ai_console::gate_scope (allow-root + deny/secret)"
type: feature
component: Backend
priority: medium
status: Doing
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
- [x] `/repo/src/x.rs` under allow_root `/repo` → InScope; **`/repo-secrets/x` is NOT in scope of `/repo`**
      (segment guard, explicit test).
- [x] `.env` → SecretPath even inside an allow root (Secret > everything); deny beats plain out-of-scope.
- [x] Empty allow_roots → OutOfScope; verdict carries the matched pattern as reason.
- [x] From `sidecar/ai-console`: `cargo test` green + `cargo clippy --all-targets -- -D warnings` clean; no new
      deps.

## Work Log
2026-07-25 (workshift) — Filed by the Product Manager as a CPE-729 pure-core slice. Held in Backlog: depends on
CPE-1075 (`gate_ignore`) landing first. CPE-1077 depends on this module's `ScopeVerdict`.

2026-07-25 (workshift, Worker) — Built `sidecar/ai-console/src/gate_scope.rs`: `FsOp`/`FsAction`/
`ScopePolicy`/`ScopeVerdict`/`evaluate_scope`, registered `pub mod gate_scope;` immediately after
`pub mod gate_ignore;` in `lib.rs`. Reuses `gate_ignore::{IgnoreRule, matches}` unmodified for deny/secret
matching; `gate_ignore.rs` itself was not touched.

Allow-root containment is a fresh, purpose-built check (`segments_contain`), not a retrofit of
`gate_ignore`'s gitignore matcher (different semantics: a root is a prefix relationship, not a
gitignore-style pattern). Both the action path and each allow root are normalized (`\`→`/`, no
`std::path`) and split into non-empty `/`-segments; containment requires every root segment to equal the
path's segment at the same index (`Iterator::zip` + `all`), never a raw string `starts_with` — this is
exactly the guard the ticket calls out, since `"/repo-secrets/x".starts_with("/repo")` is true as a string
op but `/repo-secrets` and `/repo` are different first segments. Explicit regression test
`sibling_directory_sharing_a_string_prefix_is_not_in_scope` locks this in, plus a backslash-path variant of
the same case.

Precedence Secret > Deny > OutOfScope > InScope is enforced by evaluation order in `evaluate_scope`
(secret checked first, then deny, then allow-root containment), with tests for each precedence pair
(secret-inside-allow-root, secret-beats-deny, deny-beats-out-of-scope, deny-beats-in-scope). Reason strings:
`gate_ignore::matches` only returns a bool for a whole ruleset, so to recover *which* rule produced the
verdict (for `DenyListed`/`SecretPath`'s reason), `matched_reason` re-tests each rule individually via
`matches` with a singleton slice and keeps the last one that matched (`Iterator::rfind`), mirroring
`matches`'s own last-match-wins loop exactly — if the whole-ruleset call says "matched", that last
individually-matching rule is guaranteed non-negated. 10 new tests, all green, including op-kind
insensitivity (`Create`/`Modify`/`Delete`/`Read` don't affect the verdict) and multi-root any-match.

Assumption logged: the acceptance criteria don't specify a canonical string form for the reason when a
rule was `anchored`/`dir_only`/negated at parse time; `reason_for` renders it back through the same
`!`/`/`-prefix/`-suffix shape `gate_ignore::parse_rule` strips, so a plain pattern like `*.env` round-trips
verbatim (the only shape exercised by the acceptance criteria's own examples).

Verified from `sidecar/ai-console`: `cargo test` — 10/10 new `gate_scope` tests pass, full crate suite green
(no regressions, all pre-existing suites unaffected). `cargo clippy --all-targets -- -D warnings` — clean
after fixing two lints on the reason-lookup (`double_ended_iterator_last`, then `filter_next`; settled on
`.iter().rfind(...)`). No `Cargo.toml`/`Cargo.lock` changes (no new deps). Branch `cpe-1076-gate-scope`, PR
opened; ticket stays in `Doing` pending merge per repo convention (CPE-1075's pattern), moves to `Done` in a
follow-up commit once merged.
