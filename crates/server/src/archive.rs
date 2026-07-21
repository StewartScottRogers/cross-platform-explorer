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
}
