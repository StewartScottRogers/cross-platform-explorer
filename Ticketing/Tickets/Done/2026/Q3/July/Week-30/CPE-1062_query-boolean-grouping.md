---
id: CPE-1062
title: "Search boolean grouping — cpe_server::query_group (OR / NOT / parentheses)"
type: feature
component: Backend
priority: high
status: Done
tags: ready
created: 2026-07-25
epic: CPE-703
estimate: 3-4h
---

## Summary
Child of CPE-703 (Instant index search). The current query grammar is AND-only. Add a **pure,
dependency-free** parser + evaluator for boolean structure — `OR`, `NOT`/`-`, and parenthesised grouping —
over opaque leaf tokens, turning the filter set into a real query language. Backend-only, `cargo test` on the
3-OS matrix — no GUI, no user resource, no new deps. Standalone module — does NOT touch `index_query.rs`
(it's tested with a stub leaf matcher, so it needs none of the sibling filter modules).

## Design (buildable)
New module `crates/server/src/query_group.rs`, registered `pub mod query_group;` in `lib.rs` **immediately
after `pub mod simhash;`**.

```rust
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum Node { And(Vec<Node>), Or(Vec<Node>), Not(Box<Node>), Leaf(String) }

/// Parse a query string into a predicate tree over opaque leaf tokens. Grammar (precedence low→high):
/// OR  <  AND (implicit by juxtaposition, or explicit `AND`)  <  NOT (`NOT`/`-`)  <  ( … ).
/// Whitespace separates leaves (implicit AND). Tolerate unbalanced parens gracefully (documented rule,
/// e.g. auto-close at EOF) rather than panicking.
pub fn parse(query: &str) -> Node;
/// Evaluate the tree; `leaf` decides whether an opaque leaf token matches the item under test.
pub fn eval(node: &Node, leaf: &impl Fn(&str) -> bool) -> bool;
```
- Precedence: `NOT` binds tighter than `AND` (juxtaposition) tighter than `OR`. `-token` == `NOT token`.
- Parenthesised sub-expressions group; nesting supported. An empty query → a Node that matches everything
  (document: e.g. `And(vec![])` evaluates true).
- Leaves are opaque strings (a later ticket maps a leaf like `size:>1mb` to the real predicates); this module
  is agnostic to leaf meaning and is tested with a stub `leaf` matcher (e.g. matches if in a provided set).

## Acceptance Criteria
- [ ] Precedence correct: `a OR b c` parses as `a OR (b AND c)`; `NOT a b` as `(NOT a) AND b`.
- [ ] Parenthesised grouping + nesting parse and evaluate correctly; `-x` == `NOT x`.
- [ ] `eval` with a stub leaf matcher yields correct booleans incl. De Morgan sanity (`NOT (a OR b)` ==
      `NOT a AND NOT b`); unbalanced parens tolerated (documented), empty query matches all — no panic.
- [ ] `cargo test -p cpe-server` green; clippy `--all-targets -- -D warnings` clean in default AND
      `--features index` builds; no new deps.

## Work Log
2026-07-25 (workshift) — Filed by the Product Manager as the CPE-703 DSL slice that turns the filters into a
real query language. Independent of the sibling filter modules (stub leaf matcher). One-line lib.rs `pub mod`
at a distinct anchor.

2026-07-25 (workshift Worker, overnight, unattended) — Implemented `crates/server/src/query_group.rs`:
recursive-descent parser (`lex` → `parse_or` → `parse_and` → `parse_not` → `parse_atom`) producing
`Node::{And,Or,Not,Leaf}`, plus `eval` against a caller-supplied leaf predicate. Registered
`pub mod query_group;` in `lib.rs` immediately after `pub mod simhash;` per the anchor instruction.

Rules implemented (all documented in the module doc comment):
- Precedence low→high: `OR` < implicit/explicit `AND` (juxtaposition or the word `AND`) < `NOT`/`-` < `( … )`.
- `-token` (no space) lexes identically to `NOT token`; `OR`/`AND`/`NOT` keywords are matched
  case-insensitively; leaf token text itself is preserved verbatim (not lowercased) since a leaf is opaque
  and a later ticket maps it to a real filter (e.g. `size:>1mb`) where case may matter.
- Empty/whitespace-only query → `Node::And(vec![])`, which `eval`s to `true` (matches everything) via
  `Iterator::all` over zero items — no panic.
- Unbalanced parens tolerated, never a parse error: an unclosed `(` auto-closes at EOF (contents run to
  end of input); a stray unmatched `)` is skipped as a no-op and parsing continues, ANDing together
  whatever was parsed before and after it.
- `eval`: `And` = all(), `Or` = any(), `Not` = negation, `Leaf` = the caller's predicate. Verified De Morgan
  (`NOT (a OR b)` == `(NOT a) AND (NOT b)`) across all 4 truth combinations in a parametrised test.

Verification (from `crates/server`):
- `cargo test` — **831 passed; 0 failed** (17 new in `query_group::tests`, no existing test touched or
  broken; `index_query.rs` untouched as required).
- `cargo clippy --all-targets -- -D warnings` — clean.
- `cargo clippy --all-targets --features index -- -D warnings` — clean.
- No new dependencies added; module is std-only, no `std::path`, no `#[cfg]`-gated behavior — safe across
  the 3-OS CI matrix.

Assumptions (none blocking, logged for the user):
1. `OR`/`AND`/`NOT` keywords matched case-insensitively (`or`, `Or`, `OR` all work) — not explicit in the
   ticket but consistent with common query-DSL conventions and the "tolerate gracefully" spirit; leaf text
   itself stays case-preserved.
2. A leaf token never contains `(`/`)` (parens are hard lexical separators regardless of adjacency to a
   word) — matches the ticket's "opaque leaf tokens" framing (e.g. `size:>1mb`, no parens expected).
3. Degenerate empty AND-groups that can arise mid-parse from tolerant recovery (e.g. two `OR`s in a row,
   `a OR OR b`, or a lone stray `)`) fold to the same `And(vec![])` match-everything placeholder as the
   top-level empty query, for one consistent "nothing here" semantics rather than a special-cased error node.

No blockers. Branch `cpe-1062-query-group`, PR opened targeting `main`.

2026-07-25 (workshift Worker, PR #380 review follow-up) — Reviewer found a real bug in UAT: `parse`'s
paren-recursion and NOT-prefix recursion had no depth bound, so adversarial input (e.g.
`"(".repeat(10_000)`) triggered an uncatchable `STATUS_STACK_OVERFLOW` in a release build — worse than a
panic, and a violation of this module's own "tolerant recovery, never panics" contract. Fixed on the same
branch: added `const MAX_DEPTH: usize = 128`, threaded as a `depth` param through
`parse_or`/`parse_and`/`parse_not`/`parse_atom`. Past the cap, a `(` folds into a literal `Leaf("(")`
instead of opening another group, and a `NOT`/`-` is swallowed instead of wrapping again — tolerant
recovery, not a crash, consistent with the existing unbalanced-paren handling. Since `parse` now only ever
produces a depth-bounded tree, `eval`'s recursion is bounded too; `eval` also carries its own independent
`MAX_DEPTH` guard (falls back to permissive `true`) in case a pathological `Node` is ever hand-built outside
`parse`. Added 5 regression tests (10k open parens, 10k close parens, 10k stacked `NOT`s, mixed deep
nesting, depth-under-cap sanity check) — re-verified `cargo test` (836 passed, 0 failed, incl. a
`--release` run matching the reviewer's repro mode) and both clippy modes clean. Pushed as commit 22584a9;
PR #380 updated with a summary comment.
