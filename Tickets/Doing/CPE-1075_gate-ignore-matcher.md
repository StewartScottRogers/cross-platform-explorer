---
id: CPE-1075
title: "Gate ignore matcher — ai_console::gate_ignore (gitignore-style path-vs-pattern)"
type: feature
component: Backend
priority: medium
status: Doing
tags: ready
created: 2026-07-25
epic: CPE-729
estimate: 2-3h
---

## Summary
Child of CPE-729 (Intervene & approve — pure policy core). A gitignore-style matcher deciding whether a
normalized path matches an ordered ruleset — the FOUNDATION for scope/deny/secret matching. **Pure** in the
sidecar `ai-console` crate, `cargo test` on the 3-OS Sidecar-platform CI — no GUI, no user resource, no new
deps. (This wave scopes only CPE-729's pure rule-eval core; the big-design boundary/hold-integration/GUI are
attended, not here.)

## Design (buildable)
New module `sidecar/ai-console/src/gate_ignore.rs`, registered `pub mod gate_ignore;` in
`sidecar/ai-console/src/lib.rs` **immediately after `pub mod guardrail;`**. Read `swarm_locks.rs` (`segs`
splits on `/` dropping empties) for the segment convention — but this is path-vs-pattern matching (distinct
from swarm_locks' pattern-overlap), so build fresh, don't retrofit.

```rust
#[derive(Debug, Clone, PartialEq, Eq)]   // no f64; NO serde/specta
pub struct IgnoreRule { pub negate: bool, pub anchored: bool, pub dir_only: bool, pub segments: Vec<String> }
pub fn parse_rule(pattern: &str) -> IgnoreRule;
pub fn matches(path: &str, rules: &[IgnoreRule]) -> bool;   // last matching rule's polarity wins
```
Gitignore semantics: `*`/`?` within a segment; `**` spans whole segments; leading `/` anchors to root;
trailing `/` = dir-only; `!` negation with **last-match-wins** (walk rules in order, the final matching
rule's polarity decides). Normalize `\`→`/` before splitting.

## ⚠ Bounded (learned this session — a recursive matcher stack-overflowed)
The wildcard/`**` matcher must be **iterative or explicitly depth-bounded** — deep/adversarial paths or
patterns must not stack-overflow (swarm_locks' equivalent is recursive-with-backtracking; go iterative here
or cap depth).

## ⚠ Cross-OS
Normalize `\`→`/` then split into `/`-segments — **no `std::path`**, no `#[cfg]` assertion, no `starts_with`.

## Acceptance Criteria
- [ ] `**/node_modules/` matches `a/b/node_modules/x`; `!keep.env` after `*.env` un-ignores `keep.env`
      (last-match-wins negation).
- [ ] Leading `/build` anchors (matches `build/x`, NOT `src/build/x`); `*.pem` matches a secret path.
- [ ] A `dir/`-only rule does NOT match a file of that name; deep path/pattern does not stack-overflow.
- [ ] From `sidecar/ai-console`: `cargo test` green + `cargo clippy --all-targets -- -D warnings` clean; no new
      deps.

## Work Log
2026-07-25 (workshift) — Filed by the Product Manager as the CPE-729 pure-core foundation. Independent module
in the sidecar ai-console crate; distinct lib.rs anchor. CPE-1076 depends on this module's `IgnoreRule`/`matches`.
