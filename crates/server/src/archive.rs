//! Archive listing (CPE-064/109/110/113): browse an archive's directory without extracting it, for the
//! preview pane. Dispatches by extension across ZIP, TAR (± gzip), single-file gzip, 7-Zip, and ISO —
//! reading only the archive directory so it stays cheap even for large archives. Pure-Rust deps (zip /
//! tar / flate2 / sevenz-rust / iso9660), no system libs; extracted into the Server (CPE-815) as real
//! filesystem domain logic. The Tauri `read_archive_entries` command dispatches here.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;

/// One entry inside an archive, for the archive preview.
#[derive(Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ArchiveEntry {
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
}

/// List the entries of a ZIP archive without extracting it.
pub fn zip_entries(path: &str) -> Result<Vec<ArchiveEntry>, String> {
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

/// A single-file gzip (not a .tar.gz) has no directory. Report the decompressed file as one entry: its
/// name is the archive name minus `.gz`, and its size is the gzip trailer's ISIZE (uncompressed length
/// modulo 2^32).
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
    Ok(vec![ArchiveEntry { name, size, is_dir: false }])
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
                // `continue` here would infinite-loop (CPE-1411): the `iso9660` crate's directory
                // iterator does not advance its read cursor on a parse error (`ISODirectoryIterator::next`
                // only updates `next_offset` on `Ok`), so calling `.next()` again after an `Err` re-reads
                // the exact same bytes and gets the exact same `Err`, forever — a single malformed
                // directory record (e.g. a non-UTF8 identifier) hangs the whole listing thread. `break`
                // stops reading the REST of *this* directory's entries (same skip-what-we-can't-read
                // spirit as every other archive/filesystem reader here) while the outer
                // `while let Some(..) = stack.pop()` still visits any other directories already queued.
                Err(_) => break,
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
                    out.push(ArchiveEntry { name: format!("{full}/"), size: 0, is_dir: true });
                    stack.push((full, d));
                }
                DirectoryEntry::File(f) => {
                    let full = if prefix.is_empty() {
                        f.identifier.clone()
                    } else {
                        format!("{prefix}/{}", f.identifier)
                    };
                    out.push(ArchiveEntry { name: full, size: f.size() as u64, is_dir: false });
                }
            }
        }
    }
    Ok(out)
}

/// List an archive's entries without extracting it, for the preview pane. Dispatches by extension: ZIP
/// family (zip/jar/apk/…), TAR, gzip-compressed TAR (.tar.gz/.tgz), single-file gzip (.gz), 7-Zip, ISO,
/// RAR (listing only — see [`crate::rar`]; there is no extractor for it, so it's never routed to
/// `extract_archive*`/`pack_*`).
pub fn read_archive_entries(path: &str) -> Result<Vec<ArchiveEntry>, String> {
    let lower = path.to_lowercase();
    if lower.ends_with(".tar") {
        let file = fs::File::open(path).map_err(|e| e.to_string())?;
        tar_entries(file)
    } else if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        let file = fs::File::open(path).map_err(|e| e.to_string())?;
        tar_entries(flate2::read::GzDecoder::new(file))
    } else if lower.ends_with(".gz") {
        gzip_single_entry(path)
    } else if lower.ends_with(".7z") {
        sevenz_entries(path)
    } else if lower.ends_with(".iso") {
        iso_entries(path)
    } else if lower.ends_with(".rar") {
        crate::rar::rar_entries(path)
    } else {
        zip_entries(path)
    }
}

// ---------------------------------------------------------------------------
// Archive creation & extraction (CPE-251/252/242)
// ---------------------------------------------------------------------------

/// The flat temp-file target for a single-entry extraction: `%TEMP%/cpe-archive/<basename of inner>`
/// (CPE-242/1102/1180). Shared by every format's single-entry extractor so they all land in the same
/// place the frontend expects. Creates the directory; does not create the file itself.
/// Monotonic counter making each single-entry extraction land in its own temp subdir. Without this,
/// two concurrent extractions of same-named entries (e.g. two `a.txt`) shared one flat
/// `cpe-archive/<base>` path and raced — one call would read a file another had already replaced or
/// removed. That made `extract_archive_entry_any_delegates_zip_to_the_zip_extractor` flaky and it
/// failed deterministically on the macOS CI leg (CPE-1195); it's also a real app hazard for two
/// concurrent extract-and-opens of same-named files.
static EXTRACT_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn temp_extract_target(inner: &str) -> Result<std::path::PathBuf, String> {
    let base = Path::new(inner)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .ok_or_else(|| "invalid entry name".to_string())?;
    // Per-extraction unique subdir (pid + monotonic seq) so the basename is preserved for the opened
    // file while concurrent extractions can never collide.
    let seq = EXTRACT_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir()
        .join("cpe-archive")
        .join(format!("{}-{}", std::process::id(), seq));
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join(base))
}

/// Extract a single entry of a zip to a temp file and return its path (CPE-242). Read-only: the temp
/// copy is what opens, not the archived bytes.
pub fn extract_archive_entry(zip: &str, inner: &str) -> Result<String, String> {
    let file = fs::File::open(zip).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    // The frontend uses "/"; some zips store "\" — try the given name then the backslash variant.
    let backslashed = inner.replace('/', "\\");
    let idx = archive
        .index_for_name(inner)
        .or_else(|| archive.index_for_name(&backslashed))
        .ok_or_else(|| format!("entry not found: {inner}"))?;
    let mut entry = archive.by_index(idx).map_err(|e| e.to_string())?;

    let out = temp_extract_target(inner)?;
    let mut w = fs::File::create(&out).map_err(|e| e.to_string())?;
    std::io::copy(&mut entry, &mut w).map_err(|e| e.to_string())?;
    Ok(out.to_string_lossy().to_string())
}

/// Extract a single entry from a TAR stream (optionally gzip-decompressed by the caller) to `out`.
/// Returns whether the entry was found. Mirrors [`extract_archive_entry`]'s name matching: the frontend
/// uses "/"; some tarballs store "\" — try the given name then the backslash variant.
fn extract_tar_entry<R: std::io::Read>(reader: R, inner: &str, out: &Path) -> Result<bool, String> {
    let backslashed = inner.replace('/', "\\");
    let mut archive = tar::Archive::new(reader);
    for entry in archive.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let name = entry
            .path()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        if name == inner || name == backslashed {
            let mut w = fs::File::create(out).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut w).map_err(|e| e.to_string())?;
            return Ok(true);
        }
    }
    Ok(false)
}

/// Extract a single entry from a `.7z` to `out`. Returns whether the entry was found. Applies the same
/// [`entry_name_is_safe`] guard as [`extract_7z_safe`] to every entry name the archive claims, not just
/// the caller's `inner` (CPE-628/1180): `sevenz-rust` doesn't validate names itself.
fn extract_7z_entry(path: &str, inner: &str, out: &Path) -> Result<bool, String> {
    let backslashed = inner.replace('/', "\\");
    let mut found = false;
    // `decompress_file_with_extract_fn` needs an existing directory to build entry dest-paths against
    // even though our callback ignores that argument and writes straight to `out`.
    let scratch = out.parent().map(Path::to_path_buf).unwrap_or_else(std::env::temp_dir);
    sevenz_rust::decompress_file_with_extract_fn(path, &scratch, |entry, reader, _dest| {
        let name = entry.name();
        if !found && entry_name_is_safe(name) && (name == inner || name == backslashed) {
            let mut w = fs::File::create(out).map_err(sevenz_rust::Error::io)?;
            std::io::copy(reader, &mut w).map_err(sevenz_rust::Error::io)?;
            found = true;
            Ok(false) // stop scanning the rest of this block; we have what we need
        } else {
            Ok(true) // keep scanning
        }
    })
    .map_err(|e| e.to_string())?;
    Ok(found)
}

/// Extract a single entry from any supported non-zip archive format to a temp file and return its path
/// (CPE-1180): tar, gzip-compressed tar (.tar.gz/.tgz), and 7-Zip. Zip delegates to the existing
/// [`extract_archive_entry`] (kept separate since the `zip` crate already indexes by name efficiently).
/// Mirrors `extract_archive_entry`'s contract — a flat temp file at `%TEMP%/cpe-archive/<basename>` — so
/// the frontend's open-leaf flow doesn't need to know which extractor served it. `inner` is validated
/// with [`entry_name_is_safe`] up front so a path-traversal entry name is rejected before any extraction
/// is attempted, regardless of format.
pub fn extract_archive_entry_any(path: &str, inner: &str) -> Result<String, String> {
    if !entry_name_is_safe(inner) {
        return Err(format!("unsafe entry name: {inner}"));
    }
    let lower = path.to_lowercase();
    let out = temp_extract_target(inner)?;
    let found = if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        let file = fs::File::open(path).map_err(|e| e.to_string())?;
        extract_tar_entry(flate2::read::GzDecoder::new(file), inner, &out)?
    } else if lower.ends_with(".tar") {
        let file = fs::File::open(path).map_err(|e| e.to_string())?;
        extract_tar_entry(file, inner, &out)?
    } else if lower.ends_with(".7z") {
        extract_7z_entry(path, inner, &out)?
    } else if lower.ends_with(".rar") {
        // RAR has no ZIP-style directory; the STORED-entry extractor writes its own temp file below.
        return extract_rar_entry(path, inner);
    } else {
        return extract_archive_entry(path, inner);
    };
    if found {
        Ok(out.to_string_lossy().to_string())
    } else {
        Err(format!("entry not found: {inner}"))
    }
}

