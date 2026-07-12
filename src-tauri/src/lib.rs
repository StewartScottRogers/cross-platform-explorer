use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize)]
pub struct DirEntry {
    name: String,
    path: String,
    is_dir: bool,
    size: u64,
    /// Last-modified time as milliseconds since the Unix epoch.
    /// `None` when the platform or filesystem does not report one.
    modified: Option<u64>,
    /// Lowercased file extension without the dot ("png"), empty for directories
    /// and extensionless files.
    extension: String,
    /// Hidden per the OS convention: the hidden attribute on Windows, a leading
    /// dot on POSIX.
    hidden: bool,
}

/// Per-item outcome of a bulk operation. Bulk file operations must NOT be
/// all-or-nothing and must not abort on the first failure: if 9 of 10 files
/// copy and one is locked, the user needs to know exactly which one failed.
#[derive(Serialize)]
pub struct OpResult {
    path: String,
    ok: bool,
    error: String,
}

impl OpResult {
    fn ok(path: &Path) -> Self {
        Self {
            path: path.to_string_lossy().to_string(),
            ok: true,
            error: String::new(),
        }
    }
    fn err(path: &Path, e: impl std::fmt::Display) -> Self {
        Self {
            path: path.to_string_lossy().to_string(),
            ok: false,
            error: e.to_string(),
        }
    }
}

/// Detailed metadata for the Properties dialog.
#[derive(Serialize)]
pub struct EntryInfo {
    name: String,
    path: String,
    is_dir: bool,
    size: u64,
    modified: Option<u64>,
    created: Option<u64>,
    readonly: bool,
    hidden: bool,
}

#[derive(Serialize)]
pub struct Place {
    /// Display name, e.g. "Documents" or "Local Disk (C:)".
    name: String,
    path: String,
    /// Logical kind, used by the UI to pick an icon:
    /// "desktop" | "documents" | "downloads" | "pictures" | "music" | "videos" | "drive" | "home".
    kind: String,
}

/// Convert a `SystemTime` into epoch milliseconds, if representable.
fn to_epoch_ms(t: SystemTime) -> Option<u64> {
    t.duration_since(UNIX_EPOCH).ok().map(|d| d.as_millis() as u64)
}

/// Lowercased extension without the dot; empty when there is none.
fn extension_of(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default()
}

/// Hidden per OS convention: the FILE_ATTRIBUTE_HIDDEN bit on Windows,
/// a leading dot on POSIX.
fn is_hidden(path: &Path, meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
        if meta.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0 {
            return true;
        }
    }
    #[cfg(not(windows))]
    {
        let _ = meta;
    }
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with('.'))
        .unwrap_or(false)
}

/// Would moving/copying `src` into `dest` put a directory inside itself?
/// Copying a folder into its own descendant recurses forever and shreds data —
/// this must be impossible, not merely discouraged.
fn is_self_or_descendant(src: &Path, dest: &Path) -> bool {
    let src = src.canonicalize();
    let dest = dest.canonicalize();
    match (src, dest) {
        (Ok(s), Ok(d)) => d == s || d.starts_with(&s),
        // If either path can't be canonicalized we cannot prove it is safe,
        // so refuse rather than risk it.
        _ => false,
    }
}

/// Pick a non-colliding name in `dir`, Explorer-style:
/// "report.txt" -> "report - Copy.txt" -> "report - Copy (2).txt".
/// We never overwrite an existing file — silent overwrite is data loss.
fn unique_target(dir: &Path, file_name: &str) -> PathBuf {
    let candidate = dir.join(file_name);
    if !candidate.exists() {
        return candidate;
    }

    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(file_name);
    let ext = path.extension().and_then(|e| e.to_str());

    let build = |suffix: &str| -> PathBuf {
        let name = match ext {
            Some(e) => format!("{stem}{suffix}.{e}"),
            None => format!("{stem}{suffix}"),
        };
        dir.join(name)
    };

    let first = build(" - Copy");
    if !first.exists() {
        return first;
    }
    for n in 2..10_000 {
        let p = build(&format!(" - Copy ({n})"));
        if !p.exists() {
            return p;
        }
    }
    // Pathological fallback; effectively unreachable.
    dir.join(format!("{file_name}.{}", std::process::id()))
}

/// Recursively copy a directory tree.
fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// List the immediate children of `path`.
#[tauri::command]
fn list_dir(path: String) -> Result<Vec<DirEntry>, String> {
    let mut out = Vec::new();
    let read = fs::read_dir(&path).map_err(|e| format!("{path}: {e}"))?;
    for entry in read {
        // Skip entries we can't read rather than failing the whole listing.
        let Ok(entry) = entry else { continue };
        let Ok(meta) = entry.metadata() else { continue };

        let entry_path = entry.path();
        let is_dir = meta.is_dir();

        out.push(DirEntry {
            hidden: is_hidden(&entry_path, &meta),
            name: entry.file_name().to_string_lossy().to_string(),
            path: entry_path.to_string_lossy().to_string(),
            is_dir,
            size: if is_dir { 0 } else { meta.len() },
            modified: meta.modified().ok().and_then(to_epoch_ms),
            extension: if is_dir {
                String::new()
            } else {
                extension_of(&entry_path)
            },
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Mutating file operations (CPE-030)
//
// Safety rules that these all obey:
//   * Delete goes to the OS Recycle Bin / Trash. Permanent delete is a separate,
//     explicitly-requested command.
//   * Nothing is ever silently overwritten. Collisions either error (rename,
//     create) or auto-rename (paste), never clobber.
//   * A directory can never be copied or moved into itself or a descendant.
//   * Bulk operations report per-item results rather than aborting on the first
//     failure.
// ---------------------------------------------------------------------------

/// Create a new directory `name` inside `path`.
#[tauri::command]
fn create_dir(path: String, name: String) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Name cannot be empty".to_string());
    }
    let target = Path::new(&path).join(name);
    if target.exists() {
        return Err(format!("\"{name}\" already exists"));
    }
    fs::create_dir(&target).map_err(|e| e.to_string())?;
    Ok(target.to_string_lossy().to_string())
}

/// Write UTF-8 text back to a file, replacing its contents — for the content
/// editor. Returns the new byte length.
#[tauri::command]
fn write_file_text(path: String, contents: String) -> Result<u64, String> {
    fs::write(&path, contents.as_bytes()).map_err(|e| e.to_string())?;
    Ok(contents.len() as u64)
}

/// One entry inside an archive, for the archive preview.
#[derive(Serialize)]
pub struct ArchiveEntry {
    name: String,
    size: u64,
    is_dir: bool,
}

/// List the entries of a ZIP archive without extracting it.
fn zip_entries(path: &str) -> Result<Vec<ArchiveEntry>, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(zip.len());
    for i in 0..zip.len() {
        let entry = zip.by_index(i).map_err(|e| e.to_string())?;
        out.push(ArchiveEntry {
            name: entry.name().to_string(),
            size: entry.size(),
            is_dir: entry.is_dir(),
        });
    }
    Ok(out)
}

/// List the entries of a TAR stream (optionally gzip-decompressed by the caller).
fn tar_entries<R: std::io::Read>(reader: R) -> Result<Vec<ArchiveEntry>, String> {
    let mut archive = tar::Archive::new(reader);
    let mut out = Vec::new();
    for entry in archive.entries().map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let header = entry.header();
        let is_dir = header.entry_type().is_dir();
        let size = header.size().unwrap_or(0);
        let name = entry
            .path()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        out.push(ArchiveEntry { name, size, is_dir });
    }
    Ok(out)
}

