//! Search date/age filter (CPE-1060, epic CPE-703 — the instant-search query DSL).
//!
//! Pure parser + predicate for a `date:`/`modified:`-style query token: relative age windows
//! (`<7d`, `>1w`, `today`, `yesterday`) and absolute calendar spans (`2024`, `2024-07`,
//! `2024-07-25`). Everything is **UTC epoch seconds** — no timezone crate, no `SystemTime::now()`
//! inside [`parse`] (the caller injects `now_s`), so results are deterministic and identical on
//! every OS in the CI matrix. Standalone module: does not touch [`crate::index_query`].
//!
//! Relative units are **fixed integer approximations**, not calendar-accurate:
//! `d` = 1 day = 86 400s, `w` = 7d, `m` = 30d, `y` = 365d. A month or year expressed as a relative
//! age is therefore approximate (e.g. `<1y` means "within the last 365 days", not "since the same
//! calendar date last year"). Use an absolute token (`2024`, `2024-07`) for calendar-exact spans.

/// One second, for readability at call sites that build multiples of a day.
const SECS_PER_DAY: i64 = 86_400;
const SECS_PER_WEEK: i64 = SECS_PER_DAY * 7;
/// Fixed 30-day approximation of "a month" — documented, not calendar-accurate.
const SECS_PER_MONTH_APPROX: i64 = SECS_PER_DAY * 30;
/// Fixed 365-day approximation of "a year" — documented, not calendar-accurate (no leap adjustment).
const SECS_PER_YEAR_APPROX: i64 = SECS_PER_DAY * 365;

/// A parsed date/age filter, resolved to concrete UTC epoch-second bounds. [`matches`] tests a
/// modified-time (also UTC epoch seconds) against it.
///
/// Boundary rule (pinned so `Before`/`After` partition cleanly around one threshold, never
/// overlapping and never leaving a one-second gap): `After(t)` is **inclusive** (`mtime >= t`),
/// `Before(t)` is **exclusive** at the same threshold (`mtime < t`). `Between` is inclusive at
/// both ends (`lo <= mtime <= hi`) — used for whole-span absolute dates and `today`/`yesterday`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum DateFilter {
    /// `mtime_s < t` — strictly older than the threshold.
    Before(i64),
    /// `mtime_s >= t` — on or after the threshold (the recent side of a `<Nunit` window).
    After(i64),
    /// `lo <= mtime_s <= hi` — an inclusive whole span (a day/month/year, or `today`/`yesterday`).
    Between { lo: i64, hi: i64 },
}

/// Whether `mtime_s` (UTC epoch seconds) satisfies `f`. See [`DateFilter`] for the boundary rule.
pub fn matches(f: &DateFilter, mtime_s: i64) -> bool {
    match *f {
        DateFilter::Before(t) => mtime_s < t,
        DateFilter::After(t) => mtime_s >= t,
        DateFilter::Between { lo, hi } => mtime_s >= lo && mtime_s <= hi,
    }
}

/// Parse one date/age query token against `now_s` (UTC epoch seconds, caller-supplied so the
/// parser stays pure and deterministic). Recognises:
/// - `today`, `yesterday` (case-insensitive) — the current/prior UTC calendar day.
/// - `<N<unit>` — modified within the last N units (`After`); `>N<unit>` — older than N units
///   (`Before`). `unit` is one of `d`/`w`/`m`/`y` (case-insensitive), see the fixed
///   approximations documented on the module. `N` must be a non-negative integer.
/// - `YYYY`, `YYYY-MM`, `YYYY-MM-DD` — the whole year/month/day as an inclusive [`DateFilter::Between`]
///   span, computed with integer civil-date math (no timezone/date crate).
///
/// Malformed input (bad unit, negative count, invalid calendar date such as month 13 or day 32,
/// or unrecognised garbage) returns `None` — this function never panics.
pub fn parse(token: &str, now_s: i64) -> Option<DateFilter> {
    let t = token.trim();
    if t.is_empty() {
        return None;
    }
    let lower = t.to_ascii_lowercase();
    if lower == "today" {
        return Some(day_span(now_s.div_euclid(SECS_PER_DAY)));
    }
    if lower == "yesterday" {
        return Some(day_span(now_s.div_euclid(SECS_PER_DAY) - 1));
    }
    match t.as_bytes()[0] {
        b'<' | b'>' => parse_relative(t, now_s),
        _ => parse_absolute(t),
    }
}