/// Extract a single **STORED** entry from a `.rar` to a temp file and return its path (CPE-1360). RAR's
/// compression is proprietary with no free decoder, so only uncompressed (STORE) entries can be served;
/// a compressed entry is a clean `Err` from [`crate::rar::rar_extract_entry`]. Mirrors
/// [`extract_archive_entry`]'s contract — a flat temp file at `%TEMP%/cpe-archive/<basename>` — so the
/// frontend's open-leaf / extract-for-preview flow doesn't need to know which extractor served it. The
/// entry name is [`entry_name_is_safe`]-validated before anything is written.
pub fn extract_rar_entry(path: &str, inner: &str) -> Result<String, String> {
    if !entry_name_is_safe(inner) {
        return Err(format!("unsafe entry name: {inner}"));
    }
    let bytes = crate::rar::rar_extract_entry(path, inner)?;
    let out = temp_extract_target(inner)?;
    fs::write(&out, &bytes).map_err(|e| e.to_string())?;
    Ok(out.to_string_lossy().to_string())
}

/// Recursively add `src` to an open zip under the archive path `name_in_zip`. Directories become explicit
/// entries so empty folders survive the round trip. Never packs the output archive into itself (CPE-632).
fn zip_add_path(
    writer: &mut zip::ZipWriter<fs::File>,
    src: &Path,
    name_in_zip: &str,
    opts: zip::write::FileOptions<'_, ()>,
    skip: Option<&Path>,
) -> Result<(), String> {
    if let (Some(skip), Ok(canon)) = (skip, src.canonicalize()) {
        if canon == skip {
            return Ok(());
        }
    }
    let meta = fs::symlink_metadata(src).map_err(|e| e.to_string())?;
    if meta.is_dir() {
        writer.add_directory(format!("{name_in_zip}/"), opts).map_err(|e| e.to_string())?;
        let mut children: Vec<_> = fs::read_dir(src).map_err(|e| e.to_string())?.filter_map(|e| e.ok()).collect();
        children.sort_by_key(|e| e.file_name());
        for child in children {
            let child_name = child.file_name().to_string_lossy().to_string();
            zip_add_path(writer, &child.path(), &format!("{name_in_zip}/{child_name}"), opts, skip)?;
        }
    } else {
        writer.start_file(name_in_zip, opts).map_err(|e| e.to_string())?;
        let mut f = fs::File::open(src).map_err(|e| e.to_string())?;
        std::io::copy(&mut f, writer).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Pack the given files/folders into a new deflated `.zip` at `dest` (CPE-251). Returns the created path.
pub fn compress_to_zip(paths: &[String], dest: &str) -> Result<String, String> {
    if paths.is_empty() {
        return Err("nothing to compress".into());
    }
    let file = fs::File::create(dest).map_err(|e| e.to_string())?;
    let mut writer = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    // Canonical path of the output archive so the walk can skip it if it sits inside a source (CPE-632).
    let dest_canon = Path::new(dest).canonicalize().ok();
    for p in paths {
        let src = Path::new(p);
        let name = src
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .ok_or_else(|| format!("invalid path: {p}"))?;
        zip_add_path(&mut writer, src, &name, opts, dest_canon.as_deref())?;
    }
    writer.finish().map_err(|e| e.to_string())?;
    Ok(dest.to_string())
}

/// Create a valid *empty* zip archive at `dest` (CPE-1161). An empty file is NOT a valid `.zip` — a
/// real archive needs at least the End-Of-Central-Directory record, which `ZipWriter::finish` writes.
/// Uses `create_new` so it fails atomically rather than clobbering an existing file (mirrors the
/// explorer's `create_file`). Returns the created path.
pub fn create_empty_zip(dest: &str) -> Result<String, String> {
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dest)
        .map_err(|e| e.to_string())?;
    let writer = zip::ZipWriter::new(file);
    writer.finish().map_err(|e| e.to_string())?;
    Ok(dest.to_string())
}

/// Recursively add `src` to a tar builder as `name_in_tar`, adding directory entries so empty folders
/// survive. Never packs the output archive into itself (CPE-632) — mirrors [`zip_add_path`].
fn tar_add_path<W: std::io::Write>(
    builder: &mut tar::Builder<W>,
    src: &Path,
    name_in_tar: &str,
    skip: Option<&Path>,
) -> Result<(), String> {
    if let (Some(skip), Ok(canon)) = (skip, src.canonicalize()) {
        if canon == skip {
            return Ok(());
        }
    }
    let meta = fs::symlink_metadata(src).map_err(|e| e.to_string())?;
    if meta.is_dir() {
        builder.append_dir(name_in_tar, src).map_err(|e| e.to_string())?;
        let mut children: Vec<_> = fs::read_dir(src).map_err(|e| e.to_string())?.filter_map(|e| e.ok()).collect();
        children.sort_by_key(|e| e.file_name());
        for child in children {
            let child_name = child.file_name().to_string_lossy().to_string();
            tar_add_path(builder, &child.path(), &format!("{name_in_tar}/{child_name}"), skip)?;
        }
    } else {
        builder.append_path_with_name(src, name_in_tar).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Pack the given files/folders into a new gzip-compressed tarball at `dest` (CPE-908). Returns the path.
pub fn compress_to_targz(paths: &[String], dest: &str) -> Result<String, String> {
    if paths.is_empty() {
        return Err("nothing to compress".into());
    }
    let file = fs::File::create(dest).map_err(|e| e.to_string())?;
    let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    let mut builder = tar::Builder::new(encoder);
    let dest_canon = Path::new(dest).canonicalize().ok();
    for p in paths {
        let src = Path::new(p);
        let name = src
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .ok_or_else(|| format!("invalid path: {p}"))?;
        tar_add_path(&mut builder, src, &name, dest_canon.as_deref())?;
    }
    builder.into_inner().map_err(|e| e.to_string())?.finish().map_err(|e| e.to_string())?;
    Ok(dest.to_string())
}

/// Pack files/folders into `dest`, choosing the format by `dest`'s extension: `.zip` → zip,
/// `.tar.gz`/`.tgz` → gzip tarball (CPE-908). An unrecognised extension is a clear error.
pub fn compress_archive(paths: &[String], dest: &str) -> Result<String, String> {
    let lower = dest.to_ascii_lowercase();
    if lower.ends_with(".zip") {
        compress_to_zip(paths, dest)
    } else if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        compress_to_targz(paths, dest)
    } else {
        Err(format!("unsupported archive format for '{dest}' (use .zip or .tar.gz)"))
    }
}

/// Pack files/folders into a **password-protected** (AES-256) `.zip` at `dest` (CPE-909). Returns the path.
/// Reading it back requires the same password — see [`extract_zip_encrypted`].
pub fn compress_to_zip_encrypted(paths: &[String], dest: &str, password: &str) -> Result<String, String> {
    if paths.is_empty() {
        return Err("nothing to compress".into());
    }
    if password.is_empty() {
        return Err("a password is required for an encrypted archive".into());
    }
    let file = fs::File::create(dest).map_err(|e| e.to_string())?;
    let mut writer = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .with_aes_encryption(zip::AesMode::Aes256, password);
    let dest_canon = Path::new(dest).canonicalize().ok();
    for p in paths {
        let src = Path::new(p);
        let name = src
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .ok_or_else(|| format!("invalid path: {p}"))?;
        zip_add_path(&mut writer, src, &name, opts, dest_canon.as_deref())?;
    }
    writer.finish().map_err(|e| e.to_string())?;
    Ok(dest.to_string())
}

/// Extract a password-protected `.zip` at `path` into `dest` with `password` (CPE-909). A wrong password
/// is a clear error; entries are zip-slip-guarded ([`entry_name_is_safe`]) like the plain extractor.
pub fn extract_zip_encrypted(path: &str, dest: &str, password: &str) -> Result<String, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let dest_path = Path::new(dest);
    fs::create_dir_all(dest_path).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index_decrypt(i, password.as_bytes()).map_err(|e| e.to_string())?;
        let name = entry.name().to_string();
        if !entry_name_is_safe(&name) {
            continue; // skip a zip-slip entry, keep extracting the rest
        }
        let out = dest_path.join(&name);
        if entry.is_dir() {
            fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut f = fs::File::create(&out).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut f).map_err(|e| e.to_string())?;
        }
    }
    Ok(dest.to_string())
}

/// Unpack a tar stream into `dest`.
fn tar_unpack<R: std::io::Read>(reader: R, dest: &Path) -> Result<(), String> {
    let mut archive = tar::Archive::new(reader);
    archive.unpack(dest).map_err(|e| e.to_string())
}

/// True if an archive entry name is a plain relative path that cannot escape the extraction root — the
/// shared "zip-slip" guard for extractors that don't provide one (CPE-628). `\` is normalised to `/`.
/// `pub(crate)` so [`crate::extract_plan`] can reuse it rather than duplicating the check (CPE-1055).
pub(crate) fn entry_name_is_safe(name: &str) -> bool {
    use std::path::Component;
    if name.is_empty() {
        return false;
    }
    let normalized = name.replace('\\', "/");
    let p = Path::new(&normalized);
    !p.is_absolute() && p.components().all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

/// Extract a `.7z` into `dest` **safely**: `sevenz-rust` 0.6 doesn't check path traversal, so validate
/// each entry with [`entry_name_is_safe`] and skip any that isn't a plain relative path (CPE-628).
fn extract_7z_safe(src: &Path, dest: &Path) -> Result<(), String> {
    sevenz_rust::decompress_file_with_extract_fn(src, dest, |entry, reader, entry_dest| {
        if entry_name_is_safe(entry.name()) {
            sevenz_rust::default_entry_extract_fn(entry, reader, entry_dest)
        } else {
            Ok(true) // skip the unsafe entry; keep extracting the rest
        }
    })
    .map_err(|e| e.to_string())
}

/// Extract an archive into `dest`, which is created if missing (CPE-252). Dispatched by extension. Every
/// format is guarded against zip-slip: zip via `enclosed_name`, tar via the crate's checked `unpack`, 7z
/// via [`extract_7z_safe`].
pub fn extract_archive(path: &str, dest: &str) -> Result<String, String> {
    fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let dest_path = Path::new(dest);
    let lower = path.to_lowercase();

    if lower.ends_with(".tar") {
        let file = fs::File::open(path).map_err(|e| e.to_string())?;
        tar_unpack(file, dest_path)?;
    } else if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        let file = fs::File::open(path).map_err(|e| e.to_string())?;
        tar_unpack(flate2::read::GzDecoder::new(file), dest_path)?;
    } else if lower.ends_with(".gz") {
        // A bare .gz holds a single file; its name is the archive name minus .gz.
        let stem = Path::new(path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "extracted".to_string());
        let file = fs::File::open(path).map_err(|e| e.to_string())?;
        let mut decoder = flate2::read::GzDecoder::new(file);
        let mut out = fs::File::create(dest_path.join(stem)).map_err(|e| e.to_string())?;
        std::io::copy(&mut decoder, &mut out).map_err(|e| e.to_string())?;
    } else if lower.ends_with(".7z") {
        extract_7z_safe(Path::new(path), dest_path)?;
    } else {
        // zip family: the crate's extractor guards against traversal via ZipFile::enclosed_name.
        let file = fs::File::open(path).map_err(|e| e.to_string())?;
        let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
        archive.extract(dest_path).map_err(|e| e.to_string())?;
    }
    Ok(dest.to_string())
}

