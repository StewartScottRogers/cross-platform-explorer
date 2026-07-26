//! Search size filter — parse a human size expression (`>1mb`, `<=500k`, `1mb..1gb`, `2.5g`) into a
//! [`SizeFilter`] and test a candidate's byte count against it (CPE-1059, epic CPE-703).
//!
//! **Pure, dependency-free, std-only.** Operates only on `u64` byte counts and ASCII strings — no
//! `std::path`, no platform semantics, no `#[cfg]`-dependent behavior — so there is nothing for the 3-OS
//! CI matrix to disagree about. Deliberately standalone: this does **not** touch [`crate::index_query`];
//! wiring a `size:` token into that grammar is a separate, later ticket.
//!
//! ## Units are 1024-based
//! `k`/`kb` = 1024, `m`/`mb` = 1024², `g`/`gb` = 1024³, `t`/`tb` = 1024⁴ (binary/IEC sizes, matching how
//! this app reports file sizes elsewhere). A bare number (no unit suffix, e.g. `500`) means bytes.
//! Suffixes are matched case-insensitively (`G`, `Gb`, `GB`, `gb` all work).
//!
//! ## Decimal mantissas
//! A mantissa may carry a decimal point (`2.5g`, `500.5k`). It is parsed as `f64`, multiplied by the
//! unit's byte count, then **rounded to the nearest integer, ties away from zero** (`f64::round`) and
//! cast to `u64`. Most everyday values (`2.5g`, `500k`) land on an exact integer with no rounding in
//! play; see `decimal_mantissa_rounding_is_pinned` for a case that actually rounds (`1.1k` → 1126, not
//! 1126.4 truncated to 1126 — round-half-away-from-zero and truncation agree here, but the test pins the
//! exact byte count either way so a future change can't silently drift).

/// A size comparison operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum SizeOp {
    Gt,
    Lt,
    Ge,
    Le,
    Eq,
}

/// A parsed size filter: either a single comparison, or an inclusive range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum SizeFilter {
    Cmp { op: SizeOp, bytes: u64 },
    /// `lo..hi`, inclusive on **both** ends.
    Range { lo: u64, hi: u64 },
}

const KB: u64 = 1024;
const MB: u64 = KB * 1024;
const GB: u64 = MB * 1024;
const TB: u64 = GB * 1024;

/// Parse a token like `>1mb`, `<=500k`, `=0`, `1mb..1gb`, `2.5g` into a [`SizeFilter`]. Garbage, empty
/// input, or a range with `hi < lo` all yield `None` — never a panic.
pub fn parse(token: &str) -> Option<SizeFilter> {
    let token = token.trim();
    if token.is_empty() {
        return None;
    }

    // Range form: `lo..hi`, both sides plain amounts (no comparison operator).
    if let Some(idx) = token.find("..") {
        let lo_s = token[..idx].trim();
        let hi_s = token[idx + 2..].trim();
        let lo = parse_amount(lo_s)?;
        let hi = parse_amount(hi_s)?;
        if hi < lo {
            return None;
        }
        return Some(SizeFilter::Range { lo, hi });
    }

    // Comparison form: an optional leading operator (longest match first so `>=`/`<=` aren't cut short
    // by the single-char `>`/`<` cases), defaulting to `Eq` for a bare amount.
    let (op, rest) = if let Some(r) = token.strip_prefix(">=") {
        (SizeOp::Ge, r)
    } else if let Some(r) = token.strip_prefix("<=") {
        (SizeOp::Le, r)
    } else if let Some(r) = token.strip_prefix('>') {
        (SizeOp::Gt, r)
    } else if let Some(r) = token.strip_prefix('<') {
        (SizeOp::Lt, r)
    } else if let Some(r) = token.strip_prefix('=') {
        (SizeOp::Eq, r)
    } else {
        (SizeOp::Eq, token)
    };

    let bytes = parse_amount(rest.trim())?;
    Some(SizeFilter::Cmp { op, bytes })
}

