//! Aggregate code-intelligence bundle (CPE-1089, epic CPE-724): given a source file's text, its
//! language, a tab width, and a minimap bucket count, compose the four existing pure code-intel
//! heuristics into one payload — outline, fold ranges, indent depths, and minimap rows — so a code
//! preview can fetch everything it needs in a single round trip instead of four.
//!
//! Built entirely on the existing modules, without editing any of them:
//! - [`crate::code_outline::outline`] — flat symbol list (name/kind/declaration line).
//! - [`crate::code_folds::fold_ranges`] — foldable block extents (1-based, inclusive).
//! - [`crate::indent_guides::indent_levels`] — per-line indent-guide depth.
//! - [`crate::minimap::minimap_rows`] — downsampled minimap density rows.
//!
//! `code_breadcrumb::enclosing_symbols` is deliberately **not** folded in here — the frontend can derive
//! the breadcrumb client-side from `outline` + `folds` on cursor move, saving a round-trip per keystroke.

use crate::code_folds::{self, FoldRange};
use crate::code_outline::{self, Symbol};
use crate::indent_guides;
use crate::minimap::{self, MinimapRow};

/// Upper bound on `minimap_buckets` so a caller-supplied value can never drive an unbounded allocation.
pub const MAX_MINIMAP_BUCKETS: usize = 512;

/// The aggregate code-intelligence bundle for one file's text.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CodeIntel {
    pub outline: Vec<Symbol>,
    pub folds: Vec<FoldRange>,
    pub indent: Vec<u16>,
    pub minimap: Vec<MinimapRow>,
}

/// Compute the full code-intel bundle for `text` in language `lang`.
///
/// `tab_width` is forwarded to [`indent_guides::indent_levels`] as-is (it already treats `0` as `1`).
/// `minimap_buckets` is clamped to [`MAX_MINIMAP_BUCKETS`] before reaching [`minimap::minimap_rows`], so
/// a silly caller-supplied value (e.g. `usize::MAX`) can't force a huge allocation. Empty `text` yields
/// an all-empty `CodeIntel` — every composed function already treats empty source as empty output, so
/// this never panics.
pub fn analyze(text: &str, lang: &str, tab_width: usize, minimap_buckets: usize) -> CodeIntel {
    let buckets = minimap_buckets.min(MAX_MINIMAP_BUCKETS);
    CodeIntel {
        outline: code_outline::outline(text, lang),
        folds: code_folds::fold_ranges(text, lang),
        indent: indent_guides::indent_levels(text, tab_width),
        minimap: minimap::minimap_rows(text, buckets),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_empty_rust_text_populates_every_field() {
        let src = "\
pub fn run() {
    let x = 1;
    println!(\"{}\", x);
}
";
        let got = analyze(src, "rust", 4, 8);
        assert_eq!(got.outline.len(), 1);
        assert_eq!(got.outline[0].name, "run");
        assert_eq!(got.folds.len(), 1);
        assert_eq!(got.folds[0].start_line, 1);
        assert_eq!(got.indent.len(), src.lines().count());
        assert!(got.indent.iter().any(|&d| d > 0));
        assert!(!got.minimap.is_empty());
    }

    #[test]
    fn empty_text_is_an_all_empty_bundle_no_panic() {
        let got = analyze("", "rust", 4, 120);
        assert_eq!(
            got,
            CodeIntel { outline: Vec::new(), folds: Vec::new(), indent: Vec::new(), minimap: Vec::new() }
        );
    }

    #[test]
    fn huge_minimap_buckets_is_clamped_no_panic() {
        let src = (1..=20).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        let got = analyze(&src, "text", 4, 100_000);
        assert!(got.minimap.len() <= MAX_MINIMAP_BUCKETS);
        // With only 20 lines, the composed minimap_rows never invents more rows than lines exist either.
        assert_eq!(got.minimap.len(), 20);
    }

    #[test]
    fn minimap_buckets_above_cap_but_below_line_count_is_still_clamped() {
        // 1000 lines, ask for way more than MAX_MINIMAP_BUCKETS -> clamped to the cap, not 1000.
        let src = (1..=1000).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        let got = analyze(&src, "text", 4, 50_000);
        assert_eq!(got.minimap.len(), MAX_MINIMAP_BUCKETS);
    }

    #[test]
    fn unknown_language_still_yields_indent_and_minimap() {
        // outline/folds are empty for an unrecognised language, but indent + minimap are language-agnostic.
        let src = "a\n    b\n";
        let got = analyze(src, "brainfuck", 4, 10);
        assert!(got.outline.is_empty());
        assert!(got.folds.is_empty());
        assert_eq!(got.indent, vec![0, 1]);
        assert_eq!(got.minimap.len(), 2);
    }
}