// ---------------------------------------------------------------------------
// Queue-routed compress/extract with progress + cancel (CPE-1184, epic CPE-705)
// ---------------------------------------------------------------------------
//
// `compress_archive`/`extract_archive` above are one-shot, blocking, all-or-nothing calls — fine for a
// small archive, but a large one freezes the UI (no progress, no cancel), against the streaming-liveness
// convention. The functions below are the queue-routed siblings: item-level progress (one tick per
// archive entry — cheap and enough to keep a progress bar honest without rewriting every writer to
// chunk mid-entry) plus a `cancel` flag polled between entries, so the app-adapter
// (`start_archive_compress`/`start_archive_extract` in `src-tauri/src/lib.rs`) can run them on a
// background thread and forward progress as the *same* `transfer://progress`/`transfer://done` events
// the copy/move transfer engine already uses — archive ops land in the same operations panel, are
// cancellable the same way (the shared `TRANSFER_CANCELS` registry), and stay Tauri-free here per
// SERVER-ARCHITECTURE.md. The original one-shot functions above are untouched (still used by
// `extract_archive_entry`/`extract_archive_entry_any`'s single-leaf reads and anyone else calling them),
// so nothing about their tested behaviour changes.

/// A progress snapshot emitted while a compress/extract runs. Mirrors the transfer engine's
/// `TransferProgress` shape minus the app-assigned `id` (a pure app/session concern the Tauri adapter
/// attaches when it forwards this as a `transfer://progress` event).
#[derive(Clone, Debug, Default)]
pub struct ArchiveProgress {
    pub total_bytes: u64,
    pub done_bytes: u64,
    pub total_items: u64,
    pub done_items: u64,
    pub current: String,
}

/// The final outcome of a compress/extract run. `done` counts entries actually written (an entry
/// skipped for failing the zip-slip guard is neither done nor failed — it's recorded in `errors`,
/// mirroring the silent-skip the one-shot extractors already do); `failed` stays 0 unless the whole run
/// aborted with an error (compress/extract are otherwise all-or-nothing, same as the one-shot functions).
#[derive(Clone, Debug, Default)]
pub struct ArchiveReport {
    pub done: u64,
    pub failed: u64,
    pub cancelled: bool,
    pub errors: Vec<String>,
}

/// One item queued for an archive write — either a directory placeholder or a file with known size,
/// discovered by [`collect_archive_sources`]'s recursive walk.
struct ArchiveSourceEntry {
    src: PathBuf,
    /// The archive-internal path (forward-slash, relative).
    name: String,
    is_dir: bool,
    size: u64,
}

/// Recursively enumerate `paths` into a flat list of entries to add to an archive — the same walk order
/// (a directory entry, then its children sorted by name) [`zip_add_path`]/[`tar_add_path`] use, just
/// flattened so a streamed writer can check `cancel` and emit progress between entries instead of only
/// at the end of a deep recursion. Skips `skip` (the output archive's own canonical path) so it never
/// packs itself (CPE-632), same guard the original recursive adders use.
fn collect_archive_sources(paths: &[String], skip: Option<&Path>) -> Result<Vec<ArchiveSourceEntry>, String> {
    fn walk(src: &Path, name: &str, skip: Option<&Path>, out: &mut Vec<ArchiveSourceEntry>) -> Result<(), String> {
        if let (Some(skip), Ok(canon)) = (skip, src.canonicalize()) {
            if canon == skip {
                return Ok(());
            }
        }
        let meta = fs::symlink_metadata(src).map_err(|e| e.to_string())?;
        if meta.is_dir() {
            out.push(ArchiveSourceEntry { src: src.to_path_buf(), name: name.to_string(), is_dir: true, size: 0 });
            let mut children: Vec<_> = fs::read_dir(src).map_err(|e| e.to_string())?.filter_map(|e| e.ok()).collect();
            children.sort_by_key(|e| e.file_name());
            for child in children {
                let child_name = child.file_name().to_string_lossy().to_string();
                walk(&child.path(), &format!("{name}/{child_name}"), skip, out)?;
            }
        } else {
            out.push(ArchiveSourceEntry { src: src.to_path_buf(), name: name.to_string(), is_dir: false, size: meta.len() });
        }
        Ok(())
    }
    let mut out = Vec::new();
    for p in paths {
        let src = Path::new(p);
        let name = src.file_name().map(|s| s.to_string_lossy().to_string()).ok_or_else(|| format!("invalid path: {p}"))?;
        walk(src, &name, skip, &mut out)?;
    }
    Ok(out)
}

/// Pack `paths` into a new `.zip` at `dest` — the streamed sibling of [`compress_to_zip`]: `cancel` is
/// polled between entries and `emit` receives a progress snapshot after each. On cancellation the
/// writer still finishes with whatever entries were already added, producing a valid but incomplete
/// archive — mirrors the transfer engine's "leave the partial result, don't delete" choice.
pub fn compress_to_zip_streamed(
    paths: &[String],
    dest: &str,
    cancel: &AtomicBool,
    mut emit: impl FnMut(&ArchiveProgress),
) -> Result<ArchiveReport, String> {
    if paths.is_empty() {
        return Err("nothing to compress".into());
    }
    let dest_canon = Path::new(dest).canonicalize().ok();
    let entries = collect_archive_sources(paths, dest_canon.as_deref())?;
    let total_bytes = entries.iter().map(|e| e.size).sum();
    let total_items = entries.iter().filter(|e| !e.is_dir).count() as u64;

    let file = fs::File::create(dest).map_err(|e| e.to_string())?;
    let mut writer = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let mut prog = ArchiveProgress { total_bytes, done_bytes: 0, total_items, done_items: 0, current: String::new() };
    let mut cancelled = false;
    emit(&prog);
    for e in &entries {
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }
        prog.current = e.name.clone();
        if e.is_dir {
            writer.add_directory(format!("{}/", e.name), opts).map_err(|err| err.to_string())?;
        } else {
            writer.start_file(&e.name, opts).map_err(|err| err.to_string())?;
            let mut f = fs::File::open(&e.src).map_err(|err| err.to_string())?;
            std::io::copy(&mut f, &mut writer).map_err(|err| err.to_string())?;
            prog.done_bytes += e.size;
            prog.done_items += 1;
        }
        emit(&prog);
    }
    prog.current.clear();
    emit(&prog);
    writer.finish().map_err(|e| e.to_string())?;
    Ok(ArchiveReport { done: prog.done_items, failed: 0, cancelled, errors: Vec::new() })
}

/// Pack `paths` into a new gzip-compressed tarball at `dest` — the streamed sibling of
/// [`compress_to_targz`]. Same cancel/progress/partial-result contract as [`compress_to_zip_streamed`].
pub fn compress_to_targz_streamed(
    paths: &[String],
    dest: &str,
    cancel: &AtomicBool,
    mut emit: impl FnMut(&ArchiveProgress),
) -> Result<ArchiveReport, String> {
    if paths.is_empty() {
        return Err("nothing to compress".into());
    }
    let dest_canon = Path::new(dest).canonicalize().ok();
    let entries = collect_archive_sources(paths, dest_canon.as_deref())?;
    let total_bytes = entries.iter().map(|e| e.size).sum();
    let total_items = entries.iter().filter(|e| !e.is_dir).count() as u64;

    let file = fs::File::create(dest).map_err(|e| e.to_string())?;
    let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    let mut builder = tar::Builder::new(encoder);

    let mut prog = ArchiveProgress { total_bytes, done_bytes: 0, total_items, done_items: 0, current: String::new() };
    let mut cancelled = false;
    emit(&prog);
    for e in &entries {
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }
        prog.current = e.name.clone();
        if e.is_dir {
            builder.append_dir(&e.name, &e.src).map_err(|err| err.to_string())?;
        } else {
            builder.append_path_with_name(&e.src, &e.name).map_err(|err| err.to_string())?;
            prog.done_bytes += e.size;
            prog.done_items += 1;
        }
        emit(&prog);
    }
    prog.current.clear();
    emit(&prog);
    builder.into_inner().map_err(|e| e.to_string())?.finish().map_err(|e| e.to_string())?;
    Ok(ArchiveReport { done: prog.done_items, failed: 0, cancelled, errors: Vec::new() })
}

/// Pack `paths` into `dest`, choosing the format by extension (`.zip` / `.tar.gz` / `.tgz`) — the
/// streamed sibling of [`compress_archive`], used by `start_archive_compress`.
pub fn compress_archive_streamed(
    paths: &[String],
    dest: &str,
    cancel: &AtomicBool,
    emit: impl FnMut(&ArchiveProgress),
) -> Result<ArchiveReport, String> {
    let lower = dest.to_ascii_lowercase();
    if lower.ends_with(".zip") {
        compress_to_zip_streamed(paths, dest, cancel, emit)
    } else if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        compress_to_targz_streamed(paths, dest, cancel, emit)
    } else {
        Err(format!("unsupported archive format for '{dest}' (use .zip or .tar.gz)"))
    }
}

