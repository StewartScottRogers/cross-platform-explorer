//! File comparison (CPE-418, epic CPE-722). Pure and Tauri-free (CPE-815); the Tauri `files_identical`
//! command dispatches here.

use std::fs;
use std::path::Path;

use serde::Serialize;

use crate::fsutil::to_epoch_ms;

/// One node of a scanned tree (CPE-779). Serialized camelCase to match the frontend `CompareNode`
/// (`isDir`).
///
/// # `children: Some([])` is SEVEN different facts, and CPE-1925 is what happens when they are one
///
/// Before this ticket a childless directory node meant *any* of: the directory is genuinely empty on
/// disk; [`std::fs::read_dir`] failed on it so this scan never saw inside; or `max_depth` stopped the
/// descent at it. A consumer that wants to carry empty directories — `planBackup`, which now does —
/// cannot act on that: creating an empty directory in a backup destination because the source
/// directory *looked* childless is a **fabrication** when the real reason was that the scan could not
/// look. The two flags below carry the reason, so "empty" is a fact rather than an inference.
///
/// The first round of this ticket enumerated three ways a directory's `children` can come back short.
/// There are **seven**, and the round-2 review measured three of the four that were missed as
/// **destructive** — a mirror backup deleting the destination's copies of files the scan never saw:
///
/// | # | how the listing came up short | flag | verdict |
/// |---|---|---|---|
/// | 1 | genuinely empty on disk | none | correct — "empty" is the fact |
/// | 2 | [`std::fs::read_dir`] itself returned `Err` (`d---------`) | `unreadable` | correct |
/// | 3 | `depth_left == 0`, so the descent stopped here | `truncated` | correct |
/// | 4 | `read_dir` was `Ok` but **every** [`std::fs::DirEntry::metadata`] failed (`dr--------`: the read bit lists names, the missing search bit refuses every `stat`) | `unreadable` | **was `none` — destructive** |
/// | 5 | `read_dir` was `Ok` but the iterator yielded an `Err` mid-enumeration | `unreadable` | **was `none` — destructive, and yields a PARTIAL list** |
/// | 6 | every child is a symlink / fifo / socket / device | none | correct — a **type filter**, not an access failure; see below |
/// | 7 | the **root** handed to [`scan_tree`] is itself unreadable | `Err`, not a node | **was a silent `Ok([])` — destructive over the WHOLE destination** |
///
/// **4, 5 and 6 look alike and are not.** 6 is this scanner deciding a symlink is not a thing a
/// compare tree carries — a deliberate exclusion, the same one every run makes, and the caller loses
/// nothing it could have used. 4 and 5 are the operating system refusing to tell us what is there. A
/// flag that lumped them together would have to be set on 6 too, which would make every ordinary
/// directory holding a symlink "unknown" and neuter the flag entirely. So the rule is: **an entry
/// dropped because of an `Err` sets `unreadable`; an entry dropped because of its type does not.**
///
/// **5 is the nastiest of them,** because it is the only one whose `children` can be non-empty. A
/// partial listing does not read as suspicious anywhere downstream — the files that *are* listed look
/// fine, and the ones that were dropped diff as "removed from the source", which in a mirror run means
/// *delete the backup's only copy*. `unreadable` is therefore **not** "children is empty for want of
/// access"; it is "**this list is not the whole truth**", and a node carrying it may well have
/// children.
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
    /// `Some(true)` when this directory's listing is **not the whole truth** — cases 2, 4 and 5 of the
    /// table above: [`std::fs::read_dir`] failed outright, or it succeeded and one or more entries were
    /// dropped because reading them errored. `children` is therefore short (often empty, sometimes
    /// *partial*) because the scan could not look, not because there is nothing there. An entry
    /// excluded for its **type** — a symlink, fifo, socket or device — never sets this. Never set on a
    /// file.
    #[serde(skip_serializing_if = "Option::is_none")]
    unreadable: Option<bool>,
    /// `Some(true)` when `max_depth` stopped the descent at this directory, so `children` is empty for
    /// the same reason: unknown, not absent. Never set on a file.
    #[serde(skip_serializing_if = "Option::is_none")]
    truncated: Option<bool>,
}

