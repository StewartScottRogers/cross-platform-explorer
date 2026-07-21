//! Instant-search query grammar + ranked-result model (CPE-831, epic CPE-703).
//!
//! The **backend-agnostic** core of the Everything-style instant search: it turns a typed query string
//! into a structured [`Query`], decides whether a candidate file [`matches`], and [`rank`]s matches by
//! relevance. It is reused whatever index backend CPE-832 lands, so it commits to no storage design and
//! needs no volume access or privileges — pure and fully cargo-testable.
//!
//! Grammar: whitespace-separated tokens. `ext:png,jpg` and `path:foo` are **structured filters**; every
//! other token is a **name term** — a substring, or a glob with `*`/`?` and `{a,b}` brace groups. All
//! terms AND together. Name-term matching reuses [`crate::name_search::name_matches`] so instant search
//! and folder search share exactly one matching semantics (no second glob implementation).

use crate::name_search::name_matches;

/// A parsed instant-search query. Empty (`name_terms`, `exts`, `path_terms` all empty) matches nothing.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Query {
    /// Lowercased name terms, matched against a candidate's file name (substring or glob). ANDed.
    pub name_terms: Vec<String>,
    /// Lowercased extensions (without a dot); a candidate's ext must be one of these. Empty = no filter.
    pub exts: Vec<String>,
    /// Lowercased substrings matched against a candidate's full path. ANDed.
    pub path_terms: Vec<String>,
}

impl Query {
    /// True when the query carries no constraints (so [`matches`] is always false).
    pub fn is_empty(&self) -> bool {
        self.name_terms.is_empty() && self.exts.is_empty() && self.path_terms.is_empty()
    }
}

/// Parse a query string. `ext:` (a comma list, a leading dot tolerated) and `path:` are structured
/// filters; everything else is a name term. Case-insensitive throughout.
pub fn parse(input: &str) -> Query {
    let mut q = Query::default();
    for tok in input.split_whitespace() {
        let lower = tok.to_lowercase();
        if let Some(rest) = lower.strip_prefix("ext:") {
            for e in rest.split(',') {
                let e = e.trim_start_matches('.').trim();
                if !e.is_empty() {
                    q.exts.push(e.to_string());
                }
            }
        } else if let Some(rest) = lower.strip_prefix("path:") {
            if !rest.is_empty() {
                q.path_terms.push(rest.to_string());
            }
        } else if !lower.is_empty() {
            q.name_terms.push(lower);
        }
    }
    q
}

/// A file the query is evaluated against. `ext` is the lowercased extension without a dot (empty for
/// none); the caller usually has these from a directory entry already.
#[derive(Debug, Clone, Copy)]
pub struct Candidate<'a> {
    pub name: &'a str,
    pub path: &'a str,
    pub ext: &'a str,
}

/// Whether `c` satisfies every constraint in `q`. An empty query matches nothing.
pub fn matches(q: &Query, c: &Candidate) -> bool {
    if q.is_empty() {
        return false;
    }
    // Name terms: each must match the name via the shared glob/substring matcher.
    for t in &q.name_terms {
        if !name_matches(c.name, t) {
            return false;
        }
    }
    // Extension filter: any-of.
    if !q.exts.is_empty() {
        let ext = c.ext.to_lowercase();
        if !q.exts.iter().any(|e| e == &ext) {
            return false;
        }
    }
    // Path terms: each must appear (case-insensitively) in the full path.
    if !q.path_terms.is_empty() {
        let path = c.path.to_lowercase();
        for t in &q.path_terms {
            if !path.contains(t) {
                return false;
            }
        }
    }
    true
}

/// How strongly a single name term matches a name (higher = better). A glob term (contains `*`/`?`/`{`)
/// gets a flat base score since substring position is meaningless for it.
fn term_score(term: &str, name_lower: &str) -> i32 {
    if term.contains('*') || term.contains('?') || term.contains('{') {
        return 40;
    }
    if name_lower == term {
        100
    } else if name_lower.starts_with(term) {
        70
    } else if name_lower
        .split(|ch: char| !ch.is_alphanumeric())
        .any(|w| w == term)
    {
        55 // matches a whole word within the name
    } else if name_lower.contains(term) {
        30
    } else {
        0
    }
}

/// A candidate's relevance to `q` (higher = better). Sums the per-name-term scores, then applies a
/// shorter-path-wins tiebreak. Scale keeps the term score primary and path length strictly secondary, so
/// the result is a stable total order.
pub fn score(q: &Query, c: &Candidate) -> i32 {
    let name_lower = c.name.to_lowercase();
    let term_total: i32 = q.name_terms.iter().map(|t| term_score(t, &name_lower)).sum();
    // A query with only ext:/path: filters (no name terms) scores everything equally on relevance;
    // the path-length tiebreak still orders them deterministically.
    term_total * 10_000 - (c.path.len().min(9_999) as i32)
}

