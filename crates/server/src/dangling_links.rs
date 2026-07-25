//! Pure broken/dangling + cyclic symlink classifier (CPE-1008, epic CPE-1002 "File inspection &
//! safety utilities"). Given a caller-supplied list of symlink records — the filesystem walk,
//! `read_link`, and target-existence check are the adapter's job, entirely out of scope here —
//! classify which links are unusable: **missing** (target doesn't exist) or **cyclic** (the chain
//! of symlink targets loops back on itself, incl. a direct self-loop). Pure std, zero
//! dependencies, zero I/O. Deliberately does not create or touch real filesystem symlinks
//! (Windows symlink creation needs elevated privileges and would be flaky/unsafe in CI); the
//! algorithm operates entirely over hand-built [`LinkEntry`] records, same as
//! [`crate::empty_dirs`] / [`crate::organize`] operate on caller-supplied trees/entries.

use std::collections::HashSet;

/// One symlink, already resolved by the caller: its own path, the path it points at (already
/// normalised/resolved by the caller — this module never joins or interprets paths), and whether
/// that target currently exists on disk (also caller-determined).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkEntry {
    /// This symlink's own path (an opaque caller-chosen label, echoed back in results).
    pub path: String,
    /// The path this symlink points at, already resolved/normalised by the caller.
    pub target: String,
    /// Whether `target` currently exists on disk, as determined by the caller.
    pub target_exists: bool,
}

/// Why a link was classified as dangling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DanglingReason {
    /// The target does not exist and following the target chain does not loop.
    Missing,
    /// Following the chain of symlink targets (within the supplied `links`) revisits a path
    /// already seen in the walk — the link can never resolve to real content.
    Cyclic,
}

/// One dangling link and why it was flagged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DanglingLink {
    pub path: String,
    pub reason: DanglingReason,
}

/// Classify every link in `links`, reporting the dangling ones in input order (one entry per
/// dangling link; healthy links are omitted entirely).
///
/// For each link, the target chain is followed (`link.target` → the [`LinkEntry`] whose `path`
/// equals that target → its own `target` → …) using a bounded, visited-set walk so a cycle can
/// never loop forever. **Cyclic takes precedence**: if the walk revisits a path already seen
/// (including a direct self-loop `A → A`), the link is [`DanglingReason::Cyclic`] regardless of
/// any `target_exists` flag along the chain. Otherwise, if `target_exists` is `false`, the link is
/// [`DanglingReason::Missing`]. A link whose target exists and whose chain never cycles is not
/// reported at all.
pub fn scan_dangling(links: &[LinkEntry]) -> Vec<DanglingLink> {
    let by_path: std::collections::HashMap<&str, &LinkEntry> =
        links.iter().map(|l| (l.path.as_str(), l)).collect();

    links
        .iter()
        .filter_map(|link| {
            if is_cyclic(link, &by_path) {
                Some(DanglingLink { path: link.path.clone(), reason: DanglingReason::Cyclic })
            } else if !link.target_exists {
                Some(DanglingLink { path: link.path.clone(), reason: DanglingReason::Missing })
            } else {
                None
            }
        })
        .collect()
}

