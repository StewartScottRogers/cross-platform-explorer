//! Search type-class filter (CPE-1061, epic CPE-703 "Instant index search"): a pure, dependency-free
//! mapping of file extension → semantic class, plus a `type:` filter predicate so search can filter by
//! "images", "video", etc. (`type:image`, `type:image,video`). Standalone module — extension-based only,
//! and deliberately distinct from [`crate::file_type`], which sniffs **magic bytes** (actual file content)
//! rather than classifying by extension; the two answer different questions and neither wraps the other.
//!
//! Pure string lowercasing + static-table lookup — no `std::path`, no platform semantics, no `#[cfg]`
//! branches — so behavior is identical across the 3-OS CI matrix.

/// A semantic class a file extension can fall into, used to power the `type:` search filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum FileClass {
    Image,
    Video,
    Audio,
    Document,
    Archive,
    Code,
    Executable,
    Other,
}

/// A parsed `type:` search filter — one or more classes, any of which a matching file's extension must
/// fall into (an OR filter, e.g. `type:image,video`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TypeFilter {
    pub classes: Vec<FileClass>,
}

/// Static ext → class table. Extensions are stored lowercased (no leading dot); lookup lowercases its
/// input so callers can pass any case.
const IMAGE_EXTS: &[&str] =
    &["png", "jpg", "jpeg", "jpe", "gif", "webp", "bmp", "svg", "tif", "tiff", "heic", "heif", "ico", "avif"];
const VIDEO_EXTS: &[&str] =
    &["mp4", "mov", "mkv", "avi", "webm", "m4v", "wmv", "flv", "mpg", "mpeg", "3gp"];
const AUDIO_EXTS: &[&str] = &["mp3", "flac", "wav", "ogg", "oga", "m4a", "aac", "wma", "opus", "aiff"];
const DOCUMENT_EXTS: &[&str] = &[
    "pdf", "doc", "docx", "txt", "md", "rtf", "odt", "xls", "xlsx", "ods", "ppt", "pptx", "odp", "csv",
    "epub",
];
const ARCHIVE_EXTS: &[&str] =
    &["zip", "tar", "gz", "tgz", "7z", "rar", "xz", "zst", "bz2", "iso", "cab"];
const CODE_EXTS: &[&str] = &[
    "rs", "ts", "tsx", "js", "jsx", "py", "go", "c", "h", "cpp", "cc", "hpp", "java", "rb", "sh", "ps1",
    "php", "cs", "swift", "kt", "json", "toml", "yaml", "yml", "html", "css",
];
const EXECUTABLE_EXTS: &[&str] = &["exe", "dll", "so", "dylib", "app", "msi", "bat", "cmd", "com", "bin"];

/// Classify a file extension (no leading dot, any case) into its semantic [`FileClass`]. An unrecognised
/// or empty extension yields [`FileClass::Other`] — never an error.
pub fn class_of(ext: &str) -> FileClass {
    let ext = ext.to_ascii_lowercase();
    let ext = ext.as_str();
    if IMAGE_EXTS.contains(&ext) {
        FileClass::Image
    } else if VIDEO_EXTS.contains(&ext) {
        FileClass::Video
    } else if AUDIO_EXTS.contains(&ext) {
        FileClass::Audio
    } else if DOCUMENT_EXTS.contains(&ext) {
        FileClass::Document
    } else if ARCHIVE_EXTS.contains(&ext) {
        FileClass::Archive
    } else if CODE_EXTS.contains(&ext) {
        FileClass::Code
    } else if EXECUTABLE_EXTS.contains(&ext) {
        FileClass::Executable
    } else {
        FileClass::Other
    }
}

/// Parse a class name (case-insensitive) into a [`FileClass`]. Returns `None` for anything unrecognised
/// (including `"other"`, which is not a selectable filter target — a file only lands there by exclusion).
fn class_from_name(name: &str) -> Option<FileClass> {
    match name.to_ascii_lowercase().as_str() {
        "image" | "images" => Some(FileClass::Image),
        "video" | "videos" => Some(FileClass::Video),
        "audio" => Some(FileClass::Audio),
        "document" | "documents" | "doc" | "docs" => Some(FileClass::Document),
        "archive" | "archives" => Some(FileClass::Archive),
        "code" => Some(FileClass::Code),
        "executable" | "executables" | "exe" => Some(FileClass::Executable),
        _ => None,
    }
}

