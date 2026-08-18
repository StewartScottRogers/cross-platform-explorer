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
//!
//! ## CPE-1602: metadata is no longer trusted blindly
//!
//! Every size above (`compressed_size()`/`size()`) comes straight off the ZIP's **central directory** —
//! a trailer the archive writes about itself. An independent reviewer demonstrated the obvious problem:
//! build a real bomb (2,000,000 zero bytes, honestly deflated, ~1023x), then hand-patch the
//! `uncompressed_size` field down to 100 bytes in **both** the local file header and the central
//! directory. Nothing about decoding the archive requires those two numbers to be true — `zip` only uses
//! `compressed_size` to know how many input bytes to feed the decompressor, and DEFLATE finds its own
//! end. The scan used to report a confident, fully-scanned "safe" over a genuine bomb.
//!
//! Three designs were on the table (see the ticket): (1) cross-check the local-file-header size fields
//! against the central directory's and distrust a mismatch — cheap, but the reviewer patched *both*
//! copies, so they agree with each other while still lying; (2) decompress every entry through a capped
//! counter — sound, but taxes the common case (a big, honest archive now costs real I/O to open); (3) a
//! **hybrid** — trust metadata for the ordinary case, verify by decompression only where it looks
//! implausible. This module implements (3): [`is_suspicious`] flags an entry when
//!
//! - its local-file-header sizes disagree with the central directory's (option 1's cross-check — catches
//!   a naive single-field patch on its own, cheaply, no I/O beyond a 30-byte peek), **or**
//! - the entry uses a streamed data-descriptor or a ZIP64 size sentinel, so the local header has nothing
//!   comparable to offer (ambiguous is treated as suspicious, not as a pass), **or**
//! - its declared sizes are *structurally impossible* for its compression method — DEFLATE cannot
//!   legitimately produce a compressed stream much larger than the uncompressed data it encodes (a few
//!   bytes of store overhead per block, never a large multiple), so `compressed_size` dwarfing
//!   `uncompressed_size` is a lie independent of any ratio math. This is exactly the reviewer's patch:
//!   ~1,955 real compressed bytes against a claimed 100 uncompressed catches it even though the forger
//!   patched *both* copies of the metadata into agreement, because the deception isn't in the
//!   LFH-vs-CD comparison at all — it's in the number itself being physically impossible, **or**
//! - the declared ratio is already elevated (past [`archive_safety::RatioLimits::suspicion_ratio`], well
//!   below the danger threshold) — a forger who tunes the number down to *just under* dangerous, rather
//!   than leaving it obviously safe, still doesn't get a free pass.
//!
//! A suspicious entry is verified by [`verify_by_decompression`]: stream its real decompressed bytes
//! through a bounded counter capped at `compressed_size.max(floor) * max_entry_ratio`, additionally
//! clamped by a hard per-entry ceiling and a whole-archive byte/time budget so a crafted archive cannot
//! turn the *scan itself* into the bomb (see the constants below). Reaching the ratio-derived cap without
//! finishing proves the entry is dangerous without ever reading further; finishing before the cap yields
//! the entry's *true* size, superseding whatever the archive claimed. If the scan runs out of its own
//! budget before an entry can be verified, that entry is counted in `unreadable_entries` — the CPE-1591
//! tri-state — rather than trusted, so a scan that couldn't finish verifying never renders as "safe".
//!
//! An ordinary archive (the overwhelming common case) never triggers any of the above — its local header
//! agrees with its central directory, its ratios are unremarkable — so it never pays for decompression;
//! only the metadata pass runs, exactly as before this ticket.
//!
//! ## Round 2: `compressed_size` can be forged the *other* way too
//!
//! The structural check above only catches `compressed_size` dwarfing `uncompressed_size`. A forger can
//! instead leave `uncompressed_size` truthful and patch `compressed_size` **up** — the reviewer proved it
//! by taking the same 2,000,000-zero-byte bomb (real compressed data ≈1,954 bytes) and inflating the
//! declared `compressed_size` to 1,999,999 in both the local header and the central directory. That costs
//! the forger nothing (no padding needed) and produced `dangerous: false` even *after* decompression
//! verification, because the verified (real, correctly-measured) uncompressed byte count was still being
//! divided by the **declared** compressed size — honest numerator, forgeable denominator.
//!
//! The fix is decompression-free and exact: [`analyze_archive_safety_with_limits`] first gathers every
//! entry's on-disk layout (`header_start`/`data_start`, via [`zip::ZipArchive::by_index_raw`], which needs
//! no password and does no decompression) and derives a **hard physical ceiling** per entry — the byte gap
//! from where its compressed data starts to wherever the *next* entry's local header begins in actual file
//! order (or the central directory's start, for whichever entry is physically last). A forger cannot lie
//! past that ceiling without literally writing that many extra real bytes to disk, which defeats the whole
//! point of a cheap bomb. `compressed_for_scoring = cd_compressed.min(physical_max)` replaces the raw
//! declared `compressed_size` everywhere it would otherwise be trusted: in [`is_suspicious`]'s checks, in
//! [`verify_by_decompression`]'s ratio-derived cap, and in every [`EntrySizes`] pushed into the final
//! score. An entry whose declared `compressed_size` exceeds its physical ceiling is suspicious on that
//! fact alone, independent of any ratio math.

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::archive_safety::{self, EntrySizes, RatioLimits, RatioReport};

