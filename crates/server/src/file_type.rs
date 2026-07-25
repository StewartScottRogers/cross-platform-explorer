//! Magic-byte file-type detection + extension-mismatch flagging (CPE-1001, epic CPE-1000 "True file-type
//! detection & extension-mismatch flagging"). A pure signature sniffer over an in-memory byte slice — no
//! filesystem I/O, no new dependencies — that recognises a curated set of common binary formats from their
//! leading magic bytes and, given a claimed extension, reports when the two disagree (e.g. a `.jpg` that
//! is actually a Windows PE executable in disguise). This ticket is the pure detection core only; a future
//! ticket wires a capped byte-prefix read + a UI warning on top of [`mismatch`].

/// A recognised binary file format, identified by its leading magic bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    Png,
    Jpeg,
    Gif,
    Bmp,
    WebP,
    Tiff,
    Pdf,
    Zip,
    Gzip,
    SevenZip,
    Rar,
    Elf,
    /// Windows PE image — covers both `.exe` and `.dll` (same `MZ`/PE container format).
    Pe,
    Wasm,
    Flac,
    Ogg,
    Mp3,
    Wav,
    Mp4,
}

impl FileType {
    /// A short, human-readable name for display (e.g. in a mismatch warning).
    pub fn label(self) -> &'static str {
        match self {
            FileType::Png => "PNG image",
            FileType::Jpeg => "JPEG image",
            FileType::Gif => "GIF image",
            FileType::Bmp => "BMP image",
            FileType::WebP => "WebP image",
            FileType::Tiff => "TIFF image",
            FileType::Pdf => "PDF document",
            FileType::Zip => "ZIP archive",
            FileType::Gzip => "Gzip archive",
            FileType::SevenZip => "7-Zip archive",
            FileType::Rar => "RAR archive",
            FileType::Elf => "ELF executable",
            FileType::Pe => "Windows executable/library",
            FileType::Wasm => "WebAssembly module",
            FileType::Flac => "FLAC audio",
            FileType::Ogg => "Ogg audio",
            FileType::Mp3 => "MP3 audio",
            FileType::Wav => "WAV audio",
            FileType::Mp4 => "MP4 media",
        }
    }

    /// The canonical lowercased extensions (no leading dot) this type is expected to appear under.
    ///
    /// A few formats are containers reused by several well-known file kinds — most notably ZIP, which
    /// backs Office Open XML (`.docx`/`.xlsx`/`.pptx`), OpenDocument (`.odt`/`.ods`/`.odp`), `.epub`,
    /// `.jar`, and `.apk`. All of those are listed here so [`mismatch`] doesn't false-flag a `.docx` as a
    /// "renamed .zip" — the container format is correct, only the payload inside differs.
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            FileType::Png => &["png"],
            FileType::Jpeg => &["jpg", "jpeg", "jpe"],
            FileType::Gif => &["gif"],
            FileType::Bmp => &["bmp"],
            FileType::WebP => &["webp"],
            FileType::Tiff => &["tif", "tiff"],
            FileType::Pdf => &["pdf"],
            FileType::Zip => {
                &["zip", "jar", "apk", "docx", "xlsx", "pptx", "odt", "ods", "odp", "epub"]
            }
            FileType::Gzip => &["gz", "tgz"],
            FileType::SevenZip => &["7z"],
            FileType::Rar => &["rar"],
            FileType::Elf => &["so"],
            FileType::Pe => &["exe", "dll"],
            FileType::Wasm => &["wasm"],
            FileType::Flac => &["flac"],
            FileType::Ogg => &["ogg", "oga", "ogv"],
            FileType::Mp3 => &["mp3"],
            FileType::Wav => &["wav"],
            FileType::Mp4 => &["mp4", "m4a", "m4v"],
        }
    }
}

/// True if `bytes` is at least `offset + pat.len()` long and `pat` matches at `offset`. Bounds-checked so
/// callers never slice out of range on short/empty input.
fn matches_at(bytes: &[u8], offset: usize, pat: &[u8]) -> bool {
    bytes.len() >= offset + pat.len() && &bytes[offset..offset + pat.len()] == pat
}

