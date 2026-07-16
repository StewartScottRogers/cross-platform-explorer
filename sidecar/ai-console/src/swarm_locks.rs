//! Swarm file-ownership lock manager (CPE-514) — the safety substrate for [CPE-502] Swarm
//! orchestration: concurrent agents must **never collide on files**. A task exclusively **claims path
//! globs** it owns for its duration; a claim that overlaps an active one is **queued** (FIFO) rather
//! than granted, so shared paths are sequenced, never clobbered. Non-overlapping claims run in
//! parallel. This module is **pure** — it decides ownership, it does no I/O and touches no real files.
//!
//! Glob model: `/`-separated segments where `**` matches zero or more whole segments, `*` matches any
//! run of characters **within** one segment, and everything else is literal. Two globs "overlap" iff
//! some concrete path is matched by both — decided by [`path_globs_overlap`] (pattern-intersection
//! non-emptiness; recursive with backtracking, which is linear-ish for real path globs).

/// Whether two single path **segments** (each may contain `*`) can match a common string.
fn seg_chars_overlap(a: &[u8], b: &[u8]) -> bool {
    match (a.first(), b.first()) {
        (None, None) => true,
        (Some(b'*'), _) => seg_chars_overlap(&a[1..], b) || (!b.is_empty() && seg_chars_overlap(a, &b[1..])),
        (_, Some(b'*')) => seg_chars_overlap(a, &b[1..]) || (!a.is_empty() && seg_chars_overlap(&a[1..], b)),
        (Some(x), Some(y)) => x == y && seg_chars_overlap(&a[1..], &b[1..]),
        (Some(_), None) | (None, Some(_)) => false,
    }
}

fn segs(glob: &str) -> Vec<&str> {
    glob.split('/').filter(|s| !s.is_empty()).collect()
}

fn overlap_segs(a: &[&str], b: &[&str]) -> bool {
    match (a.first(), b.first()) {
        (None, None) => true,
        // `**` matches zero or more whole segments (of the hypothetical common path).
        (Some(&"**"), _) => overlap_segs(&a[1..], b) || (!b.is_empty() && overlap_segs(a, &b[1..])),
        (_, Some(&"**")) => overlap_segs(a, &b[1..]) || (!a.is_empty() && overlap_segs(&a[1..], b)),
        (Some(x), Some(y)) => seg_chars_overlap(x.as_bytes(), y.as_bytes()) && overlap_segs(&a[1..], &b[1..]),
        (Some(_), None) | (None, Some(_)) => false,
    }
}

/// True iff globs `a` and `b` can match a common concrete path (they "overlap"). Handles prefix scopes
/// (`src/**` vs `src/auth/x.rs`), leading `**` (`**/*.rs` vs `src/lib.rs`), and rejects siblings
/// (`src/a/**` vs `src/b/**`).
pub fn path_globs_overlap(a: &str, b: &str) -> bool {
    overlap_segs(&segs(a), &segs(b))
}

/// Whether any glob in `a` overlaps any glob in `b`.
fn claims_overlap(a: &[String], b: &[String]) -> bool {
    a.iter().any(|ga| b.iter().any(|gb| path_globs_overlap(ga, gb)))
}

/// A held or pending claim: a task exclusively owning a set of path globs.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Claim {
    task_id: String,
    globs: Vec<String>,
}

/// The result of requesting a claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimOutcome {
    /// Granted immediately — nothing active overlaps.
    Granted,
    /// Queued behind an overlapping active claim; it acquires when that clears (FIFO).
    Queued,
}

/// Tracks exclusive path-glob ownership across a swarm's concurrent tasks (CPE-514).
#[derive(Debug, Default)]
pub struct LockManager {
    active: Vec<Claim>,
    queue: Vec<Claim>,
}

impl LockManager {
    pub fn new() -> Self {
        Self::default()
    }

    fn overlaps_active(&self, globs: &[String]) -> bool {
        self.active.iter().any(|c| claims_overlap(&c.globs, globs))
    }

    /// Request exclusive ownership of `globs` for `task_id`. Granted when nothing active overlaps;
    /// otherwise Queued (FIFO) — it acquires automatically when the overlap clears via [`release`].
    pub fn claim(&mut self, task_id: &str, globs: Vec<String>) -> ClaimOutcome {
        let c = Claim { task_id: task_id.to_string(), globs };
        if self.overlaps_active(&c.globs) {
            self.queue.push(c);
            ClaimOutcome::Queued
        } else {
            self.active.push(c);
            ClaimOutcome::Granted
        }
    }

