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
    Svg,
    /// EBML container — backs both Matroska (`.mkv`) and WebM (`.webm`); the two share the exact same
    /// `1A 45 DF A3` header and are not distinguishable without walking the EBML tree, so they are treated
    /// as one detected type covering both extensions (see [`FileType::extensions`]).
    Matroska,
    Avi,
    Sqlite,
    /// Compiled JVM `.class` file. Its `CA FE BA BE` header is also the magic number for a Mach-O
    /// "fat"/universal binary, so this signature is inherently ambiguous in isolation. We resolve in
    /// favour of Java class files: a class file is common in ordinary trees (build output, JARs unpacked
    /// for inspection), while a fat Mach-O is comparatively rare and, when present, is almost always inside
    /// an `.app` bundle rather than sitting around as a bare extensioned file. Telling the two apart for
    /// certain would need parsing the bytes that follow (Java's minor/major version pair vs. Mach-O's
    /// architecture count) — out of scope for a pure leading-signature sniff.
    JavaClass,
    Ico,
    /// TrueType-flavoured `sfnt` font (`.ttf`). Signature is `00 01 00 00` — note this is a different byte
    /// order to [`FileType::Ico`]'s `00 00 01 00`; the two are easy to transpose when hand-writing the byte
    /// arrays, so double-check against a hex dump if either signature is ever touched.
    TrueType,
    /// OpenType-flavoured `sfnt` font (`.otf`), tagged `OTTO`.
    OpenType,
    Woff,
    Woff2,
    Xz,
    Zstd,
    Bzip2,
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
            FileType::Svg => "SVG image",
            FileType::Matroska => "Matroska/WebM media",
            FileType::Avi => "AVI video",
            FileType::Sqlite => "SQLite database",
            FileType::JavaClass => "Java class file",
            FileType::Ico => "Windows icon",
            FileType::TrueType => "TrueType font",
            FileType::OpenType => "OpenType font",
            FileType::Woff => "WOFF font",
            FileType::Woff2 => "WOFF2 font",
            FileType::Xz => "xz archive",
            FileType::Zstd => "Zstd archive",
            FileType::Bzip2 => "Bzip2 archive",
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
            // The `<?xml`/`<svg` sniff can't tell a real SVG apart from any other well-formed XML document
            // that happens to lead with the same declaration — both are accepted here so a plain `.xml`
            // file is never false-flagged as a "renamed .svg" (mirrors the ZIP-container reasoning above).
            FileType::Svg => &["svg", "xml"],
            FileType::Matroska => &["webm", "mkv"],
            FileType::Avi => &["avi"],
            FileType::Sqlite => &["sqlite", "sqlite3", "db"],
            FileType::JavaClass => &["class"],
            FileType::Ico => &["ico"],
            FileType::TrueType => &["ttf"],
            FileType::OpenType => &["otf"],
            FileType::Woff => &["woff"],
            FileType::Woff2 => &["woff2"],
            FileType::Xz => &["xz"],
            FileType::Zstd => &["zst"],
            FileType::Bzip2 => &["bz2"],
        }
    }
}

/// True if `bytes` is at least `offset + pat.len()` long and `pat` matches at `offset`. Bounds-checked so
/// callers never slice out of range on short/empty input.
fn matches_at(bytes: &[u8], offset: usize, pat: &[u8]) -> bool {
    bytes.len() >= offset + pat.len() && &bytes[offset..offset + pat.len()] == pat
}

/// True if `bytes`, after skipping an optional UTF-8 BOM and any leading ASCII whitespace, starts with an
/// XML declaration (`<?xml`) or an inline `<svg` root tag — the two ways a real-world SVG file commonly
/// opens. Tolerant of the leading noise editors/exporters sometimes add so a BOM or a blank line doesn't
/// hide the signature.
fn looks_like_svg(bytes: &[u8]) -> bool {
    let mut s = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    while let Some(&b) = s.first() {
        if b.is_ascii_whitespace() {
            s = &s[1..];
        } else {
            break;
        }
    }
    s.starts_with(b"<?xml") || s.starts_with(b"<svg")
}