/// Sniff `bytes` for a recognised magic signature. Returns `None` for unknown or too-short input; never
/// panics regardless of length (including empty).
///
/// Order matters only where a shorter/less specific signature could otherwise shadow a longer one still to
/// come; the two-byte `MZ` (PE) check is placed last for exactly that reason. Every other signature here is
/// unambiguous relative to the rest of the set.
pub fn detect_type(bytes: &[u8]) -> Option<FileType> {
    if matches_at(bytes, 0, &[0x89, 0x50, 0x4E, 0x47]) {
        return Some(FileType::Png);
    }
    if matches_at(bytes, 0, &[0xFF, 0xD8, 0xFF]) {
        return Some(FileType::Jpeg);
    }
    if matches_at(bytes, 0, b"GIF87a") || matches_at(bytes, 0, b"GIF89a") {
        return Some(FileType::Gif);
    }
    if matches_at(bytes, 0, &[0x42, 0x4D]) {
        return Some(FileType::Bmp);
    }
    if matches_at(bytes, 0, &[0x49, 0x49, 0x2A, 0x00]) || matches_at(bytes, 0, &[0x4D, 0x4D, 0x00, 0x2A]) {
        return Some(FileType::Tiff);
    }
    if matches_at(bytes, 0, b"%PDF") {
        return Some(FileType::Pdf);
    }
    // Local file header, empty-archive, and spanned-archive ZIP variants.
    if matches_at(bytes, 0, &[0x50, 0x4B, 0x03, 0x04])
        || matches_at(bytes, 0, &[0x50, 0x4B, 0x05, 0x06])
        || matches_at(bytes, 0, &[0x50, 0x4B, 0x07, 0x08])
    {
        return Some(FileType::Zip);
    }
    if matches_at(bytes, 0, &[0x1F, 0x8B]) {
        return Some(FileType::Gzip);
    }
    if matches_at(bytes, 0, &[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C]) {
        return Some(FileType::SevenZip);
    }
    if matches_at(bytes, 0, &[0x52, 0x61, 0x72, 0x21, 0x1A, 0x07]) {
        return Some(FileType::Rar);
    }
    if matches_at(bytes, 0, &[0x7F, 0x45, 0x4C, 0x46]) {
        return Some(FileType::Elf);
    }
    if matches_at(bytes, 0, &[0x00, 0x61, 0x73, 0x6D]) {
        return Some(FileType::Wasm);
    }
    if matches_at(bytes, 0, b"fLaC") {
        return Some(FileType::Flac);
    }
    if matches_at(bytes, 0, b"OggS") {
        return Some(FileType::Ogg);
    }
    // RIFF container: the format is disambiguated by the 4-byte tag at offset 8, after the chunk size.
    if matches_at(bytes, 0, b"RIFF") {
        if matches_at(bytes, 8, b"WAVE") {
            return Some(FileType::Wav);
        }
        if matches_at(bytes, 8, b"WEBP") {
            return Some(FileType::WebP);
        }
    }
    // ISO base media container family (MP4/M4A/M4V/…): a 4-byte box size followed by "ftyp" at offset 4.
    if matches_at(bytes, 4, b"ftyp") {
        return Some(FileType::Mp4);
    }
    // MP3: either an ID3v2 tag up front, or a bare MPEG audio frame sync — 11 set sync bits, i.e. 0xFF
    // followed by a byte whose top 3 bits are all set.
    if matches_at(bytes, 0, b"ID3") {
        return Some(FileType::Mp3);
    }
    if bytes.len() >= 2 && bytes[0] == 0xFF && (bytes[1] & 0xE0) == 0xE0 {
        return Some(FileType::Mp3);
    }
    // PE ("MZ" DOS stub header): only two bytes, so checked last to minimise any accidental shadowing of
    // a more specific signature above (none of the above actually collide with it, but this keeps the
    // ordering defensively correct if a shorter signature is ever added later).
    if matches_at(bytes, 0, &[0x4D, 0x5A]) {
        return Some(FileType::Pe);
    }
    None
}

/// A detected type/claimed-extension disagreement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mismatch {
    /// The type sniffed from the file's bytes.
    pub detected: FileType,
    /// The claimed extension that was checked against it — lowercased, leading dot stripped.
    pub actual_ext: String,
}

