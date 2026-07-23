//! Disk-usage scanning (CPE-749/754, epic CPE-706): recursive directory size + the per-child size
//! breakdown for the space-analyzer treemap. Symlinked dirs are NOT followed (CPE-611) and unreadable
//! entries are skipped. The recursive walk fans subtree work across cores with rayon (CPE-754). Pure and
//! Tauri-free (CPE-815); the Tauri `dir_size` / `dir_children_sizes` commands dispatch here.

use std::fs;
use std::path::Path;

use serde::Serialize;

use crate::fsutil::entry_is_symlink;

/// Recursive size of a directory tree in bytes. Symlinked dirs are not followed; unreadable entries are
/// skipped rather than failing the whole calculation.
pub fn dir_size_walk(p: &Path) -> u64 {
    use rayon::prelude::*;
    let Ok(read) = fs::read_dir(p) else { return 0 };
    // Sum file lengths inline (cheap) and collect only the sub-directories, whose recursive walks are
    // the real cost — then fan those across cores (CPE-754). Work-stealing means one huge subtree
    // doesn't stall the others. Symlinked dirs are still skipped (CPE-611).
    let mut file_total = 0u64;
    let mut subdirs: Vec<std::path::PathBuf> = Vec::new();
    for entry in read.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
            if !entry_is_symlink(&entry) {
                subdirs.push(entry.path());
            }
        } else {
            file_total += meta.len();
        }
    }
    file_total + subdirs.par_iter().map(|d| dir_size_walk(d)).sum::<u64>()
}

/// Total recursive size of the tree at `path` (a file's own length if `path` is a file). A missing path
/// is an `Err`.
pub fn dir_size(path: &str) -> Result<u64, String> {
    let p = Path::new(path);
    if !p.exists() {
        return Err(format!("{path}: not found"));
    }
    Ok(dir_size_walk(p))
}

/// One direct child of a folder with its recursive size, for the treemap + drill-down (CPE-749). A
/// symlinked dir contributes `0` (not followed). Serialized to match the frontend `ChildSize`.
#[derive(Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ChildSize {
    name: String,
    path: String,
    is_dir: bool,
    size: u64,
}

/// The immediate children of `path`, each with its recursive size. `path` must be a directory.
/// Unreadable children are skipped, not fatal. Folder sizes are computed in parallel (CPE-754).
pub fn dir_children_sizes(path: &str) -> Result<Vec<ChildSize>, String> {
    use rayon::prelude::*;
    let p = Path::new(path);
    if !p.is_dir() {
        return Err(format!("not a folder: {path}"));
    }
    let read = fs::read_dir(p).map_err(|e| e.to_string())?;
    struct Pre {
        name: String,
        path: std::path::PathBuf,
        is_dir: bool,
        own: u64,
        symlink: bool,
    }
    let pre: Vec<Pre> = read
        .flatten()
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?; // skip unreadable child
            Some(Pre {
                name: entry.file_name().to_string_lossy().into_owned(),
                path: entry.path(),
                is_dir: meta.is_dir(),
                own: meta.len(),
                symlink: entry_is_symlink(&entry),
            })
        })
        .collect();
    let out = pre
        .into_par_iter()
        .map(|e| {
            let size = if e.is_dir {
                if e.symlink { 0 } else { dir_size_walk(&e.path) }
            } else {
                e.own
            };
            ChildSize {
                name: e.name,
                path: e.path.to_string_lossy().into_owned(),
                is_dir: e.is_dir,
                size,
            }
        })
        .collect();
    Ok(out)
}