/// Scan the children of `path` into a `CompareNode`-shaped tree (CPE-779), bounded by `max_depth` so a
/// pathological tree can't blow the stack or the payload (beyond the cap a dir is returned with no
/// children). Symlinks aren't followed. `path` must be a folder, and one this process can actually
/// list — see below. Entries *inside* the tree that can't be read are kept, flagged `unreadable`.
///
/// # The root gets no node, so an unlistable root is an [`Err`] (CPE-1925 round 2, case 7)
///
/// Every other directory in the tree has a [`TreeNode`] to carry the "this listing is not the whole
/// truth" flag on. The root does not: it is the `Vec` itself. So until round 2 this function computed
/// exactly that fact — `scan_children`'s second return value — and **threw it away**, returning
/// `Ok(vec![])` for a root that `read_dir` had flatly refused. `p.is_dir()` stats the root through its
/// *parent* and succeeds, so nothing upstream noticed either.
///
/// Measured on the round-1 branch, on ext4, with a `0o000` source root: the scan returned `[]`, and
/// `planBackup` in mirror mode turned that into `delete: ["a.txt", "b.txt"]` — **the entire
/// destination, planned for deletion, silently**. And it is reachable unattended: `runBackupJobNow` is
/// fired by the drive-connect scheduler against a stored `job.source`, and a volume that comes back
/// with different ownership after a remount is exactly this shape.
///
/// An `Err` is the honest answer and the fail-closed one. It joins the `Err` this function already
/// returns for "not a folder", and every caller already handles that: `BackupDashboard` shows it on
/// the row, `runBackupJobNow` raises the auto-backup-failed toast, `CompareDialog` falls through to
/// its file-compare path, the saved-search loader yields no entries. None of them deletes anything.
///
/// Note what this deliberately does **not** do: an unreadable directory *below* the root is still a
/// flagged node, not an error, because there the flag has somewhere to live and the rest of the tree
/// is still worth scanning. Only the root — where a short list is indistinguishable from an empty
/// one — fails the whole call.
pub fn scan_tree(path: &str, max_depth: u32) -> Result<Vec<TreeNode>, String> {
    let p = Path::new(path);
    if !p.is_dir() {
        return Err(format!("{path}: not a folder"));
    }
    let (children, whole_truth) = scan_children(p, max_depth);
    if !whole_truth {
        // Deliberately not "is empty" and not "access denied": the honest statement is that we cannot
        // vouch for the list, which covers `read_dir` refusing outright and an entry failing to stat.
        return Err(format!(
            "{path}: could not be listed completely, so this scan cannot say what is in it"
        ));
    }
    Ok(children)
}

