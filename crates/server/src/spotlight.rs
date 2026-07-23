//! Spotlight fuzzy match + ranking (CPE-937, epic CPE-704): the pure scoring core behind the global
//! quick-launch overlay. Given a query, subsequence-match it against candidate strings (file names,
//! folder paths, action labels), score each match with the usual affordances — consecutive runs,
//! word-boundary and prefix bonuses, gap penalties — and rank best-first. Pure + dependency-free; the
//! overlay UI feeds it strings and renders the ranked hits (with the matched character positions for
//! highlighting).

/// One ranked candidate: its text, score (higher is better), and the matched character indices (into
/// `text`, by `char`) so the UI can highlight them.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SpotlightMatch {
    pub text: String,
    pub score: i32,
    pub positions: Vec<usize>,
}

// Scoring weights — tuned so "consecutive, at a word start, near the front" wins.
const BONUS_CONSECUTIVE: i32 = 8; // this match char immediately follows the previous match
const BONUS_WORD_START: i32 = 10; // match at a word boundary (start, or after a separator)
const BONUS_PREFIX: i32 = 12; // the very first candidate char matches the first query char
const PENALTY_LEADING_GAP: i32 = -2; // per skipped char before the first match (capped)
const PENALTY_GAP: i32 = -1; // per skipped char between matches

fn is_boundary_before(chars: &[char], i: usize) -> bool {
    if i == 0 {
        return true;
    }
    let prev = chars[i - 1];
    // A word start: previous char is a separator, or a lower→upper camelCase hump.
    matches!(prev, ' ' | '-' | '_' | '.' | '/' | '\\')
        || (prev.is_lowercase() && chars[i].is_uppercase())
}

/// Fuzzy-score `candidate` against `query` (both matched case-insensitively). Returns `None` when
/// `query` is not a subsequence of `candidate`; otherwise `Some((score, positions))` where `positions`
/// are the matched char indices into `candidate`. An empty query scores 0 with no positions (everything
/// matches — the overlay shows all items).
pub fn fuzzy_score(query: &str, candidate: &str) -> Option<(i32, Vec<usize>)> {
    let q: Vec<char> = query.chars().flat_map(|c| c.to_lowercase()).collect();
    if q.is_empty() {
        return Some((0, Vec::new()));
    }
    let cand: Vec<char> = candidate.chars().collect();
    let cand_lower: Vec<char> = candidate.chars().flat_map(|c| c.to_lowercase()).collect();
    // Note: to_lowercase can change length; keep matching simple by lowering per-char and assuming a
    // 1:1 mapping for the scripts we care about (identifiers/filenames). Fall back to char-by-char.
    if cand_lower.len() != cand.len() {
        return fuzzy_score_simple(&q, &cand);
    }

    let mut positions = Vec::with_capacity(q.len());
    let mut score = 0i32;
    let mut qi = 0usize;
    let mut last_match: Option<usize> = None;

    for (ci, &lc) in cand_lower.iter().enumerate() {
        if qi >= q.len() {
            break;
        }
        if lc == q[qi] {
            // Bonuses.
            if qi == 0 && ci == 0 {
                score += BONUS_PREFIX;
            }
            if is_boundary_before(&cand, ci) {
                score += BONUS_WORD_START;
            }
            match last_match {
                Some(prev) if prev + 1 == ci => score += BONUS_CONSECUTIVE,
                Some(prev) => score += (PENALTY_GAP * (ci - prev - 1) as i32).max(-15),
                None => score += (PENALTY_LEADING_GAP * ci as i32).max(-10),
            }
            positions.push(ci);
            last_match = Some(ci);
            qi += 1;
        }
    }

    if qi == q.len() {
        // Slightly prefer shorter candidates (a tie-break toward tighter matches).
        score -= cand.len() as i32 / 8;
        Some((score, positions))
    } else {
        None
    }
}

