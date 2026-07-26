---
id: CPE-1062
title: "Search boolean grouping — cpe_server::query_group (OR / NOT / parentheses)"
type: feature
component: Backend
priority: high
status: Doing
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
