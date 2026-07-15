use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Live provider API-key verification + catalog egress for the AI Console sidecar (CPE-347/369/376).
/// Only compiled with the platform: without it nothing calls these, so the module would be dead
/// code under `-D warnings` (its pure logic is still unit-tested under the feature).
#[cfg(feature = "sidecar-platform")]
mod keyverify;

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

/// Create a new empty file `name` inside `path` (CPE-254). Mirrors `create_dir`:
/// `create_new` fails atomically rather than clobbering an existing file.
#[tauri::command]
fn create_file(path: String, name: String) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Name cannot be empty".to_string());
    }
    let target = Path::new(&path).join(name);
    if target.exists() {
        return Err(format!("\"{name}\" already exists"));
    }
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
        .map_err(|e| e.to_string())?;
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

/// List the entries of a 7-Zip archive via sevenz-rust (CPE-110).
fn sevenz_entries(path: &str) -> Result<Vec<ArchiveEntry>, String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let len = file.metadata().map_err(|e| e.to_string())?.len();
    let archive = sevenz_rust::Archive::read(&mut file, len, &[]).map_err(|e| e.to_string())?;
    Ok(archive
        .files
        .iter()
        .map(|f| ArchiveEntry {
            name: f.name().to_string(),
            size: f.size(),
            is_dir: f.is_directory(),
        })
        .collect())
}

/// List the files in an ISO 9660 disc image (bounded), via iso9660 (CPE-113).
fn iso_entries(path: &str) -> Result<Vec<ArchiveEntry>, String> {
    use iso9660::{DirectoryEntry, ISODirectory, ISO9660};
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let iso = ISO9660::new(file).map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    let mut stack: Vec<(String, ISODirectory<fs::File>)> = vec![(String::new(), iso.root)];
    while let Some((prefix, dir)) = stack.pop() {
        if out.len() >= 2000 {
            break;
        }
        for entry in dir.contents() {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            match entry {
                DirectoryEntry::Directory(d) => {
                    if d.identifier == "." || d.identifier == ".." {
                        continue;
                    }
                    let full = if prefix.is_empty() {
                        d.identifier.clone()
                    } else {
                        format!("{prefix}/{}", d.identifier)
                    };
                    out.push(ArchiveEntry {
                        name: format!("{full}/"),
                        size: 0,
                        is_dir: true,
                    });
                    stack.push((full, d));
                }
                DirectoryEntry::File(f) => {
                    let full = if prefix.is_empty() {
                        f.identifier.clone()
                    } else {
                        format!("{prefix}/{}", f.identifier)
                    };
                    out.push(ArchiveEntry {
                        name: full,
                        size: f.size() as u64,
                        is_dir: false,
                    });
                }
            }
        }
    }
    Ok(out)
}

/// List an archive's entries without extracting it, for the preview pane.
/// Dispatches by extension: ZIP family (zip/jar/apk/war/ear/ipa/xpi), TAR,
/// gzip-compressed TAR (.tar.gz/.tgz), single-file gzip (.gz), 7-Zip, and ISO.
/// Reads only the archive directory, so it stays cheap even for large archives.
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
    } else if lower.ends_with(".7z") {
        sevenz_entries(&path)
    } else if lower.ends_with(".iso") {
        iso_entries(&path)
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

// ---- Data formats (CPE-088/090/091) ----

/// Read-only summary of a SQLite database: its tables/views, each with a row
/// count and column list (CPE-088).
fn sqlite_info(path: &str) -> Result<String, String> {
    use rusqlite::{Connection, OpenFlags};
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;

    let items: Vec<(String, String)> = {
        let mut stmt = conn
            .prepare(
                "SELECT name, type FROM sqlite_master \
                 WHERE type IN ('table','view') AND name NOT LIKE 'sqlite_%' ORDER BY name",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| e.to_string())?;
        rows.filter_map(|r| r.ok()).collect()
    };

    let mut out = format!("SQLite database — {} table(s)/view(s)\n\n", items.len());
    for (name, kind) in &items {
        // Names come from sqlite_master; double-quote and escape for safety.
        let quoted = format!("\"{}\"", name.replace('"', "\"\""));
        let columns: Vec<String> = {
            let mut cols = Vec::new();
            if let Ok(mut stmt) = conn.prepare(&format!("PRAGMA table_info({quoted})")) {
                if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(1)) {
                    cols = rows.filter_map(|r| r.ok()).collect();
                }
            }
            cols
        };
        let detail = if kind == "table" {
            match conn.query_row(&format!("SELECT COUNT(*) FROM {quoted}"), [], |r| {
                r.get::<_, i64>(0)
            }) {
                Ok(c) => format!("{c} rows"),
                Err(_) => "unreadable".to_string(),
            }
        } else {
            "view".to_string()
        };
        out.push_str(&format!("{name} ({kind}) — {detail}\n"));
        if !columns.is_empty() {
            out.push_str(&format!("  columns: {}\n", columns.join(", ")));
        }
    }
    Ok(out)
}

/// Read-only text-grid preview of a spreadsheet workbook (XLSX/ODS) via
/// calamine: each sheet rendered as tab-separated rows, capped (CPE-090/091).
fn spreadsheet_info(path: &str) -> Result<String, String> {
    use calamine::{open_workbook_auto, Reader};
    const MAX_ROWS: usize = 100;
    const MAX_COLS: usize = 20;

    let mut wb = open_workbook_auto(path).map_err(|e| e.to_string())?;
    let names: Vec<String> = wb.sheet_names().iter().map(|s| s.to_string()).collect();
    let mut out = format!("Workbook — {} sheet(s): {}\n", names.len(), names.join(", "));

    for name in &names {
        if let Ok(range) = wb.worksheet_range(name) {
            let (h, w) = (range.height(), range.width());
            out.push_str(&format!("\n=== {name} ({h} x {w}) ===\n"));
            for row in range.rows().take(MAX_ROWS) {
                let cells: Vec<String> = row
                    .iter()
                    .take(MAX_COLS)
                    .map(|c| c.to_string())
                    .collect();
                out.push_str(&cells.join("\t"));
                out.push('\n');
            }
            if h > MAX_ROWS {
                out.push_str(&format!("… {} more rows\n", h - MAX_ROWS));
            }
        }
    }
    Ok(out)
}

/// Read-only schema summary of a Parquet file via the parquet crate's footer
/// metadata — no full column scan (CPE-089).
fn parquet_info(path: &str) -> Result<String, String> {
    use parquet::file::reader::{FileReader, SerializedFileReader};
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let reader = SerializedFileReader::new(file).map_err(|e| e.to_string())?;
    let meta = reader.metadata();
    let fmeta = meta.file_metadata();
    let schema = fmeta.schema_descr();
    let mut out = format!(
        "Parquet — {} rows, {} row group(s)\nCreated by: {}\n\nColumns ({}):\n",
        fmeta.num_rows(),
        meta.num_row_groups(),
        fmeta.created_by().unwrap_or("(unknown)"),
        schema.num_columns()
    );
    for i in 0..schema.num_columns() {
        let col = schema.column(i);
        out.push_str(&format!("  {} : {:?}\n", col.name(), col.physical_type()));
    }
    Ok(out)
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
        "sqlite" | "sqlite3" | "db" => sqlite_info(&path),
        "xlsx" | "xlsm" | "ods" => spreadsheet_info(&path),
        "parquet" => parquet_info(&path),
        // generic binary (.bin/.dat) and anything else routed here: hex dump
        _ => hex_dump(&path, 64 * 1024),
    }
}