/// Compare `bytes`' sniffed type against the claimed `ext` (a leading dot is stripped, case is folded).
///
/// Returns `Some(Mismatch)` only when a type **was** detected and `ext` is not among its
/// [`FileType::extensions`] — i.e. we can make a judgement and the judgement is "these disagree". Returns
/// `None` both when the type is unknown (nothing to compare against, so no verdict either way) and when
/// the extension already matches.
pub fn mismatch(bytes: &[u8], ext: &str) -> Option<Mismatch> {
    let detected = detect_type(bytes)?;
    let actual_ext = ext.strip_prefix('.').unwrap_or(ext).to_lowercase();
    if detected.extensions().contains(&actual_ext.as_str()) {
        None
    } else {
        Some(Mismatch { detected, actual_ext })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- detect_type: one fixture per covered format ----

    #[test]
    fn detects_png() {
        let bytes = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0];
        assert_eq!(detect_type(&bytes), Some(FileType::Png));
    }

    #[test]
    fn detects_jpeg() {
        let bytes = [0xFF, 0xD8, 0xFF, 0xE0, 0, 0, 0, 0];
        assert_eq!(detect_type(&bytes), Some(FileType::Jpeg));
    }

    #[test]
    fn detects_gif_both_versions() {
        assert_eq!(detect_type(b"GIF87a\x00\x00"), Some(FileType::Gif));
        assert_eq!(detect_type(b"GIF89a\x00\x00"), Some(FileType::Gif));
    }

    #[test]
    fn detects_bmp() {
        let bytes = [0x42, 0x4D, 0, 0, 0, 0, 0, 0];
        assert_eq!(detect_type(&bytes), Some(FileType::Bmp));
    }

    #[test]
    fn detects_tiff_both_endian_variants() {
        assert_eq!(detect_type(&[0x49, 0x49, 0x2A, 0x00, 0, 0]), Some(FileType::Tiff));
        assert_eq!(detect_type(&[0x4D, 0x4D, 0x00, 0x2A, 0, 0]), Some(FileType::Tiff));
    }

    #[test]
    fn detects_pdf() {
        assert_eq!(detect_type(b"%PDF-1.7\n"), Some(FileType::Pdf));
    }

    #[test]
    fn detects_zip_local_header_and_empty_and_spanned_variants() {
        assert_eq!(detect_type(&[0x50, 0x4B, 0x03, 0x04, 0, 0]), Some(FileType::Zip));
        assert_eq!(detect_type(&[0x50, 0x4B, 0x05, 0x06, 0, 0]), Some(FileType::Zip));
        assert_eq!(detect_type(&[0x50, 0x4B, 0x07, 0x08, 0, 0]), Some(FileType::Zip));
    }

    #[test]
    fn detects_gzip() {
        assert_eq!(detect_type(&[0x1F, 0x8B, 0x08, 0]), Some(FileType::Gzip));
    }

    #[test]
    fn detects_seven_zip() {
        assert_eq!(
            detect_type(&[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C, 0, 0]),
            Some(FileType::SevenZip)
        );
    }

    #[test]
    fn detects_rar() {
        assert_eq!(
            detect_type(&[0x52, 0x61, 0x72, 0x21, 0x1A, 0x07, 0, 0]),
            Some(FileType::Rar)
        );
    }

    #[test]
    fn detects_elf() {
        assert_eq!(detect_type(&[0x7F, 0x45, 0x4C, 0x46, 0x02, 0x01]), Some(FileType::Elf));
    }

    #[test]
    fn detects_pe() {
        assert_eq!(detect_type(&[0x4D, 0x5A, 0x90, 0x00]), Some(FileType::Pe));
    }

    #[test]
    fn detects_wasm() {
        assert_eq!(detect_type(&[0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00]), Some(FileType::Wasm));
    }

    #[test]
    fn detects_flac() {
        assert_eq!(detect_type(b"fLaC\x00\x00\x00\x22"), Some(FileType::Flac));
    }

    #[test]
    fn detects_ogg() {
        assert_eq!(detect_type(b"OggS\x00\x02"), Some(FileType::Ogg));
    }

    #[test]
    fn detects_mp3_via_id3_tag() {
        assert_eq!(detect_type(b"ID3\x03\x00\x00"), Some(FileType::Mp3));
    }

    #[test]
    fn detects_mp3_via_frame_sync() {
        assert_eq!(detect_type(&[0xFF, 0xFB, 0x90, 0x00]), Some(FileType::Mp3));
        // Sanity: a 0xFF byte NOT followed by three more set bits is not a valid frame sync.
        assert_eq!(detect_type(&[0xFF, 0x00, 0x90, 0x00]), None);
    }

    // ---- offset-based signatures ----

    #[test]
    fn detects_wav_by_riff_plus_wave_tag_at_offset_8() {
        let mut bytes = b"RIFF".to_vec();
        bytes.extend_from_slice(&[0x24, 0, 0, 0]); // chunk size, irrelevant to detection
        bytes.extend_from_slice(b"WAVEfmt ");
        assert_eq!(detect_type(&bytes), Some(FileType::Wav));
    }

    #[test]
    fn detects_webp_by_riff_plus_webp_tag_at_offset_8() {
        let mut bytes = b"RIFF".to_vec();
        bytes.extend_from_slice(&[0x1A, 0, 0, 0]);
        bytes.extend_from_slice(b"WEBPVP8 ");
        assert_eq!(detect_type(&bytes), Some(FileType::WebP));
    }

    #[test]
    fn riff_without_a_recognised_offset_8_tag_is_unknown() {
        let mut bytes = b"RIFF".to_vec();
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        bytes.extend_from_slice(b"XXXX");
        assert_eq!(detect_type(&bytes), None);
    }

    #[test]
    fn detects_mp4_by_ftyp_tag_at_offset_4() {
        let mut bytes = vec![0x00, 0x00, 0x00, 0x18]; // box size
        bytes.extend_from_slice(b"ftypisom");
        assert_eq!(detect_type(&bytes), Some(FileType::Mp4));
    }

    // ---- unknown / short / empty input never panics ----

    #[test]
    fn unknown_bytes_are_none() {
        assert_eq!(detect_type(b"just some plain text, not a binary format"), None);
    }

    #[test]
    fn empty_input_is_none_not_a_panic() {
        assert_eq!(detect_type(&[]), None);
    }

    #[test]
    fn one_byte_input_is_none_not_a_panic() {
        assert_eq!(detect_type(&[0x89]), None);
        assert_eq!(detect_type(&[0xFF]), None);
    }

    #[test]
    fn short_input_shorter_than_every_signature_never_panics() {
        for n in 0..8 {
            let bytes = vec![0xFFu8; n];
            let _ = detect_type(&bytes); // must not panic regardless of outcome
        }
    }

    // ---- mismatch ----

    fn pe_bytes() -> Vec<u8> {
        vec![0x4D, 0x5A, 0x90, 0x00]
    }

    fn jpeg_bytes() -> Vec<u8> {
        vec![0xFF, 0xD8, 0xFF, 0xE0]
    }

    fn zip_bytes() -> Vec<u8> {
        vec![0x50, 0x4B, 0x03, 0x04]
    }

    #[test]
    fn mismatch_flags_a_pe_renamed_to_jpg() {
        let m = mismatch(&pe_bytes(), "jpg").expect("PE bytes claiming .jpg must mismatch");
        assert_eq!(m.detected, FileType::Pe);
        assert_eq!(m.actual_ext, "jpg");
    }

    #[test]
    fn mismatch_is_none_when_jpeg_bytes_claim_a_jpeg_extension() {
        assert_eq!(mismatch(&jpeg_bytes(), "jpg"), None);
        assert_eq!(mismatch(&jpeg_bytes(), "jpeg"), None);
    }

    #[test]
    fn mismatch_is_none_for_zip_bytes_under_a_container_extension() {
        // .docx is a ZIP container under the hood — must not false-flag.
        assert_eq!(mismatch(&zip_bytes(), "docx"), None);
        assert_eq!(mismatch(&zip_bytes(), "xlsx"), None);
        assert_eq!(mismatch(&zip_bytes(), "zip"), None);
    }

    #[test]
    fn mismatch_strips_a_leading_dot() {
        let m = mismatch(&pe_bytes(), ".jpg").expect("leading dot must be stripped, not break the match");
        assert_eq!(m.actual_ext, "jpg");

        // And a correctly-matching extension with a leading dot is still recognised as a match (None).
        assert_eq!(mismatch(&pe_bytes(), ".exe"), None);
    }

    #[test]
    fn mismatch_is_case_insensitive() {
        assert_eq!(mismatch(&jpeg_bytes(), "JPG"), None);
        assert_eq!(mismatch(&jpeg_bytes(), "JPEG"), None);
        let m = mismatch(&pe_bytes(), "JPG").expect("case must not hide a real mismatch");
        assert_eq!(m.actual_ext, "jpg");
    }

    #[test]
    fn mismatch_is_none_for_unknown_bytes_regardless_of_extension() {
        let unknown = b"not any recognised binary format at all";
        assert_eq!(mismatch(unknown, "exe"), None);
        assert_eq!(mismatch(unknown, "jpg"), None);
        assert_eq!(mismatch(unknown, ""), None);
    }

    // ---- label()/extensions() sanity ----

    #[test]
    fn label_and_extensions_for_png_and_pe() {
        assert_eq!(FileType::Png.label(), "PNG image");
        assert_eq!(FileType::Png.extensions(), &["png"]);
        assert_eq!(FileType::Pe.label(), "Windows executable/library");
        assert_eq!(FileType::Pe.extensions(), &["exe", "dll"]);
    }

    #[test]
    fn zip_extensions_cover_the_common_container_formats() {
        let exts = FileType::Zip.extensions();
        for e in ["zip", "jar", "apk", "docx", "xlsx", "pptx", "odt", "ods", "odp", "epub"] {
            assert!(exts.contains(&e), "Zip::extensions() missing {e}");
        }
    }
}
