//! Video-tag column extractor (CPE-1040, epic CPE-707): bridge an MP4/MOV's read iTunes-style tags to
//! *typed* metadata-column cells.
//!
//! [`crate::metadata_column`] (CPE-918) defines the typed [`CellValue`] and the uniform sort/format rules;
//! [`crate::video_meta_read::read_mp4`] (CPE-1037) reads a video's `moov/udta/meta/ilst` tags into
//! [`MetaField`]s. This module joins them: pick a field by column and return it as a [`CellValue`] — mirrors
//! [`crate::doc_info_column::doc_info_cell`], plus [`crate::media_column::audio_cell`]'s numeric-Year
//! handling, since a video's Year should sort numerically like an audio track's.
//!
//! Pure + std-only: a lookup + parse over already-read fields, no I/O.

use serde::{Deserialize, Serialize};

use crate::media_meta_edit::MetaField;
use crate::metadata_column::CellValue;

/// The video-tag columns a user can add to the details view — each maps to a friendly key
/// [`crate::video_meta_read::read_mp4`] emits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum VideoTagColumn {
    Title,
    Artist,
    Album,
    Year,
    Comment,
    Genre,
    Composer,
    Encoder,
    Copyright,
}

impl VideoTagColumn {
    /// Every video-tag column, in a stable display order — what the column picker enumerates
    /// (CPE-1145).
    pub(crate) const ALL: [VideoTagColumn; 9] = [
        VideoTagColumn::Title,
        VideoTagColumn::Artist,
        VideoTagColumn::Album,
        VideoTagColumn::Year,
        VideoTagColumn::Comment,
        VideoTagColumn::Genre,
        VideoTagColumn::Composer,
        VideoTagColumn::Encoder,
        VideoTagColumn::Copyright,
    ];

    /// The friendly [`MetaField::key`] this column reads (as produced by the MP4/MOV tag read codec).
    fn key(self) -> &'static str {
        match self {
            VideoTagColumn::Title => "Title",
            VideoTagColumn::Artist => "Artist",
            VideoTagColumn::Album => "Album",
            VideoTagColumn::Year => "Year",
            VideoTagColumn::Comment => "Comment",
            VideoTagColumn::Genre => "Genre",
            VideoTagColumn::Composer => "Composer",
            VideoTagColumn::Encoder => "Encoder",
            VideoTagColumn::Copyright => "Copyright",
        }
    }

    /// Whether this column is a numeric quantity (sorts as an integer, not text).
    fn is_numeric(self) -> bool {
        matches!(self, VideoTagColumn::Year)
    }

    /// Stable snake_case token for this column, used to build [`crate::column_extract::MetaColumn::id`]
    /// (persistence in `column_config`, CPE-1145).
    pub(crate) fn id_token(self) -> &'static str {
        match self {
            VideoTagColumn::Title => "title",
            VideoTagColumn::Artist => "artist",
            VideoTagColumn::Album => "album",
            VideoTagColumn::Year => "year",
            VideoTagColumn::Comment => "comment",
            VideoTagColumn::Genre => "genre",
            VideoTagColumn::Composer => "composer",
            VideoTagColumn::Encoder => "encoder",
            VideoTagColumn::Copyright => "copyright",
        }
    }

    /// The friendly display label for the column picker — reuses [`Self::key`], which is already a
    /// human-readable name.
    pub(crate) fn label(self) -> &'static str {
        self.key()
    }
}

/// The [`CellValue`] for `col` from a file's read video-tag `fields`. The numeric Year column becomes
/// [`CellValue::Int`] from the value's leading integer (so `"2001-05"` → 2001), falling back to
/// [`CellValue::Text`] when there is no leading number; every other column is [`CellValue::Text`]; a
/// missing/blank field is [`CellValue::Empty`] (which sorts last, per [`crate::metadata_column`]).
pub fn video_tag_cell(fields: &[MetaField], col: VideoTagColumn) -> CellValue {
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
/// first non-digit, so `"2001-05"` → 2001, `"3 of 10"` → 3. Mirrors [`crate::media_column`]'s helper of the
/// same name.
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

    fn field(key: &str, value: &str) -> MetaField {
        MetaField { group: "video".into(), key: key.into(), value: value.into(), editable: false }
    }

    #[test]
    fn present_field_yields_text() {
        let fields = vec![field("Title", "Big Buck Bunny"), field("Artist", "Blender Foundation")];
        assert_eq!(video_tag_cell(&fields, VideoTagColumn::Title), CellValue::Text("Big Buck Bunny".into()));
        assert_eq!(
            video_tag_cell(&fields, VideoTagColumn::Artist),
            CellValue::Text("Blender Foundation".into())
        );
    }

    #[test]
    fn absent_field_yields_empty() {
        let fields = vec![field("Title", "Big Buck Bunny")];
        // No Artist field at all.
        assert_eq!(video_tag_cell(&fields, VideoTagColumn::Artist), CellValue::Empty);
        assert_eq!(video_tag_cell(&fields, VideoTagColumn::Album), CellValue::Empty);
    }

    #[test]
    fn blank_field_yields_empty() {
        let fields = vec![field("Genre", "   ")];
        assert_eq!(video_tag_cell(&fields, VideoTagColumn::Genre), CellValue::Empty);
    }

    #[test]
    fn year_parses_leading_integer() {
        let fields = vec![field("Year", "2001")];
        assert_eq!(video_tag_cell(&fields, VideoTagColumn::Year), CellValue::Int(2001));

        let date_ish = vec![field("Year", "2001-05")];
        assert_eq!(video_tag_cell(&date_ish, VideoTagColumn::Year), CellValue::Int(2001));
    }

    #[test]
    fn year_with_no_leading_digit_falls_back_to_text() {
        let fields = vec![field("Year", "unknown")];
        assert_eq!(video_tag_cell(&fields, VideoTagColumn::Year), CellValue::Text("unknown".into()));
    }

    #[test]
    fn non_numeric_columns_never_become_int() {
        let fields = vec![field("Composer", "42"), field("Encoder", "HandBrake"), field("Copyright", "(c) 2024")];
        assert_eq!(video_tag_cell(&fields, VideoTagColumn::Composer), CellValue::Text("42".into()));
        assert_eq!(video_tag_cell(&fields, VideoTagColumn::Encoder), CellValue::Text("HandBrake".into()));
        assert_eq!(video_tag_cell(&fields, VideoTagColumn::Copyright), CellValue::Text("(c) 2024".into()));
    }

    #[test]
    fn trims_surrounding_whitespace() {
        let fields = vec![field("Comment", "  A comment  ")];
        assert_eq!(video_tag_cell(&fields, VideoTagColumn::Comment), CellValue::Text("A comment".into()));
    }
}