/// Cap on entries scored per archive — bounds a maliciously (or just enormously) huge central directory
/// so the scan stays fast; mirrors the entry/file caps in [`crate::folder_similarity_scan`]. Hitting it
/// sets `truncated`; the score is still computed from whatever entries were collected before the cap.
/// CPE-1602: this now bounds *attempts* (the loop index), not just successes — otherwise an archive
/// whose central directory is packed with entries that all fail to open/verify would never trip it.
const MAX_ENTRIES: usize = 200_000;

/// CPE-1602 verification pass — the floor, ceilings, and deadline that bound
/// [`verify_by_decompression`]. The scan must never take a crafted archive's word for a suspicious
/// entry's size, but verifying it must also never let that archive dictate how much work the *scanner*
/// does — every one of these is a hard ceiling, not a suggestion.
///
/// `ENTRY_VERIFY_FLOOR` keeps the ratio-derived per-entry cap meaningful for a tiny `compressed_size`
/// (a 10-byte compressed entry claiming a 1KB expansion shouldn't be verified against a cap of 1,000
/// bytes — that's barely enough to prove anything). `ENTRY_VERIFY_ABS_CAP` is the hard per-entry ceiling
/// regardless of declared sizes, so a single entry can never cost more than this to verify even if its
/// own `compressed_size` is itself huge. `TOTAL_VERIFY_BUDGET` bounds the *sum* across every suspicious
/// entry in one scan, so many small suspicious entries can't add up to unbounded work. `VERIFY_DEADLINE`
/// is a wall-clock backstop checked during verification, independent of the byte caps, in case I/O is
/// unexpectedly slow (e.g. a network share).
const ENTRY_VERIFY_FLOOR: u64 = 4096;
const ENTRY_VERIFY_ABS_CAP: u64 = 256 * 1024 * 1024;
const TOTAL_VERIFY_BUDGET: u64 = 512 * 1024 * 1024;
const VERIFY_DEADLINE: Duration = Duration::from_secs(5);
const VERIFY_CHUNK_SIZE: usize = 64 * 1024;

