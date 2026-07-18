//! Filesystem provider abstraction (CPE-681, epic CPE-616): a small trait every "location" backend
//! implements — the **local** disk today, and later SFTP/SMB/WebDAV/S3 — so higher layers can operate on
//! a location by interface rather than assuming local paths. This is the enabling seam; wiring the
//! existing Tauri commands through it is a separate (attended) step (CPE-685). Ships with a `LocalProvider`
//! over `std::fs` and an in-memory `FakeProvider` for tests, per the epic's DoD.

#![allow(dead_code)] // consumed once commands route through providers (CPE-685); kept compiled + tested now.

use std::collections::BTreeMap;

/// Minimal metadata for one entry, provider-agnostic (no OS-specific fields — a remote may not have them).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

/// The operations a location backend must support. Errors are human-readable strings (surfaced to the UI),
/// matching the rest of the command layer. Paths are provider-relative, forward-slash separated.
pub trait FileSystemProvider {
    fn list(&self, path: &str) -> Result<Vec<ProviderEntry>, String>;
    fn stat(&self, path: &str) -> Result<ProviderEntry, String>;
    fn read(&self, path: &str) -> Result<Vec<u8>, String>;
    fn write(&mut self, path: &str, data: &[u8]) -> Result<(), String>;
    fn mkdir(&mut self, path: &str) -> Result<(), String>;
    fn delete(&mut self, path: &str) -> Result<(), String>;
}

/// The local disk, over `std::fs`. A thin wrapper so the existing behaviour (skip-unreadable, etc.) can be
/// layered in when commands route through it (CPE-685).
pub struct LocalProvider;

impl FileSystemProvider for LocalProvider {
    fn list(&self, path: &str) -> Result<Vec<ProviderEntry>, String> {
        let read = std::fs::read_dir(path).map_err(|e| format!("{path}: {e}"))?;
        let mut out = Vec::new();
        for entry in read.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            let is_dir = meta.is_dir();
            out.push(ProviderEntry {
                name: entry.file_name().to_string_lossy().to_string(),
                is_dir,
                size: if is_dir { 0 } else { meta.len() },
            });
        }
        Ok(out)
    }

    fn stat(&self, path: &str) -> Result<ProviderEntry, String> {
        let meta = std::fs::metadata(path).map_err(|e| format!("{path}: {e}"))?;
        let is_dir = meta.is_dir();
        Ok(ProviderEntry {
            name: std::path::Path::new(path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string()),
            is_dir,
            size: if is_dir { 0 } else { meta.len() },
        })
    }

    fn read(&self, path: &str) -> Result<Vec<u8>, String> {
        std::fs::read(path).map_err(|e| format!("{path}: {e}"))
    }

    fn write(&mut self, path: &str, data: &[u8]) -> Result<(), String> {
        std::fs::write(path, data).map_err(|e| format!("{path}: {e}"))
    }

    fn mkdir(&mut self, path: &str) -> Result<(), String> {
        std::fs::create_dir_all(path).map_err(|e| format!("{path}: {e}"))
    }

    fn delete(&mut self, path: &str) -> Result<(), String> {
        let p = std::path::Path::new(path);
        let r = if p.is_dir() { std::fs::remove_dir_all(p) } else { std::fs::remove_file(p) };
        r.map_err(|e| format!("{path}: {e}"))
    }
}

/// An in-memory provider for tests (and a reference implementation of the contract). Paths are normalised
/// to forward slashes without a trailing slash; the root is "". Every ancestor directory of a written file
/// is created implicitly, mirroring `mkdir -p` semantics so tests read naturally.
#[derive(Default)]
pub struct FakeProvider {
    dirs: std::collections::BTreeSet<String>,
    files: BTreeMap<String, Vec<u8>>,
}

fn norm(path: &str) -> String {
    path.replace('\\', "/").trim_end_matches('/').to_string()
}

fn parent_of(path: &str) -> Option<&str> {
    path.rfind('/').map(|i| &path[..i])
}

impl FakeProvider {
    pub fn new() -> Self {
        Self::default()
    }

    fn ensure_ancestors(&mut self, path: &str) {
        let mut cur = parent_of(path);
        while let Some(p) = cur {
            if p.is_empty() {
                break;
            }
            self.dirs.insert(p.to_string());
            cur = parent_of(p);
        }
    }
}

