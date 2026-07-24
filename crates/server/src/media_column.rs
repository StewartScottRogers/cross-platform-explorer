//! Audio-metadata column extractor (CPE-971, epic CPE-707): bridge a file's read audio tags to *typed*
//! metadata-column cells.
//!
//! [`crate::metadata_column`] (CPE-918) defines the typed [`CellValue`] and the uniform sort/format rules;
//! [`crate::media_meta_read`] (CPE-970) reads ID3 tags into [`MetaField`]s. This module joins them: pick a
//! tag by column and return it as the *right* [`CellValue`] type — so a **Track** or **Year** column sorts
//! numerically (9 before 10), not lexically. It is the per-family extractor the metadata-column epic was
//! missing; the image/video/doc extractors follow the same `→ CellValue` shape.
//!
//! Pure + std-only: a lookup + parse over already-read fields, no I/O.

use crate::media_meta_edit::MetaField;
use crate::metadata_column::CellValue;

/// The audio metadata columns a user can add to the details view — each maps to a friendly key
/// [`crate::media_meta_read::read_id3v2`] emits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioColumn {
    Title,
    Artist,
    Album,
    AlbumArtist,
    Track,
    Disc,
    Genre,
    Year,
    Composer,
    Publisher,
    Bpm,
    Comment,
}

impl AudioColumn {
    /// The friendly [`MetaField::key`] this column reads (as produced by the ID3 read codec).
    fn key(self) -> &'static str {
        match self {
            AudioColumn::Title => "Title",
            AudioColumn::Artist => "Artist",
            AudioColumn::Album => "Album",
            AudioColumn::AlbumArtist => "Album Artist",
            AudioColumn::Track => "Track",
            AudioColumn::Disc => "Disc",
            AudioColumn::Genre => "Genre",
            AudioColumn::Year => "Year",
            AudioColumn::Composer => "Composer",
            AudioColumn::Publisher => "Publisher",
            AudioColumn::Bpm => "BPM",
            AudioColumn::Comment => "Comment",
        }
    }

    /// Whether this column is a numeric quantity (sorts as an integer, not text).
    fn is_numeric(self) -> bool {
        matches!(self, AudioColumn::Track | AudioColumn::Disc | AudioColumn::Year | AudioColumn::Bpm)
    }
}

/// The [`CellValue`] for `col` from a file's read audio `fields`. Numeric columns become [`CellValue::Int`]
/// from the value's leading integer (so `"11/12"` → 11, `"1975-06-01"` → 1975), falling back to
/// [`CellValue::Text`] when there is no leading number; text columns become [`CellValue::Text`]; a missing
/// field is [`CellValue::Empty`] (which sorts last, per [`crate::metadata_column`]).
pub fn audio_cell(fields: &[MetaField], col: AudioColumn) -> CellValue {
    let Some(raw) = find(fields, col.key()) else { return CellValue::Empty };
    let raw = raw.trim();
    if raw.is_empty() {
        return CellValue::Empty;
    }
    if col.is_numeric() {
        match leading_int(raw) {
            Some(n) => CellValue::Int(n),
            None => CellValue::Text(raw.to_string()),
        }
    } else {
        CellValue::Text(raw.to_string())
    }
}

/// Case-insensitive lookup of a field's value by key (robust to a codec varying key casing).
fn find<'a>(fields: &'a [MetaField], key: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|f| f.key.eq_ignore_ascii_case(key))
        .map(|f| f.value.as_str())
}

