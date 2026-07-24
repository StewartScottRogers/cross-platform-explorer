//! Pure organization / declutter rules engine (CPE-987, epic CPE-979 "AI auto-organize & declutter").
//! Given a flat, already-gathered list of directory entries and a chosen [`OrganizeRule`], compute the
//! proposed destination subfolder for each **file**. Fully deterministic, filesystem-free, and Tauri-free:
//! NO AI and NO I/O — the caller supplies the entries and a later move engine executes the proposals.
//! Directories are left in place. Companion to [`crate::duplicates`] / [`crate::restore_plan`] as another
//! pure planning slice.

/// One directory entry to consider, gathered by the caller (no filesystem access happens here).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrganizeEntry {
    /// File (or directory) name as it appears in its parent — the thing a proposal moves.
    pub name: String,
    /// True for directories; directories are never proposed for moving.
    pub is_dir: bool,
    /// Extension, **lowercased and without the leading dot** (`""` when the file has none).
    pub ext: String,
    /// Size in bytes (used by [`OrganizeRule::BySizeBucket`]).
    pub size: u64,
    /// Last-modified time as UTC unix epoch **seconds** (used by [`OrganizeRule::ByModifiedYear`]).
    pub modified_secs: u64,
}

impl OrganizeEntry {
    /// Convenience constructor mirroring the struct field order.
    pub fn new(name: impl Into<String>, is_dir: bool, ext: impl Into<String>, size: u64, modified_secs: u64) -> Self {
        Self { name: name.into(), is_dir, ext: ext.into(), size, modified_secs }
    }
}

/// The declarative rule that decides each file's destination subfolder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrganizeRule {
    /// Group by content category (Images / Documents / Audio / Video / Archives / Code / Other).
    ByKind,
    /// Group by uppercased extension (e.g. `PNG`), or `NoExtension` when there is none.
    ByExtension,
    /// Group by the 4-digit year of the last-modified time.
    ByModifiedYear,
    /// Group into coarse size buckets (`Tiny` / `Small` / `Large`).
    BySizeBucket,
}

/// A proposed move: put file `name` into subfolder `target_subdir` (relative to its current folder).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveProposal {
    pub name: String,
    pub target_subdir: String,
}

/// Plan the organization: one [`MoveProposal`] per **file** in `entries`, in input order (stable and
/// deterministic). Directory entries are skipped — the engine never proposes moving a folder.
pub fn plan_organize(entries: &[OrganizeEntry], rule: OrganizeRule) -> Vec<MoveProposal> {
    entries
        .iter()
        .filter(|e| !e.is_dir) // leave directories in place
        .map(|e| MoveProposal { name: e.name.clone(), target_subdir: target_for(e, rule) })
        .collect()
}

/// The destination subfolder for a single file under `rule`.
fn target_for(entry: &OrganizeEntry, rule: OrganizeRule) -> String {
    match rule {
        OrganizeRule::ByKind => kind_category(&entry.ext).to_string(),
        OrganizeRule::ByExtension => {
            if entry.ext.is_empty() {
                "NoExtension".to_string()
            } else {
                entry.ext.to_uppercase()
            }
        }
        OrganizeRule::ByModifiedYear => year_from_epoch_secs(entry.modified_secs).to_string(),
        OrganizeRule::BySizeBucket => size_bucket(entry.size).to_string(),
    }
}

/// Map a (lowercased, dot-free) extension to a human content category. Unknown or empty → `Other`.
fn kind_category(ext: &str) -> &'static str {
    match ext {
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "tiff" | "svg" | "heic" => "Images",
        "pdf" | "doc" | "docx" | "txt" | "md" | "rtf" | "odt" | "xls" | "xlsx" | "csv" | "ppt"
        | "pptx" => "Documents",
        "mp3" | "flac" | "ogg" | "wav" | "m4a" | "aac" => "Audio",
        "mp4" | "mkv" | "mov" | "avi" | "webm" => "Video",
        "zip" | "tar" | "gz" | "7z" | "rar" => "Archives",
        "rs" | "ts" | "js" | "py" | "go" | "java" | "c" | "cpp" | "h" | "json" | "toml" | "yaml" => {
            "Code"
        }
        _ => "Other",
    }
}

const MIB: u64 = 1024 * 1024;

/// Coarse size bucket: `Tiny` (<1 MiB), `Small` (<100 MiB), `Large` (>=100 MiB).
fn size_bucket(size: u64) -> &'static str {
    if size < MIB {
        "Tiny"
    } else if size < 100 * MIB {
        "Small"
    } else {
        "Large"
    }
}