/// Build the inclusive `[start-of-day, end-of-day]` span for civil day index `day` (days since
/// the Unix epoch, i.e. `epoch_seconds.div_euclid(SECS_PER_DAY)`).
fn day_span(day: i64) -> DateFilter {
    let lo = day * SECS_PER_DAY;
    DateFilter::Between { lo, hi: lo + SECS_PER_DAY - 1 }
}

/// Parse a `<N<unit>` / `>N<unit>` relative-age token, e.g. `<7d`, `>1w`, `<30d`, `>1m`, `<1y`.
fn parse_relative(t: &str, now_s: i64) -> Option<DateFilter> {
    let (op, rest) = t.split_at(1);
    let rest = rest.trim();
    if rest.is_empty() {
        return None;
    }
    let (num_part, unit_part) = rest.split_at(rest.len() - 1);
    if num_part.is_empty() || !num_part.bytes().all(|b| b.is_ascii_digit()) {
        return None; // reject empty/negative/non-numeric counts (`<d`, `<-7d`, `<7.5d`)
    }
    let n: i64 = num_part.parse().ok()?;
    let unit_secs = match unit_part.chars().next()?.to_ascii_lowercase() {
        'd' => SECS_PER_DAY,
        'w' => SECS_PER_WEEK,
        'm' => SECS_PER_MONTH_APPROX,
        'y' => SECS_PER_YEAR_APPROX,
        _ => return None,
    };
    let delta = n.checked_mul(unit_secs)?;
    let threshold = now_s.checked_sub(delta)?;
    match op {
        "<" => Some(DateFilter::After(threshold)),
        ">" => Some(DateFilter::Before(threshold)),
        _ => None,
    }
}

/// Parse an absolute `YYYY`, `YYYY-MM`, or `YYYY-MM-DD` token into its whole-span [`DateFilter::Between`].
fn parse_absolute(t: &str) -> Option<DateFilter> {
    let parts: Vec<&str> = t.split('-').collect();
    match parts.as_slice() {
        [y] => {
            let year = parse_digits(y)?;
            let lo = days_from_civil(year, 1, 1) * SECS_PER_DAY;
            let hi = days_from_civil(year + 1, 1, 1) * SECS_PER_DAY - 1;
            Some(DateFilter::Between { lo, hi })
        }
        [y, m] => {
            let year = parse_digits(y)?;
            let month = parse_bounded(m, 1, 12)?;
            let lo = days_from_civil(year, month, 1) * SECS_PER_DAY;
            let (ny, nm) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
            let hi = days_from_civil(ny, nm, 1) * SECS_PER_DAY - 1;
            Some(DateFilter::Between { lo, hi })
        }
        [y, m, d] => {
            let year = parse_digits(y)?;
            let month = parse_bounded(m, 1, 12)?;
            let day = parse_bounded(d, 1, 31)?;
            if day > days_in_month(year, month) {
                return None; // e.g. day 32, or Feb 30, or Apr 31
            }
            let lo = days_from_civil(year, month, day) * SECS_PER_DAY;
            Some(DateFilter::Between { lo, hi: lo + SECS_PER_DAY - 1 })
        }
        _ => None,
    }
}

/// Parse an all-ASCII-digit, non-empty integer (rejects signs, decimals, and empty strings).
fn parse_digits(s: &str) -> Option<i64> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

/// [`parse_digits`] with an inclusive `[min, max]` range check (used for month/day components).
fn parse_bounded(s: &str, min: i64, max: i64) -> Option<i64> {
    let v = parse_digits(s)?;
    if v < min || v > max {
        None
    } else {
        Some(v)
    }
}

fn is_leap_year(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// Days in civil month `m` (1-12) of year `y`, accounting for leap years. `m` must already be in
/// `1..=12` (callers validate via [`parse_bounded`]).
fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => if is_leap_year(y) { 29 } else { 28 },
        _ => 0,
    }
}