/// Whether `bytes` satisfies `f`.
pub fn matches(f: &SizeFilter, bytes: u64) -> bool {
    match f {
        SizeFilter::Cmp { op, bytes: b } => match op {
            SizeOp::Gt => bytes > *b,
            SizeOp::Lt => bytes < *b,
            SizeOp::Ge => bytes >= *b,
            SizeOp::Le => bytes <= *b,
            SizeOp::Eq => bytes == *b,
        },
        SizeFilter::Range { lo, hi } => (*lo..=*hi).contains(&bytes),
    }
}

/// Parse a single amount — an optional decimal mantissa (ASCII digits + at most one `.`) followed by an
/// optional unit suffix — into a byte count. `None` for anything that isn't a clean amount: empty input,
/// a non-ASCII-digit/`.` mantissa, more than one `.`, or an unrecognised suffix.
fn parse_amount(s: &str) -> Option<u64> {
    if s.is_empty() {
        return None;
    }
    // Suffixes checked longest-first so "kb" isn't cut short at "k" leaving a dangling "b".
    const UNITS: &[(&str, u64)] = &[
        ("tb", TB),
        ("gb", GB),
        ("mb", MB),
        ("kb", KB),
        ("t", TB),
        ("g", GB),
        ("m", MB),
        ("k", KB),
        ("b", 1),
    ];
    let lower = s.to_ascii_lowercase();
    let (mantissa_s, unit) = UNITS
        .iter()
        .find_map(|&(suffix, mult)| {
            lower
                .strip_suffix(suffix)
                .map(|mantissa| (&s[..mantissa.len()], mult))
        })
        .unwrap_or((s, 1));

    let mantissa = parse_mantissa(mantissa_s)?;
    if mantissa < 0.0 {
        return None;
    }
    let bytes = (mantissa * unit as f64).round();
    if !bytes.is_finite() || bytes < 0.0 {
        return None;
    }
    Some(bytes.min(u64::MAX as f64) as u64)
}