/// Decode an image the webview can't render natively (TIFF, PSD) to a PNG
/// `data:` URL the <img> tag can show (CPE-099/101). PSD uses the psd crate's
/// flattened composite; TIFF uses the image crate. Capped by the source reader,
/// and errors (rather than hangs) on a corrupt file.
#[tauri::command]
fn read_image_data_url(path: String) -> Result<String, String> {
    use base64::Engine;
    use std::io::Cursor;

    let ext = extension_of(Path::new(&path));
    // Produce PNG bytes from the source format.
    let png: Vec<u8> = if ext == "psd" {
        let bytes = fs::read(&path).map_err(|e| e.to_string())?;
        let psd = psd::Psd::from_bytes(&bytes).map_err(|e| e.to_string())?;
        let rgba = psd.rgba();
        let buf = image::RgbaImage::from_raw(psd.width(), psd.height(), rgba)
            .ok_or("PSD pixel buffer size mismatch")?;
        let mut out = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(buf)
            .write_to(&mut out, image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        out.into_inner()
    } else {
        // TIFF (and any other image-crate-decodable format routed here)
        let img = image::open(&path).map_err(|e| e.to_string())?;
        let mut out = Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        out.into_inner()
    };

    let b64 = base64::engine::general_purpose::STANDARD.encode(&png);
    Ok(format!("data:image/png;base64,{b64}"))
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

/// Read `settings.json` from `dir`, returning `{}` when it's absent or
/// unreadable so the frontend always starts from a valid document.
fn read_settings_from(dir: &Path) -> String {
    fs::read_to_string(dir.join("settings.json")).unwrap_or_else(|_| "{}".to_string())
}

/// Write the settings document to `settings.json` in `dir`, creating `dir` if
/// needed.
fn write_settings_to(dir: &Path, contents: &str) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    fs::write(dir.join("settings.json"), contents.as_bytes()).map_err(|e| e.to_string())?;
    Ok(())
}

/// Read the single on-disk settings file (`settings.json` in the app config
/// dir). Returns `{}` when it doesn't exist yet, so the frontend can start from
/// defaults on a fresh install (CPE-226).
#[tauri::command]
fn read_settings(app: tauri::AppHandle) -> Result<String, String> {
    use tauri::Manager;
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    Ok(read_settings_from(&dir))
}

/// Write the single on-disk settings file, creating the config dir if needed
/// (CPE-226). `contents` is the full settings JSON document.
#[tauri::command]
fn write_settings(app: tauri::AppHandle, contents: String) -> Result<(), String> {
    use tauri::Manager;
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    write_settings_to(&dir, &contents)
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
/// Normalise a git remote URL to a browsable https URL:
/// `git@github.com:owner/repo.git` / `ssh://git@host/owner/repo.git` /
/// `https://host/owner/repo.git` → `https://host/owner/repo`.
fn normalize_git_url(raw: &str) -> String {
    let mut u = raw.trim().to_string();
    if let Some(rest) = u.strip_prefix("git@") {
        if let Some((host, path)) = rest.split_once(':') {
            u = format!("https://{host}/{path}");
        }
    } else if let Some(rest) = u.strip_prefix("ssh://git@") {
        u = format!("https://{rest}");
    } else if let Some(rest) = u.strip_prefix("git://") {
        u = format!("https://{rest}");
    }
    if let Some(stripped) = u.strip_suffix(".git") {
        u = stripped.to_string();
    }
    u
}

/// Open a file or folder with its default OS application (CPE-240). Uses the OS
/// shell opener directly (Windows `start`, macOS `open`, Linux `xdg-open`) —
/// more reliable than the opener plugin, which wasn't launching apps for several
/// file types. For an executable (.exe/.cmd/.bat/…) this runs it.
#[tauri::command]
fn open_external(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let spawned = std::process::Command::new("cmd")
        .args(["/C", "start", "", &path])
        .spawn();
    #[cfg(target_os = "macos")]
    let spawned = std::process::Command::new("open").arg(&path).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let spawned = std::process::Command::new("xdg-open").arg(&path).spawn();

    spawned.map(|_| ()).map_err(|e| e.to_string())
}

/// Open the platform's terminal with its working directory set to `path`
/// (CPE-253). Windows prefers Windows Terminal and falls back to a fresh cmd
/// window; macOS uses Terminal.app; Linux tries the common emulators in turn.
#[tauri::command]
fn open_terminal(path: String) -> Result<(), String> {
    use std::process::Command;

    #[cfg(target_os = "windows")]
    {
        // Windows Terminal opens directly at a directory with -d.
        if Command::new("wt.exe").args(["-d", &path]).spawn().is_ok() {
            return Ok(());
        }
        // Fallback: a new cmd window whose working dir is `path`. `start ""`
        // spawns the window; current_dir sets where it opens.
        Command::new("cmd")
            .args(["/C", "start", "", "cmd"])
            .current_dir(&path)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .args(["-a", "Terminal", &path])
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // Try the common terminals in order; the first that launches wins.
        let candidates = ["x-terminal-emulator", "gnome-terminal", "konsole", "xterm"];
        for term in candidates {
            if Command::new(term).current_dir(&path).spawn().is_ok() {
                return Ok(());
            }
        }
        Err("no terminal emulator found".into())
    }
}

/// Extract a single entry from a ZIP to a temp file and return its path, so it
/// can be opened with its default app while browsing inside the archive
/// (CPE-242). Read-only: the temp copy is what opens, not the archived bytes.
#[tauri::command]
fn extract_archive_entry(zip: String, inner: String) -> Result<String, String> {
    let file = fs::File::open(&zip).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    // The frontend uses "/"; some zips store "\" — try the given name then the
    // backslash variant so extraction works either way.
    let backslashed = inner.replace('/', "\\");
    let idx = archive
        .index_for_name(&inner)
        .or_else(|| archive.index_for_name(&backslashed))
        .ok_or_else(|| format!("entry not found: {inner}"))?;
    let mut entry = archive.by_index(idx).map_err(|e| e.to_string())?;

    let base = std::path::Path::new(&inner)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .ok_or_else(|| "invalid entry name".to_string())?;
    let dir = std::env::temp_dir().join("cpe-archive");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let out = dir.join(&base);
    let mut w = fs::File::create(&out).map_err(|e| e.to_string())?;
    std::io::copy(&mut entry, &mut w).map_err(|e| e.to_string())?;
    Ok(out.to_string_lossy().to_string())
}

// ---------------------------------------------------------------------------
// Archive creation & extraction (CPE-251 / CPE-252)
//
// The browser (CPE-064/242) reads archives and extracts single entries to a
// temp file; these two commands complete the round trip — pack a selection into
// a new .zip, and unpack a whole archive to a folder. ZIP is the create format
// (universal, already a dependency); extraction dispatches by extension like
// `read_archive_entries`.
// ---------------------------------------------------------------------------

/// Recursively add `src` to an open zip under the archive path `name_in_zip`.
/// Directories become explicit entries so empty folders survive the round trip.
fn zip_add_path(
    writer: &mut zip::ZipWriter<fs::File>,
    src: &std::path::Path,
    name_in_zip: &str,
    opts: zip::write::SimpleFileOptions,
) -> Result<(), String> {
    let meta = fs::symlink_metadata(src).map_err(|e| e.to_string())?;
    if meta.is_dir() {
        writer
            .add_directory(format!("{name_in_zip}/"), opts)
            .map_err(|e| e.to_string())?;
        // Sort children for a stable, reproducible archive order.
        let mut children: Vec<_> = fs::read_dir(src)
            .map_err(|e| e.to_string())?
            .filter_map(|e| e.ok())
            .collect();
        children.sort_by_key(|e| e.file_name());
        for child in children {
            let child_name = child.file_name().to_string_lossy().to_string();
            zip_add_path(
                writer,
                &child.path(),
                &format!("{name_in_zip}/{child_name}"),
                opts,
            )?;
        }
    } else {
        writer
            .start_file(name_in_zip, opts)
            .map_err(|e| e.to_string())?;
        let mut f = fs::File::open(src).map_err(|e| e.to_string())?;
        std::io::copy(&mut f, writer).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Pack the given files/folders into a new deflated `.zip` at `dest` (CPE-251).
/// Each top-level selection keeps its own name at the archive root; folders are
/// added recursively. Returns the created zip path.
#[tauri::command]
fn compress_to_zip(paths: Vec<String>, dest: String) -> Result<String, String> {
    if paths.is_empty() {
        return Err("nothing to compress".into());
    }
    let file = fs::File::create(&dest).map_err(|e| e.to_string())?;
    let mut writer = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for p in &paths {
        let src = std::path::Path::new(p);
        let name = src
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .ok_or_else(|| format!("invalid path: {p}"))?;
        zip_add_path(&mut writer, src, &name, opts)?;
    }
    writer.finish().map_err(|e| e.to_string())?;
    Ok(dest)
}

/// Unpack a tar stream into `dest`.
fn tar_unpack<R: std::io::Read>(reader: R, dest: &std::path::Path) -> Result<(), String> {
    let mut archive = tar::Archive::new(reader);
    archive.unpack(dest).map_err(|e| e.to_string())
}

/// Extract an archive into `dest`, which is created if missing (CPE-252).
/// Dispatched by extension like `read_archive_entries`; zip extraction uses the
/// crate's path-safe extractor, so a malicious "zip-slip" entry can't escape.
#[tauri::command]
fn extract_archive(path: String, dest: String) -> Result<String, String> {
    fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    let dest_path = std::path::Path::new(&dest);
    let lower = path.to_lowercase();

    if lower.ends_with(".tar") {
        let file = fs::File::open(&path).map_err(|e| e.to_string())?;
        tar_unpack(file, dest_path)?;
    } else if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        let file = fs::File::open(&path).map_err(|e| e.to_string())?;
        tar_unpack(flate2::read::GzDecoder::new(file), dest_path)?;
    } else if lower.ends_with(".gz") {
        // A bare .gz holds a single file; its name is the archive name minus .gz.
        let stem = std::path::Path::new(&path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "extracted".to_string());
        let file = fs::File::open(&path).map_err(|e| e.to_string())?;
        let mut decoder = flate2::read::GzDecoder::new(file);
        let mut out = fs::File::create(dest_path.join(stem)).map_err(|e| e.to_string())?;
        std::io::copy(&mut decoder, &mut out).map_err(|e| e.to_string())?;
    } else if lower.ends_with(".7z") {
        sevenz_rust::decompress_file(&path, dest_path).map_err(|e| e.to_string())?;
    } else {
        // zip family (zip/jar/apk/war/…): the crate's extractor guards against
        // path traversal via ZipFile::enclosed_name.
        let file = fs::File::open(&path).map_err(|e| e.to_string())?;
        let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
        archive.extract(dest_path).map_err(|e| e.to_string())?;
    }
    Ok(dest)
}

/// Run an executable with elevation (CPE-241). On Windows this uses
/// `Start-Process -Verb RunAs`, which shows the UAC prompt. On other platforms
/// there is no standard per-launch elevation prompt, so it runs normally.
#[tauri::command]
fn run_as_admin(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        // Single-quote the path for PowerShell; escape any embedded quote.
        let escaped = path.replace('\'', "''");
        std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!("Start-Process -FilePath '{escaped}' -Verb RunAs"),
            ])
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "windows"))]
    {
        open_external(path)
    }
}