/// True iff following `start`'s target chain (through `by_path`) revisits a path already seen in
/// this walk. Bounded by construction: `seen` can grow to at most `by_path.len() + 1` entries
/// before either a repeat is found (cyclic) or the chain runs off the supplied link set (not
/// cyclic), so the loop always terminates.
fn is_cyclic(start: &LinkEntry, by_path: &std::collections::HashMap<&str, &LinkEntry>) -> bool {
    let mut seen: HashSet<&str> = HashSet::new();
    seen.insert(start.path.as_str());
    let mut current = start;
    loop {
        let next_path = current.target.as_str();
        if seen.contains(next_path) {
            return true;
        }
        match by_path.get(next_path) {
            Some(&next) => {
                seen.insert(next_path);
                current = next;
            }
            None => return false, // chain runs off the supplied links — not a cycle
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(path: &str, target: &str, target_exists: bool) -> LinkEntry {
        LinkEntry { path: path.into(), target: target.into(), target_exists }
    }

    #[test]
    fn healthy_link_not_reported() {
        let links = vec![link("a", "/some/real/file", true)];
        assert_eq!(scan_dangling(&links), Vec::new());
    }

    #[test]
    fn broken_link_reports_missing() {
        // Target doesn't exist and isn't itself one of the supplied links.
        let links = vec![link("a", "/nowhere", false)];
        assert_eq!(
            scan_dangling(&links),
            vec![DanglingLink { path: "a".into(), reason: DanglingReason::Missing }]
        );
    }

    #[test]
    fn two_cycle_both_reported_cyclic() {
        // a -> b, b -> a; each points at the other's (existing-as-a-link) path.
        let links = vec![link("a", "b", true), link("b", "a", true)];
        assert_eq!(
            scan_dangling(&links),
            vec![
                DanglingLink { path: "a".into(), reason: DanglingReason::Cyclic },
                DanglingLink { path: "b".into(), reason: DanglingReason::Cyclic },
            ]
        );
    }

    #[test]
    fn self_loop_reported_cyclic() {
        let links = vec![link("a", "a", true)];
        assert_eq!(
            scan_dangling(&links),
            vec![DanglingLink { path: "a".into(), reason: DanglingReason::Cyclic }]
        );
    }

    #[test]
    fn three_cycle_all_reported_cyclic() {
        // a -> b -> c -> a
        let links = vec![link("a", "b", true), link("b", "c", true), link("c", "a", true)];
        assert_eq!(
            scan_dangling(&links),
            vec![
                DanglingLink { path: "a".into(), reason: DanglingReason::Cyclic },
                DanglingLink { path: "b".into(), reason: DanglingReason::Cyclic },
                DanglingLink { path: "c".into(), reason: DanglingReason::Cyclic },
            ]
        );
    }

    #[test]
    fn cyclic_precedence_over_false_target_exists() {
        // Same 2-cycle, but one leg is (incorrectly, per the caller) marked target_exists=false.
        // Cyclic still wins over Missing.
        let links = vec![link("a", "b", false), link("b", "a", true)];
        assert_eq!(
            scan_dangling(&links),
            vec![
                DanglingLink { path: "a".into(), reason: DanglingReason::Cyclic },
                DanglingLink { path: "b".into(), reason: DanglingReason::Cyclic },
            ]
        );
    }

    #[test]
    fn chain_into_broken_link_only_flags_the_broken_one() {
        // a -> b (b is itself a supplied link, and a.target_exists is true since b exists as a
        // link); b -> (broken, not among the supplied links, target_exists=false).
        let links = vec![link("a", "b", true), link("b", "/nowhere", false)];
        let result = scan_dangling(&links);
        assert_eq!(result, vec![DanglingLink { path: "b".into(), reason: DanglingReason::Missing }]);
        // Explicitly confirm `a` is absent (not cyclic — the chain terminates at the broken link
        // rather than looping — and its own target_exists is true).
        assert!(!result.iter().any(|d| d.path == "a"));
    }

    #[test]
    fn deterministic_order_with_multiple_danglers() {
        let links = vec![
            link("z", "/nowhere-z", false),
            link("healthy", "/real", true),
            link("m", "/nowhere-m", false),
            link("a", "a", true), // self-loop
        ];
        assert_eq!(
            scan_dangling(&links),
            vec![
                DanglingLink { path: "z".into(), reason: DanglingReason::Missing },
                DanglingLink { path: "m".into(), reason: DanglingReason::Missing },
                DanglingLink { path: "a".into(), reason: DanglingReason::Cyclic },
            ]
        );
    }

    #[test]
    fn empty_input_returns_empty() {
        assert_eq!(scan_dangling(&[]), Vec::new());
    }
}
