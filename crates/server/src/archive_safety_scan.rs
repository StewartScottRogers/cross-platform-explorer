//! Archive expansion-ratio / zip-bomb safety **scan** adapter (CPE-1281, epic CPE-1002). Wires the pure
//! ratio-scoring core in [`crate::archive_safety`] to a real archive on disk: opens it via the `zip`
//! crate (mirroring how [`crate::archive::zip_entries`] opens a zip for listing), collects each entry's
//! `(compressed_size, size)` into [`archive_safety::EntrySizes`], and scores the whole archive with
//! [`archive_safety::expansion_ratio`] against [`archive_safety::RatioLimits::default`].
//!
//! Only ZIP is wired here — the `zip` crate reports `compressed_size()`/`size()` per entry directly, so
//! it's the cheapest, most direct expansion-ratio source and covers the textbook zip-bomb case (a tiny
//! deflate stream that decompresses to gigabytes). Other archive formats' expansion-ratio adapters are a
//! later ticket if wanted.
//!
//! Never panics: an archive that fails to open (missing file, wrong format, truncated/corrupt central
//! directory) or an entry that fails to read yields a graceful empty/non-dangerous report rather than an
//! `Err` — a safety scan is a best-effort advisory pass, not something that should abort a caller's sweep
//! over many files just because one of them turns out to be garbage.
//!
//! CPE-1591: a per-entry read failure is **counted**, not silently discarded — an AES/ZipCrypto-encrypted
//! entry can't be read without its password, and `ZipArchive::by_index()` returns `Err` for it exactly
//! like it does for a genuinely malformed local-file header. Before this fix the loop below `continue`d
//! past that `Err` with nothing to show for it, so a password-protected zip (every entry unreadable)
//! collapsed to the same `entries_scanned: 0, unreadable: false` shape as a valid, empty archive — which
//! [`ArchiveSafetyReport`]'s consumer renders as "No zip-bomb risk detected", having examined nothing.

use std::fs;
use std::path::Path;

use serde::Serialize;

use crate::archive_safety::{self, EntrySizes, RatioLimits, RatioReport};

/// Cap on entries scored per archive — bounds a maliciously (or just enormously) huge central directory
/// so the scan stays fast; mirrors the entry/file caps in [`crate::folder_similarity_scan`]. Hitting it
/// sets `truncated`; the score is still computed from whatever entries were collected before the cap.
const MAX_ENTRIES: usize = 200_000;

/// The result of scanning a real archive for zip-bomb risk: the pure ratio scoring plus scan bookkeeping
/// (how many entries were actually considered, and whether [`MAX_ENTRIES`] truncated the scan).
///
/// `unreadable` (CPE-1320) is the signal that distinguishes a **corrupt/unopenable** archive from a
/// **valid, empty** one — both used to collapse to the same `entries_scanned: 0, report.dangerous: false`
/// shape, which the Archive-Safety dialog rendered as a misleading "No zip-bomb risk" for a file that was
/// never actually scanned. `unreadable == true` means `path` couldn't be opened at all (missing file, not
/// a zip, corrupt/truncated central directory) — the rest of the report is a placeholder, not a real
/// scan, and callers must not read it as "safe". `unreadable == false` means the archive opened fine (an
/// empty archive still reports `entries_scanned: 0`, but with `unreadable: false`).
///
/// `unreadable_entries` (CPE-1591) is the sibling signal for the case `unreadable` doesn't cover: the
/// archive itself opened fine (its central directory is readable), but one or more *individual entries*
/// couldn't be read — overwhelmingly because they're AES/ZipCrypto-encrypted and no password was
/// supplied, though the same field would also catch a one-off malformed local-file header. A skipped
/// entry contributes nothing to `report` (it was never sized or scored), so **`report.dangerous == false`
/// does not mean "safe" when `unreadable_entries > 0`** — it can just as easily mean "we couldn't check".
/// A fully password-protected zip scans zero entries and reports `unreadable_entries` equal to the
/// archive's whole entry count; a caller must treat any `unreadable_entries > 0` as "not fully assessed"
/// and never render the plain safe banner for it, mirroring how `unreadable` already gates the corrupt
/// case. `unreadable_entries == 0` (with `unreadable == false`) means every entry that exists was
/// actually scored — the only shape a "no zip-bomb risk" verdict is honest for.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ArchiveSafetyReport {
    pub report: RatioReport,
    pub entries_scanned: u64,
    pub truncated: bool,
    pub unreadable: bool,
    pub unreadable_entries: u64,
}