/// Pack `paths` into a password-protected (AES-256) `.zip` at `dest` — the streamed sibling of
/// [`compress_to_zip_encrypted`], used by `start_archive_compress` when a password is given.
pub fn compress_to_zip_encrypted_streamed(
    paths: &[String],
    dest: &str,
    password: &str,
    cancel: &AtomicBool,
    mut emit: impl FnMut(&ArchiveProgress),
) -> Result<ArchiveReport, String> {
    if paths.is_empty() {
        return Err("nothing to compress".into());
    }
    if password.is_empty() {
        return Err("a password is required for an encrypted archive".into());
    }
    let dest_canon = Path::new(dest).canonicalize().ok();
    let entries = collect_archive_sources(paths, dest_canon.as_deref())?;
    let total_bytes = entries.iter().map(|e| e.size).sum();
    let total_items = entries.iter().filter(|e| !e.is_dir).count() as u64;

    let file = fs::File::create(dest).map_err(|e| e.to_string())?;
    let mut writer = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .with_aes_encryption(zip::AesMode::Aes256, password);

    let mut prog = ArchiveProgress { total_bytes, done_bytes: 0, total_items, done_items: 0, current: String::new() };
    let mut cancelled = false;
    emit(&prog);
    for e in &entries {
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }
        prog.current = e.name.clone();
        if e.is_dir {
            writer.add_directory(format!("{}/", e.name), opts).map_err(|err| err.to_string())?;
        } else {
            writer.start_file(&e.name, opts).map_err(|err| err.to_string())?;
            let mut f = fs::File::open(&e.src).map_err(|err| err.to_string())?;
            std::io::copy(&mut f, &mut writer).map_err(|err| err.to_string())?;
            prog.done_bytes += e.size;
            prog.done_items += 1;
        }
        emit(&prog);
    }
    prog.current.clear();
    emit(&prog);
    writer.finish().map_err(|e| e.to_string())?;
    Ok(ArchiveReport { done: prog.done_items, failed: 0, cancelled, errors: Vec::new() })
}

/// Quick, cheap check of whether `path` (a zip) needs `password` to read — or needs *some* password
/// when `password` is `None` — without extracting anything (CPE-1184). Reads just entry 0's header
/// (the `zip` crate needs the password to even set up an AES entry's reader, so this fails immediately
/// for a wrong/missing password, the same error the old one-shot `extract_zip_encrypted`/`extract_archive`
/// surfaced). `start_archive_extract` calls this synchronously *before* queuing the background
/// extraction, so the frontend's password-prompt-and-retry flow keeps its original synchronous
/// try/catch shape instead of round-tripping through a transfer id + completion event for what's
/// normally an instant rejection.
pub fn check_zip_password(path: &str, password: Option<&str>) -> Result<(), String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    if archive.is_empty() {
        return Ok(());
    }
    let result = match password {
        Some(pw) => archive.by_index_decrypt(0, pw.as_bytes()).map(|_| ()),
        None => archive.by_index(0).map(|_| ()),
    };
    result.map_err(|e| e.to_string())
}

/// Extract a plain (unencrypted) zip into `dest`, streamed — the manual-loop sibling the zip branch of
/// [`extract_archive`] delegates to via [`extract_archive_streamed`]. Per-entry zip-slip guard mirrors
/// [`extract_zip_encrypted`]'s (skip an unsafe name rather than aborting), so this doesn't regress the
/// zip crate's own `enclosed_name` guard the one-shot `extract_archive` relies on — both are "safe",
/// per the `zip_extraction_does_not_escape_the_destination` test below, which tolerates either an
/// outright error or a silent skip.
fn extract_zip_stream(
    path: &str,
    dest: &Path,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(&ArchiveProgress),
) -> Result<ArchiveReport, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    extract_zip_archive_stream(&mut archive, dest, None, cancel, emit)
}

/// Shared zip-extraction loop for both the plain and password-protected streamed extractors: iterate
/// entries, skip a zip-slip name, otherwise write it out, checking `cancel` and emitting progress
/// between entries.
fn extract_zip_archive_stream(
    archive: &mut zip::ZipArchive<fs::File>,
    dest: &Path,
    password: Option<&str>,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(&ArchiveProgress),
) -> Result<ArchiveReport, String> {
    fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let total_items = archive.len() as u64;
    let mut prog = ArchiveProgress { total_bytes: 0, done_bytes: 0, total_items, done_items: 0, current: String::new() };
    let mut report = ArchiveReport::default();
    emit(&prog);
    for i in 0..archive.len() {
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            break;
        }
        let mut entry = match password {
            Some(pw) => archive.by_index_decrypt(i, pw.as_bytes()).map_err(|e| e.to_string())?,
            None => archive.by_index(i).map_err(|e| e.to_string())?,
        };
        let name = entry.name().to_string();
        prog.current = name.clone();
        if !entry_name_is_safe(&name) {
            report.errors.push(format!("{name}: unsafe entry name, skipped"));
            prog.done_items += 1;
            emit(&prog);
            continue;
        }
        let out = dest.join(&name);
        if entry.is_dir() {
            fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut f = fs::File::create(&out).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut f).map_err(|e| e.to_string())?;
            prog.done_bytes += entry.size();
            report.done += 1; // only files count toward "done" — a dir is a placeholder, not content
        }
        prog.done_items += 1;
        emit(&prog);
    }
    prog.current.clear();
    emit(&prog);
    Ok(report)
}

/// Extract a password-protected `.zip` at `path` into `dest` with `password`, streamed — the sibling of
/// [`extract_zip_encrypted`], used by `start_archive_extract` when a password is given.
pub fn extract_zip_encrypted_streamed(
    path: &str,
    dest: &str,
    password: &str,
    cancel: &AtomicBool,
    mut emit: impl FnMut(&ArchiveProgress),
) -> Result<ArchiveReport, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    extract_zip_archive_stream(&mut archive, Path::new(dest), Some(password), cancel, &mut emit)
}

/// Totals (bytes, item count) for a tar/tar.gz stream, from a cheap first-pass listing — TAR has no
/// central directory, so a byte/item total for the progress bar needs one read before the real
/// streamed-extraction pass reopens the file. Best-effort: an unreadable/corrupt archive just yields
/// `(0, 0)`, so the real pass's own error is what the caller ultimately sees.
fn tar_totals(path: &str, gz: bool) -> (u64, u64) {
    let read = || -> Result<Vec<ArchiveEntry>, String> {
        let file = fs::File::open(path).map_err(|e| e.to_string())?;
        if gz {
            tar_entries(flate2::read::GzDecoder::new(file))
        } else {
            tar_entries(file)
        }
    };
    match read() {
        Ok(entries) => {
            let total_bytes = entries.iter().filter(|e| !e.is_dir).map(|e| e.size).sum();
            let total_items = entries.iter().filter(|e| !e.is_dir).count() as u64;
            (total_bytes, total_items)
        }
        Err(_) => (0, 0),
    }
}

/// Extract a tar stream into `dest`, streamed: each entry is unpacked via `unpack_in`, the tar crate's
/// own path-safety-checked writer — the exact same per-entry call `Archive::unpack` (the one-shot
/// [`extract_archive`]'s tar path) makes internally, so this is not a behaviour change, just the same
/// work done one entry at a time so `cancel`/`emit` can run between them.
fn extract_tar_stream<R: std::io::Read>(
    reader: R,
    dest: &Path,
    total_bytes: u64,
    total_items: u64,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(&ArchiveProgress),
) -> Result<ArchiveReport, String> {
    let mut archive = tar::Archive::new(reader);
    let mut prog = ArchiveProgress { total_bytes, done_bytes: 0, total_items, done_items: 0, current: String::new() };
    let mut report = ArchiveReport::default();
    emit(&prog);
    let entries = archive.entries().map_err(|e| e.to_string())?;
    for entry in entries {
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            break;
        }
        let mut entry = entry.map_err(|e| e.to_string())?;
        let is_dir = entry.header().entry_type().is_dir();
        let size = entry.header().size().unwrap_or(0);
        let name = entry.path().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
        prog.current = name.clone();
        let unpacked = entry.unpack_in(dest).map_err(|e| e.to_string())?;
        if unpacked {
            if !is_dir {
                report.done += 1;
                prog.done_bytes += size;
                prog.done_items += 1;
            }
        } else {
            report.errors.push(format!("{name}: unsafe entry name, skipped"));
        }
        emit(&prog);
    }
    prog.current.clear();
    emit(&prog);
    Ok(report)
}

/// Extract a `.7z` into `dest`, streamed: `decompress_file_with_extract_fn`'s per-entry callback lets us
/// check `cancel` and emit progress between entries, applying the same [`entry_name_is_safe`] guard
/// [`extract_7z_safe`] does. Returning `Ok(false)` on a cancel stops the scan cooperatively (no error) —
/// the same "stop scanning" outcome [`extract_7z_entry`] already uses for its own early-stop case.
fn extract_7z_stream(
    path: &str,
    dest: &Path,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(&ArchiveProgress),
) -> Result<ArchiveReport, String> {
    let (total_bytes, total_items) = match sevenz_entries(path) {
        Ok(entries) => (
            entries.iter().filter(|e| !e.is_dir).map(|e| e.size).sum(),
            entries.iter().filter(|e| !e.is_dir).count() as u64,
        ),
        Err(_) => (0, 0),
    };
    let mut prog = ArchiveProgress { total_bytes, done_bytes: 0, total_items, done_items: 0, current: String::new() };
    let mut report = ArchiveReport::default();
    emit(&prog);
    sevenz_rust::decompress_file_with_extract_fn(path, dest, |entry, reader, entry_dest| {
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            return Ok(false); // cooperative stop, not an error
        }
        let name = entry.name().to_string();
        let size = entry.size();
        let safe = entry_name_is_safe(&name);
        prog.current = name.clone();
        let outcome =
            if safe { sevenz_rust::default_entry_extract_fn(entry, reader, entry_dest) } else { Ok(true) };
        if outcome.is_ok() {
            if safe {
                prog.done_bytes += size;
                prog.done_items += 1;
                report.done += 1;
            } else {
                report.errors.push(format!("{name}: unsafe entry name, skipped"));
            }
            emit(&prog);
        }
        outcome
    })
    .map_err(|e| e.to_string())?;
    prog.current.clear();
    emit(&prog);
    Ok(report)
}