/// Fixed byte layout of a ZIP local file header (PKZIP APPNOTE §4.3.7) — read directly off disk,
/// bypassing the `zip` crate's public API (which only ever surfaces sizes derived from the *central
/// directory*, never the local header's own copy of them — see [`peek_local_header`]).
const LOCAL_HEADER_LEN: usize = 30;
const LOCAL_HEADER_SIGNATURE: [u8; 4] = [0x50, 0x4b, 0x03, 0x04];
/// General-purpose bit flag 3 ("data descriptor follows"): when set, the local header's size/CRC fields
/// are legitimately zero placeholders — the real values live in a trailing descriptor written *after*
/// the entry's compressed data. Not comparable against the central directory at all.
const GPBF_DATA_DESCRIPTOR: u16 = 0x0008;
/// The 32-bit ZIP64 sentinel: a size field pinned to this exact value means "see the ZIP64 extra field
/// instead". This module doesn't parse ZIP64 extra fields, so it treats the sentinel like the streamed
/// case above — ambiguous, not comparable, always suspicious.
const ZIP64_SENTINEL: u32 = 0xFFFF_FFFF;

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
    // CPE-1602: a second, independent handle used only to peek raw local-file-header bytes. The `zip`
    // crate's public API only ever surfaces central-directory-derived sizes; peeking the local header
    // ourselves needs its own seek position so it never fights `zip`'s internal reader for a `&mut`
    // borrow while an entry is open. Opening it is best-effort — if it fails (rare: an I/O race), every
    // entry just falls back to the "couldn't peek the header" branch of `is_suspicious`, which treats
    // that conservatively (verify rather than trust).
    let mut header_peek = fs::File::open(path).ok();

    // CPE-1602 (round 2): gather every entry's on-disk layout before scoring anything, so a hard
    // physical ceiling on `compressed_size` can be computed from real byte gaps in the file — see the
    // module doc comment. `by_index_raw` needs no password and does no decompression, so this pass is
    // cheap and works even for encrypted entries. Best-effort: an entry whose layout can't be read here
    // simply gets no physical bound (falls back to trusting its declared size); the main pass below
    // independently re-opens the same entry via `by_index` and will land it in `unreadable_entries` if
    // that also fails, so this is not a gap.
    struct EntryLayout {
        index: usize,
        header_start: u64,
        data_start: u64,
    }
    let mut layouts: Vec<EntryLayout> = Vec::new();
    for i in 0..zip.len().min(MAX_ENTRIES) {
        if let Ok(raw) = zip.by_index_raw(i) {
            layouts.push(EntryLayout { index: i, header_start: raw.header_start(), data_start: raw.data_start() });
        }
    }
    // Sort into real on-disk order — the central directory can list entries in any order, but the
    // physical ceiling for one entry depends on whichever entry actually comes next *in the file*.
    layouts.sort_by_key(|e| e.data_start);
    let central_directory_start = zip.central_directory_start();
    let mut physical_max: HashMap<usize, u64> = HashMap::with_capacity(layouts.len());
    for (pos, layout) in layouts.iter().enumerate() {
        let boundary = layouts.get(pos + 1).map_or(central_directory_start, |next| next.header_start);
        physical_max.insert(layout.index, boundary.saturating_sub(layout.data_start));
    }

    let mut entries: Vec<EntrySizes> = Vec::new();
    let mut truncated = false;
    let mut unreadable_entries: u64 = 0;
    // Bounds on the verification pass — shared across every suspicious entry in this scan so many small
    // suspicious entries can't add up to unbounded decompression work (see the constants' doc comment).
    let mut verify_budget_remaining = TOTAL_VERIFY_BUDGET;
    let verify_deadline = Instant::now() + VERIFY_DEADLINE;

    for i in 0..zip.len() {
        if i >= MAX_ENTRIES {
            truncated = true;
            break;
        }
        // Skip (not abort) an entry the `zip` crate can't read — a single malformed local-file header,
        // or (CPE-1591) an AES/ZipCrypto-encrypted entry with no password supplied, shouldn't take down
        // the whole scan, mirroring `list_dir`'s skip-on-error discipline. Unlike the pre-fix code, the
        // skip is *counted* rather than silently dropped, so the caller can tell "scanned and clean" from
        // "couldn't check this entry" instead of both collapsing into the same all-zeros report.
        let Ok(mut entry) = zip.by_index(i) else {
            unreadable_entries += 1;
            continue;
        };
        if entry.is_dir() {
            continue; // no data to score, and nothing to verify
        }

        let cd_compressed = entry.compressed_size();
        let cd_uncompressed = entry.size();
        // CPE-1602 (round 2): clamp the declared compressed size to the physical ceiling computed above
        // — a forger inflating `compressed_size` to make the ratio look small no longer gets a free pass
        // just because they left `uncompressed_size` honest. This is what every downstream use of
        // "the compressed size" reads from now on, never the raw declared value.
        let compressed_for_scoring = physical_max.get(&i).map_or(cd_compressed, |&max| cd_compressed.min(max));

        // CPE-1602: don't take the central directory's word for it — cross-check it, and verify by
        // decompression when it looks implausible. See the module doc comment for the full rationale.
        let peek = header_peek.as_mut().and_then(|f| peek_local_header(f, entry.header_start()));
        let suspicious = is_suspicious(
            cd_compressed,
            compressed_for_scoring,
            cd_uncompressed,
            entry.compression(),
            peek.as_ref(),
            limits.suspicion_ratio,
        );

        if !suspicious {
            entries.push(EntrySizes { name: entry.name().to_string(), compressed: compressed_for_scoring, uncompressed: cd_uncompressed });
            continue;
        }

        if verify_budget_remaining == 0 || Instant::now() >= verify_deadline {
            // Out of our own budget — honestly "couldn't check this one", never silently "safe".
            unreadable_entries += 1;
            continue;
        }
        let name = entry.name().to_string();
        match verify_by_decompression(&mut entry, compressed_for_scoring, limits, verify_budget_remaining, verify_deadline) {
            VerifyOutcome::Counted { bytes_read, real } => {
                verify_budget_remaining = verify_budget_remaining.saturating_sub(bytes_read);
                // The real, measured size supersedes whatever the archive declared; the physically-capped
                // compressed size supersedes it too, on the other side of the ratio.
                entries.push(EntrySizes { name, compressed: compressed_for_scoring, uncompressed: real });
            }
            VerifyOutcome::DefinitelyDangerous { bytes_read, synthetic_uncompressed } => {
                verify_budget_remaining = verify_budget_remaining.saturating_sub(bytes_read);
                // Proven dangerous without reading further — feed a size that scores as such, saturating
                // toward "more dangerous" per the same discipline `expansion_ratio` already uses.
                entries.push(EntrySizes { name, compressed: compressed_for_scoring, uncompressed: synthetic_uncompressed });
            }
            VerifyOutcome::Inconclusive { bytes_read } => {
                verify_budget_remaining = verify_budget_remaining.saturating_sub(bytes_read);
                unreadable_entries += 1; // ran out of budget mid-entry — not "safe", "couldn't check"
            }
            VerifyOutcome::ReadError => {
                unreadable_entries += 1; // the entry failed to decompress at all
            }
        }
    }

    let entries_scanned = entries.len() as u64;
    let report = archive_safety::expansion_ratio(&entries, limits);
    ArchiveSafetyReport { report, entries_scanned, truncated, unreadable: false, unreadable_entries }
}

