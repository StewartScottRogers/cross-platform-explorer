//! Foldable block-range heuristic (CPE-1050, epic CPE-724): given a source file's text + its language,
//! compute the spans a code preview's fold gutter could collapse — brace blocks, Python indentation
//! suites, and Markdown heading sections.
//!
//! Deliberately **heuristic + dependency-free** (no tree-sitter / native grammars — no C build, no
//! bundle cost), mirroring the style of `code_outline.rs`: a private `normalize(lang) -> Lang` dispatch
//! over the same language ids, then a small per-strategy scanner. It won't be byte-perfect for every
//! exotic construct (e.g. JS template-literal interpolation), but it's fast, cross-platform, and O(n)
//! over the source.

/// The kind of a fold range (drives the gutter glyph / default-collapsed heuristics later).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum FoldKind {
    /// A `{`…`}` block (Rust / JS-TS / Go / C-family).
    Block,
    /// A Python (or similarly indentation-delimited) suite.
    Suite,
    /// A Markdown ATX heading's section.
    Section,
}

/// One foldable range. Lines are **1-based and inclusive** on both ends.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FoldRange {
    pub start_line: usize,
    pub end_line: usize,
    pub kind: FoldKind,
}

/// Compute the fold ranges for `source` in language `lang` (a highlighter language id or file
/// extension, e.g. `rust`/`rs`, `typescript`/`ts`/`tsx`/`javascript`/`js`/`jsx`, `python`/`py`, `go`,
/// a C-family id, `markdown`/`md`). An unrecognised language yields an empty vec (never an error), as
/// does empty source.
pub fn fold_ranges(source: &str, lang: &str) -> Vec<FoldRange> {
    match normalize(lang) {
        Lang::Brace => brace_folds(source),
        Lang::Python => python_folds(source),
        Lang::Markdown => markdown_folds(source),
        Lang::Other => Vec::new(),
    }
}

#[derive(Clone, Copy)]
enum Lang {
    /// Rust / JS-TS / Go / C-family — all folded the same way: brace nesting, skipping line/block
    /// comments and string/char literals.
    Brace,
    Python,
    Markdown,
    Other,
}

fn normalize(lang: &str) -> Lang {
    match lang.trim().to_ascii_lowercase().as_str() {
        "rust" | "rs" => Lang::Brace,
        "typescript" | "ts" | "tsx" | "javascript" | "js" | "jsx" | "mjs" | "cjs" => Lang::Brace,
        "go" => Lang::Brace,
        "c" | "h" | "cpp" | "cc" | "cxx" | "hpp" | "c++" | "csharp" | "cs" | "java" | "kotlin" | "kt"
        | "swift" => Lang::Brace,
        "python" | "py" | "pyi" => Lang::Python,
        "markdown" | "md" | "markdn" | "mdown" => Lang::Markdown,
        _ => Lang::Other,
    }
}

