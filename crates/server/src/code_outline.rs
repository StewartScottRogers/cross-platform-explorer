//! Lightweight source-symbol outline (CPE-910, epic CPE-724): given a source file's text + its language,
//! produce a flat list of top-level (and one-level-nested) symbols — functions, types, classes, headings —
//! each with its 1-based line, for a jump-to-symbol outline / breadcrumb over the existing highlighter.
//!
//! Deliberately **heuristic + dependency-free** (no tree-sitter / native grammars — no C build, no bundle
//! cost): a per-language line scanner that recognises the common declaration forms. It won't match every
//! exotic construct, but it's fast, cross-platform, and covers the top languages developers actually
//! browse here. Richer per-language coverage can layer on without changing the shape.

/// The kind of a source symbol (drives the outline icon).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Function,
    Method,
    Struct,
    Enum,
    Trait,
    Interface,
    Class,
    Module,
    Constant,
    TypeAlias,
    Heading,
}

/// One symbol in the outline.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    /// 1-based source line.
    pub line: usize,
}

/// Extract the outline for `source` in language `lang` (a highlighter language id or file extension, e.g.
/// `rust`/`rs`, `typescript`/`ts`/`tsx`/`js`/`jsx`, `python`/`py`, `go`, `markdown`/`md`). An unrecognised
/// language yields an empty outline (never an error).
pub fn outline(source: &str, lang: &str) -> Vec<Symbol> {
    let lang = normalize(lang);
    let mut out = Vec::new();
    for (i, raw) in source.lines().enumerate() {
        let line = i + 1;
        let scan = match lang {
            Lang::Rust => scan_rust,
            Lang::Js => scan_js,
            Lang::Python => scan_python,
            Lang::Go => scan_go,
            Lang::Ruby => scan_ruby,
            Lang::Php => scan_php,
            Lang::Clike => scan_clike,
            Lang::Markdown => scan_markdown,
            Lang::Other => return Vec::new(),
        };
        if let Some((name, kind)) = scan(raw) {
            out.push(Symbol { name, kind, line });
        }
    }
    out
}

#[derive(Clone, Copy)]
enum Lang {
    Rust,
    Js,
    Python,
    Go,
    Ruby,
    Php,
    /// C / C++ / C# / Java / Kotlin / Swift — keyword-declared **types** (functions/methods in these are
    /// keyword-less and too ambiguous to detect heuristically, so they're intentionally skipped).
    Clike,
    Markdown,
    Other,
}

fn normalize(lang: &str) -> Lang {
    match lang.trim().to_ascii_lowercase().as_str() {
        "rust" | "rs" => Lang::Rust,
        "typescript" | "ts" | "tsx" | "javascript" | "js" | "jsx" | "mjs" | "cjs" => Lang::Js,
        "python" | "py" | "pyi" => Lang::Python,
        "go" => Lang::Go,
        "ruby" | "rb" => Lang::Ruby,
        "php" => Lang::Php,
        "c" | "h" | "cpp" | "cc" | "cxx" | "hpp" | "c++" | "csharp" | "cs" | "java" | "kotlin" | "kt"
        | "swift" => Lang::Clike,
        "markdown" | "md" | "markdn" | "mdown" => Lang::Markdown,
        _ => Lang::Other,
    }
}

/// The identifier immediately following `kw` (which must be a whole word — followed by whitespace), or
/// `None`. E.g. `ident_after("fn  foo(x)", "fn") == Some("foo")`.
fn ident_after(s: &str, kw: &str) -> Option<String> {
    let rest = s.strip_prefix(kw)?;
    if !rest.starts_with(|c: char| c.is_whitespace()) {
        return None;
    }
    let name: String = rest.trim_start().chars().take_while(is_ident_char).collect();
    (!name.is_empty()).then_some(name)
}

fn is_ident_char(c: &char) -> bool {
    c.is_alphanumeric() || *c == '_'
}