impl FileSystemProvider for FakeProvider {
    fn list(&self, path: &str) -> Result<Vec<ProviderEntry>, String> {
        let dir = norm(path);
        if !dir.is_empty() && !self.dirs.contains(&dir) {
            return Err(format!("{path}: not a directory"));
        }
        let prefix = if dir.is_empty() { String::new() } else { format!("{dir}/") };
        // Direct children only: the first path segment after `prefix`. A segment that is the whole
        // remainder is the item itself (dir or file); a segment with more after it names an intermediate
        // child directory.
        let mut children: BTreeMap<String, bool> = BTreeMap::new(); // name → is_dir
        let entries = self.dirs.iter().map(|d| (d, true)).chain(self.files.keys().map(|f| (f, false)));
        for (full, is_dir) in entries {
            let Some(rest) = full.strip_prefix(&prefix) else { continue };
            if rest.is_empty() {
                continue;
            }
            match rest.split_once('/') {
                None => {
                    children.insert(rest.to_string(), is_dir);
                }
                Some((seg, _)) => {
                    children.entry(seg.to_string()).or_insert(true);
                }
            }
        }
        Ok(children
            .into_iter()
            .map(|(name, is_dir)| ProviderEntry {
                size: if is_dir { 0 } else { self.files.get(&format!("{prefix}{name}")).map(|v| v.len() as u64).unwrap_or(0) },
                is_dir,
                name,
            })
            .collect())
    }

    fn stat(&self, path: &str) -> Result<ProviderEntry, String> {
        let p = norm(path);
        let name = p.rsplit('/').next().unwrap_or(&p).to_string();
        if self.dirs.contains(&p) {
            Ok(ProviderEntry { name, is_dir: true, size: 0 })
        } else if let Some(data) = self.files.get(&p) {
            Ok(ProviderEntry { name, is_dir: false, size: data.len() as u64 })
        } else {
            Err(format!("{path}: not found"))
        }
    }

    fn read(&self, path: &str) -> Result<Vec<u8>, String> {
        self.files.get(&norm(path)).cloned().ok_or_else(|| format!("{path}: not found"))
    }

    fn write(&mut self, path: &str, data: &[u8]) -> Result<(), String> {
        let p = norm(path);
        self.ensure_ancestors(&p);
        self.files.insert(p, data.to_vec());
        Ok(())
    }

    fn mkdir(&mut self, path: &str) -> Result<(), String> {
        let p = norm(path);
        self.ensure_ancestors(&p);
        if !p.is_empty() {
            self.dirs.insert(p);
        }
        Ok(())
    }

    fn delete(&mut self, path: &str) -> Result<(), String> {
        let p = norm(path);
        let prefix = format!("{p}/");
        let existed = self.files.remove(&p).is_some() | self.dirs.remove(&p);
        // Remove everything under a deleted directory.
        self.files.retain(|k, _| !k.starts_with(&prefix));
        self.dirs.retain(|k| !k.starts_with(&prefix));
        if existed {
            Ok(())
        } else {
            Err(format!("{path}: not found"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_provider_round_trips() {
        let mut fs = FakeProvider::new();
        fs.mkdir("a/b").unwrap();
        fs.write("a/b/hi.txt", b"hello").unwrap();
        fs.write("a/top.txt", b"xy").unwrap();

        assert_eq!(fs.read("a/b/hi.txt").unwrap(), b"hello");
        assert_eq!(fs.stat("a/b/hi.txt").unwrap(), ProviderEntry { name: "hi.txt".into(), is_dir: false, size: 5 });
        assert!(fs.stat("a/b").unwrap().is_dir);

        // list "a" → the "b" dir + "top.txt" file (direct children only).
        let mut names: Vec<_> = fs.list("a").unwrap().into_iter().map(|e| (e.name, e.is_dir)).collect();
        names.sort();
        assert_eq!(names, vec![("b".to_string(), true), ("top.txt".to_string(), false)]);
    }

    #[test]
    fn fake_provider_delete_removes_subtree() {
        let mut fs = FakeProvider::new();
        fs.write("d/x.txt", b"1").unwrap();
        fs.write("d/e/y.txt", b"2").unwrap();
        fs.delete("d").unwrap();
        assert!(fs.read("d/x.txt").is_err());
        assert!(fs.read("d/e/y.txt").is_err());
        assert!(fs.stat("d").is_err());
        assert!(fs.delete("d").is_err(), "deleting a missing path errors");
    }

    #[test]
    fn local_provider_matches_the_contract() {
        let dir = std::env::temp_dir().join(format!("cpe_prov_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let base = dir.to_string_lossy().to_string();
        let mut lp = LocalProvider;

        lp.mkdir(&format!("{base}/sub")).unwrap();
        lp.write(&format!("{base}/sub/f.txt"), b"data").unwrap();
        assert_eq!(lp.read(&format!("{base}/sub/f.txt")).unwrap(), b"data");
        assert_eq!(lp.stat(&format!("{base}/sub/f.txt")).unwrap().size, 4);
        assert!(lp.stat(&format!("{base}/sub")).unwrap().is_dir);
        let names: Vec<_> = lp.list(&format!("{base}/sub")).unwrap().into_iter().map(|e| e.name).collect();
        assert_eq!(names, vec!["f.txt".to_string()]);
        lp.delete(&format!("{base}/sub")).unwrap();
        assert!(lp.stat(&format!("{base}/sub")).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
