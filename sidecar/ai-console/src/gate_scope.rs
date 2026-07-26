//! Gate scope verdict (CPE-1076, epic CPE-729): given a filesystem action and a scope policy,
//! decide whether the action is in-scope, out-of-scope, deny-listed, or touches a secret path.
//!
//! Pure, dependency-free. Reuses [`crate::gate_ignore`]'s `IgnoreRule`/`matches` for deny/secret
//! matching, but does not modify that module. Allow-root containment is implemented fresh here
//! because it has different semantics than gitignore matching: an allow root is a **prefix**
//! relationship (the action path must descend from the root), decided **segment-by-segment**,
//! never via a raw string `starts_with` — `/repo-secrets/x` must not be considered "under"
//! `/repo` just because the string `"/repo"` is a textual prefix of `"/repo-secrets/x"`.
//!
//! Precedence when multiple things could match: **Secret > Deny > OutOfScope > InScope**. A
//! secret path is always flagged even if it happens to sit inside an allow root; a denied path
//! is flagged even if it's otherwise in scope; only after both of those are cleared does allow-root
//! containment decide in-scope vs. out-of-scope.

use crate::gate_ignore::{matches, IgnoreRule};

/// The kind of filesystem operation an agent performed or is about to perform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsOp {
    Create,
    Modify,
    Delete,
    Read,
}

/// One filesystem action to evaluate: a path plus the operation performed on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsAction {
    pub path: String,
    pub op: FsOp,
}

/// The policy an action's scope is evaluated against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopePolicy {
    /// Roots the action's path must descend from (segment-wise) to be in scope.
    pub allow_roots: Vec<String>,
    /// Gitignore-style rules that deny an otherwise-in-scope path.
    pub deny: Vec<IgnoreRule>,
    /// Gitignore-style rules flagging secret paths (checked ahead of everything else).
    pub secret: Vec<IgnoreRule>,
}

/// The verdict for one [`FsAction`] against a [`ScopePolicy`]. `DenyListed`/`SecretPath` carry
/// the matched pattern as their reason string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeVerdict {
    InScope,
    OutOfScope,
    DenyListed(String),
    SecretPath(String),
}

/// Normalize `\` → `/` (cross-OS: never use `std::path` here).
fn normalize(s: &str) -> String {
    s.replace('\\', "/")
}

/// Split a normalized (already `/`-only) string into non-empty segments.
fn split_segments(s: &str) -> Vec<String> {
    s.split('/').filter(|p| !p.is_empty()).map(String::from).collect()
}

/// Whether `path_segs` is contained by `root_segs`, compared **segment-by-segment** (never a raw
/// string `starts_with`, which would wrongly let `/repo-secrets/x` count as under `/repo` since
/// `"/repo"` is a textual prefix of `"/repo-secrets/x"`). Containment requires every root segment
/// to equal the path's segment at the same position, and the path must have at least as many
/// segments as the root (the root itself counts as contained).
fn segments_contain(root_segs: &[String], path_segs: &[String]) -> bool {
    if root_segs.len() > path_segs.len() {
        return false;
    }
    root_segs.iter().zip(path_segs.iter()).all(|(r, p)| r == p)
}

/// Find the reason string for a deny/secret ruleset, or `None` if the whole ruleset doesn't
/// ignore `path`. [`crate::gate_ignore::matches`] only returns a bool for a ruleset (with
/// last-match-wins semantics across the whole list), so to surface *which* rule was responsible
/// we re-derive it here: walk the rules in order, testing each individually against `path` via
/// `matches` with a singleton slice (identical semantics to gate_ignore's own per-rule test,
/// without reaching into its private internals), and keep the last one that matched — exactly
/// mirroring `matches`'s own last-match-wins loop. If the whole-list result is "ignored", that
/// last matching rule is necessarily non-negated (that's what made the whole-list verdict true).
fn matched_reason(path: &str, rules: &[IgnoreRule]) -> Option<String> {
    if !matches(path, rules) {
        return None;
    }
    rules
        .iter()
        .rfind(|rule| matches(path, std::slice::from_ref(*rule)))
        .map(reason_for)
}

/// Render a rule back into a pattern-shaped reason string for the verdict.
fn reason_for(rule: &IgnoreRule) -> String {
    let mut s = String::new();
    if rule.negate {
        s.push('!');
    }
    if rule.anchored {
        s.push('/');
    }
    s.push_str(&rule.segments.join("/"));
    if rule.dir_only {
        s.push('/');
    }
    s
}