/// Parse a `type:` search token (e.g. `type:image` or `type:image,video`) into a [`TypeFilter`]. Accepts
/// the token with or without the `type:` prefix. Returns `None` for empty input, a missing/empty class
/// list, or any unrecognised class name — never panics.
pub fn parse(token: &str) -> Option<TypeFilter> {
    let token = token.trim();
    if token.is_empty() {
        return None;
    }
    let rest = token.strip_prefix("type:").unwrap_or(token);
    if rest.is_empty() {
        return None;
    }
    let mut classes = Vec::new();
    for part in rest.split(',') {
        let part = part.trim();
        if part.is_empty() {
            return None;
        }
        classes.push(class_from_name(part)?);
    }
    if classes.is_empty() {
        return None;
    }
    Some(TypeFilter { classes })
}

/// True iff `ext`'s class is one of `filter`'s classes.
pub fn matches(filter: &TypeFilter, ext: &str) -> bool {
    filter.classes.contains(&class_of(ext))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_of_covers_all_classes() {
        assert_eq!(class_of("png"), FileClass::Image);
        assert_eq!(class_of("jpg"), FileClass::Image);
        assert_eq!(class_of("mp4"), FileClass::Video);
        assert_eq!(class_of("mkv"), FileClass::Video);
        assert_eq!(class_of("mp3"), FileClass::Audio);
        assert_eq!(class_of("flac"), FileClass::Audio);
        assert_eq!(class_of("pdf"), FileClass::Document);
        assert_eq!(class_of("docx"), FileClass::Document);
        assert_eq!(class_of("zip"), FileClass::Archive);
        assert_eq!(class_of("7z"), FileClass::Archive);
        assert_eq!(class_of("rs"), FileClass::Code);
        assert_eq!(class_of("py"), FileClass::Code);
        assert_eq!(class_of("exe"), FileClass::Executable);
        assert_eq!(class_of("dll"), FileClass::Executable);
    }

    #[test]
    fn class_of_unknown_ext_is_other() {
        assert_eq!(class_of("xyz123"), FileClass::Other);
        assert_eq!(class_of(""), FileClass::Other);
        assert_eq!(class_of("!!!"), FileClass::Other);
    }

    #[test]
    fn class_of_is_case_insensitive() {
        assert_eq!(class_of("PNG"), FileClass::Image);
        assert_eq!(class_of("Png"), FileClass::Image);
        assert_eq!(class_of("MP4"), FileClass::Video);
        assert_eq!(class_of("Exe"), FileClass::Executable);
    }

    #[test]
    fn parse_single_class() {
        let f = parse("type:image").unwrap();
        assert_eq!(f.classes, vec![FileClass::Image]);
    }

    #[test]
    fn parse_multi_class() {
        let f = parse("type:image,video").unwrap();
        assert_eq!(f.classes, vec![FileClass::Image, FileClass::Video]);
    }

    #[test]
    fn parse_accepts_bare_token_without_prefix() {
        let f = parse("image,audio").unwrap();
        assert_eq!(f.classes, vec![FileClass::Image, FileClass::Audio]);
    }

    #[test]
    fn parse_is_case_insensitive() {
        let f = parse("type:IMAGE,Video").unwrap();
        assert_eq!(f.classes, vec![FileClass::Image, FileClass::Video]);
    }

    #[test]
    fn parse_empty_or_garbage_is_none() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("type:"), None);
        assert_eq!(parse("type:bogus"), None);
        assert_eq!(parse("type:image,bogus"), None);
        assert_eq!(parse("   "), None);
        assert_eq!(parse("type:,"), None);
        assert_eq!(parse("type:image,"), None);
    }

    #[test]
    fn matches_true_iff_ext_class_in_filter() {
        let f = parse("type:image,video").unwrap();
        assert!(matches(&f, "png"));
        assert!(matches(&f, "mp4"));
        assert!(!matches(&f, "mp3"));
        assert!(!matches(&f, "pdf"));
    }

    #[test]
    fn matches_case_insensitive_ext() {
        let f = parse("type:image").unwrap();
        assert!(matches(&f, "PNG"));
        assert!(matches(&f, "Jpg"));
    }

    #[test]
    fn matches_unknown_ext_only_matches_other_filter() {
        let f = parse("type:image").unwrap();
        assert!(!matches(&f, "unknownext"));
    }
}
