//! Near-identical-**folder** detection via Jaccard similarity over file-content-hash sets
//! (CPE-1007, epic CPE-1002 "File inspection & safety utilities"). A level up from
//! [`crate::duplicates`], which finds byte-identical *files*: this module finds folders that are
//! **~the same** even though they aren't byte-identical as a whole — e.g. `Photos/` vs.
//! `Photos (backup)/` sharing 90% of their files. Pure over caller-supplied
//! (folder-path, set-of-file-hashes) pairs; the filesystem walk + per-file hashing that build
//! those sets is the adapter's job (reusing [`crate::duplicates`]'s hashing there), out of scope
//! here. Pure std, zero I/O, zero new dependencies.

use std::collections::BTreeSet;

/// Jaccard similarity of two sets: `|A ∩ B| / |A ∪ B|`, in `[0.0, 1.0]`.
///
/// `1.0` means the sets are identical; `0.0` means they share nothing. **`jaccard(∅, ∅) = 0.0`**
/// by definition here — two empty folders (no file hashes at all) are not a meaningful "similar"
/// match, so the empty/empty case is defined as "not similar" rather than the set-theoretic
/// convention of `0/0 = 1`. This also sidesteps a `0.0 / 0.0 = NaN` division: the result is
/// **never** `NaN` for any input.
pub fn jaccard(a: &BTreeSet<String>, b: &BTreeSet<String>) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    let intersection = a.intersection(b).count();
    let union = a.union(b).count();
    // union == 0 only when both sets are empty, already handled above, but guard defensively so
    // this can never divide by zero / produce NaN regardless of future edits above.
    if union == 0 {
        return 0.0;
    }
    intersection as f64 / union as f64
}

/// A folder's path paired with the set of its (already-computed, e.g. SHA-256) file-content
/// hashes. Building this from a real filesystem — walking the tree and hashing each file — is the
/// adapter's job (see [`crate::duplicates`] for the hashing primitives); this module only
/// compares sets it's handed.
#[derive(Debug, Clone)]
pub struct FolderHashes {
    pub path: String,
    pub hashes: BTreeSet<String>,
}