/// Read a repo's `.git/config` and return its origin remote as a browsable https
/// URL (folder-context plugins, CPE-235). A cheap single file read; returns None
/// if the folder isn't a repo or has no remote.
#[tauri::command]
fn git_remote_url(path: String) -> Option<String> {
    let cfg = std::path::Path::new(&path).join(".git").join("config");
    let text = std::fs::read_to_string(cfg).ok()?;

    let mut in_origin = false;
    let mut origin_url: Option<String> = None;
    let mut first_url: Option<String> = None;
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_origin = l.contains("remote \"origin\"");
            continue;
        }
        if let Some(rest) = l.strip_prefix("url") {
            if let Some(eq) = rest.find('=') {
                let value = rest[eq + 1..].trim().to_string();
                if first_url.is_none() {
                    first_url = Some(value.clone());
                }
                if in_origin {
                    origin_url = Some(value);
                }
            }
        }
    }
    origin_url.or(first_url).map(|u| normalize_git_url(&u))
}

// ---------------------------------------------------------------------------
// Sidecar platform integration (ADR 0001 / CPE-260), feature-gated.
//
// Everything below is compiled ONLY with `--features sidecar-platform`. With the
// feature off (the default), none of this — and none of the `sidecar-host` crate —
// is part of the build, so the plain explorer stays byte-for-byte as it was. That is
// the delete-test (CPE-272): remove the sidecars and the explorer is unaffected.
// ---------------------------------------------------------------------------

/// List the ids of sidecars registered in the bundled + user registry directories.
/// A first, minimal seam into the platform host; the pane mount and supervisor wiring
/// build on this (CPE-271 onward).
/// The registry directories the platform reads sidecar manifests from: the bundled
/// catalog (shipped with the app) + a user-writable dir under app config.
#[cfg(feature = "sidecar-platform")]
fn sidecar_dirs(app: &tauri::AppHandle) -> Vec<PathBuf> {
    use tauri::Manager;
    let mut dirs = Vec::new();
    if let Ok(resource) = app.path().resource_dir() {
        dirs.push(resource.join("sidecars"));
    }
    if let Ok(config) = app.path().app_config_dir() {
        dirs.push(config.join("sidecars"));
    }
    // Dev fallback: `tauri dev` (debug) has no bundled `sidecars/` resource dir, so the host
    // registry wouldn't know the sidecar and consent would be skipped (CPE-364). Point at the
    // source-tree manifest, guarded by its existence so it's inert in a bundled release.
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    for p in [manifest.join("../sidecar/ai-console"), PathBuf::from("sidecar/ai-console")] {
        if p.join("sidecar.json").exists() {
            dirs.push(p);
        }
    }
    dirs
}