/// Score `path` (a ZIP archive) for zip-bomb-like expansion ratio, using
/// [`archive_safety::RatioLimits::default`]. Never fails or panics: an archive that can't be opened, or
/// whose central directory is corrupt/garbage, yields an [`ArchiveSafetyReport`] with zero entries scanned,
/// `report.dangerous == false`, and `unreadable == true` (CPE-1320) rather than an `Err` — the caller must
/// check `unreadable` before treating the report as "no risk found".
pub fn analyze_archive_safety(path: &Path) -> ArchiveSafetyReport {
    analyze_archive_safety_with_limits(path, &RatioLimits::default())
}

/// [`analyze_archive_safety`] with explicit limits — the real entry point uses the default; tests exercise
/// other thresholds directly.
pub fn analyze_archive_safety_with_limits(path: &Path, limits: &RatioLimits) -> ArchiveSafetyReport {
    let Ok(file) = fs::File::open(path) else {
        return empty_report(limits, true);
    };
    let Ok(mut zip) = zip::ZipArchive::new(file) else {
        return empty_report(limits, true);
    };

    let mut entries: Vec<EntrySizes> = Vec::new();
    let mut truncated = false;
    let mut unreadable_entries: u64 = 0;
    for i in 0..zip.len() {
        if entries.len() >= MAX_ENTRIES {
            truncated = true;
            break;
        }
        // Skip (not abort) an entry the `zip` crate can't read — a single malformed local-file header,
        // or (CPE-1591) an AES/ZipCrypto-encrypted entry with no password supplied, shouldn't take down
        // the whole scan, mirroring `list_dir`'s skip-on-error discipline. Unlike the pre-fix code, the
        // skip is *counted* rather than silently dropped, so the caller can tell "scanned and clean" from
        // "couldn't check this entry" instead of both collapsing into the same all-zeros report.
        let Ok(entry) = zip.by_index(i) else {
            unreadable_entries += 1;
            continue;
        };
        entries.push(EntrySizes {
            name: entry.name().to_string(),
            compressed: entry.compressed_size(),
            uncompressed: entry.size(),
        });
    }

    let entries_scanned = entries.len() as u64;
    let report = archive_safety::expansion_ratio(&entries, limits);
    ArchiveSafetyReport { report, entries_scanned, truncated, unreadable: false, unreadable_entries }
}

