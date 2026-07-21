//! Directory listing (CPE-663/662): the single directory walker behind both the synchronous `list_dir`
//! and the streaming `list_dir_stream` command, so their contents + skip behaviour stay identical. Pure
//! and Tauri-free (CPE-815): the walker takes a `flush` callback returning `ControlFlow`, so the app's
//! streaming command keeps its `ipc::Channel` (and cancel registry) in the adapter and feeds this walker.

use std::fs;

use crate::fsutil::to_epoch_ms;
use crate::model::{extension_of, is_hidden, DirEntry};

/// Number of entries per streamed batch — small enough that the first rows paint within a frame or two
/// on a big folder, large enough that a tiny folder is one flush (CPE-662).
pub const LIST_DIR_BATCH: usize = 256;

/// Map one directory entry to a [`DirEntry`], or `None` if it can't be read — the caller skips those
/// rather than failing the whole listing.
fn dir_entry_from(entry: &fs::DirEntry) -> Option<DirEntry> {
    let meta = entry.metadata().ok()?;
    let entry_path = entry.path();
    let is_dir = meta.is_dir();
    Some(DirEntry {
        hidden: is_hidden(&entry_path, &meta),
        name: entry.file_name().to_string_lossy().to_string(),
        path: entry_path.to_string_lossy().to_string(),
        is_dir,
        size: if is_dir { 0 } else { meta.len() },
        modified: meta.modified().ok().and_then(to_epoch_ms),
        extension: if is_dir { String::new() } else { extension_of(&entry_path) },
    })
}

/// Walk `path`, invoking `flush` with each batch of up to `batch` readable entries as they're read.
/// Unreadable entries are skipped (never fail the listing). Returns the total number of entries emitted.
/// `flush` returns a `ControlFlow` so a streaming caller can stop the walk early (cancellation, CPE-665)
/// at a batch boundary.
pub fn stream_dir_entries(
    path: &str,
    batch: usize,
    mut flush: impl FnMut(Vec<DirEntry>) -> std::ops::ControlFlow<()>,
) -> Result<usize, String> {
    let read = fs::read_dir(path).map_err(|e| format!("{path}: {e}"))?;
    let cap = batch.min(1024);
    let mut buf: Vec<DirEntry> = Vec::with_capacity(cap);
    let mut total = 0usize;
    for entry in read {
        let Ok(entry) = entry else { continue };
        let Some(de) = dir_entry_from(&entry) else { continue };
        buf.push(de);
        if buf.len() >= batch {
            total += buf.len();
            if flush(std::mem::replace(&mut buf, Vec::with_capacity(cap))).is_break() {
                return Ok(total);
            }
        }
    }
    if !buf.is_empty() {
        total += buf.len();
        let _ = flush(buf);
    }
    Ok(total)
}

/// Collect-to-vec directory listing: every readable entry of `path`. A missing/unreadable `path` is an
/// `Err`.
pub fn list_dir(path: &str) -> Result<Vec<DirEntry>, String> {
    let mut out = Vec::new();
    stream_dir_entries(path, LIST_DIR_BATCH, |batch| {
        out.extend(batch);
        std::ops::ControlFlow::Continue(())
    })?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-listing-{}-{}-{}", tag, std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn list_dir_errors_on_a_missing_path() {
        assert!(list_dir("/definitely/not/a/real/path/xyz").is_err());
    }

    #[test]
    fn list_dir_lists_a_real_directory() {
        assert!(list_dir(&std::env::temp_dir().to_string_lossy()).is_ok());
    }

    #[test]
    fn stream_dir_entries_batches_and_flushes_all() {
        let d = scratch("streamdir");
        for i in 0..500 {
            fs::write(d.join(format!("f{i:03}.txt")), b"x").unwrap();
        }
        let mut batch_sizes = Vec::new();
        let n = stream_dir_entries(d.to_str().unwrap(), 256, |b| {
            batch_sizes.push(b.len());
            std::ops::ControlFlow::Continue(())
        })
        .unwrap();
        assert_eq!(n, 500);
        assert_eq!(batch_sizes.iter().sum::<usize>(), 500);
        assert!(batch_sizes.len() >= 2, "500 entries at batch 256 should flush more than once");
        assert!(batch_sizes.iter().all(|&s| s <= 256), "no batch exceeds the cap");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn stream_dir_entries_stops_on_break() {
        let d = scratch("streambreak");
        for i in 0..1000 {
            fs::write(d.join(format!("f{i:04}.txt")), b"x").unwrap();
        }
        let mut seen = 0usize;
        // Break after the first flush — the walk must stop rather than read all 1000.
        let n = stream_dir_entries(d.to_str().unwrap(), 100, |b| {
            seen += b.len();
            std::ops::ControlFlow::Break(())
        })
        .unwrap();
        assert_eq!(seen, 100, "break after the first batch stops the walk");
        assert_eq!(n, 100);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn stream_dir_entries_matches_list_dir_contents() {
        let d = scratch("streameq");
        for n in ["a.txt", "b.rs", "c.png"] {
            fs::write(d.join(n), b"x").unwrap();
        }
        fs::create_dir(d.join("sub")).unwrap();
        // A tiny batch of 2 exercises the mid-walk flush path.
        let mut streamed = Vec::new();
        stream_dir_entries(d.to_str().unwrap(), 2, |b| {
            streamed.extend(b);
            std::ops::ControlFlow::Continue(())
        })
        .unwrap();
        let listed = list_dir(&d.to_string_lossy()).unwrap();
        let mut a: Vec<_> = streamed.iter().map(|e| e.name.clone()).collect();
        let mut b: Vec<_> = listed.iter().map(|e| e.name.clone()).collect();
        a.sort();
        b.sort();
        assert_eq!(a, b);
        assert_eq!(a, vec!["a.txt", "b.rs", "c.png", "sub"]);
        let _ = fs::remove_dir_all(&d);
    }
}
