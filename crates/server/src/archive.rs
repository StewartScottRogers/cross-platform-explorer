//! Archive listing (CPE-064/109/110/113): browse an archive's directory without extracting it, for the
//! preview pane. Dispatches by extension across ZIP, TAR (± gzip), single-file gzip, 7-Zip, and ISO —
//! reading only the archive directory so it stays cheap even for large archives. Pure-Rust deps (zip /
//! tar / flate2 / sevenz-rust / iso9660), no system libs; extracted into the Server (CPE-815) as real
//! filesystem domain logic. The Tauri `read_archive_entries` command dispatches here.

use std::fs;
use std::path::Path;

use serde::Serialize;

/// One entry inside an archive, for the archive preview.
#[derive(Serialize)]
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
/// family (zip/jar/apk/…), TAR, gzip-compressed TAR (.tar.gz/.tgz), single-file gzip (.gz), 7-Zip, ISO.
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
    } else {
        zip_entries(path)
    }
}

// ---------------------------------------------------------------------------
// Archive creation & extraction (CPE-251/252/242)
// ---------------------------------------------------------------------------

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

    let base = Path::new(inner)
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
fn entry_name_is_safe(name: &str) -> bool {
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
}
