//! File-inspection composition (CPE-1009, epic CPE-1002): compose the pure detectors into one file-level
//! report that the Properties dialog surfaces for a selected file.
//!
//! Pure over a file's leading bytes + its name — the Tauri command reads the bytes and calls
//! [`inspect_bytes`]. Reuses [`crate::text_encoding`] (encoding + line endings) and [`crate::file_type`]
//! (true type + extension mismatch); no I/O, no new deps.

use serde::Serialize;

use crate::bin_arch::{self, ArchInfo, Bitness, Endian};
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
    /// Detected CPU architecture for an ELF/PE/Mach-O binary (CPE-1485), e.g. `"x86-64 (64-bit,
    /// little-endian)"` or, for a Mach-O universal binary, `"Universal: x86-64 + ARM64"`; `None` when
    /// `file_type` isn't a recognised binary format, or its header doesn't parse.
    pub architecture: Option<String>,
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
    let architecture = bin_arch::detect_arch(bytes).map(|a| architecture_label(&a));

    FileInspection { encoding: enc.label().to_string(), line_endings, file_type, type_mismatch, architecture }
}

/// Render a detected [`bin_arch::BinaryArch`] as a single display-ready label.
fn architecture_label(arch: &bin_arch::BinaryArch) -> String {
    match arch {
        bin_arch::BinaryArch::Single(info) => arch_info_label(info),
        bin_arch::BinaryArch::Fat(slices) => {
            if slices.is_empty() {
                "Universal binary (no architecture slices)".to_string()
            } else {
                let list: Vec<&str> = slices.iter().map(|s| s.arch.label()).collect();
                format!("Universal: {}", list.join(" + "))
            }
        }
    }
}

/// Render one [`ArchInfo`] as `"<arch>"` or, when bitness/endianness are known, `"<arch> (64-bit,
/// little-endian)"`.
fn arch_info_label(info: &ArchInfo) -> String {
    let mut extras = Vec::new();
    if let Some(b) = info.bitness {
        extras.push(match b {
            Bitness::Bit32 => "32-bit",
            Bitness::Bit64 => "64-bit",
        });
    }
    if let Some(e) = info.endian {
        extras.push(match e {
            Endian::Little => "little-endian",
            Endian::Big => "big-endian",
        });
    }
    if extras.is_empty() {
        info.arch.label().to_string()
    } else {
        format!("{} ({})", info.arch.label(), extras.join(", "))
    }
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
        assert!(i.architecture.is_none());
    }

    // ---- architecture (CPE-1485) ----

    #[test]
    fn reports_architecture_for_an_x86_64_elf_binary() {
        let mut bytes = vec![0x7F, 0x45, 0x4C, 0x46, 2, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        bytes.extend_from_slice(&[0, 0]); // e_type, irrelevant
        bytes.extend_from_slice(&0x3Eu16.to_le_bytes()); // e_machine: EM_X86_64
        let i = inspect_bytes("myapp", &bytes);
        assert_eq!(i.file_type.as_deref(), Some("ELF executable"));
        assert_eq!(i.architecture.as_deref(), Some("x86-64 (64-bit, little-endian)"));
    }

    #[test]
    fn reports_architecture_for_a_pe_binary_with_no_bitness_or_endian_suffix() {
        let pe_offset: u32 = 0x40;
        let mut bytes = vec![0u8; pe_offset as usize + 6];
        bytes[0] = 0x4D;
        bytes[1] = 0x5A;
        bytes[0x3C..0x40].copy_from_slice(&pe_offset.to_le_bytes());
        bytes[pe_offset as usize..pe_offset as usize + 4].copy_from_slice(b"PE\0\0");
        bytes[pe_offset as usize + 4..pe_offset as usize + 6].copy_from_slice(&0x8664u16.to_le_bytes());
        let i = inspect_bytes("app.exe", &bytes);
        assert_eq!(i.file_type.as_deref(), Some("Windows executable/library"));
        assert_eq!(i.architecture.as_deref(), Some("x86-64"));
    }

    #[test]
    fn reports_architecture_slices_for_a_fat_macho_binary() {
        let mut bytes = vec![0xCA, 0xFE, 0xBA, 0xBE];
        bytes.extend_from_slice(&2u32.to_be_bytes());
        for cputype in [7u32 | 0x0100_0000, 12u32 | 0x0100_0000] {
            bytes.extend_from_slice(&cputype.to_be_bytes());
            bytes.extend_from_slice(&[0u8; 16]); // cpusubtype, offset, size, align
        }
        let i = inspect_bytes("universal", &bytes);
        assert_eq!(i.architecture.as_deref(), Some("Universal: x86-64 + ARM64"));
    }

    #[test]
    fn architecture_is_none_for_non_binary_content() {
        let i = inspect_bytes("notes.txt", b"line one\nline two\n");
        assert!(i.architecture.is_none());
    }
}
