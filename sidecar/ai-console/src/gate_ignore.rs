//! Gate ignore matcher (CPE-1075, epic CPE-729): a gitignore-style path-vs-pattern matcher — the
//! **foundation** for CPE-729's scope/deny/secret matching (consumed next by CPE-1076). Given an
//! ordered ruleset, decide whether a normalized path is ignored, with last-match-wins negation
//! (a later `!pattern` un-ignores something an earlier pattern ignored).
//!
//! Pure, dependency-free. Distinct from [`crate::swarm_locks`]'s `path_globs_overlap` (which decides
//! whether two *globs* can describe a common path) — this decides whether one *concrete path* matches
//! an ordered list of gitignore-style rules, so it is built fresh rather than retrofit onto
//! `swarm_locks`. It does reuse that module's segment convention: split on `/`, drop empty segments.
//!
//! Gitignore semantics implemented: `*`/`?` wildcards within a segment; `**` as a whole segment spans
//! zero or more whole path segments; a leading `/` anchors the rule to the root (only start position 0
//! is tried); a trailing `/` makes the rule directory-only; a leading `!` negates. Rules are evaluated
//! in order and the **last matching rule's polarity wins**.
//!
//! Both matchers here are iterative (DP / two-pointer with explicit backtrack state), never recursive,
//! so a deep or adversarial path/pattern cannot stack-overflow — only cost more loop iterations.

/// A single parsed gitignore-style rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IgnoreRule {
    /// `!pattern` — a later match un-ignores rather than ignores.
    pub negate: bool,
    /// Leading `/` — only matches from the root (path index 0), not at any depth.
    pub anchored: bool,
    /// Trailing `/` — only matches a directory (the caller signals "is a directory" by passing a
    /// path that itself ends in `/`); an ancestor-directory match is unaffected (see [`matches`]).
    pub dir_only: bool,
    /// The pattern's path segments (`/`-split, empties dropped), after stripping `!`, leading `/`,
    /// and trailing `/`. May contain the literal segment `"**"`.
    pub segments: Vec<String>,
}

/// Normalize `\` → `/` (cross-OS: never use `std::path` here).
fn normalize(s: &str) -> String {
    s.replace('\\', "/")
}

/// Split a normalized (already `/`-only) string into non-empty segments.
fn split_segments(s: &str) -> Vec<String> {
    s.split('/').filter(|p| !p.is_empty()).map(String::from).collect()
}

/// Parse one gitignore-style pattern line into a rule.
pub fn parse_rule(pattern: &str) -> IgnoreRule {
    let mut s = normalize(pattern);
    let negate = if let Some(rest) = s.strip_prefix('!') {
        s = rest.to_string();
        true
    } else {
        false
    };
    let anchored = if let Some(rest) = s.strip_prefix('/') {
        s = rest.to_string();
        true
    } else {
        false
    };
    let dir_only = if let Some(rest) = s.strip_suffix('/') {
        s = rest.to_string();
        true
    } else {
        false
    };
    IgnoreRule { negate, anchored, dir_only, segments: split_segments(&s) }
}