/// Streaming variant of [`dir_children_sizes`] (CPE-706, streaming-liveness convention): computes each
/// direct child's recursive size in parallel and hands it to `on_child` as soon as it's ready, so a
/// space-analyzer treemap of a big folder fills in progressively instead of blocking on the whole scan.
/// `on_child` must be `Sync` — it's called from rayon worker threads, in completion (not directory)
/// order; the UI re-lays-out each arrival. Unreadable children are skipped; a non-folder `path` is an `Err`.
pub fn stream_children_sizes(path: &str, on_child: impl Fn(ChildSize) + Sync) -> Result<(), String> {
    use rayon::prelude::*;
    let p = Path::new(path);
    if !p.is_dir() {
        return Err(format!("not a folder: {path}"));
    }
    let read = fs::read_dir(p).map_err(|e| e.to_string())?;
    struct Pre {
        name: String,
        path: std::path::PathBuf,
        is_dir: bool,
        own: u64,
        symlink: bool,
    }
    let pre: Vec<Pre> = read
        .flatten()
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?; // skip unreadable child
            Some(Pre {
                name: entry.file_name().to_string_lossy().into_owned(),
                path: entry.path(),
                is_dir: meta.is_dir(),
                own: meta.len(),
                symlink: entry_is_symlink(&entry),
            })
        })
        .collect();
    pre.into_par_iter().for_each(|e| {
        let size = if e.is_dir {
            if e.symlink { 0 } else { dir_size_walk(&e.path) }
        } else {
            e.own
        };
        on_child(ChildSize {
            name: e.name,
            path: e.path.to_string_lossy().into_owned(),
            is_dir: e.is_dir,
            size,
        });
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-diskusage-{}-{}-{}", tag, std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn dir_size_sums_the_tree() {
        let d = scratch("dirsize");
        fs::create_dir_all(d.join("sub")).unwrap();
        fs::write(d.join("a.bin"), vec![0u8; 100]).unwrap();
        fs::write(d.join("sub/b.bin"), vec![0u8; 50]).unwrap();
        assert_eq!(dir_size(&d.to_string_lossy()).unwrap(), 150);
        assert!(dir_size(&d.join("missing").to_string_lossy()).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    /// Create a directory symlink; returns false on unprivileged Windows so the test can skip.
    fn symlink_dir(target: &Path, link: &Path) -> bool {
        #[cfg(unix)]
        { std::os::unix::fs::symlink(target, link).is_ok() }
        #[cfg(windows)]
        { std::os::windows::fs::symlink_dir(target, link).is_ok() }
    }

    // The anti-cycle guarantee (CPE-611): symlinked directories are NOT followed. Without this, a symlink
    // that points back up its own tree would send the recursive walk into an infinite loop (a production
    // hang). We build a cycle (a symlink pointing back at its own parent) plus a large file reachable only
    // *through* the symlink, and assert the scan terminates and never counts the through-symlink bytes.
    // Sizes aren't asserted exactly: a symlink's own reported length is 0 on Windows but the target-path
    // string length on Linux, so we assert the invariant (huge target uncounted) not a byte total.
    #[test]
    fn dir_size_does_not_follow_symlinked_dirs() {
        let d = scratch("symlink_cycle");
        fs::write(d.join("real.bin"), vec![0u8; 42]).unwrap();
        // A big file that is ONLY reachable by following the symlink cycle back into d.
        const BIG: usize = 500_000;
        fs::write(d.join("big.bin"), vec![0u8; BIG]).unwrap();
        // `loop` -> d : following it would re-scan d (incl. big.bin) forever.
        if !symlink_dir(&d, &d.join("loop")) {
            let _ = fs::remove_dir_all(&d);
            return; // unprivileged Windows: symlink creation gated — skip
        }
        // Terminates (no hang). The tree's real bytes are counted exactly once — never re-counted through
        // the loop — so the total is the two real files plus at most the symlink's own tiny length.
        let total = dir_size(&d.to_string_lossy()).unwrap();
        assert!((42 + BIG as u64..42 + BIG as u64 + 4096).contains(&total),
            "symlink must not be followed (no re-count); got {total}");
        // The per-child breakdown never recurses through the symlink: its child size stays tiny, nowhere
        // near BIG (which it would reach if the loop were followed back into d).
        let kids = dir_children_sizes(&d.to_string_lossy()).unwrap();
        let looped = kids.iter().find(|c| c.name == "loop").unwrap();
        assert!(looped.size < BIG as u64, "symlinked dir must not be walked; got {}", looped.size);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn children_sizes_break_down_per_child() {
        let d = scratch("children");
        fs::create_dir_all(d.join("sub")).unwrap();
        fs::write(d.join("sub/b.bin"), vec![0u8; 50]).unwrap();
        fs::write(d.join("top.bin"), vec![0u8; 10]).unwrap();
        let mut kids = dir_children_sizes(&d.to_string_lossy()).unwrap();
        kids.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(kids.len(), 2);
        let sub = kids.iter().find(|c| c.name == "sub").unwrap();
        assert!(sub.is_dir && sub.size == 50);
        let top = kids.iter().find(|c| c.name == "top.bin").unwrap();
        assert!(!top.is_dir && top.size == 10);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn stream_children_sizes_emits_each_child_with_its_size() {
        let d = scratch("streamkids");
        fs::create_dir_all(d.join("sub")).unwrap();
        fs::write(d.join("sub/b.bin"), vec![0u8; 50]).unwrap();
        fs::write(d.join("top.bin"), vec![0u8; 10]).unwrap();

        // Collect the streamed children from the rayon workers (arrival order varies).
        let seen = std::sync::Mutex::new(Vec::new());
        stream_children_sizes(&d.to_string_lossy(), |cs| seen.lock().unwrap().push(cs)).unwrap();
        let mut kids = seen.into_inner().unwrap();
        kids.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(kids.len(), 2);
        let sub = kids.iter().find(|c| c.name == "sub").unwrap();
        assert!(sub.is_dir && sub.size == 50);
        let top = kids.iter().find(|c| c.name == "top.bin").unwrap();
        assert!(!top.is_dir && top.size == 10);

        // A non-folder path is an error.
        assert!(stream_children_sizes(&d.join("top.bin").to_string_lossy(), |_| {}).is_err());
        let _ = fs::remove_dir_all(&d);
    }
}
