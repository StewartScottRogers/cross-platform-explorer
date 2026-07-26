---
id: CPE-1053
title: "Enclosing-symbol breadcrumb — cpe_server::code_breadcrumb (symbol path for a line)"
type: feature
component: Backend
priority: medium
status: Done
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
- [x] A line inside a nested method returns `[class, method]` (outer→inner order).
- [x] A top-level line inside one symbol returns that single symbol; a line outside any symbol → empty vec.
- [x] Ordering is strictly outer→inner; deterministic.
- [x] `cargo test -p cpe-server` green; clippy `--all-targets -- -D warnings` clean in default and
      `--features index` builds; no new deps.

## Work Log
2026-07-25 (workshift) — Filed by the Product Manager as a CPE-724 slice. Held in Backlog: depends on
CPE-1050 (code_folds) landing first so this doesn't build against an unmerged API.

2026-07-25 (workshift, Worker) — Built `crates/server/src/code_breadcrumb.rs` on top of merged
`code_outline`/`code_folds`. Registered `pub mod code_breadcrumb;` in `lib.rs` immediately after
`pub mod code_folds;`. Std only, no new deps.

**Block-extent rule chosen (documented as a doc-comment on `enclosing_symbols`):**
1. Direct match — the fold range whose `start_line == symbol.line` (widest such fold wins on a tie,
   e.g. two `{` opening on the same physical line).
2. Else the smallest fold range that contains the symbol's line at all (covers a multi-line signature
   whose `{` lands on a later line than the declaration, or a nested one-liner sitting inside an outer
   block with no fold of its own — in that case its extent widens to the smallest enclosing fold, which
   is an accepted heuristic limitation, not a bug).
3. Else (no fold at all — a one-line `const`, or a language `code_folds` doesn't support, e.g. Ruby) —
   span from the symbol's own line to just before the next symbol's line, or to EOF if it's the last
   symbol.

Selected symbols (extent contains the query line) are sorted outermost→innermost by ascending extent
start (ties broken by descending extent end).

**Assumption logged:** rule 2's "smallest containing fold" can, for a childless one-line symbol nested
inside a larger block with no fold of its own, report the *same* extent as its enclosing parent (since
the only fold containing it is the parent's). This matches the ticket's literal rule 2 wording and is a
known, accepted approximation of the same style as `code_outline`/`code_folds` (heuristic, not
tree-sitter-exact) — not exercised by the acceptance-criteria fixtures, which use direct-match cases.

**Verification:**
- `cargo test` (from `crates/server`): 774 passed, 0 failed (9 new `code_breadcrumb` tests: nested
  Rust struct/impl/fn, Python class/method x2, 3-level Python nesting with an extent-containment
  assertion, top-level single-symbol, line-outside-any-symbol empty, EOF-boundary empty, no-symbols
  empty (both markdown-with-no-headings and unknown-language), determinism, Markdown heading nesting).
- `cargo clippy --all-targets -- -D warnings` — clean.
- `cargo clippy --all-targets --features index -- -D warnings` — clean.
- No new dependencies added.

Branch `cpe-1053-code-breadcrumb`, PR opened against `main`.

2026-07-25 (workshift, Worker) — PR #371 got CHANGES REQUESTED from the independent reviewer: a real
correctness bug, not the limitation logged above. Rule 2's "smallest containing fold" could swallow an
unrelated **sibling**: a symbol with a multi-line signature + one-line body (e.g. `fn a(\n x: i32,\n) ->
i32 { x }`) gets no fold of its own (the brace scanner never opens a range for a same-line `{...}`), so
rule 2 fell back to the enclosing `impl`'s fold — which also spans a later sibling (`fn b`) — and
wrongly reported `a` as enclosing lines that were actually inside `b` only.

**Fix:** rule 2 now only accepts a containing fold if no *other* symbol is declared strictly after this
symbol's line and at-or-before that fold's end line; otherwise the fold isn't really this symbol's own
extent (it belongs to an ancestor that also contains a sibling), and we fall through to rule 3's
next-symbol-capped fallback instead. Added regression test
`foldless_sibling_with_multiline_signature_does_not_swallow_the_next_sibling` reproducing the reviewer's
exact repro (asserts line 7 → `["Foo", "b"]`, and that `"a"` is absent).

**Re-verification:**
- `cargo test` (from `crates/server`): 775 passed, 0 failed (10 `code_breadcrumb` tests now, incl. the
  new regression test).
- `cargo clippy --all-targets -- -D warnings` — clean.
- `cargo clippy --all-targets --features index -- -D warnings` — clean.
- No new dependencies added; `code_outline.rs`/`code_folds.rs` untouched.

Pushed fix to `cpe-1053-code-breadcrumb` — PR #371 updated.