/// Iterative (non-recursive) wildcard match of one path segment against one pattern segment.
/// `*` matches any run of characters (possibly empty); `?` matches exactly one character.
/// Classic two-pointer scan with a remembered last-`*` backtrack point — bounded by
/// `O(pattern.len() + text.len())` iterations, no recursion, so it cannot stack-overflow.
fn seg_match(pattern: &str, text: &str) -> bool {
    let p = pattern.as_bytes();
    let t = text.as_bytes();
    let (mut pi, mut ti) = (0usize, 0usize);
    let mut star: Option<usize> = None;
    let mut star_ti = 0usize;
    while ti < t.len() {
        if pi < p.len() && (p[pi] == b'?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == b'*' {
            star = Some(pi);
            star_ti = ti;
            pi += 1;
        } else if let Some(sp) = star {
            pi = sp + 1;
            star_ti += 1;
            ti = star_ti;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }
    pi == p.len()
}

/// Whether `rule` matches `path_segs` (already `/`-split), given whether the candidate path denotes
/// a directory. Iterative DP over (pattern-segment index, path-segment index) — bounded by
/// `O(pattern_segments * path_segments)` table cells, no recursion, so an adversarially deep path or
/// pattern only costs more table cells, never a stack overflow.
///
/// An unanchored rule is evaluated as if implicitly prefixed with a `**` segment (gitignore: a
/// pattern with no leading `/` may match starting at any depth). A **terminal** match (the pattern
/// consumes every remaining path segment) respects `dir_only` against `is_dir`. An **ancestor** match
/// (the pattern fully matches only a strict prefix of the path segments, i.e. something exists below
/// it) always counts — the matched component is necessarily a directory, so `dir_only` doesn't apply.
fn rule_matches_path(rule: &IgnoreRule, path_segs: &[String], is_dir: bool) -> bool {
    let mut effective: Vec<&str> = Vec::with_capacity(rule.segments.len() + 1);
    if !rule.anchored {
        effective.push("**");
    }
    effective.extend(rule.segments.iter().map(String::as_str));

    let p_len = effective.len();
    let j_len = path_segs.len();
    let mut dp = vec![vec![false; j_len + 1]; p_len + 1];
    dp[0][0] = true;
    for p in 0..p_len {
        for j in 0..=j_len {
            if !dp[p][j] {
                continue;
            }
            if effective[p] == "**" {
                dp[p + 1][j] = true; // ** consumes zero segments
                if j < j_len {
                    dp[p][j + 1] = true; // ** absorbs one more segment, stays at p
                }
            } else if j < j_len && seg_match(effective[p], &path_segs[j]) {
                dp[p + 1][j + 1] = true;
            }
        }
    }

    if dp[p_len][j_len] && (!rule.dir_only || is_dir) {
        return true; // terminal match
    }
    dp[p_len][..j_len].iter().any(|&reached| reached) // ancestor match
}

/// Whether `path` is ignored by `rules`, walked in order with **last-match-wins** negation: the
/// final rule that matches decides the polarity (a later `!pattern` un-ignores). A path no rule
/// matches is not ignored.
///
/// `path` is normalized (`\` → `/`) internally. A trailing `/` on `path` signals "this candidate is
/// a directory" — relevant only to `dir_only` rules' terminal match (see [`rule_matches_path`]).
pub fn matches(path: &str, rules: &[IgnoreRule]) -> bool {
    let normalized = normalize(path);
    let is_dir = normalized.ends_with('/');
    let path_segs = split_segments(&normalized);

    let mut ignored = false;
    for rule in rules {
        if rule_matches_path(rule, &path_segs, is_dir) {
            ignored = !rule.negate;
        }
    }
    ignored
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(patterns: &[&str]) -> Vec<IgnoreRule> {
        patterns.iter().map(|p| parse_rule(p)).collect()
    }

    #[test]
    fn double_star_dir_only_matches_nested_contents() {
        let rs = rules(&["**/node_modules/"]);
        assert!(matches("a/b/node_modules/x", &rs));
        assert!(matches("node_modules/x", &rs));
        assert!(!matches("a/b/node_modules_extra/x", &rs));
    }

    #[test]
    fn negation_after_a_broad_rule_un_ignores_last_match_wins() {
        let rs = rules(&["*.env", "!keep.env"]);
        assert!(matches("secret.env", &rs));
        assert!(!matches("keep.env", &rs)); // negated last -> un-ignored
        // Re-ignoring after negation still respects order (last match wins, both directions).
        let rs2 = rules(&["*.env", "!keep.env", "keep.env"]);
        assert!(matches("keep.env", &rs2));
    }

    #[test]
    fn leading_slash_anchors_to_root_only() {
        let rs = rules(&["/build"]);
        assert!(matches("build/x", &rs)); // build itself, and its contents
        assert!(matches("build", &rs));
        assert!(!matches("src/build/x", &rs)); // not anchored at root -> no match
        assert!(!matches("src/build", &rs));
    }

    #[test]
    fn glob_wildcard_matches_secret_extension_anywhere() {
        let rs = rules(&["*.pem"]);
        assert!(matches("id.pem", &rs));
        assert!(matches("secrets/deploy/prod.pem", &rs));
        assert!(!matches("id.pem.bak", &rs));
    }

    #[test]
    fn dir_only_rule_does_not_match_a_file_of_the_same_name() {
        let rs = rules(&["dir/"]);
        assert!(!matches("dir", &rs)); // no trailing slash -> a file candidate, not matched
        assert!(matches("dir/", &rs)); // trailing slash -> a directory candidate, matched
        assert!(matches("dir/inside.txt", &rs)); // contents of the ignored dir are matched too
    }

    #[test]
    fn question_mark_matches_exactly_one_character() {
        let rs = rules(&["log?.txt"]);
        assert!(matches("log1.txt", &rs));
        assert!(!matches("log12.txt", &rs));
        assert!(!matches("log.txt", &rs));
    }

    #[test]
    fn unanchored_pattern_matches_at_any_depth_including_root() {
        let rs = rules(&["target"]);
        assert!(matches("target", &rs));
        assert!(matches("target/debug/bin", &rs));
        assert!(matches("crates/sub/target/debug/bin", &rs));
        assert!(!matches("targets/x", &rs)); // whole-segment match only
    }

    #[test]
    fn no_rule_matches_is_not_ignored() {
        let rs = rules(&["*.pem", "build/"]);
        assert!(!matches("src/lib.rs", &rs));
    }

    #[test]
    fn empty_ruleset_never_ignores() {
        assert!(!matches("anything/at/all.rs", &[]));
    }

    #[test]
    fn backslash_paths_and_patterns_are_normalized_before_matching() {
        let rs = rules(&["a\\b\\*.log"]);
        assert!(matches(r"a\b\out.log", &rs));
        assert!(matches("a/b/out.log", &rs));
    }

    /// A deep/adversarial path AND pattern must not stack-overflow. Both matchers here are
    /// iterative (DP table / two-pointer scan), so this only costs loop iterations, never stack
    /// depth — this test would previously have blown the stack under a recursive implementation.
    #[test]
    fn deep_path_and_pattern_do_not_stack_overflow() {
        let deep_path = "seg/".repeat(3000) + "target";
        let deep_pattern = "**/".repeat(1000) + "target";
        let rs = rules(&[&deep_pattern]);
        assert!(matches(&deep_path, &rs));

        // Also stress the within-segment wildcard scanner with a long run of '*' against a long
        // literal segment.
        let star_pattern = "*".repeat(500) + "end";
        let long_segment = "x".repeat(5000) + "end";
        assert!(seg_match(&star_pattern, &long_segment));

        // A deep path that should NOT match anything still must not overflow.
        let unrelated_rs = rules(&["*.nomatch"]);
        assert!(!matches(&deep_path, &unrelated_rs));
    }
}