/// The local-file-header size fields [`peek_local_header`] reads directly off disk.
struct LocalHeaderPeek {
    flags: u16,
    compressed_size: u32,
    uncompressed_size: u32,
}

/// Read the raw local-file-header size fields at `header_start` (the offset [`zip::read::ZipFile::header_start`]
/// already reports). `None` on any I/O error or signature mismatch — shouldn't happen for an entry the
/// `zip` crate just validated, but a scan must never panic on a hostile file, and a failed peek is
/// itself treated as a reason for suspicion by [`is_suspicious`] rather than an error worth propagating.
fn peek_local_header(file: &mut fs::File, header_start: u64) -> Option<LocalHeaderPeek> {
    file.seek(SeekFrom::Start(header_start)).ok()?;
    let mut buf = [0u8; LOCAL_HEADER_LEN];
    file.read_exact(&mut buf).ok()?;
    if buf[0..4] != LOCAL_HEADER_SIGNATURE {
        return None;
    }
    Some(LocalHeaderPeek {
        flags: u16::from_le_bytes([buf[6], buf[7]]),
        compressed_size: u32::from_le_bytes([buf[18], buf[19], buf[20], buf[21]]),
        uncompressed_size: u32::from_le_bytes([buf[22], buf[23], buf[24], buf[25]]),
    })
}

