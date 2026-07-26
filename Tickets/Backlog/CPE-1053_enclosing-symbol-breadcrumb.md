---
id: CPE-1053
title: "Enclosing-symbol breadcrumb — cpe_server::code_breadcrumb (symbol path for a line)"
type: feature
component: Backend
priority: medium
status: Backlog
tags: ready
created: 2026-07-25
epic: CPE-724
depends-on: CPE-1050
---

## Summary
Child of CPE-724 (Code intelligence preview). Given a source file + a line, return the **nested symbol
path** enclosing that line (e.g. `[Class "Server", Method "start"]`) for a jump-to / breadcrumb. Backend-only,
verified by `cargo test` on the 3-OS matrix — no GUI, no user resource. **Depends on CPE-1050** (needs
`code_folds::fold_ranges` for symbol block extents) — dispatch after CPE-1050 merges.

## Design (buildable)
New module `crates/server/src/code_breadcrumb.rs`, registered with `pub mod code_breadcrumb;` in
`crates/server/src/lib.rs`. Reuse the existing `code_outline::{Symbol, SymbolKind, outline}` — do **not**
edit `code_outline.rs`.

```rust
use crate::code_outline::{outline, Symbol};
/// The outermost→innermost chain of symbols whose block contains 1-based `line`.
pub fn enclosing_symbols(source: &str, lang: &str, line: usize) -> Vec<Symbol>
```

Algorithm:
1. `let syms = code_outline::outline(source, lang);` and `let folds = code_folds::fold_ranges(source, lang);`.
2. For each symbol, determine its **block extent**: the fold range whose `start_line == symbol.line` (a
   symbol declared on the opening line of a block), else the smallest fold range that *contains*
   `symbol.line`; if the symbol has no fold range (e.g. a one-line const or a markdown heading), treat its
   extent as `[symbol.line, next-symbol.line - 1]` or to EOF (heading-style span). Pick a single, documented
   rule and test it.
3. Select the symbols whose extent contains `line`, sorted **outermost→innermost** (by containing span:
   larger/earlier start, later end first). Return that chain.
4. A line inside no symbol → empty vec. A top-level line inside exactly one symbol → single-element vec.

Std only; reuses existing modules. No new deps.

## Acceptance Criteria
- [ ] A line inside a nested method returns `[class, method]` (outer→inner order).
- [ ] A top-level line inside one symbol returns that single symbol; a line outside any symbol → empty vec.
- [ ] Ordering is strictly outer→inner; deterministic.
- [ ] `cargo test -p cpe-server` green; clippy `--all-targets -- -D warnings` clean in default and
      `--features index` builds; no new deps.

## Work Log
2026-07-25 (workshift) — Filed by the Product Manager as a CPE-724 slice. Held in Backlog: depends on
CPE-1050 (code_folds) landing first so this doesn't build against an unmerged API.