/// Directory holding the persisted capability-consent store (CPE-296).
#[cfg(feature = "sidecar-platform")]
fn consent_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;
    app.path()
        .app_config_dir()
        .map(|c| c.join("sidecars"))
        .map_err(|e| e.to_string())
}

/// List the ids of sidecars registered in the bundled + user registry directories.
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn sidecar_registry_ids(app: tauri::AppHandle) -> Vec<String> {
    sidecar_host::registry::Registry::load_from_dirs(&sidecar_dirs(&app))
        .all()
        .map(|m| m.id.clone())
        .collect()
}

/// A sidecar's requested capabilities plus the persisted consent decision (CPE-296):
/// which are already granted, and which are still undecided (need a consent prompt).
#[cfg(feature = "sidecar-platform")]
#[derive(serde::Serialize)]
struct ConsentState {
    requested: Vec<sidecar_contract::Capability>,
    granted: Vec<sidecar_contract::Capability>,
    undecided: Vec<sidecar_contract::Capability>,
}

#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn sidecar_consent_state(app: tauri::AppHandle, id: String) -> Result<ConsentState, String> {
    let reg = sidecar_host::registry::Registry::load_from_dirs(&sidecar_dirs(&app));
    let requested = reg
        .get(&id)
        .map(|m| m.capabilities.clone())
        .ok_or_else(|| format!("no sidecar '{id}' in the registry"))?;
    let store = sidecar_host::consent::ConsentStore::load(&consent_dir(&app)?);
    let granted: Vec<_> = store.granted(&id).into_iter().collect();
    let undecided = store.undecided(&id, &requested);
    Ok(ConsentState { requested, granted, undecided })
}

/// Record the user's consent decision: `granted` are approved, the remaining `decided`
/// capabilities are denied. Persisted so the user is asked once per capability (CPE-296).
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn sidecar_set_consent(
    app: tauri::AppHandle,
    id: String,
    granted: Vec<sidecar_contract::Capability>,
    decided: Vec<sidecar_contract::Capability>,
) -> Result<(), String> {
    let mut store = sidecar_host::consent::ConsentStore::load(&consent_dir(&app)?);
    let granted_set = granted.into_iter().collect();
    store
        .record(&id, &granted_set, &decided)
        .map_err(|e| e.to_string())
}

/// Revoke a previously-granted capability (management UI, CPE-274/296). Takes effect on
/// the sidecar's next launch.
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn sidecar_revoke_capability(
    app: tauri::AppHandle,
    id: String,
    capability: sidecar_contract::Capability,
) -> Result<(), String> {
    let mut store = sidecar_host::consent::ConsentStore::load(&consent_dir(&app)?);
    store.revoke(&id, capability).map_err(|e| e.to_string())
}

/// One row in the platform management UI (CPE-274): a registered sidecar with its
/// identity, contract compatibility, running/enabled state, and consent picture.
#[cfg(feature = "sidecar-platform")]
#[derive(serde::Serialize)]
struct SidecarInfo {
    id: String,
    name: String,
    version: String,
    contract: String,
    compatible: bool,
    running: bool,
    enabled: bool,
    requested: Vec<sidecar_contract::Capability>,
    granted: Vec<sidecar_contract::Capability>,
}

/// List registered sidecars with version, contract compatibility, running/enabled state,
/// and granted capabilities — the data behind the management panel (CPE-274).
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn sidecar_details(app: tauri::AppHandle, state: tauri::State<AiConsoleState>) -> Result<Vec<SidecarInfo>, String> {
    use sidecar_contract::CONTRACT_VERSION;
    let reg = sidecar_host::registry::Registry::load_from_dirs(&sidecar_dirs(&app));
    let consent = sidecar_host::consent::ConsentStore::load(&consent_dir(&app)?);
    let enablement = sidecar_host::enablement::EnablementStore::load(&consent_dir(&app)?);
    let ai_running = state.conn.lock().map(|g| g.is_some()).unwrap_or(false);

    Ok(reg
        .all()
        .map(|m| {
            let cv = &m.contract_version;
            SidecarInfo {
                id: m.id.clone(),
                name: m.name.clone(),
                version: m.version.clone(),
                contract: format!("{}.{}", cv.major, cv.minor),
                compatible: cv.major == CONTRACT_VERSION.major && cv.minor <= CONTRACT_VERSION.minor,
                running: m.id == "ai-console" && ai_running,
                enabled: enablement.is_enabled(&m.id),
                requested: m.capabilities.clone(),
                granted: consent.granted(&m.id).into_iter().collect(),
            }
        })
        .collect())
}

/// Stop a running sidecar (management UI). Dropping the connection reaps the process.
/// Only the AI Console is currently spawnable; a no-op for others.
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn sidecar_stop(id: String, state: tauri::State<AiConsoleState>) -> Result<(), String> {
    if id == "ai-console" {
        *state.conn.lock().map_err(|_| "state lock poisoned")? = None;
        state.log(sidecar_host::observability::LogLevel::Info, "stopped by user");
    }
    Ok(())
}

/// Enable or disable a sidecar (CPE-274). Disabling stops it (if running) and prevents it
/// from starting until re-enabled. Independent per sidecar — never touches others.
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn sidecar_set_enabled(
    app: tauri::AppHandle,
    state: tauri::State<AiConsoleState>,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    let mut store = sidecar_host::enablement::EnablementStore::load(&consent_dir(&app)?);
    store.set_enabled(&id, enabled).map_err(|e| e.to_string())?;
    if !enabled && id == "ai-console" {
        *state.conn.lock().map_err(|_| "state lock poisoned")? = None; // stop it now
        state.log(sidecar_host::observability::LogLevel::Info, "disabled by user");
    }
    Ok(())
}

/// Holds the live AI Console sidecar connection for the app's lifetime so it keeps
/// running while its UI pane is mounted, plus the observability state the management
/// panel surfaces (CPE-323): a bounded ring buffer of recent lifecycle log lines and the
/// last error that stopped it from starting. Both are populated by
/// `sidecar_start_ai_console` and read (redacted) by `sidecar_diagnostics`.
#[cfg(feature = "sidecar-platform")]
struct AiConsoleState {
    /// Handle to the live AI Console connection (CPE-349): a servicing thread owns the real
    /// `ProcessConnection`; this is the control handle. `Some` still means "running", and
    /// setting the slot to `None` (stop/disable) drops it, signalling the thread to exit and
    /// reap the child — preserving the previous stop semantics.
    conn: std::sync::Mutex<Option<ConsoleConn>>,
    logs: sidecar_host::observability::LogCapture,
    last_error: std::sync::Mutex<Option<String>>,
}

/// Control handle for the AI Console servicing thread (CPE-349). Dropping it asks the thread
/// to stop; the thread then drops the underlying connection, which reaps the child.
#[cfg(feature = "sidecar-platform")]
struct ConsoleConn {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    _thread: std::thread::JoinHandle<()>,
}