/// Whether a central-directory-declared entry deserves verification instead of being trusted outright.
/// `cd_compressed` is the raw declared value (used only to test self-consistency against the local
/// header — a forger who patches both copies identically leaves nothing to disagree with there);
/// `compressed_for_scoring` is that same value clamped to the entry's physical on-disk ceiling (CPE-1602
/// round 2), used everywhere the *true* usable compressed size matters. See the module doc comment for
/// the full rationale behind each branch.
fn is_suspicious(
    cd_compressed: u64,
    compressed_for_scoring: u64,
    cd_uncompressed: u64,
    method: zip::CompressionMethod,
    peek: Option<&LocalHeaderPeek>,
    suspicion_ratio: f64,
) -> bool {
    match peek {
        None => return true, // couldn't read the local header at all — don't take the CD's word alone
        Some(p) => {
            let ambiguous = p.flags & GPBF_DATA_DESCRIPTOR != 0
                || p.compressed_size == ZIP64_SENTINEL
                || p.uncompressed_size == ZIP64_SENTINEL;
            if ambiguous {
                return true;
            }
            if u64::from(p.compressed_size) != cd_compressed || u64::from(p.uncompressed_size) != cd_uncompressed {
                return true; // local header and central directory disagree — a naive single-field forgery
            }
        }
    }
    // CPE-1602 round 2: a declared `compressed_size` that exceeds what's physically possible given real
    // on-disk entry boundaries is itself direct proof the metadata lies — independent of any ratio math,
    // and independent of whether the local header agrees with the central directory (a forger can patch
    // both consistently, as the reviewer demonstrated with the inflated-`compressed_size` variant).
    if cd_compressed > compressed_for_scoring {
        return true;
    }
    if method != zip::CompressionMethod::Stored {
        // DEFLATE (and friends) cannot legitimately shrink the declared uncompressed size to well below
        // the compressed size it supposedly encodes — the worst-case store overhead is a handful of
        // bytes per ~32KiB block, never a large multiple. A compressed size that dwarfs the declared
        // uncompressed size is a structural impossibility, independent of any ratio threshold — exactly
        // the reviewer's original patch (~1,955 real compressed bytes against a claimed 100 uncompressed),
        // which stays internally *consistent* between the local header and central directory and so
        // evades the cross-check above on its own. Uses the physically-clamped size, never the raw
        // declared one, so an inflated declaration can't dodge this check either.
        let slack = 64 + cd_uncompressed / 2_000;
        if compressed_for_scoring > cd_uncompressed.saturating_add(slack) {
            return true;
        }
    }
    archive_safety::ratio(cd_uncompressed, compressed_for_scoring) >= suspicion_ratio
}

/// The outcome of [`verify_by_decompression`] — never a bare number, so every caller has to decide what
/// each shape means rather than accidentally treating "couldn't finish" as "must be fine".
enum VerifyOutcome {
    /// Decompression reached EOF before any cap — `real` is the entry's true, measured uncompressed size.
    Counted { bytes_read: u64, real: u64 },
    /// The ratio-derived cap was reached without EOF, which alone proves the real ratio is at or beyond
    /// `limits.max_entry_ratio` — dangerous, without needing to read any further.
    DefinitelyDangerous { bytes_read: u64, synthetic_uncompressed: u64 },
    /// The scanner's own resource ceiling (the hard per-entry cap, the whole-scan budget, or the
    /// deadline) was reached first — genuinely unknown, must not be reported as either safe or dangerous.
    Inconclusive { bytes_read: u64 },
    /// The entry failed to decompress at all (corrupt stream, unsupported method actually used, etc.).
    ReadError,
}

