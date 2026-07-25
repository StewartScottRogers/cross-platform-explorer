//! Pure orphaned-sidecar-file detector (CPE-1006, epic CPE-1002 "File inspection & safety utilities").
//! A "sidecar" is a companion file (subtitles `.srt`, metadata `.xmp`/`.nfo`, camera thumbnail `.thm`, …)
//! named after a primary file; it's "orphaned" when that primary file is missing. Given a caller-supplied
//! flat listing of **one directory's** entries (already gathered — no filesystem access happens here) and
//! a set of [`SidecarRule`]s, report the names of orphaned sidecar files. Pure std, zero deps, deterministic
//! (input-order) output. Companion to [`crate::organize`] / [`crate::duplicates`] as another pure planning
//! slice.

/// One file entry to consider, gathered by the caller (no filesystem access happens here). All entries
/// passed to [`find_orphans`] together are assumed to live in the same directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    /// File name as it appears in its parent directory (e.g. `"movie.srt"`).
    pub name: String,
    /// The file name without its final extension (e.g. `"movie"` for `movie.srt`).
    pub stem: String,
    /// Extension, **lowercased and without the leading dot** (`""` when the file has none).
    pub ext: String,
}

impl FileEntry {
    /// Convenience constructor mirroring the struct field order.
    pub fn new(name: impl Into<String>, stem: impl Into<String>, ext: impl Into<String>) -> Self {
        Self { name: name.into(), stem: stem.into(), ext: ext.into() }
    }
}

/// A rule pairing one sidecar extension with the set of primary-file extensions it companions. Extensions
/// are lowercased and without the leading dot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidecarRule {
    /// The sidecar's own extension (e.g. `"srt"`).
    pub sidecar_ext: String,
    /// Extensions of the primary files this sidecar can companion (e.g. video extensions for `srt`).
    pub primary_exts: Vec<String>,
}

impl SidecarRule {
    fn new(sidecar_ext: &str, primary_exts: &[&str]) -> Self {
        Self {
            sidecar_ext: sidecar_ext.to_string(),
            primary_exts: primary_exts.iter().map(|s| s.to_string()).collect(),
        }
    }
}

/// Video container extensions treated as "primary media" for subtitle/NFO sidecars.
const VIDEO_EXTS: &[&str] = &["mp4", "mkv", "avi", "mov", "webm"];

/// Image / camera-raw extensions treated as "primary media" for XMP sidecars.
const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "tiff", "cr2", "nef", "arw", "dng"];

/// Sensible built-in sidecar rules:
/// - `srt` / `sub` / `ass` (subtitle tracks) → video files. These are the three subtitle formats a file
///   explorer commonly finds sitting next to a movie/episode.
/// - `xmp` (Adobe/XMP sidecar metadata, used by Lightroom and camera-raw workflows) → image + camera-raw
///   files.
/// - `nfo` (media-info / scraper metadata, e.g. Kodi/Plex library agents) → video files.
/// - `thm` (camera-generated thumbnail sidecar) → video **and** image files, since cameras emit `.thm` for
///   both photo and video captures.
pub fn default_rules() -> Vec<SidecarRule> {
    let video_and_image: Vec<&str> = VIDEO_EXTS.iter().chain(IMAGE_EXTS).copied().collect();
    vec![
        SidecarRule::new("srt", VIDEO_EXTS),
        SidecarRule::new("sub", VIDEO_EXTS),
        SidecarRule::new("ass", VIDEO_EXTS),
        SidecarRule::new("xmp", IMAGE_EXTS),
        SidecarRule::new("nfo", VIDEO_EXTS),
        SidecarRule::new("thm", &video_and_image),
    ]
}

