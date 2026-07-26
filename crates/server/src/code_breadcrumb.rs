//! Enclosing-symbol breadcrumb (CPE-1053, epic CPE-724): given a source file's text + its language +
//! a 1-based line, return the **nested symbol chain** (outermost→innermost) whose block contains that
//! line — e.g. `[Class "Server", Method "start"]` — for a jump-to / breadcrumb UI over a code preview.
//!
//! Built entirely on the two existing heuristics, without editing either:
//! - [`crate::code_outline::outline`] — the flat list of symbols (name/kind/declaration line).
//! - [`crate::code_folds::fold_ranges`] — the foldable block extents (1-based, inclusive).
//!
//! No new dependency-free scanning is added here; this module only *combines* the two.

use crate::code_folds::{fold_ranges, FoldRange};
use crate::code_outline::{outline, Symbol};

/// The outermost→innermost chain of symbols whose block contains 1-based `line`.
///
/// # Block-extent rule
/// Each symbol's block extent is computed independently, in this priority order:
/// 1. **Direct match** — the fold range whose `start_line == symbol.line` (the common case: a symbol is
///    declared on the same line its block opens, e.g. `fn foo() {`, `class Foo:`, `## Heading`). If more
///    than one fold range opens on that exact line (e.g. two `{` on one physical line), the one with the
///    *largest* `end_line` wins, since that is the block that actually belongs to the declaration rather
///    than an inner block that happens to open on the same line.
/// 2. **Smallest containing fold** — if no fold starts exactly on the symbol's line (e.g. a multi-line
///    signature whose `{` lands on a later line), fall back to the smallest fold range that contains the
///    symbol's line at all.
/// 3. **Next-symbol fallback** — if the symbol has no fold range at all (a one-line `const`, a Markdown
///    heading with no `code_folds` support for its language, an unsupported/`Other` language for
///    `code_folds`, …), its extent runs from its own line up to (but not including) the next symbol's
///    line, or to end-of-file if it is the last symbol.
///
/// # Selection + ordering
/// Symbols whose extent contains `line` are kept and sorted **outermost→innermost** (ascending extent
/// start line; ties broken by descending extent end line, i.e. the wider span first). A line inside no
/// symbol yields an empty vec; a top-level line inside exactly one symbol yields a single-element vec.
///
/// Std only; purely a combination of `code_outline` + `code_folds` — no new heuristics, no new deps.
pub fn enclosing_symbols(source: &str, lang: &str, line: usize) -> Vec<Symbol> {
    let syms = outline(source, lang);
    if syms.is_empty() {
        return Vec::new();
    }
    let folds = fold_ranges(source, lang);
    let total_lines = source.lines().count().max(1);

    let extents: Vec<(usize, usize)> = syms
        .iter()
        .enumerate()
        .map(|(i, sym)| block_extent(sym.line, &folds, &syms, i, total_lines))
        .collect();

    let mut chain: Vec<usize> =
        (0..syms.len()).filter(|&i| extents[i].0 <= line && line <= extents[i].1).collect();
    chain.sort_by(|&a, &b| extents[a].0.cmp(&extents[b].0).then(extents[b].1.cmp(&extents[a].1)));
    chain.into_iter().map(|i| syms[i].clone()).collect()
}

