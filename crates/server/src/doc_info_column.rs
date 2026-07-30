//! Document-metadata column extractor (CPE-1039, epic CPE-707): bridge a PDF's read `/Info` fields to
//! *typed* metadata-column cells.
//!
//! [`crate::metadata_column`] (CPE-918) defines the typed [`CellValue`] and the uniform sort/format rules;
//! [`crate::media_meta_read::read_pdf`] (CPE-1036) reads a PDF's document-info dictionary into
//! [`MetaField`]s. This module joins them: pick a field by column and return it as a [`CellValue`] — mirrors
//! [`crate::media_column::audio_cell`] exactly, but every doc-info field is text (Title/Author/…, plus the
//! two date strings), so there is no numeric branch here.
//!
//! Pure + std-only: a lookup over already-read fields, no I/O.

use serde::{Deserialize, Serialize};

use crate::media_meta_edit::MetaField;
use crate::metadata_column::CellValue;

/// The document-info columns a user can add to the details view — each maps to a friendly key
/// [`crate::media_meta_read::read_pdf`] emits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum DocInfoColumn {
    Title,
    Author,
    Subject,
    Keywords,
    Creator,
    Producer,
    DateCreated,
    DateModified,
}

impl DocInfoColumn {
    /// Every document-info column, in a stable display order — what the column picker enumerates
    /// (CPE-1145).
    pub(crate) const ALL: [DocInfoColumn; 8] = [
        DocInfoColumn::Title,
        DocInfoColumn::Author,
        DocInfoColumn::Subject,
        DocInfoColumn::Keywords,
        DocInfoColumn::Creator,
        DocInfoColumn::Producer,
        DocInfoColumn::DateCreated,
        DocInfoColumn::DateModified,
    ];

    /// The friendly [`MetaField::key`] this column reads (as produced by the PDF `/Info` read codec).
    fn key(self) -> &'static str {
        match self {
            DocInfoColumn::Title => "Title",
            DocInfoColumn::Author => "Author",
            DocInfoColumn::Subject => "Subject",
            DocInfoColumn::Keywords => "Keywords",
            DocInfoColumn::Creator => "Creator",
            DocInfoColumn::Producer => "Producer",
            DocInfoColumn::DateCreated => "Date Created",
            DocInfoColumn::DateModified => "Date Modified",
        }
    }

    /// Stable snake_case token for this column, used to build [`crate::column_extract::MetaColumn::id`]
    /// (persistence in `column_config`, CPE-1145).
    pub(crate) fn id_token(self) -> &'static str {
        match self {
            DocInfoColumn::Title => "title",
            DocInfoColumn::Author => "author",
            DocInfoColumn::Subject => "subject",
            DocInfoColumn::Keywords => "keywords",
            DocInfoColumn::Creator => "creator",
            DocInfoColumn::Producer => "producer",
            DocInfoColumn::DateCreated => "date_created",
            DocInfoColumn::DateModified => "date_modified",
        }
    }

    /// The friendly display label for the column picker — reuses [`Self::key`], which is already a
    /// human-readable name.
    pub(crate) fn label(self) -> &'static str {
        self.key()
    }
}

/// The [`CellValue`] for `col` from a file's read document-info `fields`. Every doc-info field is text, so
/// this is always [`CellValue::Text`] (trimmed) or [`CellValue::Empty`] when the field is absent or blank
/// (which sorts last, per [`crate::metadata_column`]).
pub fn doc_info_cell(fields: &[MetaField], col: DocInfoColumn) -> CellValue {
    let Some(raw) = find(fields, col.key()) else { return CellValue::Empty };
    let raw = raw.trim();
    if raw.is_empty() {
        return CellValue::Empty;
    }
    CellValue::Text(raw.to_string())
}

/// Case-insensitive lookup of a field's value by key (robust to a codec varying key casing).
fn find<'a>(fields: &'a [MetaField], key: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|f| f.key.eq_ignore_ascii_case(key))
        .map(|f| f.value.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(key: &str, value: &str) -> MetaField {
        MetaField { group: "pdf".into(), key: key.into(), value: value.into(), editable: false }
    }

    #[test]
    fn present_field_yields_text() {
        let fields = vec![field("Title", "Vacation Photos"), field("Author", "Jane Doe")];
        assert_eq!(doc_info_cell(&fields, DocInfoColumn::Title), CellValue::Text("Vacation Photos".into()));
        assert_eq!(doc_info_cell(&fields, DocInfoColumn::Author), CellValue::Text("Jane Doe".into()));
    }

    #[test]
    fn absent_field_yields_empty() {
        let fields = vec![field("Title", "Vacation Photos")];
        // No Author field at all.
        assert_eq!(doc_info_cell(&fields, DocInfoColumn::Author), CellValue::Empty);
        assert_eq!(doc_info_cell(&fields, DocInfoColumn::Subject), CellValue::Empty);
    }

    #[test]
    fn blank_field_yields_empty() {
        let fields = vec![field("Keywords", "   ")];
        assert_eq!(doc_info_cell(&fields, DocInfoColumn::Keywords), CellValue::Empty);
    }

    #[test]
    fn date_columns_yield_text_verbatim() {
        let fields = vec![field("Date Created", "D:20240101120000"), field("Date Modified", "D:20240102000000")];
        assert_eq!(
            doc_info_cell(&fields, DocInfoColumn::DateCreated),
            CellValue::Text("D:20240101120000".into())
        );
        assert_eq!(
            doc_info_cell(&fields, DocInfoColumn::DateModified),
            CellValue::Text("D:20240102000000".into())
        );
    }

    #[test]
    fn trims_surrounding_whitespace() {
        let fields = vec![field("Producer", "  Acme PDF Writer  ")];
        assert_eq!(doc_info_cell(&fields, DocInfoColumn::Producer), CellValue::Text("Acme PDF Writer".into()));
    }
}