#[cfg(feature = "sidecar-platform")]
impl Drop for ConsoleConn {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Service the AI Console sidecar's inbound capability requests (CPE-349). The sidecar sends
/// `secrets.*` (CPE-344) over the channel; without this loop those requests are never
/// answered and the launcher's Keys panel times out. Reads a request, dispatches it through a
/// broker holding the granted providers (the OS keychain secrets provider on Windows), and
/// writes the response back with the same correlation id. Exits on stop or when the sidecar
/// closes the connection, dropping the connection (which reaps the child).
/// Open the native folder dialog (on the main thread) and return `{ path }` — the response
/// to the sandboxed launcher's `host.pick_folder` request (CPE-354). `path` is null when the
/// user cancels.
#[cfg(feature = "sidecar-platform")]
fn pick_folder_response(app: &tauri::AppHandle, params: &serde_json::Value) -> sidecar_contract::Response {
    use serde_json::json;
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = std::sync::mpsc::channel();
    let mut builder = app.dialog().file();
    // Open at the launcher's current Project folder when it still exists — a typo or a since-deleted
    // path just falls back to the OS default rather than erroring.
    if let Some(start) = params.get("start").and_then(|v| v.as_str()) {
        let p = std::path::Path::new(start);
        if p.is_dir() {
            builder = builder.set_directory(p);
        }
    }
    builder.pick_folder(move |folder| {
        let _ = tx.send(folder);
    });
    let path = rx
        .recv()
        .ok()
        .flatten()
        .and_then(|f| f.into_path().ok())
        .map(|p| p.to_string_lossy().into_owned());
    sidecar_contract::Response { result: Ok(json!({ "path": path })) }
}

/// Verify a provider API key on the sidecar's behalf (CPE-347) — the response to a sandboxed
/// `host.verify_key` request. The URL is chosen host-side from an allow-list (see `keyverify`),
/// never from the request, so this can't be turned into a general fetch. Returns
/// `{ valid, live, detail }`.
#[cfg(feature = "sidecar-platform")]
fn verify_key_response(params: &serde_json::Value) -> sidecar_contract::Response {
    use serde_json::json;
    let provider = params.get("provider").and_then(|v| v.as_str()).unwrap_or("");
    let key = params.get("key").and_then(|v| v.as_str()).unwrap_or("");
    let (valid, live, detail) = keyverify::verify_live(provider, key);
    sidecar_contract::Response { result: Ok(json!({ "valid": valid, "live": live, "detail": detail })) }
}

/// Trusted first-party public keys (hex) for agent-catalog signatures (CPE-376/377/380). The
/// matching private seed is the `CPE_CATALOG_SIGNING_KEY` release secret. Empty here would mean the
/// catalog-update feature is dormant (nothing trusted); a real key activates it.
#[cfg(feature = "sidecar-platform")]
const CATALOG_TRUSTED_KEYS: &[&str] =
    &["5b18ad467b37b7c06556000f15359a845bd85790ece91de110a337890d017130"];

/// The writable agent-catalog dir on this machine — where fetched, verified manifests land and
/// where the sidecar loads them from. Both the fetch handler and the sidecar (via env) agree on it.
#[cfg(feature = "sidecar-platform")]
fn catalog_dir(app: &tauri::AppHandle) -> PathBuf {
    use tauri::Manager;
    app.path()
        .app_data_dir()
        .map(|d| d.join("ai-console-catalog"))
        .unwrap_or_else(|_| std::env::temp_dir().join("cpe-ai-console-catalog"))
}

/// The catalog source base URL — the app's GitHub Releases `latest/download/` by default (the
/// signed bundle rides next to the installer), overridable via `CPE_CATALOG_URL`.
#[cfg(feature = "sidecar-platform")]
fn catalog_url() -> String {
    std::env::var("CPE_CATALOG_URL").unwrap_or_else(|_| {
        "https://github.com/StewartScottRogers/cross-platform-explorer/releases/latest/download/"
            .into()
    })
}

/// Response to the sandboxed `host.fetch_catalog` request (CPE-376): download the signed catalog
/// bundle from GitHub Releases and apply it (gated by CPE-372/373). Never errors the channel — a
/// failure comes back as `indexOk:false` with a message.
#[cfg(feature = "sidecar-platform")]
fn fetch_catalog_response(
    app: &tauri::AppHandle,
    params: &serde_json::Value,
) -> sidecar_contract::Response {
    use serde_json::json;
    // Pinned agents the sidecar asked us to skip (CPE-378).
    let pinned: Vec<String> = params
        .get("pinned")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    let body = do_fetch_catalog(app, &pinned)
        .unwrap_or_else(|e| json!({ "indexOk": false, "applied": [], "rejected": 0, "error": e }));
    sidecar_contract::Response { result: Ok(body) }
}

#[cfg(feature = "sidecar-platform")]
fn do_fetch_catalog(app: &tauri::AppHandle, pinned: &[String]) -> Result<serde_json::Value, String> {
    use serde_json::json;
    if keyverify::is_offline(std::env::var("CPE_OFFLINE").ok()) {
        return Ok(json!({ "indexOk": false, "applied": [], "rejected": 0, "offline": true }));
    }
    let keys: Vec<String> = CATALOG_TRUSTED_KEYS.iter().map(|s| s.to_string()).collect();
    let base = catalog_url();
    let dir = catalog_dir(app);
    let staging = std::env::temp_dir().join(format!("cpe-catalog-stage-{}", std::process::id()));
    std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;

    // Index + its detached signature.
    let index_bytes = catalog_http_get(&format!("{base}catalog-index.json"))?;
    let index_sig = catalog_http_get(&format!("{base}catalog-index.json.sig"))?;
    std::fs::write(staging.join("index.json"), &index_bytes).map_err(|e| e.to_string())?;
    std::fs::write(staging.join("index.json.sig"), &index_sig).map_err(|e| e.to_string())?;

    // Each listed manifest + its signature.
    let index = sidecar_host::catalog::CatalogIndex::from_json(&String::from_utf8_lossy(&index_bytes))?;
    for entry in &index.entries {
        let m = catalog_http_get(&format!("{base}{}.json", entry.id))?;
        let s = catalog_http_get(&format!("{base}{}.json.sig", entry.id))?;
        std::fs::write(staging.join(format!("{}.json", entry.id)), &m).map_err(|e| e.to_string())?;
        std::fs::write(staging.join(format!("{}.json.sig", entry.id)), &s).map_err(|e| e.to_string())?;
    }

    // Apply with anti-rollback against the persisted version map (last-known-good on failure).
    let vpath = dir.join("versions.json");
    let mut versions = sidecar_host::catalog::load_versions(&vpath);
    let report = sidecar_host::catalog::apply_bundle(&staging, &dir, &keys, &mut versions, pinned);
    let _ = sidecar_host::catalog::save_versions(&vpath, &versions);
    let _ = std::fs::remove_dir_all(&staging);
    Ok(json!({ "indexOk": report.index_ok, "applied": report.applied, "rejected": report.rejected.len() }))
}

/// One allow-listed HTTPS GET for a catalog asset (CPE-376), proxy/offline-aware (reuses CPE-369).
/// The host builds every URL from `catalog_url()` — the sidecar never supplies one (no SSRF).
#[cfg(feature = "sidecar-platform")]
fn catalog_http_get(url: &str) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let host = keyverify::host_of(url);
    let mut builder = ureq::AgentBuilder::new();
    if let Some(p) = keyverify::resolve_proxy(host, |k| std::env::var(k).ok()) {
        if let Ok(px) = ureq::Proxy::new(&p) {
            builder = builder.proxy(px);
        }
    }
    let resp = builder
        .build()
        .get(url)
        .timeout(std::time::Duration::from_secs(20))
        .call()
        .map_err(|e| format!("fetch failed: {e}"))?;
    let mut buf = Vec::new();
    resp.into_reader().read_to_end(&mut buf).map_err(|e| e.to_string())?;
    Ok(buf)
}

