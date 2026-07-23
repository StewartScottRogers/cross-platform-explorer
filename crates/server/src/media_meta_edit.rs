//! Editable media-metadata model (CPE-942, epic CPE-725): the pure edit-apply core behind the metadata
//! studio. A media file exposes a set of metadata [`MetaField`]s across groups (EXIF / IPTC / ID3); the
//! user issues [`MetaEdit`]s (set or clear); [`apply_edits`] applies them — updating or adding editable
//! fields, refusing read-only ones — and reports what changed. No file parsing/writing here: the codec
//! layer reads the fields in and writes the result back; this owns the edit *policy*.

/// One metadata field. `editable` gates whether the studio may change it (e.g. camera-set intrinsics like
/// image dimensions are read-only; a caption or artist tag is editable).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct MetaField {
    /// The metadata group/namespace, e.g. `"exif"`, `"iptc"`, `"id3"`.
    pub group: String,
    pub key: String,
    pub value: String,
    pub editable: bool,
}

/// An edit the user asked for.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "edit", rename_all = "snake_case")]
pub enum MetaEdit {
    /// Set (update or add) a field's value.
    Set { group: String, key: String, value: String },
    /// Remove a field.
    Clear { group: String, key: String },
}

/// The outcome of applying edits: the resulting fields plus human-readable applied/rejected notes.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, Default)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct EditResult {
    pub fields: Vec<MetaField>,
    pub applied: Vec<String>,
    pub rejected: Vec<String>,
}

impl MetaEdit {
    fn group(&self) -> &str {
        match self {
            MetaEdit::Set { group, .. } | MetaEdit::Clear { group, .. } => group,
        }
    }
    fn key(&self) -> &str {
        match self {
            MetaEdit::Set { key, .. } | MetaEdit::Clear { key, .. } => key,
        }
    }
}

/// Whether an edit is well-formed (non-empty group + key). A blank Set value is allowed (it blanks the
/// field); use `Clear` to remove it entirely.
pub fn validate_edit(edit: &MetaEdit) -> Result<(), String> {
    if edit.group().trim().is_empty() {
        return Err("edit needs a metadata group".into());
    }
    if edit.key().trim().is_empty() {
        return Err("edit needs a field key".into());
    }
    Ok(())
}

fn find(fields: &[MetaField], group: &str, key: &str) -> Option<usize> {
    fields.iter().position(|f| f.group.eq_ignore_ascii_case(group) && f.key.eq_ignore_ascii_case(key))
}

/// Apply `edits` to `fields`, in order. `Set` updates an existing editable field or **adds** a new
/// (editable) one; `Clear` removes an existing editable field. Edits targeting a **read-only** field, or
/// a malformed edit, are skipped and recorded in `rejected` — the rest still apply. Deterministic.
pub fn apply_edits(fields: &[MetaField], edits: &[MetaEdit]) -> EditResult {
    let mut out = fields.to_vec();
    let mut applied = Vec::new();
    let mut rejected = Vec::new();

    for edit in edits {
        if let Err(e) = validate_edit(edit) {
            rejected.push(e);
            continue;
        }
        let (g, k) = (edit.group(), edit.key());
        match edit {
            MetaEdit::Set { value, .. } => match find(&out, g, k) {
                Some(i) if out[i].editable => {
                    out[i].value = value.clone();
                    applied.push(format!("set {g}.{k}"));
                }
                Some(_) => rejected.push(format!("{g}.{k} is read-only")),
                None => {
                    out.push(MetaField { group: g.to_string(), key: k.to_string(), value: value.clone(), editable: true });
                    applied.push(format!("add {g}.{k}"));
                }
            },
            MetaEdit::Clear { .. } => match find(&out, g, k) {
                Some(i) if out[i].editable => {
                    out.remove(i);
                    applied.push(format!("clear {g}.{k}"));
                }
                Some(_) => rejected.push(format!("{g}.{k} is read-only")),
                None => rejected.push(format!("{g}.{k} not present")),
            },
        }
    }

    EditResult { fields: out, applied, rejected }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(group: &str, key: &str, value: &str, editable: bool) -> MetaField {
        MetaField { group: group.into(), key: key.into(), value: value.into(), editable }
    }

    #[test]
    fn set_updates_an_editable_field() {
        let fields = vec![field("iptc", "Caption", "old", true)];
        let r = apply_edits(&fields, &[MetaEdit::Set { group: "iptc".into(), key: "Caption".into(), value: "new".into() }]);
        assert_eq!(r.fields[0].value, "new");
        assert_eq!(r.applied, vec!["set iptc.Caption"]);
        assert!(r.rejected.is_empty());
    }

    #[test]
    fn set_adds_a_new_field_when_absent() {
        let r = apply_edits(&[], &[MetaEdit::Set { group: "id3".into(), key: "Artist".into(), value: "Nova".into() }]);
        assert_eq!(r.fields.len(), 1);
        assert_eq!((r.fields[0].key.as_str(), r.fields[0].value.as_str(), r.fields[0].editable), ("Artist", "Nova", true));
        assert_eq!(r.applied, vec!["add id3.Artist"]);
    }

    #[test]
    fn read_only_fields_are_rejected_not_changed() {
        let fields = vec![field("exif", "PixelWidth", "1920", false)];
        let r = apply_edits(&fields, &[MetaEdit::Set { group: "exif".into(), key: "PixelWidth".into(), value: "1".into() }]);
        assert_eq!(r.fields[0].value, "1920"); // unchanged
        assert!(r.applied.is_empty());
        assert_eq!(r.rejected, vec!["exif.PixelWidth is read-only"]);
    }

    #[test]
    fn clear_removes_editable_and_reports_missing() {
        let fields = vec![field("id3", "Comment", "hi", true)];
        let r = apply_edits(&fields, &[
            MetaEdit::Clear { group: "id3".into(), key: "Comment".into() },
            MetaEdit::Clear { group: "id3".into(), key: "Nope".into() },
        ]);
        assert!(r.fields.is_empty());
        assert_eq!(r.applied, vec!["clear id3.Comment"]);
        assert_eq!(r.rejected, vec!["id3.Nope not present"]);
    }

    #[test]
    fn key_and_group_match_case_insensitively() {
        let fields = vec![field("EXIF", "Artist", "a", true)];
        let r = apply_edits(&fields, &[MetaEdit::Set { group: "exif".into(), key: "artist".into(), value: "b".into() }]);
        assert_eq!(r.fields[0].value, "b"); // matched despite case
        assert_eq!(r.applied, vec!["set exif.artist"]);
    }

    #[test]
    fn malformed_edits_are_rejected() {
        let r = apply_edits(&[], &[MetaEdit::Set { group: "  ".into(), key: "k".into(), value: "v".into() }]);
        assert!(r.fields.is_empty());
        assert_eq!(r.rejected.len(), 1);
    }
}
