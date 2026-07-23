//! Metadata-column cell model (CPE-918, epic CPE-707): the typed value a per-folder metadata column
//! yields for a row, plus the *uniform* sort + format rules so a new column (image dimensions, audio
//! bitrate, page count, duration, …) sorts and renders exactly like a built-in. Pure + dependency-free:
//! the family extractors produce [`CellValue`]s, and the details view formats and sorts them through here.
//!
//! Two rules make columns behave predictably:
//! - **Empty sorts last**, in both directions — a row with no value for the column never jumps to the top
//!   just because you reversed the sort.
//! - **Type-aware ordering** — `Bytes`/`Int`/`Float` sort numerically (not lexically, so "9" < "10"),
//!   `Dimensions` by pixel area, `Text` case-insensitively.

use std::cmp::Ordering;

/// A typed cell value for a metadata column. `Empty` means the extractor produced no value for this row
/// (unreadable, or the wrong file kind — e.g. an audio column on an image).
#[derive(Debug, Clone, PartialEq)]
pub enum CellValue {
    /// Free text (artist, album, camera model). Sorted case-insensitively.
    Text(String),
    /// An integer quantity (page count, bitrate kbps, track number).
    Int(i64),
    /// A real quantity (duration seconds, aperture f-number).
    Float(f64),
    /// A byte size — formatted human-readably, sorted numerically.
    Bytes(u64),
    /// Pixel dimensions — formatted `w × h`, sorted by area then width.
    Dimensions { w: u32, h: u32 },
    /// No value for this row.
    Empty,
}

impl CellValue {
    /// Whether this cell has no value (sorts last, renders as the placeholder).
    pub fn is_empty(&self) -> bool {
        matches!(self, CellValue::Empty)
    }

    /// The display string for the details view. `Empty` renders as `placeholder` (typically `"—"`).
    pub fn display(&self, placeholder: &str) -> String {
        match self {
            CellValue::Text(s) => s.clone(),
            CellValue::Int(n) => n.to_string(),
            CellValue::Float(f) => format_float(*f),
            CellValue::Bytes(b) => format_bytes(*b),
            CellValue::Dimensions { w, h } => format!("{w} \u{00d7} {h}"),
            CellValue::Empty => placeholder.to_string(),
        }
    }

    /// The value's ordering key within a column, ignoring emptiness (callers handle `Empty` placement).
    fn cmp_value(&self, other: &CellValue) -> Ordering {
        use CellValue::*;
        match (self, other) {
            (Text(a), Text(b)) => a.to_lowercase().cmp(&b.to_lowercase()),
            (Int(a), Int(b)) => a.cmp(b),
            (Float(a), Float(b)) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
            (Bytes(a), Bytes(b)) => a.cmp(b),
            (Dimensions { w: aw, h: ah }, Dimensions { w: bw, h: bh }) => {
                (*aw as u64 * *ah as u64).cmp(&(*bw as u64 * *bh as u64)).then(aw.cmp(bw))
            }
            // Mixed variants shouldn't occur within one column; order by a stable variant rank so the
            // sort is still deterministic rather than panicking.
            _ => variant_rank(self).cmp(&variant_rank(other)),
        }
    }
}

fn variant_rank(v: &CellValue) -> u8 {
    match v {
        CellValue::Text(_) => 0,
        CellValue::Int(_) => 1,
        CellValue::Float(_) => 2,
        CellValue::Bytes(_) => 3,
        CellValue::Dimensions { .. } => 4,
        CellValue::Empty => 5,
    }
}

/// Compare two cells for a column sort. `Empty` always sorts **after** any value, regardless of
/// `ascending` (so blanks stay at the bottom whichever way you sort); non-empty values compare by type,
/// then the result is flipped for a descending sort.
pub fn compare(a: &CellValue, b: &CellValue, ascending: bool) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater, // a is empty → after b
        (false, true) => Ordering::Less,    // b is empty → a before
        (false, false) => {
            let ord = a.cmp_value(b);
            if ascending { ord } else { ord.reverse() }
        }
    }
}

/// Stable-sort `rows` by the [`CellValue`] `key` extracts from each, honouring the empty-last rule.
pub fn sort_rows<T>(rows: &mut [T], key: impl Fn(&T) -> CellValue, ascending: bool) {
    rows.sort_by(|x, y| compare(&key(x), &key(y), ascending));
}

/// Human-readable byte size (1024-based, one decimal above KB): `512 B`, `1.5 KB`, `2.0 MB`.
fn format_bytes(b: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    if b < 1024 {
        return format!("{b} B");
    }
    let mut size = b as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    format!("{size:.1} {}", UNITS[unit])
}

/// A float trimmed of trailing zeros (`3.0` → `3`, `1.50` → `1.5`).
fn format_float(f: f64) -> String {
    let s = format!("{f:.2}");
    let trimmed = s.trim_end_matches('0').trim_end_matches('.');
    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use CellValue::*;

    #[test]
    fn formats_each_variant() {
        assert_eq!(Text("hi".into()).display("—"), "hi");
        assert_eq!(Int(42).display("—"), "42");
        assert_eq!(Float(3.0).display("—"), "3");
        assert_eq!(Float(1.5).display("—"), "1.5");
        assert_eq!(Bytes(512).display("—"), "512 B");
        assert_eq!(Bytes(1536).display("—"), "1.5 KB");
        assert_eq!(Bytes(2 * 1024 * 1024).display("—"), "2.0 MB");
        assert_eq!(Dimensions { w: 1920, h: 1080 }.display("—"), "1920 \u{00d7} 1080");
        assert_eq!(Empty.display("—"), "—");
    }

    #[test]
    fn bytes_and_ints_sort_numerically_not_lexically() {
        // The classic lexical bug: "10" < "9". Numeric sort must not do that.
        let mut rows = vec![Int(10), Int(9), Int(100), Int(2)];
        sort_rows(&mut rows, |c| c.clone(), true);
        assert_eq!(rows, vec![Int(2), Int(9), Int(10), Int(100)]);
    }

    #[test]
    fn dimensions_sort_by_area_then_width() {
        let mut rows = vec![
            Dimensions { w: 100, h: 100 }, // area 10_000
            Dimensions { w: 200, h: 10 },  // area 2_000
            Dimensions { w: 50, h: 40 },   // area 2_000, narrower
        ];
        sort_rows(&mut rows, |c| c.clone(), true);
        assert_eq!(
            rows,
            vec![
                Dimensions { w: 50, h: 40 },
                Dimensions { w: 200, h: 10 },
                Dimensions { w: 100, h: 100 },
            ]
        );
    }

    #[test]
    fn empty_always_sorts_last_in_both_directions() {
        let asc = {
            let mut r = vec![Int(3), Empty, Int(1)];
            sort_rows(&mut r, |c| c.clone(), true);
            r
        };
        assert_eq!(asc, vec![Int(1), Int(3), Empty]);

        let desc = {
            let mut r = vec![Int(3), Empty, Int(1)];
            sort_rows(&mut r, |c| c.clone(), false);
            r
        };
        assert_eq!(desc, vec![Int(3), Int(1), Empty]); // values reversed, Empty STILL last
    }

    #[test]
    fn text_sorts_case_insensitively() {
        let mut rows = vec![Text("banana".into()), Text("Apple".into()), Text("cherry".into())];
        sort_rows(&mut rows, |c| c.clone(), true);
        assert_eq!(rows, vec![Text("Apple".into()), Text("banana".into()), Text("cherry".into())]);
    }
}