/// A single-file gzip (not a .tar.gz) has no directory. Report the decompressed
/// file as one entry: its name is the archive name minus `.gz`, and its size is
/// the gzip trailer's ISIZE (uncompressed length modulo 2^32).
fn gzip_single_entry(path: &str) -> Result<Vec<ArchiveEntry>, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let name = Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let size = if bytes.len() >= 4 {
        let n = bytes.len();
        u32::from_le_bytes([bytes[n - 4], bytes[n - 3], bytes[n - 2], bytes[n - 1]]) as u64
    } else {
        0
    };
    Ok(vec![ArchiveEntry {
        name,
        size,
        is_dir: false,
    }])
}

/// List an archive's entries without extracting it, for the preview pane.
/// Dispatches by extension: ZIP family (zip/jar/apk/war/ear/ipa/xpi), TAR,
/// gzip-compressed TAR (.tar.gz/.tgz), and single-file gzip (.gz). Reads only
/// the archive directory, so it stays cheap even for large archives.
#[tauri::command]
fn read_archive_entries(path: String) -> Result<Vec<ArchiveEntry>, String> {
    let lower = path.to_lowercase();
    if lower.ends_with(".tar") {
        let file = fs::File::open(&path).map_err(|e| e.to_string())?;
        tar_entries(file)
    } else if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        let file = fs::File::open(&path).map_err(|e| e.to_string())?;
        tar_entries(flate2::read::GzDecoder::new(file))
    } else if lower.ends_with(".gz") {
        gzip_single_entry(&path)
    } else {
        zip_entries(&path)
    }
}

// ---------------------------------------------------------------------------
// Structured binary previews (CPE-210/214/215/216/218)
//
// `read_preview_info` returns a human-readable text summary of a binary file,
// dispatched by extension. The frontend renders it read-only in the preview
// pane (the "info" provider kind). Each helper reads the file itself, so a
// corrupt file yields an Err (the pane shows a "can't preview" note) rather than
// hanging.
// ---------------------------------------------------------------------------

/// Classic hex + ASCII dump of the first `max` bytes (CPE-214).
fn hex_dump(path: &str, max: usize) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let n = bytes.len().min(max);
    let mut out = String::new();
    for (i, chunk) in bytes[..n].chunks(16).enumerate() {
        let mut hex = String::new();
        let mut ascii = String::new();
        for (j, b) in chunk.iter().enumerate() {
            hex.push_str(&format!("{b:02x} "));
            if j == 7 {
                hex.push(' ');
            }
            ascii.push(if b.is_ascii_graphic() || *b == b' ' {
                *b as char
            } else {
                '.'
            });
        }
        out.push_str(&format!("{:08x}  {hex:<49}|{ascii}|\n", i * 16));
    }
    if bytes.len() > n {
        out.push_str(&format!(
            "\n… {} more bytes (showing first {n}).\n",
            bytes.len() - n
        ));
    }
    Ok(out)
}

/// Summary of a Windows PE image (EXE/DLL) via goblin (CPE-216).
fn pe_info(path: &str) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let pe = goblin::pe::PE::parse(&bytes).map_err(|e| e.to_string())?;
    let mut out = String::new();
    out.push_str(if pe.is_64 {
        "PE32+ image (64-bit)\n"
    } else {
        "PE32 image (32-bit)\n"
    });
    out.push_str(&format!(
        "Machine: 0x{:04x}\n",
        pe.header.coff_header.machine
    ));
    out.push_str(&format!("Entry point: 0x{:x}\n", pe.entry));
    out.push_str(&format!("Sections: {}\n", pe.sections.len()));
    out.push_str(&format!(
        "Imports: {} symbols from {} libraries\n",
        pe.imports.len(),
        pe.libraries.len()
    ));
    out.push_str("\nSections:\n");
    for s in &pe.sections {
        let name = String::from_utf8_lossy(&s.name);
        out.push_str(&format!(
            "  {:<9} vaddr 0x{:08x}  vsize {}\n",
            name.trim_end_matches('\0'),
            s.virtual_address,
            s.virtual_size
        ));
    }
    if !pe.libraries.is_empty() {
        out.push_str("\nLinked libraries:\n");
        for lib in &pe.libraries {
            out.push_str(&format!("  {lib}\n"));
        }
    }
    Ok(out)
}

/// Summary of a MIDI file via midly (CPE-210).
fn midi_info(path: &str) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let smf = midly::Smf::parse(&bytes).map_err(|e| e.to_string())?;
    let mut out = String::new();
    out.push_str(&format!("MIDI format: {:?}\n", smf.header.format));
    out.push_str(&format!("Timing: {:?}\n", smf.header.timing));
    out.push_str(&format!("Tracks: {}\n", smf.tracks.len()));
    let total: usize = smf.tracks.iter().map(|t| t.len()).sum();
    out.push_str(&format!("Total events: {total}\n\n"));
    for (i, t) in smf.tracks.iter().enumerate() {
        let mut name = String::new();
        for ev in t.iter() {
            if let midly::TrackEventKind::Meta(midly::MetaMessage::TrackName(n)) = ev.kind {
                name = String::from_utf8_lossy(n).to_string();
                break;
            }
        }
        let suffix = if name.is_empty() {
            String::new()
        } else {
            format!(" — {name}")
        };
        out.push_str(&format!("  Track {}: {} events{suffix}\n", i + 1, t.len()));
    }
    Ok(out)
}

/// Disassemble a WebAssembly binary to its text form (WAT) via wasmprinter,
/// capped so a huge module can't flood the pane (CPE-215).
fn wasm_info(path: &str, max: usize) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let wat = wasmprinter::print_bytes(&bytes).map_err(|e| e.to_string())?;
    if wat.len() > max {
        let mut cut = max;
        while cut > 0 && !wat.is_char_boundary(cut) {
            cut -= 1;
        }
        Ok(format!(
            "{}\n\n… truncated ({cut} of {} bytes shown).\n",
            &wat[..cut],
            wat.len()
        ))
    } else {
        Ok(wat)
    }
}

