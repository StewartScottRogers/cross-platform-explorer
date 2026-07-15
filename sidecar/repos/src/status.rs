//! Local git repo status (CPE-437).
//!
//! Parses `git status --porcelain=v2 --branch` into a compact [`RepoState`] — branch, how far
//! **ahead/behind** the upstream, and whether the working tree is **dirty**. The parse is pure so
//! it's fully unit-testable; the sidecar just shells out to `git` and hands the output here (D2:
//! shell out to the VCS tool). This state is the input to the two-way sync planner (CPE-438).

/// The sync-relevant state of a local repo relative to its upstream.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RepoState {
    /// Current branch, or `None` when detached / unborn.
    pub branch: Option<String>,
    /// The configured upstream (e.g. `origin/main`), or `None` when the branch has no upstream.
    pub upstream: Option<String>,
    /// Commits the local branch is ahead of its upstream.
    pub ahead: u32,
    /// Commits the local branch is behind its upstream.
    pub behind: u32,
    /// Any tracked/untracked change in the working tree (uncommitted work).
    pub dirty: bool,
}

impl RepoState {
    /// Diverged = both ahead AND behind (a plain pull can't fast-forward; needs merge/rebase).
    pub fn diverged(&self) -> bool {
        self.ahead > 0 && self.behind > 0
    }
    pub fn up_to_date(&self) -> bool {
        self.ahead == 0 && self.behind == 0
    }
}

/// Parse the output of `git status --porcelain=v2 --branch`. Resilient: unknown lines are ignored,
/// so a future git format never panics — worst case a field stays at its default.
pub fn parse_status(output: &str) -> RepoState {
    let mut st = RepoState::default();
    for line in output.lines() {
        if let Some(rest) = line.strip_prefix("# branch.head ") {
            let b = rest.trim();
            st.branch = (b != "(detached)").then(|| b.to_string());
        } else if let Some(rest) = line.strip_prefix("# branch.upstream ") {
            st.upstream = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("# branch.ab ") {
            // Format: "+<ahead> -<behind>".
            for tok in rest.split_whitespace() {
                if let Some(n) = tok.strip_prefix('+') {
                    st.ahead = n.parse().unwrap_or(0);
                } else if let Some(n) = tok.strip_prefix('-') {
                    st.behind = n.parse().unwrap_or(0);
                }
            }
        } else if line.starts_with(['1', '2', 'u', '?']) {
            // 1/2 = changed/renamed tracked entry, u = unmerged, ? = untracked ⇒ working tree dirty.
            st.dirty = true;
        }
    }
    st
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_clean_up_to_date_branch() {
        let out = "# branch.oid abc123\n# branch.head main\n# branch.upstream origin/main\n# branch.ab +0 -0\n";
        let st = parse_status(out);
        assert_eq!(st.branch.as_deref(), Some("main"));
        assert_eq!(st.upstream.as_deref(), Some("origin/main"));
        assert_eq!((st.ahead, st.behind, st.dirty), (0, 0, false));
        assert!(st.up_to_date() && !st.diverged());
    }

    #[test]
    fn parses_ahead_behind_and_dirty() {
        let out = "# branch.head feature\n# branch.upstream origin/feature\n# branch.ab +2 -3\n\
                   1 .M N... 100644 100644 100644 aaa bbb src/lib.rs\n? untracked.txt\n";
        let st = parse_status(out);
        assert_eq!((st.ahead, st.behind), (2, 3));
        assert!(st.dirty);
        assert!(st.diverged());
    }

    #[test]
    fn detached_head_and_no_upstream() {
        let st = parse_status("# branch.head (detached)\n");
        assert_eq!(st.branch, None);
        assert_eq!(st.upstream, None);
        assert!(st.up_to_date());
    }

    #[test]
    fn unmerged_entry_counts_as_dirty() {
        let st = parse_status("# branch.head main\nu UU N... 100644 100644 100644 100644 a b c d f.rs\n");
        assert!(st.dirty);
    }
}