/// Parse an ASCII decimal mantissa: digits with at most one `.`, at least one digit total. Rejects
/// anything else (letters, signs, multiple dots, empty) so upstream garbage never reaches `f64::parse`.
fn parse_mantissa(s: &str) -> Option<f64> {
    if s.is_empty() {
        return None;
    }
    if !s.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
        return None;
    }
    if s.bytes().filter(|&b| b == b'.').count() > 1 {
        return None;
    }
    if !s.bytes().any(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse::<f64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- unit math (1024-based) ----

    #[test]
    fn bare_number_is_bytes() {
        assert_eq!(parse("500"), Some(SizeFilter::Cmp { op: SizeOp::Eq, bytes: 500 }));
    }

    #[test]
    fn kb_is_1024() {
        assert_eq!(parse("1kb"), Some(SizeFilter::Cmp { op: SizeOp::Eq, bytes: 1024 }));
        assert_eq!(parse("1k"), Some(SizeFilter::Cmp { op: SizeOp::Eq, bytes: 1024 }));
    }

    #[test]
    fn mb_gb_tb_are_1024_based() {
        assert_eq!(parse("1mb"), Some(SizeFilter::Cmp { op: SizeOp::Eq, bytes: 1024 * 1024 }));
        assert_eq!(parse("1gb"), Some(SizeFilter::Cmp { op: SizeOp::Eq, bytes: 1024 * 1024 * 1024 }));
        assert_eq!(
            parse("1tb"),
            Some(SizeFilter::Cmp { op: SizeOp::Eq, bytes: 1024u64 * 1024 * 1024 * 1024 })
        );
    }

    #[test]
    fn units_are_case_insensitive() {
        assert_eq!(parse("2G"), parse("2g"));
        assert_eq!(parse("2Gb"), parse("2gb"));
        assert_eq!(parse("2GB"), parse("2gb"));
    }

    #[test]
    fn decimal_mantissa_parses_to_the_right_byte_count() {
        // 2.5g = 2.5 * 1024^3, exact in f64 (power-of-two base).
        assert_eq!(
            parse("2.5g"),
            Some(SizeFilter::Cmp { op: SizeOp::Eq, bytes: 2_684_354_560 })
        );
        // 500.5k = 500.5 * 1024, also exact.
        assert_eq!(parse("500.5k"), Some(SizeFilter::Cmp { op: SizeOp::Eq, bytes: 512_512 }));
    }

    #[test]
    fn decimal_mantissa_rounding_is_pinned() {
        // 1.1k = 1.1 * 1024 = 1126.4 -> rounds to 1126 (round-half-away-from-zero via f64::round;
        // truncation would also give 1126 here, but this test pins the exact number regardless of
        // which rule a future refactor might reach for).
        assert_eq!(parse("1.1k"), Some(SizeFilter::Cmp { op: SizeOp::Eq, bytes: 1126 }));
        // 1.5b (bytes, sub-unit) rounds 1.5 up to 2 (ties away from zero).
        assert_eq!(parse("1.5b"), Some(SizeFilter::Cmp { op: SizeOp::Eq, bytes: 2 }));
        // 1.4b rounds down to 1.
        assert_eq!(parse("1.4b"), Some(SizeFilter::Cmp { op: SizeOp::Eq, bytes: 1 }));
    }

    // ---- operator boundary inclusivity ----

    #[test]
    fn gt_excludes_the_endpoint() {
        let f = parse(">1k").unwrap();
        assert!(!matches(&f, 1024));
        assert!(matches(&f, 1025));
    }

    #[test]
    fn lt_excludes_the_endpoint() {
        let f = parse("<1k").unwrap();
        assert!(!matches(&f, 1024));
        assert!(matches(&f, 1023));
    }

    #[test]
    fn ge_includes_the_endpoint() {
        let f = parse(">=1k").unwrap();
        assert!(matches(&f, 1024));
        assert!(!matches(&f, 1023));
    }

    #[test]
    fn le_includes_the_endpoint() {
        let f = parse("<=1k").unwrap();
        assert!(matches(&f, 1024));
        assert!(!matches(&f, 1025));
    }

    #[test]
    fn eq_matches_only_the_exact_value() {
        let f = parse("=1k").unwrap();
        assert!(matches(&f, 1024));
        assert!(!matches(&f, 1023));
        assert!(!matches(&f, 1025));
    }

    #[test]
    fn bare_amount_defaults_to_eq() {
        assert_eq!(parse("1k"), parse("=1k"));
    }

    // ---- range ----

    #[test]
    fn range_is_inclusive_on_both_ends() {
        let f = parse("1mb..1gb").unwrap();
        let mb = 1024 * 1024;
        let gb = 1024 * 1024 * 1024;
        assert!(matches(&f, mb), "lower bound included");
        assert!(matches(&f, gb), "upper bound included");
        assert!(matches(&f, mb + 1));
        assert!(!matches(&f, mb - 1));
        assert!(!matches(&f, gb + 1));
    }

    #[test]
    fn range_rejects_hi_less_than_lo() {
        assert_eq!(parse("1gb..1mb"), None);
    }

    #[test]
    fn range_allows_equal_lo_hi() {
        let f = parse("500..500").unwrap();
        assert!(matches(&f, 500));
        assert!(!matches(&f, 499));
        assert!(!matches(&f, 501));
    }

    // ---- garbage / empty -> None, no panic ----

    #[test]
    fn empty_and_whitespace_are_none() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("   "), None);
    }

    #[test]
    fn garbage_is_none() {
        assert_eq!(parse("abc"), None);
        assert_eq!(parse(">abc"), None);
        assert_eq!(parse("1xb"), None);
        assert_eq!(parse("-5"), None);
        assert_eq!(parse("5-"), None);
        assert_eq!(parse("1.2.3"), None);
        assert_eq!(parse(".."), None);
        assert_eq!(parse("..5"), None);
        assert_eq!(parse("5.."), None);
    }

    #[test]
    fn operator_with_no_amount_is_none() {
        assert_eq!(parse(">"), None);
        assert_eq!(parse(">="), None);
        assert_eq!(parse("="), None);
    }
}