/// Strip a leading Rust visibility (`pub`, `pub(crate)`, …) + `async`/`unsafe`/`const`/`default`
/// modifiers, returning the declaration keyword onward.
fn strip_rust_modifiers(line: &str) -> &str {
    let mut s = line.trim_start();
    if let Some(rest) = s.strip_prefix("pub") {
        s = rest.trim_start();
        if let Some(open) = s.strip_prefix('(') {
            if let Some(i) = open.find(')') {
                s = open[i + 1..].trim_start();
            }
        }
    }
    for m in ["default ", "async ", "unsafe ", "extern "] {
        if let Some(rest) = s.strip_prefix(m) {
            s = rest.trim_start();
        }
    }
    // `const fn` / `const N` — leave `const` for the const/fn matchers; but skip a plain `const ` before fn.
    s
}

fn scan_rust(raw: &str) -> Option<(String, SymbolKind)> {
    let s = strip_rust_modifiers(raw);
    // `const fn foo` → treat as a function.
    let s2 = s.strip_prefix("const ").map(str::trim_start).unwrap_or(s);
    for (kw, kind) in [
        ("fn", SymbolKind::Function),
        ("struct", SymbolKind::Struct),
        ("enum", SymbolKind::Enum),
        ("trait", SymbolKind::Trait),
        ("mod", SymbolKind::Module),
        ("type", SymbolKind::TypeAlias),
        ("union", SymbolKind::Struct),
    ] {
        if let Some(name) = ident_after(s2, kw) {
            return Some((name, kind));
        }
    }
    // `impl Foo` / `impl Trait for Foo` → show the type/trait name.
    if let Some(name) = ident_after(s, "impl") {
        return Some((name, SymbolKind::Struct));
    }
    // `const NAME` / `static NAME`.
    for kw in ["const", "static"] {
        if let Some(name) = ident_after(s, kw) {
            return Some((name, SymbolKind::Constant));
        }
    }
    None
}

fn scan_js(raw: &str) -> Option<(String, SymbolKind)> {
    let mut s = raw.trim_start();
    for m in ["export ", "default ", "async ", "public ", "private ", "static "] {
        if let Some(rest) = s.strip_prefix(m) {
            s = rest.trim_start();
        }
    }
    if let Some(name) = ident_after(s, "function") {
        return Some((name, SymbolKind::Function));
    }
    if let Some(name) = ident_after(s, "class") {
        return Some((name, SymbolKind::Class));
    }
    if let Some(name) = ident_after(s, "interface") {
        return Some((name, SymbolKind::Interface));
    }
    if let Some(name) = ident_after(s, "enum") {
        return Some((name, SymbolKind::Enum));
    }
    // `const foo = (…) =>` / `let bar = function` — an assigned callable.
    for kw in ["const", "let", "var"] {
        if let Some(name) = ident_after(s, kw) {
            let after = s.split_once('=').map(|(_, r)| r.trim_start()).unwrap_or("");
            if after.starts_with("function") || after.contains("=>") || after.starts_with("async") {
                return Some((name, SymbolKind::Function));
            }
        }
    }
    None
}

fn scan_python(raw: &str) -> Option<(String, SymbolKind)> {
    // Indentation implies a method (nested under a class); we still report it flat with its line.
    let indented = raw.starts_with(char::is_whitespace);
    let s = raw.trim_start();
    let s = s.strip_prefix("async ").map(str::trim_start).unwrap_or(s);
    if let Some(name) = ident_after(s, "def") {
        return Some((name, if indented { SymbolKind::Method } else { SymbolKind::Function }));
    }
    if let Some(name) = ident_after(s, "class") {
        return Some((name, SymbolKind::Class));
    }
    None
}

fn scan_go(raw: &str) -> Option<(String, SymbolKind)> {
    let s = raw.trim_start();
    // `func Name(...)` or `func (recv T) Name(...)`.
    if let Some(rest) = s.strip_prefix("func") {
        if rest.starts_with(|c: char| c.is_whitespace()) {
            let rest = rest.trim_start();
            let rest = if let Some(open) = rest.strip_prefix('(') {
                // method receiver — skip to after ')'
                open.find(')').map(|i| open[i + 1..].trim_start()).unwrap_or(rest)
            } else {
                rest
            };
            let name: String = rest.chars().take_while(is_ident_char).collect();
            if !name.is_empty() {
                return Some((name, SymbolKind::Function));
            }
        }
    }
    if let Some(name) = ident_after(s, "type") {
        return Some((name, SymbolKind::TypeAlias));
    }
    None
}