/// The leading (optionally signed) integer of `s`, or `None` if it doesn't start with a digit. Stops at the
/// first non-digit, so `"11/12"` → 11, `"1975-06"` → 1975, `"3 of 10"` → 3.
fn leading_int(s: &str) -> Option<i64> {
    let bytes = s.as_bytes();
    let mut i = 0;
    let neg = bytes.first() == Some(&b'-');
    if neg {
        i = 1;
    }
    let start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == start {
        return None; // no digits
    }
    s[..i].parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media_meta_read::read_id3v2;
    use crate::metadata_column::compare;
    use std::cmp::Ordering;

    fn field(key: &str, value: &str) -> MetaField {
        MetaField { group: "id3".into(), key: key.into(), value: value.into(), editable: true }
    }

    #[test]
    fn text_columns_yield_text_and_missing_yields_empty() {
        let fields = vec![field("Artist", "Queen"), field("Album", "A Night at the Opera")];
        assert_eq!(audio_cell(&fields, AudioColumn::Artist), CellValue::Text("Queen".into()));
        assert_eq!(audio_cell(&fields, AudioColumn::Album), CellValue::Text("A Night at the Opera".into()));
        // No Title field → Empty.
        assert_eq!(audio_cell(&fields, AudioColumn::Title), CellValue::Empty);
        // Present but blank → Empty.
        assert_eq!(audio_cell(&[field("Genre", "   ")], AudioColumn::Genre), CellValue::Empty);
    }

    #[test]
    fn numeric_columns_parse_leading_integer() {
        let fields = vec![field("Track", "11/12"), field("Year", "1975-06-01"), field("BPM", "128")];
        assert_eq!(audio_cell(&fields, AudioColumn::Track), CellValue::Int(11));
        assert_eq!(audio_cell(&fields, AudioColumn::Year), CellValue::Int(1975));
        assert_eq!(audio_cell(&fields, AudioColumn::Bpm), CellValue::Int(128));
        // A numeric column with a non-numeric value falls back to Text rather than dropping it.
        assert_eq!(audio_cell(&[field("Track", "A-side")], AudioColumn::Track), CellValue::Text("A-side".into()));
    }

    #[test]
    fn leading_int_edge_cases() {
        assert_eq!(leading_int("11/12"), Some(11));
        assert_eq!(leading_int("1975"), Some(1975));
        assert_eq!(leading_int("3 of 10"), Some(3));
        assert_eq!(leading_int("-5dB"), Some(-5));
        assert_eq!(leading_int("side A"), None);
        assert_eq!(leading_int(""), None);
    }

    #[test]
    fn tracks_sort_numerically_not_lexically() {
        // The whole point: track 9 must sort before track 10, which text sorting gets wrong.
        let nine = audio_cell(&[field("Track", "9")], AudioColumn::Track);
        let ten = audio_cell(&[field("Track", "10")], AudioColumn::Track);
        assert_eq!(compare(&nine, &ten, true), Ordering::Less);
    }

    #[test]
    fn end_to_end_from_id3_bytes_to_typed_cells() {
        // Synthesise a minimal ID3v2.3 tag, read it, and extract typed columns — the 970 → 918 path.
        let tag = build_v23(&[("TIT2", "Under Pressure"), ("TPE1", "Queen"), ("TRCK", "2/11"), ("TYER", "1982")]);
        let fields = read_id3v2(&tag);
        assert_eq!(audio_cell(&fields, AudioColumn::Title), CellValue::Text("Under Pressure".into()));
        assert_eq!(audio_cell(&fields, AudioColumn::Artist), CellValue::Text("Queen".into()));
        assert_eq!(audio_cell(&fields, AudioColumn::Track), CellValue::Int(2));
        assert_eq!(audio_cell(&fields, AudioColumn::Year), CellValue::Int(1982));
        assert_eq!(audio_cell(&fields, AudioColumn::Album), CellValue::Empty); // not in the tag
    }

    /// Build a minimal ID3v2.3 tag from `(4-char id, latin1 text)` frames (plain frame sizes).
    fn build_v23(frames: &[(&str, &str)]) -> Vec<u8> {
        let mut body = Vec::new();
        for (id, text) in frames {
            let mut fb = vec![0u8]; // Latin-1 encoding byte
            fb.extend_from_slice(text.as_bytes());
            body.extend_from_slice(id.as_bytes());
            body.extend_from_slice(&(fb.len() as u32).to_be_bytes());
            body.extend_from_slice(&[0, 0]);
            body.extend_from_slice(&fb);
        }
        let mut syncsafe = [0u8; 4];
        let mut v = body.len() as u32;
        for i in (0..4).rev() {
            syncsafe[i] = (v & 0x7F) as u8;
            v >>= 7;
        }
        let mut tag = Vec::new();
        tag.extend_from_slice(b"ID3");
        tag.extend_from_slice(&[3, 0, 0]);
        tag.extend_from_slice(&syncsafe);
        tag.extend_from_slice(&body);
        tag
    }
}
