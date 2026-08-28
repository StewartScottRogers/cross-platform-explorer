//! File comparison (CPE-418, epic CPE-722). Pure and Tauri-free (CPE-815); the Tauri `files_identical`
//! command dispatches here.

use std::fs;
use std::path::Path;

use serde::Serialize;

use crate::fsutil::to_epoch_ms;

/// One node of a scanned tree (CPE-779). Serialized camelCase to match the frontend `CompareNode`
/// (`isDir`).
///
/// # `children: Some([])` is THREE different facts, and CPE-1925 is what happens when they are one
///
/// Before this ticket a childless directory node meant *any* of: the directory is genuinely empty on
/// disk; [`std::fs::read_dir`] failed on it so this scan never saw inside; or `max_depth` stopped the
/// descent at it. A consumer that wants to carry empty directories — `planBackup`, which now does —
/// cannot act on that: creating an empty directory in a backup destination because the source
/// directory *looked* childless is a **fabrication** when the real reason was that the scan could not
/// look. The two flags below carry the reason, so "empty" is a fact rather than an inference.
///
/// They are `Option<bool>` and skipped when `None` so a file node and an ordinary readable directory
/// serialize exactly as before — a 100,000-entry listing gains no bytes for the overwhelmingly common
/// case. `Some(true)` is the only value ever written; there is no `Some(false)`.
#[derive(Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct TreeNode {
    name: String,
    is_dir: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    modified: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    children: Option<Vec<TreeNode>>,
    /// `Some(true)` when [`std::fs::read_dir`] on this directory failed, so `children` is empty because
    /// the scan could not look inside — not because there is nothing there. Never set on a file.
    #[serde(skip_serializing_if = "Option::is_none")]
    unreadable: Option<bool>,
    /// `Some(true)` when `max_depth` stopped the descent at this directory, so `children` is empty for
    /// the same reason: unknown, not absent. Never set on a file.
    #[serde(skip_serializing_if = "Option::is_none")]
    truncated: Option<bool>,
}

/// Scan the children of `path` into a `CompareNode`-shaped tree (CPE-779), bounded by `max_depth` so a
/// pathological tree can't blow the stack or the payload (beyond the cap a dir is returned with no
/// children). Symlinks aren't followed and unreadable entries/dirs are skipped. `path` must be a folder.
pub fn scan_tree(path: &str, max_depth: u32) -> Result<Vec<TreeNode>, String> {
    let p = Path::new(path);
    if !p.is_dir() {
        return Err(format!("{path}: not a folder"));
    }
    Ok(scan_children(p, max_depth).0)
}

/// The children of `dir`, plus whether the directory could be **read at all**. The second half is the
/// CPE-1925 fact: an unreadable directory and an empty one both yield an empty `Vec`, and only the
/// caller that made the `read_dir` call can tell them apart.
fn scan_children(dir: &Path, depth_left: u32) -> (Vec<TreeNode>, bool) {
    let Ok(entries) = fs::read_dir(dir) else { return (Vec::new(), false) }; // skip unreadable dirs
    let mut out: Vec<TreeNode> = Vec::new();
    for entry in entries.flatten() {
        // metadata() doesn't traverse symlinks, so a symlink is neither dir nor file here and is skipped.
        let Ok(meta) = entry.metadata() else { continue };
        let name = entry.file_name().to_string_lossy().to_string();
        if meta.is_dir() {
            // Three outcomes, kept apart on purpose (see [`TreeNode`]): descended and read it;
            // descended and `read_dir` refused; did not descend because the depth cap stopped here.
            let (children, readable, truncated) = if depth_left > 0 {
                let (c, r) = scan_children(&entry.path(), depth_left - 1);
                (c, r, false)
            } else {
                (Vec::new(), true, true)
            };
            out.push(TreeNode {
                name,
                is_dir: true,
                size: None,
                modified: None,
                children: Some(children),
                unreadable: (!readable).then_some(true),
                truncated: truncated.then_some(true),
            });
        } else if meta.is_file() {
            out.push(TreeNode {
                name,
                is_dir: false,
                size: Some(meta.len()),
                modified: meta.modified().ok().and_then(to_epoch_ms),
                children: None,
                unreadable: None,
                truncated: None,
            });
        }
    }
    (out, true)
}