/// Summary of a .torrent's bencode metadata via serde_bencode (CPE-218).
fn torrent_info(path: &str) -> Result<String, String> {
    use serde_bencode::value::Value;
    use std::collections::HashMap;

    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let val: Value = serde_bencode::from_bytes(&bytes).map_err(|e| e.to_string())?;
    let Value::Dict(top) = &val else {
        return Err("Not a bencode dictionary".to_string());
    };
    let get = |d: &'_ HashMap<Vec<u8>, Value>, k: &str| -> Option<Value> { d.get(k.as_bytes()).cloned() };
    let as_str = |v: &Value| match v {
        Value::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
        _ => String::new(),
    };
    let as_int = |v: &Value| match v {
        Value::Int(i) => *i,
        _ => 0,
    };

    let mut out = String::new();
    if let Some(a) = get(top, "announce") {
        out.push_str(&format!("Announce: {}\n", as_str(&a)));
    }
    if let Some(Value::Dict(info)) = get(top, "info") {
        if let Some(n) = get(&info, "name") {
            out.push_str(&format!("Name: {}\n", as_str(&n)));
        }
        if let Some(pl) = get(&info, "piece length") {
            out.push_str(&format!("Piece length: {} bytes\n", as_int(&pl)));
        }
        match get(&info, "files") {
            Some(Value::List(files)) => {
                out.push_str(&format!("Files: {}\n", files.len()));
                let mut total = 0i64;
                for f in &files {
                    if let Value::Dict(fd) = f {
                        let len = get(fd, "length").map(|v| as_int(&v)).unwrap_or(0);
                        total += len;
                        let parts = match get(fd, "path") {
                            Some(Value::List(ps)) => {
                                ps.iter().map(as_str).collect::<Vec<_>>().join("/")
                            }
                            _ => String::new(),
                        };
                        out.push_str(&format!("  {parts} ({len} bytes)\n"));
                    }
                }
                out.push_str(&format!("Total size: {total} bytes\n"));
            }
            _ => {
                if let Some(l) = get(&info, "length") {
                    out.push_str(&format!("Size: {} bytes (single file)\n", as_int(&l)));
                }
            }
        }
    }
    Ok(out)
}

// ---- Document text extraction (CPE-070/071/072/077) ----

/// Decode the five predefined XML entities. Applied after tag stripping.
fn decode_xml_entities(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Strip XML/HTML tags to plain text, turning the given block/paragraph tags'
/// closing tags into newlines first. Good enough for a readable text preview of
/// office and ebook markup — not a full renderer.
fn strip_markup_to_text(markup: &str, para_tags: &[&str]) -> String {
    let mut s = markup.to_string();
    for t in para_tags {
        s = s.replace(&format!("</{t}>"), "\n");
    }
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    collapse_blank_lines(&decode_xml_entities(&out))
}

/// Collapse runs of 3+ newlines into 2 and trim, so stripped markup reads cleanly.
fn collapse_blank_lines(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut newlines = 0;
    for c in s.chars() {
        if c == '\n' {
            newlines += 1;
            if newlines <= 2 {
                out.push('\n');
            }
        } else if c == '\r' {
            // ignore
        } else {
            newlines = 0;
            out.push(c);
        }
    }
    out.trim().to_string()
}

/// Read one entry of a zip as UTF-8 text.
fn zip_read_text(path: &str, entry_name: &str) -> Result<String, String> {
    use std::io::Read;
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut entry = zip
        .by_name(entry_name)
        .map_err(|e| format!("{entry_name}: {e}"))?;
    let mut buf = String::new();
    entry.read_to_string(&mut buf).map_err(|e| e.to_string())?;
    Ok(buf)
}

/// Extract the body text of a DOCX (word/document.xml) (CPE-071).
fn docx_text(path: &str) -> Result<String, String> {
    let xml = zip_read_text(path, "word/document.xml")?;
    Ok(strip_markup_to_text(&xml, &["w:p"]))
}

/// Extract the body text of an ODT (content.xml) (CPE-072).
fn odt_text(path: &str) -> Result<String, String> {
    let xml = zip_read_text(path, "content.xml")?;
    Ok(strip_markup_to_text(&xml, &["text:p", "text:h"]))
}

/// Extract readable text from an EPUB's content documents in name order,
/// capped so a whole book can't flood the pane (CPE-077).
fn epub_text(path: &str) -> Result<String, String> {
    use std::io::Read;
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;

    let mut names: Vec<String> = Vec::new();
    for i in 0..zip.len() {
        if let Ok(entry) = zip.by_index(i) {
            let n = entry.name().to_string();
            let low = n.to_lowercase();
            if low.ends_with(".xhtml") || low.ends_with(".html") || low.ends_with(".htm") {
                names.push(n);
            }
        }
    }
    names.sort();

    let mut out = format!("EPUB — {} content document(s)\n\n", names.len());
    for n in &names {
        if out.len() > 128 * 1024 {
            out.push_str("\n… (truncated)\n");
            break;
        }
        if let Ok(mut entry) = zip.by_name(n) {
            let mut buf = String::new();
            if entry.read_to_string(&mut buf).is_ok() {
                let text = strip_markup_to_text(&buf, &["p", "h1", "h2", "h3", "h4", "div", "li", "br"]);
                if !text.trim().is_empty() {
                    out.push_str(text.trim());
                    out.push_str("\n\n");
                }
            }
        }
    }
    Ok(out)
}

/// Extract readable text from RTF: a small, dependency-free reader that drops
/// control words and the font/colour/style/info destinations, turning \par and
/// friends into newlines. Not a full RTF engine — enough for a text preview
/// (CPE-070).
fn rtf_text(path: &str) -> Result<String, String> {
    let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let bytes = raw.as_bytes();
    let mut out = String::new();
    let mut i = 0usize;
    let mut depth: i32 = 0;
    let mut skip_depth: i32 = -1; // depth of a destination group being skipped

    while i < bytes.len() {
        match bytes[i] {
            b'{' => {
                depth += 1;
                i += 1;
            }
            b'}' => {
                if skip_depth >= 0 && depth == skip_depth {
                    skip_depth = -1;
                }
                depth -= 1;
                i += 1;
            }
            b'\\' => {
                i += 1;
                if i >= bytes.len() {
                    break;
                }
                let n = bytes[i];
                if n == b'\'' && i + 2 < bytes.len() {
                    if skip_depth < 0 {
                        if let Ok(v) = u8::from_str_radix(&raw[i + 1..i + 3], 16) {
                            out.push(v as char);
                        }
                    }
                    i += 3;
                } else if n.is_ascii_alphabetic() {
                    let start = i;
                    while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                        i += 1;
                    }
                    let word = &raw[start..i];
                    // optional numeric parameter
                    if i < bytes.len() && (bytes[i] == b'-' || bytes[i].is_ascii_digit()) {
                        if bytes[i] == b'-' {
                            i += 1;
                        }
                        while i < bytes.len() && bytes[i].is_ascii_digit() {
                            i += 1;
                        }
                    }
                    // a single trailing space is part of the control word
                    if i < bytes.len() && bytes[i] == b' ' {
                        i += 1;
                    }
                    if skip_depth < 0 {
                        match word {
                            "par" | "line" | "sect" => out.push('\n'),
                            "tab" => out.push('\t'),
                            "fonttbl" | "colortbl" | "stylesheet" | "info" | "pict" | "object"
                            | "header" | "footer" | "generator" => skip_depth = depth,
                            _ => {}
                        }
                    }
                } else {
                    if skip_depth < 0 {
                        match n {
                            b'\\' | b'{' | b'}' => out.push(n as char),
                            b'~' => out.push(' '),
                            _ => {}
                        }
                    }
                    i += 1;
                }
            }
            b'\r' | b'\n' => i += 1,
            c => {
                if skip_depth < 0 && depth > 0 {
                    out.push(c as char);
                }
                i += 1;
            }
        }
    }
    Ok(collapse_blank_lines(&out))
}

