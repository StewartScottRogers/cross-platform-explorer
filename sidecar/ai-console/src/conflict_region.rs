//! Region heat-map rollup (CPE-1070, epic CPE-730): aggregate per-file attribution
//! ([`conflict_owner::PathAttribution`]) up to folder-level regions, so the heat-map can colour
//! a whole directory by its dominant agent and how contended it is. Pure aggregation over
//! already-computed attribution — no recursion, no I/O, no new deps. Reuses `conflict_owner`'s
//! tie-break rule (lexically-least agent) for a region's dominant owner. Does not modify
//! `conflict_owner.rs` or `conflict.rs`.

use std::cmp::Reverse;
use std::collections::BTreeMap;

use crate::conflict_owner::PathAttribution;

/// A directory region's dominant owner and contention count, for heat-map colouring.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionOwner {
    /// The region's `/`-joined directory segments (e.g. `"a/b"`), no leading/trailing slash.
    pub region: String,
    /// The agent owning the majority of files under this region (see [`roll_up`] for the
    /// tie-break rule).
    pub owner: String,
    /// Count of files under this region with 2+ contributors.
    pub contended_files: u32,
}

/// Split `path` into its non-empty segments, normalizing `\` to `/` at this boundary so the
/// same logic runs whether the caller hands in POSIX- or Windows-style paths.
///
/// Deliberately segment-based rather than `std::path::Path` or string `starts_with`: comparing
/// raw strings would treat sibling directory `"ab"` as sharing a prefix with `"a"` (a naive
/// `region + "/"`-prefix check on `"ab/y.rs"` can still misfire depending on how the region
/// string is built), silently folding two distinct directories into one region. Comparing whole
/// segments for equality — never substrings — avoids that collision entirely.
fn segments(path: &str) -> Vec<String> {
    path.replace('\\', "/")
        .split('/')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Roll per-file `attributions` up to directory regions, at most `max_depth` segments deep.
///
/// For each attribution, every directory prefix of its path (its segments minus the trailing
/// filename, capped at `max_depth`) casts one vote for that attribution's `owner`, plus a
/// contention tally if the file has 2+ contributors. A region's dominant owner is whichever
/// owner has the most votes; ties are broken by the lexically-least agent name — the same rule
/// [`conflict_owner::attribute`] uses to pick a single file's owner.
///
/// Iterative, not recursive: a pathological deep path just contributes `max_depth` votes and
/// stops, so nothing can blow the stack or run unbounded. Counts accumulate with
/// `u32::saturating_add`. Empty input yields empty output. Output is sorted by region (a plain
/// `BTreeMap<String, _>` sorts byte-wise, which is deterministic across runs and platforms).
pub fn roll_up(attributions: &[PathAttribution], max_depth: usize) -> Vec<RegionOwner> {
    // region -> (owner -> file votes, contended-file count)
    let mut regions: BTreeMap<String, (BTreeMap<String, u32>, u32)> = BTreeMap::new();

    for attr in attributions {
        let segs = segments(&attr.path);
        if segs.len() < 2 {
            continue; // no directory component (a bare filename at the root) — nothing to roll up
        }
        let dir_segs = &segs[..segs.len() - 1];
        let depth = dir_segs.len().min(max_depth);
        let contended = attr.contributors.len() >= 2;

        let mut region = String::new();
        for seg in dir_segs.iter().take(depth) {
            if !region.is_empty() {
                region.push('/');
            }
            region.push_str(seg);

            let entry = regions.entry(region.clone()).or_insert_with(|| (BTreeMap::new(), 0));
            let votes = entry.0.entry(attr.owner.clone()).or_insert(0);
            *votes = votes.saturating_add(1);
            if contended {
                entry.1 = entry.1.saturating_add(1);
            }
        }
    }

    regions
        .into_iter()
        .map(|(region, (votes, contended_files))| {
            // Most votes wins; on a full tie, the lexically-least owner (mirrors
            // conflict_owner::attribute's per-file tie-break).
            let owner = votes
                .into_iter()
                .min_by_key(|(owner, count)| (Reverse(*count), owner.clone()))
                .map(|(owner, _)| owner)
                .unwrap_or_default();
            RegionOwner { region, owner, contended_files }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attr(path: &str, contributors: &[(&str, u32, u32)], owner: &str) -> PathAttribution {
        PathAttribution {
            path: path.to_string(),
            contributors: contributors.iter().map(|(a, e, d)| (a.to_string(), *e, *d)).collect(),
            owner: owner.to_string(),
        }
    }

    #[test]
    fn empty_input_yields_empty_output() {
        assert!(roll_up(&[], 4).is_empty());
    }

    #[test]
    fn files_under_dir_roll_up_to_majority_owner() {
        let attrs = [
            attr("a/x.rs", &[("alice", 1, 0)], "alice"),
            attr("a/y.rs", &[("alice", 1, 0)], "alice"),
            attr("a/z.rs", &[("bob", 1, 0)], "bob"),
        ];
        let out = roll_up(&attrs, 4);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].region, "a");
        assert_eq!(out[0].owner, "alice");
        assert_eq!(out[0].contended_files, 0);
    }

    #[test]
    fn tie_on_votes_broken_by_lexically_least_owner() {
        let attrs = [
            attr("dir/x.rs", &[("zoe", 1, 0)], "zoe"),
            attr("dir/y.rs", &[("amy", 1, 0)], "amy"),
        ];
        let out = roll_up(&attrs, 4);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].owner, "amy");
    }

    /// The critical probe: a sibling directory whose name is a prefix of another (`a` vs `ab`)
    /// must roll up to two SEPARATE regions, never merge into one.
    #[test]
    fn prefix_collision_a_and_ab_stay_separate_regions() {
        let attrs = [
            attr("a/x.rs", &[("alice", 1, 0)], "alice"),
            attr("ab/y.rs", &[("bob", 1, 0)], "bob"),
        ];
        let out = roll_up(&attrs, 4);
        let mut regions: Vec<&str> = out.iter().map(|r| r.region.as_str()).collect();
        regions.sort();
        assert_eq!(regions, vec!["a", "ab"]);

        let a = out.iter().find(|r| r.region == "a").unwrap();
        let ab = out.iter().find(|r| r.region == "ab").unwrap();
        assert_eq!(a.owner, "alice");
        assert_eq!(ab.owner, "bob");
    }

    #[test]
    fn depth_is_capped_at_max_depth() {
        let attrs = [attr("a/b/c/file.rs", &[("alice", 1, 0)], "alice")];
        let out = roll_up(&attrs, 1);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].region, "a");
    }

    #[test]
    fn max_depth_zero_yields_no_regions() {
        let attrs = [attr("a/b/file.rs", &[("alice", 1, 0)], "alice")];
        assert!(roll_up(&attrs, 0).is_empty());
    }

    #[test]
    fn file_at_root_with_no_directory_produces_no_region() {
        let attrs = [attr("file.rs", &[("alice", 1, 0)], "alice")];
        assert!(roll_up(&attrs, 4).is_empty());
    }

    #[test]
    fn windows_style_paths_are_normalized_to_segments() {
        let attrs = [attr("a\\b\\file.rs", &[("alice", 1, 0)], "alice")];
        let out = roll_up(&attrs, 4);
        let mut regions: Vec<&str> = out.iter().map(|r| r.region.as_str()).collect();
        regions.sort();
        assert_eq!(regions, vec!["a", "a/b"]);
    }

    #[test]
    fn contended_files_counts_multi_contributor_files() {
        let attrs = [
            attr("dir/shared.rs", &[("alice", 2, 0), ("bob", 1, 0)], "alice"),
            attr("dir/solo.rs", &[("alice", 1, 0)], "alice"),
        ];
        let out = roll_up(&attrs, 4);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].contended_files, 1);
    }

    #[test]
    fn contended_files_accumulates_with_saturating_add_across_many_files() {
        let attrs: Vec<PathAttribution> = (0..5)
            .map(|i| attr(&format!("dir/f{i}.rs"), &[("alice", 1, 0), ("bob", 1, 0)], "alice"))
            .collect();
        let out = roll_up(&attrs, 4);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].contended_files, 5);
    }

    #[test]
    fn nested_regions_can_have_different_dominant_owners() {
        let attrs = [
            attr("a/b/x.rs", &[("alice", 1, 0)], "alice"),
            attr("a/b/y.rs", &[("bob", 1, 0)], "bob"),
            attr("a/other.rs", &[("bob", 1, 0)], "bob"),
        ];
        let out = roll_up(&attrs, 4);
        assert_eq!(out.len(), 2);
        let a = out.iter().find(|r| r.region == "a").unwrap();
        let ab = out.iter().find(|r| r.region == "a/b").unwrap();
        // Region "a" sees votes alice:1, bob:2 (from a/b/y.rs and a/other.rs) -> bob dominates.
        assert_eq!(a.owner, "bob");
        // Region "a/b" sees votes alice:1, bob:1 -> tie, lexically-least "alice" wins.
        assert_eq!(ab.owner, "alice");
    }
}