/// Whether two files have identical content. Different sizes short-circuit to `false`; otherwise the
/// bytes are streamed and compared with an early exit on the first difference — cheaper and
/// collision-free versus hashing both. A directory or unreadable path is an `Err`, never a panic.
pub fn files_identical(a: &str, b: &str) -> Result<bool, String> {
    use std::io::Read;
    let (pa, pb) = (Path::new(a), Path::new(b));
    let (ma, mb) = (
        fs::metadata(pa).map_err(|e| format!("{a}: {e}"))?,
        fs::metadata(pb).map_err(|e| format!("{b}: {e}"))?,
    );
    if ma.is_dir() || mb.is_dir() {
        return Err("folders can't be compared".into());
    }
    if ma.len() != mb.len() {
        return Ok(false); // different size ⇒ different content, no need to read
    }
    let mut fa = fs::File::open(pa).map_err(|e| format!("{a}: {e}"))?;
    let mut fb = fs::File::open(pb).map_err(|e| format!("{b}: {e}"))?;
    let (mut ba, mut bb) = ([0u8; 64 * 1024], [0u8; 64 * 1024]);
    loop {
        let na = fa.read(&mut ba).map_err(|e| format!("{a}: {e}"))?;
        // Same length overall, so read the same count from b (loop until filled or EOF).
        let mut nb = 0;
        while nb < na {
            let r = fb.read(&mut bb[nb..na]).map_err(|e| format!("{b}: {e}"))?;
            if r == 0 {
                break;
            }
            nb += r;
        }
        if na != nb || ba[..na] != bb[..nb] {
            return Ok(false);
        }
        if na == 0 {
            return Ok(true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> crate::fsutil::ScratchDir {
        crate::fsutil::scratch_dir(&format!("cpe-cmp-{tag}"))
    }

    #[test]
    fn scan_tree_builds_a_nested_size_mtime_tree() {
        let d = scratch("scan_tree");
        fs::create_dir_all(d.join("sub/deep")).unwrap();
        fs::write(d.join("a.txt"), b"hello").unwrap(); // 5 bytes
        fs::write(d.join("sub/b.txt"), b"yo").unwrap(); // 2 bytes
        fs::write(d.join("sub/deep/c.txt"), b"x").unwrap();

        let tree = scan_tree(&d.to_string_lossy(), 8).unwrap();
        let a = tree.iter().find(|n| n.name == "a.txt").unwrap();
        assert!(!a.is_dir && a.size == Some(5) && a.modified.is_some());
        let sub = tree.iter().find(|n| n.name == "sub").unwrap();
        assert!(sub.is_dir && sub.children.is_some());
        let subc = sub.children.as_ref().unwrap();
        assert!(subc.iter().any(|n| n.name == "b.txt" && n.size == Some(2)));
        let deep = subc.iter().find(|n| n.name == "deep").unwrap();
        assert_eq!(deep.children.as_ref().unwrap().len(), 1); // c.txt reached
        let _ = fs::remove_dir_all(&d);
    }

    /// CPE-1925: a genuinely empty directory is reported as empty **and readable**, so a consumer that
    /// carries empty directories (`planBackup`) can act on it. This is the positive half; the two
    /// tests below are the negative halves, and it is the *pair* that makes the flags mean anything —
    /// a scan that never set them would pass this test alone.
    #[test]
    fn scan_tree_marks_a_genuinely_empty_directory_as_neither_unreadable_nor_truncated() {
        let d = scratch("scan_empty");
        fs::create_dir_all(d.join("empty")).unwrap();
        let tree = scan_tree(&d.to_string_lossy(), 8).unwrap();
        let e = tree.iter().find(|n| n.name == "empty").unwrap();
        assert!(e.is_dir);
        assert_eq!(e.children.as_ref().unwrap().len(), 0);
        assert_eq!(e.unreadable, None, "an empty directory was read successfully");
        assert_eq!(e.truncated, None, "and the depth cap did not stop at it");
        let _ = fs::remove_dir_all(&d);
    }

    /// CPE-1925: a directory the depth cap stopped at reports `truncated`, so its empty `children` is
    /// not mistaken for "there is nothing inside". Without this flag the two are the same value.
    #[test]
    fn scan_tree_marks_a_depth_capped_directory_as_truncated() {
        let d = scratch("scan_trunc");
        fs::create_dir_all(d.join("lvl1/lvl2")).unwrap();
        fs::write(d.join("lvl1/lvl2/x.txt"), b"x").unwrap();
        let tree = scan_tree(&d.to_string_lossy(), 1).unwrap();
        let lvl1 = tree.iter().find(|n| n.name == "lvl1").unwrap();
        assert_eq!(lvl1.truncated, None, "lvl1 was descended into");
        let lvl2 = lvl1.children.as_ref().unwrap().iter().find(|n| n.name == "lvl2").unwrap();
        assert_eq!(lvl2.children.as_ref().unwrap().len(), 0);
        assert_eq!(lvl2.truncated, Some(true), "lvl2's children are unknown, not absent");
        assert_eq!(lvl2.unreadable, None, "and the reason is the cap, not access");
        let _ = fs::remove_dir_all(&d);
    }

    /// CPE-1925: a directory `read_dir` refuses reports `unreadable`. **Unix-only, and the reason is
    /// measured rather than assumed**: on Windows a deny ACE does not stop a directory enumeration the
    /// owner starts, so there is no portable way to make `read_dir` fail on demand here — the same
    /// per-platform split `fsutil::deny_dir_traversal`'s doc records at length. The Unix CI legs carry
    /// this one.
    #[cfg(unix)]
    #[test]
    fn scan_tree_marks_an_unreadable_directory_as_unreadable() {
        use std::os::unix::fs::PermissionsExt;
        let d = scratch("scan_unreadable");
        let locked = d.join("locked");
        fs::create_dir_all(&locked).unwrap();
        fs::write(locked.join("secret.txt"), b"s").unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

        let tree = scan_tree(&d.to_string_lossy(), 8).unwrap();
        let l = tree.iter().find(|n| n.name == "locked").unwrap();

        // Restore access before asserting, so a failing assertion cannot leave an undeletable dir.
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();

        assert_eq!(l.children.as_ref().unwrap().len(), 0, "nothing was listed");
        assert_eq!(l.unreadable, Some(true), "and the scan says it could not look, rather than implying empty");
        assert_eq!(l.truncated, None);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn scan_tree_honors_the_depth_cap_and_rejects_a_file() {
        let d = scratch("scan_depth");
        fs::create_dir_all(d.join("lvl1/lvl2")).unwrap();
        fs::write(d.join("lvl1/lvl2/x.txt"), b"x").unwrap();
        // depth 1: lvl1 is scanned, but lvl2's children are cut off (empty).
        let tree = scan_tree(&d.to_string_lossy(), 1).unwrap();
        let lvl1 = tree.iter().find(|n| n.name == "lvl1").unwrap();
        let lvl2 = lvl1.children.as_ref().unwrap().iter().find(|n| n.name == "lvl2").unwrap();
        assert_eq!(lvl2.children.as_ref().unwrap().len(), 0); // capped
        // a file path is an error, not a tree.
        assert!(scan_tree(&d.join("lvl1/lvl2/x.txt").to_string_lossy(), 4).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn files_identical_compares_content_and_short_circuits_on_size() {
        let d = std::env::temp_dir().join(format!("cpe-compare-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        let p = |n: &str| d.join(n).to_string_lossy().to_string();
        fs::write(d.join("a"), b"hello world").unwrap();
        fs::write(d.join("b"), b"hello world").unwrap(); // identical
        fs::write(d.join("c"), b"hello worlD").unwrap(); // same size, differing byte
        fs::write(d.join("e"), b"hello").unwrap(); // different size
        assert_eq!(files_identical(&p("a"), &p("b")), Ok(true));
        assert_eq!(files_identical(&p("a"), &p("c")), Ok(false));
        assert_eq!(files_identical(&p("a"), &p("e")), Ok(false));
        // Two empty files are identical.
        fs::write(d.join("z1"), b"").unwrap();
        fs::write(d.join("z2"), b"").unwrap();
        assert_eq!(files_identical(&p("z1"), &p("z2")), Ok(true));
        // A folder or a missing path is an error.
        assert!(files_identical(&p("a"), &d.to_string_lossy()).is_err());
        assert!(files_identical(&p("a"), &p("nope")).is_err());
        let _ = fs::remove_dir_all(&d);
    }
}