    /// Release every claim held by `task_id`, then grant any queued claims whose overlap has now
    /// cleared, in FIFO order. Returns the task ids newly granted (so the caller can wake them).
    pub fn release(&mut self, task_id: &str) -> Vec<String> {
        self.active.retain(|c| c.task_id != task_id);
        let mut granted = Vec::new();
        // Front-to-back so an earlier waiter wins a contested path; granting only adds actives (which
        // can block later waiters), so a single forward pass reaches a fixpoint.
        let mut i = 0;
        while i < self.queue.len() {
            if self.overlaps_active(&self.queue[i].globs) {
                i += 1;
            } else {
                let c = self.queue.remove(i);
                granted.push(c.task_id.clone());
                self.active.push(c);
                // don't advance i — the removed slot now holds the next waiter
            }
        }
        granted
    }

    pub fn is_held(&self, task_id: &str) -> bool {
        self.active.iter().any(|c| c.task_id == task_id)
    }
    pub fn is_queued(&self, task_id: &str) -> bool {
        self.queue.iter().any(|c| c.task_id == task_id)
    }
    pub fn active_tasks(&self) -> Vec<&str> {
        self.active.iter().map(|c| c.task_id.as_str()).collect()
    }
    pub fn queued_tasks(&self) -> Vec<&str> {
        self.queue.iter().map(|c| c.task_id.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_overlap_prefix_scopes_and_double_star() {
        assert!(path_globs_overlap("src/auth/**", "src/auth/token.rs")); // prefix scope
        assert!(path_globs_overlap("src/**", "src/lib.rs"));
        assert!(path_globs_overlap("**/*.rs", "src/deep/lib.rs")); // leading **
        assert!(path_globs_overlap("**", "anything/at/all"));
        assert!(path_globs_overlap("src/*.rs", "src/lib.rs")); // within-segment *
        assert!(path_globs_overlap("src/*.rs", "src/*.rs")); // identical
        assert!(path_globs_overlap("a/*/c", "a/b/c"));
    }

    #[test]
    fn glob_overlap_rejects_disjoint_paths() {
        assert!(!path_globs_overlap("src/a/**", "src/b/**")); // siblings
        assert!(!path_globs_overlap("src/*.rs", "src/sub/lib.rs")); // * is one segment only
        assert!(!path_globs_overlap("a/b/c", "a/b/d"));
        assert!(!path_globs_overlap("src/lib.rs", "tests/lib.rs"));
        assert!(!path_globs_overlap("src/*.rs", "src/lib.toml"));
    }

    #[test]
    fn non_overlapping_claims_run_in_parallel() {
        let mut lm = LockManager::new();
        assert_eq!(lm.claim("t1", vec!["src/a/**".into()]), ClaimOutcome::Granted);
        assert_eq!(lm.claim("t2", vec!["src/b/**".into()]), ClaimOutcome::Granted);
        assert!(lm.is_held("t1") && lm.is_held("t2"));
    }

    #[test]
    fn an_overlapping_claim_is_queued_not_granted() {
        let mut lm = LockManager::new();
        assert_eq!(lm.claim("t1", vec!["src/**".into()]), ClaimOutcome::Granted);
        assert_eq!(lm.claim("t2", vec!["src/auth/token.rs".into()]), ClaimOutcome::Queued);
        assert!(lm.is_held("t1"));
        assert!(lm.is_queued("t2") && !lm.is_held("t2"));
    }

    #[test]
    fn release_grants_the_waiting_claim_shared_dep_sequenced() {
        let mut lm = LockManager::new();
        lm.claim("t1", vec!["src/shared.rs".into()]);
        lm.claim("t2", vec!["src/shared.rs".into()]); // shared dependency → queued
        let granted = lm.release("t1");
        assert_eq!(granted, vec!["t2".to_string()]);
        assert!(lm.is_held("t2") && !lm.is_queued("t2"));
    }

    #[test]
    fn fifo_order_among_contending_waiters() {
        let mut lm = LockManager::new();
        lm.claim("t1", vec!["p".into()]);
        lm.claim("t2", vec!["p".into()]); // queued 1st
        lm.claim("t3", vec!["p".into()]); // queued 2nd
        // Releasing t1 wakes only t2 (t3 still contends with the now-active t2).
        assert_eq!(lm.release("t1"), vec!["t2".to_string()]);
        assert!(lm.is_queued("t3"));
        // Releasing t2 then wakes t3.
        assert_eq!(lm.release("t2"), vec!["t3".to_string()]);
        assert!(lm.is_held("t3"));
    }

    #[test]
    fn releasing_one_frees_only_the_waiters_it_unblocks() {
        let mut lm = LockManager::new();
        lm.claim("t1", vec!["src/a/**".into()]);
        lm.claim("t2", vec!["src/b/**".into()]);
        lm.claim("t3", vec!["src/a/x.rs".into()]); // waits on t1 only
        assert!(lm.is_queued("t3"));
        // Releasing t2 (disjoint from t3) wakes nobody.
        assert!(lm.release("t2").is_empty());
        assert!(lm.is_queued("t3"));
        // Releasing t1 wakes t3.
        assert_eq!(lm.release("t1"), vec!["t3".to_string()]);
    }
}