fn scan_ruby(raw: &str) -> Option<(String, SymbolKind)> {
    let indented = raw.starts_with(char::is_whitespace);
    let s = raw.trim_start();
    if let Some(name) = ident_after(s, "def") {
        return Some((name, if indented { SymbolKind::Method } else { SymbolKind::Function }));
    }
    if let Some(name) = ident_after(s, "class") {
        return Some((name, SymbolKind::Class));
    }
    if let Some(name) = ident_after(s, "module") {
        return Some((name, SymbolKind::Module));
    }
    None
}

fn scan_php(raw: &str) -> Option<(String, SymbolKind)> {
    let mut s = raw.trim_start();
    for m in ["public ", "private ", "protected ", "static ", "final ", "abstract "] {
        if let Some(rest) = s.strip_prefix(m) {
            s = rest.trim_start();
        }
    }
    if let Some(name) = ident_after(s, "function") {
        return Some((name, SymbolKind::Function));
    }
    if let Some(name) = ident_after(s, "class") {
        return Some((name, SymbolKind::Class));
    }
    if let Some(name) = ident_after(s, "interface") {
        return Some((name, SymbolKind::Interface));
    }
    if let Some(name) = ident_after(s, "trait") {
        return Some((name, SymbolKind::Trait));
    }
    if let Some(name) = ident_after(s, "enum") {
        return Some((name, SymbolKind::Enum));
    }
    None
}

/// C-family **types** only (functions/methods are keyword-less here — too ambiguous to detect).
fn scan_clike(raw: &str) -> Option<(String, SymbolKind)> {
    let mut s = raw.trim_start();
    for m in ["public ", "private ", "protected ", "internal ", "static ", "final ", "abstract ", "sealed ", "typedef "] {
        if let Some(rest) = s.strip_prefix(m) {
            s = rest.trim_start();
        }
    }
    for (kw, kind) in [
        ("class", SymbolKind::Class),
        ("struct", SymbolKind::Struct),
        ("interface", SymbolKind::Interface),
        ("enum", SymbolKind::Enum),
        ("union", SymbolKind::Struct),
        ("namespace", SymbolKind::Module),
        ("record", SymbolKind::Class),
    ] {
        if let Some(name) = ident_after(s, kw) {
            return Some((name, kind));
        }
    }
    None
}