#[cfg(feature = "sidecar-platform")]
fn serve_ai_console_requests(
    mut conn: sidecar_host::supervisor::ProcessConnection,
    granted: std::collections::BTreeSet<sidecar_contract::Capability>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    storage_base: std::path::PathBuf,
    app: tauri::AppHandle,
) {
    use sidecar_contract::{Envelope, Message};
    use sidecar_host::conformance::SidecarChannel;

    let mut broker = sidecar_host::broker::Broker::new();
    // The keychain-backed secrets provider: Windows Credential Manager, macOS Keychain, or Linux
    // Secret Service (CPE-268/322). On any other target the broker simply has no secrets provider
    // and denies cleanly.
    #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
    broker.register_provider(Box::new(sidecar_host::providers::secrets::SecretsProvider::new(
        sidecar_host::providers::secrets::KeyringBackend,
    )));
    // Storage: a private per-sidecar directory the console persists its presets under (CPE-352).
    broker.register_provider(Box::new(sidecar_host::providers::storage::StorageProvider::new(
        storage_base,
    )));
    broker.set_grants("ai-console", granted);

    loop {
        if stop.load(std::sync::atomic::Ordering::SeqCst) {
            break;
        }
        match conn.recv() {
            Ok(env) => {
                let env_id = env.id;
                match env.message {
                    Message::Request(req) => {
                        // host.pick_folder is a host UI action, not a brokered capability — handle
                        // it directly by opening the native folder dialog (CPE-354).
                        let resp = if req.method == "host.pick_folder" {
                            pick_folder_response(&app, &req.params)
                        } else if req.method == "host.verify_key" {
                            // A live key check against an allow-listed provider endpoint (CPE-347),
                            // not a brokered capability — handle it directly.
                            verify_key_response(&req.params)
                        } else if req.method == "host.fetch_catalog" {
                            // Fetch + apply the signed catalog bundle from GitHub Releases (CPE-376).
                            fetch_catalog_response(&app, &req.params)
                        } else {
                            broker.dispatch("ai-console", &req)
                        };
                        if conn.send(&Envelope::new(env_id, Message::Response(resp))).is_err() {
                            break; // sidecar's stdin closed
                        }
                    }
                    // Agent Watch (CPE-396): the console announces session start/end as a
                    // `session:<json>` Status. Forward it to the frontend so the explorer can list
                    // active agent sessions and locate their Project folders.
                    Message::Event(sidecar_contract::Event::Status { state })
                        if state.starts_with("session:") =>
                    {
                        use tauri::Emitter;
                        let _ = app.emit("ai-console://session", state);
                    }
                    // Other non-request frames (Lifecycle, other Status) need no reply here.
                    _ => {}
                }
            }
            // A poll timeout is normal — loop to re-check `stop`. Anything else means the
            // sidecar closed the connection.
            Err(e) if e.contains("timed out") => continue,
            Err(_) => break,
        }
    }
}

#[cfg(feature = "sidecar-platform")]
impl Default for AiConsoleState {
    fn default() -> Self {
        Self {
            conn: std::sync::Mutex::new(None),
            // A small ring buffer: enough recent lines to diagnose a failed launch or
            // crash without growing without bound (CPE-298).
            logs: sidecar_host::observability::LogCapture::new(200),
            last_error: std::sync::Mutex::new(None),
        }
    }
}

#[cfg(feature = "sidecar-platform")]
impl AiConsoleState {
    /// Append a lifecycle log line for the AI Console.
    fn log(&self, level: sidecar_host::observability::LogLevel, message: impl Into<String>) {
        self.logs.push(sidecar_host::observability::LogRecord {
            correlation_id: 0,
            sidecar_id: "ai-console".into(),
            level,
            message: message.into(),
        });
    }

    /// Record (and log) the error that stopped the AI Console from starting, then return
    /// it so callers can `?`/propagate the same string to the frontend.
    fn fail(&self, message: impl Into<String>) -> String {
        let message = message.into();
        self.log(sidecar_host::observability::LogLevel::Error, message.clone());
        if let Ok(mut slot) = self.last_error.lock() {
            *slot = Some(message.clone());
        }
        message
    }

    /// Clear the last-error marker after a successful start.
    fn clear_error(&self) {
        if let Ok(mut slot) = self.last_error.lock() {
            *slot = None;
        }
    }
}

/// Locate the bundled `ai-console` sidecar binary. Order: explicit override env var, the
/// app's resource dir, then a dev-tree fallback. Returns an error string if not found so
/// the caller can degrade gracefully rather than panic.
#[cfg(feature = "sidecar-platform")]
fn resolve_ai_console_bin(app: &tauri::AppHandle) -> Result<String, String> {
    use tauri::Manager;
    let exe = if cfg!(windows) { "ai-console.exe" } else { "ai-console" };

    if let Ok(p) = std::env::var("CPE_AICONSOLE_BIN") {
        if Path::new(&p).exists() {
            return Ok(p);
        }
    }
    if let Ok(resource) = app.path().resource_dir() {
        let p = resource.join("sidecars").join(exe);
        if p.exists() {
            return Ok(p.to_string_lossy().into_owned());
        }
    }
    // Dev fallback: resolve relative to THIS crate (src-tauri) at compile time, not the
    // runtime CWD — `cargo tauri dev` runs the app with cwd = src-tauri, so a plain
    // relative path would miss. `../sidecar/ai-console/target/<profile>/<exe>`.
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    for profile in ["debug", "release"] {
        for base in [manifest.join("../sidecar/ai-console/target"), PathBuf::from("sidecar/ai-console/target")] {
            let p = base.join(profile).join(exe);
            if p.exists() {
                return Ok(p.to_string_lossy().into_owned());
            }
        }
    }
    Err(format!("ai-console binary ('{exe}') not found"))
}