/// The 4-digit civil (UTC) year for a unix-epoch-**seconds** timestamp — computed with pure integer
/// arithmetic (no chrono, no new deps).
///
/// Uses Howard Hinnant's public-domain `civil_from_days` era algorithm: shift the epoch so the year
/// internally starts on March 1st (which moves the leap day to the end of the year, removing all
/// leap-year special-casing), split the day count into 400-year "eras", then recover the civil year.
/// Exact for every timestamp representable in `u64` seconds; pre-1970 is unrepresentable and out of scope.
fn year_from_epoch_secs(secs: u64) -> i64 {
    // Whole days since the unix epoch (1970-01-01).
    let z = (secs / 86_400) as i64 + 719_468; // shift epoch to 0000-03-01
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097; // 146_097 days per 400-year era
    let doe = z - era * 146_097; // day-of-era [0, 146096]
    // year-of-era [0, 399]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day-of-year, March-based
    let mp = (5 * doy + 2) / 153; // month, March-based [0, 11]
    // Convert March-based year back to the civil year: Jan/Feb (mp 10,11) belong to the next year.
    if mp >= 10 { y + 1 } else { y }
}

// ---- Junk / clutter detection (CPE-994, epic CPE-979) ----

/// Why a file is flagged as likely clutter — a *suggestion* for the declutter view, never an auto-action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClutterReason {
    /// A zero-byte file — usually a failed download or a leftover stub.
    ZeroByte,
    /// An installer package (`.exe`/`.msi`/`.dmg`/…) — commonly safe to remove once installed.
    Installer,
    /// A partial/temporary download (`.part`, `.crdownload`, `.tmp`) — an interrupted or transient file.
    TempOrPartial,
    /// A backup or editor lock/leftover (`.bak`, a trailing `~`, an office `~$` lock).
    Backup,
}

impl ClutterReason {
    /// A short human label for the declutter UI.
    pub fn label(self) -> &'static str {
        match self {
            ClutterReason::ZeroByte => "Empty file",
            ClutterReason::Installer => "Installer (safe to remove after install)",
            ClutterReason::TempOrPartial => "Partial / temporary download",
            ClutterReason::Backup => "Backup / leftover",
        }
    }
}

/// One flagged file: its `name` and the [`ClutterReason`] it matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClutterFinding {
    pub name: String,
    pub reason: ClutterReason,
}

/// Scan `entries` for likely-clutter **files**, returning one finding per flagged file in input order
/// (deterministic). Pure metadata heuristics — no content hashing (exact-duplicate detection is
/// [`crate::duplicates`]'s job) and no I/O. Directories are never flagged. A file matching several patterns
/// reports the first (most-definitive) reason.
pub fn find_clutter(entries: &[OrganizeEntry]) -> Vec<ClutterFinding> {
    entries
        .iter()
        .filter(|e| !e.is_dir)
        .filter_map(|e| clutter_reason(e).map(|reason| ClutterFinding { name: e.name.clone(), reason }))
        .collect()
}