/// The graceful empty result for an archive that couldn't be opened at all. `unreadable` distinguishes
/// "we tried to open this and failed" (`true`, CPE-1320: file missing, not a zip, or a corrupt central
/// directory) from "we opened it fine and it happens to have nothing scoreable" (`false` — not currently
/// reached by any call site, since a successfully-opened-but-empty zip goes through the normal scan path
/// above and constructs its own `ArchiveSafetyReport` directly; kept as a parameter rather than a hardcoded
/// `true` so this helper stays honest if a future caller needs a genuine empty-but-opened placeholder).
fn empty_report(limits: &RatioLimits, unreadable: bool) -> ArchiveSafetyReport {
    ArchiveSafetyReport {
        report: archive_safety::expansion_ratio(&[], limits),
        entries_scanned: 0,
        truncated: false,
        unreadable,
        unreadable_entries: 0,
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
        let d = std::env::temp_dir().join(format!("cpe-archivesafety-{}-{}-{}", tag, std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let file = fs::File::create(path).unwrap();
        let mut w = zip::ZipWriter::new(file);
        let opts: zip::write::FileOptions<()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (name, bytes) in entries {
            w.start_file(*name, opts).unwrap();
            w.write_all(bytes).unwrap();
        }
        w.finish().unwrap();
    }

    /// Writes a single-entry zip with `CompressionMethod::Stored` (no compression), so
    /// `compressed_size == uncompressed_size` exactly — a deterministic ratio of `1.0` independent of any
    /// deflate implementation detail, unlike [`write_zip`]'s deflated entries.
    fn write_zip_stored(path: &Path, name: &str, bytes: &[u8]) {
        let file = fs::File::create(path).unwrap();
        let mut w = zip::ZipWriter::new(file);
        let opts: zip::write::FileOptions<()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        w.start_file(name, opts).unwrap();
        w.write_all(bytes).unwrap();
        w.finish().unwrap();
    }

    #[test]
    fn a_normal_archive_is_not_flagged() {
        let d = scratch("normal");
        let zip_path = d.join("normal.zip");
        // Short, not-very-compressible text — nowhere near the 100x default ratio.
        write_zip(&zip_path, &[("readme.txt", b"hello world, this is a normal small file")]);

        let result = analyze_archive_safety(&zip_path);
        assert_eq!(result.entries_scanned, 1);
        assert!(!result.truncated);
        assert!(result.report.flagged.is_empty(), "flagged: {:?}", result.report.flagged);
        assert!(!result.report.dangerous);
        assert!(!result.unreadable, "a successfully opened archive is not unreadable");
        assert_eq!(result.unreadable_entries, 0, "every entry in a normal archive was readable");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn a_highly_compressible_entry_is_flagged_as_a_zip_bomb() {
        let d = scratch("bomb");
        let zip_path = d.join("bomb.zip");
        // 2,000,000 identical bytes deflates to a tiny fraction of its size — the classic zip-bomb
        // signature, easily clearing the default 100x per-entry ratio threshold.
        let bomb = vec![0u8; 2_000_000];
        write_zip(&zip_path, &[("normal.txt", b"a small ordinary file"), ("bomb.bin", &bomb)]);

        let result = analyze_archive_safety(&zip_path);
        assert_eq!(result.entries_scanned, 2);
        assert!(!result.truncated);
        assert_eq!(result.report.flagged.len(), 1, "flagged: {:?}", result.report.flagged);
        assert_eq!(result.report.flagged[0].name, "bomb.bin");
        assert!(result.report.flagged[0].ratio > 100.0);
        assert!(result.report.dangerous);
        assert!(!result.unreadable, "a successfully opened archive is not unreadable, even a dangerous one");
        assert_eq!(result.unreadable_entries, 0, "every entry in this archive was readable");
        let _ = fs::remove_dir_all(&d);
    }

    /// CPE-1591: a password-protected (AES-256) zip opens fine (its central directory needs no password),
    /// but every entry inside it needs the password to read — before this fix, `by_index()`'s per-entry
    /// `Err` was silently `continue`d past, so the scan finished having examined nothing and reported the
    /// same `entries_scanned: 0, unreadable: false, report.dangerous: false` shape as a genuinely safe,
    /// empty archive. This proves an encrypted zip that's actually full of a would-be-dangerous entry does
    /// NOT silently report `dangerous: false` — it now surfaces via `unreadable_entries > 0` that nothing
    /// was actually scored, so a caller can't mistake this for "scanned and safe".
    #[test]
    fn a_password_protected_zip_reports_unreadable_entries_not_silently_safe() {
        let d = scratch("encrypted");
        let zip_path = d.join("secret.zip");
        // A large, highly-compressible payload — if this could be read without the password, it would
        // trip the zip-bomb flag. Encrypted, it must instead show up as an unreadable entry.
        let bomb = vec![0u8; 2_000_000];
        let payload_path = d.join("bomb.bin");
        fs::write(&payload_path, &bomb).unwrap();
        crate::archive::compress_to_zip_encrypted(
            &[payload_path.to_string_lossy().to_string()],
            zip_path.to_str().unwrap(),
            "correct horse battery staple",
        )
        .unwrap();

        let result = analyze_archive_safety(&zip_path);
        assert_eq!(result.entries_scanned, 0, "no entry could be read without the password");
        assert!(!result.truncated);
        assert!(result.report.flagged.is_empty(), "nothing was scored, so nothing can be flagged");
        assert!(
            !result.report.dangerous,
            "the pure ratio score over zero entries is (correctly) not dangerous on its own"
        );
        assert!(!result.unreadable, "the archive itself opened fine — only its entries are unreadable");
        assert_eq!(
            result.unreadable_entries, 1,
            "the single encrypted entry must be counted as unreadable, not silently skipped"
        );
        let _ = fs::remove_dir_all(&d);
    }

    /// CPE-1320: a corrupt/truncated ZIP (garbage bytes with a `.zip`-shaped name) must be distinguishable
    /// from a valid empty archive — before this fix both collapsed to `entries_scanned: 0,
    /// report.dangerous: false`, which the Archive-Safety dialog rendered as a misleading "No zip-bomb
    /// risk · 0 entries" for a file that was never actually scanned.
    #[test]
    fn a_corrupt_zip_is_reported_unreadable_not_silently_safe() {
        let d = scratch("garbage");
        let garbage_path = d.join("not-a-zip.zip");
        fs::write(&garbage_path, b"this is definitely not a valid zip archive, just plain bytes").unwrap();

        let result = analyze_archive_safety(&garbage_path);
        assert_eq!(result.entries_scanned, 0);
        assert!(!result.truncated);
        assert!(result.report.flagged.is_empty());
        assert!(!result.report.dangerous);
        assert!(result.unreadable, "a corrupt/non-zip file must be flagged unreadable, not reported as safe");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn a_missing_file_is_handled_gracefully_never_panics() {
        let d = scratch("missing");
        let missing_path = d.join("does-not-exist.zip");

        let result = analyze_archive_safety(&missing_path);
        assert_eq!(result.entries_scanned, 0);
        assert!(!result.truncated);
        assert!(!result.report.dangerous);
        assert!(result.unreadable, "a missing file couldn't be opened, so it's unreadable too");
        assert_eq!(result.unreadable_entries, 0, "we never got as far as reading any entries");
        let _ = fs::remove_dir_all(&d);
    }

    /// CPE-1320 round-trip: a **valid** empty zip (opened fine, zero entries) must NOT be flagged
    /// unreadable — only an open-failure sets that flag. This is the contrast case for
    /// `a_corrupt_zip_is_reported_unreadable_not_silently_safe`: same `entries_scanned == 0`, same
    /// `report.dangerous == false`, but `unreadable` differs because one archive actually opened.
    ///
    /// It's also the contrast case for CPE-1591's
    /// `a_password_protected_zip_reports_unreadable_entries_not_silently_safe`: this archive has the same
    /// `entries_scanned == 0` too, but `unreadable_entries == 0` here (there was nothing to fail to read),
    /// versus `unreadable_entries == 1` there (an entry existed but couldn't be read) — the field that
    /// tells "genuinely nothing to scan" apart from "something existed but we couldn't check it".
    #[test]
    fn a_valid_empty_zip_is_not_flagged_unreadable() {
        let d = scratch("valid-empty");
        let zip_path = d.join("empty.zip");
        write_zip(&zip_path, &[]);

        let result = analyze_archive_safety(&zip_path);
        assert_eq!(result.entries_scanned, 0);
        assert!(!result.truncated);
        assert!(!result.report.dangerous);
        assert!(!result.unreadable, "a validly-opened empty archive is not unreadable");
        assert_eq!(result.unreadable_entries, 0, "a genuinely empty archive has no entries to fail on");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn explicit_limits_are_honoured() {
        let d = scratch("limits");
        let zip_path = d.join("stored.zip");
        // `Stored` (uncompressed) entry: compressed_size == uncompressed_size, so the ratio is exactly
        // 1.0 — deterministic, independent of any deflate implementation detail (unlike compression-ratio
        // based fixtures, which can vary slightly across zlib/miniz_oxide versions or platforms).
        write_zip_stored(&zip_path, "plain.bin", b"some ordinary uncompressed bytes");

        let default_result = analyze_archive_safety(&zip_path);
        assert!(!default_result.report.dangerous, "ratio 1.0 is well under the generous default limits");

        let strict = RatioLimits::new(0.5, 0.5);
        let strict_result = analyze_archive_safety_with_limits(&zip_path, &strict);
        assert!(strict_result.report.dangerous, "a 0.5x limit should flag a ratio of exactly 1.0");
        let _ = fs::remove_dir_all(&d);
    }
}
