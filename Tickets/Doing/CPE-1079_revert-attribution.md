---
id: CPE-1079
title: "Revert attribution — cpe_server::revert_attribution (agent-touched path set since checkpoint)"
type: feature
component: Backend
priority: high
status: Doing
tags: ready
created: 2026-07-25
epic: CPE-732
estimate: 1-2h
---

## Summary
Child of CPE-732 (Checkpoint & rollback). Fold the audit journal into the set of paths the agent mutated
since a checkpoint — so the revert-safety classifier can tell "the agent changed this" from "someone else
did." **Pure** in `crates/server`, `cargo test` on the 3-OS matrix — no GUI, no user resource, no new deps.
Reuses `audit_journal::AuditEvent`; does NOT modify it.

## Design (buildable)
New module `crates/server/src/revert_attribution.rs`, registered `pub mod revert_attribution;` in
`crates/server/src/lib.rs` **immediately after `pub mod audit_journal;`**. Read `audit_journal.rs` —
`AuditEvent { ts: u64, session: String, kind: String, path: String, detail: Option<String> }`.

```rust
/// Root-relative `/`-segment keys of every path the `session` agent MUTATED at/after `since_ts`.
pub fn agent_touched(events: &[AuditEvent], session: &str, since_ts: u64, root: &str)
    -> std::collections::BTreeSet<String>;
/// Cross-OS containment: Some(root-relative key) if `abs` is under `root` by SEGMENT equality, else None.
pub fn to_root_relative(root: &str, abs: &str) -> Option<String>;
```
Keep events where `session` matches AND `ts >= since_ts` AND kind is **mutating** (`created`/`modified`/
`removed`/`renamed`; **exclude `read`**). Map `event.path` to a root-relative key via `to_root_relative`
(skip if not under root). For `renamed`, contribute **both** source (`event.path`) and the rename target
parsed from `detail` (same `"-> <path>"` convention `replay.rs` uses — grep it; unparseable target → skip
just the target).

## ⚠ Cross-OS — containment via `/`-segment equality, NEVER `starts_with`
`to_root_relative` normalizes `\`→`/`, splits both into `/`-segments, and requires root's segments to be a
segment-wise prefix of abs's (`/repo-secrets/x` is NOT under `/repo`). No `std::path`, no `#[cfg]` assertion.
Only a `ts` comparison (no arithmetic risk); no recursion. Plain derives (no f64).

## Acceptance Criteria
- [ ] `read` excluded; `created`/`modified`/`removed`/`renamed` included; `ts < since_ts` and other-session
      excluded.
- [ ] `\`- and `/`-input map to the same key; **prefix-collision guard**: `root=/repo`, `/repo-secrets/x`
      → excluded, `/repo/x` → `x`.
- [ ] `renamed` yields both source + target keys; out-of-root path skipped (not panicked); empty → empty set.
- [ ] `cargo test -p cpe-server` green; clippy `--all-targets -- -D warnings` clean in default AND
      `--features index`; no new deps.

## Work Log
2026-07-25 (workshift) — Filed by the Product Manager as the CPE-732 attribution foundation (audit is already
session-tagged, so this is live-feed-ready). Independent module; distinct lib.rs anchor.