/// The clutter reason for one file, or `None` if it doesn't look like clutter. Checked most-definitive
/// first: emptiness, then installer, then partial/temp, then backup/leftover.
fn clutter_reason(entry: &OrganizeEntry) -> Option<ClutterReason> {
    if entry.size == 0 {
        return Some(ClutterReason::ZeroByte);
    }
    if matches!(entry.ext.as_str(), "exe" | "msi" | "dmg" | "pkg" | "deb" | "rpm" | "appimage") {
        return Some(ClutterReason::Installer);
    }
    let name_lower = entry.name.to_lowercase();
    if matches!(entry.ext.as_str(), "part" | "crdownload" | "tmp" | "temp" | "download")
        || name_lower.ends_with(".part")
    {
        return Some(ClutterReason::TempOrPartial);
    }
    if entry.ext == "bak" || name_lower.ends_with('~') || name_lower.starts_with("~$") {
        return Some(ClutterReason::Backup);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a file fixture (never a directory) with the given name/ext; size + mtime default to 0.
    fn file(name: &str, ext: &str) -> OrganizeEntry {
        OrganizeEntry::new(name, false, ext, 0, 0)
    }

    fn targets(entries: &[OrganizeEntry], rule: OrganizeRule) -> Vec<(String, String)> {
        plan_organize(entries, rule)
            .into_iter()
            .map(|p| (p.name, p.target_subdir))
            .collect()
    }

    #[test]
    fn by_kind_groups_each_category() {
        let entries = [
            file("photo.png", "png"),
            file("clip.MOV_shown_lower.mov", "mov"),
            file("report.pdf", "pdf"),
            file("song.mp3", "mp3"),
            file("movie.mp4", "mp4"),
            file("backup.zip", "zip"),
            file("main.rs", "rs"),
        ];
        assert_eq!(
            targets(&entries, OrganizeRule::ByKind),
            vec![
                ("photo.png".into(), "Images".into()),
                ("clip.MOV_shown_lower.mov".into(), "Video".into()),
                ("report.pdf".into(), "Documents".into()),
                ("song.mp3".into(), "Audio".into()),
                ("movie.mp4".into(), "Video".into()),
                ("backup.zip".into(), "Archives".into()),
                ("main.rs".into(), "Code".into()),
            ]
        );
    }

    #[test]
    fn by_kind_extensionless_and_unknown_go_to_other() {
        let entries = [file("README", ""), file("data.xyz", "xyz")];
        assert_eq!(
            targets(&entries, OrganizeRule::ByKind),
            vec![("README".into(), "Other".into()), ("data.xyz".into(), "Other".into())]
        );
    }

    #[test]
    fn directories_are_never_proposed() {
        let entries = [
            OrganizeEntry::new("src", true, "", 0, 0),
            file("main.rs", "rs"),
            OrganizeEntry::new("assets", true, "", 0, 0),
        ];
        let plan = plan_organize(&entries, OrganizeRule::ByKind);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].name, "main.rs");
    }

    #[test]
    fn by_extension_uppercases_or_no_extension() {
        let entries = [file("a.png", "png"), file("b.TAR", "tar"), file("LICENSE", "")];
        assert_eq!(
            targets(&entries, OrganizeRule::ByExtension),
            vec![
                ("a.png".into(), "PNG".into()),
                ("b.TAR".into(), "TAR".into()),
                ("LICENSE".into(), "NoExtension".into()),
            ]
        );
    }

    #[test]
    fn by_size_bucket_boundaries() {
        let entries = [
            OrganizeEntry::new("tiny.bin", false, "bin", MIB - 1, 0),
            OrganizeEntry::new("exactly_1mib.bin", false, "bin", MIB, 0),
            OrganizeEntry::new("small.bin", false, "bin", 100 * MIB - 1, 0),
            OrganizeEntry::new("large.bin", false, "bin", 100 * MIB, 0),
        ];
        assert_eq!(
            targets(&entries, OrganizeRule::BySizeBucket),
            vec![
                ("tiny.bin".into(), "Tiny".into()),
                ("exactly_1mib.bin".into(), "Small".into()),
                ("small.bin".into(), "Small".into()),
                ("large.bin".into(), "Large".into()),
            ]
        );
    }

    #[test]
    fn by_modified_year_computes_civil_year() {
        // Known UTC epoch seconds → year.
        let cases = [
            (0u64, 1970),                 // 1970-01-01T00:00:00Z
            (946_684_799, 1999),          // 1999-12-31T23:59:59Z (just before 2000)
            (946_684_800, 2000),          // 2000-01-01T00:00:00Z (leap year)
            (951_782_400, 2000),          // 2000-02-29 (leap day) still 2000
            (1_704_067_200, 2024),        // 2024-01-01 (leap year)
            (1_753_315_200, 2025),        // 2025-07-24
        ];
        for (secs, want) in cases {
            let entry = OrganizeEntry::new("f", false, "", 0, secs);
            let plan = plan_organize(std::slice::from_ref(&entry), OrganizeRule::ByModifiedYear);
            assert_eq!(plan[0].target_subdir, want.to_string(), "secs={secs}");
        }
    }

    #[test]
    fn output_order_is_stable_input_order() {
        let entries = [file("z.rs", "rs"), file("a.rs", "rs"), file("m.rs", "rs")];
        let names: Vec<String> = plan_organize(&entries, OrganizeRule::ByKind)
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(names, vec!["z.rs", "a.rs", "m.rs"]);
    }

    // ---- clutter detection (CPE-994) ----

    /// A non-empty file fixture (size 10) so it isn't flagged as zero-byte.
    fn nonempty(name: &str, ext: &str) -> OrganizeEntry {
        OrganizeEntry::new(name, false, ext, 10, 0)
    }

    #[test]
    fn find_clutter_flags_each_category() {
        let entries = [
            OrganizeEntry::new("empty.log", false, "log", 0, 0), // zero byte
            nonempty("setup.exe", "exe"),                        // installer
            nonempty("movie.mp4.part", "part"),                  // partial download
            nonempty("notes.txt.bak", "bak"),                    // backup
            nonempty("doc.docx~", ""),                           // trailing ~ leftover (ext parsed off by caller)
            nonempty("~$report.docx", "docx"),                   // office lock
            nonempty("keep.rs", "rs"),                           // NOT clutter
        ];
        let findings = find_clutter(&entries);
        let found: Vec<(&str, ClutterReason)> =
            findings.iter().map(|f| (f.name.as_str(), f.reason)).collect();
        assert_eq!(
            found,
            vec![
                ("empty.log", ClutterReason::ZeroByte),
                ("setup.exe", ClutterReason::Installer),
                ("movie.mp4.part", ClutterReason::TempOrPartial),
                ("notes.txt.bak", ClutterReason::Backup),
                ("doc.docx~", ClutterReason::Backup),
                ("~$report.docx", ClutterReason::Backup),
            ]
        );
        // The real file is not flagged.
        assert!(!find_clutter(&entries).iter().any(|f| f.name == "keep.rs"));
    }

    #[test]
    fn find_clutter_skips_directories_and_prefers_zero_byte() {
        let entries = [
            OrganizeEntry::new("cache", true, "", 0, 0),   // dir — never flagged even though empty
            OrganizeEntry::new("stub.exe", false, "exe", 0, 0), // installer ext BUT zero-byte wins
        ];
        let found = find_clutter(&entries);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "stub.exe");
        assert_eq!(found[0].reason, ClutterReason::ZeroByte); // most-definitive reason first
    }

    #[test]
    fn clutter_reason_labels_are_human() {
        assert_eq!(ClutterReason::ZeroByte.label(), "Empty file");
        assert!(ClutterReason::Installer.label().contains("Installer"));
    }
}
