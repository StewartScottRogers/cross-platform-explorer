//! Compress-selection planner (CPE-1056, epic CPE-705): given a user's file/folder selection, map each
//! source path to the inner archive-entry name a compress operation should use, and flag any name
//! collisions before the archive is written. Pure path logic — no filesystem access, no new deps.
//!
//! ## Naming rule
//! - **Default mode** strips the *common ancestor* shared by every source so entries are named relative
//!   to it: `/a/b/c.txt` + `/a/b/d/e.txt` → `c.txt`, `d/e.txt`. A source is never allowed to have its
//!   entire path consumed as "ancestor" — the last path segment is always kept as its own name, so a
//!   lone item plans to its **basename** (this falls out of the same rule, not a separate special case).
//! - **Mixed-root selections** (no path segment shared by every source — e.g. two absolute paths under
//!   different drives/roots) fall back to each source's normalised path with its leading separator
//!   stripped — longer, but still deterministic and still collision-checked.
//! - **`flatten` mode** ignores directory structure entirely: every source maps to its own basename, so
//!   same-named files from different directories land on the same inner name — surfaced as a collision.
//!
//! ## Collisions
//! Two *distinct* sources that plan to the same `archive_name` land in `CompressPlan::collisions` — each
//! colliding name listed once, in the order it first repeats.

/// A single pre-walked selection entry (the caller has already expanded any directory picks; an entry
/// with `is_dir: true` here is a placeholder for an empty directory, not an instruction to walk it).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SelItem {
    pub path: String,
    pub is_dir: bool,
}

/// One source mapped to its planned inner archive name.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PlannedName {
    pub source: String,
    pub archive_name: String,
}

/// The full plan: every source's inner name (same order as the input selection), plus any archive names
/// produced by more than one source.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CompressPlan {
    pub entries: Vec<PlannedName>,
    pub collisions: Vec<String>,
}

/// Plan the inner archive names for `items`. `flatten` drops all directory structure (every source →
/// its basename); otherwise names are relative to the common ancestor of every source. Never panics —
/// an empty selection yields an empty plan.
pub fn plan_compress(items: &[SelItem], flatten: bool) -> CompressPlan {
    if items.is_empty() {
        return CompressPlan { entries: Vec::new(), collisions: Vec::new() };
    }

    let normalized: Vec<String> = items.iter().map(|it| normalize(&it.path)).collect();
    let seg_lists: Vec<Vec<&str>> = normalized.iter().map(|p| segments(p)).collect();

    let names: Vec<String> = if flatten {
        seg_lists.iter().map(|segs| segs.last().copied().unwrap_or("").to_string()).collect()
    } else {
        let keep = common_ancestor_len(&seg_lists);
        seg_lists.iter().map(|segs| segs[keep..].join("/")).collect()
    };

    let entries = items
        .iter()
        .zip(names.iter())
        .map(|(item, name)| PlannedName { source: item.path.clone(), archive_name: name.clone() })
        .collect();

    CompressPlan { entries, collisions: find_collisions(&names) }
}

/// `\` → `/`. Archive tooling is forward-slash internally regardless of host OS.
fn normalize(path: &str) -> String {
    path.replace('\\', "/")
}

/// Non-empty path segments, in order (so a leading/trailing/doubled `/` never produces empty parts).
fn segments(path: &str) -> Vec<&str> {
    path.split('/').filter(|s| !s.is_empty()).collect()
}

/// Longest common prefix length across all segment lists, capped so every item keeps at least its
/// final segment as its own name — that cap is what makes a lone item plan to its basename, and what
/// stops one item's directory from swallowing a nested selection's own entry.
fn common_ancestor_len(seg_lists: &[Vec<&str>]) -> usize {
    let min_len = seg_lists.iter().map(|s| s.len()).min().unwrap_or(0);
    if min_len == 0 {
        return 0;
    }
    let cap = min_len - 1;
    let mut common = 0;
    while common < cap {
        let candidate = seg_lists[0].get(common);
        if seg_lists[1..].iter().any(|segs| segs.get(common) != candidate) {
            break;
        }
        common += 1;
    }
    common
}