/// Compute one symbol's `(start_line, end_line)` block extent per the rule documented on
/// [`enclosing_symbols`]. `idx` is the symbol's index into `syms` (used for the next-symbol fallback).
fn block_extent(
    sym_line: usize,
    folds: &[FoldRange],
    syms: &[Symbol],
    idx: usize,
    total_lines: usize,
) -> (usize, usize) {
    // Rule 1: a fold opening exactly on the symbol's declaration line — widest such fold wins.
    if let Some(f) = folds.iter().filter(|f| f.start_line == sym_line).max_by_key(|f| f.end_line) {
        return (f.start_line, f.end_line);
    }
    // Rule 2: the smallest fold range that contains the symbol's line at all — but only if it doesn't
    // also swallow a sibling. A fold-less symbol (e.g. a multi-line signature with a one-line body, so
    // `code_folds`' brace scanner never opens a range for it) sitting inside a larger block will always
    // find that block's own fold "containing" its line; accepting it unconditionally would hand the
    // symbol its ancestor's whole span, wrongly pulling in any sibling declared later in that same span.
    // So a candidate fold is only accepted if no OTHER symbol is declared strictly after this one and at
    // or before the fold's end — otherwise it isn't really *this* symbol's own extent, and we fall
    // through to rule 3's sibling-capped fallback instead.
    if let Some(f) = folds
        .iter()
        .filter(|f| f.start_line <= sym_line && sym_line <= f.end_line)
        .filter(|f| !syms.iter().any(|s| s.line > sym_line && s.line <= f.end_line))
        .min_by_key(|f| f.end_line - f.start_line)
    {
        return (f.start_line, f.end_line);
    }
    // Rule 3: no fold covers this symbol at all — span to just before the next symbol, or to EOF.
    let end = syms.get(idx + 1).map(|next| next.line.saturating_sub(1)).unwrap_or(total_lines).max(sym_line);
    (sym_line, end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code_outline::SymbolKind;

    fn names(chain: &[Symbol]) -> Vec<(&str, SymbolKind)> {
        chain.iter().map(|s| (s.name.as_str(), s.kind)).collect()
    }

    #[test]
    fn nested_method_returns_outer_then_inner() {
        // struct Config@1 is a one-liner (no fold; fallback spans just line 1). impl Config@3 and
        // fn start@4 both open their block on their own declaration line, so both get a direct-match
        // fold extent, with impl's fold strictly containing fn's.
        let src = "\
struct Config;

impl Config {
    fn start(&self) {
        let x = 1;
    }
}
";
        let got = enclosing_symbols(src, "rust", 5); // inside `let x = 1;`
        assert_eq!(names(&got), vec![("Config", SymbolKind::Struct), ("start", SymbolKind::Function)]);
    }

    #[test]
    fn foldless_sibling_with_multiline_signature_does_not_swallow_the_next_sibling() {
        // Regression: `fn a` has a multi-line signature whose `{ x }` opens and closes on the same
        // physical line, so `code_folds` never emits a range for it. Rule 2 must not fall back to the
        // enclosing `impl`'s fold (which also spans `fn b`) — that would wrongly report `a` as
        // enclosing line 7, which is inside `fn b` only.
        let src = "\
impl Foo {
    fn a(
        x: i32,
    ) -> i32 { x }

    fn b(&self) {
        y()
    }
}
";
        let got = enclosing_symbols(src, "rust", 7); // inside `y()`, within `fn b` only
        assert_eq!(names(&got), vec![("Foo", SymbolKind::Struct), ("b", SymbolKind::Function)]);
        assert!(!got.iter().any(|s| s.name == "a"), "fn a must not appear: {:?}", got);
    }

    #[test]
    fn python_class_and_method_outer_then_inner() {
        let src = "\
class Server:
    def start(self):
        setup()
        run()

    def stop(self):
        teardown()
";
        let got = enclosing_symbols(src, "python", 3); // inside `setup()`, within `start`, within `Server`
        assert_eq!(names(&got), vec![("Server", SymbolKind::Class), ("start", SymbolKind::Method)]);

        // A line inside `stop`'s body must not also report `start`.
        let got2 = enclosing_symbols(src, "python", 7);
        assert_eq!(names(&got2), vec![("Server", SymbolKind::Class), ("stop", SymbolKind::Method)]);
    }

    #[test]
    fn three_level_nesting_is_strictly_outer_to_inner() {
        let src = "\
class Outer:
    def outer_method(self):
        def inner():
            pass
        return inner
";
        let got = enclosing_symbols(src, "python", 4); // inside `pass`, nested two levels deep
        assert_eq!(
            names(&got),
            vec![
                ("Outer", SymbolKind::Class),
                ("outer_method", SymbolKind::Method),
                ("inner", SymbolKind::Method),
            ]
        );
        // Strict outer→inner: the chain's extents must be non-increasing in span as we go deeper.
        let extents: Vec<(usize, usize)> = got
            .iter()
            .map(|s| {
                let syms = outline(src, "python");
                let folds = fold_ranges(src, "python");
                let idx = syms.iter().position(|x| x.line == s.line).unwrap();
                block_extent(s.line, &folds, &syms, idx, src.lines().count())
            })
            .collect();
        for w in extents.windows(2) {
            assert!(w[0].0 <= w[1].0 && w[0].1 >= w[1].1, "outer must contain inner: {:?}", extents);
        }
    }

    #[test]
    fn top_level_line_returns_single_symbol() {
        let src = "\
class Server:
    def start(self):
        pass
";
        // Line 1 is the class header itself — only `Server`'s extent contains it, not `start`'s.
        let got = enclosing_symbols(src, "python", 1);
        assert_eq!(names(&got), vec![("Server", SymbolKind::Class)]);
    }

    #[test]
    fn line_outside_any_symbol_is_empty() {
        let src = "// a leading comment, not part of any symbol\nfn helper() {}\n";
        let got = enclosing_symbols(src, "rust", 1);
        assert!(got.is_empty());
    }

    #[test]
    fn line_beyond_last_symbol_with_no_fold_is_empty() {
        // `helper` is a one-liner with no fold; its rule-3 fallback extent is just its own line (1),
        // since it's both the last symbol and the last line of the file. Line 5 is out of range.
        let src = "fn helper() {}\n";
        assert!(enclosing_symbols(src, "rust", 5).is_empty());
    }

    #[test]
    fn no_symbols_at_all_is_empty() {
        assert!(enclosing_symbols("just plain text\nmore text\n", "markdown", 1).is_empty());
        assert!(enclosing_symbols("fn x() {}\n", "brainfuck", 1).is_empty());
    }

    #[test]
    fn result_is_deterministic_across_repeated_calls() {
        let src = "\
class Server:
    def start(self):
        pass
";
        let a = enclosing_symbols(src, "python", 3);
        let b = enclosing_symbols(src, "python", 3);
        assert_eq!(a, b);
    }

    #[test]
    fn markdown_heading_sections_nest_by_level() {
        let src = "# Title\n\nintro\n\n## Section\nbody\n";
        let got = enclosing_symbols(src, "markdown", 6); // inside "body", under "## Section" under "# Title"
        assert_eq!(names(&got), vec![("Title", SymbolKind::Heading), ("Section", SymbolKind::Heading)]);
    }
}