/// Fallback for candidates whose lowercase changes length (rare, non-identifier scripts): a plain
/// subsequence check on the lowercased chars with a flat score, so ranking still works.
fn fuzzy_score_simple(q: &[char], cand: &[char]) -> Option<(i32, Vec<usize>)> {
    let lower: Vec<char> = cand.iter().flat_map(|c| c.to_lowercase()).collect();
    let mut qi = 0;
    for &c in &lower {
        if qi < q.len() && c == q[qi] {
            qi += 1;
        }
    }
    if qi == q.len() {
        Some((1, Vec::new()))
    } else {
        None
    }
}

/// Rank `items` against `query`, best-first. Non-matches are dropped. Ties (equal score) keep a stable,
/// friendly order: shorter text first, then the original order. An empty/whitespace query returns every
/// item in its original order (score 0) so the overlay lists everything.
pub fn rank(query: &str, items: &[String]) -> Vec<SpotlightMatch> {
    let q = query.trim();
    let mut scored: Vec<(usize, SpotlightMatch)> = items
        .iter()
        .enumerate()
        .filter_map(|(i, text)| {
            fuzzy_score(q, text).map(|(score, positions)| (i, SpotlightMatch { text: text.clone(), score, positions }))
        })
        .collect();
    scored.sort_by(|(ai, a), (bi, b)| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.text.chars().count().cmp(&b.text.chars().count()))
            .then_with(|| ai.cmp(bi))
    });
    scored.into_iter().map(|(_, m)| m).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_subsequence_does_not_match() {
        assert!(fuzzy_score("xyz", "readme.md").is_none());
        assert!(fuzzy_score("mdd", "readme.md").is_none()); // not enough d's in order
    }

    #[test]
    fn subsequence_matches_and_reports_positions() {
        let (_, pos) = fuzzy_score("rme", "readme.md").unwrap();
        // r(0) … m(4) e(5)  → picks the first r, then m, then e.
        assert_eq!(pos, vec![0, 4, 5]);
    }

    #[test]
    fn empty_query_matches_everything_with_zero_score() {
        assert_eq!(fuzzy_score("", "anything").unwrap().0, 0);
    }

    #[test]
    fn case_insensitive() {
        assert!(fuzzy_score("RD", "readme").is_some());
        assert!(fuzzy_score("rd", "README").is_some());
    }

    #[test]
    fn word_start_and_prefix_beat_mid_word() {
        // "fb" matches the word-starts of "foo-bar" better than the run in "affbe".
        let a = fuzzy_score("fb", "foo-bar").unwrap().0; // f at start (prefix+boundary), b at word start
        let b = fuzzy_score("fb", "affbe").unwrap().0; // f,b mid-word consecutive
        assert!(a > b, "word-start/prefix ({a}) should beat mid-word ({b})");
    }

    #[test]
    fn consecutive_beats_scattered() {
        let tight = fuzzy_score("abc", "abcxx").unwrap().0; // consecutive
        let loose = fuzzy_score("abc", "axbxc").unwrap().0; // gaps
        assert!(tight > loose, "consecutive ({tight}) should beat scattered ({loose})");
    }

    #[test]
    fn rank_orders_best_first_and_drops_non_matches() {
        let items: Vec<String> = ["readme.md", "notes.txt", "read-later.md", "cargo.toml"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let out = rank("re", &items);
        // Both "read…" files match "re" at their prefix; "notes"/"cargo" drop out. The shorter
        // "readme.md" ranks above "read-later.md" on the length tie-break.
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].text, "readme.md");
        assert_eq!(out[1].text, "read-later.md");
    }

    #[test]
    fn empty_query_returns_all_in_order() {
        let items: Vec<String> = ["b", "a", "c"].iter().map(|s| s.to_string()).collect();
        let out = rank("  ", &items);
        assert_eq!(out.iter().map(|m| m.text.as_str()).collect::<Vec<_>>(), vec!["b", "a", "c"]);
    }
}