/// Sniff `bytes` for a recognised magic signature. Returns `None` for unknown or too-short input; never
/// panics regardless of length (including empty).
///
/// Order matters only where a shorter/less specific signature could otherwise shadow a longer one still to
/// come; the two-byte `MZ` (PE) check is placed last for exactly that reason. The `Ico`/`TrueType` pair and
/// the `Woff`/`Woff2` pair are full-length exact matches against different byte sequences, so despite
/// looking similar at a glance neither pair can shadow the other. The one genuine full-length collision —
/// `CA FE BA BE` meaning both a Java class file and a Mach-O fat binary — is resolved as documented on
/// [`FileType::JavaClass`]. Every other signature here is unambiguous relative to the rest of the set.
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
        if matches_at(bytes, 8, b"AVI ") {
            return Some(FileType::Avi);
        }
    }
    // EBML container header — shared verbatim by Matroska and WebM (see the doc comment on the variant).
    if matches_at(bytes, 0, &[0x1A, 0x45, 0xDF, 0xA3]) {
        return Some(FileType::Matroska);
    }
    if matches_at(bytes, 0, b"SQLite format 3\0") {
        return Some(FileType::Sqlite);
    }
    if matches_at(bytes, 0, &[0xCA, 0xFE, 0xBA, 0xBE]) {
        return Some(FileType::JavaClass);
    }
    // ICONDIR header: reserved=0, idType=1 (icon), little-endian — see the doc comment on `TrueType` for
    // how this is kept distinct from the sfnt version signature it superficially resembles.
    if matches_at(bytes, 0, &[0x00, 0x00, 0x01, 0x00]) {
        return Some(FileType::Ico);
    }
    if matches_at(bytes, 0, &[0x00, 0x01, 0x00, 0x00]) {
        return Some(FileType::TrueType);
    }
    if matches_at(bytes, 0, b"OTTO") {
        return Some(FileType::OpenType);
    }
    if matches_at(bytes, 0, b"wOFF") {
        return Some(FileType::Woff);
    }
    if matches_at(bytes, 0, b"wOF2") {
        return Some(FileType::Woff2);
    }
    if matches_at(bytes, 0, &[0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00]) {
        return Some(FileType::Xz);
    }
    if matches_at(bytes, 0, &[0x28, 0xB5, 0x2F, 0xFD]) {
        return Some(FileType::Zstd);
    }
    if matches_at(bytes, 0, b"BZh") {
        return Some(FileType::Bzip2);
    }
    if looks_like_svg(bytes) {
        return Some(FileType::Svg);
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
/// `None` when the type is unknown (nothing to compare against, so no verdict either way), when the
/// extension already matches, **and when there is no extension at all** (an empty/absent `ext` has nothing
/// to disagree with — a Linux ELF binary named `myapp` or a `README` must not be flagged).
pub fn mismatch(bytes: &[u8], ext: &str) -> Option<Mismatch> {
    let detected = detect_type(bytes)?;
    let actual_ext = ext.strip_prefix('.').unwrap_or(ext).to_lowercase();
    // No claimed extension → nothing to contradict (avoids a false positive on every extensionless file).
    if actual_ext.is_empty() {
        return None;
    }
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

    #[test]
    fn detects_avi_by_riff_plus_avi_tag_at_offset_8() {
        let mut bytes = b"RIFF".to_vec();
        bytes.extend_from_slice(&[0x00, 0x01, 0x00, 0x00]);
        bytes.extend_from_slice(b"AVI LIST");
        assert_eq!(detect_type(&bytes), Some(FileType::Avi));
    }

    #[test]
    fn detects_matroska_and_webm_via_shared_ebml_header() {
        // Matroska and WebM share the exact same EBML magic — the sniff can only report the container.
        assert_eq!(detect_type(&[0x1A, 0x45, 0xDF, 0xA3, 0x9F, 0x42]), Some(FileType::Matroska));
    }

    #[test]
    fn detects_sqlite() {
        let mut bytes = b"SQLite format 3\x00".to_vec();
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        assert_eq!(detect_type(&bytes), Some(FileType::Sqlite));
    }

    #[test]
    fn detects_java_class() {
        assert_eq!(
            detect_type(&[0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x3D]),
            Some(FileType::JavaClass)
        );
    }

    #[test]
    fn detects_ico() {
        assert_eq!(detect_type(&[0x00, 0x00, 0x01, 0x00, 0x01, 0x00]), Some(FileType::Ico));
    }

    #[test]
    fn detects_truetype_and_does_not_collide_with_ico() {
        // Same four bytes reordered — must resolve to the other type, not ICO.
        assert_eq!(detect_type(&[0x00, 0x01, 0x00, 0x00, 0x00, 0x0C]), Some(FileType::TrueType));
        assert_ne!(detect_type(&[0x00, 0x01, 0x00, 0x00]), detect_type(&[0x00, 0x00, 0x01, 0x00]));
    }

    #[test]
    fn detects_opentype() {
        assert_eq!(detect_type(b"OTTO\x00\x0C"), Some(FileType::OpenType));
    }

    #[test]
    fn detects_woff_and_woff2_distinctly() {
        assert_eq!(detect_type(b"wOFF\x00\x01"), Some(FileType::Woff));
        assert_eq!(detect_type(b"wOF2\x00\x01"), Some(FileType::Woff2));
    }

    #[test]
    fn detects_xz() {
        assert_eq!(
            detect_type(&[0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00, 0x00]),
            Some(FileType::Xz)
        );
    }

    #[test]
    fn detects_zstd() {
        assert_eq!(detect_type(&[0x28, 0xB5, 0x2F, 0xFD, 0x00]), Some(FileType::Zstd));
    }

    #[test]
    fn detects_bzip2() {
        assert_eq!(detect_type(b"BZh91AY"), Some(FileType::Bzip2));
    }

    #[test]
    fn detects_svg_via_xml_declaration_and_bare_svg_tag() {
        assert_eq!(detect_type(b"<?xml version=\"1.0\"?><svg/>"), Some(FileType::Svg));
        assert_eq!(detect_type(b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>"), Some(FileType::Svg));
    }

    #[test]
    fn detects_svg_tolerant_of_leading_bom_and_whitespace() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF]; // UTF-8 BOM
        bytes.extend_from_slice(b"\n\t  <?xml version=\"1.0\"?><svg/>");
        assert_eq!(detect_type(&bytes), Some(FileType::Svg));
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
    fn mismatch_is_none_for_svg_bytes_under_the_xml_container_extension() {
        // The `<?xml`/`<svg` sniff can't distinguish a real SVG from a plain XML document — `.xml` is
        // accepted as a non-mismatching extension for the detected type so a genuine XML file isn't
        // false-flagged as a "renamed .svg".
        let svg_bytes = b"<?xml version=\"1.0\"?><svg/>".to_vec();
        assert_eq!(mismatch(&svg_bytes, "svg"), None);
        assert_eq!(mismatch(&svg_bytes, "xml"), None);
        let m = mismatch(&svg_bytes, "png").expect("SVG bytes claiming .png must mismatch");
        assert_eq!(m.detected, FileType::Svg);
    }

    #[test]
    fn mismatch_distinguishes_ico_and_truetype_despite_similar_bytes() {
        let ico_bytes = vec![0x00, 0x00, 0x01, 0x00, 0x01, 0x00];
        let ttf_bytes = vec![0x00, 0x01, 0x00, 0x00, 0x00, 0x0C];
        assert_eq!(mismatch(&ico_bytes, "ico"), None);
        assert_eq!(mismatch(&ttf_bytes, "ttf"), None);
        let m = mismatch(&ico_bytes, "ttf").expect("ICO bytes claiming .ttf must mismatch");
        assert_eq!(m.detected, FileType::Ico);
        let m = mismatch(&ttf_bytes, "ico").expect("TrueType bytes claiming .ico must mismatch");
        assert_eq!(m.detected, FileType::TrueType);
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

    #[test]
    fn mismatch_is_none_for_extensionless_files() {
        // A DETECTED file with no claimed extension must not be flagged — a Linux ELF binary named `myapp`,
        // a PDF named `report`, a screenshot named with no extension. There is nothing to disagree with.
        // (These returned Some before the empty-ext guard — this is the regression test for that fix.)
        assert_eq!(mismatch(&pe_bytes(), ""), None);
        assert_eq!(mismatch(&[0x7F, 0x45, 0x4C, 0x46, 0x02, 0x01, 0x01, 0x00], ""), None); // ELF binary
        assert_eq!(mismatch(&jpeg_bytes(), ""), None);
        // A leading-dot-only ext normalises to empty as well → still no verdict.
        assert_eq!(mismatch(&pe_bytes(), "."), None);
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

    #[test]
    fn label_and_extensions_for_new_signature_formats() {
        assert_eq!(FileType::Svg.label(), "SVG image");
        assert_eq!(FileType::Svg.extensions(), &["svg", "xml"]);
        assert_eq!(FileType::Matroska.label(), "Matroska/WebM media");
        assert_eq!(FileType::Matroska.extensions(), &["webm", "mkv"]);
        assert_eq!(FileType::Avi.label(), "AVI video");
        assert_eq!(FileType::Avi.extensions(), &["avi"]);
        assert_eq!(FileType::Sqlite.label(), "SQLite database");
        assert_eq!(FileType::Sqlite.extensions(), &["sqlite", "sqlite3", "db"]);
        assert_eq!(FileType::JavaClass.label(), "Java class file");
        assert_eq!(FileType::JavaClass.extensions(), &["class"]);
        assert_eq!(FileType::Ico.label(), "Windows icon");
        assert_eq!(FileType::Ico.extensions(), &["ico"]);
        assert_eq!(FileType::TrueType.label(), "TrueType font");
        assert_eq!(FileType::TrueType.extensions(), &["ttf"]);
        assert_eq!(FileType::OpenType.label(), "OpenType font");
        assert_eq!(FileType::OpenType.extensions(), &["otf"]);
        assert_eq!(FileType::Woff.label(), "WOFF font");
        assert_eq!(FileType::Woff.extensions(), &["woff"]);
        assert_eq!(FileType::Woff2.label(), "WOFF2 font");
        assert_eq!(FileType::Woff2.extensions(), &["woff2"]);
        assert_eq!(FileType::Xz.label(), "xz archive");
        assert_eq!(FileType::Xz.extensions(), &["xz"]);
        assert_eq!(FileType::Zstd.label(), "Zstd archive");
        assert_eq!(FileType::Zstd.extensions(), &["zst"]);
        assert_eq!(FileType::Bzip2.label(), "Bzip2 archive");
        assert_eq!(FileType::Bzip2.extensions(), &["bz2"]);
    }
}