/// Brace-nesting fold scan shared by Rust/JS-TS/Go/C-family: tracks `{`…`}` nesting depth char-by-char,
/// ignoring braces inside `//` line comments, `/* */` block comments, and string/char literals (with
/// `\`-escape awareness). A closing `}` on the same line as its opening `{` yields no range; nested
/// braces naturally yield nested (containing) ranges.
fn brace_folds(source: &str) -> Vec<FoldRange> {
    let chars: Vec<char> = source.chars().collect();
    let n = chars.len();
    let mut ranges = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    let mut line = 1usize;
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    let mut string_quote: Option<char> = None;
    let mut escape = false;
    let mut i = 0usize;

    while i < n {
        let c = chars[i];

        if c == '\n' {
            line += 1;
            in_line_comment = false;
            i += 1;
            continue;
        }

        if in_line_comment {
            i += 1;
            continue;
        }

        if in_block_comment {
            if c == '*' && i + 1 < n && chars[i + 1] == '/' {
                in_block_comment = false;
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }

        if let Some(q) = string_quote {
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == q {
                string_quote = None;
            }
            i += 1;
            continue;
        }

        if c == '/' && i + 1 < n && chars[i + 1] == '/' {
            in_line_comment = true;
            i += 2;
            continue;
        }
        if c == '/' && i + 1 < n && chars[i + 1] == '*' {
            in_block_comment = true;
            i += 2;
            continue;
        }
        if c == '"' || c == '`' {
            string_quote = Some(c);
            i += 1;
            continue;
        }
        if c == '\'' {
            // A `'` opens a char-literal string only if a closing `'` follows within a short window on
            // the same line — otherwise it's a Rust lifetime (`'a`) or a stray apostrophe, and braces
            // after it should still count normally.
            let mut j = i + 1;
            let mut esc = false;
            let mut closed = false;
            while j < n && j < i + 4 {
                let cj = chars[j];
                if cj == '\n' {
                    break;
                }
                if cj == '\\' && !esc {
                    esc = true;
                    j += 1;
                    continue;
                }
                if cj == '\'' {
                    closed = true;
                    break;
                }
                esc = false;
                j += 1;
            }
            if closed {
                string_quote = Some('\'');
            }
            i += 1;
            continue;
        }
        if c == '{' {
            stack.push(line);
            i += 1;
            continue;
        }
        if c == '}' {
            if let Some(start) = stack.pop() {
                if line > start {
                    ranges.push(FoldRange { start_line: start, end_line: line, kind: FoldKind::Block });
                }
            }
            i += 1;
            continue;
        }
        i += 1;
    }
    ranges
}

const PY_SUITE_KEYWORDS: [&str; 11] =
    ["def", "class", "if", "for", "while", "with", "try", "elif", "else", "except", "finally"];

/// Does `text` (already trimmed of leading whitespace) open a Python suite? — starts with one of the
/// suite keywords as a whole word, and ends (after trimming trailing whitespace) with `:`.
fn py_is_suite_header(text: &str) -> bool {
    let trimmed_end = text.trim_end();
    if !trimmed_end.ends_with(':') {
        return false;
    }
    for kw in PY_SUITE_KEYWORDS {
        if let Some(rest) = text.strip_prefix(kw) {
            if rest.is_empty() || !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_') {
                return true;
            }
        }
    }
    false
}