/// Group folder paths whose pairwise [`jaccard`] similarity is `>= threshold` into near-identical
/// clusters, transitively (single-link clustering, mirroring [`crate::perceptual::cluster`]'s
/// union-find approach but keyed on a *float similarity floor* instead of an integer distance
/// ceiling): if A~B and B~C but A and C alone fall short of `threshold`, A/B/C still land in one
/// group. Implemented with union-find over folder indices, so the transitive closure is O(n²)
/// pairwise comparisons + near-linear union/find.
///
/// Only groups with **2 or more** members are returned — a folder with nothing similar enough is a
/// dropped singleton, not a group of one. Each group's paths are sorted for a stable order, and
/// the groups themselves are sorted by their (already-sorted) first path, so the result is fully
/// deterministic regardless of `folders`' input order.
///
/// `threshold` is a plain caller parameter (no default baked in here); a sensible default for
/// "near-identical folder" is documented on the ticket (0.8 — see CPE-1007's Work Log), but that
/// judgment call belongs to the adapter/UI, not this pure core.
pub fn cluster_similar_folders(folders: &[FolderHashes], threshold: f64) -> Vec<Vec<String>> {
    let n = folders.len();
    if n < 2 {
        return Vec::new();
    }

    // Union-find over folder indices (same shape as `perceptual::cluster`).
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], x: usize) -> usize {
        if parent[x] != x {
            parent[x] = find(parent, parent[x]); // path compression
        }
        parent[x]
    }
    fn union(parent: &mut [usize], a: usize, b: usize) {
        let ra = find(parent, a);
        let rb = find(parent, b);
        if ra != rb {
            parent[ra] = rb;
        }
    }

    for i in 0..n {
        for j in (i + 1)..n {
            if jaccard(&folders[i].hashes, &folders[j].hashes) >= threshold {
                union(&mut parent, i, j);
            }
        }
    }

    // Bucket indices by root, then keep only buckets with 2+ members.
    let mut groups: std::collections::HashMap<usize, Vec<String>> = std::collections::HashMap::new();
    for (i, folder) in folders.iter().enumerate() {
        let root = find(&mut parent, i);
        groups.entry(root).or_default().push(folder.path.clone());
    }

    let mut result: Vec<Vec<String>> =
        groups.into_values().filter(|g| g.len() >= 2).map(|mut g| { g.sort(); g }).collect();
    result.sort_by(|a, b| a[0].cmp(&b[0]));
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-9;

    fn set(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    // ---- jaccard ----

    #[test]
    fn jaccard_identical_sets_is_one() {
        let a = set(&["h1", "h2", "h3"]);
        let b = set(&["h1", "h2", "h3"]);
        assert!((jaccard(&a, &b) - 1.0).abs() < EPS);
    }

    #[test]
    fn jaccard_disjoint_sets_is_zero() {
        let a = set(&["h1", "h2"]);
        let b = set(&["h3", "h4"]);
        assert!((jaccard(&a, &b) - 0.0).abs() < EPS);
    }

    #[test]
    fn jaccard_known_overlap() {
        // {a,b,c,d} vs {a,b,c,e}: intersection {a,b,c} = 3, union {a,b,c,d,e} = 5 -> 3/5 = 0.6.
        let a = set(&["a", "b", "c", "d"]);
        let b = set(&["a", "b", "c", "e"]);
        assert!((jaccard(&a, &b) - 0.6).abs() < EPS, "expected 0.6, got {}", jaccard(&a, &b));
    }

    #[test]
    fn jaccard_both_empty_is_zero_not_nan() {
        let a: BTreeSet<String> = BTreeSet::new();
        let b: BTreeSet<String> = BTreeSet::new();
        let result = jaccard(&a, &b);
        assert!(!result.is_nan(), "empty/empty must not be NaN");
        assert!((result - 0.0).abs() < EPS, "empty/empty is defined as 0.0, got {result}");
    }

    #[test]
    fn jaccard_one_empty_one_not_is_zero() {
        let a: BTreeSet<String> = BTreeSet::new();
        let b = set(&["h1"]);
        assert!((jaccard(&a, &b) - 0.0).abs() < EPS);
        assert!((jaccard(&b, &a) - 0.0).abs() < EPS, "symmetric");
    }

    #[test]
    fn jaccard_result_is_always_in_unit_range() {
        let a = set(&["h1", "h2", "h3", "h4"]);
        let b = set(&["h2", "h3"]);
        let r = jaccard(&a, &b);
        assert!((0.0..=1.0).contains(&r));
    }

    // ---- cluster_similar_folders ----

    fn folder(path: &str, hashes: &[&str]) -> FolderHashes {
        FolderHashes { path: path.to_string(), hashes: set(hashes) }
    }

    #[test]
    fn near_pair_groups_and_disjoint_singleton_is_omitted() {
        // "Photos" and "Photos (backup)" share 9/10 hashes -> jaccard 0.9 (>= 0.8 threshold).
        // "Unrelated" shares nothing.
        let photos = folder("Photos", &["h1", "h2", "h3", "h4", "h5", "h6", "h7", "h8", "h9", "h10"]);
        let backup = folder(
            "Photos (backup)",
            &["h1", "h2", "h3", "h4", "h5", "h6", "h7", "h8", "h9", "hX"],
        );
        let unrelated = folder("Unrelated", &["z1", "z2", "z3"]);

        let groups = cluster_similar_folders(&[photos, backup, unrelated], 0.8);

        assert_eq!(groups.len(), 1, "only the near pair forms a group");
        assert_eq!(groups[0], vec!["Photos".to_string(), "Photos (backup)".to_string()]);
    }

    #[test]
    fn identical_folders_cluster() {
        let a = folder("A", &["h1", "h2", "h3"]);
        let b = folder("B", &["h1", "h2", "h3"]);
        let groups = cluster_similar_folders(&[a, b], 0.8);
        assert_eq!(groups, vec![vec!["A".to_string(), "B".to_string()]]);
    }

    #[test]
    fn transitive_single_link_chains_a_b_c() {
        // A~B and B~C at >= threshold, but A and C alone fall short of threshold directly.
        // A = {1,2,3,4,5,6,7,8,9,10} vs B = {1,2,3,4,5,6,7,8,9,X}   -> 9/11 ≈ 0.818
        // B = {1,2,3,4,5,6,7,8,9,X} vs C = {1,2,3,4,5,6,7,8,Y,X}   -> 8/12 ≈ 0.667... too low.
        // Use a cleaner construction to make the "A-C directly below threshold" property exact.
        let a = folder("A", &["1", "2", "3", "4", "5", "6", "7", "8", "9"]);
        let b = folder("B", &["2", "3", "4", "5", "6", "7", "8", "9", "10"]); // vs A: 8/10=0.8
        let c = folder("C", &["3", "4", "5", "6", "7", "8", "9", "10", "11"]); // vs B: 8/10=0.8
        // A vs C: intersection {3,4,5,6,7,8,9} = 7, union = {1,2,3,4,5,6,7,8,9,10,11} = 11 -> 7/11 ≈ 0.636
        let a_c = jaccard(&a.hashes, &c.hashes);
        assert!(a_c < 0.8, "A and C must NOT be directly similar enough (got {a_c})");

        let groups = cluster_similar_folders(&[a, b, c], 0.8);
        assert_eq!(groups.len(), 1, "single-link transitivity should chain A-B-C into one group");
        assert_eq!(groups[0], vec!["A".to_string(), "B".to_string(), "C".to_string()]);
    }

    #[test]
    fn threshold_boundary_is_inclusive() {
        // Exactly at threshold (0.6) must be included (`>=`, not `>`).
        let a = folder("A", &["a", "b", "c", "d"]);
        let b = folder("B", &["a", "b", "c", "e"]); // jaccard == 0.6 exactly
        let j = jaccard(&a.hashes, &b.hashes);
        assert!((j - 0.6).abs() < EPS, "fixture sanity check");

        let groups_at = cluster_similar_folders(&[a.clone(), b.clone()], 0.6);
        assert_eq!(groups_at, vec![vec!["A".to_string(), "B".to_string()]], "boundary value included (>=)");

        let groups_above = cluster_similar_folders(&[a, b], 0.600_001);
        assert!(groups_above.is_empty(), "just above the exact similarity excludes the pair");
    }

    #[test]
    fn no_pair_meets_threshold_returns_empty() {
        let a = folder("A", &["1", "2"]);
        let b = folder("B", &["3", "4"]);
        assert!(cluster_similar_folders(&[a, b], 0.5).is_empty());
    }

    #[test]
    fn empty_input_returns_empty() {
        assert!(cluster_similar_folders(&[], 0.8).is_empty());
    }

    #[test]
    fn single_folder_returns_empty() {
        let a = folder("A", &["1", "2"]);
        assert!(cluster_similar_folders(&[a], 0.8).is_empty());
    }

    #[test]
    fn output_is_deterministic_and_input_order_independent() {
        let a = folder("zzz", &["1", "2", "3"]);
        let b = folder("aaa", &["1", "2", "3"]);
        let c = folder("mmm", &["9", "9", "9"]); // dup literal collapses to one hash in the set
        let d = folder("nnn", &["9"]);

        let groups_1 = cluster_similar_folders(&[a.clone(), b.clone(), c.clone(), d.clone()], 0.9);
        let groups_2 = cluster_similar_folders(&[d, c, b, a], 0.9);

        assert_eq!(groups_1, groups_2, "result must not depend on input order");
        assert_eq!(groups_1.len(), 2);
        // Groups sorted by their own (sorted) first path.
        assert_eq!(groups_1[0], vec!["aaa".to_string(), "zzz".to_string()]);
        assert_eq!(groups_1[1], vec!["mmm".to_string(), "nnn".to_string()]);
    }

    #[test]
    fn folders_with_empty_hash_sets_never_cluster_at_a_realistic_threshold() {
        // Two folders with no hashes at all: jaccard(empty, empty) = 0.0 (the documented
        // convention), so at any positive threshold — the realistic/default case — they never
        // cluster as "similar".
        let a = folder("Empty1", &[]);
        let b = folder("Empty2", &[]);
        let j = jaccard(&a.hashes, &b.hashes);
        assert!((j - 0.0).abs() < EPS);
        let groups = cluster_similar_folders(&[a, b], 0.01);
        assert!(groups.is_empty(), "empty folders must not cluster at any realistic threshold");
    }
}
