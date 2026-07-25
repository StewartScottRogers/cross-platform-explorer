//! Archive expansion-ratio / zip-bomb safety score (CPE-1004, epic CPE-1002). Pure: it scores
//! per-entry compressed/uncompressed size metadata the caller already has from an archive listing
//! (e.g. [`crate::archive::ArchiveEntry`]-shaped data) — no extraction, no filesystem I/O, no new
//! dependencies. A high uncompressed:compressed ratio is the classic zip-bomb signature (a tiny
//! archive that expands to gigabytes); this module flags entries and the archive as a whole against
//! configurable thresholds so a caller can warn before extracting. Wiring real `compressed_size`
//! out of the `zip`/`tar`/`sevenz-rust` crates into `archive.rs`'s listing is a later adapter
//! concern — kept out of this ticket to avoid touching that multi-format dispatch plumbing.

/// One archive entry's compressed and uncompressed size — the input the caller supplies (typically
/// read straight off an archive's central directory / header during listing).
pub struct EntrySizes {
    pub name: String,
    pub compressed: u64,
    pub uncompressed: u64,
}

/// An entry whose expansion ratio exceeded [`RatioLimits::max_entry_ratio`].
#[derive(Debug, Clone, PartialEq)]
pub struct FlaggedEntry {
    pub name: String,
    pub ratio: f64,
}

/// The result of scoring an archive's entries: totals, the overall ratio, any per-entry flags, and
/// a single `dangerous` verdict.
#[derive(Debug, Clone, PartialEq)]
pub struct RatioReport {
    pub total_compressed: u64,
    pub total_uncompressed: u64,
    pub overall_ratio: f64,
    pub flagged: Vec<FlaggedEntry>,
    pub dangerous: bool,
}

/// Heuristic zip-bomb thresholds. A legitimate archive of mixed content (text, images, already-
/// compressed formats) rarely exceeds a ~10-20x expansion ratio; 100x is generous headroom that
/// still catches the pathological cases (a single-entry deflate bomb can reach 1000x+) without
/// false-flagging a merely well-compressed archive (e.g. a large text log deflating ~10x).
pub struct RatioLimits {
    pub max_entry_ratio: f64,
    pub max_overall_ratio: f64,
}

/// Default per-entry expansion-ratio threshold (see [`RatioLimits`] doc for the reasoning).
pub const DEFAULT_MAX_ENTRY_RATIO: f64 = 100.0;
/// Default overall (whole-archive) expansion-ratio threshold.
pub const DEFAULT_MAX_OVERALL_RATIO: f64 = 100.0;

impl Default for RatioLimits {
    fn default() -> Self {
        RatioLimits { max_entry_ratio: DEFAULT_MAX_ENTRY_RATIO, max_overall_ratio: DEFAULT_MAX_OVERALL_RATIO }
    }
}

impl RatioLimits {
    /// Build limits with explicit thresholds (for a caller that wants stricter/looser scoring than
    /// the [`Default`]).
    pub const fn new(max_entry_ratio: f64, max_overall_ratio: f64) -> Self {
        RatioLimits { max_entry_ratio, max_overall_ratio }
    }
}

/// `uncompressed / compressed`, guarding the divide-by-zero case a stored/empty entry (or an
/// all-zero archive) would otherwise hit: `compressed == 0` and `uncompressed == 0` scores `0.0`
/// (nothing expanded — not dangerous); `compressed == 0` and `uncompressed > 0` scores
/// [`f64::INFINITY`] (infinite expansion from zero bytes — always flagged as the most dangerous
/// case). Both branches are exact and can never produce `NaN`.
fn ratio(uncompressed: u64, compressed: u64) -> f64 {
    if compressed == 0 {
        if uncompressed == 0 {
            0.0
        } else {
            f64::INFINITY
        }
    } else {
        uncompressed as f64 / compressed as f64
    }
}