/// Spawn (or reuse) the AI Console sidecar, complete the handshake, and return the URL of
/// the UI it serves so the frontend can mount it in an iframe pane (CPE-271). Non-fatal:
/// returns an error string that the UI surfaces, never panicking the explorer.
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn sidecar_start_ai_console(
    app: tauri::AppHandle,
    state: tauri::State<AiConsoleState>,
) -> Result<String, String> {
    use sidecar_contract::{Event, Message, CONTRACT_VERSION};
    use sidecar_host::conformance::SidecarChannel; // brings `.recv()` into scope
    use sidecar_host::observability::LogLevel;
    use sidecar_host::supervisor::{handshake, spawn_process_with_env};
    use tauri::Manager; // for app.path()

    // Respect the enable/disable toggle (CPE-274): a disabled sidecar must not start.
    if !sidecar_host::enablement::EnablementStore::load(&consent_dir(&app)?).is_enabled("ai-console") {
        return Err(state.fail("the AI Console is disabled"));
    }

    state.log(LogLevel::Info, "starting ai-console");
    let bin = resolve_ai_console_bin(&app).map_err(|e| state.fail(e))?;
    // Tell the sidecar where the (fetched) catalog lives + which key to trust, so it loads and can
    // reload verified updates (CPE-376). Empty keys until CPE-377 ⇒ nothing is trusted (dormant).
    let cat_dir = catalog_dir(&app);
    let _ = std::fs::create_dir_all(&cat_dir);
    let cat_dir_str = cat_dir.to_string_lossy().into_owned();
    let cat_keys = CATALOG_TRUSTED_KEYS.join(",");
    let cat_env = [
        ("CPE_AICONSOLE_CATALOG", cat_dir_str.as_str()),
        ("CPE_AICONSOLE_CATALOG_KEYS", cat_keys.as_str()),
    ];
    let mut conn =
        spawn_process_with_env(&bin, &[], &cat_env).map_err(|e| state.fail(format!("spawn failed: {e}")))?;
    let token = conn.launch_token().to_string();

    // Grant only what the user consented to (CPE-296). The frontend prompts for any
    // undecided capability before calling this; whatever isn't granted is simply withheld,
    // and the sidecar degrades gracefully (it still serves its UI, capability calls are
    // denied by the broker).
    let consented = sidecar_host::consent::ConsentStore::load(&consent_dir(&app)?).granted("ai-console");
    handshake(&mut conn, CONTRACT_VERSION, &consented, Some(&token))
        .map_err(|e| state.fail(format!("handshake failed: {e:?}")))?;
    state.log(LogLevel::Info, "handshake ok");

    // Read a bounded number of frames for the `ui:<url>` announcement.
    let mut url = None;
    for _ in 0..20 {
        match conn.recv() {
            Ok(env) => {
                if let Message::Event(Event::Status { state }) = env.message {
                    if let Some(u) = state.strip_prefix("ui:") {
                        url = Some(u.to_string());
                        break;
                    }
                }
            }
            Err(_) => break,
        }
    }
    let url = url.ok_or_else(|| state.fail("the AI Console did not announce a UI"))?;
    state.log(LogLevel::Info, format!("ui ready at {url}"));
    state.clear_error(); // a clean start clears any prior failure marker

    // Hand the connection to a servicing thread so the sidecar's capability requests (secrets
    // for the Keys panel) are actually answered (CPE-349). The control handle in state keeps
    // "running" = is_some() and lets stop/disable end it by dropping the handle.
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let thread_stop = stop.clone();
    // Host-owned base for the sidecar's private storage (presets etc.); the provider roots
    // each sidecar's dir under it by id.
    let storage_base = app
        .path()
        .app_data_dir()
        .map(|d| d.join("sidecar-storage"))
        .unwrap_or_else(|_| std::env::temp_dir().join("cpe-sidecar-storage"));
    let app_for_thread = app.clone();
    let thread = std::thread::spawn(move || {
        serve_ai_console_requests(conn, consented, thread_stop, storage_base, app_for_thread)
    });
    *state.conn.lock().map_err(|_| "state lock poisoned")? = Some(ConsoleConn { stop, _thread: thread });
    Ok(url)
}

/// One redacted log line in a diagnostics response (CPE-323).
#[cfg(feature = "sidecar-platform")]
#[derive(serde::Serialize)]
struct DiagLogLine {
    /// Severity, snake_case (`info` / `warn` / `error` / …).
    level: String,
    /// The log message, run through the redactor — never contains a secret.
    message: String,
}

/// A sidecar's health for the management panel (CPE-323): running state, the last error
/// that stopped it (if any), and recent log lines. Every string here is redacted.
#[cfg(feature = "sidecar-platform")]
#[derive(serde::Serialize)]
struct SidecarDiagnostics {
    id: String,
    running: bool,
    /// The last start/crash error, redacted, or `None` if the sidecar is healthy.
    last_error: Option<String>,
    /// Recent log lines, oldest first, already redacted.
    logs: Vec<DiagLogLine>,
}

/// Return a sidecar's last error and recent, REDACTED log lines for the management panel
/// (CPE-323). Only the AI Console currently produces live logs; other registered sidecars
/// return an empty (but valid) diagnostics record so the panel can render uniformly.
///
/// Redaction is defence-in-depth: every message runs through
/// [`Redactor::redact_log_line`], which masks registered secrets *and* heuristic secret
/// shapes (API-key prefixes, bearer tokens, `sensitive_key=value`), so a secret can never
/// surface here even if one reached a log line.
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn sidecar_diagnostics(
    id: String,
    state: tauri::State<AiConsoleState>,
) -> Result<SidecarDiagnostics, String> {
    use sidecar_host::observability::Redactor;

    let redactor = Redactor::new();
    let is_ai_console = id == "ai-console";
    let running = is_ai_console && state.conn.lock().map(|g| g.is_some()).unwrap_or(false);

    let last_error = if is_ai_console {
        state
            .last_error
            .lock()
            .ok()
            .and_then(|g| g.clone())
            .map(|e| redactor.redact_log_line(&e))
    } else {
        None
    };

    let logs = if is_ai_console {
        state
            .logs
            .recent()
            .into_iter()
            .map(|r| DiagLogLine {
                level: format!("{:?}", r.level).to_ascii_lowercase(),
                message: redactor.redact_log_line(&r.message),
            })
            .collect()
    } else {
        Vec::new()
    };

    Ok(SidecarDiagnostics { id, running, last_error, logs })
}