/// Filter `candidates` to those matching `q`, returned best-first with a deterministic total order
/// (score desc, then name, then path).
pub fn rank<'a>(q: &Query, candidates: &'a [Candidate<'a>]) -> Vec<&'a Candidate<'a>> {
    let mut hits: Vec<&Candidate> = candidates.iter().filter(|c| matches(q, c)).collect();
    hits.sort_by(|a, b| {
        score(q, b)
            .cmp(&score(q, a))
            .then_with(|| a.name.cmp(b.name))
            .then_with(|| a.path.cmp(b.path))
    });
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand<'a>(name: &'a str, path: &'a str, ext: &'a str) -> Candidate<'a> {
        Candidate { name, path, ext }
    }

    #[test]
    fn parse_splits_filters_from_name_terms() {
        let q = parse("Report ext:.PNG,jpg path:/Docs Final");
        assert_eq!(q.name_terms, vec!["report".to_string(), "final".to_string()]);
        assert_eq!(q.exts, vec!["png".to_string(), "jpg".to_string()]);
        assert_eq!(q.path_terms, vec!["/docs".to_string()]);
        assert!(parse("   ").is_empty());
    }

    #[test]
    fn matches_ands_all_constraints() {
        let q = parse("report ext:pdf path:/work");
        assert!(matches(&q, &cand("Q3-Report.pdf", "/work/Q3-Report.pdf", "pdf")));
        // wrong ext
        assert!(!matches(&q, &cand("Q3-Report.txt", "/work/Q3-Report.txt", "txt")));
        // wrong path
        assert!(!matches(&q, &cand("Q3-Report.pdf", "/home/Q3-Report.pdf", "pdf")));
        // missing a name term
        assert!(!matches(&q, &cand("summary.pdf", "/work/summary.pdf", "pdf")));
    }

    #[test]
    fn matches_supports_glob_name_terms() {
        let q = parse("*.rs");
        assert!(matches(&q, &cand("main.rs", "/src/main.rs", "rs")));
        assert!(!matches(&q, &cand("main.py", "/src/main.py", "py")));
        let braced = parse("img_??.{png,jpg}");
        assert!(matches(&braced, &cand("img_04.png", "/p/img_04.png", "png")));
        assert!(!matches(&braced, &cand("img_4.png", "/p/img_4.png", "png"))); // ? is exactly one char
    }

    #[test]
    fn empty_query_matches_nothing() {
        assert!(!matches(&Query::default(), &cand("anything", "/anything", "")));
    }

    #[test]
    fn score_orders_exact_over_prefix_over_substring() {
        let q = parse("report");
        let exact = cand("report", "/report", "");
        let prefix = cand("report-final", "/report-final", "");
        let substr = cand("q3-report-x", "/q3-report-x", "");
        assert!(score(&q, &exact) > score(&q, &prefix));
        assert!(score(&q, &prefix) > score(&q, &substr));
    }

    #[test]
    fn score_prefers_shorter_paths_as_a_tiebreak() {
        let q = parse("data");
        let shallow = cand("data.csv", "/data.csv", "csv");
        let deep = cand("data.csv", "/a/very/deep/nested/data.csv", "csv");
        assert!(score(&q, &shallow) > score(&q, &deep));
    }

    #[test]
    fn rank_filters_and_orders_best_first() {
        let q = parse("report");
        let cands = vec![
            cand("q3-report.txt", "/x/q3-report.txt", "txt"),
            cand("report", "/report", ""),
            cand("unrelated.txt", "/unrelated.txt", "txt"),
            cand("report-2024.md", "/report-2024.md", "md"),
        ];
        let ranked = rank(&q, &cands);
        let names: Vec<&str> = ranked.iter().map(|c| c.name).collect();
        assert_eq!(names, vec!["report", "report-2024.md", "q3-report.txt"]);
        assert!(!names.contains(&"unrelated.txt"), "non-matches are filtered out");
    }

    #[test]
    fn ext_only_query_filters_and_orders_deterministically() {
        let q = parse("ext:rs");
        let cands = vec![
            cand("b.rs", "/deep/dir/b.rs", "rs"),
            cand("a.rs", "/a.rs", "rs"),
            cand("c.py", "/c.py", "py"),
        ];
        let ranked = rank(&q, &cands);
        let names: Vec<&str> = ranked.iter().map(|c| c.name).collect();
        // Both .rs match; shorter path ("/a.rs") wins the tiebreak, .py filtered out.
        assert_eq!(names, vec!["a.rs", "b.rs"]);
    }
}