/// Find orphaned sidecar files in `entries` under `rules`. For each entry whose `ext` matches a rule's
/// `sidecar_ext` (case-insensitive), its `name` is reported when **no other** entry in `entries` shares its
/// `stem` (case-insensitive) and has an `ext` in that rule's `primary_exts` (also case-insensitive).
/// Non-sidecar files, and sidecars whose primary is present, are never reported. Output is in input order
/// (deterministic).
pub fn find_orphans(entries: &[FileEntry], rules: &[SidecarRule]) -> Vec<String> {
    entries
        .iter()
        .filter_map(|entry| {
            let rule = rules.iter().find(|r| r.sidecar_ext.eq_ignore_ascii_case(&entry.ext))?;
            let has_primary = entries.iter().any(|other| {
                !std::ptr::eq(other, entry)
                    && other.stem.eq_ignore_ascii_case(&entry.stem)
                    && rule.primary_exts.iter().any(|pe| pe.eq_ignore_ascii_case(&other.ext))
            });
            if has_primary { None } else { Some(entry.name.clone()) }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_present_keeps_sidecar() {
        let entries = [
            FileEntry::new("movie.mp4", "movie", "mp4"),
            FileEntry::new("movie.srt", "movie", "srt"),
        ];
        let orphans = find_orphans(&entries, &default_rules());
        assert!(orphans.is_empty(), "sidecar with a present primary must not be reported: {orphans:?}");
    }

    #[test]
    fn sidecar_alone_is_orphaned() {
        let entries = [FileEntry::new("deleted.srt", "deleted", "srt")];
        let orphans = find_orphans(&entries, &default_rules());
        assert_eq!(orphans, vec!["deleted.srt".to_string()]);
    }

    #[test]
    fn xmp_primary_present_keeps_sidecar_and_orphan_alone_is_reported() {
        let entries = [
            FileEntry::new("photo.jpg", "photo", "jpg"),
            FileEntry::new("photo.xmp", "photo", "xmp"),
            FileEntry::new("orphan.xmp", "orphan", "xmp"),
        ];
        let orphans = find_orphans(&entries, &default_rules());
        assert_eq!(orphans, vec!["orphan.xmp".to_string()]);
    }

    #[test]
    fn stem_matching_is_case_insensitive() {
        let entries = [
            FileEntry::new("Movie.MP4", "Movie", "mp4"),
            FileEntry::new("movie.srt", "movie", "srt"),
        ];
        let orphans = find_orphans(&entries, &default_rules());
        assert!(orphans.is_empty(), "case-insensitive stem match should keep the sidecar: {orphans:?}");
    }

    #[test]
    fn wrong_primary_type_is_still_orphaned() {
        // `notes.txt` shares the stem with `notes.srt`, but `txt` is not one of `srt`'s primary
        // extensions (it isn't a video container), so the subtitle is still orphaned.
        let entries = [
            FileEntry::new("notes.srt", "notes", "srt"),
            FileEntry::new("notes.txt", "notes", "txt"),
        ];
        let orphans = find_orphans(&entries, &default_rules());
        assert_eq!(orphans, vec!["notes.srt".to_string()]);
    }

    #[test]
    fn non_sidecar_files_are_never_reported() {
        let entries = [FileEntry::new("readme.txt", "readme", "txt")];
        let orphans = find_orphans(&entries, &default_rules());
        assert!(orphans.is_empty());
    }

    #[test]
    fn output_order_is_stable_input_order() {
        let entries = [
            FileEntry::new("z.srt", "z", "srt"),
            FileEntry::new("a.xmp", "a", "xmp"),
            FileEntry::new("m.nfo", "m", "nfo"),
        ];
        let orphans = find_orphans(&entries, &default_rules());
        assert_eq!(orphans, vec!["z.srt".to_string(), "a.xmp".to_string(), "m.nfo".to_string()]);
    }

    #[test]
    fn thm_orphans_against_both_video_and_image_primaries() {
        // A `.thm` sidecar is kept when a video primary is present...
        let with_video = [
            FileEntry::new("clip.mov", "clip", "mov"),
            FileEntry::new("clip.thm", "clip", "thm"),
        ];
        assert!(find_orphans(&with_video, &default_rules()).is_empty());

        // ...and also kept when an image primary is present...
        let with_image = [
            FileEntry::new("photo.png", "photo", "png"),
            FileEntry::new("photo.thm", "photo", "thm"),
        ];
        assert!(find_orphans(&with_image, &default_rules()).is_empty());

        // ...but orphaned when neither is present.
        let alone = [FileEntry::new("lonely.thm", "lonely", "thm")];
        assert_eq!(find_orphans(&alone, &default_rules()), vec!["lonely.thm".to_string()]);
    }

    #[test]
    fn nfo_orphans_without_a_matching_video() {
        let entries = [FileEntry::new("show.nfo", "show", "nfo")];
        assert_eq!(find_orphans(&entries, &default_rules()), vec!["show.nfo".to_string()]);
    }

    #[test]
    fn default_rules_cover_documented_extensions() {
        let rules = default_rules();
        let sidecar_exts: Vec<&str> = rules.iter().map(|r| r.sidecar_ext.as_str()).collect();
        assert_eq!(sidecar_exts, vec!["srt", "sub", "ass", "xmp", "nfo", "thm"]);
    }
}