/// Stream `entry`'s real decompressed bytes through a bounded counter, capped at
/// `compressed_for_scoring.max(ENTRY_VERIFY_FLOOR) * limits.max_entry_ratio` — the caller passes the
/// *physically-clamped* compressed size (CPE-1602 round 2), never the raw declared one, so an inflated
/// `compressed_size` can't blow this cap open — and additionally clamped by the hard per-entry ceiling,
/// the remaining whole-scan budget, and the wall-clock deadline — whichever is smallest wins, and which
/// one wins determines how the result is interpreted (see [`VerifyOutcome`]).
fn verify_by_decompression(
    entry: &mut zip::read::ZipFile<'_>,
    compressed_for_scoring: u64,
    limits: &RatioLimits,
    budget_remaining: u64,
    deadline: Instant,
) -> VerifyOutcome {
    let ratio_cap = (compressed_for_scoring.max(ENTRY_VERIFY_FLOOR) as f64 * limits.max_entry_ratio) as u64;
    let entry_cap = ratio_cap.min(ENTRY_VERIFY_ABS_CAP).min(budget_remaining);
    // Only when the ratio-derived cap is the smallest (binding) bound does hitting it *prove* danger —
    // otherwise hitting the entry_cap just means our own ceiling was reached first, which says nothing
    // about the entry's true ratio either way.
    let ratio_cap_is_binding = ratio_cap <= ENTRY_VERIFY_ABS_CAP && ratio_cap <= budget_remaining;

    let mut buf = [0u8; VERIFY_CHUNK_SIZE];
    let mut read_total: u64 = 0;
    loop {
        if read_total >= entry_cap {
            return if ratio_cap_is_binding {
                VerifyOutcome::DefinitelyDangerous { bytes_read: read_total, synthetic_uncompressed: read_total.saturating_add(1) }
            } else {
                VerifyOutcome::Inconclusive { bytes_read: read_total }
            };
        }
        if Instant::now() >= deadline {
            return VerifyOutcome::Inconclusive { bytes_read: read_total };
        }
        let want = (entry_cap - read_total).min(VERIFY_CHUNK_SIZE as u64) as usize;
        match entry.read(&mut buf[..want]) {
            Ok(0) => return VerifyOutcome::Counted { bytes_read: read_total, real: read_total },
            Ok(n) => read_total += n as u64,
            Err(_) => return VerifyOutcome::ReadError,
        }
    }
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

    fn scratch(tag: &str) -> crate::fsutil::ScratchDir {
        crate::fsutil::scratch_dir(&format!("cpe-archivesafety-{tag}"))
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

    /// Returns `(header_start, central_header_start)` for the entry named `name` — the two absolute file
    /// offsets CPE-1602's adversarial tests patch into, found via the same `zip` crate the scanner itself
    /// uses (so the offsets are exactly where the real reader looks).
    fn entry_offsets(path: &Path, name: &str) -> (u64, u64) {
        let file = fs::File::open(path).unwrap();
        let mut zip = zip::ZipArchive::new(file).unwrap();
        let entry = zip.by_name(name).unwrap();
        (entry.header_start(), entry.central_header_start())
    }

    /// Overwrites a 4-byte little-endian `u32` field at absolute offset `at` with `value` — used to
    /// hand-patch a local-file-header or central-directory `uncompressed_size` field, exactly like the
    /// independent reviewer's reproduction. `at` is the *header's* start plus the field's fixed offset
    /// within it: `+22` for a local-file-header's `uncompressed_size`, `+24` for a central-directory
    /// header's (see the module's `LOCAL_HEADER_LEN`-adjacent doc comment for the full byte layout).
    fn patch_u32_le(path: &Path, at: u64, value: u32) {
        let mut file = fs::OpenOptions::new().write(true).open(path).unwrap();
        file.seek(SeekFrom::Start(at)).unwrap();
        file.write_all(&value.to_le_bytes()).unwrap();
    }

    /// CPE-1602: the independent reviewer's exact reproduction. A real bomb (2,000,000 zero bytes,
    /// honestly deflated, ~1023x expansion) with `uncompressed_size` hand-patched down to 100 bytes in
    /// **both** the local file header and the central directory. Before this fix the scan trusted those
    /// declared sizes outright and reported `overall_ratio ≈ 0.05, dangerous: false` — a confident,
    /// fully-scanned "safe" verdict over a genuine bomb. Patching both copies keeps them internally
    /// consistent with each other, so the cheap local-header-vs-central-directory cross-check alone
    /// (option 1 from the ticket) would NOT catch this on its own — it's caught instead because a
    /// compressed size that dwarfs a claimed uncompressed size is a structural impossibility for DEFLATE,
    /// independent of whether the two copies of the metadata agree with each other.
    #[test]
    fn the_reviewers_hand_patched_bomb_is_no_longer_reported_safe() {
        let d = scratch("patched-bomb");
        let zip_path = d.join("bomb.zip");
        let bomb = vec![0u8; 2_000_000];
        write_zip(&zip_path, &[("bomb.bin", &bomb)]);

        let (header_start, central_header_start) = entry_offsets(&zip_path, "bomb.bin");
        patch_u32_le(&zip_path, header_start + 22, 100);
        patch_u32_le(&zip_path, central_header_start + 24, 100);

        // Sanity check the patch actually took: both the local header and the central directory must
        // now read back the lie, so a purely metadata-trusting scan would have scored `100 / ~1955 ≈
        // 0.05` — comfortably "safe" by the reviewer's own numbers.
        let peeked = {
            let file = fs::File::open(&zip_path).unwrap();
            let mut zip = zip::ZipArchive::new(file).unwrap();
            let size = zip.by_index(0).unwrap().size();
            size
        };
        assert_eq!(peeked, 100, "the central-directory size must read back as the patched value");

        let result = analyze_archive_safety(&zip_path);
        assert!(
            result.report.dangerous,
            "a real bomb with both declared sizes patched down must still be caught: {:?}",
            result.report
        );
        assert!(!result.unreadable, "the archive opens fine — only its declared metadata lies");
        let _ = fs::remove_dir_all(&d);
    }

    /// CPE-1602: a naive forger who patches only ONE copy of the size metadata (here, just the central
    /// directory — the local file header still truthfully says ~2,000,000) is caught by the cheap
    /// local-header-vs-central-directory cross-check alone, with no decompression needed at all.
    #[test]
    fn a_local_header_central_directory_mismatch_is_never_reported_safe() {
        let d = scratch("mismatch");
        let zip_path = d.join("bomb.zip");
        let bomb = vec![0u8; 2_000_000];
        write_zip(&zip_path, &[("bomb.bin", &bomb)]);

        let (_header_start, central_header_start) = entry_offsets(&zip_path, "bomb.bin");
        patch_u32_le(&zip_path, central_header_start + 24, 100);

        let result = analyze_archive_safety(&zip_path);
        assert!(result.report.dangerous, "a single-field mismatch must still be caught: {:?}", result.report);
        let _ = fs::remove_dir_all(&d);
    }

    /// CPE-1602 round 2: the code-review finding that reopened the hole. The original fix's structural
    /// check only catches `compressed_size` dwarfing `uncompressed_size` — a forger can instead leave
    /// `uncompressed_size` truthful and patch `compressed_size` **up**, in both the local header and the
    /// central directory, so the two copies still agree with each other. This costs the forger nothing
    /// (no real bytes need padding), and defeats a naive fix even after decompression verification: the
    /// verified (honestly measured) uncompressed byte count would still be divided by the inflated,
    /// never-revalidated declared `compressed_size`. The fix is the physical-ceiling clamp: no forger can
    /// declare a `compressed_size` larger than the real on-disk gap to the next entry's header (or the
    /// central directory, for the last entry) without literally writing that many extra bytes.
    #[test]
    fn the_reviewers_inflated_compressed_size_bomb_is_no_longer_reported_safe() {
        let d = scratch("inflated-compressed");
        let zip_path = d.join("bomb.zip");
        let bomb = vec![0u8; 2_000_000];
        write_zip(&zip_path, &[("bomb.bin", &bomb)]);

        let (header_start, central_header_start) = entry_offsets(&zip_path, "bomb.bin");
        // Patch ONLY compressed_size, upward, in both copies — uncompressed_size (the truth: 2,000,000)
        // is left untouched in both, so the two copies of compressed_size still agree with each other,
        // and the two copies of uncompressed_size still agree with each other and with reality.
        patch_u32_le(&zip_path, header_start + 18, 1_999_999);
        patch_u32_le(&zip_path, central_header_start + 20, 1_999_999);

        // Sanity check: the naive metadata-only ratio (2,000,000 / 1,999,999) would have scored ~1.0 —
        // comfortably "safe" — had the physical ceiling not clamped the declared compressed size back
        // down to what the file can actually hold at that offset.
        let (peeked_uncompressed, peeked_compressed) = {
            let file = fs::File::open(&zip_path).unwrap();
            let mut zip = zip::ZipArchive::new(file).unwrap();
            let entry = zip.by_index(0).unwrap();
            (entry.size(), entry.compressed_size())
        };
        assert_eq!(peeked_uncompressed, 2_000_000);
        assert_eq!(peeked_compressed, 1_999_999);

        let result = analyze_archive_safety(&zip_path);
        assert!(
            result.report.dangerous,
            "a bomb with compressed_size inflated (not uncompressed_size deflated) must still be caught: {:?}",
            result.report
        );
        assert!(!result.unreadable);
        let _ = fs::remove_dir_all(&d);
    }

    /// CPE-1602 round 2 false-positive guard: the physical-ceiling clamp derives each entry's bound from
    /// the real byte gap to whatever comes next in the file, so a multi-entry archive with varied
    /// compression methods, sizes, and file-name lengths (all of which shift where each entry's data
    /// actually starts/ends) must still score every entry as safe — clamping to a physical ceiling that
    /// happens to be generous (an honest archive's declared size never exceeds it) must never manufacture
    /// a false "dangerous".
    #[test]
    fn physical_ceiling_clamp_does_not_false_positive_on_a_varied_multi_entry_archive() {
        let d = scratch("varied");
        let zip_path = d.join("varied.zip");
        let pseudo_random: Vec<u8> = (0..5_000_000u32).map(|i| (i.wrapping_mul(2654435761) >> 16) as u8).collect();
        let file = fs::File::create(&zip_path).unwrap();
        let mut w = zip::ZipWriter::new(file);
        let stored: zip::write::FileOptions<()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let deflated: zip::write::FileOptions<()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        w.start_file("empty.txt", stored).unwrap();
        w.start_file("a-name-that-is-considerably-longer-than-the-others-on-purpose.bin", stored).unwrap();
        w.write_all(b"short stored payload").unwrap();
        w.start_file("log.txt", deflated).unwrap();
        w.write_all(&b"repeated log line, very compressible text content\n".repeat(200)).unwrap();
        w.start_file("already-compressed.bin", deflated).unwrap();
        w.write_all(&pseudo_random).unwrap();
        w.finish().unwrap();

        let result = analyze_archive_safety(&zip_path);
        assert_eq!(result.entries_scanned, 4, "every entry should be scored, not skipped");
        assert!(!result.report.dangerous, "an ordinary varied archive must not be flagged: {:?}", result.report);
        assert!(result.report.flagged.is_empty(), "no individual entry should be flagged: {:?}", result.report.flagged);
        assert_eq!(result.unreadable_entries, 0);
        let _ = fs::remove_dir_all(&d);
    }

    /// CPE-1602 DoS-bound proof. Not part of the default suite (a ~512MB fixture isn't worth building on
    /// every CI run) — run explicitly via `cargo test -p cpe-server -- --ignored --nocapture
    /// a_decompression_bomb_against_the_scanner_itself_is_bounded`. Proves the verification pass's own
    /// cap holds: an entry whose true decompressed size (512MB) vastly exceeds every cap must still be
    /// caught, and caught within a small fraction of the time a naive "fully decompress to verify" scan
    /// would take, because the scanner stops at the ratio-derived cap rather than reading to EOF.
    #[test]
    #[ignore]
    fn a_decompression_bomb_against_the_scanner_itself_is_bounded() {
        let d = scratch("scanner-bomb");
        let zip_path = d.join("huge.zip");
        let huge = vec![0u8; 512_000_000]; // 512MB of zeros — deflates to well under 1MB
        write_zip(&zip_path, &[("huge.bin", &huge)]);
        drop(huge);

        let (header_start, central_header_start) = entry_offsets(&zip_path, "huge.bin");
        // Patch the declared size down so metadata alone would look safe, forcing the scan to do real
        // verification work — the scenario this test proves a bound on.
        patch_u32_le(&zip_path, header_start + 22, 1024);
        patch_u32_le(&zip_path, central_header_start + 24, 1024);

        let started = Instant::now();
        let result = analyze_archive_safety(&zip_path);
        let elapsed = started.elapsed();
        eprintln!("scanner-bomb: verified in {:?}, report = {:?}", elapsed, result.report);

        assert!(result.report.dangerous, "the patched huge entry must still be caught: {:?}", result.report);
        assert!(
            elapsed < Duration::from_secs(10),
            "verification must be bounded by the cap, not by the entry's true 512MB size: took {:?}",
            elapsed
        );
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

        let strict = RatioLimits::new(0.5, 0.5, archive_safety::DEFAULT_SUSPICION_RATIO);
        let strict_result = analyze_archive_safety_with_limits(&zip_path, &strict);
        assert!(strict_result.report.dangerous, "a 0.5x limit should flag a ratio of exactly 1.0");
        let _ = fs::remove_dir_all(&d);
    }
}
