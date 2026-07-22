//! Provider-agnostic recursive walk + tree transfer (CPE-684/905, epic CPE-616). These operate over the
//! [`FileSystemProvider`] trait, so **every** backend — local disk, SFTP, WebDAV, … — gets a cancellable
//! recursive walk and bidirectional (remote⇄local) tree copy for free, with the logic living once here
//! instead of duplicated per provider.
//!
//! Paths are the provider's own convention (`/`-separated for remote backends; an empty `root` means the
//! provider's root). Every step checks a `cancel` flag so a slow/large enumeration or transfer stops
//! promptly.

use crate::provider::FileSystemProvider;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// One entry yielded by [`walk`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalkEntry {
    /// Full path within the provider.
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

/// Join a directory + child name. An empty `dir` (the provider root) yields the bare name — so a
/// remote root of `/` produces `/name` while a `FakeProvider`/relative root of `` produces `name`.
fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{}/{}", dir.trim_end_matches('/'), name)
    }
}

/// Recursively walk the tree under `root` (depth-first), invoking `on_entry` for every file and directory.
/// `cancel` is checked before each directory listing and each entry. A directory that can't be listed is
/// skipped rather than aborting the walk. Returns the number of entries visited.
pub fn walk(
    provider: &dyn FileSystemProvider,
    root: &str,
    cancel: &AtomicBool,
    mut on_entry: impl FnMut(WalkEntry),
) -> Result<usize, String> {
    let mut stack = vec![root.to_string()];
    let mut visited = 0usize;
    while let Some(dir) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let Ok(entries) = provider.list(&dir) else { continue };
        for entry in entries {
            if cancel.load(Ordering::Relaxed) {
                return Ok(visited);
            }
            let path = join(&dir, &entry.name);
            visited += 1;
            let is_dir = entry.is_dir;
            on_entry(WalkEntry { path: path.clone(), name: entry.name, is_dir, size: entry.size });
            if is_dir {
                stack.push(path);
            }
        }
    }
    Ok(visited)
}

/// Download the tree under `remote_root` into `local_dir`, recreating the directory structure. Returns the
/// number of files written. Cancellable.
pub fn download_tree(
    provider: &dyn FileSystemProvider,
    remote_root: &str,
    local_dir: &Path,
    cancel: &AtomicBool,
) -> Result<usize, String> {
    let base = remote_root.trim_end_matches('/').to_string();
    let mut entries = Vec::new();
    walk(provider, remote_root, cancel, |e| entries.push(e))?;

    std::fs::create_dir_all(local_dir).map_err(|e| format!("{}: {e}", local_dir.display()))?;
    let mut files = 0usize;
    for entry in &entries {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let rel = entry.path.strip_prefix(&base).unwrap_or(&entry.path).trim_start_matches('/');
        let local = local_dir.join(rel);
        if entry.is_dir {
            std::fs::create_dir_all(&local).map_err(|e| format!("{}: {e}", local.display()))?;
        } else {
            if let Some(parent) = local.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
            }
            let data = provider.read(&entry.path)?;
            std::fs::write(&local, data).map_err(|e| format!("{}: {e}", local.display()))?;
            files += 1;
        }
    }
    Ok(files)
}

/// Upload the local tree under `local_dir` into `remote_root`, recreating the structure — the symmetric
/// counterpart to [`download_tree`]. Returns the number of files written. Cancellable. Local `\` are mapped
/// to `/` so a Windows source produces provider-native paths.
pub fn upload_tree(
    provider: &mut dyn FileSystemProvider,
    local_dir: &Path,
    remote_root: &str,
    cancel: &AtomicBool,
) -> Result<usize, String> {
    let base = remote_root.trim_end_matches('/').to_string();
    provider.mkdir(&base)?; // ensure the remote root exists
    let mut stack = vec![local_dir.to_path_buf()];
    let mut files = 0usize;
    while let Some(dir) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let Ok(read_dir) = std::fs::read_dir(&dir) else { continue };
        for entry in read_dir.flatten() {
            if cancel.load(Ordering::Relaxed) {
                return Ok(files);
            }
            let local = entry.path();
            let Ok(rel) = local.strip_prefix(local_dir) else { continue };
            let remote = join(&base, &rel.to_string_lossy().replace('\\', "/"));
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                provider.mkdir(&remote)?;
                stack.push(local);
            } else {
                let data = std::fs::read(&local).map_err(|e| format!("{}: {e}", local.display()))?;
                provider.write(&remote, &data)?;
                files += 1;
            }
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::FakeProvider;

    /// A `FakeProvider` seeded with `a.txt` + `sub/b.txt` (an empty `""` root, no leading slash).
    fn seeded() -> FakeProvider {
        let mut fs = FakeProvider::new();
        fs.write("a.txt", b"alpha").unwrap();
        fs.write("sub/b.txt", b"bravo").unwrap();
        fs
    }

    #[test]
    fn walk_recurses_every_file_and_dir() {
        let fs = seeded();
        let cancel = AtomicBool::new(false);
        let mut paths: Vec<_> = Vec::new();
        let n = walk(&fs, "", &cancel, |e| paths.push((e.path, e.is_dir))).unwrap();
        paths.sort();
        assert_eq!(n, 3, "a.txt + sub + sub/b.txt; got {paths:?}");
        assert!(paths.contains(&("a.txt".to_string(), false)));
        assert!(paths.contains(&("sub".to_string(), true)));
        assert!(paths.contains(&("sub/b.txt".to_string(), false)));
    }

    #[test]
    fn walk_stops_when_cancelled() {
        let fs = seeded();
        let cancel = AtomicBool::new(false);
        let mut count = 0;
        let visited = walk(&fs, "", &cancel, |_| {
            count += 1;
            cancel.store(true, Ordering::Relaxed);
        })
        .unwrap();
        assert_eq!((visited, count), (1, 1));
    }

    #[test]
    fn download_tree_writes_the_provider_files_locally() {
        let fs = seeded();
        let out = std::env::temp_dir().join(format!("cpe-xfer-dl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out);
        let cancel = AtomicBool::new(false);
        let files = download_tree(&fs, "", &out, &cancel).unwrap();
        assert_eq!(files, 2);
        assert_eq!(std::fs::read(out.join("a.txt")).unwrap(), b"alpha");
        assert_eq!(std::fs::read(out.join("sub").join("b.txt")).unwrap(), b"bravo");
        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn upload_tree_writes_local_files_into_the_provider() {
        let src = std::env::temp_dir().join(format!("cpe-xfer-up-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&src);
        std::fs::create_dir_all(src.join("inner")).unwrap();
        std::fs::write(src.join("x.txt"), b"ex").unwrap();
        std::fs::write(src.join("inner").join("y.txt"), b"why").unwrap();

        let mut fs = FakeProvider::new();
        let cancel = AtomicBool::new(false);
        let files = upload_tree(&mut fs, &src, "dest", &cancel).unwrap();
        assert_eq!(files, 2);
        assert_eq!(fs.read("dest/x.txt").unwrap(), b"ex");
        assert_eq!(fs.read("dest/inner/y.txt").unwrap(), b"why");
        let _ = std::fs::remove_dir_all(&src);
    }
}