/// Names produced by more than one source, in first-repeat order. Deterministic, not a `HashMap`.
fn find_collisions(names: &[String]) -> Vec<String> {
    let mut seen: Vec<(String, usize)> = Vec::new();
    for name in names {
        match seen.iter_mut().find(|(n, _)| n == name) {
            Some(entry) => entry.1 += 1,
            None => seen.push((name.clone(), 1)),
        }
    }
    seen.into_iter().filter(|(_, count)| *count > 1).map(|(name, _)| name).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sel(path: &str, is_dir: bool) -> SelItem {
        SelItem { path: path.to_string(), is_dir }
    }

    fn archive_names(plan: &CompressPlan) -> Vec<&str> {
        plan.entries.iter().map(|e| e.archive_name.as_str()).collect()
    }

    #[test]
    fn common_base_stripping_across_nested_paths() {
        let items = vec![sel("/a/b/c.txt", false), sel("/a/b/d/e.txt", false)];
        let plan = plan_compress(&items, false);
        assert_eq!(archive_names(&plan), vec!["c.txt", "d/e.txt"]);
        assert!(plan.collisions.is_empty());
    }

    #[test]
    fn deeper_nesting_still_strips_to_shared_ancestor() {
        let items = vec![
            sel("/proj/src/main.rs", false),
            sel("/proj/src/lib/util.rs", false),
            sel("/proj/README.md", false),
        ];
        let plan = plan_compress(&items, false);
        assert_eq!(archive_names(&plan), vec!["src/main.rs", "src/lib/util.rs", "README.md"]);
    }

    #[test]
    fn single_file_maps_to_its_basename() {
        let items = vec![sel("/a/b/c.txt", false)];
        let plan = plan_compress(&items, false);
        assert_eq!(archive_names(&plan), vec!["c.txt"]);
        assert!(plan.collisions.is_empty());
    }

    #[test]
    fn single_directory_also_maps_to_its_basename() {
        let items = vec![sel("/a/b/photos", true)];
        let plan = plan_compress(&items, false);
        assert_eq!(archive_names(&plan), vec!["photos"]);
    }

    #[test]
    fn flatten_maps_every_source_to_its_basename_and_surfaces_collision() {
        let items = vec![
            sel("/a/docs/README.md", false),
            sel("/a/pkg/README.md", false),
            sel("/a/pkg/lib.rs", false),
        ];
        let plan = plan_compress(&items, true);
        assert_eq!(archive_names(&plan), vec!["README.md", "README.md", "lib.rs"]);
        assert_eq!(plan.collisions, vec!["README.md".to_string()]);
    }

    #[test]
    fn non_flatten_collision_from_duplicate_selection() {
        // Same source picked twice (or two sources that legitimately resolve to the same relative
        // name) both plan to the same archive_name — still a collision even without `flatten`.
        let items = vec![sel("/a/b/c.txt", false), sel("/a/b/c.txt", false), sel("/a/b/d.txt", false)];
        let plan = plan_compress(&items, false);
        assert_eq!(archive_names(&plan), vec!["c.txt", "c.txt", "d.txt"]);
        assert_eq!(plan.collisions, vec!["c.txt".to_string()]);
    }

    /// Documented rule: when sources share **no** path segment at all (different drives/roots), there
    /// is no common ancestor to strip to — each source falls back to its full normalised path (leading
    /// separator stripped), which is still deterministic and still collision-checked.
    #[test]
    fn mixed_root_selection_falls_back_to_full_relative_paths() {
        let items = vec![sel("/a/x.txt", false), sel("/b/y.txt", false)];
        let plan = plan_compress(&items, false);
        assert_eq!(archive_names(&plan), vec!["a/x.txt", "b/y.txt"]);
        assert!(plan.collisions.is_empty());

        // Windows-style drive roots are the same case after normalisation.
        let items = vec![sel(r"C:\Users\me\a.txt", false), sel(r"D:\backup\b.txt", false)];
        let plan = plan_compress(&items, false);
        assert_eq!(archive_names(&plan), vec!["C:/Users/me/a.txt", "D:/backup/b.txt"]);
    }

    #[test]
    fn forward_and_back_slash_inputs_normalize_identically() {
        let unix = vec![sel("/a/b/c.txt", false), sel("/a/b/d/e.txt", false)];
        let windows = vec![sel(r"\a\b\c.txt", false), sel(r"\a\b\d\e.txt", false)];
        let plan_unix = plan_compress(&unix, false);
        let plan_windows = plan_compress(&windows, false);
        assert_eq!(archive_names(&plan_unix), archive_names(&plan_windows));

        let mixed = vec![sel(r"\a\b\c.txt", false), sel("/a/b/d/e.txt", false)];
        let plan_mixed = plan_compress(&mixed, false);
        assert_eq!(archive_names(&plan_mixed), archive_names(&plan_unix));
    }

    #[test]
    fn empty_selection_yields_empty_plan_no_panic() {
        let plan = plan_compress(&[], false);
        assert!(plan.entries.is_empty());
        assert!(plan.collisions.is_empty());

        let plan = plan_compress(&[], true);
        assert!(plan.entries.is_empty());
        assert!(plan.collisions.is_empty());
    }

    #[test]
    fn entries_preserve_input_order_deterministically() {
        let items = vec![sel("/z/1.txt", false), sel("/a/2.txt", false), sel("/m/3.txt", false)];
        let plan = plan_compress(&items, true);
        let sources: Vec<&str> = plan.entries.iter().map(|e| e.source.as_str()).collect();
        assert_eq!(sources, vec!["/z/1.txt", "/a/2.txt", "/m/3.txt"]);
    }
}