/// Extract an archive into `dest`, streamed — the sibling of [`extract_archive`], used by
/// `start_archive_extract`. Dispatched by extension exactly like the one-shot version; `dest` is
/// created the same way.
pub fn extract_archive_streamed(
    path: &str,
    dest: &str,
    cancel: &AtomicBool,
    mut emit: impl FnMut(&ArchiveProgress),
) -> Result<ArchiveReport, String> {
    fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let dest_path = Path::new(dest);
    let lower = path.to_lowercase();

    if lower.ends_with(".tar") {
        let (total_bytes, total_items) = tar_totals(path, false);
        let file = fs::File::open(path).map_err(|e| e.to_string())?;
        extract_tar_stream(file, dest_path, total_bytes, total_items, cancel, &mut emit)
    } else if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        let (total_bytes, total_items) = tar_totals(path, true);
        let file = fs::File::open(path).map_err(|e| e.to_string())?;
        extract_tar_stream(flate2::read::GzDecoder::new(file), dest_path, total_bytes, total_items, cancel, &mut emit)
    } else if lower.ends_with(".gz") {
        // A bare .gz holds a single file — one item, no per-entry granularity possible.
        let stem = Path::new(path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "extracted".to_string());
        let mut prog = ArchiveProgress { total_bytes: 0, done_bytes: 0, total_items: 1, done_items: 0, current: stem.clone() };
        emit(&prog);
        let file = fs::File::open(path).map_err(|e| e.to_string())?;
        let mut decoder = flate2::read::GzDecoder::new(file);
        let mut out = fs::File::create(dest_path.join(&stem)).map_err(|e| e.to_string())?;
        std::io::copy(&mut decoder, &mut out).map_err(|e| e.to_string())?;
        prog.done_items = 1;
        prog.current.clear();
        emit(&prog);
        Ok(ArchiveReport { done: 1, failed: 0, cancelled: false, errors: Vec::new() })
    } else if lower.ends_with(".7z") {
        extract_7z_stream(path, dest_path, cancel, &mut emit)
    } else {
        extract_zip_stream(path, dest_path, cancel, &mut emit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn scratch(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-archive-{}-{}-{}", tag, std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn lists_a_zip_archive() {
        let d = scratch("zip");
        let zip_path = d.join("a.zip");
        {
            let file = fs::File::create(&zip_path).unwrap();
            let mut w = zip::ZipWriter::new(file);
            let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default();
            w.start_file("hello.txt", opts).unwrap();
            w.write_all(b"hi there").unwrap();
            w.add_directory("sub/", opts).unwrap();
            w.finish().unwrap();
        }
        let entries = read_archive_entries(&zip_path.to_string_lossy()).unwrap();
        let hello = entries.iter().find(|e| e.name == "hello.txt").unwrap();
        assert!(!hello.is_dir && hello.size == 8);
        assert!(entries.iter().any(|e| e.name == "sub/" && e.is_dir));
        // Also reachable via the zip-specific lister (used by the compress verifier).
        assert!(zip_entries(&zip_path.to_string_lossy()).unwrap().iter().any(|e| e.name == "hello.txt"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn create_empty_zip_makes_a_valid_openable_archive() {
        let d = scratch("emptyzip");
        let zip_path = d.join("New Compressed (zipped) Folder.zip");
        let out = create_empty_zip(&zip_path.to_string_lossy()).unwrap();
        assert_eq!(out, zip_path.to_string_lossy());
        assert!(zip_path.exists());
        // It must be a genuinely valid archive: the zip reader opens it and reports zero entries,
        // and our own lister agrees (an empty file would fail both).
        let archive = zip::ZipArchive::new(fs::File::open(&zip_path).unwrap()).unwrap();
        assert_eq!(archive.len(), 0);
        assert!(read_archive_entries(&zip_path.to_string_lossy()).unwrap().is_empty());
        // Refuses to clobber an existing file (atomic create_new).
        assert!(create_empty_zip(&zip_path.to_string_lossy()).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn a_non_archive_is_an_error() {
        let d = scratch("bad");
        let f = d.join("not.zip");
        fs::write(&f, b"this is not a zip").unwrap();
        assert!(read_archive_entries(&f.to_string_lossy()).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn entry_name_is_safe_rejects_traversal() {
        assert!(entry_name_is_safe("a/b/c.txt"));
        assert!(entry_name_is_safe("./x.txt"));
        assert!(entry_name_is_safe("folder/leaf"));
        assert!(!entry_name_is_safe("../evil"));
        assert!(!entry_name_is_safe("a/../../evil"));
        assert!(!entry_name_is_safe("..\\evil")); // backslash traversal, normalised
        assert!(!entry_name_is_safe("a\\..\\..\\evil"));
        assert!(!entry_name_is_safe("/etc/passwd"));
        assert!(!entry_name_is_safe(""));
    }

    #[test]
    fn compress_to_zip_then_extract_round_trips() {
        let d = scratch("roundtrip");
        // Build a small source tree.
        fs::create_dir_all(d.join("src/sub")).unwrap();
        fs::write(d.join("src/a.txt"), b"alpha").unwrap();
        fs::write(d.join("src/sub/b.txt"), b"beta").unwrap();

        let zip_path = d.join("out.zip");
        // Empty selection errors.
        assert!(compress_to_zip(&[], &zip_path.to_string_lossy()).is_err());
        // Pack the folder.
        compress_to_zip(&[d.join("src").to_string_lossy().to_string()], &zip_path.to_string_lossy()).unwrap();
        // The listing sees both files.
        let names: Vec<String> = read_archive_entries(&zip_path.to_string_lossy())
            .unwrap()
            .iter()
            .map(|e| e.name.replace('\\', "/"))
            .collect();
        assert!(names.iter().any(|n| n.ends_with("a.txt")));
        assert!(names.iter().any(|n| n.ends_with("sub/b.txt")));

        // Extract it back out and verify contents.
        let out = d.join("unpacked");
        extract_archive(&zip_path.to_string_lossy(), &out.to_string_lossy()).unwrap();
        assert_eq!(fs::read(out.join("src/a.txt")).unwrap(), b"alpha");
        assert_eq!(fs::read(out.join("src/sub/b.txt")).unwrap(), b"beta");

        // Extract a single entry to a temp file.
        let tmp = extract_archive_entry(&zip_path.to_string_lossy(), "src/a.txt").unwrap();
        assert_eq!(fs::read(&tmp).unwrap(), b"alpha");
        let _ = fs::remove_file(&tmp);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn compress_to_targz_then_extract_round_trips() {
        let d = scratch("targz-roundtrip");
        fs::create_dir_all(d.join("src/sub")).unwrap();
        fs::write(d.join("src/a.txt"), b"alpha").unwrap();
        fs::write(d.join("src/sub/b.txt"), b"beta").unwrap();

        let tgz = d.join("out.tar.gz");
        assert!(compress_to_targz(&[], &tgz.to_string_lossy()).is_err(), "empty selection errors");
        compress_to_targz(&[d.join("src").to_string_lossy().to_string()], &tgz.to_string_lossy()).unwrap();

        // The listing (via the existing reader) sees both files.
        let names: Vec<String> = read_archive_entries(&tgz.to_string_lossy())
            .unwrap()
            .iter()
            .map(|e| e.name.replace('\\', "/"))
            .collect();
        assert!(names.iter().any(|n| n.ends_with("a.txt")), "got {names:?}");
        assert!(names.iter().any(|n| n.ends_with("sub/b.txt")), "got {names:?}");

        // Extract it back out and verify contents.
        let out = d.join("unpacked");
        extract_archive(&tgz.to_string_lossy(), &out.to_string_lossy()).unwrap();
        assert_eq!(fs::read(out.join("src/a.txt")).unwrap(), b"alpha");
        assert_eq!(fs::read(out.join("src/sub/b.txt")).unwrap(), b"beta");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn compress_archive_dispatches_by_extension() {
        let d = scratch("dispatch");
        fs::write(d.join("f.txt"), b"x").unwrap();
        let src = d.join("f.txt").to_string_lossy().to_string();

        // .zip and .tar.gz both work; both list the file back.
        for ext in ["out.zip", "out.tar.gz", "out.tgz"] {
            let dest = d.join(ext).to_string_lossy().to_string();
            compress_archive(std::slice::from_ref(&src), &dest).unwrap_or_else(|e| panic!("{ext}: {e}"));
            let names: Vec<_> = read_archive_entries(&dest).unwrap().iter().map(|e| e.name.clone()).collect();
            assert!(names.iter().any(|n| n.ends_with("f.txt")), "{ext}: got {names:?}");
        }
        // An unrecognised extension is a clear error.
        assert!(compress_archive(&[src], &d.join("out.rar").to_string_lossy()).unwrap_err().contains("unsupported"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn encrypted_zip_round_trips_and_rejects_a_wrong_password() {
        let d = scratch("encrypted");
        fs::write(d.join("secret.txt"), b"top secret").unwrap();
        let src = d.join("secret.txt").to_string_lossy().to_string();
        let zip = d.join("locked.zip");

        assert!(compress_to_zip_encrypted(&[], &zip.to_string_lossy(), "pw").is_err(), "empty selection errors");
        assert!(
            compress_to_zip_encrypted(std::slice::from_ref(&src), &zip.to_string_lossy(), "").is_err(),
            "an empty password errors"
        );
        compress_to_zip_encrypted(&[src], &zip.to_string_lossy(), "hunter2").unwrap();

        // The right password extracts the file byte-exact.
        let out = d.join("out");
        extract_zip_encrypted(&zip.to_string_lossy(), &out.to_string_lossy(), "hunter2").unwrap();
        assert_eq!(fs::read(out.join("secret.txt")).unwrap(), b"top secret");

        // A wrong password is a clear error, not a silent garbage extraction.
        let bad = d.join("bad");
        assert!(extract_zip_encrypted(&zip.to_string_lossy(), &bad.to_string_lossy(), "wrong").is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn compress_skips_the_output_archive_inside_a_source() {
        let d = scratch("zip_self");
        fs::create_dir_all(d.join("folder")).unwrap();
        fs::write(d.join("folder").join("a.txt"), b"a").unwrap();
        // The output .zip lives INSIDE the folder being compressed (CPE-632).
        let dest = d.join("folder").join("out.zip");
        compress_to_zip(&[d.join("folder").to_string_lossy().to_string()], &dest.to_string_lossy()).unwrap();
        let names: Vec<String> = zip_entries(&dest.to_string_lossy()).unwrap().into_iter().map(|e| e.name).collect();
        assert!(names.iter().any(|n| n.ends_with("a.txt")), "should contain the real file: {names:?}");
        assert!(!names.iter().any(|n| n.contains("out.zip")), "must not contain itself: {names:?}");
        let _ = fs::remove_dir_all(&d);
    }

    /// CRC-32 (IEEE), so the hand-built malicious zip below has a valid checksum the extractor accepts.
    fn crc32(data: &[u8]) -> u32 {
        let mut crc: u32 = 0xFFFF_FFFF;
        for &byte in data {
            crc ^= byte as u32;
            for _ in 0..8 {
                if crc & 1 != 0 {
                    crc = (crc >> 1) ^ 0xEDB8_8320;
                } else {
                    crc >>= 1;
                }
            }
        }
        !crc
    }

    /// A minimal single-entry STORED zip whose filename is `name` verbatim — used to smuggle a `../`
    /// traversal name past the zip *writer* (which rejects it), so we can test the *extractor's* guard.
    fn craft_zip_with_entry_name(name: &str, data: &[u8]) -> Vec<u8> {
        let name = name.as_bytes();
        let crc = crc32(data);
        let (nlen, dlen) = (name.len() as u16, data.len() as u32);
        let mut z = Vec::new();
        let u16le = |v: u16, z: &mut Vec<u8>| z.extend_from_slice(&v.to_le_bytes());
        let u32le = |v: u32, z: &mut Vec<u8>| z.extend_from_slice(&v.to_le_bytes());
        // Local file header.
        u32le(0x0403_4b50, &mut z);
        u16le(20, &mut z); u16le(0, &mut z); u16le(0, &mut z); // ver, flags, method(stored)
        u16le(0, &mut z); u16le(0, &mut z);                     // mod time/date
        u32le(crc, &mut z); u32le(dlen, &mut z); u32le(dlen, &mut z); // crc, comp, uncomp
        u16le(nlen, &mut z); u16le(0, &mut z);                  // name len, extra len
        z.extend_from_slice(name);
        z.extend_from_slice(data);
        let cd_offset = z.len() as u32;
        // Central directory header.
        u32le(0x0201_4b50, &mut z);
        u16le(20, &mut z); u16le(20, &mut z); u16le(0, &mut z); u16le(0, &mut z); // made-by, needed, flags, method
        u16le(0, &mut z); u16le(0, &mut z);                     // mod time/date
        u32le(crc, &mut z); u32le(dlen, &mut z); u32le(dlen, &mut z);
        u16le(nlen, &mut z); u16le(0, &mut z); u16le(0, &mut z); // name, extra, comment len
        u16le(0, &mut z); u16le(0, &mut z); u32le(0, &mut z);    // disk start, internal attrs, external attrs
        u32le(0, &mut z);                                        // local header offset
        z.extend_from_slice(name);
        let cd_size = z.len() as u32 - cd_offset;
        // End of central directory.
        u32le(0x0605_4b50, &mut z);
        u16le(0, &mut z); u16le(0, &mut z); u16le(1, &mut z); u16le(1, &mut z); // disks, entries
        u32le(cd_size, &mut z); u32le(cd_offset, &mut z); u16le(0, &mut z);     // cd size/offset, comment len
        z
    }

    // End-to-end zip-slip guard: a zip carrying a `../escape.txt` entry must NOT write outside the
    // extraction root. `extract_archive` leans on the zip crate's `enclosed_name`; this pins that the
    // guard actually holds, so a future crate bump that regressed it would fail CI (the 7z path has its
    // own `entry_name_is_safe` unit test; this covers the far more common zip format end-to-end).
    #[test]
    fn zip_extraction_does_not_escape_the_destination() {
        let d = scratch("zip_slip");
        let zip_path = d.join("evil.zip");
        // Hand-crafted because the zip *writer* refuses a `../` name — we're testing the extractor.
        fs::write(&zip_path, craft_zip_with_entry_name("../escape.txt", b"pwned")).unwrap();

        let dest = d.join("out");
        // The guard may either reject the archive (Err) or skip the unsafe entry (Ok) — both are safe.
        // The invariant we care about is that the traversal entry is NEVER written outside `dest`.
        let _ = extract_archive(&zip_path.to_string_lossy(), &dest.to_string_lossy());
        assert!(!d.join("escape.txt").exists(), "traversal entry escaped the extraction root");
        assert!(!dest.parent().unwrap().join("escape.txt").exists(), "traversal entry escaped the extraction root");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn extract_archive_unpacks_a_tar_gz() {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        let d = scratch("targz");
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
        extract_archive(&tgz.to_string_lossy(), &out.to_string_lossy()).unwrap();
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
        let names: Vec<String> = read_archive_entries(&tar_path.to_string_lossy()).unwrap().into_iter().map(|e| e.name).collect();
        assert!(names.iter().any(|n| n == "hello.txt"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn read_archive_entries_lists_gzip_single_file() {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        let d = scratch("gz_single");
        let gz_path = d.join("note.txt.gz");
        {
            let f = fs::File::create(&gz_path).unwrap();
            let mut enc = GzEncoder::new(f, Compression::default());
            enc.write_all(b"hello world").unwrap();
            enc.finish().unwrap();
        }
        let entries = read_archive_entries(&gz_path.to_string_lossy()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "note.txt", "name is the archive name minus .gz");
        assert_eq!(entries[0].size, 11, "ISIZE trailer is the uncompressed length");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn read_archive_entries_lists_rar_contents() {
        // Build a minimal synthetic RAR4 archive (mirrors crate::rar's own test fixtures, rebuilt here
        // rather than imported since those builders are private to that module) and drive it THROUGH
        // read_archive_entries's dispatch (CPE-1348) — proving the .rar branch is wired to
        // crate::rar::rar_entries, not just exercising rar_entries directly.
        const RAR4_MARKER: [u8; 7] = [0x52, 0x61, 0x72, 0x21, 0x1A, 0x07, 0x00];
        const RAR4_FILE_HEAD: u8 = 0x74;

        fn rar4_file_block(name: &str, unp_size: u32) -> Vec<u8> {
            let mut body = Vec::new();
            body.extend_from_slice(&0u32.to_le_bytes()); // pack_size
            body.extend_from_slice(&unp_size.to_le_bytes()); // unp_size
            body.push(0); // host_os
            body.extend_from_slice(&0u32.to_le_bytes()); // file_crc
            body.extend_from_slice(&0u32.to_le_bytes()); // ftime
            body.push(0); // unp_ver
            body.push(0); // method
            body.extend_from_slice(&(name.len() as u16).to_le_bytes()); // name_size
            body.extend_from_slice(&0u32.to_le_bytes()); // attr
            body.extend_from_slice(name.as_bytes());
            let head_size = (7 + body.len()) as u16;
            let mut block = Vec::new();
            block.extend_from_slice(&0u16.to_le_bytes()); // crc16 (unchecked)
            block.push(RAR4_FILE_HEAD);
            block.extend_from_slice(&0u16.to_le_bytes()); // flags
            block.extend_from_slice(&head_size.to_le_bytes());
            block.extend_from_slice(&body);
            block
        }

        let d = scratch("rar_list");
        let rar_path = d.join("bundle.rar");
        let mut buf = RAR4_MARKER.to_vec();
        buf.extend(rar4_file_block("hello.txt", 8));
        fs::write(&rar_path, &buf).unwrap();

        let entries = read_archive_entries(&rar_path.to_string_lossy()).unwrap();
        let hello = entries.iter().find(|e| e.name == "hello.txt").unwrap();
        assert!(!hello.is_dir);
        assert_eq!(hello.size, 8);
        let _ = fs::remove_dir_all(&d);
    }

    /// Build a minimal `.7z` fixture at `dest` containing `entries` (name, bytes), via `sevenz-rust`'s
    /// own writer — there is no `compress_to_7z` in this crate (CPE-1180 is extraction-only), so the test
    /// packs its own fixture the same way the crate would consume a real one.
    fn write_7z_fixture(dest: &Path, entries: &[(&str, &[u8])]) {
        let mut w = sevenz_rust::SevenZWriter::create(dest).unwrap();
        for (name, data) in entries {
            let mut entry = sevenz_rust::SevenZArchiveEntry::new();
            entry.name = (*name).to_string();
            w.push_archive_entry(entry, Some(std::io::Cursor::new(*data))).unwrap();
        }
        w.finish().unwrap();
    }

    #[test]
    fn extract_archive_entry_any_round_trips_tar_gz_and_7z() {
        // .tar.gz, built via the existing create fn.
        let d = scratch("any_targz");
        fs::create_dir_all(d.join("src/sub")).unwrap();
        fs::write(d.join("src/a.txt"), b"alpha").unwrap();
        fs::write(d.join("src/sub/b.txt"), b"beta").unwrap();
        let tgz = d.join("out.tar.gz");
        compress_to_targz(&[d.join("src").to_string_lossy().to_string()], &tgz.to_string_lossy()).unwrap();

        let tmp = extract_archive_entry_any(&tgz.to_string_lossy(), "src/sub/b.txt").unwrap();
        assert_eq!(fs::read(&tmp).unwrap(), b"beta");
        let _ = fs::remove_file(&tmp);

        // .7z, built via sevenz-rust's own writer (no create fn exists for 7z in this crate).
        let sevenz = d.join("out.7z");
        write_7z_fixture(&sevenz, &[("a.txt", b"alpha7z"), ("sub/b.txt", b"beta7z")]);
        let tmp = extract_archive_entry_any(&sevenz.to_string_lossy(), "sub/b.txt").unwrap();
        assert_eq!(fs::read(&tmp).unwrap(), b"beta7z");
        let _ = fs::remove_file(&tmp);
        let tmp = extract_archive_entry_any(&sevenz.to_string_lossy(), "a.txt").unwrap();
        assert_eq!(fs::read(&tmp).unwrap(), b"alpha7z");
        let _ = fs::remove_file(&tmp);

        // A missing entry is a clear error, not a silent empty file.
        assert!(extract_archive_entry_any(&sevenz.to_string_lossy(), "nope.txt").is_err());
        assert!(extract_archive_entry_any(&tgz.to_string_lossy(), "nope.txt").is_err());

        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn extract_archive_entry_any_round_trips_plain_tar_and_tgz_extension() {
        let d = scratch("any_tar");
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
        let tmp = extract_archive_entry_any(&tar_path.to_string_lossy(), "hello.txt").unwrap();
        assert_eq!(fs::read(&tmp).unwrap(), b"hi there");
        let _ = fs::remove_file(&tmp);

        // .tgz is the same gzip-tar format under a different extension.
        fs::create_dir_all(d.join("src")).unwrap();
        fs::write(d.join("src/note.txt"), b"tgz-note").unwrap();
        let tgz = d.join("bundle.tgz");
        compress_to_targz(&[d.join("src").to_string_lossy().to_string()], &tgz.to_string_lossy()).unwrap();
        let tmp = extract_archive_entry_any(&tgz.to_string_lossy(), "src/note.txt").unwrap();
        assert_eq!(fs::read(&tmp).unwrap(), b"tgz-note");
        let _ = fs::remove_file(&tmp);

        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn extract_archive_entry_any_delegates_zip_to_the_zip_extractor() {
        let d = scratch("any_zip");
        fs::write(d.join("a.txt"), b"alpha").unwrap();
        let zip_path = d.join("out.zip");
        compress_to_zip(&[d.join("a.txt").to_string_lossy().to_string()], &zip_path.to_string_lossy()).unwrap();
        let tmp = extract_archive_entry_any(&zip_path.to_string_lossy(), "a.txt").unwrap();
        assert_eq!(fs::read(&tmp).unwrap(), b"alpha");
        let _ = fs::remove_file(&tmp);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn extract_archive_entry_any_routes_rar_to_the_rar_extractor() {
        // Hand-build a minimal RAR4 with one STORED entry (the `rar` module's own builders are private
        // to its test module, so assemble the few bytes here) and prove `.rar` now routes to the RAR
        // extractor instead of the ZIP one (which used to fail).
        fn rar4_block(head_type: u8, body: &[u8], payload: &[u8]) -> Vec<u8> {
            let head_size = (7 + body.len()) as u16;
            let mut b = Vec::new();
            b.extend_from_slice(&0u16.to_le_bytes()); // crc16
            b.push(head_type);
            b.extend_from_slice(&0u16.to_le_bytes()); // flags
            b.extend_from_slice(&head_size.to_le_bytes());
            b.extend_from_slice(body);
            b.extend_from_slice(payload);
            b
        }
        let payload = b"rar stored bytes";
        let mut file_body = Vec::new();
        file_body.extend_from_slice(&(payload.len() as u32).to_le_bytes()); // pack_size
        file_body.extend_from_slice(&(payload.len() as u32).to_le_bytes()); // unp_size
        file_body.push(0); // host_os
        file_body.extend_from_slice(&0u32.to_le_bytes()); // crc
        file_body.extend_from_slice(&0u32.to_le_bytes()); // ftime
        file_body.push(0); // unp_ver
        file_body.push(0x30); // method = STORE
        file_body.extend_from_slice(&(b"note.txt".len() as u16).to_le_bytes()); // name_size
        file_body.extend_from_slice(&0u32.to_le_bytes()); // attr
        file_body.extend_from_slice(b"note.txt");

        let mut buf = vec![0x52, 0x61, 0x72, 0x21, 0x1A, 0x07, 0x00]; // RAR4 marker
        buf.extend(rar4_block(0x73, &[], &[])); // MAIN_HEAD
        buf.extend(rar4_block(0x74, &file_body, payload)); // FILE_HEAD (stored)

        let d = scratch("any_rar");
        let path = d.join("a.rar");
        fs::write(&path, &buf).unwrap();
        let tmp = extract_archive_entry_any(&path.to_string_lossy(), "note.txt").unwrap();
        assert_eq!(fs::read(&tmp).unwrap(), payload);
        let _ = fs::remove_file(&tmp);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn extract_archive_entry_any_rejects_a_traversal_inner_regardless_of_format() {
        let d = scratch("any_traversal");
        fs::write(d.join("a.txt"), b"alpha").unwrap();
        let tgz = d.join("out.tar.gz");
        compress_to_targz(&[d.join("a.txt").to_string_lossy().to_string()], &tgz.to_string_lossy()).unwrap();
        let sevenz = d.join("out.7z");
        write_7z_fixture(&sevenz, &[("a.txt", b"alpha")]);

        for bad in ["../evil.txt", "a/../../evil", "..\\evil"] {
            let err = extract_archive_entry_any(&tgz.to_string_lossy(), bad).unwrap_err();
            assert!(err.contains("unsafe"), "tgz: expected an unsafe-entry error, got {err:?}");
            let err = extract_archive_entry_any(&sevenz.to_string_lossy(), bad).unwrap_err();
            assert!(err.contains("unsafe"), "7z: expected an unsafe-entry error, got {err:?}");
        }
        let _ = fs::remove_dir_all(&d);
    }

    // -----------------------------------------------------------------------------------------
    // Streamed compress/extract with progress + cancel (CPE-1184)
    // -----------------------------------------------------------------------------------------

    /// Build a small source tree with enough files that a streamed run emits more than one progress
    /// tick, so cancellation tests have somewhere to stop mid-run.
    fn build_source_tree(d: &Path) -> String {
        fs::create_dir_all(d.join("src/sub")).unwrap();
        for i in 0..6 {
            fs::write(d.join(format!("src/f{i}.txt")), format!("payload-{i}").repeat(50)).unwrap();
        }
        fs::write(d.join("src/sub/nested.txt"), b"nested").unwrap();
        d.join("src").to_string_lossy().to_string()
    }

    #[test]
    fn compress_to_zip_streamed_reports_growing_progress_and_round_trips() {
        let d = scratch("stream_zip_compress");
        let src = build_source_tree(&d);
        let zip_path = d.join("out.zip");
        let cancel = AtomicBool::new(false);
        let mut ticks: Vec<ArchiveProgress> = Vec::new();

        let report =
            compress_to_zip_streamed(&[src], &zip_path.to_string_lossy(), &cancel, |p| ticks.push(p.clone())).unwrap();

        assert!(!report.cancelled);
        assert_eq!(report.done, 7, "6 files + 1 nested file"); // f0..f5 + sub/nested.txt
        assert!(ticks.len() >= 7, "expected a progress tick per file, got {}", ticks.len());
        // Progress is monotonically non-decreasing and the final tick reaches the totals.
        for w in ticks.windows(2) {
            assert!(w[1].done_bytes >= w[0].done_bytes);
            assert!(w[1].done_items >= w[0].done_items);
        }
        let last = ticks.last().unwrap();
        assert_eq!(last.done_items, last.total_items);
        assert_eq!(last.done_bytes, last.total_bytes);

        // Round-trips: the archive really contains what was streamed in.
        let names: Vec<String> =
            read_archive_entries(&zip_path.to_string_lossy()).unwrap().into_iter().map(|e| e.name.replace('\\', "/")).collect();
        assert!(names.iter().any(|n| n.ends_with("f0.txt")));
        assert!(names.iter().any(|n| n.ends_with("sub/nested.txt")));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn compress_to_zip_streamed_can_be_cancelled_mid_run_and_leaves_a_valid_partial_archive() {
        let d = scratch("stream_zip_cancel");
        let src = build_source_tree(&d);
        let zip_path = d.join("partial.zip");
        let cancel = AtomicBool::new(false);
        let mut count = 0u32;

        let report = compress_to_zip_streamed(&[src], &zip_path.to_string_lossy(), &cancel, |_| {
            count += 1;
            if count == 2 {
                cancel.store(true, Ordering::Relaxed);
            }
        })
        .unwrap();

        assert!(report.cancelled);
        assert!(report.done < 7, "cancellation should stop before every file is packed");
        // The partial file is still a valid, openable archive (writer.finish() ran regardless).
        let archive = zip::ZipArchive::new(fs::File::open(&zip_path).unwrap()).unwrap();
        assert!(archive.len() < 7);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn compress_to_targz_streamed_reports_progress_and_round_trips() {
        let d = scratch("stream_targz_compress");
        let src = build_source_tree(&d);
        let tgz = d.join("out.tar.gz");
        let cancel = AtomicBool::new(false);
        let mut ticks = 0u32;

        let report =
            compress_to_targz_streamed(&[src], &tgz.to_string_lossy(), &cancel, |_| ticks += 1).unwrap();

        assert!(!report.cancelled);
        assert_eq!(report.done, 7);
        assert!(ticks >= 7);
        let names: Vec<String> =
            read_archive_entries(&tgz.to_string_lossy()).unwrap().into_iter().map(|e| e.name.replace('\\', "/")).collect();
        assert!(names.iter().any(|n| n.ends_with("f3.txt")));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn compress_archive_streamed_dispatches_by_extension_like_the_one_shot_version() {
        let d = scratch("stream_dispatch");
        fs::write(d.join("f.txt"), b"x").unwrap();
        let src = d.join("f.txt").to_string_lossy().to_string();
        let cancel = AtomicBool::new(false);

        for ext in ["out.zip", "out.tar.gz", "out.tgz"] {
            let dest = d.join(ext).to_string_lossy().to_string();
            let report = compress_archive_streamed(std::slice::from_ref(&src), &dest, &cancel, |_| {})
                .unwrap_or_else(|e| panic!("{ext}: {e}"));
            assert_eq!(report.done, 1);
        }
        let err = compress_archive_streamed(&[src], &d.join("out.rar").to_string_lossy(), &cancel, |_| {}).unwrap_err();
        assert!(err.contains("unsupported"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn compress_to_zip_encrypted_streamed_round_trips_and_rejects_a_wrong_password() {
        let d = scratch("stream_encrypted");
        fs::write(d.join("secret.txt"), b"top secret").unwrap();
        let src = d.join("secret.txt").to_string_lossy().to_string();
        let zip = d.join("locked.zip");
        let cancel = AtomicBool::new(false);

        assert!(compress_to_zip_encrypted_streamed(&[], &zip.to_string_lossy(), "pw", &cancel, |_| {}).is_err());
        assert!(
            compress_to_zip_encrypted_streamed(std::slice::from_ref(&src), &zip.to_string_lossy(), "", &cancel, |_| {})
                .is_err(),
            "an empty password errors"
        );
        let report = compress_to_zip_encrypted_streamed(&[src], &zip.to_string_lossy(), "hunter2", &cancel, |_| {}).unwrap();
        assert_eq!(report.done, 1);

        let out = d.join("out");
        extract_zip_encrypted(&zip.to_string_lossy(), &out.to_string_lossy(), "hunter2").unwrap();
        assert_eq!(fs::read(out.join("secret.txt")).unwrap(), b"top secret");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn check_zip_password_detects_missing_and_wrong_passwords_fast() {
        let d = scratch("check_password");
        fs::write(d.join("secret.txt"), b"top secret").unwrap();
        let src = d.join("secret.txt").to_string_lossy().to_string();
        let zip = d.join("locked.zip");
        compress_to_zip_encrypted(&[src], &zip.to_string_lossy(), "hunter2").unwrap();

        assert!(check_zip_password(&zip.to_string_lossy(), None).is_err(), "no password -> needs one");
        assert!(check_zip_password(&zip.to_string_lossy(), Some("wrong")).is_err(), "wrong password rejected");
        assert!(check_zip_password(&zip.to_string_lossy(), Some("hunter2")).is_ok(), "right password accepted");

        // A plain (unencrypted) archive never needs a password, with or without one supplied.
        let plain = d.join("plain.zip");
        compress_to_zip(&[d.join("secret.txt").to_string_lossy().to_string()], &plain.to_string_lossy()).unwrap();
        assert!(check_zip_password(&plain.to_string_lossy(), None).is_ok());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn extract_archive_streamed_zip_round_trips_and_reports_progress() {
        let d = scratch("stream_zip_extract");
        let src = build_source_tree(&d);
        let zip_path = d.join("out.zip");
        compress_to_zip(&[src], &zip_path.to_string_lossy()).unwrap();

        let cancel = AtomicBool::new(false);
        let mut ticks: Vec<ArchiveProgress> = Vec::new();
        let out = d.join("unpacked");
        let report =
            extract_archive_streamed(&zip_path.to_string_lossy(), &out.to_string_lossy(), &cancel, |p| ticks.push(p.clone()))
                .unwrap();

        assert!(!report.cancelled);
        assert_eq!(report.done, 7, "6 files + 1 nested file");
        assert!(ticks.len() >= 7);
        // total_items is the zip's whole entry count (files + directory placeholders — 7 files + 2 dirs
        // = 9), known up front from the central directory before any entry is read.
        assert_eq!(ticks.first().unwrap().total_items, 9, "totals known up front for zip (central directory)");
        assert_eq!(fs::read_to_string(out.join("src/f0.txt")).unwrap(), "payload-0".repeat(50));
        assert_eq!(fs::read_to_string(out.join("src/sub/nested.txt")).unwrap(), "nested");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn extract_archive_streamed_zip_can_be_cancelled_mid_run() {
        let d = scratch("stream_zip_extract_cancel");
        let src = build_source_tree(&d);
        let zip_path = d.join("out.zip");
        compress_to_zip(&[src], &zip_path.to_string_lossy()).unwrap();

        let cancel = AtomicBool::new(false);
        let mut count = 0u32;
        let out = d.join("unpacked");
        let report = extract_archive_streamed(&zip_path.to_string_lossy(), &out.to_string_lossy(), &cancel, |_| {
            count += 1;
            if count == 2 {
                cancel.store(true, Ordering::Relaxed);
            }
        })
        .unwrap();

        assert!(report.cancelled);
        assert!(report.done < 7);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn extract_archive_streamed_targz_reports_pre_measured_totals() {
        let d = scratch("stream_targz_extract");
        let src = build_source_tree(&d);
        let tgz = d.join("out.tar.gz");
        compress_to_targz(&[src], &tgz.to_string_lossy()).unwrap();

        let cancel = AtomicBool::new(false);
        let mut ticks: Vec<ArchiveProgress> = Vec::new();
        let out = d.join("unpacked");
        let report =
            extract_archive_streamed(&tgz.to_string_lossy(), &out.to_string_lossy(), &cancel, |p| ticks.push(p.clone()))
                .unwrap();

        assert!(!report.cancelled);
        assert_eq!(report.done, 7);
        // The very FIRST emitted tick already knows the totals — proof of the tar pre-measurement pass,
        // not just a running item count that only reaches the total at the end.
        assert_eq!(ticks.first().unwrap().total_items, 7);
        assert!(ticks.first().unwrap().total_bytes > 0);
        assert_eq!(fs::read_to_string(out.join("src/f0.txt")).unwrap(), "payload-0".repeat(50));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn extract_archive_streamed_7z_round_trips_and_reports_progress() {
        let d = scratch("stream_7z_extract");
        let sevenz = d.join("out.7z");
        write_7z_fixture(&sevenz, &[("a.txt", b"alpha7z"), ("sub/b.txt", b"beta7z")]);

        let cancel = AtomicBool::new(false);
        let mut ticks: Vec<ArchiveProgress> = Vec::new();
        let out = d.join("unpacked");
        let report =
            extract_archive_streamed(&sevenz.to_string_lossy(), &out.to_string_lossy(), &cancel, |p| ticks.push(p.clone()))
                .unwrap();

        assert!(!report.cancelled);
        assert_eq!(report.done, 2);
        assert!(ticks.len() >= 2);
        assert_eq!(fs::read(out.join("a.txt")).unwrap(), b"alpha7z");
        assert_eq!(fs::read(out.join("sub/b.txt")).unwrap(), b"beta7z");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn extract_archive_streamed_gz_single_file_is_one_item() {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        let d = scratch("stream_gz_extract");
        let gz_path = d.join("note.txt.gz");
        {
            let f = fs::File::create(&gz_path).unwrap();
            let mut enc = GzEncoder::new(f, Compression::default());
            enc.write_all(b"hello world").unwrap();
            enc.finish().unwrap();
        }
        let cancel = AtomicBool::new(false);
        let out = d.join("unpacked");
        let report =
            extract_archive_streamed(&gz_path.to_string_lossy(), &out.to_string_lossy(), &cancel, |_| {}).unwrap();
        assert_eq!(report.done, 1);
        assert_eq!(fs::read_to_string(out.join("note.txt")).unwrap(), "hello world");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn extract_zip_encrypted_streamed_round_trips_and_rejects_a_wrong_password() {
        let d = scratch("stream_extract_encrypted");
        fs::write(d.join("secret.txt"), b"top secret").unwrap();
        let src = d.join("secret.txt").to_string_lossy().to_string();
        let zip = d.join("locked.zip");
        compress_to_zip_encrypted(&[src], &zip.to_string_lossy(), "hunter2").unwrap();

        let cancel = AtomicBool::new(false);
        let bad = d.join("bad");
        assert!(extract_zip_encrypted_streamed(&zip.to_string_lossy(), &bad.to_string_lossy(), "wrong", &cancel, |_| {})
            .is_err());

        let out = d.join("out");
        let report =
            extract_zip_encrypted_streamed(&zip.to_string_lossy(), &out.to_string_lossy(), "hunter2", &cancel, |_| {})
                .unwrap();
        assert_eq!(report.done, 1);
        assert_eq!(fs::read(out.join("secret.txt")).unwrap(), b"top secret");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn extract_archive_streamed_zip_extraction_does_not_escape_the_destination() {
        let d = scratch("stream_zip_slip");
        let zip_path = d.join("evil.zip");
        fs::write(&zip_path, craft_zip_with_entry_name("../escape.txt", b"pwned")).unwrap();

        let cancel = AtomicBool::new(false);
        let dest = d.join("out");
        // Same invariant as the one-shot extractor's guard test: either an error or a silent skip is
        // acceptable, but the traversal entry must never land outside `dest`.
        let _ = extract_archive_streamed(&zip_path.to_string_lossy(), &dest.to_string_lossy(), &cancel, |_| {});
        assert!(!d.join("escape.txt").exists());
        assert!(!dest.parent().unwrap().join("escape.txt").exists());
        let _ = fs::remove_dir_all(&d);
    }
}