pub fn run() {
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init());

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        builder = builder
            .plugin(tauri_plugin_process::init())
            .plugin(tauri_plugin_updater::Builder::new().build())
            // Remember window size/position/maximized across restarts (CPE-228).
            // The plugin auto-restores each config window on launch and auto-saves
            // on exit, writing its own `.window-state.json`. Builder::default()
            // uses StateFlags::all(), so maximized state is restored too.
            .plugin(tauri_plugin_window_state::Builder::default().build());
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

    // Hold the AI Console sidecar connection in managed state (feature-gated).
    #[cfg(feature = "sidecar-platform")]
    {
        builder = builder.manage(AiConsoleState::default());
    }

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
            read_image_data_url,
            read_settings,
            write_settings,
            rename_entry,
            delete_to_trash,
            delete_permanent,
            can_restore_from_trash,
            restore_from_trash,
            copy_entries,
            move_entries,
            move_exact,
            entry_info,
            dir_size,
            git_remote_url,
            open_external,
            run_as_admin,
            extract_archive_entry,
            compress_to_zip,
            extract_archive,
            open_terminal,
            create_file,
            #[cfg(feature = "sidecar-platform")]
            sidecar_registry_ids,
            #[cfg(feature = "sidecar-platform")]
            sidecar_consent_state,
            #[cfg(feature = "sidecar-platform")]
            sidecar_set_consent,
            #[cfg(feature = "sidecar-platform")]
            sidecar_revoke_capability,
            #[cfg(feature = "sidecar-platform")]
            sidecar_details,
            #[cfg(feature = "sidecar-platform")]
            sidecar_stop,
            #[cfg(feature = "sidecar-platform")]
            sidecar_set_enabled,
            #[cfg(feature = "sidecar-platform")]
            sidecar_start_ai_console,
            #[cfg(feature = "sidecar-platform")]
            sidecar_diagnostics
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
    fn compress_then_extract_round_trips_files_and_folders() {
        let d = scratch("zip_roundtrip");
        // A file and a folder-with-nested-file to prove recursion and layout.
        let top = d.join("top.txt");
        fs::write(&top, b"top-level").unwrap();
        let sub = d.join("dir");
        fs::create_dir_all(sub.join("nested")).unwrap();
        fs::write(sub.join("a.txt"), b"alpha").unwrap();
        fs::write(sub.join("nested").join("b.txt"), b"beta").unwrap();

        // Compress both selections into one zip.
        let zip_path = d.join("out.zip");
        compress_to_zip(
            vec![
                top.to_string_lossy().to_string(),
                sub.to_string_lossy().to_string(),
            ],
            zip_path.to_string_lossy().to_string(),
        )
        .unwrap();

        // The archive lists the expected entries at the right paths.
        let names: Vec<String> = read_archive_entries(zip_path.to_string_lossy().to_string())
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert!(names.iter().any(|n| n == "top.txt"));
        assert!(names.iter().any(|n| n == "dir/a.txt"));
        assert!(names.iter().any(|n| n == "dir/nested/b.txt"));

        // Extract it back out and confirm the bytes survived.
        let out = d.join("unpacked");
        extract_archive(
            zip_path.to_string_lossy().to_string(),
            out.to_string_lossy().to_string(),
        )
        .unwrap();
        assert_eq!(fs::read_to_string(out.join("top.txt")).unwrap(), "top-level");
        assert_eq!(fs::read_to_string(out.join("dir").join("a.txt")).unwrap(), "alpha");
        assert_eq!(
            fs::read_to_string(out.join("dir").join("nested").join("b.txt")).unwrap(),
            "beta"
        );
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn compress_to_zip_rejects_an_empty_selection() {
        let d = scratch("zip_empty");
        let zip_path = d.join("empty.zip");
        assert!(compress_to_zip(vec![], zip_path.to_string_lossy().to_string()).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn extract_archive_unpacks_a_tar_gz() {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        let d = scratch("targz_extract");
        let tgz = d.join("bundle.tar.gz");
        {
            let f = fs::File::create(&tgz).unwrap();
            let enc = GzEncoder::new(f, Compression::default());
            let mut b = tar::Builder::new(enc);
            let data = b"packed";
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_entry_type(tar::EntryType::Regular);
            header.set_cksum();
            b.append_data(&mut header, "note.txt", &data[..]).unwrap();
            b.into_inner().unwrap().finish().unwrap();
        }
        let out = d.join("out");
        extract_archive(
            tgz.to_string_lossy().to_string(),
            out.to_string_lossy().to_string(),
        )
        .unwrap();
        assert_eq!(fs::read_to_string(out.join("note.txt")).unwrap(), "packed");
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
    fn sqlite_info_lists_tables_rows_and_columns() {
        use rusqlite::Connection;
        let d = scratch("sqlite");
        let f = d.join("test.db");
        {
            let conn = Connection::open(&f).unwrap();
            conn.execute_batch(
                "CREATE TABLE people (id INTEGER PRIMARY KEY, name TEXT);
                 INSERT INTO people (name) VALUES ('Ann'), ('Bo');",
            )
            .unwrap();
        }
        let info = sqlite_info(&f.to_string_lossy()).unwrap();
        assert!(info.contains("people (table) — 2 rows"), "table + row count: {info:?}");
        assert!(info.contains("columns: id, name"), "column list present");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn spreadsheet_info_renders_cells_as_a_grid() {
        use rust_xlsxwriter::Workbook;
        let d = scratch("xlsx");
        let f = d.join("book.xlsx");
        {
            let mut wb = Workbook::new();
            let sheet = wb.add_worksheet();
            sheet.write_string(0, 0, "Name").unwrap();
            sheet.write_string(0, 1, "Age").unwrap();
            sheet.write_string(1, 0, "Ann").unwrap();
            sheet.write_number(1, 1, 30.0).unwrap();
            wb.save(&f).unwrap();
        }
        let info = spreadsheet_info(&f.to_string_lossy()).unwrap();
        assert!(info.contains("Workbook — 1 sheet"), "sheet count: {info:?}");
        assert!(info.contains("Name\tAge"), "header row rendered tab-separated");
        assert!(info.contains("Ann\t30"), "data row rendered");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn parquet_info_errors_on_a_non_parquet() {
        let d = scratch("parquet_bad");
        let f = d.join("x.parquet");
        fs::write(&f, b"not a parquet file").unwrap();
        assert!(parquet_info(&f.to_string_lossy()).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn read_image_data_url_transcodes_tiff_to_png() {
        let d = scratch("tiff");
        let f = d.join("a.tiff");
        let mut img = image::RgbaImage::new(2, 2);
        img.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        img.save_with_format(&f, image::ImageFormat::Tiff).unwrap();
        let url = read_image_data_url(f.to_string_lossy().to_string()).unwrap();
        assert!(url.starts_with("data:image/png;base64,"), "returns a PNG data URL");
        assert!(url.len() > 40, "carries encoded bytes");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn read_image_data_url_errors_on_a_corrupt_psd() {
        let d = scratch("psd_bad");
        let f = d.join("a.psd");
        fs::write(&f, b"not a real psd").unwrap();
        assert!(read_image_data_url(f.to_string_lossy().to_string()).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn read_archive_entries_errors_on_a_non_iso() {
        let d = scratch("iso_bad");
        let f = d.join("x.iso");
        fs::write(&f, vec![0u8; 4096]).unwrap();
        assert!(read_archive_entries(f.to_string_lossy().to_string()).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn read_archive_entries_errors_on_a_non_7z() {
        let d = scratch("sevenz_bad");
        let f = d.join("x.7z");
        fs::write(&f, b"not a 7z archive").unwrap();
        assert!(read_archive_entries(f.to_string_lossy().to_string()).is_err());
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
    fn settings_round_trip_and_default_to_empty_object() {
        let d = scratch("settings");
        // Absent file → "{}" default (never errors on a fresh install).
        assert_eq!(read_settings_from(&d), "{}");
        // Write then read back the exact document.
        let doc = r#"{"cpe.view":"list","cpe.showHidden":true}"#;
        write_settings_to(&d, doc).unwrap();
        assert_eq!(read_settings_from(&d), doc);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn write_settings_creates_the_config_dir() {
        let d = scratch("settings_mkdir").join("nested/config");
        assert!(!d.exists());
        write_settings_to(&d, "{}").unwrap();
        assert!(d.join("settings.json").exists());
        let _ = fs::remove_dir_all(d.parent().unwrap().parent().unwrap());
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
    fn create_file_makes_an_empty_file() {
        let d = scratch("create_file");
        let created =
            create_file(d.to_string_lossy().to_string(), "New Text Document.txt".into()).unwrap();
        assert!(std::path::Path::new(&created).is_file());
        assert_eq!(fs::metadata(&created).unwrap().len(), 0, "file starts empty");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn create_file_refuses_to_clobber_existing_content() {
        let d = scratch("create_file_dup");
        let p = d.to_string_lossy().to_string();
        // Pre-existing file with content must not be truncated by a New file.
        fs::write(d.join("note.txt"), b"important").unwrap();
        assert!(create_file(p, "note.txt".into()).is_err());
        assert_eq!(fs::read_to_string(d.join("note.txt")).unwrap(), "important");
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
