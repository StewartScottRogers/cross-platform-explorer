//! Archive container format detector (CPE-1054, epic CPE-705): a pure, dependency-free magic-byte +
//! extension sniff that identifies which archive container a file is, so the app can route it to the
//! right reader/extractor (see `archive.rs`) before opening it. Deliberately **does not call any
//! archive crate** — just byte-prefix checks and an extension fallback, so it's O(1)/O(len) with no
//! allocation beyond lowercasing the name. Distinct from `file_type::detect_type` (general file-type
//! detection): this is archive-*container* disambiguation, notably the tar-in-gz (`.tar.gz`/`.tgz` vs
//! bare `.gz`) nuance that a plain magic-byte check can't resolve on its own.

/// Which archive container a file is, as sniffed by magic bytes (falling back to the file extension).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum ArchiveFormat {
    Zip,
    Tar,
    Gz,
    TarGz,
    SevenZ,
    Iso,
    Unknown,
}

/// Detect the archive container format from `bytes` (the file's leading bytes — a full read or a
/// prefix long enough to cover the signature offsets used here) and its file `name` (used to
/// disambiguate gzip-vs-tar-in-gzip, and as the sole signal when magic bytes are absent/ambiguous).
///
/// Checks, in order:
/// - `PK\x03\x04` / `PK\x05\x06` / `PK\x07\x08` at offset 0 → [`ArchiveFormat::Zip`]
/// - `1F 8B` at offset 0 → gzip; `.tar.gz`/`.tgz` in `name` → [`ArchiveFormat::TarGz`], else
///   [`ArchiveFormat::Gz`]
/// - `7z\xBC\xAF\x27\x1C` at offset 0 → [`ArchiveFormat::SevenZ`]
/// - `ustar` at offset 257 → [`ArchiveFormat::Tar`]
/// - `CD001` at offset 0x8001 (32769) → [`ArchiveFormat::Iso`]
/// - otherwise, the file extension (`.zip`, `.tar`, `.tgz`/`.tar.gz`, `.gz`, `.7z`, `.iso`); no match
///   → [`ArchiveFormat::Unknown`]
///
/// Never panics on truncated, empty, or too-short input — every offset is bounds-checked first.
pub fn detect_format(bytes: &[u8], name: &str) -> ArchiveFormat {
    let lower = name.to_ascii_lowercase();
    let is_tar_gz_name = lower.ends_with(".tar.gz") || lower.ends_with(".tgz");

    if bytes.starts_with(b"PK\x03\x04")
        || bytes.starts_with(b"PK\x05\x06")
        || bytes.starts_with(b"PK\x07\x08")
    {
        return ArchiveFormat::Zip;
    }

    if bytes.starts_with(&[0x1F, 0x8B]) {
        return if is_tar_gz_name {
            ArchiveFormat::TarGz
        } else {
            ArchiveFormat::Gz
        };
    }

    if bytes.starts_with(&[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C]) {
        return ArchiveFormat::SevenZ;
    }

    if bytes.len() >= 262 && &bytes[257..262] == b"ustar" {
        return ArchiveFormat::Tar;
    }

    const ISO_SIG_OFFSET: usize = 0x8001;
    if bytes.len() >= ISO_SIG_OFFSET + 5 && &bytes[ISO_SIG_OFFSET..ISO_SIG_OFFSET + 5] == b"CD001" {
        return ArchiveFormat::Iso;
    }

    // No magic-byte match (or bytes too short to check it) — fall back to the extension.
    if is_tar_gz_name {
        ArchiveFormat::TarGz
    } else if lower.ends_with(".zip") {
        ArchiveFormat::Zip
    } else if lower.ends_with(".tar") {
        ArchiveFormat::Tar
    } else if lower.ends_with(".gz") {
        ArchiveFormat::Gz
    } else if lower.ends_with(".7z") {
        ArchiveFormat::SevenZ
    } else if lower.ends_with(".iso") {
        ArchiveFormat::Iso
    } else {
        ArchiveFormat::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zip_signatures() {
        assert_eq!(detect_format(b"PK\x03\x04rest", "a.zip"), ArchiveFormat::Zip);
        assert_eq!(detect_format(b"PK\x05\x06rest", "empty.zip"), ArchiveFormat::Zip);
        assert_eq!(detect_format(b"PK\x07\x08rest", "spanned.zip"), ArchiveFormat::Zip);
        // Magic wins even with a misleading extension.
        assert_eq!(detect_format(b"PK\x03\x04rest", "a.dat"), ArchiveFormat::Zip);
    }

    #[test]
    fn gzip_vs_tar_gz_by_name() {
        let gzip_bytes = [0x1F, 0x8B, 0x08, 0x00];
        assert_eq!(detect_format(&gzip_bytes, "archive.tar.gz"), ArchiveFormat::TarGz);
        assert_eq!(detect_format(&gzip_bytes, "archive.tgz"), ArchiveFormat::TarGz);
        assert_eq!(detect_format(&gzip_bytes, "file.gz"), ArchiveFormat::Gz);
        // No recognisable name hint at all — plain gzip.
        assert_eq!(detect_format(&gzip_bytes, "noext"), ArchiveFormat::Gz);
    }

    #[test]
    fn sevenz_signature() {
        let bytes = [0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C, 0x00, 0x04];
        assert_eq!(detect_format(&bytes, "a.7z"), ArchiveFormat::SevenZ);
        assert_eq!(detect_format(&bytes, "a.dat"), ArchiveFormat::SevenZ);
    }

    #[test]
    fn tar_ustar_at_offset_257() {
        let mut bytes = vec![0u8; 262];
        bytes[257..262].copy_from_slice(b"ustar");
        assert_eq!(detect_format(&bytes, "a.tar"), ArchiveFormat::Tar);
        assert_eq!(detect_format(&bytes, "a.dat"), ArchiveFormat::Tar);
    }

    #[test]
    fn iso_cd001_at_offset_0x8001() {
        let mut bytes = vec![0u8; 0x8001 + 5];
        bytes[0x8001..0x8001 + 5].copy_from_slice(b"CD001");
        assert_eq!(detect_format(&bytes, "a.iso"), ArchiveFormat::Iso);
        assert_eq!(detect_format(&bytes, "a.dat"), ArchiveFormat::Iso);
    }

    #[test]
    fn truncated_and_empty_inputs_never_panic() {
        assert_eq!(detect_format(&[], ""), ArchiveFormat::Unknown);
        assert_eq!(detect_format(&[], "a.zip"), ArchiveFormat::Zip);
        assert_eq!(detect_format(&[0x50], "a.tar"), ArchiveFormat::Tar);
        assert_eq!(detect_format(&[0x1F], "a.gz"), ArchiveFormat::Gz);
        // Short buffer that can't reach the ustar/CD001 offsets falls back to extension.
        let short = [0u8; 10];
        assert_eq!(detect_format(&short, "a.tar"), ArchiveFormat::Tar);
        assert_eq!(detect_format(&short, "a.iso"), ArchiveFormat::Iso);
        assert_eq!(detect_format(&short, "unrecognised"), ArchiveFormat::Unknown);
    }

    #[test]
    fn extension_only_fallback_when_magic_absent() {
        let plain = [0u8; 16];
        assert_eq!(detect_format(&plain, "a.zip"), ArchiveFormat::Zip);
        assert_eq!(detect_format(&plain, "a.tar"), ArchiveFormat::Tar);
        assert_eq!(detect_format(&plain, "a.tar.gz"), ArchiveFormat::TarGz);
        assert_eq!(detect_format(&plain, "a.tgz"), ArchiveFormat::TarGz);
        assert_eq!(detect_format(&plain, "a.gz"), ArchiveFormat::Gz);
        assert_eq!(detect_format(&plain, "a.7z"), ArchiveFormat::SevenZ);
        assert_eq!(detect_format(&plain, "a.iso"), ArchiveFormat::Iso);
        assert_eq!(detect_format(&plain, "a.txt"), ArchiveFormat::Unknown);
    }

    #[test]
    fn extension_matching_is_case_insensitive() {
        let plain = [0u8; 4];
        assert_eq!(detect_format(&plain, "A.ZIP"), ArchiveFormat::Zip);
        assert_eq!(detect_format(&plain, "A.TAR.GZ"), ArchiveFormat::TarGz);
    }
}