/// Evaluate one filesystem action against a scope policy. Precedence: **Secret > Deny >
/// OutOfScope > InScope**. Empty `allow_roots` makes every path out of scope by default (once
/// secret/deny are cleared).
pub fn evaluate_scope(action: &FsAction, policy: &ScopePolicy) -> ScopeVerdict {
    let normalized_path = normalize(&action.path);

    if let Some(reason) = matched_reason(&normalized_path, &policy.secret) {
        return ScopeVerdict::SecretPath(reason);
    }
    if let Some(reason) = matched_reason(&normalized_path, &policy.deny) {
        return ScopeVerdict::DenyListed(reason);
    }

    if policy.allow_roots.is_empty() {
        return ScopeVerdict::OutOfScope;
    }

    let path_segs = split_segments(&normalized_path);
    let in_scope = policy.allow_roots.iter().any(|root| {
        let root_segs = split_segments(&normalize(root));
        segments_contain(&root_segs, &path_segs)
    });

    if in_scope {
        ScopeVerdict::InScope
    } else {
        ScopeVerdict::OutOfScope
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gate_ignore::parse_rule;

    fn action(path: &str) -> FsAction {
        FsAction { path: path.to_string(), op: FsOp::Modify }
    }

    fn policy(allow_roots: &[&str], deny: &[&str], secret: &[&str]) -> ScopePolicy {
        ScopePolicy {
            allow_roots: allow_roots.iter().map(|s| s.to_string()).collect(),
            deny: deny.iter().map(|p| parse_rule(p)).collect(),
            secret: secret.iter().map(|p| parse_rule(p)).collect(),
        }
    }

    #[test]
    fn path_under_allow_root_is_in_scope() {
        let p = policy(&["/repo"], &[], &[]);
        assert_eq!(evaluate_scope(&action("/repo/src/x.rs"), &p), ScopeVerdict::InScope);
    }

    /// The whole point of this ticket: a sibling directory that merely shares a string prefix
    /// with the allow root (`/repo-secrets` vs `/repo`) must NOT be treated as contained. A
    /// naive `starts_with("/repo")` check would wrongly admit this path.
    #[test]
    fn sibling_directory_sharing_a_string_prefix_is_not_in_scope() {
        let p = policy(&["/repo"], &[], &[]);
        assert_eq!(evaluate_scope(&action("/repo-secrets/x"), &p), ScopeVerdict::OutOfScope);
    }

    #[test]
    fn secret_path_wins_even_inside_an_allow_root() {
        let p = policy(&["/repo"], &[], &["*.env"]);
        match evaluate_scope(&action("/repo/.env"), &p) {
            ScopeVerdict::SecretPath(reason) => assert_eq!(reason, "*.env"),
            other => panic!("expected SecretPath, got {other:?}"),
        }
    }

    #[test]
    fn deny_beats_plain_out_of_scope_and_in_scope() {
        // Denied path outside any allow root: still reported as DenyListed, not OutOfScope.
        let p = policy(&["/repo"], &["*.lock"], &[]);
        match evaluate_scope(&action("/elsewhere/x.lock"), &p) {
            ScopeVerdict::DenyListed(reason) => assert_eq!(reason, "*.lock"),
            other => panic!("expected DenyListed, got {other:?}"),
        }
        // Denied path inside an allow root: still DenyListed, not InScope.
        match evaluate_scope(&action("/repo/x.lock"), &p) {
            ScopeVerdict::DenyListed(reason) => assert_eq!(reason, "*.lock"),
            other => panic!("expected DenyListed, got {other:?}"),
        }
    }

    #[test]
    fn empty_allow_roots_defaults_to_out_of_scope() {
        let p = policy(&[], &[], &[]);
        assert_eq!(evaluate_scope(&action("/anything/at/all"), &p), ScopeVerdict::OutOfScope);
    }

    #[test]
    fn secret_beats_deny_when_both_match() {
        let p = policy(&["/repo"], &["*.env"], &["*.env"]);
        match evaluate_scope(&action("/repo/.env"), &p) {
            ScopeVerdict::SecretPath(_) => {}
            other => panic!("expected SecretPath (Secret > Deny), got {other:?}"),
        }
    }

    #[test]
    fn backslash_paths_are_normalized_before_containment_check() {
        let p = policy(&["/repo"], &[], &[]);
        assert_eq!(evaluate_scope(&action(r"\repo\src\x.rs"), &p), ScopeVerdict::InScope);
        assert_eq!(evaluate_scope(&action(r"\repo-secrets\x"), &p), ScopeVerdict::OutOfScope);
    }

    #[test]
    fn allow_root_itself_is_in_scope() {
        let p = policy(&["/repo"], &[], &[]);
        assert_eq!(evaluate_scope(&action("/repo"), &p), ScopeVerdict::InScope);
    }

    #[test]
    fn multiple_allow_roots_any_match_is_in_scope() {
        let p = policy(&["/repo", "/other"], &[], &[]);
        assert_eq!(evaluate_scope(&action("/other/y.rs"), &p), ScopeVerdict::InScope);
        assert_eq!(evaluate_scope(&action("/neither/z.rs"), &p), ScopeVerdict::OutOfScope);
    }

    #[test]
    fn op_kind_does_not_affect_the_verdict() {
        let p = policy(&["/repo"], &[], &[]);
        for op in [FsOp::Create, FsOp::Modify, FsOp::Delete, FsOp::Read] {
            let a = FsAction { path: "/repo/x".to_string(), op };
            assert_eq!(evaluate_scope(&a, &p), ScopeVerdict::InScope);
        }
    }
}