/// Return a human-readable text summary of a binary file, dispatched by
/// extension. Rendered read-only by the preview pane's "info" provider.
#[tauri::command]
fn read_preview_info(path: String) -> Result<String, String> {
    let ext = extension_of(Path::new(&path));
    match ext.as_str() {
        "exe" | "dll" | "sys" | "efi" | "ocx" | "scr" | "cpl" => pe_info(&path),
        "torrent" => torrent_info(&path),
        "wasm" => wasm_info(&path, 256 * 1024),
        "mid" | "midi" => midi_info(&path),
        "rtf" => rtf_text(&path),
        "docx" => docx_text(&path),
        "odt" => odt_text(&path),
        "epub" => epub_text(&path),
        // generic binary (.bin/.dat) and anything else routed here: hex dump
        _ => hex_dump(&path, 64 * 1024),
    }
}

/// Read a text file's contents for the preview pane, capped at `max_bytes` so a
/// huge file can never be slurped into memory. Errors (rather than truncating)
/// when the file is too large, unreadable, or not valid UTF-8 — the frontend
/// shows a "can't preview" state in that case.
#[tauri::command]
fn read_file_text(path: String, max_bytes: u64) -> Result<String, String> {
    let p = Path::new(&path);
    let meta = fs::metadata(p).map_err(|e| e.to_string())?;
    if meta.len() > max_bytes {
        return Err(format!(
            "File is too large to preview ({} bytes; limit {max_bytes}).",
            meta.len()
        ));
    }
    let bytes = fs::read(p).map_err(|e| e.to_string())?;
    String::from_utf8(bytes).map_err(|_| "File is not valid UTF-8 text.".to_string())
}

/// Rename a single entry in place. Returns the new path.
#[tauri::command]
fn rename_entry(path: String, new_name: String) -> Result<String, String> {
    let new_name = new_name.trim();
    if new_name.is_empty() {
        return Err("Name cannot be empty".to_string());
    }
    let src = Path::new(&path);
    let parent = src
        .parent()
        .ok_or_else(|| "Cannot rename a filesystem root".to_string())?;
    let target = parent.join(new_name);

    if target == src {
        return Ok(path.clone()); // no-op rename
    }
    if target.exists() {
        return Err(format!("\"{new_name}\" already exists"));
    }
    fs::rename(src, &target).map_err(|e| e.to_string())?;
    Ok(target.to_string_lossy().to_string())
}

/// Move entries to the OS Recycle Bin / Trash. Recoverable by the user.
#[tauri::command]
fn delete_to_trash(paths: Vec<String>) -> Vec<OpResult> {
    paths
        .iter()
        .map(|p| {
            let path = Path::new(p);
            match trash::delete(path) {
                Ok(()) => OpResult::ok(path),
                Err(e) => OpResult::err(path, e),
            }
        })
        .collect()
}

/// Can this platform restore items from the OS trash?
///
/// `trash::os_limited` (list + restore) is implemented on Windows and Linux but
/// NOT on macOS. The UI calls this so it can decide whether to offer undo-of-
/// delete at all. Offering an Undo that silently does nothing on one platform is
/// worse than not offering it — so we tell the truth instead of guessing.
#[tauri::command]
fn can_restore_from_trash() -> bool {
    cfg!(any(target_os = "windows", target_os = "linux"))
}

/// Restore previously-trashed items to their original paths.
#[cfg(any(target_os = "windows", target_os = "linux"))]
#[tauri::command]
fn restore_from_trash(paths: Vec<String>) -> Vec<OpResult> {
    use trash::os_limited::{list, restore_all};

    let all = match list() {
        Ok(v) => v,
        Err(e) => {
            return paths
                .iter()
                .map(|p| OpResult::err(Path::new(p), &e))
                .collect()
        }
    };

    let mut results = Vec::new();
    let mut to_restore = Vec::new();

    for p in &paths {
        let target = Path::new(p);

        // Never clobber: if something now occupies the original path, refuse
        // rather than overwrite it to satisfy an undo.
        if target.exists() {
            results.push(OpResult::err(
                target,
                "Something already exists at the original location",
            ));
            continue;
        }

        // Match the trashed item by the full path it was deleted from.
        let found = all
            .iter()
            .find(|item| item.original_parent.join(&item.name) == target);

        match found {
            Some(item) => {
                to_restore.push(item.clone());
                results.push(OpResult::ok(target));
            }
            None => results.push(OpResult::err(
                target,
                "Not found in the Recycle Bin — it may have been emptied",
            )),
        }
    }

    if !to_restore.is_empty() {
        if let Err(e) = restore_all(to_restore) {
            // The restore failed as a batch; report it against every item we
            // had intended to restore rather than falsely claiming success.
            return paths
                .iter()
                .map(|p| OpResult::err(Path::new(p), &e))
                .collect();
        }
    }

    results
}

/// macOS has no trash listing/restore API in the `trash` crate. Rather than
/// pretend, this returns a clear error — and the UI never reaches here, because
/// `can_restore_from_trash()` is false so delete is never pushed onto the undo
/// stack in the first place.
#[cfg(not(any(target_os = "windows", target_os = "linux")))]
#[tauri::command]
fn restore_from_trash(paths: Vec<String>) -> Vec<OpResult> {
    paths
        .iter()
        .map(|p| {
            OpResult::err(
                Path::new(p),
                "Restoring from the Trash isn't supported on this platform — open the Trash to recover it",
            )
        })
        .collect()
}

/// Permanently delete entries. Irreversible — the UI must confirm explicitly
/// before ever calling this.
#[tauri::command]
fn delete_permanent(paths: Vec<String>) -> Vec<OpResult> {
    paths
        .iter()
        .map(|p| {
            let path = Path::new(p);
            let result = if path.is_dir() {
                fs::remove_dir_all(path)
            } else {
                fs::remove_file(path)
            };
            match result {
                Ok(()) => OpResult::ok(path),
                Err(e) => OpResult::err(path, e),
            }
        })
        .collect()
}

/// Copy entries into `dest`, auto-renaming on collision.
#[tauri::command]
fn copy_entries(paths: Vec<String>, dest: String) -> Vec<OpResult> {
    let dest_dir = PathBuf::from(&dest);
    paths
        .iter()
        .map(|p| {
            let src = Path::new(p);
            let Some(file_name) = src.file_name().and_then(|n| n.to_str()) else {
                return OpResult::err(src, "Invalid file name");
            };
            if src.is_dir() && is_self_or_descendant(src, &dest_dir) {
                return OpResult::err(src, "Cannot copy a folder into itself");
            }
            let target = unique_target(&dest_dir, file_name);
            let result = if src.is_dir() {
                copy_dir_all(src, &target)
            } else {
                fs::copy(src, &target).map(|_| ())
            };
            match result {
                Ok(()) => OpResult::ok(&target),
                Err(e) => OpResult::err(src, e),
            }
        })
        .collect()
}

