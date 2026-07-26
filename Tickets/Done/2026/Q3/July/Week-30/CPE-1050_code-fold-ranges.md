---
id: CPE-1050
title: "Code fold ranges — cpe_server::code_folds (foldable block spans)"
type: feature
component: Backend
priority: medium
status: Done
tags: ready
created: 2026-07-25
epic: CPE-724
estimate: 3h
---

## Summary
Child of CPE-724 (Code intelligence preview). Add a **pure, dependency-free** heuristic that computes the
foldable block ranges of a source file, so the future preview gutter can draw fold controls. Backend-only,
verified by `cargo test` on the 3-OS matrix — no GUI, no user resource.

## Design (buildable)
New module `crates/server/src/code_folds.rs`, registered with a one-line `pub mod code_folds;` in
`crates/server/src/lib.rs` (alongside `pub mod code_outline;` at line ~53).

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum FoldKind { Block, Suite, Section }   // brace block / python suite / markdown section

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FoldRange { pub start_line: usize, pub end_line: usize, pub kind: FoldKind } // 1-based inclusive

pub fn fold_ranges(source: &str, lang: &str) -> Vec<FoldRange>
```

Reuse the **same `Lang` dispatch style as `code_outline.rs`** (a private `normalize(lang) -> Lang`, matching
the exact same language ids: rust/rs, ts/tsx/js/jsx, python/py, go, c-family, markdown/md). Strategies:
- **Brace languages** (Rust / JS-TS / Go / C-family): scan char-by-char tracking `{`…`}` nesting depth;
  when a `}` closes a `{` that opened on an *earlier* line, emit a `FoldRange{Block}` from the opening line
  to the closing line (only if `end_line > start_line`). **Ignore braces inside line comments** (`//`, and
  `#` for langs that use it) **and inside string/char literals** — a light scanner: track "in string"
  (`"`/`'`/backtick, honoring `\` escapes) and "in line comment" (until newline). Nested blocks naturally
  yield nested (containing) ranges.
- **Python**: indentation strategy — a line ending in `:` whose keyword is `def`/`class`/`if`/`for`/`while`/
  `with`/`try`/`elif`/`else`/`except`/`finally` opens a suite that folds from that header to the last
  following line more-indented than the header (blank lines don't end a suite). Emit `FoldKind::Suite`.
- **Markdown**: an ATX heading (`#`..`######`) folds from its line to just before the next heading of
  same-or-higher level (or EOF). Emit `FoldKind::Section`.
- Unknown language → empty vec.

Keep it O(n) over the source; no new dependencies (std only + serde/specta already in the crate).

## Acceptance Criteria
- [ ] `fold_ranges` returns correct 1-based inclusive spans for: a multi-line Rust/JS function body
      (`{`→`}`); **nested** blocks producing nested (containing) ranges; a Python `class`/`def` suite folding
      by indentation; a Markdown section folding to the next heading.
- [ ] A one-line `{}` (or a block that starts+ends on the same line) yields **no** range.
- [ ] Braces inside `// comment` and inside a string literal are **not** counted.
- [ ] Unknown language → empty vec; empty source → empty vec (no panic).
- [ ] `cargo test -p cpe-server` (run from `crates/server`) green; `cargo clippy --all-targets -- -D warnings`
      clean in **both** default and `--features index` builds; no new deps.

## Work Log
2026-07-25 (workshift) — Filed by the Product Manager as a clean headless CPE-724 slice. Independent of the
other children except a one-line lib.rs `pub mod` (serial-merge coordination only).

2026-07-25 (workshift, Worker) — Implemented `crates/server/src/code_folds.rs` (`FoldKind`, `FoldRange`,
`fold_ranges`), registered via `pub mod code_folds;` immediately after `pub mod code_outline;` in
`crates/server/src/lib.rs`. Mirrored `code_outline.rs`'s derive stack and `normalize(lang) -> Lang`
dispatch style, but collapsed Rust/JS-TS/Go/C-family into one `Lang::Brace` variant since they share the
same brace-nesting fold strategy (one scanner, not four copies).
Assumptions logged (none blocking, no clarification needed):
- Also skip `/* */` block comments (not explicitly required by the acceptance criteria, only `//` line
  comments + string/char literals), since it's a one-off, low-risk addition that avoids an obvious false
  positive in Rust/C-family doc comments.
- A `'` is only treated as opening a char-literal string if a closing `'` appears within a short
  lookahead window on the same line; otherwise (e.g. a Rust lifetime like `'a`) it's left alone so it
  doesn't corrupt subsequent brace counting. Added a regression test for this
  (`rust_lifetimes_do_not_confuse_the_char_literal_scanner`).
- Python/Markdown fold extents use a stack (O(n)) rather than a per-header forward scan, so nested
  suites/sections fall out naturally without being asked for explicitly beyond the stated criteria.
Verified from `crates/server`: `cargo test` → 744 passed (17 new in `code_folds`), 0 failed; no Defender
os-error-225 hit. `cargo clippy --all-targets -- -D warnings` clean; `cargo clippy --all-targets --features
index -- -D warnings` clean. No new dependencies added to `Cargo.toml`.