/// Score an archive's entries for expansion-ratio zip-bomb risk. Sums compressed/uncompressed
/// totals, computes the overall ratio, and flags (in input order) every entry whose own ratio
/// exceeds `limits.max_entry_ratio`. `dangerous` is true when the overall ratio exceeds
/// `limits.max_overall_ratio`, OR at least one entry was flagged — either condition alone is
/// reason to warn before extracting.
pub fn expansion_ratio(entries: &[EntrySizes], limits: &RatioLimits) -> RatioReport {
    let mut total_compressed: u64 = 0;
    let mut total_uncompressed: u64 = 0;
    let mut flagged = Vec::new();

    for entry in entries {
        // Saturating: a pathological/crafted archive could claim sizes that overflow a running u64
        // sum; saturate rather than panic or wrap, since a saturated total only ever makes the
        // resulting ratio look *more* dangerous, never masks a real bomb.
        total_compressed = total_compressed.saturating_add(entry.compressed);
        total_uncompressed = total_uncompressed.saturating_add(entry.uncompressed);

        let entry_ratio = ratio(entry.uncompressed, entry.compressed);
        if entry_ratio > limits.max_entry_ratio {
            flagged.push(FlaggedEntry { name: entry.name.clone(), ratio: entry_ratio });
        }
    }

    let overall_ratio = ratio(total_uncompressed, total_compressed);
    let dangerous = overall_ratio > limits.max_overall_ratio || !flagged.is_empty();

    RatioReport { total_compressed, total_uncompressed, overall_ratio, flagged, dangerous }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, compressed: u64, uncompressed: u64) -> EntrySizes {
        EntrySizes { name: name.to_string(), compressed, uncompressed }
    }

    #[test]
    fn normal_archive_is_not_dangerous() {
        // Mixed content: ~2x-10x per entry, nothing alarming.
        let entries = vec![
            entry("readme.txt", 500, 1_000),   // 2x
            entry("photo.jpg", 900_000, 950_000), // ~1.06x (already-compressed format)
            entry("data.csv", 10_000, 100_000), // 10x
        ];
        let report = expansion_ratio(&entries, &RatioLimits::default());

        assert_eq!(report.total_compressed, 500 + 900_000 + 10_000);
        assert_eq!(report.total_uncompressed, 1_000 + 950_000 + 100_000);
        let expected_overall = report.total_uncompressed as f64 / report.total_compressed as f64;
        assert!((report.overall_ratio - expected_overall).abs() < 1e-9);
        assert!(report.flagged.is_empty(), "no entry should be flagged: {:?}", report.flagged);
        assert!(!report.dangerous);
    }

    #[test]
    fn a_crafted_zip_bomb_entry_is_flagged_and_dangerous() {
        let entries = vec![
            entry("normal.txt", 500, 1_000), // 2x, fine on its own
            entry("bomb.bin", 1_000, 10_000_000), // 10,000x — classic zip-bomb ratio
        ];
        let report = expansion_ratio(&entries, &RatioLimits::default());

        assert_eq!(report.flagged.len(), 1);
        assert_eq!(report.flagged[0].name, "bomb.bin");
        assert!((report.flagged[0].ratio - 10_000.0).abs() < 1e-6);
        assert!(report.dangerous);
    }

    #[test]
    fn zero_compressed_with_uncompressed_is_infinite_and_flagged() {
        let entries = vec![entry("weird.bin", 0, 1)];
        let report = expansion_ratio(&entries, &RatioLimits::default());

        assert_eq!(report.flagged.len(), 1);
        assert!(report.flagged[0].ratio.is_infinite());
        assert!(!report.flagged[0].ratio.is_nan());
        assert!(report.dangerous);
        assert!(report.overall_ratio.is_infinite());
        assert!(!report.overall_ratio.is_nan());
    }

    #[test]
    fn all_zero_entry_is_zero_ratio_and_not_dangerous() {
        let entries = vec![entry("empty.bin", 0, 0)];
        let report = expansion_ratio(&entries, &RatioLimits::default());

        assert!(report.flagged.is_empty());
        assert_eq!(report.overall_ratio, 0.0);
        assert!(!report.dangerous);
    }

    #[test]
    fn overall_ratio_can_be_dangerous_even_when_no_single_entry_is_flagged() {
        // Three entries, each 90x — every one individually passes a 100x max_entry_ratio, so nothing
        // gets flagged. But max_entry_ratio and max_overall_ratio are independent knobs: tighten just
        // the overall threshold to 50x (below the 90x archive-wide ratio) to prove `dangerous` can
        // trip purely off `overall_ratio`, via the OR in its definition, with an empty `flagged` list.
        let entries = vec![
            entry("a", 1, 90), // 90x, under the 100x entry threshold
            entry("b", 1, 90), // 90x, under the 100x entry threshold
            entry("c", 1, 90), // 90x, under the 100x entry threshold
        ];
        let limits = RatioLimits::new(100.0, 50.0);
        let report = expansion_ratio(&entries, &limits);

        assert!(report.flagged.is_empty(), "no entry individually crosses max_entry_ratio");
        assert!(report.overall_ratio > 50.0, "overall ratio should exceed the tightened threshold");
        assert!(report.dangerous, "dangerous via overall_ratio even though nothing was flagged");
    }

    #[test]
    fn flag_order_is_deterministic_input_order() {
        let entries = vec![
            entry("first-bomb", 1, 1_000), // 1000x
            entry("normal", 100, 200),     // 2x
            entry("second-bomb", 1, 5_000), // 5000x
        ];
        let report = expansion_ratio(&entries, &RatioLimits::default());

        let names: Vec<&str> = report.flagged.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["first-bomb", "second-bomb"], "flags preserve input order");
    }

    #[test]
    fn empty_entry_list_is_all_zero_and_not_dangerous() {
        let report = expansion_ratio(&[], &RatioLimits::default());

        assert_eq!(report.total_compressed, 0);
        assert_eq!(report.total_uncompressed, 0);
        assert_eq!(report.overall_ratio, 0.0);
        assert!(report.flagged.is_empty());
        assert!(!report.dangerous);
    }
}