/// Civil date (year, month 1-12, day 1-31) → days since the Unix epoch (1970-01-01 = day 0).
/// Howard Hinnant's `days_from_civil` algorithm: integer-only, proleptic Gregorian, valid for any
/// `i64` year (positive or negative), no timezone involved. Does not validate the date — callers
/// validate month/day range and days-in-month first ([`parse_bounded`], [`days_in_month`]).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let mp = if m > 2 { m - 3 } else { m + 9 }; // [0, 11], Mar=0 .. Feb=11
    let doy = (153 * mp + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- days_from_civil: known reference points ----

    #[test]
    fn days_from_civil_matches_known_epoch_points() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(1970, 1, 2), 1);
        assert_eq!(days_from_civil(1969, 12, 31), -1);
        // 2024 is a leap year: Jan 1 2024 -> Jan 1 2025 is 366 days.
        assert_eq!(days_from_civil(2025, 1, 1) - days_from_civil(2024, 1, 1), 366);
        // 2023 is not a leap year: Jan 1 2023 -> Jan 1 2024 is 365 days.
        assert_eq!(days_from_civil(2024, 1, 1) - days_from_civil(2023, 1, 1), 365);
        // Feb 2000 (leap, divisible by 400) has 29 days: Mar 1 - Feb 1 = 29.
        assert_eq!(days_from_civil(2000, 3, 1) - days_from_civil(2000, 2, 1), 29);
        // Feb 1900 (NOT leap, divisible by 100 but not 400) has 28 days.
        assert_eq!(days_from_civil(1900, 3, 1) - days_from_civil(1900, 2, 1), 28);
    }

    // ---- relative windows, fixed injected `now` ----

    #[test]
    fn relative_recent_window_matches_within_and_rejects_beyond() {
        let now_s: i64 = 2_000_000_000;
        let f = parse("<7d", now_s).unwrap();
        assert_eq!(f, DateFilter::After(now_s - 7 * SECS_PER_DAY));
        assert!(matches(&f, now_s)); // now itself
        assert!(matches(&f, now_s - 3 * SECS_PER_DAY)); // within the window
        assert!(matches(&f, now_s - 7 * SECS_PER_DAY)); // boundary: inclusive
        assert!(!matches(&f, now_s - 7 * SECS_PER_DAY - 1)); // one second past the boundary
        assert!(!matches(&f, now_s - 10 * SECS_PER_DAY)); // well outside
    }

    #[test]
    fn relative_older_than_window_matches_beyond_and_rejects_within() {
        let now_s: i64 = 2_000_000_000;
        let f = parse(">7d", now_s).unwrap();
        assert_eq!(f, DateFilter::Before(now_s - 7 * SECS_PER_DAY));
        assert!(matches(&f, now_s - 10 * SECS_PER_DAY)); // older than 7d
        assert!(!matches(&f, now_s - 3 * SECS_PER_DAY)); // too recent
        assert!(!matches(&f, now_s - 7 * SECS_PER_DAY)); // boundary: NOT older (After/Before partition, no overlap)
        assert!(matches(&f, now_s - 7 * SECS_PER_DAY - 1)); // one second past the boundary
    }

    #[test]
    fn relative_units_week_month_year_use_documented_fixed_approximations() {
        let now_s: i64 = 2_000_000_000;
        assert_eq!(parse("<1w", now_s), Some(DateFilter::After(now_s - 7 * SECS_PER_DAY)));
        assert_eq!(parse("<1m", now_s), Some(DateFilter::After(now_s - 30 * SECS_PER_DAY)));
        assert_eq!(parse("<1y", now_s), Some(DateFilter::After(now_s - 365 * SECS_PER_DAY)));
        assert_eq!(parse("<2w", now_s), Some(DateFilter::After(now_s - 14 * SECS_PER_DAY)));
    }

    #[test]
    fn relative_parsing_is_case_insensitive_and_tolerates_whitespace() {
        let now_s: i64 = 2_000_000_000;
        assert_eq!(parse("<7d", now_s), parse("<7D", now_s));
        assert_eq!(parse("<7d", now_s), parse(" <7d ", now_s));
        assert_eq!(parse(">1Y", now_s), parse(">1y", now_s));
    }

    // ---- today / yesterday via integer day math ----

    #[test]
    fn today_and_yesterday_boundaries() {
        // Anchor "now" at noon UTC on a known civil date, built from days_from_civil itself so the
        // test doesn't depend on a hand-computed epoch literal.
        let day = days_from_civil(2024, 7, 25);
        let now_s = day * SECS_PER_DAY + 12 * 3600; // 2024-07-25 12:00:00 UTC

        let today = parse("today", now_s).unwrap();
        let expected_today =
            DateFilter::Between { lo: day * SECS_PER_DAY, hi: day * SECS_PER_DAY + SECS_PER_DAY - 1 };
        assert_eq!(today, expected_today);
        assert!(matches(&today, day * SECS_PER_DAY)); // start of day: inclusive
        assert!(matches(&today, day * SECS_PER_DAY + SECS_PER_DAY - 1)); // last second: inclusive
        assert!(!matches(&today, day * SECS_PER_DAY - 1)); // one second before midnight: not today
        assert!(!matches(&today, day * SECS_PER_DAY + SECS_PER_DAY)); // one second into tomorrow

        let yesterday = parse("yesterday", now_s).unwrap();
        let expected_yesterday = DateFilter::Between {
            lo: (day - 1) * SECS_PER_DAY,
            hi: day * SECS_PER_DAY - 1,
        };
        assert_eq!(yesterday, expected_yesterday);
        assert!(matches(&yesterday, day * SECS_PER_DAY - 1)); // last second of yesterday
        assert!(!matches(&yesterday, day * SECS_PER_DAY)); // start of today: not yesterday

        // Case-insensitive.
        assert_eq!(parse("Today", now_s), Some(today));
        assert_eq!(parse("YESTERDAY", now_s), Some(yesterday));
    }

    // ---- absolute year / month / day spans ----

    #[test]
    fn absolute_year_span_is_start_inclusive_end_inclusive() {
        let f = parse("2024", 0).unwrap();
        let jan1_2024 = days_from_civil(2024, 1, 1) * SECS_PER_DAY;
        let jan1_2025 = days_from_civil(2025, 1, 1) * SECS_PER_DAY;
        assert_eq!(f, DateFilter::Between { lo: jan1_2024, hi: jan1_2025 - 1 });
        assert!(matches(&f, jan1_2024)); // first second of the year
        assert!(matches(&f, jan1_2025 - 1)); // last second of the year
        assert!(!matches(&f, jan1_2024 - 1)); // last second of the prior year
        assert!(!matches(&f, jan1_2025)); // first second of the next year
    }

    #[test]
    fn absolute_month_span_is_start_inclusive_end_inclusive() {
        let f = parse("2024-07", 0).unwrap();
        let jul1 = days_from_civil(2024, 7, 1) * SECS_PER_DAY;
        let aug1 = days_from_civil(2024, 8, 1) * SECS_PER_DAY;
        assert_eq!(f, DateFilter::Between { lo: jul1, hi: aug1 - 1 });
        assert!(matches(&f, jul1));
        assert!(matches(&f, aug1 - 1));
        assert!(!matches(&f, aug1));

        // December rolls the span into next year correctly.
        let dec = parse("2024-12", 0).unwrap();
        let dec1 = days_from_civil(2024, 12, 1) * SECS_PER_DAY;
        let jan1_next = days_from_civil(2025, 1, 1) * SECS_PER_DAY;
        assert_eq!(dec, DateFilter::Between { lo: dec1, hi: jan1_next - 1 });
    }

    #[test]
    fn absolute_day_span_is_a_single_inclusive_day() {
        let f = parse("2024-07-25", 0).unwrap();
        let day0 = days_from_civil(2024, 7, 25) * SECS_PER_DAY;
        assert_eq!(f, DateFilter::Between { lo: day0, hi: day0 + SECS_PER_DAY - 1 });
        assert!(matches(&f, day0));
        assert!(matches(&f, day0 + SECS_PER_DAY - 1));
        assert!(!matches(&f, day0 - 1));
        assert!(!matches(&f, day0 + SECS_PER_DAY));
    }

    // ---- malformed input and garbage: None, never panic ----

    #[test]
    fn malformed_calendar_dates_are_rejected() {
        assert_eq!(parse("2024-13", 0), None); // month 13
        assert_eq!(parse("2024-13-01", 0), None); // month 13
        assert_eq!(parse("2024-00", 0), None); // month 0
        assert_eq!(parse("2024-01-32", 0), None); // day 32 (out of any month's range)
        assert_eq!(parse("2024-01-00", 0), None); // day 0
        assert_eq!(parse("2024-04-31", 0), None); // April has only 30 days
        assert_eq!(parse("2023-02-29", 0), None); // 2023 is not a leap year
        assert!(parse("2024-02-29", 0).is_some()); // 2024 IS a leap year: Feb 29 is valid
    }

    #[test]
    fn garbage_and_malformed_tokens_never_panic_and_yield_none() {
        assert_eq!(parse("", 0), None);
        assert_eq!(parse("   ", 0), None);
        assert_eq!(parse("garbage", 0), None);
        assert_eq!(parse("<", 0), None);
        assert_eq!(parse(">", 0), None);
        assert_eq!(parse("<d", 0), None); // missing count
        assert_eq!(parse("<7x", 0), None); // invalid unit
        assert_eq!(parse("<-7d", 0), None); // negative count
        assert_eq!(parse("<7.5d", 0), None); // non-integer count
        assert_eq!(parse("2024-07-25-01", 0), None); // extra segment
        assert_eq!(parse("abcd", 0), None);
        assert_eq!(parse("2024-ab", 0), None); // non-numeric month
        assert_eq!(parse("-", 0), None);
    }
}