fn scan_markdown(raw: &str) -> Option<(String, SymbolKind)> {
    let s = raw.trim_start();
    if s.starts_with('#') {
        let level = s.chars().take_while(|c| *c == '#').count();
        let text = s[level..].trim();
        if (1..=6).contains(&level) && !text.is_empty() {
            return Some((text.to_string(), SymbolKind::Heading));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(source: &str, lang: &str) -> Vec<(String, SymbolKind, usize)> {
        outline(source, lang).into_iter().map(|s| (s.name, s.kind, s.line)).collect()
    }

    #[test]
    fn rust_functions_types_and_impls() {
        let src = "\
use std::io;

pub fn run() {}
async fn fetch() {}
pub(crate) struct Config;
enum Mode { A, B }
trait Handler {}
impl Handler for Config {}
mod util;
const MAX: u32 = 5;
";
        let got = names(src, "rust");
        assert_eq!(got[0], ("run".into(), SymbolKind::Function, 3));
        assert_eq!(got[1], ("fetch".into(), SymbolKind::Function, 4));
        assert_eq!(got[2], ("Config".into(), SymbolKind::Struct, 5));
        assert_eq!(got[3], ("Mode".into(), SymbolKind::Enum, 6));
        assert_eq!(got[4], ("Handler".into(), SymbolKind::Trait, 7));
        assert_eq!(got[5], ("Handler".into(), SymbolKind::Struct, 8)); // impl
        assert_eq!(got[6], ("util".into(), SymbolKind::Module, 9));
        assert_eq!(got[7], ("MAX".into(), SymbolKind::Constant, 10));
    }

    #[test]
    fn js_ts_functions_classes_and_arrows() {
        let src = "\
export function main() {}
class Widget {}
export interface Props {}
const handler = (e) => {};
let make = function () {};
const notCallable = 42;
";
        let got = names(src, "ts");
        assert_eq!(got[0], ("main".into(), SymbolKind::Function, 1));
        assert_eq!(got[1], ("Widget".into(), SymbolKind::Class, 2));
        assert_eq!(got[2], ("Props".into(), SymbolKind::Interface, 3));
        assert_eq!(got[3], ("handler".into(), SymbolKind::Function, 4));
        assert_eq!(got[4], ("make".into(), SymbolKind::Function, 5));
        // A plain constant is not a symbol in this heuristic.
        assert!(!got.iter().any(|(n, ..)| n == "notCallable"));
    }

    #[test]
    fn python_defs_classes_and_methods() {
        let src = "\
class Server:
    def start(self):
        pass

def helper():
    pass
";
        let got = names(src, "py");
        assert_eq!(got[0], ("Server".into(), SymbolKind::Class, 1));
        assert_eq!(got[1], ("start".into(), SymbolKind::Method, 2)); // indented → method
        assert_eq!(got[2], ("helper".into(), SymbolKind::Function, 5));
    }

    #[test]
    fn go_funcs_methods_and_types() {
        let src = "\
package main
func main() {}
func (s *Server) Serve() {}
type Config struct {}
";
        let got = names(src, "go");
        assert_eq!(got[0], ("main".into(), SymbolKind::Function, 2));
        assert_eq!(got[1], ("Serve".into(), SymbolKind::Function, 3)); // method receiver skipped
        assert_eq!(got[2], ("Config".into(), SymbolKind::TypeAlias, 4));
    }

    #[test]
    fn markdown_headings_by_level() {
        let src = "# Title\n\nsome text\n## Section\n### Sub\nnot ## a heading\n";
        let got = names(src, "md");
        assert_eq!(got[0], ("Title".into(), SymbolKind::Heading, 1));
        assert_eq!(got[1], ("Section".into(), SymbolKind::Heading, 4));
        assert_eq!(got[2], ("Sub".into(), SymbolKind::Heading, 5));
        assert_eq!(got.len(), 3, "an inline ## mid-line is not a heading");
    }

    #[test]
    fn ruby_defs_classes_and_modules() {
        let src = "module App\n  class Server\n    def start\n    end\n  end\nend\ndef top\nend\n";
        let got = names(src, "rb");
        assert_eq!(got[0], ("App".into(), SymbolKind::Module, 1));
        assert_eq!(got[1], ("Server".into(), SymbolKind::Class, 2));
        assert_eq!(got[2], ("start".into(), SymbolKind::Method, 3)); // indented → method
        assert_eq!(got[3], ("top".into(), SymbolKind::Function, 7));
    }

    #[test]
    fn php_functions_classes_and_traits() {
        let src = "<?php\nclass Widget {}\ninterface Renderable {}\ntrait Sized {}\npublic function build() {}\n";
        let got = names(src, "php");
        assert_eq!(got[0], ("Widget".into(), SymbolKind::Class, 2));
        assert_eq!(got[1], ("Renderable".into(), SymbolKind::Interface, 3));
        assert_eq!(got[2], ("Sized".into(), SymbolKind::Trait, 4));
        assert_eq!(got[3], ("build".into(), SymbolKind::Function, 5));
    }

    #[test]
    fn clike_types_java_cpp_csharp() {
        let src = "public class Main {}\nstruct Point {};\nenum Color { RED }\nnamespace app {}\nrecord Pair() {}\n";
        let got = names(src, "java");
        assert_eq!(got[0], ("Main".into(), SymbolKind::Class, 1));
        assert_eq!(got[1], ("Point".into(), SymbolKind::Struct, 2));
        assert_eq!(got[2], ("Color".into(), SymbolKind::Enum, 3));
        assert_eq!(got[3], ("app".into(), SymbolKind::Module, 4));
        assert_eq!(got[4], ("Pair".into(), SymbolKind::Class, 5));
    }

    #[test]
    fn an_unknown_language_is_empty() {
        assert!(outline("fn x() {}", "brainfuck").is_empty());
    }
}