/// The children of `dir`, plus whether that list is **the whole truth** — `true` only when every entry
/// the OS holds was seen. The second half is the CPE-1925 fact: a directory the scan could not fully
/// read and an empty one both yield a short `Vec`, and only the caller that made the `read_dir` call
/// can tell them apart. See [`TreeNode`] for the seven-way table this implements.
fn scan_children(dir: &Path, depth_left: u32) -> (Vec<TreeNode>, bool) {
    // Case 2: nothing to list, nothing to iterate.
    let Ok(entries) = fs::read_dir(dir) else { return (Vec::new(), false) };
    let mut out: Vec<TreeNode> = Vec::new();
    // Cleared by any entry dropped for an ERROR; untouched by entries dropped for their TYPE.
    let mut whole_truth = true;
    for entry in entries {
        // Cases 4 and 5 share this one arm on purpose — read them off the two `.ok()`s below. 5 is the
        // `DirEntry` itself coming back `Err` mid-enumeration; 4 is the entry arriving fine and
        // `metadata()` (an `fstatat` against the directory) being refused, which is what a `dr--------`
        // directory does to EVERY child: the read bit is enough to list the names and the missing
        // search bit refuses every stat. Before round 2 both were a bare `continue` and the directory
        // reported itself readable-and-empty.
        //
        // Sharing the arm is also what makes 4's on-disk test cover 5, which has no portable fixture:
        // you cannot make `readdir` fail on demand. That is a claim about these three lines and
        // nothing else — verify it by reading them, not by trusting this comment.
        let Some((entry, meta)) = entry.ok().and_then(|e| e.metadata().ok().map(|m| (e, m))) else {
            whole_truth = false;
            continue;
        };
        let name = entry.file_name().to_string_lossy().to_string();
        if meta.is_dir() {
            // Three outcomes, kept apart on purpose (see [`TreeNode`]): descended and read the whole
            // thing; descended and could not read all of it; did not descend because the cap stopped here.
            let (children, child_whole_truth, truncated) = if depth_left > 0 {
                let (c, w) = scan_children(&entry.path(), depth_left - 1);
                (c, w, false)
            } else {
                (Vec::new(), true, true)
            };
            out.push(TreeNode {
                name,
                is_dir: true,
                size: None,
                modified: None,
                children: Some(children),
                unreadable: (!child_whole_truth).then_some(true),
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
        // Case 6: a symlink, fifo, socket or device. `metadata()` here is a no-follow stat, so a
        // symlink is neither dir nor file and falls out. That is a TYPE filter — a deliberate,
        // every-run exclusion the caller loses nothing by — so `whole_truth` stays set. Flagging it
        // would mark every ordinary directory holding a symlink "unknown" and the flag would mean
        // nothing anywhere.
    }
    (out, whole_truth)
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

    /// CPE-1925 round 2, **case 4** of [`TreeNode`]'s table — the destructive one the first round
    /// missed, because it does not look like an access failure from the inside.
    ///
    /// A `dr--------` directory has the read bit but not the search bit. [`fs::read_dir`] therefore
    /// **succeeds** (listing names needs read), the iterator yields every child, and
    /// [`fs::DirEntry::metadata`] — an `fstatat` against the directory, which needs search — is refused
    /// for every one of them. Round 1's `let Ok(meta) = ... else { continue }` dropped them all and
    /// then returned `(out, true)`: readable, and empty.
    ///
    /// Measured on the round-1 branch, ext4: the scan emitted
    /// `{"name":"nosearch","isDir":true,"children":[]}` with neither flag, `planBackup` in mirror mode
    /// emitted `delete: ["nosearch/a.txt"]` and `skippedDirs: []`, and the run reported `ok=1 fail=0`
    /// having deleted the destination's only copy of a file that is still in the source. Zero
    /// disclosure, which is exactly the shape this ticket exists to end.
    ///
    /// Unix-only for the same measured reason as the test above.
    ///
    /// **Red-proof, run on real ext4 and recorded here.** Putting the round-1 arm back — dropping the
    /// `whole_truth = false` so an entry that failed to read is a bare `continue` — reds **this test
    /// and only this test**: `8 passed; 1 failed`, on `unreadable: None` with `children=Some(0)`. The
    /// three older `scan_tree` flag tests all stayed green under it, which is the whole point: none of
    /// them could reach this arm.
    #[cfg(unix)]
    #[test]
    fn scan_tree_marks_a_listable_but_unstatable_directory_as_unreadable() {
        use std::os::unix::fs::PermissionsExt;
        let d = scratch("scan_nosearch");
        let nosearch = d.join("nosearch");
        fs::create_dir_all(&nosearch).unwrap();
        fs::write(nosearch.join("a.txt"), b"a").unwrap();
        // 0o400: read, no search. Distinct from the 0o000 above, and the distinction is the test.
        fs::set_permissions(&nosearch, fs::Permissions::from_mode(0o400)).unwrap();

        // Liveness: the fixture must really be the case-4 shape and not the case-2 one, or this test
        // is a duplicate of the previous one wearing a different name. `read_dir` has to SUCCEED.
        let listed = fs::read_dir(&nosearch).map(|it| it.count()).ok();
        let stat_refused = fs::metadata(nosearch.join("a.txt")).is_err();

        let tree = scan_tree(&d.to_string_lossy(), 8).unwrap();
        let n = tree.iter().find(|n| n.name == "nosearch").unwrap();

        fs::set_permissions(&nosearch, fs::Permissions::from_mode(0o755)).unwrap();

        if listed != Some(1) || !stat_refused {
            crate::skip_notice!(
                "SKIPPING scan_tree_marks_a_listable_but_unstatable_directory_as_unreadable: this \
                 filesystem does not produce the read-without-search shape (read_dir gave {listed:?}, \
                 stat refused: {stat_refused}). NOTHING on this run covered case 4 — a directory whose \
                 names list but whose entries cannot be stat'd"
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }

        assert_eq!(
            n.unreadable,
            Some(true),
            "CPE-1925 case 4: every child was dropped because stat was REFUSED, so this listing is not \
             the whole truth and must say so. Reporting it as an ordinary empty directory is what let a \
             mirror run delete the destination's only copies. Node: children={:?} truncated={:?}",
            n.children.as_ref().map(|c| c.len()),
            n.truncated
        );
        assert_eq!(n.truncated, None, "the reason is access, not the depth cap");
        let _ = fs::remove_dir_all(&d);
    }

    /// **Case 6, the one that must NOT be flagged** — and the reason case 4 needed a `whole_truth`
    /// flag rather than "did we drop anything?". Symlinks, fifos, sockets and devices are excluded by
    /// *type*: a deliberate, every-run decision the caller loses nothing by. If dropping them set
    /// `unreadable`, every ordinary directory that happens to hold a symlink would report its contents
    /// unknown, `planBackup` would refuse to create it and suppress mirror deletes underneath it, and
    /// the flag would mean nothing anywhere. Paired with the case-4 test above on purpose: it is the
    /// pair that pins the distinction, since either one alone is satisfied by a constant.
    #[cfg(unix)]
    #[test]
    fn scan_tree_does_not_flag_a_directory_whose_children_are_excluded_by_type() {
        let d = scratch("scan_typefilter");
        let links = d.join("links");
        fs::create_dir_all(&links).unwrap();
        fs::write(d.join("target.txt"), b"t").unwrap();
        std::os::unix::fs::symlink(d.join("target.txt"), links.join("a-link")).unwrap();
        std::os::unix::fs::symlink(d.join("nowhere"), links.join("a-dangling-link")).unwrap();

        let tree = scan_tree(&d.to_string_lossy(), 8).unwrap();
        let l = tree.iter().find(|n| n.name == "links").unwrap();
        assert_eq!(l.children.as_ref().unwrap().len(), 0, "both children are symlinks, so neither is carried");
        assert_eq!(
            l.unreadable, None,
            "a TYPE exclusion is not an access failure — flagging it would neuter the flag for case 4"
        );
        assert_eq!(l.truncated, None);
        let _ = fs::remove_dir_all(&d);
    }

    /// CPE-1925 round 2, **case 7**: the root has no [`TreeNode`] to carry a flag on, so an unlistable
    /// root is an [`Err`]. Round 1 computed the very bool this needed — `scan_children(p, ..).0`
    /// discarded the `.1` — and returned `Ok([])`, which is byte-identical to an empty folder.
    ///
    /// Measured on the round-1 branch, ext4, `0o000` source root: `scan_tree` returned `[]` and
    /// `planBackup` in mirror mode turned it into `delete: ["a.txt", "b.txt"]` — the WHOLE destination,
    /// silently. Largest blast radius of the three, and reachable on the unattended drive-connect path.
    ///
    /// Unix-only for the same measured reason as the two tests above.
    ///
    /// **Red-proof, run on real ext4 and recorded here.** Disabling the refusal (`if false &&
    /// !whole_truth`) reds **this test and only this test**: `8 passed; 1 failed`, panicking with "an
    /// unlistable root must be an Err, got Ok with 0 node(s)". Nothing else in the module noticed,
    /// because the root is the one directory with no node to carry a flag.
    #[cfg(unix)]
    #[test]
    fn scan_tree_refuses_an_unlistable_root_instead_of_calling_it_empty() {
        use std::os::unix::fs::PermissionsExt;
        let d = scratch("scan_root_locked");
        let root = d.join("root");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("a.txt"), b"a").unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o000)).unwrap();

        // `is_dir()` stats the root through its PARENT, which is readable — so the old guard passed and
        // the failure was entirely invisible. Pin that, because it is why the bug existed.
        let looks_like_a_folder = root.is_dir();
        // Liveness, measured WHILE the mode is still 0o000: running as root (some CI containers do)
        // defeats every mode-based fixture, and a green assertion on an inert fixture is worse than a
        // skip. Must be read before the restore below, or it is always `true`.
        let inert = fs::read_dir(&root).is_ok();
        let scanned = scan_tree(&root.to_string_lossy(), 8);

        fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();

        if !looks_like_a_folder || inert {
            crate::skip_notice!(
                "SKIPPING scan_tree_refuses_an_unlistable_root_instead_of_calling_it_empty: a 0o000 \
                 directory is still listable here (running as root?). NOTHING on this run covered case \
                 7 — an unreadable scan ROOT"
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }
        assert!(looks_like_a_folder, "the fixture must still pass the `not a folder` guard");

        // `TreeNode` is deliberately not `Debug` (it is a serialization type), so unwrap by hand.
        let err = match scanned {
            Err(e) => e,
            Ok(nodes) => panic!(
                "CPE-1925 case 7: an unlistable root must be an Err, got Ok with {} node(s). Ok([]) is \
                 indistinguishable from an empty folder, and planBackup turns that into `delete \
                 everything in the destination`",
                nodes.len()
            ),
        };
        assert!(err.contains("could not be listed"), "and it must say what went wrong: {err}");
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
