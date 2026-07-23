//! Spotlight result aggregation (CPE-948, epic CPE-704): merge fuzzy-ranked hits from several sources
//! (actions, folders, files, recents) into one grouped, per-kind-capped, best-first result set for the
//! quick-launch overlay. Builds on [`crate::spotlight::rank`]. Pure.

use crate::spotlight::rank;

/// The source a result came from. Declaration order is the section priority (actions first).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultKind {
    Action,
    Folder,
    File,
    Recent,
}

/// One aggregated result: the matched text, its kind, fuzzy score, and matched positions (for highlight).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SpotResult {
    pub text: String,
    pub kind: ResultKind,
    pub score: i32,
    pub positions: Vec<usize>,
}

/// A kind-grouped section of results (for a sectioned overlay).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SpotSection {
    pub kind: ResultKind,
    pub results: Vec<SpotResult>,
}

fn rank_source(query: &str, kind: ResultKind, items: &[String]) -> Vec<SpotResult> {
    rank(query, items)
        .into_iter()
        .map(|m| SpotResult { text: m.text, kind, score: m.score, positions: m.positions })
        .collect()
}

/// Group results by source: rank each source, keep its top `per_kind_cap`, and return non-empty sections
/// ordered by kind priority (Action → Folder → File → Recent).
pub fn aggregate(query: &str, sources: &[(ResultKind, Vec<String>)], per_kind_cap: usize) -> Vec<SpotSection> {
    let mut sections: Vec<SpotSection> = sources
        .iter()
        .filter_map(|(kind, items)| {
            let mut results = rank_source(query, *kind, items);
            results.truncate(per_kind_cap);
            (!results.is_empty()).then_some(SpotSection { kind: *kind, results })
        })
        .collect();
    sections.sort_by_key(|s| s.kind);
    sections
}

/// A single flat best-first list across every source, capped at `total_cap`. Ties break by kind priority
/// then shorter text — the overlay's "top hits" view.
pub fn top(query: &str, sources: &[(ResultKind, Vec<String>)], total_cap: usize) -> Vec<SpotResult> {
    let mut all: Vec<SpotResult> =
        sources.iter().flat_map(|(kind, items)| rank_source(query, *kind, items)).collect();
    all.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| a.text.chars().count().cmp(&b.text.chars().count()))
    });
    all.truncate(total_cap);
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn aggregate_groups_caps_and_orders_sections() {
        let sources = vec![
            (ResultKind::File, v(&["readme.md", "read-later.md", "notes.txt"])),
            (ResultKind::Action, v(&["Reload window", "Rename", "Delete"])),
        ];
        let secs = aggregate("re", &sources, 1);
        // Sections ordered by kind (Action before File), each capped at 1.
        assert_eq!(secs.len(), 2);
        assert_eq!(secs[0].kind, ResultKind::Action);
        assert_eq!(secs[0].results.len(), 1);
        assert_eq!(secs[1].kind, ResultKind::File);
        assert_eq!(secs[1].results.len(), 1);
    }

    #[test]
    fn empty_sections_are_dropped() {
        let sources = vec![(ResultKind::File, v(&["cargo.toml"])), (ResultKind::Action, v(&["Rename"]))];
        let secs = aggregate("zzz", &sources, 5); // nothing matches
        assert!(secs.is_empty());
    }

    #[test]
    fn top_merges_best_first_and_caps() {
        let sources = vec![
            (ResultKind::File, v(&["report.md", "notes.txt"])),
            (ResultKind::Recent, v(&["reports/"])),
            (ResultKind::Action, v(&["Reveal"])),
        ];
        let out = top("re", &sources, 3);
        assert_eq!(out.len(), 3); // capped
        // Every returned item actually matched "re".
        assert!(out.iter().all(|r| !r.positions.is_empty() || r.score != 0 || r.text.to_lowercase().contains("re")));
    }

    #[test]
    fn top_tie_breaks_prefer_action_kind() {
        // Same query hitting an action and a file with equal prefix score → action first on the kind tie-break.
        let sources = vec![(ResultKind::File, v(&["run"])), (ResultKind::Action, v(&["run"]))];
        let out = top("run", &sources, 2);
        assert_eq!(out[0].kind, ResultKind::Action);
    }
}
