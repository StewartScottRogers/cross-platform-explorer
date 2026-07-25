//! Cascade-aware empty-folder finder (CPE-1005, epic CPE-1002 "File inspection & safety
//! utilities"). Given a caller-supplied directory tree (already gathered by the adapter's own
//! filesystem walk — that walk is out of scope here), find directories that are empty **or would
//! become empty once their nested-empty subdirectories are removed**, and report only the
//! **topmost** such directory in each cascade (descendants are implied, never listed alongside
//! their reported ancestor). Pure std, zero I/O, zero new dependencies.

/// A directory and its immediate file count + subdirectories, supplied by the caller. The walk
/// that builds this tree from a real filesystem is the adapter's job, not this module's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirNode {
    /// This directory's path (any caller-chosen representation — used only as an opaque label
    /// and echoed back in the result, never parsed or joined here).
    pub path: String,
    /// Count of regular files directly inside this directory (not counting subdirectories or
    /// files nested inside them).
    pub file_count: usize,
    /// This directory's immediate subdirectories.
    pub children: Vec<DirNode>,
}

impl DirNode {
    /// Convenience constructor for a leaf directory (no subdirectories) with the given file count.
    pub fn leaf(path: impl Into<String>, file_count: usize) -> Self {
        Self { path: path.into(), file_count, children: Vec::new() }
    }
}

/// Find the topmost cascade-empty directories under `root`, in deterministic pre-order.
///
/// A directory is **cascade-empty** iff it has zero files of its own AND every one of its
/// immediate children is itself cascade-empty (recursively — so a chain of nested empty folders
/// all collapses into "cascade-empty" once you reach the bottom). Only the **topmost**
/// cascade-empty directory in each branch is reported: if a directory is cascade-empty, none of
/// its descendants are also listed, since removing the topmost one removes them too. `root`
/// itself may be reported if it is cascade-empty.
pub fn cascade_empty(root: &DirNode) -> Vec<String> {
    let mut out = Vec::new();
    collect(root, &mut out);
    out
}

/// Recursive worker: if `node` is cascade-empty, record its path and stop descending (its
/// descendants are implied). Otherwise recurse into each child in order, letting any cascade-empty
/// subtree report itself.
fn collect(node: &DirNode, out: &mut Vec<String>) {
    if is_cascade_empty(node) {
        out.push(node.path.clone());
        return;
    }
    for child in &node.children {
        collect(child, out);
    }
}

/// True iff `node` has no files of its own and every child is (recursively) cascade-empty.
fn is_cascade_empty(node: &DirNode) -> bool {
    node.file_count == 0 && node.children.iter().all(is_cascade_empty)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tree from the ticket's worked example:
    /// - `a` (1 file) — has content, never reported.
    /// - `b/c` (both empty) — `c` is cascade-empty, so `b` is too; report topmost `b` only.
    /// - `d/e/f` (all empty down the chain) — report topmost `d` only.
    /// - `g` (0 files) with child `g/h` (1 file) — `h` is not cascade-empty, so neither is `g`.
    fn worked_example() -> DirNode {
        DirNode {
            path: "root".into(),
            file_count: 0,
            children: vec![
                DirNode::leaf("a", 1),
                DirNode { path: "b".into(), file_count: 0, children: vec![DirNode::leaf("b/c", 0)] },
                DirNode {
                    path: "d".into(),
                    file_count: 0,
                    children: vec![DirNode {
                        path: "d/e".into(),
                        file_count: 0,
                        children: vec![DirNode::leaf("d/e/f", 0)],
                    }],
                },
                DirNode {
                    path: "g".into(),
                    file_count: 0,
                    children: vec![DirNode::leaf("g/h", 1)],
                },
            ],
        }
    }

    #[test]
    fn reports_topmost_cascade_empty_dirs_only() {
        // `root` itself has content-bearing descendants (a, g/h), so it is not cascade-empty and
        // is not reported; its cascade-empty branches `b` and `d` are reported at their topmost
        // point, never their nested-empty children `b/c` / `d/e` / `d/e/f`.
        assert_eq!(cascade_empty(&worked_example()), vec!["b", "d"]);
    }

    #[test]
    fn dir_with_file_is_not_reported() {
        let tree = DirNode::leaf("a", 1);
        assert_eq!(cascade_empty(&tree), Vec::<String>::new());
    }

    #[test]
    fn dir_whose_descendant_has_a_file_anywhere_below_is_not_reported() {
        // g has 0 files itself, but its child g/h has a file, so g is not cascade-empty and
        // nothing under it is reported either.
        let g = DirNode {
            path: "g".into(),
            file_count: 0,
            children: vec![DirNode {
                path: "g/h".into(),
                file_count: 0,
                children: vec![DirNode::leaf("g/h/i", 1)],
            }],
        };
        assert_eq!(cascade_empty(&g), Vec::<String>::new());
    }

    #[test]
    fn single_empty_node_reports_itself() {
        let tree = DirNode::leaf("only", 0);
        assert_eq!(cascade_empty(&tree), vec!["only"]);
    }

    #[test]
    fn deep_chain_of_empties_collapses_to_the_topmost() {
        let tree = DirNode {
            path: "d".into(),
            file_count: 0,
            children: vec![DirNode {
                path: "d/e".into(),
                file_count: 0,
                children: vec![DirNode::leaf("d/e/f", 0)],
            }],
        };
        assert_eq!(cascade_empty(&tree), vec!["d"]);
    }

    #[test]
    fn siblings_are_each_evaluated_independently() {
        // Two independent empty branches plus one non-empty branch under a root that itself has
        // no files of its own but is NOT cascade-empty (because "keep" has a file) — proves each
        // child subtree is judged on its own, in left-to-right order.
        let tree = DirNode {
            path: "root".into(),
            file_count: 0,
            children: vec![
                DirNode::leaf("empty1", 0),
                DirNode::leaf("keep", 1),
                DirNode::leaf("empty2", 0),
            ],
        };
        assert_eq!(cascade_empty(&tree), vec!["empty1", "empty2"]);
    }

    #[test]
    fn root_itself_reported_when_fully_cascade_empty() {
        let tree = DirNode {
            path: "root".into(),
            file_count: 0,
            children: vec![
                DirNode::leaf("a", 0),
                DirNode { path: "b".into(), file_count: 0, children: vec![DirNode::leaf("b/c", 0)] },
            ],
        };
        assert_eq!(cascade_empty(&tree), vec!["root"]);
    }

    #[test]
    fn order_is_deterministic_pre_order() {
        let tree = DirNode {
            path: "root".into(),
            file_count: 1, // keep root itself out of the report so children are compared
            children: vec![
                DirNode::leaf("z_empty", 0),
                DirNode::leaf("m_empty", 0),
                DirNode::leaf("a_empty", 0),
            ],
        };
        // Reported in input (pre-)order, not sorted alphabetically.
        assert_eq!(cascade_empty(&tree), vec!["z_empty", "m_empty", "a_empty"]);
    }
}
