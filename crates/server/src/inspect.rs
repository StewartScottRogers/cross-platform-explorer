//! File-inspection composition (CPE-1009, epic CPE-1002): compose the pure detectors into one file-level
//! report that the Properties dialog surfaces for a selected file.
//!
//! Pure over a file's leading bytes + its name — the Tauri command reads the bytes and calls
//! [`inspect_bytes`]. Reuses [`crate::text_encoding`] (encoding + line endings) and [`crate::file_type`]
//! (true type + extension mismatch); no I/O, no new deps.

use serde::Serialize;

use crate::file_type::{detect_type, mismatch};
use crate::text_encoding::{detect_encoding, detect_line_endings, EncodingGuess, LineEnding};

/// A file's inspection result — display-ready strings for the Properties panel. `None` fields are simply
/// not shown.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FileInspection {
    /// Detected text encoding label (e.g. `"UTF-8"`, `"UTF-16 LE"`, `"Binary"`).
    pub encoding: String,
    /// Line-ending summary for text-ish files (e.g. `"LF (Unix)"`, `"CRLF (Windows)"`, `"Mixed"`); `None`
    /// for binary/empty files or text with no line breaks.
    pub line_endings: Option<String>,
    /// Detected true file type from the magic bytes (e.g. `"PNG image"`); `None` if unrecognised.
    pub file_type: Option<String>,
    /// A human warning when the content doesn't match the extension (a disguised file); `None` when it
    /// matches, the type is unknown, or there is no extension.
    pub type_mismatch: Option<String>,
}

/// Inspect a file from its `name` (for the extension) + its leading `bytes`.
pub fn inspect_bytes(name: &str, bytes: &[u8]) -> FileInspection {
    let enc = detect_encoding(bytes);
    let line_endings = match enc {
        EncodingGuess::Binary | EncodingGuess::Empty => None,
        _ => line_ending_label(&String::from_utf8_lossy(bytes)),
    };
    let file_type = detect_type(bytes).map(|t| t.label().to_string());
    let ext = name.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    let type_mismatch = mismatch(bytes, ext)
        .map(|m| format!("Looks like {} but the extension is .{}", m.detected.label(), m.actual_ext));

    FileInspection { encoding: enc.label().to_string(), line_endings, file_type, type_mismatch }
}

/// A human line-ending label from a decoded string, or `None` when the text has no line breaks.
fn line_ending_label(text: &str) -> Option<String> {
    let r = detect_line_endings(text);
    if r.crlf == 0 && r.lf == 0 && r.cr == 0 {
        return None;
    }
    if r.mixed {
        return Some("Mixed".to_string());
    }
    Some(
        match r.dominant {
            LineEnding::Crlf => "CRLF (Windows)",
            LineEnding::Lf => "LF (Unix)",
            LineEnding::Cr => "CR (classic Mac)",
            LineEnding::None | LineEnding::Mixed => "—",
        }
        .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspects_a_utf8_text_file() {
        let i = inspect_bytes("notes.txt", b"line one\nline two\n");
        assert_eq!(i.encoding, EncodingGuess::Utf8.label());
        assert_eq!(i.line_endings.as_deref(), Some("LF (Unix)"));
        assert!(i.type_mismatch.is_none()); // no magic type detected for plain text → no mismatch verdict
    }

    #[test]
    fn reports_crlf_and_mixed_line_endings() {
        assert_eq!(inspect_bytes("a.txt", b"x\r\ny\r\n").line_endings.as_deref(), Some("CRLF (Windows)"));
        assert_eq!(inspect_bytes("a.txt", b"x\r\ny\n").line_endings.as_deref(), Some("Mixed"));
        // No line breaks → no line-ending line shown.
        assert_eq!(inspect_bytes("a.txt", b"one line, no break").line_endings, None);
    }

    #[test]
    fn detects_type_and_flags_a_disguised_file() {
        // PNG magic bytes but a .jpg name → recognised type + a mismatch warning.
        let png = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0];
        let i = inspect_bytes("photo.jpg", &png);
        assert_eq!(i.file_type.as_deref(), Some("PNG image"));
        let warn = i.type_mismatch.expect("a .jpg with PNG bytes must warn");
        assert!(warn.contains("PNG") && warn.contains("jpg"), "warning was: {warn}");
        // Binary image → no line-ending line.
        assert!(i.line_endings.is_none());
    }

    #[test]
    fn matching_extension_has_no_mismatch_warning() {
        let png = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0];
        let i = inspect_bytes("photo.png", &png);
        assert_eq!(i.file_type.as_deref(), Some("PNG image"));
        assert!(i.type_mismatch.is_none());
    }

    #[test]
    fn empty_file_is_reported_without_line_endings() {
        let i = inspect_bytes("empty", b"");
        assert_eq!(i.encoding, EncodingGuess::Empty.label());
        assert!(i.line_endings.is_none());
        assert!(i.file_type.is_none());
    }
}
