---
id: CPE-1077
title: "Gate decision fusion — ai_console::gate_decision (command-risk + scope → outcome)"
type: feature
component: Backend
priority: medium
status: Doing
tags: ready
created: 2026-07-25
epic: CPE-729
depends-on: CPE-1076
---

## Summary
Child of CPE-729 (Intervene & approve — pure policy core). The policy-fusion heart: combine a command's risk
(existing `guardrail::assess_command`) with a scope verdict + approval policy into a gate outcome
(auto-allow / needs-approval / auto-block). **Pure** in the sidecar `ai-console` crate, `cargo test` on the
3-OS Sidecar-platform CI — no GUI, no user resource, no new deps. **Depends on CPE-1076** (reuses
`gate_scope::ScopeVerdict`) — dispatch after CPE-1076 merges.

## Design (buildable)
New module `sidecar/ai-console/src/gate_decision.rs`, registered `pub mod gate_decision;` in
`sidecar/ai-console/src/lib.rs` at a distinct anchor (e.g. **immediately after `pub mod gate_scope;`** once
CPE-1076 has landed). Builds on `guardrail::{RiskAssessment, RiskLevel, ApprovalPolicy, assess_command,
needs_approval}` (read guardrail.rs) + `gate_scope::ScopeVerdict`.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateOutcome { AutoAllow, NeedsApproval, AutoBlock }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateDecision { pub outcome: GateOutcome, pub reasons: Vec<String> }

pub fn decide(
    command: Option<&str>,       // scored via guardrail::assess_command if Some
    scope: &ScopeVerdict,
    policy: ApprovalPolicy,
    strict: bool,                // strict → escalate NeedsApproval to AutoBlock for Secret/Deny
) -> GateDecision;
```
Rules (strongest signal wins): `SecretPath`/`DenyListed` → NeedsApproval (AutoBlock if `strict`), regardless
of command risk; `OutOfScope` for a write/delete (not read) → NeedsApproval; otherwise fold in
`guardrail::needs_approval(risk_level, policy)` (true → NeedsApproval, else AutoAllow). Collect human-readable
`reasons`, **sorted + deduped** for determinism (mirror `guardrail::dedup` if it exists, else sort+dedup).

## ⚠ Notes
Deterministic sorted/deduped reasons. No arithmetic, no recursion. Derives plain (no f64 → Eq OK); no
serde/specta. No `std::path`, no `#[cfg]`.

## Acceptance Criteria
- [ ] High-risk command in-scope under `HighOnly` policy → NeedsApproval; a Low-risk READ of a SecretPath →
      NeedsApproval (scope escalates past command risk).
- [ ] `Off` policy + in-scope + Low risk → AutoAllow; `DenyListed` under `strict` → AutoBlock.
- [ ] `reasons` deterministic (sorted + deduped).
- [ ] From `sidecar/ai-console`: `cargo test` green + `cargo clippy --all-targets -- -D warnings` clean; no new
      deps.

## Work Log
2026-07-25 (workshift) — Filed by the Product Manager as the CPE-729 fusion heart. Held in Backlog: depends on
CPE-1076 (`gate_scope`) landing first.