/// Move entries into `dest`, auto-renaming on collision. Falls back to
/// copy-then-delete when the move crosses a filesystem boundary (`fs::rename`
/// fails across volumes, e.g. C: -> Z:).
#[tauri::command]
fn move_entries(paths: Vec<String>, dest: String) -> Vec<OpResult> {
    let dest_dir = PathBuf::from(&dest);
    paths
        .iter()
        .map(|p| {
            let src = Path::new(p);
            let Some(file_name) = src.file_name().and_then(|n| n.to_str()) else {
                return OpResult::err(src, "Invalid file name");
            };
            if src.is_dir() && is_self_or_descendant(src, &dest_dir) {
                return OpResult::err(src, "Cannot move a folder into itself");
            }
            let target = unique_target(&dest_dir, file_name);

            if fs::rename(src, &target).is_ok() {
                return OpResult::ok(&target);
            }

            // Cross-volume move: copy, then remove the original only if the
            // copy fully succeeded. Never delete the source on a failed copy.
            let copied = if src.is_dir() {
                copy_dir_all(src, &target)
            } else {
                fs::copy(src, &target).map(|_| ())
            };
            match copied {
                Ok(()) => {
                    let removed = if src.is_dir() {
                        fs::remove_dir_all(src)
                    } else {
                        fs::remove_file(src)
                    };
                    match removed {
                        Ok(()) => OpResult::ok(&target),
                        Err(e) => OpResult::err(src, format!("Copied, but could not remove original: {e}")),
                    }
                }
                Err(e) => OpResult::err(src, e),
            }
        })
        .collect()
}

/// Move each `from` to an EXACT `to` path. Used by undo, which must restore an
/// item to its original name — auto-renaming here would defeat the point (undo
/// of "rename a -> b" must produce "a", not "a - Copy").
///
/// Refuses to overwrite: if `to` already exists, the undo fails loudly rather
/// than clobbering whatever now occupies that name.
#[tauri::command]
fn move_exact(pairs: Vec<(String, String)>) -> Vec<OpResult> {
    pairs
        .iter()
        .map(|(from, to)| {
            let src = Path::new(from);
            let dst = Path::new(to);
            if dst.exists() {
                return OpResult::err(
                    src,
                    format!(
                        "\"{}\" already exists",
                        dst.file_name().unwrap_or_default().to_string_lossy()
                    ),
                );
            }
            if let Some(parent) = dst.parent() {
                if !parent.exists() {
                    return OpResult::err(src, "The original folder no longer exists");
                }
            }
            match fs::rename(src, dst) {
                Ok(()) => OpResult::ok(dst),
                Err(e) => OpResult::err(src, e),
            }
        })
        .collect()
}

/// Detailed metadata for the Properties dialog.
#[tauri::command]
fn entry_info(path: String) -> Result<EntryInfo, String> {
    let p = Path::new(&path);
    let meta = fs::metadata(p).map_err(|e| format!("{path}: {e}"))?;
    Ok(EntryInfo {
        name: p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.clone()),
        path: path.clone(),
        is_dir: meta.is_dir(),
        size: if meta.is_dir() { 0 } else { meta.len() },
        modified: meta.modified().ok().and_then(to_epoch_ms),
        created: meta.created().ok().and_then(to_epoch_ms),
        readonly: meta.permissions().readonly(),
        hidden: is_hidden(p, &meta),
    })
}

/// Total size of a directory tree. Unreadable subtrees are skipped rather than
/// failing the whole calculation.
#[tauri::command]
fn dir_size(path: String) -> Result<u64, String> {
    fn walk(p: &Path) -> u64 {
        let Ok(read) = fs::read_dir(p) else { return 0 };
        let mut total = 0u64;
        for entry in read.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                total += walk(&entry.path());
            } else {
                total += meta.len();
            }
        }
        total
    }
    let p = Path::new(&path);
    if !p.exists() {
        return Err(format!("{path}: not found"));
    }
    Ok(walk(p))
}

/// Return the user's home directory.
#[tauri::command]
fn home_dir() -> Result<String, String> {
    dirs_home()
        .map(|p| p.to_string_lossy().to_string())
        .ok_or_else(|| "could not determine home directory".to_string())
}