fn leading_ws_count(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// Python indentation-suite fold scan: a `def`/`class`/`if`/… header line ending in `:` folds from its
/// line to the last following line more-indented than the header (blank lines don't end a suite).
/// Nested headers (e.g. a `def` inside a `class`) naturally yield nested (containing) ranges.
fn python_folds(source: &str) -> Vec<FoldRange> {
    let mut ranges = Vec::new();
    // (start_line, header_indent)
    let mut stack: Vec<(usize, usize)> = Vec::new();
    let mut last_nonblank = 0usize;

    for (i, raw) in source.lines().enumerate() {
        let line_no = i + 1;
        if raw.trim().is_empty() {
            continue; // blank lines don't end a suite and aren't headers
        }
        let indent = leading_ws_count(raw);
        while let Some(&(start, hi)) = stack.last() {
            if indent <= hi {
                stack.pop();
                if last_nonblank > start {
                    ranges.push(FoldRange { start_line: start, end_line: last_nonblank, kind: FoldKind::Suite });
                }
            } else {
                break;
            }
        }
        last_nonblank = line_no;
        if py_is_suite_header(raw.trim_start()) {
            stack.push((line_no, indent));
        }
    }
    while let Some((start, _)) = stack.pop() {
        if last_nonblank > start {
            ranges.push(FoldRange { start_line: start, end_line: last_nonblank, kind: FoldKind::Suite });
        }
    }
    ranges
}

/// An ATX heading's level (1..=6), or `None` if `raw` isn't a heading line.
fn markdown_heading_level(raw: &str) -> Option<usize> {
    let s = raw.trim_start();
    if !s.starts_with('#') {
        return None;
    }
    let level = s.chars().take_while(|c| *c == '#').count();
    let text = s[level..].trim();
    ((1..=6).contains(&level) && !text.is_empty()).then_some(level)
}

/// Markdown section fold scan: an ATX heading folds from its line to just before the next heading of
/// same-or-higher level (or EOF). Nested (deeper) headings naturally yield nested (containing) ranges.
fn markdown_folds(source: &str) -> Vec<FoldRange> {
    let mut ranges = Vec::new();
    // (start_line, level)
    let mut stack: Vec<(usize, usize)> = Vec::new();
    let mut total_lines = 0usize;

    for (i, raw) in source.lines().enumerate() {
        let line_no = i + 1;
        total_lines = line_no;
        if let Some(level) = markdown_heading_level(raw) {
            while let Some(&(start, lvl)) = stack.last() {
                if level <= lvl {
                    stack.pop();
                    let end = line_no - 1;
                    if end > start {
                        ranges.push(FoldRange { start_line: start, end_line: end, kind: FoldKind::Section });
                    }
                } else {
                    break;
                }
            }
            stack.push((line_no, level));
        }
    }
    while let Some((start, _)) = stack.pop() {
        if total_lines > start {
            ranges.push(FoldRange { start_line: start, end_line: total_lines, kind: FoldKind::Section });
        }
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans(source: &str, lang: &str) -> Vec<(usize, usize, FoldKind)> {
        fold_ranges(source, lang).into_iter().map(|f| (f.start_line, f.end_line, f.kind)).collect()
    }

    #[test]
    fn rust_function_body_folds() {
        let src = "fn run() {\n    let x = 1;\n    println!(\"{}\", x);\n}\n";
        let got = spans(src, "rust");
        assert_eq!(got, vec![(1, 4, FoldKind::Block)]);
    }

    #[test]
    fn js_function_body_folds() {
        let src = "function run() {\n  doThing();\n}\n";
        let got = spans(src, "js");
        assert_eq!(got, vec![(1, 3, FoldKind::Block)]);
    }

    #[test]
    fn nested_blocks_yield_nested_containing_ranges() {
        let src = "\
fn outer() {
    if true {
        inner();
    }
}
";
        let got = spans(src, "rust");
        // Outer spans 1..5, inner spans 2..4 — both present, outer contains inner.
        assert!(got.contains(&(1, 5, FoldKind::Block)));
        assert!(got.contains(&(2, 4, FoldKind::Block)));
        assert_eq!(got.len(), 2);
    }

    #[test]
    fn one_line_braces_yield_no_range() {
        assert_eq!(spans("fn run() {}\n", "rust"), vec![]);
        assert_eq!(spans("if (x) { y(); }\n", "js"), vec![]);
    }

    #[test]
    fn braces_in_line_comment_are_not_counted() {
        let src = "fn run() {\n    // a brace in a comment: { } does not open a block\n    let x = 1;\n}\n";
        let got = spans(src, "rust");
        assert_eq!(got, vec![(1, 4, FoldKind::Block)]);
    }

    #[test]
    fn braces_in_block_comment_are_not_counted() {
        let src = "fn run() {\n    /* a brace in a block comment: { } */\n    let x = 1;\n}\n";
        let got = spans(src, "rust");
        assert_eq!(got, vec![(1, 4, FoldKind::Block)]);
    }

    #[test]
    fn braces_in_string_literal_are_not_counted() {
        let src = "fn run() {\n    let s = \"a { brace } in a string\";\n    let c = '{';\n}\n";
        let got = spans(src, "rust");
        assert_eq!(got, vec![(1, 4, FoldKind::Block)]);
    }

    #[test]
    fn rust_lifetimes_do_not_confuse_the_char_literal_scanner() {
        let src = "fn run<'a>(x: &'a str) {\n    let c = 'x';\n    let y = 1;\n}\n";
        let got = spans(src, "rust");
        assert_eq!(got, vec![(1, 4, FoldKind::Block)]);
    }

    #[test]
    fn go_and_c_family_brace_blocks_fold() {
        let go_src = "func main() {\n\tdoThing()\n}\n";
        assert_eq!(spans(go_src, "go"), vec![(1, 3, FoldKind::Block)]);

        let java_src = "class Main {\n    void run() {\n        doThing();\n    }\n}\n";
        let got = spans(java_src, "java");
        assert!(got.contains(&(1, 5, FoldKind::Block)));
        assert!(got.contains(&(2, 4, FoldKind::Block)));
    }

    #[test]
    fn python_class_and_def_suites_fold_by_indentation() {
        let src = "\
class Server:
    def start(self):
        setup()
        run()

    def stop(self):
        teardown()

x = 1
";
        let got = spans(src, "python");
        // class Server: header line 1, body runs through the last indented line (7).
        assert!(got.contains(&(1, 7, FoldKind::Suite)));
        // def start suite: lines 2..4 (setup() and run() are both in its body).
        assert!(got.contains(&(2, 4, FoldKind::Suite)));
        // def stop suite: lines 6..7.
        assert!(got.contains(&(6, 7, FoldKind::Suite)));
        assert_eq!(got.len(), 3);
    }

    #[test]
    fn python_blank_lines_do_not_end_a_suite() {
        let src = "def run():\n    a()\n\n    b()\n";
        let got = spans(src, "py");
        assert_eq!(got, vec![(1, 4, FoldKind::Suite)]);
    }

    #[test]
    fn python_header_with_no_indented_body_yields_no_range() {
        // A header with nothing indented beneath it (EOF right after) has no fold-worthy extent.
        assert_eq!(spans("def run():\n", "python"), vec![]);
        // A one-line `def ...: pass` never ends its line with `:`, so it isn't even a header.
        assert_eq!(spans("def run(): pass\nx = 1\n", "python"), vec![]);
    }

    #[test]
    fn markdown_section_folds_to_next_heading() {
        let src = "# Title\n\nintro text\n\n## Section\nbody line\nmore body\n\n## Section Two\nother body\n";
        let got = spans(src, "markdown");
        // "# Title" is level 1: only another level-1 (or higher) heading would end it, and there isn't
        // one, so it runs to EOF.
        assert!(got.contains(&(1, 10, FoldKind::Section)));
        // "## Section" is level 2: the next level-<=2 heading ("## Section Two", line 9) ends it.
        assert!(got.contains(&(5, 8, FoldKind::Section)));
        // "## Section Two" runs to EOF (no heading follows it).
        assert!(got.contains(&(9, 10, FoldKind::Section)));
        assert_eq!(got.len(), 3);
    }

    #[test]
    fn markdown_adjacent_same_level_heading_yields_no_range() {
        // A heading immediately followed by another of the same-or-higher level has an empty section.
        let src = "# One\n# Two\nbody\n";
        let got = spans(src, "md");
        assert!(!got.iter().any(|(s, ..)| *s == 1), "heading 1 has no content before heading 2");
        assert_eq!(got, vec![(2, 3, FoldKind::Section)]);
    }

    #[test]
    fn markdown_deeper_heading_does_not_end_a_shallower_section() {
        let src = "# Title\n## Section\nbody\n";
        let got = spans(src, "md");
        // "## Section" (level 2) does not end "# Title" (level 1); the level-1 section runs to EOF.
        assert!(got.contains(&(1, 3, FoldKind::Section)));
        assert!(got.contains(&(2, 3, FoldKind::Section)));
        assert_eq!(got.len(), 2);
    }

    #[test]
    fn unknown_language_is_empty() {
        assert_eq!(fold_ranges("fn x() { { } }", "brainfuck"), vec![]);
    }

    #[test]
    fn empty_source_is_empty_for_every_language() {
        assert_eq!(fold_ranges("", "rust"), vec![]);
        assert_eq!(fold_ranges("", "python"), vec![]);
        assert_eq!(fold_ranges("", "markdown"), vec![]);
        assert_eq!(fold_ranges("", "brainfuck"), vec![]);
    }
}