/// Return the parent of `path`, or null if already at a root.
#[tauri::command]
fn parent_dir(path: String) -> Option<String> {
    Path::new(&path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
}

/// Available drives (Windows) or filesystem roots (Unix).
#[tauri::command]
fn list_drives() -> Vec<Place> {
    let mut drives = Vec::new();

    #[cfg(target_os = "windows")]
    {
        for letter in b'A'..=b'Z' {
            let root = format!("{}:\\", letter as char);
            if Path::new(&root).exists() {
                drives.push(Place {
                    name: format!("Local Disk ({}:)", letter as char),
                    path: root,
                    kind: "drive".to_string(),
                });
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        drives.push(Place {
            name: "File System".to_string(),
            path: "/".to_string(),
            kind: "drive".to_string(),
        });
    }

    drives
}

/// On Windows, look up a known folder's REAL location in the registry.
///
/// Windows "Known Folder redirection" lets OneDrive move Desktop, Documents,
/// Pictures, etc. anywhere at all. On a real machine Pictures resolved to
/// `C:\Users\<user>\OneDrive\Exteriors Cave Homes\Pictures` — a path no
/// `%USERPROFILE%\Pictures` or `%OneDrive%\Pictures` heuristic could ever guess.
/// Worse, Windows often leaves an empty stub at `%USERPROFILE%\Desktop`, so
/// probing the profile first returns the *wrong* folder rather than none.
///
/// `Shell Folders` holds fully-expanded paths (`User Shell Folders` holds
/// unexpanded `%USERPROFILE%` tokens), so we read the former.
///
/// `registry_name` is the value name, which is NOT the display name:
/// Documents is "Personal", Pictures is "My Pictures", Downloads is a GUID.
#[cfg(windows)]
fn known_folder_from_registry(registry_name: &str) -> Option<PathBuf> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Explorer\Shell Folders")
        .ok()?;
    let value: String = key.get_value(registry_name).ok()?;
    let path = PathBuf::from(value);
    if path.is_dir() {
        Some(path)
    } else {
        None
    }
}

#[cfg(not(windows))]
fn known_folder_from_registry(_registry_name: &str) -> Option<PathBuf> {
    None
}

/// Resolve a known folder: the registry (authoritative on Windows) first, then
/// the plain `<home>/<folder>` path as a fallback for POSIX and for any folder
/// Windows does not list.
fn resolve_known_folder(home: &Path, folder: &str, registry_name: &str) -> Option<PathBuf> {
    if let Some(p) = known_folder_from_registry(registry_name) {
        return Some(p);
    }
    let in_profile = home.join(folder);
    if in_profile.is_dir() {
        return Some(in_profile);
    }
    None
}

/// The user's well-known folders. Only folders that actually exist are returned,
/// so the sidebar never shows a link that leads nowhere.
#[tauri::command]
fn special_folders() -> Vec<Place> {
    let Some(home) = dirs_home() else {
        return Vec::new();
    };

    // (display name, icon kind, Windows registry value name)
    // The registry names are historical and do not match the display names:
    // Documents is "Personal", Pictures is "My Pictures", Videos is "My Video",
    // and Downloads is only exposed under its known-folder GUID.
    let candidates = [
        ("Desktop", "desktop", "Desktop"),
        ("Documents", "documents", "Personal"),
        (
            "Downloads",
            "downloads",
            "{374DE290-123F-4565-9164-39C4925E467B}",
        ),
        ("Pictures", "pictures", "My Pictures"),
        ("Music", "music", "My Music"),
        ("Videos", "videos", "My Video"),
    ];

    candidates
        .iter()
        .filter_map(|(folder, kind, registry_name)| {
            resolve_known_folder(&home, folder, registry_name).map(|p| Place {
                name: (*folder).to_string(),
                path: p.to_string_lossy().to_string(),
                kind: (*kind).to_string(),
            })
        })
        .collect()
}

// Small cross-platform home-dir resolver without an extra dependency.
fn dirs_home() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init());

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        builder = builder
            .plugin(tauri_plugin_process::init())
            .plugin(tauri_plugin_updater::Builder::new().build());
    }

    // Keep the screen awake for as long as the app is open (CPE-225). We hold a
    // single keep-awake assertion for the app's whole lifetime: created here on
    // the main thread, owned by the run-loop callback below, and dropped — which
    // releases it — the instant that loop ends, i.e. when the app quits. On a
    // hard crash the OS releases the assertion on process death, so nothing
    // lingers either way. Desktop-only: mobile has no such assertion. A failure
    // to acquire is logged, not fatal — the explorer still works, the screen just
    // isn't held awake.
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let keep_awake = keepawake::Builder::default()
        .display(true)
        .reason("Cross-Platform Explorer is open")
        .app_name("Cross-Platform Explorer")
        .app_reverse_domain("com.cross-platform-explorer.app")
        .create()
        .map_err(|e| eprintln!("keep-awake: could not inhibit screen lock: {e}"))
        .ok();

    let app = builder
        .invoke_handler(tauri::generate_handler![
            list_dir,
            home_dir,
            parent_dir,
            list_drives,
            special_folders,
            create_dir,
            read_file_text,
            write_file_text,
            read_archive_entries,
            read_preview_info,
            rename_entry,
            delete_to_trash,
            delete_permanent,
            can_restore_from_trash,
            restore_from_trash,
            copy_entries,
            move_entries,
            move_exact,
            entry_info,
            dir_size
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(move |_app_handle, _event| {
        // Owning `keep_awake` here keeps the assertion alive for the entire run
        // loop; it is dropped (and the screen lock re-enabled) when the loop
        // ends. The reference just anchors the capture — see the comment above.
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        let _ = &keep_awake;
    });
}

// NOTE: clippy's `items_after_test_module` lint requires the test module to be
// the LAST item in the file. Keep it here, at the bottom.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parent_dir_returns_the_parent() {
        assert_eq!(
            parent_dir("/home/user/docs".to_string()),
            Some("/home/user".to_string())
        );
    }

    #[test]
    fn parent_dir_at_root_returns_none() {
        assert_eq!(parent_dir("/".to_string()), None);
    }

    #[test]
    fn list_dir_errors_on_a_missing_path() {
        assert!(list_dir("/definitely/not/a/real/path/xyz".to_string()).is_err());
    }

    #[test]
    fn list_dir_lists_a_real_directory() {
        let dir = std::env::temp_dir();
        assert!(list_dir(dir.to_string_lossy().to_string()).is_ok());
    }

    #[test]
    fn home_dir_resolves() {
        assert!(home_dir().is_ok());
    }

    #[test]
    fn extension_is_lowercased_and_dotless() {
        assert_eq!(extension_of(Path::new("/a/b/Photo.PNG")), "png");
        assert_eq!(extension_of(Path::new("/a/b/archive.tar.gz")), "gz");
    }

    #[test]
    fn extension_is_empty_when_absent() {
        assert_eq!(extension_of(Path::new("/a/b/README")), "");
    }

    #[test]
    fn epoch_ms_of_unix_epoch_is_zero() {
        assert_eq!(to_epoch_ms(UNIX_EPOCH), Some(0));
    }

    #[test]
    fn epoch_ms_is_monotonic_for_later_times() {
        use std::time::Duration;
        let later = UNIX_EPOCH + Duration::from_millis(1_500);
        assert_eq!(to_epoch_ms(later), Some(1_500));
    }

    #[test]
    fn list_drives_returns_at_least_one_root() {
        assert!(!list_drives().is_empty(), "there is always at least one root");
    }

    #[test]
    fn special_folders_all_exist_and_are_labelled() {
        for place in special_folders() {
            assert!(Path::new(&place.path).is_dir(), "{} should exist", place.path);
            assert!(!place.kind.is_empty());
            assert!(!place.name.is_empty());
        }
    }

    #[test]
    fn known_folder_falls_back_to_the_profile_path() {
        // Use a registry value name that cannot exist, so the registry lookup
        // misses and the profile-relative fallback is exercised on every OS.
        let tmp = std::env::temp_dir();
        let sub = tmp.join("cpe_known_folder_test");
        std::fs::create_dir_all(&sub).expect("create temp subdir");

        let found = resolve_known_folder(&tmp, "cpe_known_folder_test", "CpeNoSuchRegistryValue");
        assert_eq!(found, Some(sub.clone()));

        let _ = std::fs::remove_dir(&sub);
    }

    #[test]
    fn known_folder_returns_none_when_it_exists_nowhere() {
        let tmp = std::env::temp_dir();
        assert_eq!(
            resolve_known_folder(
                &tmp,
                "cpe_definitely_missing_folder_xyz",
                "CpeNoSuchRegistryValue"
            ),
            None
        );
    }

    #[cfg(windows)]
    #[test]
    fn registry_lookup_misses_cleanly_for_an_unknown_value() {
        assert_eq!(known_folder_from_registry("CpeNoSuchRegistryValue"), None);
    }

    #[cfg(windows)]
    #[test]
    fn registry_resolves_the_desktop_known_folder() {
        // Desktop is always present in Shell Folders on a real Windows session.
        let desktop = known_folder_from_registry("Desktop");
        assert!(desktop.is_some(), "Desktop should resolve from the registry");
        assert!(desktop.unwrap().is_dir());
    }

    // ---- file operations (CPE-030) ----

    /// Unique scratch dir per test, so tests don't collide when run in parallel.
    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cpe_test_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    #[test]
    fn read_file_text_returns_contents_within_the_cap() {
        let d = scratch("read_ok");
        let f = d.join("note.txt");
        fs::write(&f, b"hello world").unwrap();
        let r = read_file_text(f.to_string_lossy().to_string(), 1024);
        assert_eq!(r.unwrap(), "hello world");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn read_file_text_errors_when_over_the_cap() {
        let d = scratch("read_big");
        let f = d.join("big.txt");
        fs::write(&f, vec![b'x'; 200]).unwrap();
        let r = read_file_text(f.to_string_lossy().to_string(), 100);
        assert!(r.is_err(), "a file over the cap must error, not truncate");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn read_file_text_errors_on_invalid_utf8() {
        let d = scratch("read_bin");
        let f = d.join("blob.bin");
        fs::write(&f, [0xff, 0xfe, 0x00, 0x01]).unwrap();
        let r = read_file_text(f.to_string_lossy().to_string(), 1024);
        assert!(r.is_err(), "non-UTF-8 content must error");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn write_file_text_replaces_contents() {
        let d = scratch("write_txt");
        let f = d.join("note.txt");
        fs::write(&f, b"old text").unwrap();
        let n = write_file_text(f.to_string_lossy().to_string(), "brand new".to_string()).unwrap();
        assert_eq!(n, 9);
        assert_eq!(fs::read_to_string(&f).unwrap(), "brand new");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn read_archive_entries_lists_zip_contents_without_extracting() {
        use std::io::Write;
        let d = scratch("zip_list");
        let zip_path = d.join("bundle.zip");
        {
            let f = fs::File::create(&zip_path).unwrap();
            let mut w = zip::ZipWriter::new(f);
            let opts = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            w.start_file("hello.txt", opts).unwrap();
            w.write_all(b"hi there").unwrap();
            w.add_directory("sub/", opts).unwrap();
            w.finish().unwrap();
        }

        let entries =
            read_archive_entries(zip_path.to_string_lossy().to_string()).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"hello.txt"), "should list the file entry");
        let file = entries.iter().find(|e| e.name == "hello.txt").unwrap();
        assert_eq!(file.size, 8, "size is the uncompressed length");
        assert!(!file.is_dir);
        assert!(
            entries.iter().any(|e| e.is_dir),
            "the directory entry should be flagged is_dir"
        );
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn read_archive_entries_errors_on_a_non_zip() {
        let d = scratch("zip_bad");
        let f = d.join("notazip.zip");
        fs::write(&f, b"this is not a zip file").unwrap();
        assert!(read_archive_entries(f.to_string_lossy().to_string()).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn read_archive_entries_lists_tar_contents() {
        let d = scratch("tar_list");
        let tar_path = d.join("bundle.tar");
        {
            let f = fs::File::create(&tar_path).unwrap();
            let mut b = tar::Builder::new(f);
            let data = b"hi there";
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_entry_type(tar::EntryType::Regular);
            header.set_cksum();
            b.append_data(&mut header, "hello.txt", &data[..]).unwrap();
            b.finish().unwrap();
        }
        let entries = read_archive_entries(tar_path.to_string_lossy().to_string()).unwrap();
        let file = entries.iter().find(|e| e.name == "hello.txt").unwrap();
        assert_eq!(file.size, 8, "size is the uncompressed length");
        assert!(!file.is_dir);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn read_archive_entries_lists_gzip_single_file() {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;
        let d = scratch("gz_single");
        let gz_path = d.join("note.txt.gz");
        {
            let f = fs::File::create(&gz_path).unwrap();
            let mut enc = GzEncoder::new(f, Compression::default());
            enc.write_all(b"hello world").unwrap();
            enc.finish().unwrap();
        }
        let entries = read_archive_entries(gz_path.to_string_lossy().to_string()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "note.txt", "name is the archive name minus .gz");
        assert_eq!(entries[0].size, 11, "ISIZE trailer is the uncompressed length");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn hex_dump_formats_offsets_and_ascii() {
        let d = scratch("hex");
        let f = d.join("blob.bin");
        fs::write(&f, b"AB\x00\xff").unwrap();
        let dump = hex_dump(&f.to_string_lossy(), 64).unwrap();
        assert!(dump.contains("00000000"), "has an offset column");
        assert!(dump.contains("41 42 00 ff"), "has the hex bytes");
        assert!(dump.contains("|AB..|"), "has the ASCII gutter with dots for non-print");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn wasm_info_disassembles_an_empty_module() {
        let d = scratch("wasm");
        let f = d.join("m.wasm");
        // The 8-byte empty WebAssembly module: magic "\0asm" + version 1.
        fs::write(&f, [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]).unwrap();
        let wat = wasm_info(&f.to_string_lossy(), 4096).unwrap();
        assert!(wat.contains("module"), "prints the (module) wat form");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn torrent_info_reads_bencode_metadata() {
        let d = scratch("torrent");
        let f = d.join("x.torrent");
        // d8:announce9:http://t/4:infod6:lengthi100e4:name3:foo12:piece lengthi16384eee
        let bytes = b"d8:announce9:http://t/4:infod6:lengthi100e4:name3:foo12:piece lengthi16384eee";
        fs::write(&f, bytes).unwrap();
        let info = torrent_info(&f.to_string_lossy()).unwrap();
        assert!(info.contains("Name: foo"), "extracts the name");
        assert!(info.contains("http://t/"), "extracts the announce URL");
        assert!(info.contains("single file"), "reports the single-file length");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn pe_info_errors_on_a_non_pe() {
        let d = scratch("pe_bad");
        let f = d.join("notpe.exe");
        fs::write(&f, b"MZ but not really a PE").unwrap();
        assert!(pe_info(&f.to_string_lossy()).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[cfg(windows)]
    #[test]
    fn pe_info_parses_a_real_windows_binary() {
        // The test executable itself is a PE on Windows.
        let exe = std::env::current_exe().unwrap();
        let info = pe_info(&exe.to_string_lossy()).unwrap();
        assert!(info.contains("PE32"), "identifies the PE image");
        assert!(info.contains("Sections:"), "lists sections");
    }

    #[test]
    fn rtf_text_extracts_body_and_drops_control_words() {
        let d = scratch("rtf");
        let f = d.join("doc.rtf");
        let rtf = r"{\rtf1\ansi{\fonttbl{\f0 Arial;}}\f0\fs24 Hello \b world\b0.\par Second line.}";
        fs::write(&f, rtf).unwrap();
        let text = rtf_text(&f.to_string_lossy()).unwrap();
        assert!(text.contains("Hello world."), "body text extracted: {text:?}");
        assert!(text.contains("Second line."), "second paragraph present");
        assert!(!text.contains("fonttbl"), "font table dropped");
        assert!(!text.contains("Arial"), "font table contents dropped");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn docx_text_extracts_paragraph_text() {
        use std::io::Write;
        let d = scratch("docx");
        let f = d.join("doc.docx");
        {
            let file = fs::File::create(&f).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let opts = zip::write::SimpleFileOptions::default();
            zip.start_file("word/document.xml", opts).unwrap();
            let xml = r#"<?xml version="1.0"?><w:document><w:body><w:p><w:r><w:t>Hello</w:t></w:r><w:r><w:t> world</w:t></w:r></w:p><w:p><w:r><w:t>Next &amp; last</w:t></w:r></w:p></w:body></w:document>"#;
            zip.write_all(xml.as_bytes()).unwrap();
            zip.finish().unwrap();
        }
        let text = docx_text(&f.to_string_lossy()).unwrap();
        assert!(text.contains("Hello world"), "runs joined within a paragraph: {text:?}");
        assert!(text.contains("Next & last"), "entities decoded");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn read_preview_info_dispatches_by_extension() {
        let d = scratch("dispatch");
        let f = d.join("thing.bin");
        fs::write(&f, b"\x01\x02\x03").unwrap();
        // .bin -> hex dump path
        let out = read_preview_info(f.to_string_lossy().to_string()).unwrap();
        assert!(out.contains("01 02 03"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn create_dir_rejects_an_empty_name() {
        let d = scratch("create_empty");
        let r = create_dir(d.to_string_lossy().to_string(), "   ".to_string());
        assert!(r.is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn create_dir_refuses_to_clobber_an_existing_name() {
        let d = scratch("create_dup");
        let p = d.to_string_lossy().to_string();
        assert!(create_dir(p.clone(), "thing".into()).is_ok());
        let second = create_dir(p, "thing".into());
        assert!(second.is_err(), "must not silently overwrite");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn rename_refuses_to_clobber_an_existing_name() {
        let d = scratch("rename_dup");
        fs::write(d.join("a.txt"), b"a").unwrap();
        fs::write(d.join("b.txt"), b"b").unwrap();

        let r = rename_entry(
            d.join("a.txt").to_string_lossy().to_string(),
            "b.txt".into(),
        );
        assert!(r.is_err(), "renaming onto an existing file must fail");
        // b.txt must be untouched.
        assert_eq!(fs::read(d.join("b.txt")).unwrap(), b"b");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn rename_moves_the_file() {
        let d = scratch("rename_ok");
        fs::write(d.join("a.txt"), b"a").unwrap();
        let r = rename_entry(
            d.join("a.txt").to_string_lossy().to_string(),
            "c.txt".into(),
        );
        assert!(r.is_ok());
        assert!(d.join("c.txt").exists());
        assert!(!d.join("a.txt").exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn unique_target_appends_copy_suffixes_instead_of_overwriting() {
        let d = scratch("unique");
        assert_eq!(unique_target(&d, "x.txt"), d.join("x.txt"));

        fs::write(d.join("x.txt"), b"1").unwrap();
        assert_eq!(unique_target(&d, "x.txt"), d.join("x - Copy.txt"));

        fs::write(d.join("x - Copy.txt"), b"2").unwrap();
        assert_eq!(unique_target(&d, "x.txt"), d.join("x - Copy (2).txt"));

        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn unique_target_handles_extensionless_names() {
        let d = scratch("unique_noext");
        fs::write(d.join("README"), b"1").unwrap();
        assert_eq!(unique_target(&d, "README"), d.join("README - Copy"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn copy_auto_renames_rather_than_overwriting() {
        let d = scratch("copy_same");
        fs::write(d.join("f.txt"), b"original").unwrap();

        let results = copy_entries(
            vec![d.join("f.txt").to_string_lossy().to_string()],
            d.to_string_lossy().to_string(),
        );
        assert!(results[0].ok, "{}", results[0].error);
        // The original must be untouched.
        assert_eq!(fs::read(d.join("f.txt")).unwrap(), b"original");
        assert!(d.join("f - Copy.txt").exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn copying_a_folder_into_itself_is_refused() {
        let d = scratch("copy_self");
        let inner = d.join("inner");
        fs::create_dir_all(inner.join("deep")).unwrap();

        // inner -> inner/deep  is a descendant: must be refused, not recursed.
        let results = copy_entries(
            vec![inner.to_string_lossy().to_string()],
            inner.join("deep").to_string_lossy().to_string(),
        );
        assert!(!results[0].ok, "copying a folder into its descendant must fail");
        assert!(results[0].error.contains("itself"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn copy_dir_all_copies_the_whole_tree() {
        let d = scratch("copy_tree");
        let src = d.join("src");
        fs::create_dir_all(src.join("a/b")).unwrap();
        fs::write(src.join("a/b/leaf.txt"), b"leaf").unwrap();

        let dst = d.join("dst");
        copy_dir_all(&src, &dst).unwrap();
        assert_eq!(fs::read(dst.join("a/b/leaf.txt")).unwrap(), b"leaf");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn move_relocates_and_removes_the_original() {
        let d = scratch("move_ok");
        let from = d.join("from");
        let to = d.join("to");
        fs::create_dir_all(&from).unwrap();
        fs::create_dir_all(&to).unwrap();
        fs::write(from.join("m.txt"), b"m").unwrap();

        let results = move_entries(
            vec![from.join("m.txt").to_string_lossy().to_string()],
            to.to_string_lossy().to_string(),
        );
        assert!(results[0].ok, "{}", results[0].error);
        assert!(to.join("m.txt").exists());
        assert!(!from.join("m.txt").exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn bulk_ops_report_per_item_instead_of_aborting_on_first_failure() {
        let d = scratch("bulk");
        let to = d.join("to");
        fs::create_dir_all(&to).unwrap();
        fs::write(d.join("good.txt"), b"g").unwrap();

        let results = copy_entries(
            vec![
                d.join("good.txt").to_string_lossy().to_string(),
                d.join("missing.txt").to_string_lossy().to_string(),
            ],
            to.to_string_lossy().to_string(),
        );
        assert_eq!(results.len(), 2, "every item must get a result");
        assert!(results[0].ok);
        assert!(!results[1].ok, "the missing file must be reported, not skipped");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn entry_info_reports_metadata() {
        let d = scratch("info");
        fs::write(d.join("i.txt"), b"12345").unwrap();
        let info = entry_info(d.join("i.txt").to_string_lossy().to_string()).unwrap();
        assert_eq!(info.name, "i.txt");
        assert!(!info.is_dir);
        assert_eq!(info.size, 5);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn dir_size_sums_the_tree() {
        let d = scratch("dirsize");
        fs::create_dir_all(d.join("sub")).unwrap();
        fs::write(d.join("a.bin"), vec![0u8; 100]).unwrap();
        fs::write(d.join("sub/b.bin"), vec![0u8; 50]).unwrap();

        let total = dir_size(d.to_string_lossy().to_string()).unwrap();
        assert_eq!(total, 150);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn move_exact_restores_to_the_original_name() {
        let d = scratch("move_exact");
        fs::write(d.join("b.txt"), b"x").unwrap();

        let results = move_exact(vec![(
            d.join("b.txt").to_string_lossy().to_string(),
            d.join("a.txt").to_string_lossy().to_string(),
        )]);
        assert!(results[0].ok, "{}", results[0].error);
        assert!(d.join("a.txt").exists());
        assert!(!d.join("b.txt").exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn move_exact_refuses_to_overwrite() {
        let d = scratch("move_exact_clobber");
        fs::write(d.join("a.txt"), b"keep").unwrap();
        fs::write(d.join("b.txt"), b"other").unwrap();

        let results = move_exact(vec![(
            d.join("b.txt").to_string_lossy().to_string(),
            d.join("a.txt").to_string_lossy().to_string(),
        )]);
        assert!(!results[0].ok, "undo must not clobber an existing file");
        assert_eq!(fs::read(d.join("a.txt")).unwrap(), b"keep");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn dotfiles_are_hidden_on_posix_convention() {
        let d = scratch("hidden");
        let p = d.join(".secret");
        fs::write(&p, b"x").unwrap();
        let meta = fs::metadata(&p).unwrap();
        assert!(is_hidden(&p, &meta));

        let visible = d.join("plain.txt");
        fs::write(&visible, b"x").unwrap();
        let vmeta = fs::metadata(&visible).unwrap();
        assert!(!is_hidden(&visible, &vmeta));
        let _ = fs::remove_dir_all(&d);
    }
}
