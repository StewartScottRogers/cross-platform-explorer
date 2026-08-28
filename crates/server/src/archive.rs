//! Archive listing (CPE-064/109/110/113): browse an archive's directory without extracting it, for the
//! preview pane. Dispatches by extension across ZIP, TAR (± gzip), single-file gzip, 7-Zip, and ISO —
//! reading only the archive directory so it stays cheap even for large archives. Pure-Rust deps (zip /
//! tar / flate2 / sevenz-rust / iso9660), no system libs; extracted into the Server (CPE-815) as real
//! filesystem domain logic. The Tauri `read_archive_entries` command dispatches here.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;

/// One entry inside an archive, for the archive preview.
#[derive(Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ArchiveEntry {
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
}

/// List the entries of a ZIP archive without extracting it.
pub fn zip_entries(path: &str) -> Result<Vec<ArchiveEntry>, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(zip.len());
    for i in 0..zip.len() {
        let entry = zip.by_index(i).map_err(|e| e.to_string())?;
        out.push(ArchiveEntry {
            name: entry.name().to_string(),
            size: entry.size(),
            is_dir: entry.is_dir(),
        });
    }
    Ok(out)
}

/// List the entries of a TAR stream (optionally gzip-decompressed by the caller).
fn tar_entries<R: std::io::Read>(reader: R) -> Result<Vec<ArchiveEntry>, String> {
    let mut archive = tar::Archive::new(reader);
    let mut out = Vec::new();
    for entry in archive.entries().map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let header = entry.header();
        let is_dir = header.entry_type().is_dir();
        let size = header.size().unwrap_or(0);
        let name = entry
            .path()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        out.push(ArchiveEntry { name, size, is_dir });
    }
    Ok(out)
}

/// A single-file gzip (not a .tar.gz) has no directory. Report the decompressed file as one entry: its
/// name is the archive name minus `.gz`, and its size is the gzip trailer's ISIZE (uncompressed length
/// modulo 2^32).
fn gzip_single_entry(path: &str) -> Result<Vec<ArchiveEntry>, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let name = Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let size = if bytes.len() >= 4 {
        let n = bytes.len();
        u32::from_le_bytes([bytes[n - 4], bytes[n - 3], bytes[n - 2], bytes[n - 1]]) as u64
    } else {
        0
    };
    Ok(vec![ArchiveEntry { name, size, is_dir: false }])
}

/// Sentinel `Err` message when a `sevenz-rust` call panics on crafted/malformed `.7z` bytes.
const SEVENZ_PANIC_ERR: &str = "corrupt or unsupported 7z archive";

/// Run a `sevenz-rust` call inside [`std::panic::catch_unwind`], converting a caught panic into a clean
/// `Err(SEVENZ_PANIC_ERR)` instead of letting it keep unwinding (CPE-1415, defensive mitigation for
/// CPE-1411's finding).
///
/// `sevenz-rust` 0.6.1 has a known, unfixed upstream bug: `Archive::init_archive` does a bare, unchecked
/// `SIGNATURE_HEADER_SIZE + next_header_offset` (`u64 + u64`) add on bytes read straight out of the
/// signature header, with no range check — a crafted `.7z` whose header claims a `next_header_offset` near
/// `u64::MAX` panics ("attempt to add with overflow" in a debug build; see the two
/// `sevenz_signature_header_*_overflow_is_a_known_upstream_panic` tests in
/// `crates/server/tests/archive_panic_safety.rs` for the full writeup and a byte-exact repro). This was
/// already CONTAINED before this function existed: every one of this module's `sevenz-rust` call sites
/// runs inside a Tokio `spawn_blocking` task, and an uncaught panic in a blocking task is itself caught at
/// the task boundary and surfaced as a `JoinError` → `Err` (no `panic = "abort"` override in this
/// workspace), so a crafted `.7z` never crashed the process — at worst it failed one listing/extraction.
/// This helper is belt-and-suspenders on top of that: it turns the panic into a normal `Err` right at the
/// `sevenz-rust` call site instead of relying on the outer task boundary, which avoids unwinding all the
/// way up through this module's own frames and gives a domain-appropriate error message instead of a
/// generic `JoinError`.
///
/// `AssertUnwindSafe` is required because `f` typically closes over `&mut`/trait-object state (a
/// found-flag, a progress emitter, a running byte/item tally) that isn't `UnwindSafe` by default — that's
/// sound here because every caller propagates the `Err` immediately via `?` and never reads that captured
/// state again after a panic is caught.
///
/// If a future `sevenz-rust` upgrade fixes the overflow, the `#[should_panic]` tests above start failing
/// (they stop panicking) — that's the signal to revisit both those tests AND whether this wrapper is still
/// worth keeping (it's cheap and harmless either way, so there's no urgency to remove it).
fn catch_sevenz_panic<T>(f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or_else(|_| Err(SEVENZ_PANIC_ERR.to_string()))
}

/// List the entries of a 7-Zip archive via sevenz-rust (CPE-110).
fn sevenz_entries(path: &str) -> Result<Vec<ArchiveEntry>, String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let len = file.metadata().map_err(|e| e.to_string())?.len();
    let archive = catch_sevenz_panic(|| sevenz_rust::Archive::read(&mut file, len, &[]).map_err(|e| e.to_string()))?;
    Ok(archive
        .files
        .iter()
        .map(|f| ArchiveEntry {
            name: f.name().to_string(),
            size: f.size(),
            is_dir: f.is_directory(),
        })
        .collect())
}

/// List the files in an ISO 9660 disc image (bounded), via iso9660 (CPE-113).
fn iso_entries(path: &str) -> Result<Vec<ArchiveEntry>, String> {
    use iso9660::{DirectoryEntry, ISODirectory, ISO9660};
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let iso = ISO9660::new(file).map_err(|e| e.to_string())?;

    let mut out = Vec::new();
    let mut stack: Vec<(String, ISODirectory<fs::File>)> = vec![(String::new(), iso.root)];
    while let Some((prefix, dir)) = stack.pop() {
        if out.len() >= 2000 {
            break;
        }
        for entry in dir.contents() {
            let entry = match entry {
                Ok(e) => e,
                // `continue` here would infinite-loop (CPE-1411): the `iso9660` crate's directory
                // iterator does not advance its read cursor on a parse error (`ISODirectoryIterator::next`
                // only updates `next_offset` on `Ok`), so calling `.next()` again after an `Err` re-reads
                // the exact same bytes and gets the exact same `Err`, forever — a single malformed
                // directory record (e.g. a non-UTF8 identifier) hangs the whole listing thread. `break`
                // stops reading the REST of *this* directory's entries (same skip-what-we-can't-read
                // spirit as every other archive/filesystem reader here) while the outer
                // `while let Some(..) = stack.pop()` still visits any other directories already queued.
                Err(_) => break,
            };
            match entry {
                DirectoryEntry::Directory(d) => {
                    if d.identifier == "." || d.identifier == ".." {
                        continue;
                    }
                    let full = if prefix.is_empty() {
                        d.identifier.clone()
                    } else {
                        format!("{prefix}/{}", d.identifier)
                    };
                    out.push(ArchiveEntry { name: format!("{full}/"), size: 0, is_dir: true });
                    stack.push((full, d));
                }
                DirectoryEntry::File(f) => {
                    let full = if prefix.is_empty() {
                        f.identifier.clone()
                    } else {
                        format!("{prefix}/{}", f.identifier)
                    };
                    out.push(ArchiveEntry { name: full, size: f.size() as u64, is_dir: false });
                }
            }
        }
    }
    Ok(out)
}

/// List an archive's entries without extracting it, for the preview pane. Dispatches by extension: ZIP
/// family (zip/jar/apk/…), TAR, gzip-compressed TAR (.tar.gz/.tgz), single-file gzip (.gz), 7-Zip, ISO,
/// RAR (listing only — see [`crate::rar`]; there is no extractor for it, so it's never routed to
/// `extract_archive*`/`pack_*`).
///
/// Deliberately NOT dispatched here (CPE-1439): xz/bz2/zst/lz/lzma single-file compression — this crate
/// has no xz/bzip2/zstd decoder (only `flate2` for gzip), so there's no way to peel the wrapper off even a
/// `.tar.xz`-style inner tar without adding a new dependency, and a bare single-file blob has no entry
/// list at all. dmg (Apple disk image) / cab (MS cabinet) — no container reader exists for either. The
/// frontend (`provider.ts`'s `ARCHIVE_EXT`) never routes any of these seven extensions here; if it ever
/// did, they'd fall into the `else` branch below and fail as a bad zip.
pub fn read_archive_entries(path: &str) -> Result<Vec<ArchiveEntry>, String> {
    let lower = path.to_lowercase();
    if lower.ends_with(".tar") {
        let file = fs::File::open(path).map_err(|e| e.to_string())?;
        tar_entries(file)
    } else if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        let file = fs::File::open(path).map_err(|e| e.to_string())?;
        tar_entries(flate2::read::GzDecoder::new(file))
    } else if lower.ends_with(".gz") {
        gzip_single_entry(path)
    } else if lower.ends_with(".7z") {
        sevenz_entries(path)
    } else if lower.ends_with(".iso") {
        iso_entries(path)
    } else if lower.ends_with(".rar") {
        crate::rar::rar_entries(path)
    } else {
        zip_entries(path)
    }
}

// ---------------------------------------------------------------------------
// Archive creation & extraction (CPE-251/252/242)
// ---------------------------------------------------------------------------
//
// # Where every file-creating call in this module writes, and who chose the name (CPE-1733)
//
// PR #901's review observed that this module has ~14 `File::create`/`fs::write` calls with no clobber or
// link guard on any of them, and deliberately scoped the claim to *"same primitive shape, unguarded"*
// rather than *"same bug"*, because nobody had split which of those destinations a **user** names from
// which are app-internal. This is that split. It is the load-bearing half of CPE-1733; the guards below
// are what it concluded, not what it assumed.
//
// Provenance is the question that decides whether a guard is owed, because the hazard is a **pre-existing
// link at the destination name**, and only a path the user (or an archive) can point somewhere can have
// one:
//
// | # | Site                                       | Primitive        | Destination provenance                      | Guard |
// |---|--------------------------------------------|------------------|---------------------------------------------|-------|
// |  1| `temp_extract_target` / `session_root`     | `create_dir_all` + **exclusive `create_dir`** | shared `%TEMP%/cpe-archive` root, then a private `s<pid>-<random>` session root, then `e<seq>` inside it | link on the root + exclusive create (twice) |
// |  2| `extract_archive_entry`                    | `File::create`   | inside row 1's private dir + `file_name()`   | none — carried by row 1 |
// |  3| `extract_tar_entry`                        | `File::create`   | `out`, always a `temp_extract_target`        | none — carried by row 1 |
// |  4| `extract_7z_entry`                         | `File::create`   | `out`, always a `temp_extract_target`        | none — carried by row 1 |
// |  5| `extract_rar_entry`                        | `fs::write`      | `out`, always a `temp_extract_target`        | none — carried by row 1 |
// |  6| `compress_to_zip`                          | `File::create`   | **caller-supplied `dest`**                   | link |
// |  7| `create_empty_zip`                         | `create_new`     | **caller-supplied `dest`**                   | link (+ `create_new`); occupancy wording (CPE-1744) |
// |  8| `compress_to_targz`                        | `File::create`   | **caller-supplied `dest`**                   | link |
// |  9| `compress_to_zip_encrypted`                | `File::create`   | **caller-supplied `dest`**                   | link |
// | 10| `compress_to_zip_streamed`                 | `File::create`   | **caller-supplied `dest`**                   | link |
// | 11| `compress_to_targz_streamed`               | `File::create`   | **caller-supplied `dest`**                   | link |
// | 12| `compress_to_zip_encrypted_streamed`       | `File::create`   | **caller-supplied `dest`**                   | link |
// | 13| `extract_archive` (`.gz` branch)           | `File::create`   | **user-named dir** + stem of the archive name| link |
// | 14| `extract_archive_streamed` (`.gz` branch)  | `File::create`   | **user-named dir** + stem of the archive name| link |
// | 15| `extract_zip_encrypted`                    | *row 16's loop* | **archive-controlled** name under user dir   | row 16's, exactly — this row no longer has an extractor of its own (CPE-1807) |
// | 16| `extract_zip_archive_stream`               | `claim_destination_handle`/`symlink` (the `File::create` went with CPE-1913) | **archive-controlled** name under user dir   | leaf link + **per-component containment** + **handle-relative component walk** (file + dir branches CPE-1913, **symlink branch CPE-1973**) + **link-target containment** (skip, recorded) + `entry_name_is_safe`; **residual: a RACED component swap on the symlink branch** |
// | 17| the three remaining extraction `dest` roots | `create_dir_all` | **user-named dir**                           | none (a live link is followed on purpose) — wording only, see below |
// | 18| the per-entry directory creation inside row 16's loop, shared by rows 15/23 | `create_dir_beneath` / `create_beneath` — the two `create_dir_all`s went with CPE-1913 | **archive-controlled** dir name under user dir | **per-component containment** + **handle-relative component walk** (skip) |
// | 19| `extract_7z_safe`'s callback                | `File::create` **inside `sevenz-rust`** | **archive-controlled** name under user dir | leaf link + **per-component containment** + **handle-relative component walk** (CPE-1938) (skip) + `entry_name_is_safe`; **residual: a RACED component swap — planted only, see `entry_component_action`** |
// | 20| `extract_7z_stream`'s callback              | `File::create` **inside `sevenz-rust`** | **archive-controlled** name under user dir | leaf link + **per-component containment** + **handle-relative component walk** (CPE-1938) (skip, recorded) + `entry_name_is_safe`; **residual: a RACED component swap — planted only, see `entry_component_action`** |
// | 21| `tar_unpack`                                | `File::create`/`symlink` **inside `tar`** | **archive-controlled** name under user dir | `entry_name_is_safe` + leaf link + **per-component containment** + **handle-relative component walk** (CPE-1938) + **link-target containment** (both link kinds) (skip, recorded); **residual: a RACED component swap — planted only, see `entry_component_action`** |
// | 22| `extract_tar_stream`                        | `File::create`/`symlink` **inside `tar`** | **archive-controlled** name under user dir | `entry_name_is_safe` + leaf link + **per-component containment** + **handle-relative component walk** (CPE-1938) + **link-target containment** (both link kinds) (skip, recorded); **residual: a RACED component swap — planted only, see `entry_component_action`** |
// | 23| `extract_archive`'s zip branch              | *row 16's loop* | **archive-controlled** name under user dir | row 16's, exactly — this row no longer has an extractor of its own (CPE-1759) |
//
// **Rows 21–23 are CPE-1773 + CPE-1774, and the table itself is why they were missing.** The version of
// this table CPE-1733 wrote listed `entry_name_is_safe` as the guard for rows 15/16 and 19/20 and named
// **no tar row at all** — so CPE-1758, whose scope came from this table, closed the ADS/reserved-name
// hole at four sinks and left it wide open for a whole archive family, on the path the right-click
// → Extract button actually uses. A sink omitted from the inventory is a sink nobody is scheduled to
// guard, which is the same lesson as the unpinned-prose one below, one level up. Rows 21–22 own no
// `File::create` in this file for rows 19–20's reason: the write is `tar`'s `Entry::unpack_in`, and the
// guard is a check *before* handing the entry over. **Row 23 no longer owns an extractor at all**
// (CPE-1759): its write is row 16's, because that is now the loop it calls.
//
// **Rows 21–23 are also the first rows guarding a destination that is not a path at all.** A zip or tar
// entry can declare itself a **symlink**, and its stored bytes are the link's *target*. Every guard
// above asks about the entry's name; this attack's name is ordinary (`evil_link`) and the payload is the
// target, so nothing above it fires. See [`link_target_action`] for the measurement, the policy, and why
// `confined_to` — not a string check — is what answers it.
//
// **7z is NOT in this group, and that is measured rather than assumed.** `sevenz-rust` 0.6.1's source
// contains the string "symlink" **zero times** (`grep -ri symlink sevenz-rust-0.6.1/src` → no hits), so
// `default_entry_extract_fn` has no link-materialising branch to guard: a 7z entry that names itself a
// link is written as an ordinary file whose contents are the target text. CPE-1746 already covers a link
// **already sitting in `dest`** on that path, which is the other half.
//
// **The row count reconciles to the source**, and all three numbers were re-derived from the file for
// CPE-1938 round 2 rather than carried forward. `archive.rs` has **6** `create_dir_all` calls: **2 in
// row 1** (the shared root, and re-creating this session's own root if another instance's sweeper
// removed it — CPE-1786), **3 in row 17** (the extraction `dest` roots — down from 4:
// [`extract_zip_encrypted`] no longer creates `dest` itself, since CPE-1807 made it call row 16's loop,
// which already does), and **1 in row 21** (`tar_unpack`'s, which is `tar::Archive::_unpack`'s own
// `dst` creation, reproduced along with the rest of that loop — CPE-1773; it is a row-17-shaped call, on
// the same user-named `dest`, and shares row 17's wording guard via `extraction_dest_error`). **Row 18
// contributes none**: CPE-1913 replaced both of its per-entry calls with the handle-relative walk.
// Plus row 1's **2** exclusive `fs::create_dir`s (the session root, then the per-extraction directory
// inside it).
//
// Rows 2–14 are the **11** `File::create` calls, plus the 1 `fs::write` (row 5) and the 1 `create_new`
// (row 7). Rows 15, 16 and 21–23 add none of their own: row 15's and row 23's write is row 16's loop,
// rows 21–22's is `tar`'s, and **row 16's own went with CPE-1913**, which replaced it with
// `claim_destination_handle` + `create_beneath`. So the `File::create` count moves only when a row
// stops owning an extractor.
//
// **The numbers this line used to carry, and why that matters more than the arithmetic.** Until
// CPE-1938 round 2 it read `8, 12, 2` and itemised "2 in row 18" — the pre-CPE-1913 counts, wrong on
// both of the two numbers CPE-1913 changed, six lines above the rows CPE-1938 was editing, in a ticket
// about enumerating rather than recalling. That is the **CPE-1933** shape exactly: a claim *about the
// source* that no test reads the source to check, going quietly false while every green test around it
// reads as vouching for it. It is stated rather than mechanised because the derivation a guard would
// need — "which table row owns this call site" — is not in the source; if that changes, derive it.
// Current, counted over the production half of this file (everything above `#[cfg(test)]`, comment
// lines excluded): **6 `create_dir_all`, 11 `File::create`, 2 exclusive `fs::create_dir`.**
// Row 18 was missing from the first version of this table, which billed itself as the inventory — the
// count line exists so the next reader can check that claim in one subtraction instead of trusting it
// (PR #906 review), which only works if the subtraction is redone whenever a call site moves.
//
// **Rows 19–20 are deliberately outside that arithmetic**: they own no `File::create` in this file. The
// write is `sevenz_rust::default_entry_extract_fn`'s, and the guard is a **pre-call check in the callback
// we already supply**, which is the whole finding of CPE-1746 — "the writer is in another crate" was read
// as "there is nothing to guard", when the callback receives `entry_dest` one statement earlier. They are
// in the table because the *destination provenance* is what decides whether a guard is owed, and theirs is
// the same archive-controlled name under a user directory as rows 15–16.
//
// ## The three primitives do NOT behave alike, and each row above was measured, not reasoned about
//
// - `File::create` / `fs::write` **follow the link and write through it.** Measured on Windows for this
//   ticket, with no guard: on a *dangling* link `File::create` returns `Ok`, creates the link's target,
//   and leaves the slot a symlink; on a *live* link it returns `Ok` and the link's target is clobbered
//   (`victim bytes = Some("CLOBBERED")`). This is the CPE-1719 finding, reproduced here.
// - `create_dir_all` is **not destructive** (CPE-1729) — but the two things this ticket first inferred
//   from that were both wrong, and both were measured wrong by its UAT.
//
//   **It does not "do nothing at all" on a dangling link — it fails**, and takes the whole extraction
//   with it: Windows `Err(os error 183, "Cannot create a file when that file already exists.")`, Linux
//   `Err(os error 17, "File exists")`. That is the *same misleading wording* row 7 got a guard for.
//
//   **And on a *live* directory link it succeeds and redirects.** `create_dir_all` returns `Ok` over a
//   directory that already exists *and* over a directory symlink or junction standing in for one, and
//   everything written beneath goes wherever the link points — `landed_outside = true`. Redirection, not
//   destruction; "not destructive" never meant "not hazardous".
//
//   **Rows 17 and 18 part company here, and CPE-1744 is where they did.** Row 17's `dest` is an existing
//   folder the user **pointed at**, not a new name being claimed. `fsutil`'s own rule for the family —
//   *"am I claiming this name, or editing this thing?"* — says following a link is **correct** when the
//   user pointed at the thing, which is why `replace_file_contents` follows one too; refusing would break
//   extracting into a folder the user deliberately reached through a shortcut. So row 17 keeps following a
//   **live** link, and CPE-1744 changed only what it says about a **dangling** one — a pure wording fix on
//   a failure that already happened, exactly like row 7's, and for the same reason (see
//   [`extraction_dest_error`]).
//
//   Row 18 is the opposite case and is now **guarded**: the directory name is the *archive's*, so nobody
//   pointed at anything, and a link there redirects the entry out of the folder the user chose. That is
//   [`entry_dir_action`] — containment only, because a directory link that leads somewhere else *inside*
//   `dest` still writes where the user asked.
//
//   **Platform boundary:** the redirect figures are Windows 11; the `create_dir_all`-on-a-dangling-link
//   failure was measured on both Windows and Linux (codes above).
// - `create_new` (row 7) is the only site here that was **already** safe, and it is safe by the OS rather
//   than by us. Measured on Windows: `create_new` on a dangling link fails `AlreadyExists (os error 80)`
//   and does *not* create the target; on a live link it fails the same way and the target is untouched.
//   Its message, though, is *"The file exists"* — the exact wording `fsutil`'s module doc calls out as
//   sending the user to delete a file at a name that actually holds a link elsewhere. So row 7's guard is
//   a **message** fix on top of a working belt, not a new belt, and it is labelled that way at the site.
//
// ## Rows 1–5: app-owned — but that is now earned rather than asserted
//
// All five write under `temp_extract_target`. The first version of this section claimed they were
// **unreachable** by the hazard because the directory "is created by that call and never reused". That
// was wrong, and the PR #906 review measured it: `create_dir_all` silently accepts a pre-existing
// directory *and* a directory symlink, so it establishes nothing about who got there first, and the leaf
// name is archive-controlled — an attacker supplying the archive knows exactly what to name a link. What
// was actually protecting these rows was `%TEMP%` being **per-user on Windows**; on a shared Unix `/tmp`
// it is CWE-377 into CWE-59.
//
// `temp_extract_target` now claims its per-extraction directory with an **exclusive `fs::create_dir`**,
// which bounces both a squatted directory and a directory link (`Err(AlreadyExists)` for each, measured),
// retrying the next sequence number. Once that returns `Ok`, this process is the only thing that has ever
// been inside, so rows 2–5's leaf really is unreachable. The full measurement, the residuals it does not
// close, and why "never reused" was only ever true *within* a process are on `temp_extract_target` itself.
//
// **CPE-1786 added a directory between the shared root and that leaf** — a per-process `s<pid>-<random>`
// session root, also claimed with an exclusive `create_dir` — because the per-extraction directories were
// never removed (1,394,403 of them on one machine) and the resulting `<pid>-<seq>` name pressure was
// producing real failures. That is a *lifetime* change, not a provenance one: every sentence above about
// who created what still holds, one level deeper. Who owns an extraction directory and for how long is on
// `session_root`.
//
// ## Rows 15–16 — what they cover, and the two things they do NOT
//
// **Rows 19–20 (CPE-1746) inherit this section unchanged.** They apply the same guards to the same kind
// of destination — an archive-controlled name under a user-named directory — so the section below is
// theirs too: `entry_name_is_safe` now applies the same per-segment rules as `is_safe_name` (closed by
// CPE-1758). The only difference is where the write happens (inside `sevenz-rust`), which changes
// nothing about what the checks see. Do not read the "rows 15–16" wording below as scoping the fix away
// from 7z.
//
// **Traversal is answered, and only traversal.** `guarded_join`/`is_safe_name` (CPE-1461,
// `crate::transfer`) is not in this path and does not need to be added *for traversal*: this module has
// had [`entry_name_is_safe`] since CPE-628, applied to every archive-controlled name at every site it
// writes itself, plus `extract_archive_entry_any`/`extract_rar_entry` validate `inner` up front. A second
// traversal guard would be two guards for one question.
//
// The first version of this paragraph stopped there, and that was wider than the search behind it.
// `guarded_join` does not only answer traversal: it applies [`crate::transfer::is_safe_name`] per segment
// (which fails closed on a `:` anywhere and on a leading `..`, CPE-1461/1709) and, on Windows, sanitises
// each segment through `local_safe_segment`. [`entry_name_is_safe`] had **no equivalent to either**, until
// CPE-1758 (below). Measured for the PR #906 review, before the fix:
//
// ```text
// [M7] entry_name_is_safe("file:stream") = true    entry_name_is_safe("..evil") = true
//      entry_name_is_safe("con") = true            entry_name_is_safe(" sp ") = true   ("x." = true)
// [M8 fs::write to "adsbase:stream"] = Ok(())
//      adsbase_len = Some(4) (unchanged)   a plain file named "adsbase:stream" exists = false
// ```
//
// So a ZIP entry named `file:stream` passed this module's check, reached rows 15–16's `File::create`, and
// on NTFS the bytes landed in an **alternate data stream** of a neighbouring file, leaving the user no
// visible file at all — the CPE-1709 bug, at a sink CPE-1709 did not cover. The Windows reserved-device
// and trailing-space/dot shapes were accepted too. CPE-1744 closed the *containment* half of `guarded_join`
// (below) and deliberately left this *per-segment name* half, which belongs with the `local_safe_segment`
// family and changes what names are legal rather than where they land.
//
// **CPE-1758 closed it: [`entry_name_is_safe`] now runs every `Normal` path segment through
// `is_safe_name` and `local_safe_segment`, same as `guarded_join`.** Re-measured after the fix:
//
// ```text
// [CPE-1758 M] entry_name_is_safe("file:stream") = false   entry_name_is_safe("..evil") = false
//              entry_name_is_safe("con") = false (Windows) entry_name_is_safe(" sp ") = false (Windows)
//              entry_name_is_safe("x.") = false (Windows)  entry_name_is_safe("a/b.txt") = true (unchanged)
// ```
//
// **REFUSE, not rename** — see [`entry_name_is_safe`]'s own doc for the full argument. In short:
// `local_safe_segment`'s rename is right for a transfer sink that owns the destination name outright; an
// extraction entry that fails the check is **skipped, same as a traversal escape always was**, and the
// skip is not silent: the streamed extractors (rows 16/20/22) push `"{name}: unsafe entry name, skipped"`
// into `ArchiveReport::errors` **and increment `ArchiveReport::skipped`** (CPE-1775 — the count is what
// the toast reads, and until it existed the frontend read `errors` only when `failed > 0`, so every
// refusal produced a plain success notice with a quietly lower count; see [`ArchiveReport`]).
// `extract_plan::plan_extract` also records it in `skipped_unsafe`, though that
// field currently has no UI consumer — nothing calls `plan_extract` outside its own module yet. Pinned by
// `entry_name_is_safe_now_agrees_with_transfers_is_safe_name` below, so the gap closing is a recorded,
// CI-enforced fact rather than a sentence.
//
// **The link check WAS leaf-only. CPE-1744 closed that** — and it was the largest of the three gaps
// CPE-1733 recorded, by blast radius: five shipping sinks, against the one 7z path CPE-1746 fixed.
// `entry_name_is_safe("sub/x.txt")` is `true`, and rows 15–16 used to run `create_dir_all(parent)` *before*
// the leaf guard, so a directory symlink or junction already sitting at `dest/sub` redirected everything
// under it out of `dest` while the leaf guard saw no link (the leaf did not exist yet). A **junction needs
// no privilege on Windows**, so this was an ordinary user's folder, not a hardened attacker scenario.
// Measured before the fix, one entry named `sub/leaf.txt` and `dest/sub` a live directory link:
//
// ```text
// [CPE-1744 M] zip ONE-SHOT extract_archive   landed_outside=false  Err("invalid Zip archive: Invalid
//                                                                       symlink target path")
// [CPE-1744 M] zip STREAMED                   landed_outside=TRUE   Ok(ArchiveReport { done: 1, errors: [] })
// [CPE-1744 M] zip ENCRYPTED one-shot (row 15) landed_outside=TRUE  Ok(..)
// [CPE-1744 M] zip ENCRYPTED streamed (row 16) landed_outside=TRUE  Ok(ArchiveReport { done: 1, errors: [] })
// [CPE-1744 M] 7z ONE-SHOT (row 19)           landed_outside=TRUE   Ok(..)
// [CPE-1744 M] 7z STREAMED (row 20)           landed_outside=TRUE   Ok(ArchiveReport { done: 1, errors: [] })
// [CPE-1744 M] tar.gz ONE-SHOT                landed_outside=false  Err("trying to unpack outside of
//                                                                       destination path: …")
// [CPE-1744 M] tar.gz STREAMED                landed_outside=false  Err("failed to unpack `…\\out\\sub`")
// ```
//
// Five `Ok`s with the bytes outside the folder the user chose, no notice on any of them. Rows 15/16/19/20
// now resolve **every intermediate component** via [`crate::fsutil::confined_to`] before writing —
// [`entry_sink_action`] for a file entry, [`entry_dir_action`] for a directory one — and the check runs
// *before* the `create_dir_all(parent)` so an escaping entry cannot create its intermediate folders
// outside `dest` on the way to being refused. Pinned by
// `rows_15_to_20_refuse_a_file_entry_addressed_through_a_symlinked_intermediate_directory`.
//
// **The two paths that already refused have now been adopted, not merely recorded — CPE-1759.** `tar`
// and the zip crate's one-shot `extract` both aborted the whole run rather than skipping the entry:
// safe-sounding, and the opposite of the skip-and-keep-going contract rows 15–20 have. For tar,
// [`entry_sink_action`]/[`entry_dir_action`] answer the containment question *before* `unpack_in`
// reaches its own internal `validate_inside_dst`, and [`hard_link_target_action`] answers the hard-link
// one before `unpack_in`'s, converting both aborts into counted skips without changing either verdict
// (`unpack_in`'s checks stay as the belt behind them). For zip, row 23 stopped being an extractor at all
// and became a call into row 16's loop.
//
// **The line this table draws, stated so it can be checked rather than assumed: a REFUSAL skips, a
// FAILURE is recorded against its own entry, and only a RUN-scoped problem aborts.** A refusal is a
// per-entry decision this module makes and can repeat — an unsafe name, a link at the slot, an escaping
// destination, an escaping link target, a link this platform will not create. A failure is the write
// itself not working: `File::create`, `io::copy`, `fs::hard_link`, an unreadable slot
// ([`EntrySlotAction::Fail`]).
//
// **CPE-1935 rewrote the second half of that sentence, and this paragraph with it.** Through CPE-1759
// the rule was *"a refusal skips; a failure aborts"*, and a failure at any row discarded the report
// along with every entry already on disk. A leaf failure is now counted in [`ArchiveReport::failed`]
// with its reason beside the skips and the run carries on — measured and pinned rather than left
// implied: a tar hard link whose target is simply missing is one counted failure whose neighbours still
// extract (`cpe1759_an_escaping_tar_hard_link_is_skipped_while_a_missing_target_still_fails` asserts
// `Ok` with `failed == 1`, having previously asserted `Err`), and so is a slot whose `symlink_metadata`
// fails (`cpe1935_an_unreadable_slot_is_a_recorded_entry_failure_on_both_tar_paths`).
// [`EntrySlotAction::Abort`] keeps only what genuinely is the whole run's problem — the extraction
// folder, a shared path component, the archive container — where recording it against one entry would
// be a lie about scope.
//
// **CPE-1759's own review found the rule broken in two places by the commit that stated it**, which is
// the argument for stating it as a testable line rather than a principle:
//
// - `tar_entry_refusal` collapsed `Skip(m) | Abort(m)` into one arm. That arm had been *dead* on `main`
//   — only `link_target_action` fed it, and it never returns `Abort` — and adding the slot guard made it
//   live, so an unreadable slot became a silent tar skip returning `Ok`. UAT finding 6 verbatim, three
//   functions from the comment warning about it.
// - The link-creation fallback swallowed every `io::ErrorKind` into a refusal asserting the cause was
//   the Windows symlink privilege, so a full disk produced a green extraction advising Developer Mode.
//   [`link_creation_is_categorical`] now draws the line on the **raw OS code**.
//
// **And round 3 found the reason given for that second fix was itself unmeasured.** Round 2 wrote that
// raw codes were needed because Rust decodes `ERROR_PRIVILEGE_NOT_HELD` (1314) and
// `ERROR_ACCESS_DENIED` (5) to the same `PermissionDenied`. Measured on the pinned toolchain, 1314
// decodes to `Uncategorized` and 5 to `PermissionDenied` — they never collided. The conclusion survived
// on a *stronger* reason (`Uncategorized` has no stable name, so a kind-based match cannot express the
// case at all), and the red-proof had gone red for a different reason than its author believed. **That
// is this ticket's own "abort is atomic" mistake, committed by the person who had just demolished it**,
// two commits later, and it is written up on [`ERROR_PRIVILEGE_NOT_HELD`] rather than quietly corrected:
// a mutation going red confirms the code decides something; it never confirms the story about why.
//
// Round 3 also found the *promise* half broken: the in-app help told users a link-less filesystem would
// skip the entry, while the only code path that delivered it was an arm no shipping platform reaches.
// Windows 1/50 and POSIX `EPERM` now deliver it — see [`WINDOWS_NO_LINK_SUPPORT`] and [`EPERM`], and
// note that on POSIX `EPERM`-vs-`EACCES` **is** a real same-kind collision, which is where round 2's
// story would have been true if it had been told about the right platform.
//
// **That refusal was ZIP-only through round 4, which caught the help claiming otherwise; CPE-1813
// closed the gap.** `materialise_entry_symlink` still has exactly one call site — inside
// `extract_zip_archive_stream` (rows 15/16/23, all three since CPE-1807) — and `tar`'s `unpack_in` still owns rows 21–22's link
// creation, deliberately. What changed is what happens to its `Err`: instead of both tar sinks
// propagating it with `?`, [`tar_link_creation_outcome`] translates it through the same classifier,
// after recovering the raw OS code `unpack_in` rewraps away (see [`recover_link_syscall_error`]). A tar link
// the volume cannot hold now skips, the same as a zip one. The pattern across rounds 2–4 was one thing:
// every wrong claim was reasoned from the shape of the code instead of read off the path. The
// implementation was right or nearly right each time; the story about it was not.
//
// The one judgement call left is that a machine which *categorically* has no links refuses rather than
// fails: it is a standing property of the machine, and every ordinary entry in the archive still
// extracts — see [`link_creation_is_categorical`] for the causes that qualify and the ones that
// deliberately do not.
//
// ## The three extractors that are NOT our write loop — measured one at a time, because they differ
//
// `tar`'s `Archive::unpack`/`Entry::unpack_in`, `zip`'s `ZipArchive::extract` and `sevenz_rust`'s
// `default_entry_extract_fn` create their files **inside those crates**, so this module has no create site
// to guard.
//
// **"No create site to guard" is not the same as "cannot be guarded", and CPE-1746 is where that gap
// closed.** The sentence here used to add "and cannot reach one without reimplementing each crate's
// extraction", which was true of `tar` and `zip` — whose extractors take a destination and no per-entry
// hook — and **false of `sevenz-rust`**, whose `decompress_file_with_extract_fn` hands us the entry and
// its `entry_dest` *before* the write, in a callback this module was already writing for the
// `entry_name_is_safe` check. The guard was one more condition in a closure that existed, not a
// reimplemented extractor. Rows 19–20 below are that condition; the generalisation from "the write is in
// another crate" to "nothing can be done about it" is the same one-step-too-far move rows 1–5 were
// corrected for, and it was costing the shipping path real bytes for as long as it stood.
//
// **CPE-1773/1774 finished the thought for the other two, and the "true of `tar` and `zip`" clause above
// is now only half true.** Neither crate offers a per-entry hook, so the guard cannot go *inside* their
// extraction — but neither has to. `tar`'s per-entry unit, `Entry::unpack_in`, is public and is what
// `Archive::unpack` calls in a loop, so rows 21–22 own the loop and ask before handing each entry over
// (`tar_unpack` reproduces `_unpack`'s directory-deferral pass verbatim so nothing else moves; see its
// doc). `zip`'s `ZipArchive::extract` genuinely has no unit to borrow, so CPE-1774 made row 23 a
// **pre-pass** over the central directory instead — enough for the link-target question, which can be
// answered from the entry list alone, but able only to abort.
//
// **CPE-1759 removed that last asymmetry by removing the extractor.** Row 23 no longer calls
// `ZipArchive::extract` at all: it calls row 16's loop, which this module already owns entry by entry.
// The pre-pass is gone, and with it the "answerable from the entry list alone" constraint that shaped
// it. What kept that from being an option for CPE-1774 was the two capabilities the crate's extractor
// had and our loop did not — unix permission bits and real symlink entries — and CPE-1759 implemented
// both here rather than trading them away (see [`create_entry_symlink`]). So the honest generalisation
// is one step further on again: what mattered was not whether the write is in another crate, nor even
// whether that crate exposes the entry before it writes, but whether we were willing to own everything
// that crate's extractor did for us.
//
// **An earlier version of this comment said a pre-existing link in the destination "is therefore still
// followed on the tar, 7z and one-shot-zip paths". That is false for two of the three, and the UAT for
// this ticket measured it on Windows and Linux alike.** The sentence was a reasonable inference from "we
// do not guard it" and it was never checked — the exact move this ticket exists to stop, shipped inside
// the ticket about it, in four places including the user-facing docs. Measured here, live link at
// `dest/a.txt` pointing at a victim outside `dest`:
//
// ```text
// [tar ONE-SHOT and STREAMED]  outcome = Ok(..)   victim bytes = Some("VICTIM ORIGINAL")
//                              slot is link = Ok(false)   slot is file = Ok(true)
// [zip ONE-SHOT]               outcome = Err("invalid Zip archive: Invalid symlink target path")
//                              victim bytes = Some("VICTIM ORIGINAL")   b.txt extracted = false
// [7z STREAMED]                outcome = Ok(ArchiveReport { done: 2, failed: 0, errors: [] })
//                              victim bytes = Some("ARCHIVED A")   slot is link = Ok(true)
// ```
//
// So, one behaviour each:
//
// - **tar did not follow — it *destroyed*. FIXED by CPE-1759 (rows 21–22).** "Destroyed" is precise, and
//   which of its two possible meanings applied decided the fix: `tar-0.4.46/src/entry.rs:644-662` opens
//   the destination with `create_new(true)` and, on the `AlreadyExists` a symlink at that name produces,
//   calls `fs::remove_file(dst)` and retries. `remove_file` does not follow a symlink on any supported
//   platform, so it **unlinked the user's link and wrote a regular file in its place**; it never wrote
//   *through* it (the crate's own comment there — "Ensure we write a new file rather than overwriting
//   in-place which is attackable" — is why the victim's bytes were always safe). What the user lost was
//   the link, silently, with the call returning `Ok`. Both tar sinks now ask [`entry_sink_action`] before
//   handing the entry to `unpack_in`, so the same input gives rows 15–20's answer: the entry is skipped
//   (recorded on the streamed path), and the link and its target are untouched.
//
//   **CPE-1744 looked at closing it and did not**, on the grounds that the one-shot path was
//   `Archive::unpack`, which has no per-entry hook, so a half-fix would manufacture a fresh divergence.
//   That obstacle was removed by CPE-1773, which owns the one-shot loop as [`tar_unpack`] for its own
//   reasons; the guard then went in at the one place both sinks already share ([`tar_entry_refusal`]),
//   which is why this closed as five lines rather than as the `unpack` reimplementation CPE-1744 priced.
// - **one-shot zip did not follow either — it aborted the whole extraction. FIXED by CPE-1759 (row 23),
//   in the skip direction.** That branch handed its entry loop to `zip::ZipArchive::extract`; it now runs
//   [`extract_zip_archive_stream`], the same loop the streamed path uses. The decision, and the three
//   reasons behind it, are on [`extract_archive`]. The one worth repeating here, because it corrects a
//   claim this comment used to make:
//
//   **"nothing is written outside; the user gets a clear error and can retry into an empty folder" was
//   false**, and it survived two tickets because every measurement behind it used an archive poisoned at
//   entry 0. `zip-2.4.2`'s `extract_internal` (`src/read.rs:897`) is a plain `for` loop with `?`, so the
//   refusal fires mid-loop. Re-measured with the poison second of three:
//
//   ```text
//   [M1] outcome                          = Err("invalid Zip archive: Invalid symlink target path")
//   [M1] a.txt (BEFORE the poison) exists = true
//   [M1] c.txt (AFTER  the poison) exists = false
//   ```
//
//   A half-extraction *and* an error naming neither half. Abort was never the atomic option, so the
//   choice was between "partial, with an error" and "complete-but-one, with a refusal".
//
//   **And the downgrade CPE-1744 priced the merge at is gone rather than accepted.** Its objection was
//   real — the crate's `extract` restores unix permission bits and materialises symlink entries, and our
//   loop did neither, measured here as `[M4] good_link is symlink = Ok(false) content = Ok("ok.txt")`,
//   a *legitimate* internal link arriving as a file containing its own target's name, on the shipping
//   streamed path. Both capabilities now live in [`extract_zip_archive_stream`], so the merge moved them
//   **up** to the streamed path instead of down from the one-shot one.
// - **7z followed, and it was the live one — FIXED by CPE-1746 (rows 19–20).** `extract_archive_streamed`
//   routes `.7z` to `extract_7z_stream`, which is what `start_archive_extract` calls, so the figures above
//   (`Ok`, `errors: []`, victim reading `"ARCHIVED A"`) were what the shipping UI did. Both 7z callbacks
//   now run the rows 15–16 decision before handing the entry to `default_entry_extract_fn`, so the same
//   input gives ZIP's answer: the entry is skipped (recorded in `errors` on the streamed path), the link
//   and its target are untouched, and the rest of the archive still extracts. Re-measured after the fix:
//
//   ```text
//   [7z ONE-SHOT extract_archive]      outcome = Ok(..)   victim = "VICTIM ORIGINAL"   b.txt = "ARCHIVED B"
//   [7z STREAMED extract_archive_..]   outcome = Ok(ArchiveReport { done: 1, errors: ["a.txt: \"…\" is a
//                                                link, and creating a file at a link's name writes …"] })
//                                      victim = "VICTIM ORIGINAL"   slot is link = true
//   ```
//
// **All four behaviours above are pinned by tests**, one per extractor —
// `rows_21_and_22_tar_refuse_a_link_at_an_entry_name_and_still_extract_the_rest`,
// `one_shot_and_streamed_zip_answer_a_link_at_an_entry_name_identically` and
// `rows_19_and_20_sevenz_refuse_a_link_at_an_entry_name_and_still_extract_the_rest` — and **all three
// are re-pointed characterization tests that pinned the hazard first**. Each was written to go red the
// moment its guard landed, each did, and each now asserts the refusal instead of the defect. That is the
// whole argument for pinning behaviour you consider wrong, run three times: the sentence these
// paragraphs replaced was prose, unpinned, and wrong in two of the three cases it described, and it
// survived four commits into the user-facing docs.
//
// An earlier round of this comment declined to pin them, on the grounds that pinning behaviour we
// consider wrong makes it harder to change. That argument does not survive what happened here: the
// sentence these paragraphs replaced was **prose, unpinned, and wrong in two of the three cases it
// described**, and it survived four commits and reached the user-facing docs. Prose is not cheaper to
// keep true — it is merely cheaper to leave false. So each behaviour is asserted with a message that
// names the ticket allowed to change it (CPE-1759 for tar and the ZIP divergence, CPE-1746 for 7z) and
// says which other places must move in the same commit. A characterization test does not endorse the
// behaviour; it makes the fix announce itself instead of drifting away from four descriptions of it.
// (CPE-1744 re-aimed those two tickets: tar and the ZIP divergence are **CPE-1759**, the
// `entry_name_is_safe`/`is_safe_name` delta is **CPE-1758** (closed). CPE-1744 itself closed the
// containment gap and the two wording defects, so a test still naming it would name a closed ticket.)
// CPE-1718 established that an unrecorded absence is indistinguishable from an overlooked one; the
// PR #906 review added that a recorded absence with no ticket is one nobody is scheduled to fix; this
// round adds that an unpinned description is one nobody is scheduled to keep true.
//
// ## Why the guard on rows 6–14 is the **link** half only
//
// `fsutil::create_slot_link_refusal`, not `create_slot_refusal`. Overwriting an existing archive at a
// destination the caller named is a legitimate, long-standing behaviour of these functions (and the app's
// own compress flow picks its own non-colliding name upstream in `App.svelte`), so refusing on occupancy
// would be a contract change smuggled in as a link fix. The link hazard was measured on its own, so it is
// guarded on its own. See `create_slot_link_refusal`'s doc for the full argument.

/// Monotonic counter making each single-entry extraction land in its own temp subdir. Without this,
/// two concurrent extractions of same-named entries (e.g. two `a.txt`) shared one flat
/// `cpe-archive/<base>` path and raced — one call would read a file another had already replaced or
/// removed. That made `extract_archive_entry_any_delegates_zip_to_the_zip_extractor` flaky and it
/// failed deterministically on the macOS CI leg (CPE-1195); it's also a real app hazard for two
/// concurrent extract-and-opens of same-named files.
///
/// **CPE-1786 moved what this counts under.** It used to number directories directly under the shared
/// `%TEMP%/cpe-archive` root (`<pid>-<seq>`), a namespace shared with every other process that has ever
/// run and never cleaned — so a fresh process with a recycled PID could walk over a previous run's whole
/// range and exhaust [`TEMP_TARGET_ATTEMPTS`]. It now numbers directories inside **this process's own
/// session root** (`e<seq>`, see [`session_root`]), which was created exclusively moments ago and which
/// nothing else numbers into. A monotonic counter inside a private directory cannot collide with itself,
/// so the namespace cannot exhaust; see [`temp_extract_target`].
///
/// **Process-global, and correctly so** — the `fetch_add` + exclusive `create_dir` walk in
/// [`temp_extract_target_in`] is built for exactly that. What it is *not* is predictable: a test that
/// reads this counter and then acts on the value it read is racing every sibling test that extracts
/// anything, which is CPE-1927. Such a test takes its own namespace instead; see [`ExtractNamespace`].
static EXTRACT_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Refuse before creating a file at `dest` when a **link** already occupies that name (CPE-1733) — the
/// one call every guarded row in the table above makes, so the sites stay one line each and a `grep` for
/// this name is the inventory of what is guarded.
///
/// It is a thin `Result` adapter over [`crate::fsutil::create_slot_link_refusal`] and adds no decision of
/// its own: the classification, the wording, and the "not provably a non-link ⇒ do not write" failure
/// policy all live there, shared with the rename/create family. Deliberately **not** the occupancy half —
/// see the section comment above for why overwriting an existing archive stays legal here.
fn refuse_link_at_new_file(dest: &Path) -> Result<(), String> {
    match crate::fsutil::create_slot_link_refusal(dest) {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// What a per-entry extraction loop (rows 15–16) should do about the slot it is about to write.
///
/// Rows 6–14 refuse the whole operation, so [`refuse_link_at_new_file`]'s flattened `Result` is right for
/// them. Rows 15–16 *skip and keep going*, and the UAT for this ticket caught them treating the two
/// refusal reasons as one: `if refuse_link_at_new_file(&out).is_err() { continue; }` dropped an entry
/// **silently and returned `Ok`** when the slot merely could not be *read* — an I/O failure reported as a
/// successful extraction with a file quietly missing.
///
/// # THE RULE (CPE-1935) — scope decides who dies, severity decides how it reads
///
/// This enum used to have three arms and carry two questions in two: `Skip` meant *"a verdict about one
/// entry"* **and** *"the run continues"*; `Abort` meant *"an I/O failure"* **and** *"the run stops"*.
/// Bundling them made every I/O failure a whole-run failure, which is CPE-1935: a read-only file or a
/// plain directory sitting at one entry's name took the other 26 entries of a 27-entry archive down with
/// it and left the 23 already on disk unrecorded. Measured before the split, six legs, two occupants,
/// Windows **and** real ext4 — identical everywhere (`a.txt` written, `zc.txt` never written, one
/// sentence naming only `blocked.txt`; see `cpe1935_a_blocked_entry_never_takes_the_run_down`).
///
/// The two questions are now separate and asked in this order:
///
/// 1. **Scope — what is this evidence ABOUT?** If it is about the **one name the archive asked for**,
///    it is an *entry* verdict and the run carries on. If it is about the **extraction folder, or a
///    path component more than one entry travels through, or the archive container itself**, it is a
///    *run* verdict and the run stops. An entry is the archive's business; the destination and the
///    container are the run's.
/// 2. **Severity — did anyone CHOOSE not to write?** A guard saying *"this entry would escape the
///    folder"* chose ([`Skip`](Self::Skip), counted in [`ArchiveReport::skipped`]). The filesystem
///    saying *"you cannot write here"* chose nothing ([`Fail`](Self::Fail), counted in
///    [`ArchiveReport::failed`]) — the user asked for a file and did not get it, which must never read
///    as a policy decision, and never as a success.
///
/// **How this reconciles with CPE-1938's `Abort` arm rather than reversing it.** That ticket's Security
/// Auditor flagged the same shape one door along — a transient `ENOENT` on a directory *component*
/// promoted from a per-entry skip to total denial — and it was kept as an abort, argued at the site.
/// Under the rule above that arm is **correct and unchanged**: a directory component is shared by every
/// entry beneath it, and `create_dir_beneath` *creates* missing components, so a refusal there is
/// evidence about the destination being mutated underneath a run in progress, not about one entry. The
/// two positions were never in conflict; they were being told apart by *severity*, which cannot
/// distinguish them, instead of by *scope*, which can. See [`entry_component_action`].
///
/// The one thing that is NOT reclassified: a **containment** refusal. "This entry would land outside the
/// folder", "this name is a link", "this leaf has other names" are verdicts and stay
/// [`Skip`](Self::Skip). Collapsing them into `Fail` would relabel a successful defence as a malfunction.
#[derive(Debug, PartialEq, Eq)]
enum EntrySlotAction {
    /// The slot is provably not a link: write it.
    Write,
    /// A confirmed link. Policy skip — carry on with the rest of the archive, recording the reason where
    /// the caller has somewhere to put it.
    Skip(String),
    /// **The entry could not be delivered and nobody chose that** (CPE-1935) — an unwritable occupant, a
    /// directory in the leaf's way, a slot whose safety could not be established. Scope is one entry, so
    /// the run continues; severity is a failure, so it lands in [`ArchiveReport::failed`] and never in
    /// `skipped`. Nothing is written for it either way — this arm changes what happens to the *other*
    /// entries, never what happens to this one.
    Fail(EntryFailure),
    /// The **run** cannot go on: the extraction folder, a shared path component, or the archive
    /// container itself. Not one entry's problem, so recording it per entry would be a lie about scope.
    Abort(String),
}

/// One entry that could not be delivered, plus **whether extracting again here could come out
/// differently** (CPE-1935).
///
/// The retryable answer is carried **from the point of refusal**, never re-derived later from the
/// sentence — the identical shape, and for the identical reason, as
/// [`crate::revert_engine::Refused`]: the site that met the filesystem knows, and everywhere else is
/// guessing. CPE-1845 shipped that guess once (a containment verdict labelled "temporary — run the
/// revert again") and this module's standing rule against pattern-matching refusal wording
/// ([`crate::open_beneath::Refusal`]) is the same lesson.
#[derive(Debug, PartialEq, Eq)]
struct EntryFailure {
    /// The sentence naming what happened at this entry. The next-step clause is **not** part of it —
    /// [`ArchiveReport::fail`] appends that from `retryable`, so the two cannot drift.
    why: String,
    /// `true` when the cause is something the user can clear and re-run into (a read-only file, a
    /// directory in the way, a lock, a permission). `false` when re-running reaches the same answer (a
    /// malformed or truncated archive).
    retryable: bool,
}

impl EntryFailure {
    /// The filesystem refused this one name for a reason the user can act on, then extract again.
    fn retryable(why: impl Into<String>) -> Self {
        Self { why: why.into(), retryable: true }
    }

    /// Classify a write failure by [`std::io::ErrorKind`] — **a structured field, not the message
    /// text.** The three kinds below are the ones that mean *the bytes we were handed are wrong*
    /// (a truncated member, a corrupt deflate stream, a name this filesystem will not accept); every
    /// other kind is the destination saying no, which is the case the user can fix and re-run.
    ///
    /// Erring toward `retryable` is deliberate: telling someone "try again" when it will not help costs
    /// them one re-run, while telling them "this will never work" about a file they could have unlocked
    /// costs them the file.
    ///
    /// **CPE-1929 pair, run on this predicate (Windows `--lib`, `Compiling cpe-server` confirmed on
    /// every run):**
    ///
    /// ```text
    /// baseline                                   2434 passed /  0 failed
    /// A  disable  (`true || !matches!(..)`)       2434 passed /  2 failed
    /// B  lie      (`matches!(..)`, inverted)      2433 passed /  3 failed
    /// ```
    ///
    /// A's *first* run — before `cpe1935_a_write_failure_says_whether_re_running_helps` and
    /// `cpe1935_a_corrupt_entry_fails_permanently_while_its_neighbours_land` existed — came back **2434
    /// passed / 0 failed**, i.e. the classifier was unreachable from any assertion and would have
    /// shipped reading as covered. Those two tests are what the pair bought.
    fn from_write_error(why: impl Into<String>, e: &std::io::Error) -> Self {
        let retryable = !matches!(
            e.kind(),
            std::io::ErrorKind::InvalidData
                | std::io::ErrorKind::UnexpectedEof
                | std::io::ErrorKind::InvalidInput
        );
        Self { why: why.into(), retryable }
    }
}

/// The next-step clause [`ArchiveReport::fail`] appends to a retryable entry failure. Says what the user
/// can do **and** that the rest of the archive is already on disk — the sentence CPE-1935 was filed for
/// the absence of, since re-running was the only recourse and nothing said so.
const RETRY_HELPS: &str =
    "The rest of the archive was extracted; clear that and extract again to get this entry too.";
/// The next-step clause for a failure re-running cannot change — see [`EntryFailure::from_write_error`].
const RETRY_DOES_NOT_HELP: &str =
    "The rest of the archive was extracted; extracting again will not change this entry.";

/// Join a failure's own sentence to its next-step clause with a real sentence break.
///
/// [`EntryFailure::why`] arrives from wherever the write failed and only *some* of those sources end in
/// punctuation: our own wrappers do not (``failed to unpack `…\blocked.txt` ``), and a bare OS string
/// ends in its code (`Access is denied. (os error 5)`). Round 1 concatenated with a plain space, so both
/// of those reached the operations panel as two sentences run together —
/// ``…\blocked.txt` The rest of the archive was extracted…`` — which reads as one broken sentence rather
/// than as advice. The full stop is added only when the text has not already ended itself, so a message
/// that was already a sentence is untouched.
fn join_failure_sentence(why: &str, next: &str) -> String {
    let why = why.trim_end();
    if why.is_empty() {
        return next.to_string();
    }
    if why.ends_with(['.', '!', '?', ':', ';']) {
        format!("{why} {next}")
    } else {
        format!("{why}. {next}")
    }
}

/// The pure decision behind [`EntrySlotAction`], split from the filesystem probe for the reason
/// `fsutil`'s classifiers are: **the `Unknown` arm cannot be staged on every platform** (it needs a slot
/// whose `symlink_metadata` fails with something other than `NotFound`), so with the mapping inline the
/// one arm this ticket got wrong would again be the one arm no test could reach.
///
/// **CPE-1935 moved the `Unknown` arm from [`EntrySlotAction::Abort`] to [`EntrySlotAction::Fail`], and
/// that is not a weakening of the gate.** Nothing is written for the entry either way — the refusal is
/// byte-for-byte the same sentence and the same non-write. What changed is the *other* entries: an
/// unreadable slot at one name used to deny the whole archive, which is precisely the "one planted
/// object = total denial" amplifier CPE-1938's Security Auditor named one door along, reachable here by
/// anything that can make a single leaf unstattable. Under [`EntrySlotAction`]'s rule the evidence is
/// about **one name**, so its scope is one entry; its severity is a failure, not a policy skip, so it
/// lands in [`ArchiveReport::failed`] where it cannot be mistaken for a guard's decision. The property
/// the two tests here have always defended — *a gate that cannot tell must not write* — is untouched.
fn entry_slot_action(verdict: crate::fsutil::CreateSlotLink) -> EntrySlotAction {
    match verdict {
        crate::fsutil::CreateSlotLink::NotALink => EntrySlotAction::Write,
        crate::fsutil::CreateSlotLink::Link(m) => EntrySlotAction::Skip(m),
        crate::fsutil::CreateSlotLink::Unknown(m) => EntrySlotAction::Fail(EntryFailure::retryable(m)),
    }
}

/// The refusal for an entry whose destination does not provably stay under the extraction folder
/// (CPE-1744) — the message half of [`entry_sink_action`]/[`entry_dir_action`], in one place so the
/// wording cannot drift between the five sinks that produce it.
///
/// **It deliberately does not claim to know *why*.** [`crate::fsutil::confined_to`] fails closed: it
/// answers `false` both for "a component resolved outside `dest`" and for "this could not be resolved at
/// all" (`EACCES`, `ELOOP`, a Windows sharing violation, an unresolvable root). Naming only the first
/// would be the same one-step-past-the-evidence move this module's section comment is a monument to, so
/// the sentence covers both and the outcome — skip, do not write — is the same either way.
fn escaped_dest_message(dest: &Path, out: &Path) -> String {
    format!(
        "\"{}\" could not be shown to stay inside the extraction folder \"{}\" — either a folder on the \
         way there is a link (a symlink or, on Windows, a junction, which needs no privilege) and the \
         entry's bytes would land somewhere you did not choose, or the path could not be resolved at all. \
         Skipped; the rest of the archive still extracts",
        out.display(),
        dest.display()
    )
}

/// **The whole per-entry destination decision for rows 15/16/19/20 — and the end of their LEAF-ONLY
/// caveat** (CPE-1744).
///
/// Two questions, asked in this order and *not* interchangeable:
///
/// 1. **Is the final name itself a link?** [`entry_slot_action`], unchanged — CPE-1733's guard, whose
///    wording rows 15/16/19/20's tests pin verbatim (`"is a link"` / `"writes THROUGH it"`).
/// 2. **Does the whole path still resolve inside `dest` once every *intermediate* component is
///    followed?** [`crate::fsutil::confined_to`], which is the guard CPE-1733 measured the absence of and
///    scoped out as "a different guard".
///
/// **The order is load-bearing.** A link *at the leaf* pointing outside `dest` fails question 2 as well,
/// so asking containment first would relabel every row-15/16/19/20 refusal as a containment refusal and
/// silently retire the link wording those four tests assert. Leaf first keeps each hazard reported as
/// itself, which is also what makes a mutation of either guard turn a *distinct* test red.
///
/// # Why this was not just `guarded_join`
///
/// CPE-1744's checklist asked whether [`crate::transfer::guarded_join`] could be adopted wholesale here.
/// It cannot, for this half: `guarded_join` is a **lexical + per-segment-name** guard (it splits the
/// relative path, applies `is_safe_name` to each segment, sanitises on Windows, and rejoins). It never
/// touches the filesystem, so it cannot see a *live directory link* at `dest/sub` — the escape shape
/// measured here, which needs neither `..` nor an absolute path and is invisible to any textual check.
/// `confined_to` is the filesystem-resolving half and is what this needs. The *other* half of
/// `guarded_join` — the per-segment name rules (`:` anywhere, a leading `..`, the Windows
/// reserved-device/trailing-dot shapes) — is a separate guard with a separate blast radius, now adopted
/// at [`entry_name_is_safe`] itself (CPE-1758); see `entry_name_is_safe_now_agrees_with_transfers_is_safe_name`.
///
/// # Cost
///
/// One `canonicalize` per entry in the common case (two when the leaf's parent does not exist yet). That
/// is a syscall against a decompress-and-write per entry, so it does not move an extraction's cost; it is
/// stated rather than measured-and-ignored because "per entry" is the kind of phrase that hides a
/// quadratic.
fn entry_sink_action(dest: &Path, out: &Path) -> EntrySlotAction {
    match entry_slot_action(crate::fsutil::create_slot_link_verdict(out)) {
        EntrySlotAction::Write => {}
        decided => return decided,
    }
    if !crate::fsutil::confined_to(out, dest) {
        return EntrySlotAction::Skip(escaped_dest_message(dest, out));
    }
    // **CPE-1857 — the third question, and the only one no path can answer.** The two above ask what
    // this NAME is and where it resolves; both pass a hard link, and both are *right* to — a hard link
    // is not a reparse point, has no target, and `canonicalize` resolves it to itself, so the name
    // really is inside `dest`. The bytes still come out at the object's other name, which may be
    // anywhere. The entry's name is archive-controlled, so this is the untrusted half of the same
    // shape CPE-1857 measured on a checkpoint manifest.
    //
    // A **Skip**, not an Abort, and for [`EntrySlotAction`]'s stated reason: this is a policy verdict
    // about one entry, the rest of the archive still extracts, and the caller records the reason where
    // it has somewhere to put it.
    //
    // **What it costs:** one extra name probe per *file* entry — a `symlink_metadata` on Unix, a
    // `CreateFileW` with `FILE_READ_ATTRIBUTES` on Windows (see `batch_media::name_is_multiply_linked`).
    // Both are attribute-only calls, against a loop that is already doing a `File::create` plus an
    // `io::copy` of the entry's whole payload for every entry it accepts.
    //
    // **CPE-1857 Security-Auditor finding 1: this is a GATE, so `Unknown` must refuse.** The `bool` form
    // of this question folds "could not tell" into "no", which is right at the revert engine's refusal
    // *classifier* (the write is already settled there) and fails **open** here, where the bytes have
    // not moved yet. `Unknown` therefore refuses, on exactly the terms `entry_slot_action`'s own
    // `Unknown` arm refuses an unreadable link verdict — same condition, same answer. **CPE-1935 changed
    // only its blast radius**, from the whole archive to this entry (`Fail`, not `Abort`);
    // `cpe1935_an_unreadable_slot_is_a_recorded_entry_failure_on_both_tar_paths` pins the shape.
    match crate::batch_media::name_links(out) {
        crate::batch_media::NameLinks::Many(names) => {
            return EntrySlotAction::Skip(multiply_linked_message(out, names));
        }
        crate::batch_media::NameLinks::Unknown(why) => {
            // CPE-1935: `Fail`, not `Abort` — same non-write, same sentence, one entry instead of the
            // whole archive. See `entry_slot_action`'s doc for why that is a scope correction and not a
            // relaxation of the gate.
            return EntrySlotAction::Fail(EntryFailure::retryable(format!(
                "could not check how many names \"{}\" has, so nothing was written for it — refusing \
                 to guess rather than risk writing through a hard link into a file outside this \
                 folder: {why}",
                out.display()
            )));
        }
        crate::batch_media::NameLinks::One | crate::batch_media::NameLinks::NoFileHere => {}
    }
    EntrySlotAction::Write
}

/// The refusal wording for a slot that is a **hard link** — a second name for a file that may live
/// anywhere (CPE-1857), in one place so the sinks that produce it cannot drift apart.
///
/// It does **not** try to name the other file, because it cannot: there is no way to walk from an inode
/// back to its names without scanning a filesystem. So it names what the user can act on — this path,
/// and the fact that it is shared.
fn multiply_linked_message(out: &Path, names: u64) -> String {
    format!(
        "\"{}\" has {names} names (it is a hard link) — the others may live anywhere, including outside \
         the extraction folder, which no path check can see because a hard link resolves to itself. \
         Writing this entry would change that file's content too. Skipped; the rest of the archive \
         still extracts",
        out.display()
    )
}

/// The **directory**-entry half of the same decision — row 18 (CPE-1744).
///
/// Deliberately only question 2. A link sitting at a *directory* entry's own name is
/// `create_dir_all` **redirection**, not destruction (CPE-1729), and redirection only costs the user
/// something when it redirects **out of `dest`** — which is exactly what [`crate::fsutil::confined_to`]
/// answers. A directory link that leads to another folder *inside* the extraction root writes the
/// entries where the user asked, so refusing it would be a wider claim than the hazard.
///
/// This is what stops `create_dir_all` from creating the *deeper* directories of an escaping entry
/// (`dest/link/a/b`) out beyond the destination, which is why both ZIP loops now ask it **before** the
/// `create_dir_all`, not after.
fn entry_dir_action(dest: &Path, out: &Path) -> EntrySlotAction {
    if crate::fsutil::confined_to(out, dest) {
        EntrySlotAction::Write
    } else {
        EntrySlotAction::Skip(escaped_dest_message(dest, out))
    }
}

/// The refusal wording for a **link entry whose target leaves the extraction folder** (CPE-1774) — the
/// message half of [`link_target_action`], in one place so the two sinks that produce it cannot drift.
///
/// It names the target, because that is the whole payload of this attack and the user has no other way
/// to see it: the entry's *name* is perfectly ordinary (`evil_link`), and after a successful extraction
/// the link looks like an ordinary file in the pane.
fn escaping_link_target_message(dest: &Path, target: &Path) -> String {
    format!(
        "this entry is a link pointing at \"{}\", which is outside the extraction folder \"{}\" — \
         extracting it would put a shortcut in your folder that silently reads and writes a file you \
         never chose. Skipped; the rest of the archive still extracts",
        target.display(),
        dest.display()
    )
}

/// **The link-TARGET decision** (CPE-1774) — the guard none of rows 1–20 had, because every one of them
/// asks about a *destination path* and this attack's payload is the link's *content*.
///
/// A `.zip` and a `.tar` can both carry an entry flagged as a symlink whose stored bytes are the link's
/// target. `entry_name_is_safe` passes it — the name is an ordinary one — and then the crate-native
/// extractor materialises a **real OS symlink** with that raw target and no check of any kind.
/// Reproduced on this branch before the fix, one zip and one tar, entry named `evil_link`:
///
/// ```text
/// [M2 abs zip ONE-SHOT ] symlink_metadata(is_symlink) = Ok(true)
///                        read_link  = Ok("…\\cpe_measure_m2\\outside_secret.txt")
///                        read_to_string(THROUGH the link) = Ok("SECRET")
/// [M3 abs tar ONE-SHOT ] same three lines
/// [M3 abs tar STREAMED ] same three lines   <- the SHIPPING path (start_archive_extract)
/// ```
///
/// # The policy, and why this one
///
/// The ticket offered three: refuse every link entry, refuse only escaping ones, or write the target as
/// ordinary text. **Refuse only the escaping ones.** Refusing all of them would break archives that
/// legitimately carry internal links (source tarballs routinely do, and the `inside` leg of the same
/// measurement shows one working today); writing the target as text is what the *streamed zip* loops
/// already do by accident and it silently turns a link into a file with confusing contents. Refusing
/// only the escape keeps a valid archive valid and costs the attacker the whole attack.
///
/// # Resolution — against the destination, never the literal string
///
/// The target is resolved against the **link's own parent directory** (a relative target is interpreted
/// from where the link sits, which is what the OS does), then handed to [`crate::fsutil::confined_to`],
/// the same filesystem-resolving guard rows 15–20 use. That is what makes `x/../..`, an absolute target,
/// and a link-to-a-link chain all answer correctly rather than only literal `..`:
///
/// - **`x/../..`** — `confined_to` canonicalises, so the `..`s collapse and the verdict is about where
///   the path actually lands, not how it is spelled.
/// - **absolute** — `Path::join` on an absolute (or, on Windows, a rooted) target replaces the base, so
///   the candidate *is* the absolute path and `starts_with(dest)` answers it.
/// - **a chain** — `confined_to` follows a link it meets on the way, including a dangling one, so
///   `evil -> inner` where `inner` is a pre-existing link out of `dest` is refused at `evil`.
/// - **not-yet-created targets** — a link to a file later in the same archive resolves to nothing yet;
///   `confined_to` walks up to the nearest existing ancestor, which is `dest`, and allows it. That is
///   why an ordinary internal link still extracts.
///
/// **Backslashes are normalised to `/` on every platform**, matching [`entry_name_is_safe`]. On Windows
/// that is a no-op (both are separators already). On POSIX it is deliberately *over-broad*: a target
/// literally named `..\secret` is a legal single filename there and would be harmless, and this refuses
/// it. The trade is one-directional — a POSIX user loses a pathological filename, and a Windows-authored
/// archive cannot smuggle a traversal past a POSIX check by spelling it with the other separator.
///
/// **That over-broadness is a real, accepted false refusal, recorded rather than hand-waved**: extracting
/// a POSIX archive whose symlink target is literally `..\secret` — legal and harmless there — now skips
/// that entry and says so. It is a false *refusal*, never a false permit, and the user sees it (CPE-1775),
/// which is the only reason it is an acceptable price. **Note the asymmetry with the entry NAME**, which
/// this function must never normalise: over-broad on the target fails safe, over-broad on the name fails
/// open, and the review that caught the name half is written up above.
///
/// **The `is_symlink()` note for Windows:** this guard runs on the *archive entry's* declared type, not
/// on anything already on disk, so the junction-vs-symlink difference that
/// `symlink_metadata().is_symlink()` papers over elsewhere in this codebase does not arise here. What
/// each extractor then creates is the crate's business — the zip crate picks `symlink_dir`/`symlink_file`
/// on Windows, `tar` calls `symlink`; neither creates a junction. `confined_to` is what covers a junction
/// **already sitting in `dest`**, and it covers it because it canonicalises rather than inspecting a
/// file type.
/// # `out` MUST be the path the extractor will actually create the link at
///
/// This function derives the target's containment base from `out.parent()`, so `out` decides **how deep
/// the guard believes the link sits**, and every level of disagreement with reality is worth one extra
/// `..` of real escape. The first version of this code passed
/// `dest.join(name.replace('\\', "/"))` at both call sites, and the reviewer measured the hole that
/// opened on POSIX:
///
/// ```text
/// tar_entry_refusal("a\\b\\evil" -> "../../x")   = None                    <- ALLOWED
/// tar_entry_refusal("evil"       -> "../../x")   = Some("...outside...")   <- refused
/// ```
///
/// Same target, same real location, opposite verdicts — because on Unix `Path::new("a\\b\\evil")` is
/// **one** `Component::Normal`. `tar-0.4.46`'s `unpack_in` builds `file_dst` from
/// `self.path()?.components()` and `zip-2.4.2`'s `simplified_components` does the same, so both write the
/// link at `<dest>/a\b\evil` — directly in `dest` — while the pre-normalised `out` told this function it
/// was two directories down. An attacker adds fake components for arbitrary depth. End to end through
/// `start_archive_extract`: real `a/` and `a/b/` directory entries (so `confined_to` resolves rather than
/// failing closed), then a symlink named `a\b\evil` targeting `../../etc/passwd`.
///
/// Note the asymmetry, because it decides which side the fix belongs on: normalising `\` to `/` in the
/// **target** below is *over*-broad and therefore fails safe; normalising it in the **name** is
/// *under*-broad and was the hole. So the callers now build `out` exactly as their own extractor does —
/// `dest.join(name)`, computed inline in the loop that owns the write, for both tar (`tar_entry_refusal`)
/// and zip ([`extract_zip_archive_stream`]) — and this function no longer touches the name at all. (The
/// zip side used to be a separate pre-pass function, `zip_entry_out`; CPE-1773/1774/1775 folded it into
/// the loop, which is also why every zip extractor, [`extract_zip_encrypted`] included since CPE-1807,
/// gets this guard the same way.)
fn link_target_action(dest: &Path, out: &Path, target: &Path) -> EntrySlotAction {
    let normalized = target.to_string_lossy().replace('\\', "/");
    let base = out.parent().unwrap_or(dest);
    let candidate = base.join(&normalized);
    if crate::fsutil::confined_to(&candidate, dest) {
        EntrySlotAction::Write
    } else {
        EntrySlotAction::Skip(escaping_link_target_message(dest, target))
    }
}

/// Materialise the OS symlink a ZIP symlink entry asks for (CPE-1759).
///
/// This is the capability [`extract_zip_archive_stream`] was missing, and the reason CPE-1744 recorded
/// that routing the one-shot path through that loop would "silently downgrade the more capable path".
/// Measured on this branch before the fix, a zip carrying a **legitimate internal** link entry
/// `good_link -> ok.txt`, extracted through the streamed loop:
///
/// ```text
/// [M4] good_link is symlink = Ok(false)   content = Ok("ok.txt")
/// ```
///
/// A file whose bytes are the target's *name* — not a link, and not the archive's content either. So the
/// downgrade was real, and it was already shipping: this loop is what `start_archive_extract` uses. It is
/// fixed here rather than worked around, which is also what lets [`extract_archive`]'s zip branch adopt
/// the same loop without losing anything.
///
/// **Deliberately mirrors `zip-2.4.2`'s `read::make_symlink` on the two platforms that have links**, with
/// one correction: for the `symlink_dir`/`symlink_file` choice Windows needs, the crate probes
/// `fs::metadata(target)` on the **raw, still-relative** target, which resolves against the *process's*
/// working directory rather than the link's own. This resolves it against `out.parent()`, which is where
/// the OS will resolve it from. Getting it wrong is not a safety question (containment is
/// [`link_target_action`]'s, already answered before this is called) — it picks the wrong link *flavour*,
/// which on Windows means a directory link that does not behave like one.
///
/// **A platform that *categorically* will not create links is a SKIP; everything else is a FAILURE and
/// aborts** — [`materialise_entry_symlink`] draws that line, and the first version of CPE-1759 did not
/// draw it at all (it swallowed every `io::ErrorKind` into a refusal that asserted the cause was the
/// Windows privilege, while `File::create` twelve lines away aborted on the same errors). The
/// `not(any(unix, windows))` arm returns `Unsupported` rather than copying the crate's
/// write-the-target-as-a-file fallback, because that fallback is precisely the defect measured above; no
/// CI leg compiles it (the matrix is Linux/macOS/Windows).
#[cfg(unix)]
fn create_entry_symlink(out: &Path, target: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, out)
}

#[cfg(windows)]
fn create_entry_symlink(out: &Path, target: &Path) -> std::io::Result<()> {
    let probe = if target.is_absolute() {
        target.to_path_buf()
    } else {
        out.parent().unwrap_or(Path::new(".")).join(target)
    };
    if fs::metadata(&probe).map(|m| m.is_dir()).unwrap_or(false) {
        std::os::windows::fs::symlink_dir(target, out)
    } else {
        std::os::windows::fs::symlink_file(target, out)
    }
}

#[cfg(not(any(unix, windows)))]
fn create_entry_symlink(_out: &Path, _target: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "this platform has no symbolic links",
    ))
}

/// Windows `ERROR_PRIVILEGE_NOT_HELD` — `symlink_file`/`symlink_dir` without
/// `SeCreateSymbolicLinkPrivilege` (administrator, or Developer Mode).
///
/// # Why the raw code, and NOT the reason review round 2 gave
///
/// That round justified raw-code matching by claiming Rust decodes 1314 and `ERROR_ACCESS_DENIED` (5)
/// to the *same* `PermissionDenied`, so only the code could separate them. **That was false, and it was
/// measured false on the pinned toolchain** (`rustc 1.97.0`, msvc):
///
/// ```text
/// [K] raw     1 -> Uncategorized      [K] raw   120 -> Unsupported
/// [K] raw     5 -> PermissionDenied   [K] raw   183 -> AlreadyExists
/// [K] raw    50 -> Uncategorized      [K] raw  1314 -> Uncategorized
/// ```
///
/// The real reason is **stronger**: 1314 decodes to `ErrorKind::Uncategorized`, which is **unstable and
/// unnameable** — `ErrorKind` is `#[non_exhaustive]` and that variant has no stable path — so a
/// kind-based match cannot express this case *at all*, not merely less precisely. Raw-code matching is
/// the only construction that compiles, never mind the only one that is correct.
///
/// **Worth recording how the false version survived its own red-proof.** The mutation (match on
/// `ErrorKind::PermissionDenied`) did go red — but for a different reason than the model predicted:
/// under it the *1314* assertion fails because 1314 is not `PermissionDenied`, not because 5 collides
/// with it. A test going red confirms the code changed behaviour; it does not confirm the story about
/// *why*. That is the same shape as the "abort leaves nothing partial" premise this very ticket
/// demolished — inherited, plausible, and never measured — reproduced by its own author two commits
/// later.
///
/// **And the red count from that mutation is platform-conditional**, which is worth stating because
/// the number was quoted flat: on Windows it reds three tests, on Linux and macOS exactly one — the
/// `EACCES` leg of `cpe1759_link_creation_separates_a_categorical_refusal_from_a_failure`. Everything
/// above about 1314 compiles nowhere but Windows.
#[cfg(windows)]
const ERROR_PRIVILEGE_NOT_HELD: i32 = 1314;

/// The Windows codes that mean **this volume has no symbolic links**, whoever you are.
///
/// `ERROR_NOT_SUPPORTED` (50) and `ERROR_INVALID_FUNCTION` (1) are what a FAT/exFAT volume or a network
/// redirector answers; `ERROR_CALL_NOT_IMPLEMENTED` (120) is the one that already decodes to
/// `ErrorKind::Unsupported` and is listed anyway so the set reads as one thing rather than two.
///
/// **Measured for their `ErrorKind`, not for their occurrence.** 50 and 1 decode to `Uncategorized` (see
/// [`ERROR_PRIVILEGE_NOT_HELD`]), which is why they need naming here at all. That these are the codes
/// Windows *emits* for a link-less volume comes from the platform documentation, not from a runner: no
/// CI leg can mount a FAT volume.
///
/// # The two ways this list can be wrong are NOT symmetric
///
/// An earlier version of this comment said being wrong "costs a refusal that arrives as an abort — the
/// safe direction — never the reverse". That is true of one error and backwards for the other:
///
/// - **Omission** — a code that really does mean "this volume has no links" is missing. The entry is
///   refused as an abort instead of a skip: the pre-CPE-1759 behaviour, noisy, and safe.
/// - **Inclusion** — a code listed here can also arise from something that is *not* a link-less volume.
///   That files a **failure as a refusal**: `Ok`, one entry quietly absent. It is the exact defect class
///   this ticket exists to remove, arriving through the list meant to fix a different one.
///
/// **`ERROR_INVALID_FUNCTION` (1) is the entry carrying that risk**, and it is named rather than
/// averaged into the set: 50 and 120 are specific ("not supported", "not implemented"), while 1 is
/// Windows' generic "the device cannot do this" and is not exclusively a link-support answer. It is
/// kept because a FAT/network volume is documented to produce it and the alternative is the broken
/// promise round 3 fixed — but it is the one to revisit first if this list ever needs narrowing, and
/// narrowing it is a behaviour change rather than a comment fix.
#[cfg(windows)]
const WINDOWS_NO_LINK_SUPPORT: &[i32] = &[1, 50, 120];

/// POSIX `EPERM`, which `symlink(2)` documents as *"the filesystem containing linkpath does not support
/// the creation of symbolic links"* — the FAT-stick case, on the other family.
///
/// **`EACCES` (13) is the write-permission failure and is deliberately absent**: that one must be
/// reported as a failure, not dressed up as a categorical refusal.
/// The two are indistinguishable by `ErrorKind` (both `PermissionDenied`), which is the *genuine*
/// same-kind collision review round 2 mistakenly attributed to the Windows pair.
///
/// **This is only sound because the classifier sees exactly one syscall's errno.**
/// [`create_entry_symlink`]'s unix arm is a bare `std::os::unix::fs::symlink`, and
/// [`materialise_entry_symlink`] never routes a `remove_file` error here — which matters concretely:
/// `remove_file` returns `EPERM` on a sticky-bit directory such as `/tmp`, and classifying *that* as
/// "this filesystem has no links" would file a failure as a refusal, the exact defect review round 2
/// existed to remove.
///
/// **And it is load-bearing on macOS specifically, which round 3 stated too weakly** (the round-4
/// reviewer supplied this; it is from the platform's `unlink(2)`, not from a runner here — no CI leg
/// stages it). Darwin's `unlink` answers `EPERM` for *"the named file is a directory"*, where Linux
/// answers `EISDIR`. So without the direct return, the directory-occupant leg of
/// `cpe1759_a_link_entry_overwrites_an_ordinary_file_but_a_directory_is_a_failure` would flip from
/// failure to refusal **on the macOS leg alone** — the sticky-bit case is the hypothetical one, and this
/// is the one already sitting in this PR's own test matrix.
///
/// `EPERM` is 1 on Linux, macOS and every BSD (the original UNIX errno ordering); it is spelled out
/// rather than taken from `libc` because this crate has no such dependency and is not gaining one.
#[cfg(unix)]
const EPERM: i32 = 1;

/// **The refusal/failure line for link creation** (CPE-1759, review rounds 2 and 3).
///
/// `true` only for *categorical* refusals — this machine or volume does not do symbolic links at all,
/// for anyone, until something about the machine changes. Everything else is a **failure**, with the
/// same treatment `File::create` and `io::copy` get in the same loop: `EACCES` on the directory,
/// `NotFound`, a full disk, `EIO`. (That treatment was a whole-run abort until CPE-1935 and is one
/// counted entry failure now; this function decides *which side of the line* an error falls on, which
/// is unchanged.)
///
/// **Round 3 fixed a promise the code was not keeping.** Round 2 matched `ErrorKind::Unsupported` plus
/// Windows 1314 and told users, in the in-app help, that a link-less filesystem would skip the entry.
/// Measured, `Unsupported` is reachable from Windows 120 and from the
/// `not(any(unix, windows))` arm of [`create_entry_symlink`] that no CI leg compiles — while the codes a
/// real FAT volume produces (Windows 1/50, POSIX `EPERM`) all aborted. The help promised a skip the code
/// did not deliver. It is delivered now, on `WINDOWS_NO_LINK_SUPPORT` and `EPERM` — plain code text, not
/// an intra-doc link, because each name only compiles under its own platform's `cfg`: linking either one
/// from this unconditionally-compiled function resolves on the platform that has it and dangles on the
/// other (CPE-1814).
///
/// **This function itself only ever sees ZIP's syscall error directly** — [`materialise_entry_symlink`]
/// has exactly one call site, inside [`extract_zip_archive_stream`] — but it is no longer the only
/// format that answers this way. **CPE-1813 brought TAR to parity**, by translating rather than
/// duplicating: `tar`'s own `unpack_in` still makes the `symlink`/`hard_link` syscall (`entry.rs:573`
/// for symlinks, `:552` for hard links) — that ownership of the write was deliberate and stays — but
/// the two tar sinks ([`tar_unpack`], [`extract_tar_stream`]) now route its `Err` through this same
/// function via [`tar_link_creation_outcome`] instead of aborting on it with `?`. The wrinkle that made
/// this more than a five-line change is [`recover_link_syscall_error`]: `unpack_in` rewraps the
/// syscall's `io::Error`
/// twice before it reaches our code, and each wrap discards `raw_os_error()`, so the raw code this
/// function's Windows/POSIX arms key on has to be recovered from the wrapped error's rendered text
/// first — see that function's doc for the measurement.
///
/// So: before CPE-1759 the ZIP loop wrote a symlink entry's target out as *text*, and a zip carrying
/// link entries extracted onto a FAT stick without failing. Materialising real links without this arm
/// would have turned that into a dead extraction — a regression introduced by a fix, on a platform
/// combination nothing here can test. A tar with links on that same stick aborted before CPE-1759 and
/// CPE-1813 alike (the containment guard CPE-1759 added did not touch this path; only an entry whose
/// *creation itself* the volume refuses reaches here) — it now answers the same way ZIP does.
fn link_creation_is_categorical(e: &std::io::Error) -> bool {
    if e.kind() == std::io::ErrorKind::Unsupported {
        return true;
    }
    #[cfg(windows)]
    if e.raw_os_error() == Some(ERROR_PRIVILEGE_NOT_HELD)
        || e.raw_os_error().is_some_and(|c| WINDOWS_NO_LINK_SUPPORT.contains(&c))
    {
        return true;
    }
    #[cfg(unix)]
    if e.raw_os_error() == Some(EPERM) {
        return true;
    }
    false
}

/// True when the refusal is the **Windows privilege**, as opposed to a volume with no links at all.
/// The two need different things from the user, so they are worded differently by
/// [`link_creation_refusal`]; on every other platform this is `false` and only the volume wording ships.
fn link_creation_needs_privilege(e: &std::io::Error) -> bool {
    // A cfg'd `let`, not two cfg'd blocks: a bare `#[cfg] { .. }` in statement position is a *statement*,
    // not the function's tail expression, so that shape silently returns `()` and fails to compile — and
    // the `return` spelling that does work trips clippy's `needless_return`.
    #[cfg(windows)]
    let privilege = e.raw_os_error() == Some(ERROR_PRIVILEGE_NOT_HELD);
    #[cfg(not(windows))]
    let privilege = {
        let _ = e;
        false
    };
    privilege
}

/// The wording for a link entry this machine categorically will not create (CPE-1759) — in one place so
/// it cannot drift, and **it no longer guesses the cause**. The first version said "On Windows that
/// normally means…" for every error it was handed, including ones that had nothing to do with
/// privileges; the two causes [`link_creation_is_categorical`] admits are now named separately.
fn link_creation_refusal(target: &Path, e: &std::io::Error) -> String {
    let remedy = if link_creation_needs_privilege(e) {
        "creating a link needs administrator rights or Developer Mode on Windows"
    } else {
        "this folder is on a drive whose filesystem has no links"
    };
    format!(
        "this entry is a link to \"{}\", and it could not be created here — {remedy} ({e}). Skipped; \
         the rest of the archive still extracts",
        target.display()
    )
}

/// **What a link-creation error means, as a pure function**: `Ok(None)` unreachable here, `Ok(Some(m))`
/// a **refusal** (skip this entry), `Err(m)` a **failure**.
///
/// The two verdicts still mean what they always meant here; what CPE-1935 changed is what the *callers*
/// do with `Err`. It used to leave via `?` and end the run; every call site now records it against this
/// one entry ([`ArchiveReport::fail`]) and carries on. This function is unchanged and needs to be —
/// misfiling a failure as a refusal is still the defect, whatever the failure then costs.
///
/// Split out of [`materialise_entry_symlink`] in review round 3, for the reason `entry_slot_action` and
/// `fsutil`'s classifiers are split out — and the review measured the cost of not having it: mutating
/// the `Ok(Some(..))` arm to `Err(..)`, which converts every categorical refusal into an aborted
/// extraction, turned **no test red**. Neither arm can be staged on any runner (a machine with the
/// symlink privilege cannot produce 1314; no CI leg can mount a link-less volume), so with the decision
/// inline the routing that carries this ticket's whole point went unverified.
fn link_creation_outcome(target: &Path, out: &Path, e: &std::io::Error) -> Result<Option<String>, String> {
    if link_creation_is_categorical(e) {
        return Ok(Some(link_creation_refusal(target, e)));
    }
    // Names the path, unlike the bare `e.to_string()` its neighbours use: an OS string alone ("Access is
    // denied.") tells the user nothing about *which* of an archive's entries died on it.
    Err(format!("could not create the link \"{}\": {e}", out.display()))
}

/// The literal text `tar-0.4.46`'s `EntryFields::unpack` inserts immediately after a **symlink**
/// entry's own syscall `io::Error`'s `Display` (`entry.rs:573`, measured against the crate source, not
/// guessed from the block's start) — the anchor [`recover_link_syscall_error`] keys on. No other way
/// `unpack_in` can fail for a link entry produces this text (CPE-1813 review round 1, blocker 2).
const TAR_SYMLINK_MARKER: &str = " when symlinking ";

/// Same, for a **hard link** entry (`entry.rs:552`).
const TAR_HARDLINK_MARKER: &str = " when hard linking ";

/// Walk `e`'s `source()` chain — **never `e` itself**, see below — for the one level that is provably
/// `tar`'s own wrap of the `symlink`/`hard_link` syscall's `io::Error`, and reconstruct an error
/// carrying that level's real `kind()` and, where parseable, its raw OS code — the seam that lets TAR
/// reuse [`link_creation_is_categorical`] instead of a second copy of `WINDOWS_NO_LINK_SUPPORT`/`EPERM`
/// (plain code text, not a link — each is `cfg`-gated to one platform, and this function is not, so a
/// link to either dangles on the other platform's `cargo doc`; CPE-1814) that could drift from it. `None`
/// means `e` did not genuinely
/// come from the link-creation syscall, however it is shaped; [`tar_link_creation_outcome`] then treats
/// it as an ordinary failure, exactly as before this ticket.
///
/// **CPE-1813 review round 1 found two independent ways the first version of this recovery was
/// exploitable or over-eager. Round 2 found a third, in the same family: the fix for round 1's blocker 2
/// modelled the wrong nesting for one of its own two examples.**
///
/// **Blocker 1 (security).** The first version scraped `e.to_string()` directly. `e` here is always
/// `unpack_in`'s own outermost `TarError::desc`, `"failed to unpack `{file_dst}`"` — entirely the
/// archive's own attacker-controlled entry path (measured at the three sites that can produce it for a
/// **link** entry — `entry.rs:440` here, `:435` for [`ensure_dir_created`], `:589`/`:696` for
/// `set_symlink_file_times`/`set_ownerships` — all fixed text plus a *path*, never `{err}`; the crate has
/// further `TarError::desc` sites — `:684`, `:721`, `:793`, `:881`, `:940`, plus this crate's own
/// `archive.rs:222`/`:237` — but none of them fires on the file/symlink/hard-link arms a link entry takes,
/// so they are out of scope rather than overlooked). A crafted symlink entry named literally
/// `payload (os error 1)` recovered code 1 (in `WINDOWS_NO_LINK_SUPPORT` — plain code text, not a link:
/// this function is not `cfg`-gated, and `WINDOWS_NO_LINK_SUPPORT` only compiles on Windows, so linking
/// it here dangles `cargo doc` on every other platform; CPE-1814) from a genuine
/// `ERROR_ACCESS_DENIED` (5) failure, turning a real write failure into a silent skip — the exact
/// failure-filed-as-refusal defect [`materialise_entry_symlink`]'s own doc names. `e` is never inspected
/// at all now; only `e.source()` onward.
///
/// **Blocker 2 (parity), and round 2's correction to it.** Even a level down, not every `source()`
/// `unpack_in` can produce for a link entry is the link syscall's own error — `ensure_dir_created`
/// (parent-directory creation, `entry.rs:434`) wraps its **raw, unreformatted** `io::Error` straight into
/// a `TarError`, one level, so a plain `raw_os_error()` walk over every level found a perfectly genuine
/// `EPERM` there and misclassified it as "this volume has no links". **Round 1's fix — anchoring to
/// `marker` — closed that leg but not `set_symlink_file_times`'s (the mtime set that runs *after* a
/// symlink already exists on disk, `entry.rs:585-591`, on by default), because round 1 modelled it as the
/// same one-level shape and it is not.** `unpack()`'s mtime branch wraps the raw error in its OWN
/// `TarError` (`"failed to set mtime for `{dst}`"`, `entry.rs:589`) — call it `mid` — and `unpack_in`
/// wraps `mid` again in the outer "failed to unpack" `TarError`. So `mid` itself, one level down from `e`,
/// is what an unanchored `.find(marker)` walk actually reads, and `mid`'s text embeds `{dst}` — the
/// entry's own attacker-controlled destination path. An entry named `a when symlinking b` puts the
/// literal marker text inside `mid`'s rendering with an `Unsupported`-kind mtime failure behind it, and
/// round 1's `parse_os_error_code` prefix check does not save the *kind*-only fallback arm, which had no
/// equivalent guard: `Some(io::Error::new(mid.kind(), "failed to set mtime for `a"))` — categorical,
/// wrongly, for a symlink that was already successfully created.
///
/// **The fix is structural, not another phrase to anchor on: only a LEAF level — one with no `source()`
/// of its own — is ever trusted.** `mid` fails this immediately (`TarError::source` unconditionally
/// returns `Some(&self.io)`), so the walk passes straight through it to the raw error underneath, which
/// carries only the OS's own message — the entry's name never reaches it. This is what closes
/// `ensure_dir_created`'s case too, without needing the marker anchor to do that work: its raw-error level
/// is *also* the leaf the walk lands on, plain OS text, no wrap phrase at all. Being structural rather
/// than a list of phrases is the point — it holds for every current and future `TarError::desc` wording
/// without enumerating them, which round 1's version implicitly needed to (and, per blocker 1 above, did
/// not even do completely).
///
/// **One further leaf-shaped wrap exists, found auditing every `Error::new(kind, format!(..))` site in
/// `entry.rs` for round 2 rather than trusting round 1's enumeration: `validate_inside_dst`'s hard-link
/// leg** (`entry.rs:543`, validating the hard link's own resolved target against `target_base`) wraps a
/// canonicalize failure as `"{err} while canonicalizing {attacker-declared target}"` — `{err}` first,
/// like the genuine wrap, so the leaf check alone does not exclude it. Reachable only via a hard-link
/// entry whose declared target does not resolve (forcing the canonicalize to fail) and whose text embeds
/// this marker — measured as theoretical rather than staged (canonicalize failures are practically always
/// `NotFound`/`PermissionDenied`, neither categorical), but the mtime hole was "theoretical" under the
/// same reasoning before it was measured, so this is excluded structurally too: a level whose text
/// contains `" while canonicalizing "` is never trusted, symlinks included, since the phrase cannot occur
/// in a genuine `symlink`/`hard_link` syscall wrap.
///
/// **CPE-1829 — two more leaf-shaped wraps, enumerated but not previously written down.** `entry.rs:514`
/// (`"hard link listed for {header} but no link name found"`) and `:522` (`"symlink destination for
/// {header} is empty"`) both wrap via `other(&format!(..))` — `ErrorKind::Other`, no `source()` — and
/// `{header}` is `String::from_utf8_lossy` of the entry's raw, fully attacker-controlled 512-byte tar
/// header, so **either can carry either marker text** if an attacker shapes the header bytes that way.
/// They are harmless *today*, for two independent reasons: `ErrorKind::Other` is what
/// [`link_creation_is_categorical`] rejects outright when no `(os error N)` prefix parses, and
/// [`tar_entry_refusal`] already refuses an entry with an empty or missing link target before `unpack_in`
/// is ever reached, so this code path cannot fire from our own callers. **Say this out loud so the next
/// person does not re-derive it and does not assume it is guaranteed: that safety is a coincidence of
/// today's `ErrorKind` choice and today's pre-check, not a structural property of this function** — unlike
/// the `" while canonicalizing "` exclusion above, nothing here stops a future `tar` release, or a caller
/// that skips the pre-check, from making one of these two leaves reachable with a forged categorical
/// prefix.
fn recover_link_syscall_error(e: &std::io::Error, marker: &str) -> Option<std::io::Error> {
    let mut cur: Option<&(dyn std::error::Error + 'static)> = std::error::Error::source(e);
    while let Some(err) = cur {
        if let Some(level) = err.downcast_ref::<std::io::Error>() {
            // Only a LEAF — no `source()` of its own — can be tar's genuine
            // `Error::new(err.kind(), format!("{err}{marker}…"))` wrap: that shape wraps a bare `String`
            // with no source chain, whereas every `TarError`-mediated level does have one. See this
            // function's doc for why that is what actually closes blocker 2, structurally.
            if std::error::Error::source(level).is_none() {
                let text = level.to_string();
                if let Some(marker_at) = text.find(marker) {
                    let prefix = &text[..marker_at];
                    // `validate_inside_dst`'s own leaf-shaped wrap (see doc) puts attacker-controlled
                    // text after this exact phrase — never trust a prefix carrying it.
                    if !prefix.contains(" while canonicalizing ") {
                        return Some(match parse_os_error_code(prefix) {
                            Some(code) => std::io::Error::from_raw_os_error(code),
                            // No parseable code (e.g. `ErrorKind::Unsupported` with no "(os error N)"
                            // text) — keep the level's real `kind()` and the genuine prefix text rather
                            // than a manufactured "unknown" error, so the caller's message still says
                            // what happened.
                            None => std::io::Error::new(level.kind(), prefix.to_string()),
                        });
                    }
                }
            }
        }
        cur = err.source();
    }
    None
}

/// Pull the numeric code out of `std::io::Error`'s own `Display` shape for an OS-repr error,
/// `"<message> (os error <code>)"`, but only when `text` **begins with** exactly what
/// `Error::from_raw_os_error(code)` itself renders — so a match cannot be forged by attacker text placed
/// anywhere else in a longer string (CPE-1813 review round 1, blocker 1's second, independent hardening:
/// callers are expected to have already isolated `text` to a trustworthy prefix — see
/// [`recover_link_syscall_error`]'s doc — this check does not depend on that alone).
fn parse_os_error_code(text: &str) -> Option<i32> {
    let after = text.split_once("(os error ")?.1;
    let code: i32 = after.split(')').next()?.trim().parse().ok()?;
    text.starts_with(&std::io::Error::from_raw_os_error(code).to_string()).then_some(code)
}

/// [`link_creation_outcome`] for a TAR link entry, but **only when `e` is provably the
/// `symlink`/`hard_link` syscall's own failure** (CPE-1813; see [`recover_link_syscall_error`] for why
/// "provably" is load-bearing, not decorative). `marker` is [`TAR_SYMLINK_MARKER`] or
/// [`TAR_HARDLINK_MARKER`], chosen by the caller to match the entry's own declared kind.
///
/// When no such evidence is found, `e` is an ordinary write failure `unpack_in` hit somewhere else
/// (parent-directory creation, containment validation, an mtime set on a link that already exists on
/// disk) and this returns `Err` with `e`'s own message, never routed through the classifier at all —
/// the same treatment every other write failure in this module gets, which since CPE-1935 means one
/// recorded entry failure rather than a dead run.
///
/// **CPE-1813 review round 1, minor 4 — the message shown for a refusal is the genuine syscall text,
/// not a manufactured one.** [`recover_link_syscall_error`]'s reconstructed error is what gets
/// classified, but it is also what gets *displayed*: when a code was parsed, it is rebuilt via
/// `Error::from_raw_os_error(code)`, whose `Display` is the same deterministic OS strerror lookup that
/// produced the genuine wrapped text in the first place — same code, same platform, same string, not an
/// approximation of it. When no code was parseable (the `Unsupported`-kind fallback), the reconstructed
/// error carries the *exact* original prefix text verbatim (see that function's `None` arm), not a
/// generic "unknown" message. Either way ZIP and TAR show the user the same words for the same cause.
fn tar_link_creation_outcome(
    target: &Path,
    out: &Path,
    e: &std::io::Error,
    marker: &str,
) -> Result<Option<String>, String> {
    match recover_link_syscall_error(e, marker) {
        Some(syscall_err) => link_creation_outcome(target, out, &syscall_err),
        None => Err(format!("could not create the link \"{}\": {e}", out.display())),
    }
}

/// Create the link a symlink entry asks for, classifying what happens into this module's two outcomes:
/// `Ok(None)` created, `Ok(Some(reason))` **refused** (skip), `Err(e)` **failed** (recorded against
/// this entry since CPE-1935; a whole-run abort before it).
///
/// # The `AlreadyExists` retry, and why it is not a new policy
///
/// `symlink`/`symlink_file` are exclusive-create: they fail `AlreadyExists` over *anything* already at
/// the name. The slot has already been proven **not a link** by `entry_sink_action` one step earlier, so
/// what is there is an ordinary file or directory — and *"overwriting an ordinary existing file is
/// unaffected, that stays allowed"* is this module's documented, long-standing contract (see the section
/// comment's rows 6–14 note, and `src/docs/explorer-archives.md`). The file branch of the same loop
/// honours it via `File::create`'s truncate; `tar` honours it for its own links the same way
/// (`remove_file` then retry, `tar-0.4.46/src/entry.rs:562-568`). So this branch honours it too, rather
/// than inventing a link-shaped exception to it.
///
/// **The first version of this code got that backwards** and, worse, pinned it: it reported
/// `AlreadyExists` as "this system would not create it — enable Developer Mode", and its only test
/// staged an ordinary file in the slot and asserted the file survived. The behaviour was wrong, the
/// message was untrue, and the test certified both.
///
/// A `remove_file` that fails — because the occupant is a **directory**, most obviously — is a failure,
/// treated exactly as `File::create` would be on the same path (one recorded entry failure since
/// CPE-1935; before it, a whole-run abort). **Its error is returned directly and
/// never handed to [`link_creation_outcome`]**, which is not tidiness: `remove_file` answers `EPERM` on
/// a sticky-bit directory such as `/tmp`, and `EPERM` is one of the codes that classifier reads as (plain
/// code text, not a link: `EPERM` only compiles under `cfg(unix)` and `materialise_entry_symlink` is not
/// itself `cfg`-gated, so a link here dangles `cargo doc` on Windows; CPE-1814)
/// "this filesystem has no links". Routing it there would file a failure as a refusal — the defect this
/// whole review chain is about — on POSIX only, where nothing here runs.
fn materialise_entry_symlink(out: &Path, target: &Path) -> Result<Option<String>, String> {
    let err = match create_entry_symlink(out, target) {
        Ok(()) => return Ok(None),
        Err(e) => e,
    };
    if err.kind() != std::io::ErrorKind::AlreadyExists {
        return link_creation_outcome(target, out, &err);
    }
    if let Err(removing) = fs::remove_file(out) {
        return Err(format!(
            "could not replace \"{}\" with the archive's link: {removing}",
            out.display()
        ));
    }
    match create_entry_symlink(out, target) {
        Ok(()) => Ok(None),
        Err(retried) => link_creation_outcome(target, out, &retried),
    }
}

/// Row 17: create an extraction's destination folder, and **say something true when that fails**
/// (CPE-1744).
///
/// The `create_dir_all` itself is unchanged, and so is the *live*-link behaviour: `dest` is a folder the
/// user **pointed at**, not a name being claimed, so following a link there is the right answer
/// (`fsutil`'s claiming-vs-editing rule) and this returns `Ok` exactly as before. Only the **dangling**
/// link case is touched, and only its wording — measured on both platforms:
///
/// ```text
/// [CPE-1744 M] extract_archive           dest = dangling link -> Err("Cannot create a file when that
///                                                                    file already exists. (os error 183)")
/// [CPE-1744 M] extract_archive_streamed  dest = dangling link -> the same
/// [Linux]                                                        Err("File exists (os error 17)")
/// ```
///
/// Neither string names the folder, and both send the user to delete "the file that already exists" —
/// which does not exist. What exists is the link. That is the identical defect
/// `fsutil::create_slot_refusal`'s doc calls out and that row 7 got a guard for; this is the same fix at
/// the same kind of site, and like row 7's it changes behaviour on **no input at all** (it only rewords a
/// failure that already happened).
fn extraction_dest_error(dest: &Path, e: &std::io::Error) -> String {
    let is_link = fs::symlink_metadata(dest).map(|m| m.file_type().is_symlink()).unwrap_or(false);
    if is_link {
        format!(
            "\"{}\" is a link, and it leads nowhere — the extraction folder cannot be created there. The \
             OS reports \"{e}\", which sends you to delete a file that does not exist; what exists at that \
             name is the link. Repair the link's target, or extract somewhere else",
            dest.display()
        )
    } else {
        format!("the extraction folder \"{}\" could not be created: {e}", dest.display())
    }
}

/// Open the extraction folder as a handle the per-component walk can resolve against, or say — naming
/// the folder — why nothing can be written into it (CPE-1938).
///
/// [`extract_zip_archive_stream`] grew this inline for CPE-1913; the tar and 7z legs need the identical
/// two steps and the identical wording, so it is one function rather than three copies. `dest` is
/// `canonicalize`d here for [`crate::open_beneath::open_root`]'s stated precondition, and following a
/// link at `dest` itself is deliberate and unchanged — row 17 of the CPE-1733 table: the folder is the
/// one the user pointed at, so a link there is their own arrangement.
fn open_extraction_root(dest: &Path) -> Result<crate::open_beneath::RootDir, String> {
    let real_dest = fs::canonicalize(dest).map_err(|e| extraction_dest_error(dest, &e))?;
    crate::open_beneath::open_root(&real_dest, "extraction folder").map_err(|e| {
        format!(
            "the extraction folder \"{}\" could not be opened ({e}), so nothing can be written into it \
             in a way that can be checked. Nothing was extracted",
            dest.display()
        )
    })
}

/// **Every directory component of `name` must be a real directory, opened relative to the handle above
/// it** — the question `confined_to` cannot ask, for the legs that hand their write to something this
/// module does not own (CPE-1938, rows 19–22; CPE-1973, row 16's symlink sub-branch).
///
/// **Three callers, not two, and the third is why CPE-1973 exists.** Round 1 scoped this to "the two
/// legs that still hand their write to a third-party unpacker" and treated the whole zip loop as
/// already handle-gated by CPE-1913. It is not: `create_beneath` is called only in that loop's *file*
/// branch and `create_dir_beneath` only under `entry.is_dir()`, so a zip **symlink** entry reached a
/// by-path `symlink`/`remove_file` with its components unresolved. "The loop was converted" was read
/// as "every branch of the loop was converted", and the one branch that was not is the one whose
/// residual is a delete. Enumerate the branches, not the function.
///
/// # The defect this closes, measured on Windows before it landed
///
/// A junction at `dest/sub` needs no privilege to plant. Point it **outside** the extraction folder and
/// [`entry_sink_action`]'s `confined_to` refuses the entry, which is what CPE-1744 fixed. Point it at
/// `dest/other` — still inside — and containment says **yes**, correctly by its own contract: the write
/// really does stay inside the folder the user chose. It just does not go where the archive said.
///
/// ```text
/// [tar  one-shot  junction -> dest/other] Err("failed to unpack `…\out\sub`")
///                                         other/leaf.txt  = "ARCHIVED LEAF"   <- payload redirected
///                                         other/deeper    = created           <- tree shape redirected
/// [tar  streamed  junction -> dest/other] Err("failed to unpack `…\out\sub`")
///                                         nothing extracted at all, ok.txt included  <- denial
/// [7z   one-shot  junction -> dest/other] Ok(done: 2, skipped: 0, errors: [])
///                                         other/leaf.txt  = "ARCHIVED LEAF"   <- silent, reported clean
/// [7z   streamed  junction -> dest/other] Ok(done: 2, skipped: 0, errors: [])
///                                         other/leaf.txt  = "ARCHIVED LEAF"   <- silent, reported clean
/// ```
///
/// The 7z rows are the ones CPE-1938 was filed calling *inferred*; they are measured now, and they are
/// the worst of the four — a complete success report over a payload written somewhere the archive never
/// named. The tar rows are two different failures of the same cause: the one-shot leg does the harm and
/// *then* errors (its directory entries are deferred to a second pass, which is what finally trips over
/// the junction), and the streamed leg turns one planted junction into total denial of the archive.
///
/// # The property, and why an inside-pointing link violates it
///
/// **Every component of the entry's destination is a real directory, opened by name relative to the
/// handle of the component above it, starting from the extraction folder's own handle** — never
/// resolved as a path. Containment asks a different question, *where does this path end up*, and an
/// inside-pointing junction answers it honestly: inside. That is why no path check can see this shape,
/// and why the direction the link points is irrelevant here — [`crate::open_beneath`] refuses a link at
/// a component without asking where it goes.
///
/// # Ordering: this runs AFTER [`entry_sink_action`], and that is a decision (CPE-1929)
///
/// The outside-pointing junction is answered by `confined_to` first, so this check's refusal is
/// observable only for the shapes containment cannot see — the inside-pointing one above, chiefly.
/// That keeps **both** guards reachable and red-proofable, which was measured rather than reasoned
/// about: disable this one and the inside-pointing legs red while the outside ones stay green; disable
/// `confined_to` and the reverse happens (the outside legs red on *which guard answered*, not on
/// escaped bytes — this walk catches them too). Put this one **first** instead and `confined_to` would
/// answer nothing this does not for the intermediate-component shape, i.e. it would become a shadowed
/// guard reading as coverage, so it stays in front. The three sabotage runs and their suite-wide
/// numbers are on
/// `cpe1938_tar_and_7z_never_redirect_an_entry_through_a_link_at_a_path_component`.
///
/// # What it is NOT: this is containment for a PLANTED link, not for a RACED one
///
/// `tar`'s `Entry::unpack_in` and `sevenz-rust`'s `default_entry_extract_fn` own the write and take a
/// **path**. There is no handle to hand them and no per-entry hook to intercept, so replacing them is
/// the only way to close the race — that is the "third-party unpacker replaced" work CPE-1913 scoped
/// out and CPE-1938 keeps scoped out, deliberately, rather than doing four legs shallowly. What this
/// function buys is real and bounded: **a link already sitting at a component when the entry is
/// processed is refused, whichever way it points**, on all four tar/7z legs. A component swapped in
/// between this walk and the unpacker's own resolution is **not** covered, and this comment is the
/// record of that rather than a green test implying otherwise.
///
/// **The raced residual puts archive bytes OUTSIDE the folder, and the numbers are here because
/// round 1 framed the whole ticket as an inside-redirect** (CPE-1938 round 2, Security Auditor). It
/// narrows the race; it does not close it. Real ext4, 40 trials × 500 entries, `renameat2`'s
/// `RENAME_EXCHANGE` swapping `dest/sub` atomically so the component is **never absent**, target
/// **outside** `dest`:
///
/// ```text
///                  with this walk        with this walk disabled
/// tar one-shot     8/40 trials,  10 entries escaped     9/40, 18 entries
/// tar streamed     5/40 trials,   5 entries escaped    17/40, 28 entries
/// ```
///
/// Planted goes 100% → 0%, which is the ticket's point; raced narrows ~2–5× and stays open. The
/// attacker shape matters as much as the numbers: a **naive** remove-then-create racer makes the
/// component briefly vanish, which this walk turns into a whole-run stop, so the naive attacker looks
/// harmless and the atomic one does not. Rows 16 and 19–22 of the CPE-1733 table carry this as an
/// explicit residual marker rather than advertising containment the legs do not have (CPE-1958).
///
/// It also materialises the chain it verified (`create_dir_beneath` creates missing levels inside the
/// handle above them), so the unpacker's own `create_dir_all` finds real directories where it would
/// otherwise have followed a link.
fn entry_component_action(
    root: &crate::open_beneath::RootDir,
    name: &str,
    is_dir: bool,
) -> EntrySlotAction {
    let rel = Path::new(name);
    // A file entry's own leaf is NOT walked here: the unpacker creates it, and a link sitting at it is
    // `entry_sink_action`'s question (`create_slot_link_verdict`), already asked. A directory entry's
    // leaf IS its own last component, so the whole `rel` goes in.
    let chain = if is_dir { Some(rel) } else { rel.parent() };
    let Some(chain) = chain else { return EntrySlotAction::Write };
    if chain.as_os_str().is_empty() {
        return EntrySlotAction::Write; // a leaf directly in the extraction folder — no components to walk
    }
    match crate::open_beneath::create_dir_beneath(root, chain) {
        Ok(()) => EntrySlotAction::Write,
        // Same split every other refusal in this module uses, and carried by `Refusal::policy` rather
        // than re-derived: a link at a component is a **verdict** about one entry (skip, keep going);
        // a permission or sharing answer is an I/O **failure** and takes the run down, because an entry
        // the filesystem refused for an I/O reason is a file the user asked for and did not get.
        Err(r) if r.policy => EntrySlotAction::Skip(r.why),
        // **The Abort arm escalates severity relative to `main`, and that is deliberate — but it was
        // uncovered, which is how it got through round 1** (CPE-1938 round 2, Security Auditor).
        // Before this function existed, `confined_to` failing closed degraded a component it could not
        // resolve to a per-entry **Skip**; this arm turns the same condition into failure of the whole
        // archive, and the Auditor's first attacker hit it *by accident* with a transient `ENOENT`.
        // The CPE-1929 pair came back green in both halves — full `--lib` at 2413/2 with the arm
        // demoted to `Skip`, and 2413/2 unmodified — i.e. nothing in the suite could tell the two
        // apart. `cpe1938_an_unopenable_extraction_folder_aborts_the_tar_and_7z_runs` looks like
        // coverage and is not: it exercises `open_extraction_root`, a different call.
        //
        // **Why total denial is nonetheless right here, argued rather than assumed.**
        // `create_dir_beneath` *creates* missing components, so `ENOENT` is not the ordinary
        // "not there yet" case that a Skip would be the kind answer to — it means something removed a
        // component out from under an extraction that is already in progress, which is precisely the
        // concurrent-mutation attacker the rest of this ticket is about. Continuing to write into a
        // destination that is being mutated underneath us is the worse failure. And the file branch
        // one level down already returns `Err` on the same `Refusal::policy == false` class, so a Skip
        // here would leave two branches disagreeing about one fact — the drift
        // `Refusal::policy` exists to prevent. Covered now by
        // `cpe1938_a_component_the_filesystem_refuses_for_an_io_reason_stops_the_run`, which forces a
        // real `EACCES` rather than racing for one.
        //
        // **The wording complaint is real and deliberately NOT fixed here.** "could not be opened
        // (No such file or directory)" reads as a permissions problem. The sentence is
        // `open_beneath::refuse`'s, shared by the archive, transfer and revert legs and pinned by
        // tests in all three, so re-wording it is its own change with its own blast radius rather than
        // a line in this ticket.
        //
        // **CPE-1935 re-examined this arm and KEPT it, on a better reason than the one above.** That
        // ticket demoted every *leaf* I/O failure in this module from a whole-run abort to a per-entry
        // `Fail`, which is the opposite move; the two are consistent because [`EntrySlotAction`]'s rule
        // asks **scope** before severity, and this walk's scope is not one entry. `chain` here is the
        // entry's *directory components* — for a file entry, `rel.parent()`, every level of which every
        // sibling entry underneath it also travels through; for a directory entry, the directory it
        // came to create, which is the same thing one level down. `create_dir_beneath` **creates**
        // missing levels, so a refusal is never "not there yet": it is the destination answering that
        // it cannot hold the tree the archive describes, and continuing to write into a destination
        // that is being mutated underneath a run in progress is the worse failure. Per-entry recording
        // would also be a lie about scope — it would name one entry for a fact about a directory.
        //
        // The line, stated once so a future arm can be placed by it rather than by feel: **the leaf is
        // the archive's business and the chain is the run's.**
        Err(r) => EntrySlotAction::Abort(r.why),
    }
}

/// How many `e<seq>` names [`temp_extract_target`] will try before giving up. Exclusive creation makes a
/// name that already exists a *retry*, not a failure.
///
/// **CPE-1786 changed what this bound protects against, and it is worth being precise about.** Before
/// that ticket the loop numbered into the shared `cpe-archive` root, where `EXTRACT_SEQ` restarts at 0 in
/// every process, PIDs are reused by the OS, and nothing ever cleaned up — so a fresh process with a
/// recycled PID could genuinely walk over a previous run's whole range, and on the machine CPE-1786 was
/// filed from (1,394,403 leftover directories) it **did**: `could not claim a private extraction
/// directory … after 1024 attempts` was a real, observed failure, not a theoretical one.
///
/// Numbering now happens inside [`session_root`] — a directory this process created exclusively moments
/// ago, which no other process numbers into and which nothing has ever been inside. A monotonic counter
/// in a private directory cannot collide with itself, so **exhaustion is no longer reachable by
/// accumulation**; every attempt after the first would have to lose a race with something that is
/// deliberately squatting names inside our own session directory. The bound stays because that squatter
/// (a same-user process; or, in its own private namespace, our own
/// `row1_a_squatted_temp_directory_is_stepped_over_not_written_into` test) must still end as a clear
/// error rather than an infinite loop.
const TEMP_TARGET_ATTEMPTS: u64 = 1024;

/// The shared root every extraction directory lives under, named once so the claim, the sweeper and the
/// tests cannot drift apart.
const ARCHIVE_TEMP_ROOT: &str = "cpe-archive";

/// How long a session root that is **not ours** must have gone untouched before [`sweep_stale_sessions`]
/// will remove it.
///
/// **This TTL is the whole protection for a live session. There is no second mechanism** (PR #945
/// re-review), and the sentence that used to claim one is the reason this paragraph is now this blunt.
///
/// The liveness signal is the session root's own mtime: creating each `e<seq>` subdirectory updates the
/// mtime of the directory it is created in, so an instance that is actively extracting keeps its own root
/// fresh. **Measured on Windows** for this ticket (parent `LastWriteTimeUtc` moved from `05:38:18.544` to
/// `05:38:19.794` on creating one child). On Linux and macOS this is ordinary POSIX directory behaviour
/// and CI exercises the sweep there, but it was **not measured here** — stated that way rather than as
/// "on every platform this ships to", which is what the previous version of this comment claimed on no
/// evidence at all.
///
/// **24 hours, not one.** There is no PID check — mtime is the entire signal — so a short TTL makes
/// sweeping a *live* session reachable: instance A extracts at 10:00 and the user leaves it open; at
/// 11:05 instance B launches and removes A's whole session root. [`temp_extract_target`] recovers the
/// *directory* on the next extraction but cannot recover the *files*, so an external editor holding an
/// opened temp copy loses the user's Save. An hour is a realistic afternoon; a day is not, and it costs
/// nothing: the sweep's drain rate is set by [`SWEEP_REMOVE_BUDGET`] per launch, not by the TTL.
///
/// # The mechanism that used to be here, and why it is gone
///
/// A `session.lock` file was held open for the process lifetime, on the belief that an open handle makes
/// `remove_dir_all` fail so another instance's sweep bounces. **Measured cross-process by the PR #945
/// re-reviewer, it did nothing at all:**
///
/// ```text
/// [cross-process] remove_dir_all = Ok(())
/// [cross-process] session still exists = false
/// [cross-process] e0/a.txt still exists = false
/// ```
///
/// `fs::File::create` opens with Rust's default Windows share mode (`READ|WRITE|DELETE`) and std's
/// `remove_dir_all` uses POSIX-semantics deletes, so the whole tree went, files included. Adding
/// `share_mode(1)` does make the *root* survive — but the same measurement showed the **contents are
/// deleted first** and only the lock file stops the directory itself going, so even the fixed version
/// would not save the files it was supposed to be protecting. A mechanism whose honest description is
/// "the empty directory survives" is not worth the code, and a comment overstating a protection is worse
/// than no comment because it stops the next person checking. Removed rather than repaired.
///
/// This is the same distinction this file already draws about an *external* application's handle — that
/// it protects one class of consumer and not `notepad.exe` — applied to our own handle, which is exactly
/// the check that was skipped when it was written.
///
/// The cost of the TTL being wrong was designed for: the frontend's archive-preview cache re-validates
/// every cached temp path before use and re-extracts if it is gone (`src/lib/archivePreview.ts`). A
/// reader that already holds the file open keeps reading it on Unix — POSIX unlink semantics, asserted
/// from the specification and **not measured here**.
const SESSION_TTL: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

/// Diagnostic override for [`SESSION_TTL`], in whole seconds (`0` = sweep every foreign session root on
/// sight). It exists so the CPE-1786 evidence run can show cross-session growth actually *stopping*
/// inside a single measurement instead of asserting that it would an hour later, and so the sweep is
/// exercisable from a test harness. Same shape as this crate's other diagnostic env switches
/// (`CPE_STAGING_STRICT`, `CPE_STAGING_SABOTAGE`). Not a supported production setting: with a short TTL a
/// second instance will reclaim a *live* instance's directories.
const SESSION_TTL_ENV: &str = "CPE_ARCHIVE_TEMP_TTL_SECS";

/// How many entries of the shared root one process will look at while sweeping, and how many it will
/// remove. Deliberately small and deliberately **synchronous**.
///
/// Synchronous because the alternative does not work: a detached sweeper thread is killed when the
/// process exits, and the processes that create most of these directories (test binaries, a `/run`
/// launch) are short-lived — the sweep would reliably never finish. Small because the root it reads may
/// legitimately hold a very large number of entries (`read_dir` yields lazily, so a bounded `take` does
/// not depend on the directory's size the way a full listing would — and removing is the expensive half
/// regardless), and because this runs on the first extraction of a session, where a
/// visible stall would be paid by the user opening a file inside a zip. One process launch therefore
/// reclaims at most 32 dead sessions, which is far more than the ~1 it creates: the steady state drains.
const SWEEP_EXAMINE_BUDGET: usize = 256;
/// See [`SWEEP_EXAMINE_BUDGET`].
const SWEEP_REMOVE_BUDGET: usize = 32;

/// How many extraction directories one session keeps alive before reclaiming its oldest, once the
/// process has gone quiet — see [`drain_reapable`], which is where "quiet" is defined and where the
/// PR #945 review's blocker lives.
///
/// This is the half of the ownership model that a startup sweep cannot provide: a single long-running app
/// session that extracts all day would otherwise still grow without bound, because its own session root
/// is only reclaimed by the *next* session.
///
/// **512, not 64** (PR #945 review). The cap is the second line of defence behind the quiet gate, and it
/// is what bounds the shape the quiet gate cannot see — see [`REAP_GRACE`] and `drain_reapable`'s
/// residual section for what that shape actually is, which is **not** what this comment claimed for two
/// rounds. The memory cost of the difference is a few hundred `PathBuf`s.
const MAX_LIVE_EXTRACTIONS: usize = 512;

/// How long the process must have started **no new extraction** before [`drain_reapable`] will reclaim
/// anything. Measured against the newest entry, not the oldest — see that function.
///
/// **Ten minutes, not one** (PR #945 re-review), and this is a *boundary* move rather than a fix. The
/// residual is a single inter-entry gap longer than this value, so the value is the whole exposure: at
/// 60 s the re-reviewer reclaimed 89 directories out from under a still-staging 601-entry drag two
/// minutes in, because one entry crossed a minute. Ten minutes means a *single* archive entry must take
/// over ten minutes to extract, mid-batch, in a batch already past [`MAX_LIVE_EXTRACTIONS`] — a strictly
/// rarer event, so the exposure shrinks rather than merely moving.
///
/// What it costs is one sentence that is easy to get wrong, so precisely: reclamation is **push-driven**
/// — [`note_extraction_dir`] is the only thing that calls [`drain_reapable`] — so an idle session never
/// tidies itself at all. **The next extraction after ten minutes of quiet is the one that tidies.** A
/// session that goes idle forever keeps its directories until the process exits and the next launch's
/// [`sweep_stale_sessions`] takes the whole root. Nothing depends on the timing either way; the bound is
/// [`HARD_CAP_EXTRACTIONS`], not this.
///
/// It is deliberately *not* claimed to close the hole. `drain_reapable` documents exactly what remains,
/// and `cpe_1786_the_quiet_gate_protects_a_slow_batch_but_one_long_gap_is_the_known_residual` asserts it
/// so it cannot quietly stop being true in either direction.
const REAP_GRACE: std::time::Duration = std::time::Duration::from_secs(10 * 60);

/// The bound on a session that is *never* quiet — a script or an agent extracting back-to-back for
/// hours, where [`REAP_GRACE`] never elapses and [`MAX_LIVE_EXTRACTIONS`] is therefore never applied.
/// Past this, [`drain_reapable`] reclaims down to this figure regardless of quiet, because "unbounded"
/// is the defect this whole ticket exists to close and a mechanism that can be held open forever by a
/// busy caller has not closed it. Set far above any plausible single batch, so reaching it means the
/// caller is a loop rather than an interaction.
const HARD_CAP_EXTRACTIONS: usize = 4096;

/// This process's private extraction root under `%TEMP%/cpe-archive`, claimed once on first use.
static SESSION_ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();


/// The extraction directories this process has created, oldest first, with when it created them — the
/// bookkeeping behind [`MAX_LIVE_EXTRACTIONS`]. Only ever holds directories **we** created exclusively,
/// so nothing here can name something another process owns.
static LIVE_EXTRACTIONS: std::sync::Mutex<std::collections::VecDeque<(PathBuf, std::time::Instant)>> =
    std::sync::Mutex::new(std::collections::VecDeque::new());

/// Who owns an extraction directory, and for how long — **the whole of CPE-1786** (see also
/// [`temp_extract_target`], which is where the lifetime is handed out).
///
/// # The question, and why the obvious answers are all wrong
///
/// `temp_extract_target` created `%TEMP%/cpe-archive/<pid>-<seq>/` per extraction and never removed it.
/// The measurement that opened the ticket: **1,394,403** directories on one machine, and a clean-slate
/// run of one crate's test module adding thousands more — live growth, not history. It is user-facing
/// too: every archive entry a real user opens or drags out leaves a directory behind forever.
///
/// The test-helper half of the same class (CPE-1693) was fixed with a `Drop` guard. **That cannot work
/// here**, and the reason is the ticket: the whole point of the extracted file is that it *outlives* the
/// call that made it. So the question is who owns it instead. Reading the three real consumers:
///
/// 1. **Open-in-external** (`App.svelte`) hands the path to `open_external` — an arbitrary OS
///    application. When it is finished, or whether it ever opened, is not observable from here.
/// 2. **Drag-out** (`FileList.svelte`, alt-drag) hands a batch of paths to the native drag; the OS copies
///    them at drop time, after the function that staged them has long returned. The research note for
///    that feature already concluded *"do NOT delete on Dropped — the OS copy may still be reading;
///    session/periodic cleanup"*.
/// 3. **Archive preview** (`src/lib/archivePreview.ts`) caches the path per (archive, entry) **for the
///    whole session**, and — decisively — *re-validates the cached path before every reuse and
///    re-extracts if it is gone*, because "the temp file can be reaped mid-session".
///
/// So: the extraction call cannot own the directory (the path escapes it), and neither can the consuming
/// operation (it ends inside another process). **The session owns it** — and consumer 3 is already
/// written to survive reaping, which is the property that makes an owner other than "forever" possible at
/// all. This function is that owner made explicit.
///
/// # The shape
///
/// - **One session root per process**, `cpe-archive/s<pid>-<random>`, created with an exclusive
///   `fs::create_dir`. The random half is what stops a recycled PID from inheriting a previous run's
///   name; it comes from [`std::collections::hash_map::RandomState`], which is seeded by the OS and adds
///   no dependency.
/// - **Extractions are numbered inside it** (`e<seq>`), so the namespace that exhausted at 1024 attempts
///   is now private to one process and monotonic — it cannot collide with itself. See
///   [`TEMP_TARGET_ATTEMPTS`].
/// - **The next session reclaims dead ones.** Claiming the root also sweeps the shared root, budgeted, for
///   session directories nothing has touched in [`SESSION_TTL`] — including the pre-CPE-1786
///   `<pid>-<seq>` shape, so the old leak drains rather than being frozen in place.
/// - **A long session reclaims its own**, oldest first, past [`MAX_LIVE_EXTRACTIONS`] and
///   [`REAP_GRACE`] — the case a startup-only sweep does not cover.
/// - [`cleanup_extraction_scratch`] removes the whole session root when an embedder tells us the session
///   is over — **not yet wired into the app's exit path (CPE-1797)**, so today every session is reclaimed
///   by the next launch's sweep or by the cap, never at shutdown.
///
/// # Degraded mode
///
/// If the session root cannot be claimed at all, this returns the shared root itself and extractions are
/// numbered directly under it. That is strictly the old behaviour, which is the right failure mode: an
/// extraction still works, it is still an exclusive `create_dir` on a private directory, and only the
/// leak-freedom is lost. Refusing to extract because a *cleanup* mechanism could not be set up would be a
/// worse trade than the leak it prevents.
fn session_root() -> Result<PathBuf, String> {
    let root = std::env::temp_dir().join(ARCHIVE_TEMP_ROOT);
    // The shared root is the one directory here we cannot own exclusively. Refuse it if it is a link:
    // everything below would be redirected wholesale, and unlike a squatted leaf that is not something a
    // per-extraction guard can catch later. Re-checked on every call, not just the first, because the
    // root outlives us and this is cheap.
    refuse_link_at_new_file(&root)?;
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    if let Some(existing) = SESSION_ROOT.get() {
        return Ok(existing.clone());
    }
    Ok(SESSION_ROOT
        .get_or_init(|| {
            let claimed = claim_session_root(&root).unwrap_or_else(|| root.clone());
            // Inside `get_or_init` so it runs exactly once per process, and before the first extraction
            // rather than after, so a session that extracts once still pays its share of the cleanup.
            let removed = sweep_stale_sessions(
                &root,
                &claimed,
                std::time::SystemTime::now(),
                session_ttl(),
                SWEEP_EXAMINE_BUDGET,
                SWEEP_REMOVE_BUDGET,
            );
            // Silence on the extraction path is right; being *undiscoverable* is a different axis, and a
            // sweep that quietly never works would look identical to one that does (PR #945 review). The
            // one line is emitted only when the diagnostic knob is set, and written straight to the
            // process's stderr handle rather than through `eprintln!`, which libtest swallows.
            if std::env::var(SESSION_TTL_ENV).is_ok() {
                use std::io::Write;
                let _ = writeln!(
                    std::io::stderr(),
                    "[CPE-1786] swept {removed} stale session(s) under {} (examined at most {}, removal \
                     budget {}, ttl {:?})",
                    root.display(),
                    SWEEP_EXAMINE_BUDGET,
                    SWEEP_REMOVE_BUDGET,
                    session_ttl()
                );
            }
            claimed
        })
        .clone())
}

/// [`SESSION_TTL`], with the [`SESSION_TTL_ENV`] override applied. An unparseable value is ignored rather
/// than reported: this is a diagnostic knob on a best-effort cleanup path, and failing an extraction over
/// a typo in an environment variable would be the worse outcome.
fn session_ttl() -> std::time::Duration {
    std::env::var(SESSION_TTL_ENV)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map_or(SESSION_TTL, std::time::Duration::from_secs)
}

/// A per-process session directory name: `s<pid>-<16 hex digits>`.
///
/// The PID is kept because it makes the directory identifiable in a process listing when something goes
/// wrong; the trailing half is what carries the uniqueness, since a PID alone is exactly what CPE-1786
/// measured colliding.
///
/// That half is the hash of **two** inputs — an [`std::collections::hash_map::RandomState`] seed and a
/// nanosecond wall-clock reading — and the honest claim is only about their *combination*. The PR #945
/// final verifier measured why that distinction matters: swapping in a fixed-seed `DefaultHasher`, the
/// names **still all came out distinct**, because the nanosecond term alone varies between calls. So
/// "`RandomState` is what makes these unique" was not something the test below established, and it is not
/// claimed here. What is relied on, and what *is* measured, is that `session_dir_name()` does not repeat.
///
/// Both inputs are kept anyway, because they fail differently: the clock reading is what separates two
/// calls in one process, and the OS-seeded state is what separates two processes that start in the same
/// nanosecond (and is unguessable, which is what retires the pre-CPE-1786 denial-of-service residual).
/// Both are in `std`, so the naming costs no dependency.
fn session_dir_name() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u32(std::process::id());
    if let Ok(since) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        hasher.write_u128(since.as_nanos());
    }
    format!("s{}-{:016x}", std::process::id(), hasher.finish())
}

/// Claim a fresh session directory under `root`, exclusively. `None` means every attempt was taken or the
/// filesystem refused — see [`session_root`]'s "degraded mode".
fn claim_session_root(root: &Path) -> Option<PathBuf> {
    /// A random 64-bit name colliding once is already implausible; eight in a row means something other
    /// than chance is happening, and that something is better handled by degrading than by looping.
    const ATTEMPTS: u32 = 8;
    for _ in 0..ATTEMPTS {
        let dir = root.join(session_dir_name());
        match fs::create_dir(&dir) {
            Ok(()) => return Some(dir),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return None,
        }
    }
    None
}

/// Whether `name` is a directory name **this code has ever created** under the shared root, and therefore
/// one the sweeper may remove. Three shapes, all of them ours:
///
/// - `s<pid>-<hex>` — a session root (CPE-1786);
/// - `e<seq>` — an extraction directory, which only appears directly under the shared root in
///   [`session_root`]'s degraded mode;
/// - `<pid>-<seq>` — the pre-CPE-1786 shape. **Recognising it is about not re-accumulating, not about
///   clearing the backlog**: at [`SWEEP_REMOVE_BUDGET`] per launch, 1.39 million would need roughly
///   43,500 launches, so the existing pile was cleared by a one-shot purge and this sweep is what keeps
///   it from coming back. Named explicitly so the leftovers still on disk drain
///   through the same mechanism instead of needing a person to remember them.
///
/// Everything else is left alone. `%TEMP%/cpe-archive` is a shared directory — on a Unix `/tmp` it is
/// shared between *users* — and a sweeper that removed names it did not recognise would be a recursive
/// delete pointed at whatever someone else happened to put there.
fn is_our_temp_dir_name(name: &str) -> bool {
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let hex = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_hexdigit());
    if let Some(rest) = name.strip_prefix('s') {
        return match rest.split_once('-') {
            Some((pid, token)) => digits(pid) && hex(token),
            None => false,
        };
    }
    if let Some(rest) = name.strip_prefix('e') {
        return digits(rest);
    }
    match name.split_once('-') {
        Some((pid, seq)) => digits(pid) && digits(seq),
        None => false,
    }
}

/// Remove session directories under `root` that nothing has touched for `ttl`, skipping `keep` (ours).
/// Returns how many were removed — the value the tests assert on, and the only reason this returns
/// anything at all.
///
/// `now` and `ttl` are parameters rather than reads of the clock so the age decision can be tested
/// without waiting an hour or forging a directory's mtime (which has no portable API in `std`).
///
/// **Failures are skipped, not reported**, which is deliberately the same philosophy `CLAUDE.md` pins on
/// `list_dir`: *skip entries you can't read rather than failing the whole listing*. The analogue here is
/// stronger than an aesthetic one — this runs on the first extraction of a session, so a cleanup that
/// turned an unreadable or in-use leftover into an error would fail the user's extraction over somebody
/// else's litter. A directory that will not be removed today is simply examined again by the next
/// session. On Windows that also gives a *persistently* locked file some protection for free — but the
/// PR #945 UAT measured the limits of that and they matter, because "open in external app" is one of the
/// three consumers this whole ownership argument rests on. A handle held open with a share mode that
/// denies delete (a media player, an editor that keeps the file mapped) does bounce the removal, cleanly
/// and skipped exactly as described. **A modern `notepad.exe` does not**: it reads the file and releases
/// the handle almost immediately, and the sweep then deleted the file out from under an open Notepad
/// window. So this is a real mitigation for one class of consumer, not a blanket guarantee for all of
/// them, and it is why the age thresholds rather than the OS are what actually carry the safety.
///
/// **The examine budget takes directory order, not age order**, so an entry that cannot be removed —
/// another user's session root under a sticky `/tmp`, a directory an application still has open —
/// permanently occupies a slot in the window every launch looks at, and the sweep sees
/// less of the root than the budget suggests. That is acceptable for the same reason the failures are
/// skipped: the sweep is a background drain, not a guarantee, and the entries it keeps re-examining are
/// precisely the ones it must not remove. It is stated because a reader measuring the drain rate would
/// otherwise compute `SWEEP_REMOVE_BUDGET` per launch and be wrong.
///
/// A `keep`-shaped subtlety worth stating: an entry that is a symlink or a Windows junction is **skipped
/// entirely, never followed**. Modern `fs::remove_dir_all` is *documented* not to follow one, but that
/// is a promise this code has not measured and does not need: the check is explicit here because the
/// CPE-1693 purge measured a bulk-delete mechanism
/// following a junction out of `%TEMP%` and destroying the far side, and a reader of this function should
/// not have to know `remove_dir_all`'s reparse-point semantics to be sure it does not.
fn sweep_stale_sessions(
    root: &Path,
    keep: &Path,
    now: std::time::SystemTime,
    ttl: std::time::Duration,
    examine_budget: usize,
    remove_budget: usize,
) -> usize {
    let Ok(entries) = fs::read_dir(root) else {
        return 0;
    };
    let mut removed = 0usize;
    for entry in entries.take(examine_budget) {
        if removed >= remove_budget {
            break;
        }
        let Ok(entry) = entry else { continue };
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !is_our_temp_dir_name(name) {
            continue;
        }
        let path = entry.path();
        if path == keep {
            continue;
        }
        // `symlink_metadata`, never `metadata`: the question is what the NAME is, not what it points at.
        let Ok(meta) = fs::symlink_metadata(&path) else { continue };
        // **CPE-1929: `is_symlink()` here is subsumed by `!is_dir()` and can never be the decider.**
        // `std`'s `FileType::is_dir` is false for a link on every platform (on Windows it is defined as
        // "a directory and NOT a name-surrogate reparse point"), so on a `symlink_metadata` result
        // `is_symlink() => !is_dir()`: deleting the first disjunct changes no behaviour anywhere, and no
        // fixture can make it the reason a name is skipped. It is kept as a **statement of intent**, not
        // as a second net — the intent being that a symlinked directory is never descended into by this
        // sweep, which is what stops someone later relaxing `symlink_metadata` to `metadata` and
        // silently following links. Untestable on its own, deliberately, and recorded here so a green
        // sabotage on it reads as expected rather than as a missing test. **Measured, not just read off
        // `std`'s definitions:** with this disjunct and its twin in `vault_manager`'s staging-prefix
        // cleanup BOTH deleted, the lib suite is **2,425 passed / 0 failed / 11 ignored** — identical to
        // baseline.
        if meta.file_type().is_symlink() || !meta.is_dir() {
            continue;
        }
        let Ok(modified) = meta.modified() else { continue };
        // `duration_since` errs when `modified` is in the future (clock skew, a copied tree): not stale.
        if now.duration_since(modified).is_ok_and(|age| age >= ttl) && fs::remove_dir_all(&path).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// Which of this session's extraction directories are due for reclamation. Splitting the decision out of
/// the removal is what makes it testable, and keeps `fs::remove_dir_all` off the lock.
///
/// # The grace is measured against the NEWEST entry, and the first version got that wrong
///
/// This is the PR #945 review's blocker, and it is worth recording exactly, because the mistake was not
/// in the code — it was in believing a comment that the code did not implement.
///
/// The first version asked *"is the **oldest** entry older than the grace?"* and called that "nothing
/// younger than the grace is ever touched, which covers the alt-drag staging loop". It does not. The
/// queue is capped, so during a batch the front sits `max_live` entries behind the entry being pushed;
/// the front's age is therefore `max_live × per-entry staging time`, **not** time since the batch
/// started. `FileList.svelte`'s alt-drag stages *every* selected entry in a loop and only calls
/// `startFileDrag(tempPaths)` afterwards, and `extract_archive_entry_any` reopens the archive and streams
/// a fresh decoder from byte zero for each entry — O(n²), seconds per entry on a large `.tar.gz` or
/// `.7z`. So on a 500-entry drag the front went stale mid-loop and one directory was removed on every
/// push after that: 500 paths handed to the OS, ~64 still on disk, **436 files silently dropped**, with
/// no error anywhere because the extraction returned `Ok` before the reap and the reap is `let _ =`.
/// A new silent data-loss path, introduced by the fix for a leak.
///
/// The question that actually protects a burst is *"has this process been quiet?"* — i.e. **how long ago
/// was the most recent extraction started**. Inside the grace window a caller may still be building a
/// batch, and reclaiming any part of it hands the OS paths that no longer exist; once the process has
/// been quiet for the grace, every batch that was in flight has finished by definition. That makes a
/// burst atomically safe *however long it runs and however slow each entry is*, which is the property
/// the per-victim age check could never have.
///
/// # The residual, stated as a NECESSARY condition — the second thing this comment got wrong
///
/// The previous version said the residual needed "more than 512 entries **each** taking over a minute —
/// eight hours of staging". That is a *sufficient* condition presented as a *necessary* one, and the
/// PR #945 re-reviewer measured the difference on the shipped code:
///
/// ```text
/// 601-entry alt-drag: entries 0..599 at 100 ms each, entry 600 takes 61 s
/// PROBE: elapsed since batch start = 121s; reclaimed = 89; batch left = 512; first due = Some("f0")
/// ```
///
/// **89 directories reclaimed out from under a still-staging drag-out, two minutes in.** `quiet` reads
/// only `live.back()`, so **one** inter-entry gap over the grace opens the gate for that single push and
/// the cut happens immediately, however fast every other entry was. Worse, it is not contrived: the
/// O(n²) re-decode noted above means per-entry time *grows within the batch*, so the long gaps arrive
/// exactly when the queue is longest.
///
/// So the true condition is: **more than `max_live` live entries AND any single inter-arrival gap of at
/// least the grace.** There is no signal here that separates "one slow entry" from "the user stopped" —
/// both are simply a gap — so this is a boundary, not a bug that can be closed from inside this
/// function. What was done about it: [`REAP_GRACE`] went from 1 minute to 10, which is where the whole
/// exposure now lives, and
/// `cpe_1786_the_quiet_gate_protects_a_slow_batch_but_one_long_gap_is_the_known_residual` pins **both**
/// halves — a multi-hour batch with every gap under the grace is untouched, and the one-long-gap case
/// still loses the overflow. The loss is asserted rather than described so it cannot rot back into a
/// comforting sentence.
///
/// **The prescribed two-line fix was measured and not taken.** The re-review proposed additionally
/// requiring the popped entry's own age ≥ grace. Timestamps are taken under this queue's own lock
/// immediately before the push, so the queue is monotonic and `front` is never newer than `back`;
/// whenever `quiet` holds, every entry already satisfies that condition. Verified by running the probe
/// above with the extra check compiled in — identical output, `reclaimed = 89`. Shipping it would have
/// added a condition that can never fire, which is the code-shaped version of the identical-timestamp
/// test this ticket was already caught by. The invariant it rests on is pinned by
/// `cpe_1786_the_live_queue_is_monotonic_so_the_front_is_never_newer_than_the_back`, so if a future
/// change makes these timestamps non-monotonic the prescription becomes live again and that test says so.
///
/// `hard_cap` is the other end: a caller that is *never* quiet would otherwise pin the queue open
/// forever, so past it the reclamation runs anyway, down to `hard_cap` rather than to `max_live` — the
/// smallest cut that restores the bound.
fn drain_reapable(
    live: &mut std::collections::VecDeque<(PathBuf, std::time::Instant)>,
    max_live: usize,
    hard_cap: usize,
    grace: std::time::Duration,
    now: std::time::Instant,
) -> Vec<PathBuf> {
    // The newest entry's age is how long this process has been quiet.
    let quiet = live.back().is_none_or(|(_, started)| now.duration_since(*started) >= grace);
    let target = if quiet { max_live } else { hard_cap };
    let mut due = Vec::new();
    // Oldest first: within a session the oldest extraction is the one whose consumer is most likely to
    // be finished with it, and the preview cache re-extracts on demand if it is not.
    while live.len() > target {
        match live.pop_front() {
            Some((path, _)) => due.push(path),
            None => break,
        }
    }
    due
}

/// The [`MAX_LIVE_EXTRACTIONS`] half of the ownership model, applied. Best-effort and single-attempt on
/// purpose: unlike `fsutil::ScratchDir`'s retrying removal this runs **on the extraction path**, where a
/// few hundred milliseconds of retry-and-sleep against a file some other application is holding open
/// would be paid by the user opening a file inside an archive. A directory that resists removal is left
/// for [`sweep_stale_sessions`], which will see it in a later session.
fn note_extraction_dir(dir: PathBuf) {
    let due = {
        // A poisoned lock means some *other* thread panicked while holding it; the queue is still a
        // valid queue, and dropping cleanup on the floor for the rest of the process because of that
        // would reintroduce the leak this function exists to stop.
        let mut live = LIVE_EXTRACTIONS.lock().unwrap_or_else(|e| e.into_inner());
        let now = std::time::Instant::now();
        // Decide **before** pushing: the quiet test asks how long ago the previous extraction started,
        // and the entry being added now would answer "zero" and pin the gate open forever.
        let due = drain_reapable(&mut live, MAX_LIVE_EXTRACTIONS, HARD_CAP_EXTRACTIONS, REAP_GRACE, now);
        live.push_back((dir, now));
        due
    };
    for path in due {
        let _ = fs::remove_dir_all(path);
    }
}

/// Remove everything this process has extracted, for an embedder that knows the session is over (app
/// shutdown). The session-end half of [`session_root`]'s ownership model.
///
/// **Nothing calls this yet, and that is the current behaviour rather than a detail** (PR #945 UAT,
/// which grepped `src-tauri/**/*.rs` and every `*.ts` and found zero call sites). So today *every*
/// session's directories wait for the next launch's sweep or for [`MAX_LIVE_EXTRACTIONS`] — which is
/// correct and bounded, but slower than it needs to be when the answer is already known at shutdown.
/// Wiring it into the app's exit path is **CPE-1797**. This comment says "will" rather than "does" on
/// purpose: a doc that describes an unwired hook as if it were running is how the last round's
/// "nothing here cleans up" sentence stayed true for months while reading as though it were handled.
///
/// Best-effort and idempotent — a session that never extracted anything has nothing to remove, and a
/// directory that will not delete is left to the sweeper exactly as everywhere else here.
/// **The recorded directories are drained and removed, not merely forgotten** (PR #945 review). The first
/// version `clear()`ed the queue and then handed the session root to [`remove_session_tree`] — which is
/// correct in the normal case, where the root removal takes the recorded directories with it, and
/// **worse than a no-op in degraded mode**, where `remove_session_tree` rightly refuses the shared root
/// and the `clear()` has already destroyed the only record of the `e<seq>` directories sitting directly
/// under it. Shutdown removed nothing *and* threw away what could have. Draining first makes the degraded
/// path do real work and costs the normal path nothing.
pub fn cleanup_extraction_scratch() {
    let recorded: Vec<PathBuf> = {
        let mut live = LIVE_EXTRACTIONS.lock().unwrap_or_else(|e| e.into_inner());
        live.drain(..).map(|(path, _)| path).collect()
    };
    cleanup_session(SESSION_ROOT.get().map(PathBuf::as_path), recorded);
}

/// The body of [`cleanup_extraction_scratch`], with the process-global state passed in.
///
/// Split out so it can be tested: the public function reads and mutates statics the whole process
/// shares, so a test that called it would delete other parallel tests' live extractions — which is
/// exactly why the degraded-mode defect this shape fixes went unnoticed.
fn cleanup_session(session: Option<&Path>, recorded: Vec<PathBuf>) {
    // The recorded directories first and unconditionally. In the normal case the tree removal below
    // would have covered them; in degraded mode it refuses, and these are the only thing that can be
    // removed at all.
    for path in recorded {
        let _ = fs::remove_dir_all(path);
    }
    if let Some(session) = session {
        remove_session_tree(session);
    }
}

/// The recursive delete behind [`cleanup_extraction_scratch`], with the one check that makes it safe to
/// arm: **it removes a session directory and refuses anything else.**
///
/// In [`session_root`]'s degraded mode `SESSION_ROOT` holds the *shared* `cpe-archive` root, which
/// belongs to every process on the machine (and, on a Unix `/tmp`, every user on it). Passing that to
/// `remove_dir_all` would turn a shutdown tidy-up into deleting other people's live extractions. The
/// `s` prefix is what distinguishes the two, so it is checked here rather than assumed at the call site —
/// the same lesson `fsutil::ScratchDir::adopt` learned in CPE-1693's PR #934 review, where an
/// unconditional recursive delete sat in a helper and one wrong argument was all it took.
fn remove_session_tree(session: &Path) {
    if session.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with('s')) {
        let _ = fs::remove_dir_all(session);
    }
}

/// Row 1 of the CPE-1733 table — **and the row the PR #906 review corrected, which is the point of the
/// enumeration existing at all.**
///
/// # What this used to claim, and why it was wrong
///
/// The first version of this comment said the directory *"is created by that call and never reused, so
/// nothing — no user, no archive — can have placed a link at the leaf name before we get there"*, and
/// concluded rows 1–5 were **unreachable** by the link hazard. `fs::create_dir_all` does not support that
/// claim. Measured here on Windows:
///
/// ```text
/// [M6] create_dir_all over an EXISTING dir = Ok(())
/// [M6 File::create at a squatted leaf inside a pre-existing dir] Ok(())
///       victim bytes = Some("")        <- the victim was TRUNCATED through the link
/// [M6d] create_dir_all over a DIRECTORY LINK = true
/// ```
///
/// `create_dir_all` succeeds on a directory that already exists **and on a symlink/junction standing in
/// for one**, so it establishes nothing about who made the directory. What actually protected rows 1–5
/// was a *platform* fact rather than anything this code did: on Windows `%TEMP%` is per-user. On Linux
/// `std::env::temp_dir()` is `$TMPDIR` or `/tmp` — world-writable and sticky — where the PID is public,
/// the sequence restarts at 0, the directory is never cleaned, and the **leaf name is archive-controlled,
/// so an attacker who supplies the archive already knows what to call the link.** That is CWE-377
/// (insecure temporary file) into CWE-59 (link following), and "we choose the path" was generalised one
/// step past its evidence into "nobody can have been there first" — the exact defect shape this ticket
/// was filed about.
///
/// # What it does now
///
/// The shared `cpe-archive` root still has to be `create_dir_all`'d (it is shared across processes by
/// design) and is link-checked before use, but the **per-extraction directory is created exclusively**
/// with `fs::create_dir`, which is the primitive that actually carries the claim. Measured, same run:
///
/// ```text
/// [M6b] fs::create_dir (exclusive) over the SAME pre-existing dir = Err(kind AlreadyExists)  [os error 183]
/// [M6d] fs::create_dir over a DIRECTORY LINK                      = Err(AlreadyExists)
/// [M6c] fs::create_dir at a FRESH name                            = true
/// ```
///
/// Both squatting shapes now bounce, and a bounced name is retried with the next sequence number rather
/// than failing the extraction. Once `create_dir` returns `Ok`, this process is the only thing that has
/// ever been inside that directory, so the leaf — and therefore rows 2–5 — really is unreachable by a
/// pre-placed link. That sentence is now earned rather than assumed.
///
/// **CPE-1786 raised the floor under that argument** without changing it: the exclusive `create_dir` is
/// still what carries the claim, but it now happens inside [`session_root`] — a directory created
/// exclusively by this process, whose default permissions keep other users out of it on a shared `/tmp`
/// in the first place. The squat hazard is therefore narrower than it was (a same-user process), and the
/// guard against it is unchanged, because "narrower" is not "gone".
///
/// # Residuals, stated rather than implied
///
/// - **The `cpe-archive` root itself is still shared.** On a multi-user `/tmp` another user can create it
///   first as an ordinary directory. The link check refuses the symlink case; the ordinary-directory
///   case is not refused, because that is the normal state of affairs on every second run. The exclusive
///   subdirectory creation is what makes that safe, not the root.
/// - **The session directory is no longer predictable**, which retires the denial-of-service residual
///   this list used to carry. `<pid>-<seq>` could be pre-created in bulk by a hostile local user to force
///   the [`TEMP_TARGET_ATTEMPTS`] error; `s<pid>-<random>` cannot be guessed, and inside it the sequence
///   starts from a directory that was empty a moment ago.
/// - **Cleanup is owned, not absent.** The pre-CPE-1786 version of this list said *"nothing here cleans
///   up"*, and it was the truest sentence in the file: 1,394,403 leftover directories. What owns them
///   now, and why it cannot simply be a `Drop` guard, is on [`session_root`].
///
/// # One evaluation-order change, deliberately left in place (CPE-1927 round 2)
///
/// Splitting the body out moved `session_root()?` **in front of** the `file_name()` validation, which the
/// old single function ran first. So an entry with no `file_name()` — `..`, `.`, `""` — now claims the
/// session root and can run the stale-session sweep before returning the same `"invalid entry name"`.
///
/// It is benign, and precisely: **nothing derived from `inner` reaches `session_root()`**, which only ever
/// builds `%TEMP%/cpe-archive/s<pid>-<random>` out of our own pid and RNG, so no attacker-controlled path
/// is created and the extraction still refuses. The one externally visible difference is the error
/// *string*: if the shared root is a link, the refusal now surfaces
/// [`refuse_link_at_new_file`]'s message instead of `"invalid entry name"` — the more informative of the
/// two, since it names a real environmental hazard rather than masking it behind a name complaint.
///
/// It was left rather than reordered because **the ordering is not pinnable by a test in this suite**, and
/// this is the ticket about not shipping guards that prove nothing. The only observable is a
/// once-per-process side effect on a `OnceLock` every sibling test races to initialise, so a test
/// asserting "`session_root()` was not called" would be exactly the shared-mutable-fixture shape CPE-1927
/// exists to delete. An unverifiable edit to a refusal path is the worse trade; this comment is the
/// honest one. If a future change gives `session_root()` a caller-visible cost or a side effect that
/// **is** derived from `inner`, reorder it then — the fix is to have this wrapper pass an
/// [`ExtractNamespace`] and let the body resolve the root after validating.
fn temp_extract_target(inner: &str) -> Result<std::path::PathBuf, String> {
    temp_extract_target_in(&session_root()?, &EXTRACT_SEQ, inner)
}

/// A caller-supplied extraction namespace: **the root to number inside, and the counter that hands out
/// the numbers.** `None` means the process-global pair ([`session_root`] + [`EXTRACT_SEQ`]) — which is
/// what production always passes, via [`temp_extract_target`].
///
/// It exists for one reason (CPE-1927), and the reason is *not* that the sharing is wrong. Both halves of
/// the process-global pair are deliberately shared, and surviving that sharing is exactly what
/// [`temp_extract_target_in`]'s atomic `fetch_add` + exclusive `create_dir` walk is for; the app runs
/// concurrent extractions through it every day. The problem is one-sided: a **test** that stages the
/// CWE-377 hazard has to predict which `e<seq>` name the extraction is about to claim, and a counter every
/// sibling test is also moving cannot be predicted. Measured on this suite before this seam existed,
/// `row1_a_squatted_temp_directory_is_stepped_over_not_written_into` silently lost names out of its
/// squatted block in 2 of 7 full-suite runs and was raced clean past the block in 1 of 7 — drifting toward
/// proving nothing, with a `skip_notice!` (a passing test) as its only signal.
///
/// Handing that one test a namespace of its own **removes** the sharing rather than serialising around it.
/// A `HOME_ENV_LOCK`-style mutex would have been the other option and is worse here: it leaves the
/// prediction in place (it only makes it likelier to hold), it puts an ordering requirement on every
/// future test that extracts anything, and it would have hidden the coupling instead of deleting it.
type ExtractNamespace<'a> = Option<(&'a Path, &'a std::sync::atomic::AtomicU64)>;

/// [`temp_extract_target`]'s body with the namespace passed in explicitly — see [`ExtractNamespace`].
fn temp_extract_target_in(
    session: &Path,
    seq_source: &std::sync::atomic::AtomicU64,
    inner: &str,
) -> Result<std::path::PathBuf, String> {
    let base = Path::new(inner)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .ok_or_else(|| "invalid entry name".to_string())?;
    for _ in 0..TEMP_TARGET_ATTEMPTS {
        // Per-extraction unique subdir (monotonic seq inside this session's private root) so the basename
        // is preserved for the opened file while concurrent extractions can never collide (CPE-1195).
        let seq = seq_source.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = session.join(format!("e{seq}"));
        match fs::create_dir(&dir) {
            // Exclusive: this returning `Ok` is the whole basis for rows 2–5 being unguarded.
            Ok(()) => {
                note_extraction_dir(dir.clone());
                return Ok(dir.join(base));
            }
            // Occupied by a concurrent run or a squatter — both want the same answer.
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            // Our session root is gone: another instance's sweeper decided we were dead (a session that
            // extracted nothing for `SESSION_TTL`), or a user emptied `%TEMP%`. Re-create it and carry
            // on rather than failing an extraction over a cleanup decision made elsewhere.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let _ = fs::create_dir_all(session);
                continue;
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    Err(format!(
        "could not claim a private extraction directory under \"{}\" after {TEMP_TARGET_ATTEMPTS} attempts — \
         every name tried was already taken. Nothing was extracted; clearing that folder should fix it",
        session.display()
    ))
}

/// Extract a single entry of a zip to a temp file and return its path (CPE-242). Read-only: the temp
/// copy is what opens, not the archived bytes.
pub fn extract_archive_entry(zip: &str, inner: &str) -> Result<String, String> {
    extract_archive_entry_in(zip, inner, None)
}

/// [`extract_archive_entry`]'s body with the extraction namespace passed in — see [`ExtractNamespace`]
/// for why the seam exists (CPE-1927). The public function above is a one-line dispatcher into this, so
/// a test driving it with its own root and counter is exercising the production path byte for byte,
/// `File::create` and all: that is the whole point, because the bug row 1 guards is a write that lands
/// somewhere else entirely and still returns `Ok`.
fn extract_archive_entry_in(zip: &str, inner: &str, ns: ExtractNamespace<'_>) -> Result<String, String> {
    let file = fs::File::open(zip).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    // The frontend uses "/"; some zips store "\" — try the given name then the backslash variant.
    let backslashed = inner.replace('/', "\\");
    let idx = archive
        .index_for_name(inner)
        .or_else(|| archive.index_for_name(&backslashed))
        .ok_or_else(|| format!("entry not found: {inner}"))?;
    let mut entry = archive.by_index(idx).map_err(|e| e.to_string())?;

    // Row 2 of the CPE-1733 table: unguarded on purpose — `temp_extract_target` is a fresh, per-call,
    // app-owned directory, and the leaf is `file_name()` of the entry, so no user and no archive can have
    // placed a link at this path.
    let out = match ns {
        Some((session, seq_source)) => temp_extract_target_in(session, seq_source, inner)?,
        None => temp_extract_target(inner)?,
    };
    let mut w = fs::File::create(&out).map_err(|e| e.to_string())?;
    std::io::copy(&mut entry, &mut w).map_err(|e| e.to_string())?;
    Ok(out.to_string_lossy().to_string())
}

/// Extract a single entry from a TAR stream (optionally gzip-decompressed by the caller) to `out`.
/// Returns whether the entry was found. Mirrors [`extract_archive_entry`]'s name matching: the frontend
/// uses "/"; some tarballs store "\" — try the given name then the backslash variant.
fn extract_tar_entry<R: std::io::Read>(reader: R, inner: &str, out: &Path) -> Result<bool, String> {
    let backslashed = inner.replace('/', "\\");
    let mut archive = tar::Archive::new(reader);
    for entry in archive.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let name = entry
            .path()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        if name == inner || name == backslashed {
            // Row 3 of the CPE-1733 table: `out` is always a `temp_extract_target` (its only caller is
            // `extract_archive_entry_any`), so it is app-owned — unguarded for the row-2 reason.
            let mut w = fs::File::create(out).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut w).map_err(|e| e.to_string())?;
            return Ok(true);
        }
    }
    Ok(false)
}

/// Extract a single entry from a `.7z` to `out`. Returns whether the entry was found. Applies the same
/// [`entry_name_is_safe`] guard as [`extract_7z_safe`] to every entry name the archive claims, not just
/// the caller's `inner` (CPE-628/1180): `sevenz-rust` doesn't validate names itself.
fn extract_7z_entry(path: &str, inner: &str, out: &Path) -> Result<bool, String> {
    let backslashed = inner.replace('/', "\\");
    let mut found = false;
    // `decompress_file_with_extract_fn` needs an existing directory to build entry dest-paths against
    // even though our callback ignores that argument and writes straight to `out`.
    let scratch = out.parent().map(Path::to_path_buf).unwrap_or_else(std::env::temp_dir);
    catch_sevenz_panic(|| {
        sevenz_rust::decompress_file_with_extract_fn(path, &scratch, |entry, reader, _dest| {
            let name = entry.name();
            if !found && entry_name_is_safe(name) && (name == inner || name == backslashed) {
                // Row 4 of the CPE-1733 table: `out` is a `temp_extract_target`, app-owned — unguarded for
                // the row-2 reason.
                let mut w = fs::File::create(out).map_err(sevenz_rust::Error::io)?;
                std::io::copy(reader, &mut w).map_err(sevenz_rust::Error::io)?;
                found = true;
                Ok(false) // stop scanning the rest of this block; we have what we need
            } else {
                Ok(true) // keep scanning
            }
        })
        .map_err(|e| e.to_string())
    })?;
    Ok(found)
}

/// Extract a single entry from any supported non-zip archive format to a temp file and return its path
/// (CPE-1180): tar, gzip-compressed tar (.tar.gz/.tgz), and 7-Zip. Zip delegates to the existing
/// [`extract_archive_entry`] (kept separate since the `zip` crate already indexes by name efficiently).
/// Mirrors `extract_archive_entry`'s contract — a temp file named `<basename>` in a private directory
/// under `%TEMP%/cpe-archive` ([`temp_extract_target`]) — so
/// the frontend's open-leaf flow doesn't need to know which extractor served it. `inner` is validated
/// with [`entry_name_is_safe`] up front so a path-traversal entry name is rejected before any extraction
/// is attempted, regardless of format.
pub fn extract_archive_entry_any(path: &str, inner: &str) -> Result<String, String> {
    if !entry_name_is_safe(inner) {
        return Err(format!("unsafe entry name: {inner}"));
    }
    let lower = path.to_lowercase();
    let out = temp_extract_target(inner)?;
    let found = if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        let file = fs::File::open(path).map_err(|e| e.to_string())?;
        extract_tar_entry(flate2::read::GzDecoder::new(file), inner, &out)?
    } else if lower.ends_with(".tar") {
        let file = fs::File::open(path).map_err(|e| e.to_string())?;
        extract_tar_entry(file, inner, &out)?
    } else if lower.ends_with(".7z") {
        extract_7z_entry(path, inner, &out)?
    } else if lower.ends_with(".rar") {
        // RAR has no ZIP-style directory; the STORED-entry extractor writes its own temp file below.
        return extract_rar_entry(path, inner);
    } else {
        return extract_archive_entry(path, inner);
    };
    if found {
        Ok(out.to_string_lossy().to_string())
    } else {
        Err(format!("entry not found: {inner}"))
    }
}

/// Extract a single **STORED** entry from a `.rar` to a temp file and return its path (CPE-1360). RAR's
/// compression is proprietary with no free decoder, so only uncompressed (STORE) entries can be served;
/// a compressed entry is a clean `Err` from [`crate::rar::rar_extract_entry`]. Mirrors
/// [`extract_archive_entry`]'s contract — a temp file named `<basename>` in a private directory under
/// `%TEMP%/cpe-archive` ([`temp_extract_target`]) — so the
/// frontend's open-leaf / extract-for-preview flow doesn't need to know which extractor served it. The
/// entry name is [`entry_name_is_safe`]-validated before anything is written.
pub fn extract_rar_entry(path: &str, inner: &str) -> Result<String, String> {
    if !entry_name_is_safe(inner) {
        return Err(format!("unsafe entry name: {inner}"));
    }
    let bytes = crate::rar::rar_extract_entry(path, inner)?;
    // Row 5 of the CPE-1733 table: `fs::write` writes through a link exactly like `File::create` does
    // (measured), but `out` is a `temp_extract_target` — app-owned, so unguarded for the row-2 reason.
    let out = temp_extract_target(inner)?;
    fs::write(&out, &bytes).map_err(|e| e.to_string())?;
    Ok(out.to_string_lossy().to_string())
}

/// Recursively add `src` to an open zip under the archive path `name_in_zip`. Directories become explicit
/// entries so empty folders survive the round trip. Never packs the output archive into itself (CPE-632).
fn zip_add_path(
    writer: &mut zip::ZipWriter<fs::File>,
    src: &Path,
    name_in_zip: &str,
    opts: zip::write::FileOptions<'_, ()>,
    skip: Option<&Path>,
) -> Result<(), String> {
    if let (Some(skip), Ok(canon)) = (skip, src.canonicalize()) {
        if canon == skip {
            return Ok(());
        }
    }
    let meta = fs::symlink_metadata(src).map_err(|e| e.to_string())?;
    if meta.is_dir() {
        writer.add_directory(format!("{name_in_zip}/"), opts).map_err(|e| e.to_string())?;
        let mut children: Vec<_> = fs::read_dir(src).map_err(|e| e.to_string())?.filter_map(|e| e.ok()).collect();
        children.sort_by_key(|e| e.file_name());
        for child in children {
            let child_name = child.file_name().to_string_lossy().to_string();
            zip_add_path(writer, &child.path(), &format!("{name_in_zip}/{child_name}"), opts, skip)?;
        }
    } else {
        writer.start_file(name_in_zip, opts).map_err(|e| e.to_string())?;
        let mut f = fs::File::open(src).map_err(|e| e.to_string())?;
        std::io::copy(&mut f, writer).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Pack the given files/folders into a new deflated `.zip` at `dest` (CPE-251). Returns the created path.
pub fn compress_to_zip(paths: &[String], dest: &str) -> Result<String, String> {
    if paths.is_empty() {
        return Err("nothing to compress".into());
    }
    refuse_link_at_new_file(Path::new(dest))?; // row 6
    let file = fs::File::create(dest).map_err(|e| e.to_string())?;
    let mut writer = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    // Canonical path of the output archive so the walk can skip it if it sits inside a source (CPE-632).
    let dest_canon = Path::new(dest).canonicalize().ok();
    for p in paths {
        let src = Path::new(p);
        let name = src
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .ok_or_else(|| format!("invalid path: {p}"))?;
        zip_add_path(&mut writer, src, &name, opts, dest_canon.as_deref())?;
    }
    writer.finish().map_err(|e| e.to_string())?;
    Ok(dest.to_string())
}

/// Create a valid *empty* zip archive at `dest` (CPE-1161). An empty file is NOT a valid `.zip` — a
/// real archive needs at least the End-Of-Central-Directory record, which `ZipWriter::finish` writes.
/// Uses `create_new` so it fails atomically rather than clobbering an existing file (mirrors the
/// explorer's `create_file`). Returns the created path.
///
/// **Row 7 of the CPE-1733 table, and the only site there that was already safe.** Measured on Windows:
/// `create_new` on a *dangling* link fails `AlreadyExists (os error 80)` without creating the link's
/// target, and on a *live* link fails the same way with the target's bytes untouched — so unlike its five
/// sibling creation sites this one never wrote through a link. What it did do was **explain itself
/// wrongly**: `AlreadyExists` stringifies to *"The file exists"*, which is precisely the wording
/// `fsutil::create_slot_refusal`'s doc calls out as sending the user to delete a file at a name that
/// actually holds a link to somewhere else. The guard below is therefore a **message** fix in front of a
/// working belt, not a second belt — `create_new` stays, and stays the thing that makes this atomic.
///
/// **CPE-1744 finished that message fix.** Row 7's guard reworded only the *link* case; onto a plain
/// existing file this still returned the raw `Err("The file exists. (os error 80)")` — measured for this
/// ticket — which names neither the path nor which of the two files (the one already there, or the
/// archive being created) is meant. That is the identical defect one step over from the one row 7 was
/// filed about, so the `AlreadyExists` arm now says both.
pub fn create_empty_zip(dest: &str) -> Result<String, String> {
    refuse_link_at_new_file(Path::new(dest))?; // row 7 — see above: wording, not safety
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dest)
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                // Reached only when row 7's guard said "not a link" — so this really is occupancy, and it
                // can say so plainly. (A link at `dest` never gets here; it is refused above, in link
                // wording, which `row7_…`/`every_guarded_row_…` pin.)
                format!(
                    "the new empty archive \"{dest}\" was not created: something already exists at that \
                     name. This action only ever creates a NEW archive and never replaces a file that is \
                     already there — rename or remove that one first"
                )
            } else {
                e.to_string()
            }
        })?;
    let writer = zip::ZipWriter::new(file);
    writer.finish().map_err(|e| e.to_string())?;
    Ok(dest.to_string())
}

/// Recursively add `src` to a tar builder as `name_in_tar`, adding directory entries so empty folders
/// survive. Never packs the output archive into itself (CPE-632) — mirrors [`zip_add_path`].
fn tar_add_path<W: std::io::Write>(
    builder: &mut tar::Builder<W>,
    src: &Path,
    name_in_tar: &str,
    skip: Option<&Path>,
) -> Result<(), String> {
    if let (Some(skip), Ok(canon)) = (skip, src.canonicalize()) {
        if canon == skip {
            return Ok(());
        }
    }
    let meta = fs::symlink_metadata(src).map_err(|e| e.to_string())?;
    if meta.is_dir() {
        builder.append_dir(name_in_tar, src).map_err(|e| e.to_string())?;
        let mut children: Vec<_> = fs::read_dir(src).map_err(|e| e.to_string())?.filter_map(|e| e.ok()).collect();
        children.sort_by_key(|e| e.file_name());
        for child in children {
            let child_name = child.file_name().to_string_lossy().to_string();
            tar_add_path(builder, &child.path(), &format!("{name_in_tar}/{child_name}"), skip)?;
        }
    } else {
        builder.append_path_with_name(src, name_in_tar).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Pack the given files/folders into a new gzip-compressed tarball at `dest` (CPE-908). Returns the path.
pub fn compress_to_targz(paths: &[String], dest: &str) -> Result<String, String> {
    if paths.is_empty() {
        return Err("nothing to compress".into());
    }
    refuse_link_at_new_file(Path::new(dest))?; // row 8
    let file = fs::File::create(dest).map_err(|e| e.to_string())?;
    let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    let mut builder = tar::Builder::new(encoder);
    let dest_canon = Path::new(dest).canonicalize().ok();
    for p in paths {
        let src = Path::new(p);
        let name = src
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .ok_or_else(|| format!("invalid path: {p}"))?;
        tar_add_path(&mut builder, src, &name, dest_canon.as_deref())?;
    }
    builder.into_inner().map_err(|e| e.to_string())?.finish().map_err(|e| e.to_string())?;
    Ok(dest.to_string())
}

/// Pack files/folders into `dest`, choosing the format by `dest`'s extension: `.zip` → zip,
/// `.tar.gz`/`.tgz` → gzip tarball (CPE-908). An unrecognised extension is a clear error.
pub fn compress_archive(paths: &[String], dest: &str) -> Result<String, String> {
    let lower = dest.to_ascii_lowercase();
    if lower.ends_with(".zip") {
        compress_to_zip(paths, dest)
    } else if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        compress_to_targz(paths, dest)
    } else {
        Err(format!("unsupported archive format for '{dest}' (use .zip or .tar.gz)"))
    }
}

/// Pack files/folders into a **password-protected** (AES-256) `.zip` at `dest` (CPE-909). Returns the path.
/// Reading it back requires the same password — see [`extract_zip_encrypted`].
pub fn compress_to_zip_encrypted(paths: &[String], dest: &str, password: &str) -> Result<String, String> {
    if paths.is_empty() {
        return Err("nothing to compress".into());
    }
    if password.is_empty() {
        return Err("a password is required for an encrypted archive".into());
    }
    refuse_link_at_new_file(Path::new(dest))?; // row 9
    let file = fs::File::create(dest).map_err(|e| e.to_string())?;
    let mut writer = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .with_aes_encryption(zip::AesMode::Aes256, password);
    let dest_canon = Path::new(dest).canonicalize().ok();
    for p in paths {
        let src = Path::new(p);
        let name = src
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .ok_or_else(|| format!("invalid path: {p}"))?;
        zip_add_path(&mut writer, src, &name, opts, dest_canon.as_deref())?;
    }
    writer.finish().map_err(|e| e.to_string())?;
    Ok(dest.to_string())
}

/// Extract a password-protected `.zip` at `path` into `dest` with `password` (CPE-909). A wrong password
/// is a clear error — `by_index_decrypt` fails on the first entry it cannot open, aborting the whole run
/// via `?`, exactly as before this function was rewritten.
///
/// **CPE-1807: no longer its own loop.** This used to run its own `for i in 0..archive.len()` —
/// [`extract_zip_archive_stream`]'s own doc named it the "fourth, unmerged" zip extractor — because it
/// predates [`ArchiveReport`] and had nowhere to put a per-entry skip note. It now calls that shared loop
/// with `password` set, the same delegation [`extract_zip_encrypted_streamed`] and [`extract_archive`]'s
/// zip branch (row 23) already used. **The skip stays silent here** — the signature is unchanged, so
/// there is still nowhere to put the note; only the *loop body* is shared, not the return type.
///
/// **What the merge changes, audited guard by guard rather than assumed safe:**
/// - [`entry_name_is_safe`] (zip-slip / reserved-name refusal) and [`entry_dir_action`]/[`entry_sink_action`]
///   (leaf-link + per-component containment, rows 15/18 of the CPE-1733 table) were already applied
///   identically by the old loop, in the same order; unchanged by the merge.
/// - **The merge changes what this path can DO, not what it previously failed to refuse.** The old loop
///   pushed every entry — symlink-flagged or not — through `File::create` + `io::copy`, so a symlink
///   entry landed as an ordinary file containing the target path as literal text: one of the three
///   policies [`link_target_action`]'s own doc names as SAFE for CPE-1774. It could not create a real
///   link at all, on any target, benign or escaping — there was no hole here to close. The merge makes
///   this one-shot path able to create a real symlink for the first time ([`link_target_action`], reached
///   via `entry.is_symlink()` in the shared loop, the same capability move CPE-1759 made for the one-shot
///   *plain* zip path), and it is [`link_target_action`]'s containment check — not the previous inability
///   to create a link — that keeps an escaping target from working post-merge. This reads correctly on an
///   AES entry because a zip entry's symlink flag and declared target are central-directory metadata the
///   `zip` crate does not encrypt (only the entry's *content* stream is AES-protected); the streamed
///   encrypted extractor already proved the guard fires on real ciphertext, since it already routed
///   through this loop.
/// - **User-visible consequence of that widening — fixed by CPE-1837, documented here rather than
///   re-derived:** an entry the old loop always wrote as a readable text file — including one whose
///   target escapes `dest` — can now instead be refused by [`link_target_action`]. Before CPE-1837 that
///   refusal vanished with no note, because this signature predated [`ArchiveReport`] and had nowhere to
///   put one — a successful-looking extraction quietly missing a file, exactly the hazard shape this
///   module reports everywhere else it can. It now returns [`ArchiveExtractOutcome`], whose `report`
///   carries the same `skipped`/`errors` the streamed path already surfaced, so this path can too.
/// - **Newly gained: unix permission-bit restoration.** Same metadata reasoning — `unix_mode()` is
///   central-directory data, unaffected by AES encryption of the content stream.
/// - **Nothing password-specific is lost.** `password` is threaded straight into the loop's own
///   `archive.by_index_decrypt(i, pw.as_bytes())` call (see [`extract_zip_archive_stream`]) — the
///   identical call this function used to make itself, at the identical point in the per-entry sequence —
///   so decryption happens exactly where and how it did before the merge.
pub fn extract_zip_encrypted(path: &str, dest: &str, password: &str) -> Result<ArchiveExtractOutcome, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let dest_path = Path::new(dest);
    let never = AtomicBool::new(false);
    let report = extract_zip_archive_stream(&mut archive, dest_path, Some(password), &never, &mut |_| {})?;
    Ok(ArchiveExtractOutcome { dest: dest.to_string(), report })
}

/// The one refusal message for an entry name this module will not write, so the tar sinks and the zip
/// sinks say the *same words* about the same shape (CPE-1773 — "the user does not think in sinks").
const UNSAFE_NAME_SKIP: &str = "unsafe entry name, skipped";

/// **The whole per-entry decision for the two TAR sinks** (CPE-1773 + CPE-1774), pure so it can be
/// tested without a tar in hand.
///
/// `tar`'s `Entry::unpack_in` owns the write, so unlike rows 15/16 there is no `File::create` to
/// intercept — the questions have to be asked *before* handing the entry over, and a refused entry is
/// skipped, which is exactly the zip loops' contract.
///
/// Three questions, and the first two were being asked nowhere on this path:
///
/// 1. **Is the name one we will write?** [`entry_name_is_safe`]. `unpack_in` guards traversal and
///    nothing else, so measured on `main` a `.tar`/`.tar.gz`/`.tgz` entry named `file:stream` reached
///    NTFS as an **alternate data stream** of a neighbouring file — no visible file, `errors: []`,
///    `done: 1`. `..evil`, `con`, `x.` and `x ` were written literally, and `nul` aborted the whole
///    extraction with a hard `Err`, taking every other entry with it. All six now answer as they do in
///    zip.
/// 2. **Is something already sitting at that name, and is it a link?** [`entry_sink_action`] for a file
///    entry, [`entry_dir_action`] for a directory one — the same pair rows 15/16/19/20 ask. **This is
///    CPE-1759's half**, and it is the one question tar answers *destructively* rather than by following
///    or refusing; see [`tar_unpack`] and the tar bullet in the section comment for what
///    `tar-0.4.46/src/entry.rs:644-662` does and why the victim's bytes survive while the user's link
///    does not.
/// 3. **If it is a link, does its target stay inside `dest`?** [`link_target_action`] for a symlink,
///    [`hard_link_target_action`] for a hard link — two functions because the crate resolves the two
///    targets against different bases. `unpack_in` calls `symlink(&src, dst)` with the raw bytes and
///    validates nothing (measured, not inferred: both tar paths created a real link reading a file
///    outside the extraction folder — see [`link_target_action`]); it *does*
///    canonicalisation-validate a hard link's, but by **aborting the whole run**, which is the half
///    CPE-1759 converted to a skip.
/// 4. Anything else is written.
///
/// **Question 2 before question 3, and both after question 1** — the same order the ZIP loop uses, and
/// load-bearing for the same reason [`entry_sink_action`]'s own two halves are ordered: a link already in
/// the user's folder and a link the archive is asking us to create are different hazards with different
/// remedies, and each must stay reported as itself so a mutation of either guard turns a *distinct* test
/// red.
///
/// **Question 2's containment half overlaps a check `unpack_in` also makes, and that is the point.**
/// `validate_inside_dst` already refuses an entry addressed through a symlinked intermediate directory —
/// but it refuses it as an `io::Error`, which both tar sinks propagate with `?`, taking the whole archive
/// down. Asking [`entry_sink_action`] first converts that abort into a counted skip without changing the
/// verdict, which is the same alignment CPE-1759 makes for zip. `unpack_in`'s check stays as the belt
/// behind it.
///
/// **An unreadable entry path fails closed.** The callers pass what `entry.path()` gave them, and its
/// failure case is the empty string; `entry_name_is_safe("")` is `false`, so such an entry is skipped
/// and recorded rather than handed to `unpack_in` under a name we could not read.
///
/// **`dest.join(name)` — with no `\`-to-`/` normalisation.** That normalisation was a live escape on
/// POSIX; the measurement and the reasoning are on [`link_target_action`]. This is also exactly what the
/// sibling ZIP loop ([`extract_zip_archive_stream`]) already does for its own `out`.
fn tar_entry_refusal(dest: &Path, name: &str, kind: TarEntryKind<'_>) -> EntrySlotAction {
    if !entry_name_is_safe(name) {
        return EntrySlotAction::Skip(UNSAFE_NAME_SKIP.to_string());
    }
    let out = dest.join(name);
    let slot = match kind {
        TarEntryKind::Directory => entry_dir_action(dest, &out),
        _ => entry_sink_action(dest, &out),
    };
    match slot {
        EntrySlotAction::Write => {}
        // **Both arms are propagated, not collapsed** — and the first version of CPE-1759 collapsed them,
        // which is the whole reason this paragraph exists. Before this ticket the `Abort` arm here was
        // *dead*: the only producer was `link_target_action`, which never returns it. Adding
        // `entry_sink_action` above made it live, and mapping it to a skip meant an unreadable slot —
        // classified as a **failure** by [`EntrySlotAction`]'s own doc, and refused by all three zip
        // sinks — dropped a tar entry silently while the run returned `Ok`. That is UAT finding 6
        // verbatim, reintroduced three functions from the comment warning about it.
        //
        // The rule this module states is that a refusal we *chose* is a skip and a failure the
        // filesystem handed us is not, and "`unpack_in` owns the write" is not an exception to it: the
        // caller can record a failure just as easily as a skip, and a slot we could not classify is not
        // a refusal we chose — it is a question the filesystem would not answer. **CPE-1935 kept that
        // distinction and separated it from a second one it had been carrying** (whether the *run*
        // stops), so what arrives here as `Fail` is recorded per entry; see [`EntrySlotAction`].
        decided => return decided,
    }
    let (target, decision) = match kind {
        TarEntryKind::Symlink(t) => (t, link_target_action(dest, &out, t)),
        // The base is `dest`, not the link's own parent, because that is what the crate resolves a hard
        // link's target against: `unpack_in` passes the canonical destination root as `target_base` and
        // computes `p.join(src)` (`tar-0.4.46/src/entry.rs:529-547`). A guard measuring from the wrong
        // base is worth one `..` of real escape per level of disagreement — see [`link_target_action`]
        // for the round of this that shipped a live hole.
        TarEntryKind::HardLink(t) => (t, hard_link_target_action(dest, t)),
        TarEntryKind::Directory | TarEntryKind::Other => return EntrySlotAction::Write,
    };
    if target.as_os_str().is_empty() {
        return EntrySlotAction::Skip(EMPTY_LINK_SKIP.to_string());
    }
    // Neither target guard can return `Abort` — both are pure containment verdicts over
    // `confined_to`, which fails *closed* into `Skip` rather than reporting that it could not tell.
    // Propagated whole anyway, so that stays true by construction rather than by this comment.
    decision
}

/// The containment decision for a **hard link** entry's target (CPE-1759).
///
/// CPE-1774 deliberately left hard links to `unpack_in`'s own `validate_inside_dst`, on the sound
/// grounds that a second guard for one question is a liability. That reasoning was about *safety*, and
/// it still holds — nothing escapes either way. It said nothing about *how* the refusal arrives, which is
/// this ticket's question, and the answer was measured on both tar paths:
///
/// ```text
/// [HL escaping streamed=false] outcome=Err("failed to unpack `…\dst\hard`")  ok.txt=false
/// [HL escaping streamed=true ] outcome=Err("failed to unpack `…\dst\hard`")  ok.txt=false
/// [HL absolute streamed=false] outcome=Err("failed to unpack `…\dst\hard`")  ok.txt=false
/// [HL absolute streamed=true ] outcome=Err("failed to unpack `…\dst\hard`")  ok.txt=false
/// ```
///
/// One hostile hard-link entry took the whole archive down, `ok.txt` included, with a message naming a
/// path and no reason. Asking here first turns that into a counted skip with the same wording every other
/// escaping link entry gets, and leaves `validate_inside_dst` in place as the belt behind it.
///
/// **`\`-to-`/` normalisation, over-broad on POSIX, for [`link_target_action`]'s reason** — a target
/// literally named `..\secret` is a legal filename there and is refused. One-directional: a false
/// refusal the user is told about, never a false permit.
///
/// **What this does NOT convert**, because it is a failure rather than a refusal: a hard link whose
/// target simply is not there (measured — `[HL nonexistent-inside]` fails on both paths, and it is the
/// same `Err` shape). `fs::hard_link` owns that write and there is no way to predict its outcome without
/// attempting it, so it stays a failure, exactly like a `File::create` or `io::copy` failure at rows
/// 15/16/19/20.
///
/// **CPE-1935 changed what a failure then costs, not which side of this line it falls on.** The rule
/// used to read *refusals skip, failures abort*; it now reads *refusals skip, failures are counted
/// against their own entry, and only a run-scoped problem aborts* — so `[HL nonexistent-inside]` lands
/// as `failed == 1` with the rest of the archive extracted. Whether that entry is a **skip** or a
/// **failure** is still this function's question and still answered the same way: the counts the user
/// reads mean different things, so misfiling one as the other is as wrong as it ever was.
fn hard_link_target_action(dest: &Path, target: &Path) -> EntrySlotAction {
    let normalized = target.to_string_lossy().replace('\\', "/");
    if crate::fsutil::confined_to(&dest.join(&normalized), dest) {
        EntrySlotAction::Write
    } else {
        EntrySlotAction::Skip(escaping_link_target_message(dest, target))
    }
}

/// What a tar entry is, as far as [`tar_entry_refusal`] needs to care: the three shapes that get three
/// different questions asked of them.
///
/// Split out by CPE-1759, which needed the **directory/not-directory** distinction the previous
/// `Option<&Path>` parameter could not carry: a link at a *directory* entry's name is `create_dir_all`
/// redirection and only costs the user something when it redirects out of `dest` ([`entry_dir_action`]),
/// while a link at a *file* entry's name is destroyed outright ([`entry_sink_action`]). Every other tar
/// entry type — hard link, fifo, device node, and the unrecognised typeflags a POSIX implementation must
/// treat as regular files — is `Other`, and gets the file treatment, because `unpack_in`'s
/// unlink-and-replace runs for all of them.
#[derive(Clone, Copy)]
enum TarEntryKind<'a> {
    Directory,
    /// The declared link target — possibly empty, which [`tar_entry_refusal`] refuses; see
    /// [`EMPTY_LINK_SKIP`].
    Symlink(&'a Path),
    /// Same, for a hard link. Separate from [`TarEntryKind::Symlink`] because the crate resolves the two
    /// targets against **different bases** — see [`hard_link_target_action`].
    HardLink(&'a Path),
    Other,
}

/// The refusal for a link entry that declares no readable target (CPE-1774 review nit 4).
///
/// **This is a fix, not a wording change.** The first version returned an empty target here and let it
/// through on the reasoning that `unpack_in` "gets to fail on its own terms" — which it does, with
/// *"symlink destination is empty"*, and [`extract_tar_stream`] propagates that with `?`. So one crafted
/// entry still took the whole streamed run down, which is the exact failure mode CPE-1773 removed for
/// `nul` two paragraphs earlier. It is not silent (the user sees `failed: 1`), but "not silent" is a
/// lower bar than this path already meets everywhere else. An empty link target is never legitimate —
/// `symlink("", …)` fails on every supported platform — so refusing it costs no valid archive anything.
const EMPTY_LINK_SKIP: &str =
    "this entry is a link with no target — there is nothing it could point at, so it cannot be created. \
     Skipped; the rest of the archive still extracts";

/// The link target an entry would materialise, or `None` for an ordinary file/directory entry.
///
/// **Both link kinds, as of CPE-1759.** CPE-1774 returned symlinks only, because `unpack_in` already
/// canonicalisation-validates a hard link's target (`validate_inside_dst`) and a second guard for one
/// question is a liability. That reasoning was about *safety* and it still holds; what it did not cover
/// is that `unpack_in` refuses by **aborting the run**, which is the divergence CPE-1759 exists to close.
/// See [`hard_link_target_action`] for the measurement and for the base the two kinds differ on.
///
/// A link entry whose link name cannot be read yields an **empty** target, which [`tar_entry_refusal`]
/// refuses outright — see [`EMPTY_LINK_SKIP`].
///
/// Returns an owned target rather than a [`TarEntryKind`] because the borrow has to outlive the `entry`
/// borrow the callers still hold while unpacking; they build the `TarEntryKind` from it.
fn tar_link_target<R: std::io::Read>(entry: &tar::Entry<'_, R>) -> Option<PathBuf> {
    let kind = entry.header().entry_type();
    if !kind.is_symlink() && !kind.is_hard_link() {
        return None;
    }
    Some(entry.link_name().ok().flatten().map(|p| p.into_owned()).unwrap_or_default())
}

/// [`TarEntryKind`] from the pieces both tar sinks already have to hand.
fn tar_entry_kind<'a>(entry_type: tar::EntryType, link_target: Option<&'a Path>) -> TarEntryKind<'a> {
    if entry_type.is_dir() {
        return TarEntryKind::Directory;
    }
    match link_target {
        Some(t) if entry_type.is_symlink() => TarEntryKind::Symlink(t),
        Some(t) if entry_type.is_hard_link() => TarEntryKind::HardLink(t),
        _ => TarEntryKind::Other,
    }
}

/// Unpack a tar stream into `dest`, applying [`tar_entry_refusal`] to every entry (CPE-1773/1774).
///
/// **This is `tar::Archive::unpack`'s own loop, reproduced so the guard has somewhere to stand**, not a
/// new extraction strategy: `unpack` takes a destination and offers no per-entry hook, so there is no way
/// to refuse an entry without owning the iteration. Everything `tar-0.4.46`'s `_unpack` does is kept —
/// creating `dst` when it has no `symlink_metadata`, canonicalising it (which is what gives Windows the
/// `\\?\` extended-length prefix and therefore paths over 260 characters), deferring **directory**
/// entries to a second pass sorted by descending path bytes so a restrictive directory mode cannot be
/// applied before its children are written. Dropping that second pass would have been a silent
/// permissions regression on POSIX, which is why it is here rather than "simplified away".
///
/// **CPE-1837: the skip is recorded, not silent.** This used to return `Result<(), String>` with nowhere
/// to put a per-entry note — [`extract_archive`], the only caller, discarded it. It now returns the same
/// [`ArchiveReport`] the streamed sibling ([`extract_tar_stream`]) already builds; the skip-and-continue
/// behaviour itself is unchanged.
///
/// **A link entry's `unpack_in` failure is translated, not just propagated (CPE-1813).** `tar_entry_refusal`
/// only answers the containment questions asked *before* the write; if the write itself fails because
/// this volume cannot hold links at all, that is [`tar_link_creation_outcome`]'s question, asked here
/// because it needs the `io::Error` `unpack_in` actually produced. A refusal is skipped exactly like
/// [`EntrySlotAction::Skip`] above, and anything else is recorded as one entry failure.
///
/// (Two clauses of that sentence went stale and are corrected here. CPE-1837 gave this path an
/// `ArchiveReport`, so the skip is no longer silent; CPE-1935 replaced the `?` with
/// [`ArchiveReport::fail`], so the failure no longer ends the run.)
///
/// A thin wrapper over [`tar_unpack_with`], which does the real work parameterised over how a single
/// entry gets unpacked — see that function's doc for why (CPE-1813 review round 2, blocker 3).
fn tar_unpack<R: std::io::Read>(reader: R, dest: &Path) -> Result<ArchiveReport, String> {
    tar_unpack_with(reader, dest, |entry, root| entry.unpack_in(root))
}

/// [`tar_unpack`]'s real body, parameterised over `unpack_entry` — normally `Entry::unpack_in` itself,
/// substituted in tests to inject a controlled `Err` at a chosen entry without depending on this
/// machine's OS or filesystem to genuinely refuse a link (CPE-1813 review round 2, blocker 3).
///
/// **Why this seam exists at all.** The classifier this ticket wires in — "does this volume refuse to
/// create links at all" — has no portable trigger: 1314 needs a non-elevated, non-Developer-Mode Windows
/// account; the FAT-stick codes need an actual link-less volume mounted; neither is available on this
/// machine or any CI runner (measured directly: this box has Developer Mode on, so the unprivileged
/// `symlink_file` flag `create_entry_symlink` uses succeeds even unelevated, while the *older*
/// `New-Item -ItemType SymbolicLink` still fails 1314 in the same session — the OS-level trigger is real,
/// the reachable API just does not hit it here). A probe-and-skip test built on that trigger would
/// self-skip on every machine that can run this suite, which is a test that cannot fail — the exact
/// defect class this whole review chain exists to catch. So instead of depending on the OS to refuse,
/// this injects the refusal directly at the one seam that matters: the `Result` `unpack_in` hands back
/// for a single entry. Everything before and after that call — the entry loop, [`tar_entry_refusal`],
/// the directory-deferral pass — is the real, unmodified production code path, so a test built on this
/// seam is exercising the actual wiring, not a copy of it.
fn tar_unpack_with<R: std::io::Read>(
    reader: R,
    dest: &Path,
    mut unpack_entry: impl FnMut(&mut tar::Entry<'_, R>, &Path) -> std::io::Result<bool>,
) -> Result<ArchiveReport, String> {
    let mut archive = tar::Archive::new(reader);
    if fs::symlink_metadata(dest).is_err() {
        fs::create_dir_all(dest).map_err(|e| extraction_dest_error(dest, &e))?;
    }
    let root = dest.canonicalize().unwrap_or_else(|_| dest.to_path_buf());
    // CPE-1938: the extraction folder, held open for the whole archive, so every entry's directory
    // chain can be walked component-by-component against it — see `entry_component_action`.
    let root_dir = open_extraction_root(&root)?;
    let mut directories = Vec::new();
    let mut report = ArchiveReport::default();
    for entry in archive.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let name = entry.path().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
        let link = tar_link_target(&entry);
        let entry_type = entry.header().entry_type();
        let is_dir = entry_type.is_dir();
        match tar_entry_refusal(&root, &name, tar_entry_kind(entry_type, link.as_deref())) {
            EntrySlotAction::Write => {}
            // CPE-1837: recorded, not merely skipped — this used to be a bare `continue`.
            EntrySlotAction::Skip(reason) => {
                report.skip(&name, &reason);
                continue;
            }
            // Not a skip: an unreadable slot is an I/O failure, and silently dropping the entry would
            // report success about a file that is missing (UAT finding 6) — same as row 15. CPE-1935:
            // recorded per entry rather than taking the archive down, because the evidence is about one
            // name; see `EntrySlotAction`.
            EntrySlotAction::Fail(f) => {
                report.fail(&name, &f);
                continue;
            }
            EntrySlotAction::Abort(e) => return Err(e),
        }
        // CPE-1938, and deliberately AFTER the path questions above — see `entry_component_action`
        // for why that ordering keeps both guards reachable rather than shadowing one.
        match entry_component_action(&root_dir, &name, is_dir) {
            EntrySlotAction::Write => {}
            EntrySlotAction::Skip(reason) => {
                report.skip(&name, &reason);
                continue;
            }
            EntrySlotAction::Fail(f) => {
                report.fail(&name, &f);
                continue;
            }
            EntrySlotAction::Abort(e) => return Err(e),
        }
        if is_dir {
            directories.push(entry);
        } else {
            match unpack_entry(&mut entry, &root) {
                Ok(true) => report.done += 1,
                // `unpack_in`'s own traversal refusal (`../evil`), which never reaches our guard because
                // `entry_name_is_safe` rejects those first — kept as the belt it always was, and now
                // recorded like every other refusal on this path (CPE-1837; previously silently ignored).
                Ok(false) => report.skip(&name, UNSAFE_NAME_SKIP),
                Err(e) => match &link {
                    // CPE-1813: this volume may simply not support links at all — a refusal, not a
                    // failure. CPE-1837: the refusal reason is now recorded instead of discarded.
                    Some(target) => {
                        let marker =
                            if entry_type.is_hard_link() { TAR_HARDLINK_MARKER } else { TAR_SYMLINK_MARKER };
                        match tar_link_creation_outcome(target, &root.join(&name), &e, marker) {
                            Ok(Some(reason)) => report.skip(&name, &reason),
                            Ok(None) => {}
                            // CPE-1935: `?` stood here. A link this machine could not create for a
                            // non-categorical reason is still one entry's problem.
                            Err(why) => report.fail(&name, &EntryFailure::from_write_error(why, &e)),
                        }
                    }
                    // **CPE-1935 — the ticket's own shape on this leg.** `return Err(e.to_string())`
                    // stood here: a read-only file or a plain directory at one entry's name ended the
                    // archive, left everything already unpacked on disk unrecorded, and reported one
                    // sentence naming the blocker. `unpack_in` has already been handed this entry, so
                    // the evidence is about this name — an entry verdict. The tar iterator's own `Err`
                    // (above, and on the next `entries()` step) is still the run verdict for a broken
                    // stream, so a genuinely unreadable archive stops here as it always did.
                    None => report.fail(
                        &name,
                        &EntryFailure::from_write_error(
                            format!("could not be written into the extraction folder: {e}"),
                            &e,
                        ),
                    ),
                },
            }
        }
    }
    directories.sort_by(|a, b| b.path_bytes().cmp(&a.path_bytes()));
    for mut dir in directories {
        // **CPE-1935: a per-entry failure, and it does NOT contradict `entry_component_action`'s
        // `Abort`.** This second pass runs over directory entries whose whole chain — leaf included —
        // `create_dir_beneath` has already created and verified, so what is left for `unpack_in` to do
        // is set that directory's mode and mtime. Nothing downstream depends on it: the directory
        // exists, every file entry beneath it has already been written into it. So this is metadata on
        // one entry, not the destination refusing to hold the tree, and the two answers differ because
        // the *facts* differ, not because the rule bends.
        let dir_name = dir.path().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
        if let Err(e) = unpack_entry(&mut dir, &root) {
            report.fail(
                &dir_name,
                &EntryFailure::from_write_error(
                    format!("the folder was created, but its permissions and timestamp could not be set: {e}"),
                    &e,
                ),
            );
        }
    }
    Ok(report)
}

/// True if an archive entry name is a plain relative path that cannot escape the extraction root — the
/// shared "zip-slip" guard for extractors that don't provide one (CPE-628). `\` is normalised to `/`.
/// `pub(crate)` so [`crate::extract_plan`] can reuse it rather than duplicating the check (CPE-1055).
///
/// **CPE-1758: adopts `crate::transfer::is_safe_name` / `local_safe_segment` per segment**, closing the
/// gap the section comment above measured — `entry_name_is_safe("file:stream")` used to be `true`, and
/// that name reaches `File::create` at rows 15/16/19/20 and lands in an NTFS alternate data stream,
/// leaving the user no visible file. Every `Component::Normal` segment (a `Component::CurDir` — a lone
/// `.` — still passes through untouched, exactly as before) now has to clear BOTH:
///
/// - [`crate::transfer::is_safe_name`] — fails closed on a `:` anywhere in the segment (the ADS shape)
///   and on a segment that *starts with* `..` without being exactly a traversal component (`..evil`,
///   `..:$DATA`), platform-independently.
/// - **content-unchanged by [`crate::transfer::local_safe_segment`]** — on Windows this also fails
///   closed on a reserved DOS device name (`con`, `nul`, …) and a trailing run of `.`/space (`"x."`,
///   `" sp "`), because those are exactly the shapes [`crate::transfer::windows_safe_segment`] would
///   otherwise rewrite; `local_safe_segment` is the identity function on every other OS (`cfg!(windows)`
///   inside it), so this half of the check is a no-op there, matching the platform scope of the hazard.
///   **Compared by rewritten bytes (`local_safe_segment(seg).as_ref() != seg`), never by the `Cow`
///   variant it returns.** `windows_safe_segment`'s cheap pre-scan is deliberately over-broad — it
///   allocates an `Owned` copy for any segment containing a bare `%` even when that copy comes out
///   byte-identical, a guarantee that only mattered to a rename sink. An earlier version of this
///   function matched on `Cow::Borrowed`/`Cow::Owned` and refused every `%`-containing name on Windows
///   as a result — `"50% off.txt"`, `"report%2ffinal.txt"`, an ordinary Hive/Athena partition value like
///   `"city=A%2FB"` — a real regression caught in review before it shipped. Pinned by
///   `entry_name_is_safe_does_not_reject_percent_names_that_round_trip_unchanged`.
///
/// **Adopted, not reimplemented** — a third "is this leaf name safe" predicate is exactly how
/// `deny_stat_of` needed the same fix three times (CPE-1733's own finding); this reuses the two
/// functions `guarded_join` already applies at the transfer sink instead of duplicating their rules.
///
/// **Decision: REFUSE, not rename.** `local_safe_segment` *sanitises* at `guarded_join` — the transfer
/// sink can rewrite a segment because a rewritten name is still a fresh, unclaimed leaf under a
/// destination the caller is free to name however it likes. An archive extraction has no such freedom:
/// every one of `entry_name_is_safe`'s ~10 call sites already treats a `false` result as "skip this
/// entry, keep extracting the rest" (see the section comment above for the real surfacing route — the
/// streamed extractors record it in `ArchiveReport::errors`, rendered as an error count in the
/// operations panel; `extract_plan::plan_extract` also records it in `skipped_unsafe`, which has no UI
/// consumer yet). Switching to rename would mean growing this
/// function's contract from a predicate to a name transform and threading a renamed destination through
/// every call site — including the two `sevenz-rust` callbacks, which receive `entry_dest` already built
/// by a crate we do not control, so there is nowhere to apply a rename before the fact. That is the same
/// one-third-implementation sprawl the "adopted, not reimplemented" call above exists to avoid, for a
/// ticket scoped to *what a name may be*, not *how the sink recovers a bad one*. Skip is also not new
/// silence: it is the same "successful-looking extraction, missing entry" shape the traversal check
/// already produces for `../evil`, which nobody has treated as this module's bug to fix.
pub(crate) fn entry_name_is_safe(name: &str) -> bool {
    use std::path::Component;
    if name.is_empty() {
        return false;
    }
    let normalized = name.replace('\\', "/");
    let p = Path::new(&normalized);
    if p.is_absolute() {
        return false;
    }
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::Normal(seg) => {
                let Some(seg) = seg.to_str() else { return false };
                if !crate::transfer::is_safe_name(seg) {
                    return false;
                }
                // NOT a `Cow::Borrowed`/`Cow::Owned` match: `windows_safe_segment`'s cheap pre-scan
                // (`crate::transfer::windows_safe_segment`) is DELIBERATELY over-broad — it allocates an
                // `Owned` copy for any segment containing a bare `%` (so it can escape a pre-existing
                // `%XX` this encoder itself could have emitted) even when the copy comes out
                // byte-identical to the input. That guarantee only ever mattered to a rename sink
                // ("never a wrong answer" because the caller writes the returned bytes either way); a
                // predicate that reads `Cow::Owned` as "reject" turns "allocated, but identical" into a
                // false refusal for every `%` name on Windows — `"50% off.txt"`, `"report%2ffinal.txt"`,
                // an ordinary Hive/Athena partition value like `"city=A%2FB"` — exactly the
                // successful-looking-extraction-missing-file shape this ticket exists to remove, just
                // moved onto a common character CPE-1709 round 2 specifically fixed the mangling of.
                // Comparing the rewritten *content* is what `local_safe_segment` actually promises to
                // preserve when nothing needed rewriting.
                if crate::transfer::local_safe_segment(seg).as_ref() != seg {
                    return false;
                }
            }
            _ => return false,
        }
    }
    true
}

/// **The link decision for a `sevenz-rust` per-entry callback — rows 19–20** (CPE-1746).
///
/// `sevenz_rust::default_entry_extract_fn` writes the file **inside that crate** with a plain
/// `File::create`, which follows a link at the destination name and lands the archive's bytes in a path
/// nobody named, returning `Ok`. There is no create site here to guard, but there does not need to be: the
/// callback is handed `entry_dest` *before* it hands the entry to the writer, so the same
/// [`entry_slot_action`] decision rows 15–16 make fits in the same place, one condition alongside the
/// [`entry_name_is_safe`] check that is already there. Replacing the writer — this ticket's original
/// proposal — would mean owning decompression details this crate gets for free, for no extra coverage.
///
/// One carve-out remains, matching rows 15–16 rather than inventing policy — and the second,
/// **LEAF ONLY**, is gone as of CPE-1744 (`entry_sink_action` resolves every intermediate component now):
///
/// - **The link question is files-only.** A link at a *directory* entry's name is `create_dir_all`
///   redirection (row 18), not destruction, so it is answered by containment alone —
///   [`entry_dir_action`] — exactly as at rows 15–16.
///
/// # Why the abort arm is a captured message, not a `sevenz_rust::Error`
///
/// The callback's error type is `sevenz_rust::Error`, so an [`EntrySlotAction::Abort`] surfaced as one has
/// to survive that crate's `Display` to reach the user — and it does not. `sevenz-rust` 0.6.1 implements
/// `Display for Error` as `Debug::fmt` (`src/error.rs:74-78`), so `Error::other(msg)` re-emerges from the
/// call sites' `.map_err(|e| e.to_string())` **debug-quoted and escaped**. Measured on the exact wording
/// `fsutil::classify_create_slot` produces:
///
/// ```text
/// [CPE-1746 MEASURE] sevenz_rust::Error::other(msg).to_string(), first 160 chars:
///     Other("\"out\\a.txt\" is a link, and creating a file at a link's name writes THROUGH it — the
///     bytes would land at the link's target, a path you did not name,
/// ```
///
/// Every `\` in a Windows path doubles, the quotes `fsutil` puts around the path become `\"`, and the
/// refusal arrives wrapped in `Other(..)`. Rows 6–16 surface that wording **verbatim** — it is what the
/// user reads and what `row16_…` and `rows_15_and_16_…` pin — so routing 7z's copy through the crate's
/// error type would make one path report the same refusal differently for no gain. Pinned by
/// `sevenz_error_display_would_mangle_our_refusal_wording`, so a future `sevenz-rust` that fixes its
/// `Display` tells us this choice can be revisited.
///
/// So the message travels in a captured `Option<String>` and the callback returns the **cooperative
/// `Ok(false)` stop** the cancel path already uses; the caller converts it to `Err` once
/// `decompress_file_with_extract_fn` has returned. Byte-identical wording, one shared decision, one
/// `match` per call site.
///
/// **And each call site's `abort.is_some()` latch is load-bearing, not belt-and-braces.**
/// `SevenZReader::for_each_entries` calls its per-block helper as `block_dec.for_each_entries(&mut each)?`
/// and **discards the `bool` that helper returns** (`sevenz-rust` 0.6.1 `src/reader.rs:1370`), so an
/// `Ok(false)` ends only the *current* block — a multi-block archive would call us again for the next one
/// and write the entries after the abort. The latch makes the stop total. The existing cancel check needs
/// no latch only because it re-reads its `AtomicBool` on every entry.
fn sevenz_entry_slot_action(
    entry: &sevenz_rust::SevenZArchiveEntry,
    entry_dest: &Path,
    dest: &Path,
) -> EntrySlotAction {
    if entry.is_directory() {
        return entry_dir_action(dest, entry_dest);
    }
    entry_sink_action(dest, entry_dest)
}

/// Turn `sevenz-rust`'s per-entry write error into one of **our** sentences plus a retryable answer
/// (CPE-1935) — rows 19–20's half of the same job [`EntryFailure::from_write_error`] does everywhere
/// else.
///
/// It cannot go through `e.to_string()`, for the reason already written up on
/// [`sevenz_entry_slot_action`]: `sevenz-rust` 0.6.1 implements `Display for Error` as `Debug::fmt`
/// (`src/error.rs:74-78`), so the whole thing arrives `Debug`-quoted with every `\` in a Windows path
/// doubled and the OS message buried inside an `Io(Os { code: 5, kind: PermissionDenied, message:
/// "Access is denied." }, "C:\\…")` wrapper. That is what CPE-1935's reproduction measured coming back
/// as the run's single error string on both 7z legs, on Windows and on real ext4.
///
/// **`Error::Io` and `Error::FileOpen` are the same variant.** `Error::file_open` constructs
/// `Self::Io(e, filename)` (`src/error.rs:60-63`), so matching `Io` covers both spellings; `FileOpen`
/// exists in the enum but nothing in 0.6.1 builds it. `MaybeBadPassword` also carries an `io::Error` but
/// is deliberately **not** unwrapped here — its whole meaning is "this may be the password rather than
/// the disk", which the raw errno would hide.
fn sevenz_entry_failure(e: &sevenz_rust::Error) -> EntryFailure {
    match e {
        sevenz_rust::Error::Io(io, _) => EntryFailure::from_write_error(
            format!("could not be written into the extraction folder: {io}"),
            io,
        ),
        // Everything else is the archive's own structure — a bad header, an unsupported method, a
        // checksum. Re-running reads the same bytes and reaches the same answer.
        other => EntryFailure {
            why: format!("could not be extracted from this 7z archive: {other}"),
            retryable: false,
        },
    }
}

/// Extract a `.7z` into `dest` **safely**: `sevenz-rust` 0.6 doesn't check path traversal, so validate
/// each entry with [`entry_name_is_safe`] and skip any that isn't a plain relative path (CPE-628).
///
/// **Row 19 of the CPE-1733 table** (CPE-1746): it also refuses a **link** already sitting at the entry's
/// final name, via [`sevenz_entry_slot_action`] — see there for the decision and for why the abort arm is
/// a captured message. **CPE-1837: the skip is recorded, not silent** — this used to return
/// `Result<(), String>` with nowhere to put it; it now returns the same [`ArchiveReport`] the streamed
/// twin (row 20, [`extract_7z_stream`]) already builds.
fn extract_7z_safe(src: &Path, dest: &Path) -> Result<ArchiveReport, String> {
    let mut abort: Option<String> = None;
    let mut report = ArchiveReport::default();
    // CPE-1938 — the extraction folder's handle, held for the whole archive; see
    // `entry_component_action`. `dest` exists by now: `extract_archive` creates it (row 17).
    let root_dir = open_extraction_root(dest)?;
    catch_sevenz_panic(|| {
        sevenz_rust::decompress_file_with_extract_fn(src, dest, |entry, reader, entry_dest| {
            if abort.is_some() {
                return Ok(false); // the latch — see `sevenz_entry_slot_action`
            }
            let name = entry.name().to_string();
            if !entry_name_is_safe(&name) {
                report.skip(&name, UNSAFE_NAME_SKIP);
                return Ok(true); // skip the unsafe entry; keep extracting the rest
            }
            match sevenz_entry_slot_action(entry, entry_dest, dest) {
                EntrySlotAction::Write => {}
                EntrySlotAction::Skip(e) => {
                    report.skip(&name, &e);
                    return Ok(true); // skip this entry; keep extracting the rest
                }
                // CPE-1935: recorded and the run carries on — one entry's evidence, one entry's cost.
                EntrySlotAction::Fail(f) => {
                    report.fail(&name, &f);
                    return Ok(true);
                }
                EntrySlotAction::Abort(e) => {
                    abort = Some(e);
                    return Ok(false);
                }
            }
            // CPE-1938 — the component walk, after the path questions; see `entry_component_action`.
            match entry_component_action(&root_dir, &name, entry.is_directory()) {
                EntrySlotAction::Write => {}
                EntrySlotAction::Skip(e) => {
                    report.skip(&name, &e);
                    return Ok(true);
                }
                EntrySlotAction::Fail(f) => {
                    report.fail(&name, &f);
                    return Ok(true);
                }
                EntrySlotAction::Abort(e) => {
                    abort = Some(e);
                    return Ok(false);
                }
            }
            match sevenz_rust::default_entry_extract_fn(entry, reader, entry_dest) {
                Ok(carry_on) => {
                    report.done += 1;
                    Ok(carry_on)
                }
                // **CPE-1935 — the ticket's shape on this leg.** The `Err` was returned to
                // `sevenz-rust`, which abandoned the archive and surfaced its own debug-quoted
                // `Display` as the run's one error. `sevenz_entry_failure` builds our sentence from
                // the io error underneath instead, and `Ok(true)` keeps the scan going — the same
                // cooperative continue the `entry_name_is_safe` skip above already returns **without
                // having read `reader` at all**, which is why leaving an entry's bytes unconsumed is
                // this crate's normal case rather than a new risk.
                //
                // **CPE-1929 pair on this arm**, run on both 7z legs at once since they are twins
                // (Windows `--lib`, `Compiling cpe-server` seen each run; baseline 2434/0):
                //
                // ```text
                // A  disable (return `Err(e)` to sevenz-rust again)   2435 passed / 1 failed
                // B  lie     (treat the Err as `Ok`: `report.done += 1`) 2435 passed / 1 failed
                // ```
                //
                // A reds on `zc.txt=ABSENT` — the crate really does abandon the archive on an `Err`,
                // which is the fact the whole arm turns on — and B reds on `(done, failed, skipped)`
                // being `(3, 0, 0)` where `(2, 1, 0)` is required.
                Err(e) => {
                    report.fail(&name, &sevenz_entry_failure(&e));
                    Ok(true)
                }
            }
        })
        .map_err(|e| e.to_string())
    })?;
    match abort {
        Some(e) => Err(e),
        None => Ok(report),
    }
}

/// Extract an archive into `dest`, which is created if missing (CPE-252). Dispatched by extension. Every
/// format is guarded against zip-slip via [`entry_name_is_safe`], applied by the loop each branch owns.
///
/// # CPE-1759 — abort vs skip, decided
///
/// Four of this function's five branches **skip** a refused entry and extract the rest. The zip branch
/// **aborted** the whole run, because it handed the entry loop to `zip::ZipArchive::extract`. Same
/// function, same call, opposite answers depending on which extension the user right-clicked. The
/// decision is **skip**, and the zip branch now runs [`extract_zip_archive_stream`] — the loop its
/// streamed sibling already used.
///
/// **Why skip, given abort is the safer-sounding one.** Three reasons, in the order they settled it:
///
/// 1. **Skip is this module's contract, and abort was one branch out of twenty-three.** Rows 15, 19 and
///    21 of the table above are the other three one-shot sinks, and all three skip. Aligning the other
///    way meant converting three shipping behaviours to abort to fix one divergence.
/// 2. **Abort's only advantage turned out not to exist.** CPE-1744 recorded it as failing safe — "the
///    user gets a clear error and can retry into an empty folder" — and the review of CPE-1773/1774
///    confirmed an empty destination. Both observations came from a test archive whose *first* entry is
///    the poisoned one. `zip-2.4.2`'s `extract_internal` (`src/read.rs:897`) is a plain `for` loop with
///    `?` on `safe_prepare_path`, so the refusal fires **mid-loop**. Re-measured on this branch with the
///    poisoned entry second of three:
///
///    ```text
///    [M1] outcome                        = Err("invalid Zip archive: Invalid symlink target path")
///    [M1] a.txt (BEFORE the poison) exists = true
///    [M1] c.txt (AFTER  the poison) exists = false
///    ```
///
///    A half-extraction *and* an error naming neither what landed nor what did not. The folder is empty
///    only when the archive happens to be poisoned at entry 0. So the choice was never
///    "atomic vs partial"; it was "partial with an error" vs "complete-but-one with a refusal".
/// 3. **One hostile entry cannot be allowed to deny the other 499.** That was always skip's argument and
///    CPE-1775 supplied its precondition on the streamed path by making refusals *counted*
///    (`ArchiveReport::skipped`), not merely logged.
///
/// **What this used to NOT fix — closed by CPE-1837, stated here rather than re-derived.** This function
/// returned `Result<String, String>`, which had nowhere to put a per-entry note, so its skips were
/// **silent** — the same limitation rows 15, 19 and 21 carried and the table above records. Skip alone
/// did not make the one-shot path *informative*; it made it *consistent*, which is what CPE-1759 asked
/// for. `extract_archive` is a registered Tauri command with no Svelte caller today (every user-facing
/// extraction goes through [`extract_archive_streamed`], which already reports) — verified by grepping
/// `src/` for `extractArchive(`/`extractZipEncrypted(` outside `bindings.gen.ts` and finding nothing —
/// but a signature that cannot carry a refusal is a trap for whoever wires it up next, and it is also
/// reachable today from any IPC caller that is not the shipped Svelte frontend. It now returns
/// [`ArchiveExtractOutcome`], carrying exactly the [`ArchiveReport`] the streamed path already builds, so
/// a caller — present or future — gets the same information either path already had internally.
///
/// **And it is not the downgrade CPE-1744 measured it as.** That ticket declined the merge because
/// `ZipArchive::extract` restores unix permission bits and materialises symlink entries while our loop
/// did neither — true, and re-measured here (`[M4] good_link is symlink = Ok(false) content =
/// Ok("ok.txt")`). Both now live in [`extract_zip_archive_stream`] (see [`create_entry_symlink`]), so
/// the capability moved *up* to the streamed path rather than down from the one-shot one.
pub fn extract_archive(path: &str, dest: &str) -> Result<ArchiveExtractOutcome, String> {
    // Row 17 of the CPE-1733 table — a folder the user pointed at, so a live link there is still followed
    // on purpose; CPE-1744 reworded only the dangling case (`extraction_dest_error`).
    let dest_path = Path::new(dest);
    fs::create_dir_all(dest).map_err(|e| extraction_dest_error(dest_path, &e))?;
    let lower = path.to_lowercase();

    let report = if lower.ends_with(".tar") {
        let file = fs::File::open(path).map_err(|e| e.to_string())?;
        tar_unpack(file, dest_path)?
    } else if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        let file = fs::File::open(path).map_err(|e| e.to_string())?;
        tar_unpack(flate2::read::GzDecoder::new(file), dest_path)?
    } else if lower.ends_with(".gz") {
        // A bare .gz holds a single file; its name is the archive name minus .gz. No skip semantics here
        // — a refusal at the leaf (row 13, below) is a genuine `Err` via `refuse_link_at_new_file`, not a
        // per-entry skip, so there is nothing an `ArchiveReport` would add.
        let stem = Path::new(path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "extracted".to_string());
        // Row 13: the destination *folder* is the user's, and the leaf is the archive's own filename
        // minus `.gz` — so the whole path is one the user can already have a link sitting at.
        let leaf = dest_path.join(stem);
        refuse_link_at_new_file(&leaf)?;
        let file = fs::File::open(path).map_err(|e| e.to_string())?;
        let mut decoder = flate2::read::GzDecoder::new(file);
        let mut out = fs::File::create(&leaf).map_err(|e| e.to_string())?;
        std::io::copy(&mut decoder, &mut out).map_err(|e| e.to_string())?;
        ArchiveReport { done: 1, ..ArchiveReport::default() }
    } else if lower.ends_with(".7z") {
        extract_7z_safe(Path::new(path), dest_path)?
    } else {
        // zip family — **CPE-1759**: the same loop the streamed path uses, with a cancel flag that is
        // never set and a progress sink that discards. See this function's doc for why skip won.
        let file = fs::File::open(path).map_err(|e| e.to_string())?;
        let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
        let never = AtomicBool::new(false);
        extract_zip_archive_stream(&mut archive, dest_path, None, &never, &mut |_| {})?
    };
    Ok(ArchiveExtractOutcome { dest: dest.to_string(), report })
}

// ---------------------------------------------------------------------------
// Queue-routed compress/extract with progress + cancel (CPE-1184, epic CPE-705)
// ---------------------------------------------------------------------------
//
// `compress_archive`/`extract_archive` above are one-shot, blocking, all-or-nothing calls — fine for a
// small archive, but a large one freezes the UI (no progress, no cancel), against the streaming-liveness
// convention. The functions below are the queue-routed siblings: item-level progress (one tick per
// archive entry — cheap and enough to keep a progress bar honest without rewriting every writer to
// chunk mid-entry) plus a `cancel` flag polled between entries, so the app-adapter
// (`start_archive_compress`/`start_archive_extract` in `src-tauri/src/lib.rs`) can run them on a
// background thread and forward progress as the *same* `transfer://progress`/`transfer://done` events
// the copy/move transfer engine already uses — archive ops land in the same operations panel, are
// cancellable the same way (the shared `TRANSFER_CANCELS` registry), and stay Tauri-free here per
// SERVER-ARCHITECTURE.md. The original one-shot functions above are untouched (still used by
// `extract_archive_entry`/`extract_archive_entry_any`'s single-leaf reads and anyone else calling them),
// so nothing about their tested behaviour changes.

/// A progress snapshot emitted while a compress/extract runs. Mirrors the transfer engine's
/// `TransferProgress` shape minus the app-assigned `id` (a pure app/session concern the Tauri adapter
/// attaches when it forwards this as a `transfer://progress` event).
#[derive(Clone, Debug, Default)]
pub struct ArchiveProgress {
    pub total_bytes: u64,
    pub done_bytes: u64,
    pub total_items: u64,
    pub done_items: u64,
    pub current: String,
}

/// The final outcome of a compress/extract run. `done` counts entries actually written.
///
/// **`failed` is a per-entry count as of CPE-1935.** It used to stay 0 always, because an extraction was
/// all-or-nothing on any I/O failure: one unwritable entry returned `Err` and the report was discarded,
/// so a 27-entry archive with a read-only file at entry 4 left 23 files on disk, reported nothing about
/// them, and named only the file that stopped it. Entry-scoped failures are now counted here and
/// recorded in `errors` beside the skips, so "what landed" is answerable from the report alone. A
/// *run*-scoped abort (the extraction folder, a shared path component, the archive container) is still
/// an `Err` and still carries no report — see [`EntrySlotAction`] for the rule that draws that line.
///
/// **`skipped` is CPE-1775's addition, and it is the count the UI was missing.** An entry refused by a
/// guard — an unsafe name, a link sitting at the destination, a destination that escapes the extraction
/// folder, a link entry whose target escapes it — is neither `done` nor `failed`. Before this field, the
/// only trace was a line in `errors`, which the frontend read **only when `failed > 0`**: a refused entry
/// produced a plain "N items extracted" success toast with N quietly one lower than the archive's
/// contents. `failed` could not be reused for it (nothing failed, and a genuine failure must stay
/// distinguishable), so the honest shape is a third count, carried through `TransferReport` to the
/// `transfer://done` event.
///
/// **Invariant: every per-entry line in `errors` is pushed by [`ArchiveReport::skip`] or
/// [`ArchiveReport::fail`], which also increment the matching count.** They are two halves of one record
/// — the count is what the headline notice reads, the string is the reason behind it — and a site that
/// grew one without the other would put a number and a list in front of the user that describe different
/// things.
///
/// **That invariant was folklore until CPE-1935.** This paragraph named
/// `skipped_count_matches_the_recorded_reasons_on_every_streamed_skip_path` as its enforcement for two
/// tickets; **no commit in this repository has ever contained a test of that name** —
/// `git log --all -S"fn skipped_count_matches_the_recorded_reasons"` returns nothing, which is the
/// question worth asking (a `grep` of the working tree only says it is absent *today*, and every hit it
/// does return is prose about the absence, this sentence included — the first draft of this paragraph
/// quoted a hit count that was already wrong by the time it was reviewed).
///
/// It is now derived from the source rather than asserted about it —
/// `archive_report_counts_and_reasons_can_only_be_grown_together` reads this file, masks comments and
/// string literals, and fails if `skipped`/`failed` is incremented or `errors` pushed on **any**
/// receiver anywhere but inside these two helpers. CPE-1933's rule, applied to the claim that was
/// standing in for the check.
///
/// **CPE-1837: also the report the one-shot extractors return, not only the streamed ones.**
/// `Serialize`/`specta::Type` so it can cross the IPC boundary directly as an
/// [`ArchiveExtractOutcome`] field rather than only ever being flattened into `TransferReport` for a
/// `transfer://done` event.
#[derive(Clone, Debug, Default, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ArchiveReport {
    pub done: u64,
    pub failed: u64,
    /// Entries a guard refused. Neither written nor failed — see the type doc.
    pub skipped: u64,
    pub cancelled: bool,
    pub errors: Vec<String>,
}

impl ArchiveReport {
    /// Record one refused entry: the count the headline notice reads **and** the reason behind it, in
    /// one call so a future skip site cannot grow one without the other (CPE-1775).
    fn skip(&mut self, name: &str, reason: &str) {
        self.skipped += 1;
        self.errors.push(format!("{name}: {reason}"));
    }

    /// Record one entry that could not be delivered (CPE-1935) — the same paired count-and-reason as
    /// [`skip`](Self::skip), on the other side of [`EntrySlotAction`]'s severity question, plus the
    /// next-step clause chosen from [`EntryFailure::retryable`] rather than from the sentence's wording.
    fn fail(&mut self, name: &str, f: &EntryFailure) {
        self.failed += 1;
        let next = if f.retryable { RETRY_HELPS } else { RETRY_DOES_NOT_HELP };
        self.errors.push(format!("{name}: {}", join_failure_sentence(&f.why, next)));
    }
}

/// **CPE-1837**: what a one-shot extraction (`extract_archive`/`extract_zip_encrypted`) returns on
/// success — the destination path the caller already got, plus the [`ArchiveReport`] the streamed
/// variants always had. Before this type existed, a one-shot extraction's `Result<String, String>` had
/// nowhere to put a per-entry refusal: the loop still skipped the entry (CPE-1759 settled that a
/// one-shot extraction must not abort over a single bad entry, same as the streamed path), but the
/// caller had no way to learn it happened — a successful-looking `Ok(dest)` with a file quietly missing.
/// The skip-and-continue *behaviour* is unchanged by this ticket; only the return type stopped discarding
/// the report that was already being built.
#[derive(Clone, Debug, Default, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ArchiveExtractOutcome {
    pub dest: String,
    pub report: ArchiveReport,
}

/// One item queued for an archive write — either a directory placeholder or a file with known size,
/// discovered by [`collect_archive_sources`]'s recursive walk.
struct ArchiveSourceEntry {
    src: PathBuf,
    /// The archive-internal path (forward-slash, relative).
    name: String,
    is_dir: bool,
    size: u64,
}

/// Recursively enumerate `paths` into a flat list of entries to add to an archive — the same walk order
/// (a directory entry, then its children sorted by name) [`zip_add_path`]/[`tar_add_path`] use, just
/// flattened so a streamed writer can check `cancel` and emit progress between entries instead of only
/// at the end of a deep recursion. Skips `skip` (the output archive's own canonical path) so it never
/// packs itself (CPE-632), same guard the original recursive adders use.
fn collect_archive_sources(paths: &[String], skip: Option<&Path>) -> Result<Vec<ArchiveSourceEntry>, String> {
    fn walk(src: &Path, name: &str, skip: Option<&Path>, out: &mut Vec<ArchiveSourceEntry>) -> Result<(), String> {
        if let (Some(skip), Ok(canon)) = (skip, src.canonicalize()) {
            if canon == skip {
                return Ok(());
            }
        }
        let meta = fs::symlink_metadata(src).map_err(|e| e.to_string())?;
        if meta.is_dir() {
            out.push(ArchiveSourceEntry { src: src.to_path_buf(), name: name.to_string(), is_dir: true, size: 0 });
            let mut children: Vec<_> = fs::read_dir(src).map_err(|e| e.to_string())?.filter_map(|e| e.ok()).collect();
            children.sort_by_key(|e| e.file_name());
            for child in children {
                let child_name = child.file_name().to_string_lossy().to_string();
                walk(&child.path(), &format!("{name}/{child_name}"), skip, out)?;
            }
        } else {
            out.push(ArchiveSourceEntry { src: src.to_path_buf(), name: name.to_string(), is_dir: false, size: meta.len() });
        }
        Ok(())
    }
    let mut out = Vec::new();
    for p in paths {
        let src = Path::new(p);
        let name = src.file_name().map(|s| s.to_string_lossy().to_string()).ok_or_else(|| format!("invalid path: {p}"))?;
        walk(src, &name, skip, &mut out)?;
    }
    Ok(out)
}

/// Pack `paths` into a new `.zip` at `dest` — the streamed sibling of [`compress_to_zip`]: `cancel` is
/// polled between entries and `emit` receives a progress snapshot after each. On cancellation the
/// writer still finishes with whatever entries were already added, producing a valid but incomplete
/// archive — mirrors the transfer engine's "leave the partial result, don't delete" choice.
pub fn compress_to_zip_streamed(
    paths: &[String],
    dest: &str,
    cancel: &AtomicBool,
    mut emit: impl FnMut(&ArchiveProgress),
) -> Result<ArchiveReport, String> {
    if paths.is_empty() {
        return Err("nothing to compress".into());
    }
    let dest_canon = Path::new(dest).canonicalize().ok();
    let entries = collect_archive_sources(paths, dest_canon.as_deref())?;
    let total_bytes = entries.iter().map(|e| e.size).sum();
    let total_items = entries.iter().filter(|e| !e.is_dir).count() as u64;

    refuse_link_at_new_file(Path::new(dest))?; // row 10
    let file = fs::File::create(dest).map_err(|e| e.to_string())?;
    let mut writer = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let mut prog = ArchiveProgress { total_bytes, done_bytes: 0, total_items, done_items: 0, current: String::new() };
    let mut cancelled = false;
    emit(&prog);
    for e in &entries {
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }
        prog.current = e.name.clone();
        if e.is_dir {
            writer.add_directory(format!("{}/", e.name), opts).map_err(|err| err.to_string())?;
        } else {
            writer.start_file(&e.name, opts).map_err(|err| err.to_string())?;
            let mut f = fs::File::open(&e.src).map_err(|err| err.to_string())?;
            std::io::copy(&mut f, &mut writer).map_err(|err| err.to_string())?;
            prog.done_bytes += e.size;
            prog.done_items += 1;
        }
        emit(&prog);
    }
    prog.current.clear();
    emit(&prog);
    writer.finish().map_err(|e| e.to_string())?;
    Ok(ArchiveReport { done: prog.done_items, failed: 0, skipped: 0, cancelled, errors: Vec::new() })
}

/// Pack `paths` into a new gzip-compressed tarball at `dest` — the streamed sibling of
/// [`compress_to_targz`]. Same cancel/progress/partial-result contract as [`compress_to_zip_streamed`].
pub fn compress_to_targz_streamed(
    paths: &[String],
    dest: &str,
    cancel: &AtomicBool,
    mut emit: impl FnMut(&ArchiveProgress),
) -> Result<ArchiveReport, String> {
    if paths.is_empty() {
        return Err("nothing to compress".into());
    }
    let dest_canon = Path::new(dest).canonicalize().ok();
    let entries = collect_archive_sources(paths, dest_canon.as_deref())?;
    let total_bytes = entries.iter().map(|e| e.size).sum();
    let total_items = entries.iter().filter(|e| !e.is_dir).count() as u64;

    refuse_link_at_new_file(Path::new(dest))?; // row 11
    let file = fs::File::create(dest).map_err(|e| e.to_string())?;
    let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    let mut builder = tar::Builder::new(encoder);

    let mut prog = ArchiveProgress { total_bytes, done_bytes: 0, total_items, done_items: 0, current: String::new() };
    let mut cancelled = false;
    emit(&prog);
    for e in &entries {
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }
        prog.current = e.name.clone();
        if e.is_dir {
            builder.append_dir(&e.name, &e.src).map_err(|err| err.to_string())?;
        } else {
            builder.append_path_with_name(&e.src, &e.name).map_err(|err| err.to_string())?;
            prog.done_bytes += e.size;
            prog.done_items += 1;
        }
        emit(&prog);
    }
    prog.current.clear();
    emit(&prog);
    builder.into_inner().map_err(|e| e.to_string())?.finish().map_err(|e| e.to_string())?;
    Ok(ArchiveReport { done: prog.done_items, failed: 0, skipped: 0, cancelled, errors: Vec::new() })
}

/// Pack `paths` into `dest`, choosing the format by extension (`.zip` / `.tar.gz` / `.tgz`) — the
/// streamed sibling of [`compress_archive`], used by `start_archive_compress`.
pub fn compress_archive_streamed(
    paths: &[String],
    dest: &str,
    cancel: &AtomicBool,
    emit: impl FnMut(&ArchiveProgress),
) -> Result<ArchiveReport, String> {
    let lower = dest.to_ascii_lowercase();
    if lower.ends_with(".zip") {
        compress_to_zip_streamed(paths, dest, cancel, emit)
    } else if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        compress_to_targz_streamed(paths, dest, cancel, emit)
    } else {
        Err(format!("unsupported archive format for '{dest}' (use .zip or .tar.gz)"))
    }
}

/// Pack `paths` into a password-protected (AES-256) `.zip` at `dest` — the streamed sibling of
/// [`compress_to_zip_encrypted`], used by `start_archive_compress` when a password is given.
pub fn compress_to_zip_encrypted_streamed(
    paths: &[String],
    dest: &str,
    password: &str,
    cancel: &AtomicBool,
    mut emit: impl FnMut(&ArchiveProgress),
) -> Result<ArchiveReport, String> {
    if paths.is_empty() {
        return Err("nothing to compress".into());
    }
    if password.is_empty() {
        return Err("a password is required for an encrypted archive".into());
    }
    let dest_canon = Path::new(dest).canonicalize().ok();
    let entries = collect_archive_sources(paths, dest_canon.as_deref())?;
    let total_bytes = entries.iter().map(|e| e.size).sum();
    let total_items = entries.iter().filter(|e| !e.is_dir).count() as u64;

    refuse_link_at_new_file(Path::new(dest))?; // row 12
    let file = fs::File::create(dest).map_err(|e| e.to_string())?;
    let mut writer = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .with_aes_encryption(zip::AesMode::Aes256, password);

    let mut prog = ArchiveProgress { total_bytes, done_bytes: 0, total_items, done_items: 0, current: String::new() };
    let mut cancelled = false;
    emit(&prog);
    for e in &entries {
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }
        prog.current = e.name.clone();
        if e.is_dir {
            writer.add_directory(format!("{}/", e.name), opts).map_err(|err| err.to_string())?;
        } else {
            writer.start_file(&e.name, opts).map_err(|err| err.to_string())?;
            let mut f = fs::File::open(&e.src).map_err(|err| err.to_string())?;
            std::io::copy(&mut f, &mut writer).map_err(|err| err.to_string())?;
            prog.done_bytes += e.size;
            prog.done_items += 1;
        }
        emit(&prog);
    }
    prog.current.clear();
    emit(&prog);
    writer.finish().map_err(|e| e.to_string())?;
    Ok(ArchiveReport { done: prog.done_items, failed: 0, skipped: 0, cancelled, errors: Vec::new() })
}

/// Quick, cheap check of whether `path` (a zip) needs `password` to read — or needs *some* password
/// when `password` is `None` — without extracting anything (CPE-1184). Reads just entry 0's header
/// (the `zip` crate needs the password to even set up an AES entry's reader, so this fails immediately
/// for a wrong/missing password, the same error the old one-shot `extract_zip_encrypted`/`extract_archive`
/// surfaced). `start_archive_extract` calls this synchronously *before* queuing the background
/// extraction, so the frontend's password-prompt-and-retry flow keeps its original synchronous
/// try/catch shape instead of round-tripping through a transfer id + completion event for what's
/// normally an instant rejection.
pub fn check_zip_password(path: &str, password: Option<&str>) -> Result<(), String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    if archive.is_empty() {
        return Ok(());
    }
    let result = match password {
        Some(pw) => archive.by_index_decrypt(0, pw.as_bytes()).map(|_| ()),
        None => archive.by_index(0).map(|_| ()),
    };
    result.map_err(|e| e.to_string())
}

/// Extract a plain (unencrypted) zip into `dest`, streamed — the manual-loop sibling the zip branch of
/// [`extract_archive`] delegates to via [`extract_archive_streamed`]. Per-entry zip-slip guard mirrors
/// [`extract_zip_encrypted`]'s (skip an unsafe name rather than aborting), so this doesn't regress the
/// zip crate's own `enclosed_name` guard the one-shot `extract_archive` relies on — both are "safe",
/// per the `zip_extraction_does_not_escape_the_destination` test below, which tolerates either an
/// outright error or a silent skip.
fn extract_zip_stream(
    path: &str,
    dest: &Path,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(&ArchiveProgress),
) -> Result<ArchiveReport, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    extract_zip_archive_stream(&mut archive, dest, None, cancel, emit)
}

/// Shared zip-extraction loop for the plain and password-protected streamed extractors, and — since
/// CPE-1759 for the one-shot plain path and CPE-1807 for the one-shot password-protected path
/// ([`extract_zip_encrypted`]) — both one-shot zip extractors too. Iterate entries, skip a zip-slip
/// name, otherwise write it out, checking `cancel` and emitting progress between entries.
///
/// **Every zip extraction in this module now goes through here — there is no fourth, unmerged loop
/// anymore.** A CPE-1814 correction to this section once had to say [`extract_zip_encrypted`] was still
/// its own separate `for i in 0..archive.len()`; CPE-1807 finished the merge that correction was
/// tracking. See [`extract_zip_encrypted`]'s own doc for the guard-by-guard audit of what routing a
/// password-protected entry through this loop changes.
///
/// # CPE-1759/CPE-1807: all four zip extractors now share this loop, and why
///
/// `extract_archive` used to hand its zip branch to `zip::ZipArchive::extract`, which is all-or-nothing:
/// one refused entry ended the run. Its streamed sibling skipped the entry and kept going. Two shipped
/// paths, opposite answers to one input. CPE-1759 chose **skip**, and the decision is written up on
/// [`extract_archive`]; the two capabilities CPE-1744 measured as blocking the merge — unix permission
/// bits and real symlink entries — are implemented here rather than lost, so adopting this loop costs
/// the one-shot path nothing and *gains* the streamed path both.
///
/// **Order of questions per entry:** the name ([`entry_name_is_safe`]), the link *target*
/// ([`link_target_action`]) for an entry that declares itself a link, and then — for an entry that is
/// actually going to have bytes written into it — the **slot**, asked of the destination handle rather
/// than of the destination path.
///
/// # CPE-1913: this loop asks the slot question by HANDLE; the tar and 7z legs still ask it by path
///
/// `entry_sink_action` / `entry_dir_action` are no longer called here. They asked, by path, whether a
/// link sits at the leaf, whether the path resolves inside `dest`, and how many names the leaf has —
/// then `create_dir_all` and `fs::File::create` re-resolved that path from scratch. Two consequences,
/// both measured:
///
/// - the containment answer could be invalidated between the asking and the create (CPE-1896's window,
///   one subsystem over, measured at 73 escapes in 1200 trials); and
/// - it could not see a link pointing **inside** `dest` at all, because such a link satisfies
///   containment. `sub/leaf.txt` behind a junction `dest/sub -> dest/other` extracted into `other` and
///   reported `done: 1, errors: []` — CPE-1912's shape, no race required. Pinned by
///   `cpe_1913_a_junction_inside_the_extraction_folder_never_redirects_an_entry`.
///
/// The extraction folder is now resolved and **held open** for the whole archive, every component is
/// opened relative to the one before it ([`crate::open_beneath`]), and the three slot questions are
/// asked of that handle by [`crate::fsutil::claim_destination_handle`] — the same gate the backup,
/// restore and download legs use.
///
/// **They are not asked twice.** A path question standing in front of a handle question answers first,
/// which makes the handle question unreachable for every refusal and un-red-proofable (CPE-1929). So
/// this loop stopped calling them rather than keeping them as belt-and-braces.
///
/// **`entry_sink_action` and `entry_dir_action` are still live and still correct** — for the tar leg
/// (`tar_entry_refusal`) and the 7z leg (`sevenz_entry_slot_action`), neither of which has a
/// handle-relative walk yet. Converting those needs replacing the `tar` crate's own `unpack_in` and
/// `sevenz-rust`'s extract callback, which is a larger piece of work than this loop was, and CPE-1913
/// says so plainly rather than doing all the legs shallowly. Until then the two answer differently and
/// `rows_15_to_20_…` asserts a different marker per leg for exactly that reason.
///
/// **One new failure mode, stated rather than discovered:** the extraction folder must now be
/// **openable for read**, because a directory handle is what the walk resolves against. A folder that
/// can be written but not opened would previously have extracted; it now fails the run with a named
/// reason. Rare, loud rather than silent, and the same trade CPE-1896 recorded for the backup
/// destination.
///
/// # What is still addressed BY PATH in this loop — one thing; it was two, and the first doc said none
///
/// CPE-1938 closed the second (item 2 below). The symlink-entry branch is the survivor.
///
/// Round 1 called the permission pass "the last path-addressed write here". That was wrong, and PR
/// #1050's Reviewer and Security Auditor found the two halves of why. Both are **unchanged from
/// `main`** — this PR neither introduced nor worsened either — and both are recorded rather than
/// claimed away, because a doc that says the loop is clean is worse than no doc.
///
/// 1. **The symlink-entry branch.** An entry that declares itself a link goes to
///    [`materialise_entry_symlink`], which calls `create_entry_symlink` and, on a retry,
///    `fs::remove_file` — **both by path**. It is not converted because it is not a byte write: there
///    is no handle to open, and `symlinkat`/`unlinkat` relative to the parent handle are primitives
///    [`crate::open_beneath`] does not have yet. Its own containment question ([`link_target_action`],
///    CPE-1774) is unchanged and still runs before it.
///
///    **CPE-1938 re-checked this and half of it is now stale, so here is the current position.**
///    `unlinkat` *does* exist in this module now — [`crate::open_beneath::remove_file_beneath`],
///    added by CPE-1937 — so the `fs::remove_file` in [`materialise_entry_symlink`]'s overwrite retry
///    is convertible today. `symlinkat` is not; there is no `symlink_beneath`, and adding one is a
///    new primitive on three platforms rather than a call-site change. Converting only the delete
///    would put a handle-relative unlink one line in front of a by-path `symlink` that re-resolves
///    the same components anyway — a guard whose predicate can never decide anything, which is the
///    shadowed-guard shape CPE-1929 exists to stop (and `remove_file` on its own is already
///    link-safe: it removes the name, it does not follow it). So both halves stay by path, together,
///    until `symlink_beneath` exists — which is the piece of work, not a call-site edit.
///
///    **CPE-1973 corrected the bound this paragraph used to put on that, and the correction is the
///    reason the paragraph is now this long.** It said a planted (non-racing) link at a component was
///    refused "because `link_target_action`'s `confined_to` resolves the whole path", and that the
///    residual "creates a link, never bytes — [`create_entry_symlink`] is exclusive-create, so it
///    clobbers nothing". **Both were false, and each was false in the direction that made the branch
///    look safe.** `confined_to` *does* resolve the whole path — through the plant — so an
///    inside-pointing link at a component satisfies containment, which is this ticket's entire
///    premise applied to the one leg round 1 exempted; and the exclusive-create's `AlreadyExists`
///    retry is an `fs::remove_file` on a re-resolved path, so the residual was a **delete of a file
///    the archive never named**, not a harmless extra link. Measured, `Ok(done: 2, skipped: 0,
///    errors: [])` with the victim gone — see the comment at the walk itself.
///
///    A [`entry_component_action`] call now runs on this branch before anything by-path touches
///    `out`, so a **planted** component link is refused here exactly as it is on the tar and 7z legs.
///
///    **What is genuinely at risk in the meantime, stated rather than waved at:** an attacker who can
///    swap a *directory* component of a link entry's name **between that walk and
///    `create_entry_symlink`** still gets a symlink created outside the root — the same raced residual
///    rows 19–22 carry, and for the same reason (`symlinkat` does not exist here yet). The link's own
///    target has already been contained, so the escape is a dangling name rather than archive bytes;
///    the `fs::remove_file` retry remains reachable in that raced window, so "it clobbers nothing" is
///    not claimed for it either.
/// 2. ~~**The `#[cfg(unix)]` permission pass at the bottom**~~ — **fixed by CPE-1938.** It used to
///    collect `(path, mode)` for every written file and `fs::set_permissions` them after the loop.
///    `set_permissions` is `chmod(2)` and **follows links**, and the mode is the *archive's*, so a
///    component swapped in between the write and the drain re-aimed an archive-chosen mode — setuid
///    included — at whatever the name then pointed at. It is now an `fchmod` on the descriptor the
///    bytes went into, applied inline in the file branch; see the comment at that call for the
///    60/60 measurement that made the case and for why the deferral was safe to drop.
fn extract_zip_archive_stream(
    archive: &mut zip::ZipArchive<fs::File>,
    dest: &Path,
    password: Option<&str>,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(&ArchiveProgress),
) -> Result<ArchiveReport, String> {
    use std::io::Read;
    // Row 17 of the CPE-1733 table — a folder the user pointed at; still followed when it is a live link,
    // and CPE-1744 fixed only what a *dangling* one says (`extraction_dest_error`).
    fs::create_dir_all(dest).map_err(|e| extraction_dest_error(dest, &e))?;
    // **CPE-1913: hold the extraction folder open for the whole archive, once.** Every entry below is
    // then resolved component-by-component against *this object* rather than by re-parsing
    // `dest.join(name)`, so an archive entry naming a directory that turns out to be a link cannot
    // redirect the write — wherever the link points, inside the folder or out of it.
    //
    // Row 17 of the CPE-1733 table still holds for the folder ITSELF: the user pointed at it, so a live
    // link at `dest` is followed on purpose. `canonicalize` is what follows it, once, here.
    //
    // CPE-1938 shared the two steps with the tar and 7z legs, which now need the identical handle and
    // the identical wording — see [`open_extraction_root`].
    let root = open_extraction_root(dest)?;
    let total_items = archive.len() as u64;
    let mut prog = ArchiveProgress { total_bytes: 0, done_bytes: 0, total_items, done_items: 0, current: String::new() };
    let mut report = ArchiveReport::default();
    emit(&prog);
    for i in 0..archive.len() {
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            break;
        }
        let mut entry = match password {
            Some(pw) => archive.by_index_decrypt(i, pw.as_bytes()).map_err(|e| e.to_string())?,
            None => archive.by_index(i).map_err(|e| e.to_string())?,
        };
        let name = entry.name().to_string();
        prog.current = name.clone();
        if !entry_name_is_safe(&name) {
            report.skip(&name, UNSAFE_NAME_SKIP);
            prog.done_items += 1;
            emit(&prog);
            continue;
        }
        let out = dest.join(&name);
        let rel = std::path::Path::new(&name);
        // Row 18 of the CPE-1733 table. `entry_dir_action` + `fs::create_dir_all` stood here: a path
        // containment question, then a by-path directory create that walks a link like any other
        // directory. CPE-1913 replaced both with the per-component walk, which creates each level
        // inside the handle above it and refuses a link at every one — so a directory entry can no
        // longer materialise a tree behind a junction, and it can no longer do it *inside* the
        // extraction folder either, which containment could never see (CPE-1912's shape).
        //
        // A refusal is a `skip` when it is a verdict and an abort when it is an I/O failure, which is
        // the same rule `entry_slot_action` applies and is now carried by `Refusal::policy` rather than
        // re-derived per site.
        if entry.is_dir() {
            if let Err(r) = crate::open_beneath::create_dir_beneath(&root, rel) {
                // CPE-1935: `return Err(r.why)` stood on the `!policy` branch. A zip DIRECTORY entry's
                // walk is the one place in this loop where `entry_component_action`'s run-scoped
                // reasoning would also apply — the chain includes the leaf, and files land inside it.
                // It is nonetheless an entry failure here, because unlike the tar/7z legs this loop
                // does not defer directory entries: every file entry carries its own full chain
                // through `create_beneath` below, so a directory entry that could not be created stops
                // exactly the entries that name it and nothing else. Whatever is wrong will be met
                // again, per entry, by the file branch — and reported there, per entry, rather than
                // once for the archive.
                if !r.policy {
                    report.fail(&name, &EntryFailure::retryable(r.why));
                } else {
                    report.skip(&name, &r.why);
                }
                prog.done_items += 1;
                emit(&prog);
                continue;
            }
        } else {
            // Row 16 of the CPE-1733 table — the streamed twin of row 15, with somewhere to put the note.
            // Recorded in `errors` and counted as a done *item* (not a done *file*), the same shape the
            // unsafe-name skip above uses, so the progress bar still reaches its total. No longer leaf-only
            // as of CPE-1744, and — as at row 15 — this must run BEFORE the `create_dir_all(parent)` below
            // so an escaping entry cannot create its intermediate folders outside `dest` first.
            // Row 16 of the CPE-1733 table. `entry_sink_action` stood here and asked three questions
            // by PATH — is a link sitting at the leaf, does the path resolve inside `dest`, does the
            // leaf have more than one name — followed by `create_dir_all(parent)` and, at the bottom of
            // this branch, a plain `fs::File::create`. Every one of those answers could be invalidated
            // between the asking and the create, and the containment one could not see a link pointing
            // *inside* `dest` at all.
            //
            // The three are now asked of the HANDLE the bytes will enter, by the same
            // `fsutil::claim_destination_handle` the backup, restore and download legs use. They are
            // not asked twice: `entry_sink_action` is not called on this path any more, because a path
            // question standing in front of a handle question answers first and makes the handle
            // question unreachable (CPE-1929). It is still the answer for the tar and 7z legs, which
            // have no handle walk yet — see this function's doc.
            //
            // The claim happens further down, immediately before the bytes, because the symlink-entry
            // branch below must run first: a link entry creates a link rather than writing a file, and
            // claiming a destination for it would create an empty file at the name.
            // Row 23 (CPE-1774), moved off the pre-pass and into the loop by CPE-1759: a zip entry can
            // declare itself a symlink, and its stored bytes are the link's TARGET. Nothing above asks
            // about that — `evil_link` is a perfectly ordinary entry name. This used to be answered by
            // `refuse_escaping_zip_symlinks`, a pre-pass that could only abort; asked here it is a
            // counted skip like every other refusal on this path, and it covers `extract_zip_encrypted`'s
            // streamed twin too, which the pre-pass never ran for.
            if entry.is_symlink() {
                let mut target = Vec::new();
                // CPE-1935: `?` stood here. `by_index` hands out an independent reader per entry, so a
                // member whose stored bytes will not decompress is one entry's problem, not the
                // container's — the container's own failures are still `by_index`/`ZipArchive::new`
                // above, which stay `Err`.
                if let Err(e) = entry.read_to_end(&mut target) {
                    report.fail(
                        &name,
                        &EntryFailure::from_write_error(
                            format!("this entry is a link, and its target could not be read out of the archive: {e}"),
                            &e,
                        ),
                    );
                    prog.done_items += 1;
                    emit(&prog);
                    continue;
                }
                let target = PathBuf::from(String::from_utf8_lossy(&target).into_owned());
                let refusal = if target.as_os_str().is_empty() {
                    // `symlink("", …)` fails on every supported platform, so refusing costs no valid
                    // archive anything — the same call `tar_entry_refusal` makes, see `EMPTY_LINK_SKIP`.
                    Some(EMPTY_LINK_SKIP.to_string())
                } else {
                    // **CPE-1973 — the link entry's PARENT CHAIN, asked of handles, before anything
                    // by-path touches `out`.** This branch had no component walk at all:
                    // `create_beneath` is called only in the file branch below, `create_dir_beneath`
                    // only under `entry.is_dir()`. A zip *symlink* entry was therefore the one shape
                    // left in this loop reaching a by-path call with its components unresolved — which
                    // the per-path table opposite wrongly recorded as "already handle-gated
                    // (CPE-1913)", and which the residual note on this function wrongly bounded as
                    // race-only.
                    //
                    // Measured on real ext4, before this walk existed, with a zip carrying the link
                    // entry `sub/victim`, a **planted** (no race, no privilege) `dest/sub ->
                    // dest/other`, and a real user file at `dest/other/victim`:
                    //
                    // ```text
                    // outcome = Ok(ArchiveReport { done: 2, failed: 0, skipped: 0, errors: [] })
                    // dest/other/victim is now a symlink: true   target: Some("benign.txt")
                    // its content reads back as: None            <- the user's file was DELETED
                    // ```
                    //
                    // `confined_to` canonicalises `dest/sub/victim` **through** the planted link,
                    // lands inside `dest/other` and so answers *inside* — the exact blind spot this
                    // ticket exists for. `materialise_entry_symlink`'s `AlreadyExists` retry then
                    // re-resolves `sub` through the link and `fs::remove_file`s a file the archive
                    // never named. The old note called that retry harmless because
                    // `create_entry_symlink` is exclusive-create; **the retry IS the clobber, and it
                    // is a delete.** Regression:
                    // `cpe1973_a_zip_symlink_entry_is_never_created_through_a_planted_component_link`.
                    //
                    // **This does not shadow `link_target_action`, and they are not one question**
                    // (CPE-1929). The walk asks whether the entry's *name* stays inside, by opening
                    // each component; `link_target_action` asks whether the link's *target* escapes,
                    // which no walk can see. A top-level `evil_link -> ../../etc/passwd` has an empty
                    // component chain and is refused only by the second; a planted `dest/sub` is
                    // refused only by the first. Both stay, and each is red-proofed on its own shape.
                    let chain = match entry_component_action(&root, &name, false) {
                        EntrySlotAction::Write => None,
                        EntrySlotAction::Skip(m) => Some(m),
                        // CPE-1935: an entry-scoped failure on a link entry is recorded and the loop
                        // moves on, exactly as on the file branch below. Nothing is created for it.
                        EntrySlotAction::Fail(f) => {
                            report.fail(&name, &f);
                            prog.done_items += 1;
                            emit(&prog);
                            continue;
                        }
                        EntrySlotAction::Abort(e) => return Err(e),
                    };
                    match chain {
                        Some(m) => Some(m),
                        None => match link_target_action(dest, &out, &target) {
                            EntrySlotAction::Write => None,
                            EntrySlotAction::Skip(m) => Some(m),
                            // Dead today for the same reason the `Abort` arm below is — CPE-1814's
                            // argument, unchanged: matched explicitly so a future feeder cannot make
                            // an entry silently skip where the rule says it must be recorded as a
                            // failure.
                            EntrySlotAction::Fail(f) => {
                                report.fail(&name, &f);
                                prog.done_items += 1;
                                emit(&prog);
                                continue;
                            }
                            // Propagated, not collapsed with `Skip` — CPE-1814. This is the identical
                            // construct `tar_entry_refusal` collapsed before CPE-1759: dead today (only
                            // `link_target_action` feeds it, and that function returns `Write`/`Skip`
                            // only, never `Abort`), and dead is exactly what it was there too, until a
                            // new feeder made the collapsed arm live and an entry started silently
                            // skipping where the rule (`EntrySlotAction`'s own doc) says it must abort
                            // — UAT finding 6. Matching it explicitly, rather than folding it back into
                            // `Skip`, is what keeps a future feeder honest instead of reintroducing
                            // that bug a third time.
                            EntrySlotAction::Abort(e) => return Err(e),
                        },
                    }
                };
                let refusal = match refusal {
                    Some(m) => Some(m),
                    // A machine that categorically has no links refuses the ENTRY; anything else that
                    // goes wrong is a failure — recorded against this entry as of CPE-1935, where it
                    // used to take the run down. `materialise_entry_symlink` draws that line and owns
                    // the overwrite retry.
                    None => match materialise_entry_symlink(&out, &target) {
                        Ok(r) => r,
                        Err(why) => {
                            report.fail(&name, &EntryFailure::retryable(why));
                            prog.done_items += 1;
                            emit(&prog);
                            continue;
                        }
                    },
                };
                match refusal {
                    Some(m) => report.skip(&name, &m),
                    None => report.done += 1,
                }
                prog.done_items += 1;
                emit(&prog);
                continue;
            }
            // CPE-1913: `fs::File::create(&out)` — a by-path, follow-everything, truncate-anything
            // open — replaced by the shared gate. `create_beneath` resolves `rel` one component at a
            // time against the root handle (creating the missing directories inside it, which is what
            // removed the `create_dir_all` above), and `claim_destination_handle` then refuses a link,
            // a directory or a hard link at the leaf by asking the handle rather than the name.
            let claimed = crate::fsutil::claim_destination_handle(
                &out,
                crate::fsutil::LinkGuardWording::EXTRACT,
                crate::fsutil::DestinationSite::Beneath { root: &root, rel },
            );
            // CPE-1961: the claim is HELD, not unwrapped to its handle. The bytes go into a staging
            // sibling and `commit()` renames it over `out`; dropping the claim instead — which is what
            // every `continue` below does — removes the staged file, and the destination name too when
            // this call created it, leaving the entry's name exactly as the extraction found it.
            //
            // **`continue`, and never `?` — round 4 (Reviewer Blocker 1).** Round 3's version of this
            // sentence said *"which is what the `?` below does"*, and the `?` it meant was the one on
            // `commit()`. That is a run abort, and this loop's contract stopped permitting one when
            // CPE-1935 landed in this branch's base. **Every early exit in this per-entry body records
            // the entry and continues**; the only exceptions are the two `EntrySlotAction::Abort`
            // returns, which are the deliberate hostile-swap aborts and say so at their own sites.
            let mut claimed = match claimed {
                Ok(c) => c,
                Err(r) if r.policy => {
                    report.skip(&name, &r.why);
                    prog.done_items += 1;
                    emit(&prog);
                    continue;
                }
                // Not a skip — see row 15 (UAT finding 6): an entry the filesystem refused for an I/O
                // reason is a file the user asked for and did not get.
                //
                // **CPE-1935 — THE site the ticket was filed from.** `return Err(r.why)` stood here,
                // and it is the sentence PR #1050's UAT quoted: *"the path component \"existing.txt\"
                // could not be opened for writing (Access is denied. (os error 5))"* over a 27-entry
                // archive that had already put 23 files on disk. Reproduced for this ticket on both a
                // read-only occupant and a plain-directory occupant, Windows and real ext4, identical
                // on every leg: the entry before the blocker landed, the entry after it never did, and
                // the one error named neither. The entry is still refused and still unwritten — only
                // the other 26 entries stopped paying for it.
                //
                // **CPE-1929 pair on this arm** (Windows `--lib`, `Compiling cpe-server` seen each run;
                // baseline 2434/0):
                //
                // ```text
                // A  disable (put `return Err(r.why)` back)      2434 passed / 2 failed
                // B  lie     (`Err(r) if true || r.policy`)      2434 passed / 2 failed
                // ```
                //
                // A reds `cpe1935_a_blocked_entry_never_takes_the_run_down` on the *filesystem*
                // (`zc.txt=ABSENT`) and B reds it on the *classification* (`skipped 1` where `failed 1`
                // is required) — two different reds for the two different mistakes, which is what says
                // this arm is reached on its own terms rather than shadowed by the guard in front of it.
                Err(r) => {
                    report.fail(&name, &EntryFailure::retryable(r.why));
                    prog.done_items += 1;
                    emit(&prog);
                    continue;
                }
            };
            // CPE-1935: `?` stood here. Same argument as the link branch's `read_to_end` above — the
            // reader is this entry's own, so a failure decompressing or writing it is this entry's.
            //
            // **CPE-1961: the bytes go into the CLAIM's staging sibling, never the destination.**
            // `continue`ing here drops `claimed`, which removes the staged file — and the destination
            // name too when this call created it — so a decompression failure now leaves the entry's
            // name exactly as the extraction found it instead of truncated-then-abandoned.
            if let Err(e) = std::io::copy(&mut entry, &mut claimed.file) {
                report.fail(
                    &name,
                    &EntryFailure::from_write_error(
                        format!("could not be written into the extraction folder: {e}"),
                        &e,
                    ),
                );
                prog.done_items += 1;
                emit(&prog);
                continue;
            }
            prog.done_bytes += entry.size();
            // **CPE-1938 F-B — the mode is set through the HANDLE the bytes went into, not by name.**
            //
            // What stood here until this ticket: `modes.push((out.clone(), mode))`, drained after the
            // loop into `fs::set_permissions(&path, …)`. `set_permissions` is `chmod(2)`, which
            // **follows symlinks**, and the archive picks `mode` — so anything that replaced the NAME
            // between the write and the drain redirected an archive-chosen mode onto whatever the new
            // name pointed at. Measured on real ext4 before this change, with a thread that swaps
            // `dest/a.txt` for a symlink to a file **outside** the extraction folder while the loop is
            // still working through later entries:
            //
            // ```text
            // trials=60  swaps=60  MODES_CHANGED_OUTSIDE=60   (victim 0o644 -> 0o777)
            // ```
            //
            // 60 out of 60, not a narrow window: the drain ran only after the *last* entry, so the
            // window was the whole rest of the archive. `cpe1938_the_old_path_addressed_mode_pass_
            // chmods_through_a_planted_link` is the standing control for the same fact, and
            // `cpe1938_a_swapped_slot_never_moves_an_archive_chosen_mode_outside_the_root` is the
            // regression.
            //
            // `File::set_permissions` is `fchmod(2)` on the descriptor `claim_destination_handle`
            // returned — the same object `io::copy` just filled, reached through the per-component walk
            // and never named again. **The property: the mode lands on the file OBJECT this loop wrote,
            // identified by an open descriptor; the destination is never re-resolved from a path.** A
            // link planted at the name cannot violate it whether it points outside the root or back
            // inside it, because there is no second name resolution to redirect.
            //
            // **Applying it here rather than after the loop loses nothing.** The deferral existed to
            // apply a directory's mode after its children (`zip`'s own `Reverse(path)` sort), and this
            // loop never recorded a directory: the push was inside the file branch, so `modes` only
            // ever held leaves. The descriptor stays writable across its own `fchmod` regardless of
            // what the new mode says.
            //
            // **"A leaf's mode cannot make anything else unwritable" is the sentence that used to close
            // this paragraph, and it is true across paths but not across entries that share a NAME** —
            // so it was replaced with the measurement rather than narrowed on reasoning (CPE-1938
            // round 2). Applying a read-only mode inline means a *later* entry resolving to the same
            // on-disk name meets a 0o444 file: `create_beneath`'s `O_CREAT|O_EXCL` gets `EEXIST`, the
            // open-existing retry gets `EACCES`, and `Refusal::policy == false` takes the whole run
            // down where the deferred drain would have landed both. Measured on real ext4 (`TMPDIR`
            // off tmpfs), hand-built STORED zips because `ZipWriter::start_file` refuses a duplicate
            // name outright (`InvalidArchive("Duplicate filename")`):
            //
            // ```text
            // [x.txt 0o444, x.txt 0o644, zz_ok.txt]  reader lists ["x.txt", "zz_ok.txt"]
            //                                        Ok(done: 2)  x.txt = "SECOND", mode 0o644
            // [x.txt 0o644, x.txt 0o644, zz_ok.txt]  Ok(done: 2)  x.txt = "SECOND", mode 0o644
            // [a.txt 0o444, b.txt, sub/c.txt]        Ok(done: 3)  all three land
            // ```
            //
            // **The duplicate case never reaches the loop: `zip::ZipArchive` collapses duplicate names
            // in its central directory**, so `archive.len()` is 2 for a three-entry archive and the
            // last copy wins — identically with and without the read-only mode. The read-only leaf
            // followed by *different* later entries (row 3) is the shape that does occur, and it is
            // unaffected, which is the part the old sentence got right.
            //
            // **Re-extraction into the same folder is a real behaviour, and it is not this change's.**
            // A second run over a 0o444 leaf fails with the component wording and `os error 13`; it
            // failed identically before, because the deferred drain still left the file 0o444 at the
            // end of the first run. Same on `main`, measured, so nothing here made it worse.
            //
            // **CPE-1961 round 2 — this block sits ABOVE the commit, and that position is required
            // twice over.** Mechanically: `ClaimedDestination::commit(mut self)` *consumes* the claim,
            // so `claimed.file` does not exist after it — round 1 left the block below the commit
            // still referring to the pre-rename local `f` and `crates/server` stopped compiling on
            // Linux and macOS (`E0425: cannot find value `f``), which no Windows job could catch
            // because the whole block is `#[cfg(unix)]`.
            // Semantically it is the only position in which the paragraph above stays TRUE: the handle
            // the bytes went into is the *staging sibling*, and applying the mode here puts it on that
            // object while it is still nameless, so the file takes the destination name already
            // wearing its final mode. There is no instant at which `out` exists with the wrong bits —
            // the same "mode onto the staged file before it takes the name" ordering
            // `fsutil::HandleCarryover::apply` uses, and the reason `create_staging_beneath` creates
            // at `0600` rather than `0666 & ~umask`. Covered by
            // `cpe1961_a_zip_entrys_unix_mode_lands_on_the_committed_file`.
            //
            // **CPE-1935's per-entry handling, kept — with the one word staging changes.** `map_err(..)?`
            // stood here before CPE-1935 and a `?` would still take the whole run down for one entry, so
            // the `fail`-and-`continue` is unchanged. What did change is the *sentence*: CPE-1935 said
            // *"its contents were written, but its permissions could not be set"* because the bytes were
            // already at the destination by this point. They are not any more — they are in a staging
            // sibling that the `continue` below drops and unlinks — so the message now says nothing was
            // written, which is what the filesystem will show. Still a `fail` rather than a `done`: the
            // file the archive described had a mode this filesystem would not apply, and calling that
            // success is the silent-partial shape one layer down.
            #[cfg(unix)]
            if let Some(mode) = entry.unix_mode() {
                use std::os::unix::fs::PermissionsExt;
                if let Err(e) = claimed.file.set_permissions(fs::Permissions::from_mode(mode)) {
                    report.fail(
                        &name,
                        &EntryFailure::from_write_error(
                            format!(
                                "its permissions could not be set, so nothing was written for it: {e}"
                            ),
                            &e,
                        ),
                    );
                    prog.done_items += 1;
                    emit(&prog);
                    continue;
                }
            }
            // **CPE-1961 round 4 (Reviewer Blocker 1): a failed commit is THIS ENTRY's failure.**
            //
            // Round 3 wrote `claimed.commit().map_err(|r| r.why)?` here — the only bare `?` left in
            // this per-entry body — and that is a **run abort**. CPE-1961 introduces a failure point
            // this loop did not have (`sync_all`, then a rename the filesystem can refuse) and round 3
            // gave it the exact semantics CPE-1935, merged one commit before this branch's base, was
            // filed to remove from this loop. Measured, same three-entry zip, `victim.txt` held open by
            // another process with `FILE_SHARE_READ|FILE_SHARE_WRITE` and no `FILE_SHARE_DELETE`:
            //
            // ```text
            //                          outcome                        before  victim       after
            // base 104b0bc5 (main)     Ok(done: 3)                    BEFORE  REPLACEMENT  AFTER
            // head 9902e1f5 (round 3)  Err("…could not be replaced…")  BEFORE  ORIGINAL     ABSENT
            // ```
            //
            // Refusing the entry is right and unchanged — the user's file is intact and the reason
            // names it. Aborting the archive over it is a regression against `main`, and it also
            // contradicts `src/docs/explorer-archives.md`, which lists a full disk under **Failed**
            // ("the extraction keeps going") and says only the whole destination can stop a run.
            // Cost row 1 predicts `ENOSPC` from staging, and under ext4's delayed allocation that
            // lands at `sync_all` — i.e. exactly here, on the platform whose fixture this loop's
            // regression test cannot build.
            //
            // **`retryable`, not `from_write_error`:** a `Refusal` carries no `io::Error` to classify,
            // and every way this line *fails* — a lock, a sharing violation, a full disk, a share that
            // dropped — is something the user can clear and extract again into.
            //
            // **The `policy` fork — CPE-1961 round 5 (Reviewer Major 1). Round 4 wrote `report.fail`
            // unconditionally here, on a premise that reading `commit`'s callee refutes.** The premise,
            // written at `fsutil::claim_destination_handle`, was *"`commit` only ever returns
            // `Refusal::failure`"*. It is true of `DestinationSite::ByPath`, which does
            // `commit_replacement(...).map_err(Refusal::failure)`; it is false of the **`Beneath`** arm
            // this loop uses, which returns `open_beneath::rename_beneath`'s `Refusal` unchanged — and
            // that function's `descend(root, Act::Commit, dirs)` calls `refuse_link` on a directory
            // component that has become a link since the claim. `policy: true`. Executed, not read off:
            // `fsutil::tests::cpe_1961_a_link_planted_at_an_interior_component_makes_commit_refuse_with_policy_true`.
            //
            // With `report.fail` unconditional, that entry landed in the **failed** bucket carrying
            // *"clear that and extract again"* — advice that cannot work, because re-extracting into a
            // folder whose component is a planted link refuses again, and refuses at the *claim* this
            // time, where the arm above already calls it a **skip**. So one refusal, one folder, two
            // buckets and two different sentences, decided by nothing but which microsecond the link
            // was planted in. The fork makes the two moments agree: `policy` — "not writing is the
            // correct outcome" — is a **skip** whether the guard that reached that verdict fired at the
            // claim or at the commit, and everything else is still this entry's failure.
            //
            // **CPE-1929 pair, run rather than reasoned about** (Windows `--lib`, `Compiling
            // cpe-server` seen on each run; baseline 2,456 passed / 0 failed / 14 ignored):
            //
            // ```text
            // A  disable (`if false && r.policy`)   2456 passed / 0 failed   GREEN
            // B  lie     (`if true  || r.policy`)   2455 passed / 1 failed   RED
            //
            //   B: cpe_1961_a_destination_the_commit_cannot_replace_costs_one_entry_not_the_run
            //      "two entries written and the blocked one in the FAILED bucket … which is a failure
            //       and not a policy skip: ArchiveReport { done: 2, failed: 0, skipped: 1, … }"
            //        left: (2, 0, 1)   right: (2, 1, 0)
            // ```
            //
            // **A green and B red is NOT the shadowed signature** — that one is *both* green, and it
            // means nothing reaches the guard. B reds, so control does reach this fork and the `else`
            // arm is load-bearing: an I/O commit refusal still has to land in **failed**, and this
            // change cannot have quietly moved it. What A's green says is narrower and is the honest
            // caveat: the **`policy: true` side specifically** has no in-tree test, and that is
            // structural rather than an omission — its only input is a component swapped inside
            // `io::copy`'s window, so a leg-level fixture would have to *race* the extraction and could
            // pass by missing it, which is worse than none. What is pinned instead is the two halves
            // this arm is built from: that `commit` really does produce a `policy: true` refusal
            // (`fsutil::tests::cpe_1961_a_link_planted_at_an_interior_component_makes_commit_refuse_with_policy_true`,
            // red-proofed on Linux against a real planted link) and that `policy: false` still reaches
            // `failed` (B, above).
            //
            // **Red-proof, run rather than asserted** (Windows `--lib`, `Compiling cpe-server` seen).
            // Putting `claimed.commit().map_err(|r| r.why)?` back:
            //
            // ```text
            // cpe_1961_a_destination_the_commit_cannot_replace_costs_one_entry_not_the_run ... FAILED
            //   THE POINT: the entry AFTER the blocked one must be written too. … :
            //   Err("…the path component \"victim.txt\" could not be replaced by the staged copy of it
            //   (Access is denied. (os error 5)) …")   left: None  right: Some([65,70,84,69,82])
            // cpe1935_a_blocked_entry_never_takes_the_run_down ... ok
            // ```
            //
            // Note the second line: CPE-1935's own test does **not** red on this, because nothing in
            // the tree drove a commit failure until the test above existed. That is why the `?` shipped
            // through three rounds — a clean interdiff of the rebase proved the resolutions textually
            // right and could not see that the loop's contract had changed underneath them.
            if let Err(r) = claimed.commit() {
                // Same two arms, same order and the same meaning as the claim's above — a verdict is a
                // skip, an I/O refusal is this entry's failure.
                if r.policy {
                    report.skip(&name, &r.why);
                } else {
                    report.fail(&name, &EntryFailure::retryable(r.why));
                }
                prog.done_items += 1;
                emit(&prog);
                continue;
            }
            report.done += 1; // only files count toward "done" — a dir is a placeholder, not content
        }
        prog.done_items += 1;
        emit(&prog);
    }
    prog.current.clear();
    emit(&prog);
    Ok(report)
}

/// Extract a password-protected `.zip` at `path` into `dest` with `password`, streamed — the sibling of
/// [`extract_zip_encrypted`], used by `start_archive_extract` when a password is given.
pub fn extract_zip_encrypted_streamed(
    path: &str,
    dest: &str,
    password: &str,
    cancel: &AtomicBool,
    mut emit: impl FnMut(&ArchiveProgress),
) -> Result<ArchiveReport, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    extract_zip_archive_stream(&mut archive, Path::new(dest), Some(password), cancel, &mut emit)
}

/// Totals (bytes, item count) for a tar/tar.gz stream, from a cheap first-pass listing — TAR has no
/// central directory, so a byte/item total for the progress bar needs one read before the real
/// streamed-extraction pass reopens the file. Best-effort: an unreadable/corrupt archive just yields
/// `(0, 0)`, so the real pass's own error is what the caller ultimately sees.
fn tar_totals(path: &str, gz: bool) -> (u64, u64) {
    let read = || -> Result<Vec<ArchiveEntry>, String> {
        let file = fs::File::open(path).map_err(|e| e.to_string())?;
        if gz {
            tar_entries(flate2::read::GzDecoder::new(file))
        } else {
            tar_entries(file)
        }
    };
    match read() {
        Ok(entries) => {
            let total_bytes = entries.iter().filter(|e| !e.is_dir).map(|e| e.size).sum();
            let total_items = entries.iter().filter(|e| !e.is_dir).count() as u64;
            (total_bytes, total_items)
        }
        Err(_) => (0, 0),
    }
}

/// Extract a tar stream into `dest`, streamed: each entry is unpacked via `unpack_in`, the tar crate's
/// own path-safety-checked writer — the exact same per-entry call `Archive::unpack` (the one-shot
/// [`extract_archive`]'s tar path) makes internally, so this is not a behaviour change, just the same
/// work done one entry at a time so `cancel`/`emit` can run between them.
///
/// **One behaviour change layered on top (CPE-1813): a link entry whose creation `unpack_in` refuses
/// because this volume cannot hold links is a counted, recorded skip, not an abort** — see
/// [`tar_link_creation_outcome`]. Everything else `unpack_in` can fail on is still a **failure**, which
/// since CPE-1935 means one recorded entry failure with the run carrying on, where it used to leave via
/// `?` and end the extraction.
///
/// A thin wrapper over [`extract_tar_stream_with`] — see that function's doc for why the real body is
/// parameterised over how a single entry gets unpacked (CPE-1813 review round 2, blocker 3).
fn extract_tar_stream<R: std::io::Read>(
    reader: R,
    dest: &Path,
    total_bytes: u64,
    total_items: u64,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(&ArchiveProgress),
) -> Result<ArchiveReport, String> {
    extract_tar_stream_with(reader, dest, total_bytes, total_items, cancel, emit, |entry, root| {
        entry.unpack_in(root)
    })
}

/// [`extract_tar_stream`]'s real body, parameterised over `unpack_entry` for the same reason
/// [`tar_unpack_with`] is — see that function's doc for why a probe-and-skip live test cannot pin this
/// routing on every machine, and why injecting the failure at this seam is what can (CPE-1813 review
/// round 2, blocker 3).
#[allow(clippy::too_many_arguments)]
fn extract_tar_stream_with<R: std::io::Read>(
    reader: R,
    dest: &Path,
    total_bytes: u64,
    total_items: u64,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(&ArchiveProgress),
    mut unpack_entry: impl FnMut(&mut tar::Entry<'_, R>, &Path) -> std::io::Result<bool>,
) -> Result<ArchiveReport, String> {
    let mut archive = tar::Archive::new(reader);
    // CPE-1938 — the streamed twin of `tar_unpack_with`'s root handle. `dest` already exists here:
    // `extract_archive_streamed` creates it (row 17) before dispatching.
    let root_dir = open_extraction_root(dest)?;
    let mut prog = ArchiveProgress { total_bytes, done_bytes: 0, total_items, done_items: 0, current: String::new() };
    let mut report = ArchiveReport::default();
    emit(&prog);
    let entries = archive.entries().map_err(|e| e.to_string())?;
    for entry in entries {
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            break;
        }
        let mut entry = entry.map_err(|e| e.to_string())?;
        let is_dir = entry.header().entry_type().is_dir();
        let size = entry.header().size().unwrap_or(0);
        let name = entry.path().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
        prog.current = name.clone();
        // CPE-1773/1774: the guard `unpack_in` does not have. Asked BEFORE the entry is handed over,
        // because `unpack_in` owns the write and there is no `File::create` here to intercept — see
        // [`tar_entry_refusal`] for the three questions and the measurements behind each.
        let link = tar_link_target(&entry);
        let entry_type = entry.header().entry_type();
        match tar_entry_refusal(dest, &name, tar_entry_kind(entry_type, link.as_deref())) {
            EntrySlotAction::Write => {}
            EntrySlotAction::Skip(reason) => {
                report.skip(&name, &reason);
                // Counted as a done *item* so the progress bar still reaches its total — but only for a
                // non-directory entry, because [`tar_totals`] counts only those into `total_items`
                // (unlike the ZIP loop, whose total is `archive.len()`). Incrementing here for a refused
                // directory would push `done_items` past `total_items` and show a bar over 100%.
                if !is_dir {
                    prog.done_items += 1;
                }
                emit(&prog);
                continue;
            }
            // Not a skip — see row 16 (UAT finding 6). An unreadable slot is a failure, and this path
            // having somewhere to *record* a skip is not a reason to reclassify one as a skip. CPE-1935
            // gave it its own count instead of the whole run; see `EntrySlotAction`.
            EntrySlotAction::Fail(f) => {
                report.fail(&name, &f);
                if !is_dir {
                    prog.done_items += 1;
                }
                emit(&prog);
                continue;
            }
            EntrySlotAction::Abort(e) => return Err(e),
        }
        // CPE-1938 — the component walk, after the path questions; see `entry_component_action`.
        match entry_component_action(&root_dir, &name, is_dir) {
            EntrySlotAction::Write => {}
            EntrySlotAction::Skip(reason) => {
                report.skip(&name, &reason);
                if !is_dir {
                    prog.done_items += 1;
                }
                emit(&prog);
                continue;
            }
            EntrySlotAction::Fail(f) => {
                report.fail(&name, &f);
                if !is_dir {
                    prog.done_items += 1;
                }
                emit(&prog);
                continue;
            }
            EntrySlotAction::Abort(e) => return Err(e),
        }
        match unpack_entry(&mut entry, dest) {
            Ok(unpacked) => {
                if unpacked {
                    if !is_dir {
                        report.done += 1;
                        prog.done_bytes += size;
                        prog.done_items += 1;
                    }
                } else {
                    // `unpack_in`'s own traversal refusal (`../evil`), which never reached our guard
                    // because `entry_name_is_safe` rejects those first — kept as the belt it always was.
                    report.skip(&name, UNSAFE_NAME_SKIP);
                }
            }
            // CPE-1813: this volume may simply not support links at all — a refusal, not a failure —
            // and only reachable for a link entry (containment already passed `tar_entry_refusal`).
            Err(e) => match &link {
                Some(target) => {
                    let marker =
                        if entry_type.is_hard_link() { TAR_HARDLINK_MARKER } else { TAR_SYMLINK_MARKER };
                    match tar_link_creation_outcome(target, &dest.join(&name), &e, marker) {
                        Ok(Some(reason)) => {
                            report.skip(&name, &reason);
                            if !is_dir {
                                prog.done_items += 1;
                            }
                        }
                        Ok(None) => {}
                        // CPE-1935 — the streamed twin of `tar_unpack_with`'s arm; `?` stood here.
                        Err(why) => {
                            report.fail(&name, &EntryFailure::from_write_error(why, &e));
                            if !is_dir {
                                prog.done_items += 1;
                            }
                        }
                    }
                }
                // CPE-1935 — the streamed twin of `tar_unpack_with`'s ticket shape; `return Err` stood
                // here and took the archive down over one unwritable name.
                None => {
                    report.fail(
                        &name,
                        &EntryFailure::from_write_error(
                            format!("could not be written into the extraction folder: {e}"),
                            &e,
                        ),
                    );
                    if !is_dir {
                        prog.done_items += 1;
                    }
                }
            },
        }
        emit(&prog);
    }
    prog.current.clear();
    emit(&prog);
    Ok(report)
}

/// Extract a `.7z` into `dest`, streamed: `decompress_file_with_extract_fn`'s per-entry callback lets us
/// check `cancel` and emit progress between entries, applying the same [`entry_name_is_safe`] guard
/// [`extract_7z_safe`] does. Returning `Ok(false)` on a cancel stops the scan cooperatively (no error) —
/// the same "stop scanning" outcome [`extract_7z_entry`] already uses for its own early-stop case.
///
/// **Row 20 of the CPE-1733 table** (CPE-1746) — the streamed twin of row 19, and **the live one**:
/// `start_archive_extract` → [`extract_archive_streamed`] → here is the path the UI's queued extract
/// takes, so this is where a pre-planted link in the destination was actually costing users bytes. Same
/// [`sevenz_entry_slot_action`] decision; unlike row 19 this one has an [`ArchiveReport`] to record the
/// skip in, so it does — the same `{name}: {reason}` shape row 16 uses.
fn extract_7z_stream(
    path: &str,
    dest: &Path,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(&ArchiveProgress),
) -> Result<ArchiveReport, String> {
    let (total_bytes, total_items) = match sevenz_entries(path) {
        Ok(entries) => (
            entries.iter().filter(|e| !e.is_dir).map(|e| e.size).sum(),
            entries.iter().filter(|e| !e.is_dir).count() as u64,
        ),
        Err(_) => (0, 0),
    };
    let mut prog = ArchiveProgress { total_bytes, done_bytes: 0, total_items, done_items: 0, current: String::new() };
    let mut report = ArchiveReport::default();
    let mut abort: Option<String> = None;
    // CPE-1938 — the streamed twin of `extract_7z_safe`'s root handle; see `entry_component_action`.
    let root_dir = open_extraction_root(dest)?;
    emit(&prog);
    catch_sevenz_panic(|| {
        sevenz_rust::decompress_file_with_extract_fn(path, dest, |entry, reader, entry_dest| {
            if abort.is_some() {
                return Ok(false); // the latch — see `sevenz_entry_slot_action`
            }
            if cancel.load(Ordering::Relaxed) {
                report.cancelled = true;
                return Ok(false); // cooperative stop, not an error
            }
            let name = entry.name().to_string();
            let size = entry.size();
            prog.current = name.clone();
            if !entry_name_is_safe(&name) {
                report.skip(&name, UNSAFE_NAME_SKIP);
                emit(&prog);
                return Ok(true);
            }
            // Row 20 — the link guard, before the entry reaches `sevenz-rust`'s `File::create`. Recorded in
            // `errors` and counted as a done *item* (not a done *file*), the same shape row 16 uses, so the
            // progress bar still reaches its total.
            match sevenz_entry_slot_action(entry, entry_dest, dest) {
                EntrySlotAction::Write => {}
                EntrySlotAction::Skip(e) => {
                    report.skip(&name, &e);
                    prog.done_items += 1;
                    emit(&prog);
                    return Ok(true);
                }
                // Not a skip — see row 15 (CPE-1733 UAT finding 6). CPE-1935: recorded per entry, the
                // scan carries on. Never raised as a `sevenz_rust::Error`; the reason is on
                // `sevenz_entry_slot_action`.
                EntrySlotAction::Fail(f) => {
                    report.fail(&name, &f);
                    prog.done_items += 1;
                    emit(&prog);
                    return Ok(true);
                }
                EntrySlotAction::Abort(e) => {
                    abort = Some(e);
                    return Ok(false);
                }
            }
            // CPE-1938 — the component walk, after the path questions; see `entry_component_action`.
            match entry_component_action(&root_dir, &name, entry.is_directory()) {
                EntrySlotAction::Write => {}
                EntrySlotAction::Skip(e) => {
                    report.skip(&name, &e);
                    prog.done_items += 1;
                    emit(&prog);
                    return Ok(true);
                }
                EntrySlotAction::Fail(f) => {
                    report.fail(&name, &f);
                    prog.done_items += 1;
                    emit(&prog);
                    return Ok(true);
                }
                EntrySlotAction::Abort(e) => {
                    abort = Some(e);
                    return Ok(false);
                }
            }
            match sevenz_rust::default_entry_extract_fn(entry, reader, entry_dest) {
                Ok(carry_on) => {
                    prog.done_bytes += size;
                    prog.done_items += 1;
                    report.done += 1;
                    emit(&prog);
                    Ok(carry_on)
                }
                // CPE-1935 — the streamed twin of `extract_7z_safe`'s arm, and the leg the UI actually
                // takes. See there for why `Ok(true)` after an unread entry is this crate's normal case.
                Err(e) => {
                    report.fail(&name, &sevenz_entry_failure(&e));
                    prog.done_items += 1;
                    emit(&prog);
                    Ok(true)
                }
            }
        })
        .map_err(|e| e.to_string())
    })?;
    if let Some(e) = abort {
        return Err(e);
    }
    prog.current.clear();
    emit(&prog);
    Ok(report)
}

/// Extract an archive into `dest`, streamed — the sibling of [`extract_archive`], used by
/// `start_archive_extract`. Dispatched by extension exactly like the one-shot version; `dest` is
/// created the same way.
pub fn extract_archive_streamed(
    path: &str,
    dest: &str,
    cancel: &AtomicBool,
    mut emit: impl FnMut(&ArchiveProgress),
) -> Result<ArchiveReport, String> {
    // Row 17 of the CPE-1733 table — a folder the user pointed at, so a live link there is still followed
    // on purpose; CPE-1744 reworded only the dangling case (`extraction_dest_error`).
    let dest_path = Path::new(dest);
    fs::create_dir_all(dest).map_err(|e| extraction_dest_error(dest_path, &e))?;
    let lower = path.to_lowercase();

    if lower.ends_with(".tar") {
        let (total_bytes, total_items) = tar_totals(path, false);
        let file = fs::File::open(path).map_err(|e| e.to_string())?;
        extract_tar_stream(file, dest_path, total_bytes, total_items, cancel, &mut emit)
    } else if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        let (total_bytes, total_items) = tar_totals(path, true);
        let file = fs::File::open(path).map_err(|e| e.to_string())?;
        extract_tar_stream(flate2::read::GzDecoder::new(file), dest_path, total_bytes, total_items, cancel, &mut emit)
    } else if lower.ends_with(".gz") {
        // A bare .gz holds a single file — one item, no per-entry granularity possible.
        let stem = Path::new(path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "extracted".to_string());
        let mut prog = ArchiveProgress { total_bytes: 0, done_bytes: 0, total_items: 1, done_items: 0, current: stem.clone() };
        emit(&prog);
        // Row 14 — the streamed twin of row 13; same destination shape, same guard.
        let leaf = dest_path.join(&stem);
        refuse_link_at_new_file(&leaf)?;
        let file = fs::File::open(path).map_err(|e| e.to_string())?;
        let mut decoder = flate2::read::GzDecoder::new(file);
        let mut out = fs::File::create(&leaf).map_err(|e| e.to_string())?;
        std::io::copy(&mut decoder, &mut out).map_err(|e| e.to_string())?;
        prog.done_items = 1;
        prog.current.clear();
        emit(&prog);
        Ok(ArchiveReport { done: 1, failed: 0, skipped: 0, cancelled: false, errors: Vec::new() })
    } else if lower.ends_with(".7z") {
        extract_7z_stream(path, dest_path, cancel, &mut emit)
    } else {
        extract_zip_stream(path, dest_path, cancel, &mut emit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn scratch(tag: &str) -> crate::fsutil::ScratchDir {
        crate::fsutil::scratch_dir(&format!("cpe-archive-{tag}"))
    }

    #[test]
    fn lists_a_zip_archive() {
        let d = scratch("zip");
        let zip_path = d.join("a.zip");
        {
            let file = fs::File::create(&zip_path).unwrap();
            let mut w = zip::ZipWriter::new(file);
            let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default();
            w.start_file("hello.txt", opts).unwrap();
            w.write_all(b"hi there").unwrap();
            w.add_directory("sub/", opts).unwrap();
            w.finish().unwrap();
        }
        let entries = read_archive_entries(&zip_path.to_string_lossy()).unwrap();
        let hello = entries.iter().find(|e| e.name == "hello.txt").unwrap();
        assert!(!hello.is_dir && hello.size == 8);
        assert!(entries.iter().any(|e| e.name == "sub/" && e.is_dir));
        // Also reachable via the zip-specific lister (used by the compress verifier).
        assert!(zip_entries(&zip_path.to_string_lossy()).unwrap().iter().any(|e| e.name == "hello.txt"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn create_empty_zip_makes_a_valid_openable_archive() {
        let d = scratch("emptyzip");
        let zip_path = d.join("New Compressed (zipped) Folder.zip");
        let out = create_empty_zip(&zip_path.to_string_lossy()).unwrap();
        assert_eq!(out, zip_path.to_string_lossy());
        assert!(zip_path.exists());
        // It must be a genuinely valid archive: the zip reader opens it and reports zero entries,
        // and our own lister agrees (an empty file would fail both).
        let archive = zip::ZipArchive::new(fs::File::open(&zip_path).unwrap()).unwrap();
        assert_eq!(archive.len(), 0);
        assert!(read_archive_entries(&zip_path.to_string_lossy()).unwrap().is_empty());
        // Refuses to clobber an existing file (atomic create_new).
        assert!(create_empty_zip(&zip_path.to_string_lossy()).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn a_non_archive_is_an_error() {
        let d = scratch("bad");
        let f = d.join("not.zip");
        fs::write(&f, b"this is not a zip").unwrap();
        assert!(read_archive_entries(&f.to_string_lossy()).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn entry_name_is_safe_rejects_traversal() {
        assert!(entry_name_is_safe("a/b/c.txt"));
        assert!(entry_name_is_safe("./x.txt"));
        assert!(entry_name_is_safe("folder/leaf"));
        assert!(!entry_name_is_safe("../evil"));
        assert!(!entry_name_is_safe("a/../../evil"));
        assert!(!entry_name_is_safe("..\\evil")); // backslash traversal, normalised
        assert!(!entry_name_is_safe("a\\..\\..\\evil"));
        assert!(!entry_name_is_safe("/etc/passwd"));
        assert!(!entry_name_is_safe(""));
    }

    #[test]
    fn compress_to_zip_then_extract_round_trips() {
        let d = scratch("roundtrip");
        // Build a small source tree.
        fs::create_dir_all(d.join("src/sub")).unwrap();
        fs::write(d.join("src/a.txt"), b"alpha").unwrap();
        fs::write(d.join("src/sub/b.txt"), b"beta").unwrap();

        let zip_path = d.join("out.zip");
        // Empty selection errors.
        assert!(compress_to_zip(&[], &zip_path.to_string_lossy()).is_err());
        // Pack the folder.
        compress_to_zip(&[d.join("src").to_string_lossy().to_string()], &zip_path.to_string_lossy()).unwrap();
        // The listing sees both files.
        let names: Vec<String> = read_archive_entries(&zip_path.to_string_lossy())
            .unwrap()
            .iter()
            .map(|e| e.name.replace('\\', "/"))
            .collect();
        assert!(names.iter().any(|n| n.ends_with("a.txt")));
        assert!(names.iter().any(|n| n.ends_with("sub/b.txt")));

        // Extract it back out and verify contents.
        let out = d.join("unpacked");
        extract_archive(&zip_path.to_string_lossy(), &out.to_string_lossy()).unwrap();
        assert_eq!(fs::read(out.join("src/a.txt")).unwrap(), b"alpha");
        assert_eq!(fs::read(out.join("src/sub/b.txt")).unwrap(), b"beta");

        // Extract a single entry to a temp file.
        let tmp = extract_archive_entry(&zip_path.to_string_lossy(), "src/a.txt").unwrap();
        assert_eq!(fs::read(&tmp).unwrap(), b"alpha");
        let _ = fs::remove_file(&tmp);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn compress_to_targz_then_extract_round_trips() {
        let d = scratch("targz-roundtrip");
        fs::create_dir_all(d.join("src/sub")).unwrap();
        fs::write(d.join("src/a.txt"), b"alpha").unwrap();
        fs::write(d.join("src/sub/b.txt"), b"beta").unwrap();

        let tgz = d.join("out.tar.gz");
        assert!(compress_to_targz(&[], &tgz.to_string_lossy()).is_err(), "empty selection errors");
        compress_to_targz(&[d.join("src").to_string_lossy().to_string()], &tgz.to_string_lossy()).unwrap();

        // The listing (via the existing reader) sees both files.
        let names: Vec<String> = read_archive_entries(&tgz.to_string_lossy())
            .unwrap()
            .iter()
            .map(|e| e.name.replace('\\', "/"))
            .collect();
        assert!(names.iter().any(|n| n.ends_with("a.txt")), "got {names:?}");
        assert!(names.iter().any(|n| n.ends_with("sub/b.txt")), "got {names:?}");

        // Extract it back out and verify contents.
        let out = d.join("unpacked");
        extract_archive(&tgz.to_string_lossy(), &out.to_string_lossy()).unwrap();
        assert_eq!(fs::read(out.join("src/a.txt")).unwrap(), b"alpha");
        assert_eq!(fs::read(out.join("src/sub/b.txt")).unwrap(), b"beta");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn compress_archive_dispatches_by_extension() {
        let d = scratch("dispatch");
        fs::write(d.join("f.txt"), b"x").unwrap();
        let src = d.join("f.txt").to_string_lossy().to_string();

        // .zip and .tar.gz both work; both list the file back.
        for ext in ["out.zip", "out.tar.gz", "out.tgz"] {
            let dest = d.join(ext).to_string_lossy().to_string();
            compress_archive(std::slice::from_ref(&src), &dest).unwrap_or_else(|e| panic!("{ext}: {e}"));
            let names: Vec<_> = read_archive_entries(&dest).unwrap().iter().map(|e| e.name.clone()).collect();
            assert!(names.iter().any(|n| n.ends_with("f.txt")), "{ext}: got {names:?}");
        }
        // An unrecognised extension is a clear error.
        assert!(compress_archive(&[src], &d.join("out.rar").to_string_lossy()).unwrap_err().contains("unsupported"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn encrypted_zip_round_trips_and_rejects_a_wrong_password() {
        let d = scratch("encrypted");
        fs::write(d.join("secret.txt"), b"top secret").unwrap();
        let src = d.join("secret.txt").to_string_lossy().to_string();
        let zip = d.join("locked.zip");

        assert!(compress_to_zip_encrypted(&[], &zip.to_string_lossy(), "pw").is_err(), "empty selection errors");
        assert!(
            compress_to_zip_encrypted(std::slice::from_ref(&src), &zip.to_string_lossy(), "").is_err(),
            "an empty password errors"
        );
        compress_to_zip_encrypted(&[src], &zip.to_string_lossy(), "hunter2").unwrap();

        // The right password extracts the file byte-exact.
        let out = d.join("out");
        extract_zip_encrypted(&zip.to_string_lossy(), &out.to_string_lossy(), "hunter2").unwrap();
        assert_eq!(fs::read(out.join("secret.txt")).unwrap(), b"top secret");

        // A wrong password is a clear error, not a silent garbage extraction.
        let bad = d.join("bad");
        assert!(extract_zip_encrypted(&zip.to_string_lossy(), &bad.to_string_lossy(), "wrong").is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn compress_skips_the_output_archive_inside_a_source() {
        let d = scratch("zip_self");
        fs::create_dir_all(d.join("folder")).unwrap();
        fs::write(d.join("folder").join("a.txt"), b"a").unwrap();
        // The output .zip lives INSIDE the folder being compressed (CPE-632).
        let dest = d.join("folder").join("out.zip");
        compress_to_zip(&[d.join("folder").to_string_lossy().to_string()], &dest.to_string_lossy()).unwrap();
        let names: Vec<String> = zip_entries(&dest.to_string_lossy()).unwrap().into_iter().map(|e| e.name).collect();
        assert!(names.iter().any(|n| n.ends_with("a.txt")), "should contain the real file: {names:?}");
        assert!(!names.iter().any(|n| n.contains("out.zip")), "must not contain itself: {names:?}");
        let _ = fs::remove_dir_all(&d);
    }

    /// CRC-32 (IEEE), so the hand-built malicious zip below has a valid checksum the extractor accepts.
    fn crc32(data: &[u8]) -> u32 {
        let mut crc: u32 = 0xFFFF_FFFF;
        for &byte in data {
            crc ^= byte as u32;
            for _ in 0..8 {
                if crc & 1 != 0 {
                    crc = (crc >> 1) ^ 0xEDB8_8320;
                } else {
                    crc >>= 1;
                }
            }
        }
        !crc
    }

    /// A minimal single-entry STORED zip whose filename is `name` verbatim — used to smuggle a `../`
    /// traversal name past the zip *writer* (which rejects it), so we can test the *extractor's* guard.
    fn craft_zip_with_entry_name(name: &str, data: &[u8]) -> Vec<u8> {
        let name = name.as_bytes();
        let crc = crc32(data);
        let (nlen, dlen) = (name.len() as u16, data.len() as u32);
        let mut z = Vec::new();
        let u16le = |v: u16, z: &mut Vec<u8>| z.extend_from_slice(&v.to_le_bytes());
        let u32le = |v: u32, z: &mut Vec<u8>| z.extend_from_slice(&v.to_le_bytes());
        // Local file header.
        u32le(0x0403_4b50, &mut z);
        u16le(20, &mut z); u16le(0, &mut z); u16le(0, &mut z); // ver, flags, method(stored)
        u16le(0, &mut z); u16le(0, &mut z);                     // mod time/date
        u32le(crc, &mut z); u32le(dlen, &mut z); u32le(dlen, &mut z); // crc, comp, uncomp
        u16le(nlen, &mut z); u16le(0, &mut z);                  // name len, extra len
        z.extend_from_slice(name);
        z.extend_from_slice(data);
        let cd_offset = z.len() as u32;
        // Central directory header.
        u32le(0x0201_4b50, &mut z);
        u16le(20, &mut z); u16le(20, &mut z); u16le(0, &mut z); u16le(0, &mut z); // made-by, needed, flags, method
        u16le(0, &mut z); u16le(0, &mut z);                     // mod time/date
        u32le(crc, &mut z); u32le(dlen, &mut z); u32le(dlen, &mut z);
        u16le(nlen, &mut z); u16le(0, &mut z); u16le(0, &mut z); // name, extra, comment len
        u16le(0, &mut z); u16le(0, &mut z); u32le(0, &mut z);    // disk start, internal attrs, external attrs
        u32le(0, &mut z);                                        // local header offset
        z.extend_from_slice(name);
        let cd_size = z.len() as u32 - cd_offset;
        // End of central directory.
        u32le(0x0605_4b50, &mut z);
        u16le(0, &mut z); u16le(0, &mut z); u16le(1, &mut z); u16le(1, &mut z); // disks, entries
        u32le(cd_size, &mut z); u32le(cd_offset, &mut z); u16le(0, &mut z);     // cd size/offset, comment len
        z
    }

    // End-to-end zip-slip guard: a zip carrying a `../escape.txt` entry must NOT write outside the
    // extraction root. `extract_archive` leans on the zip crate's `enclosed_name`; this pins that the
    // guard actually holds, so a future crate bump that regressed it would fail CI (the 7z path has its
    // own `entry_name_is_safe` unit test; this covers the far more common zip format end-to-end).
    #[test]
    fn zip_extraction_does_not_escape_the_destination() {
        let d = scratch("zip_slip");
        let zip_path = d.join("evil.zip");
        // Hand-crafted because the zip *writer* refuses a `../` name — we're testing the extractor.
        fs::write(&zip_path, craft_zip_with_entry_name("../escape.txt", b"pwned")).unwrap();

        let dest = d.join("out");
        // The guard may either reject the archive (Err) or skip the unsafe entry (Ok) — both are safe.
        // The invariant we care about is that the traversal entry is NEVER written outside `dest`.
        let _ = extract_archive(&zip_path.to_string_lossy(), &dest.to_string_lossy());
        assert!(!d.join("escape.txt").exists(), "traversal entry escaped the extraction root");
        assert!(!dest.parent().unwrap().join("escape.txt").exists(), "traversal entry escaped the extraction root");
        let _ = fs::remove_dir_all(&d);
    }

    /// **CPE-1857, the archive half.** The entry's name is chosen by the archive, so an archive can aim
    /// an entry at any pre-existing name under the folder the user picked. If that name happens to be a
    /// hard link — a second name for a file living anywhere, including outside the extraction folder —
    /// `File::create` writes the entry's bytes into the **inode**, and they come out at the other name
    /// too. `entry_name_is_safe`, the per-component containment walk and the leaf-link check all pass it,
    /// and all three are *right*: a hard link has no target, so it resolves to itself and the name really
    /// is inside `dest`.
    ///
    /// The fixture's liveness is proved before anything is asserted about the extractor, the only way a
    /// hard link can be: content written through the OUTSIDE name, read back through the IN-TREE one.
    #[test]
    fn cpe_1857_a_zip_entry_aimed_at_a_hard_link_never_writes_the_outside_file() {
        let d = scratch("cpe1857-zip-hardlink");
        let dest = d.join("out");
        let outside = d.join("elsewhere");
        fs::create_dir_all(&dest).unwrap();
        fs::create_dir_all(&outside).unwrap();

        let victim = outside.join("victim.txt");
        fs::write(&victim, b"placeholder").unwrap();
        let slot = dest.join("note.txt");
        if fs::hard_link(&victim, &slot).is_err() {
            crate::skip_notice!(
                "SKIPPING cpe_1857_a_zip_entry_aimed_at_a_hard_link_never_writes_the_outside_file: no \
                 hard-link support on this filesystem — NOTHING on this run covered the hard-link hole"
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }
        fs::write(&victim, b"OUTSIDE CONTENT").unwrap();
        assert_eq!(
            fs::read(&slot).ok().as_deref(),
            Some(&b"OUTSIDE CONTENT"[..]),
            "fixture is inert: the entry's slot and the outside file are not one object, so this run \
             could not have tested writing through a hard link at all"
        );

        let zip_path = d.join("aimed.zip");
        fs::write(&zip_path, craft_zip_with_entry_name("note.txt", b"ARCHIVE PAYLOAD")).unwrap();
        let outcome = extract_archive(&zip_path.to_string_lossy(), &dest.to_string_lossy());

        // HARM FIRST, on the filesystem, before any claim about what was reported.
        assert_eq!(
            fs::read(&victim).ok().as_deref(),
            Some(&b"OUTSIDE CONTENT"[..]),
            "HARM: the extraction put an archive entry's bytes on a file OUTSIDE the extraction folder, \
             through a pre-existing hard link no path check can see: {outcome:?}"
        );
        assert_eq!(
            fs::read(&slot).ok().as_deref(),
            Some(&b"OUTSIDE CONTENT"[..]),
            "HARM: the slot was written too — the skip must land before any byte moves"
        );
        let _ = fs::remove_dir_all(&d);
    }

    /// **CPE-1857 Security-Auditor finding 1, half one — the guard was present and SILENT on a network
    /// share.** `name_is_multiply_linked` read its answer through `probe_no_follow`, which funnels every
    /// probe through `facts_or_unreadable` and discards the whole result when the identity is degenerate
    /// (zero volume or zero file index). This repo already documents that
    /// `GetFileInformationByHandle` *succeeds and hands back a zero index* on several network
    /// redirectors — and in that case `nNumberOfLinks` **is present and correct, and was thrown away**.
    /// The function then answered "not multiply linked", the gate let the entry through, and extraction
    /// to a share wrote through a pre-existing hard link exactly as before the ticket.
    ///
    /// Driven through the injection seam because the condition cannot be staged: a real redirector that
    /// zeroes the index is not something a test can conjure, and the auditor confirmed a denied
    /// `FILE_READ_ATTRIBUTES` ACE does not reach this path on Windows.
    #[test]
    fn cpe_1857_a_degenerate_identity_must_not_silently_disable_the_hard_link_guard() {
        let d = scratch("cpe1857-degenerate-id");
        let dest = d.join("out");
        let outside = d.join("elsewhere");
        fs::create_dir_all(&dest).unwrap();
        fs::create_dir_all(&outside).unwrap();
        let victim = outside.join("victim.txt");
        fs::write(&victim, b"placeholder").unwrap();
        let slot = dest.join("note.txt");
        if fs::hard_link(&victim, &slot).is_err() {
            crate::skip_notice!(
                "SKIPPING cpe_1857_a_degenerate_identity_must_not_silently_disable_the_hard_link_guard: \
                 no hard-link support here — NOTHING on this run covered the degenerate-identity fail-open"
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }
        fs::write(&victim, b"OUTSIDE CONTENT").unwrap();
        assert_eq!(
            fs::read(&slot).ok().as_deref(),
            Some(&b"OUTSIDE CONTENT"[..]),
            "fixture is inert: the slot and the outside file are not one object"
        );

        let zip_path = d.join("aimed.zip");
        fs::write(&zip_path, craft_zip_with_entry_name("note.txt", b"ARCHIVE PAYLOAD")).unwrap();

        // Every probe now reports a correct link count under an identity that identifies nothing.
        let _reset = crate::batch_media::ProbeReset::arm(
            crate::batch_media::ProbeInjection::DegenerateIdentity,
        );
        let outcome = extract_archive(&zip_path.to_string_lossy(), &dest.to_string_lossy());
        drop(_reset);

        assert_eq!(
            fs::read(&victim).ok().as_deref(),
            Some(&b"OUTSIDE CONTENT"[..]),
            "HARM: on a volume whose identity is degenerate — a network share — the extraction wrote an \
             archive entry's bytes through a hard link into a file outside the extraction folder. The \
             link COUNT was readable the whole time; it was discarded by an identity gate that has \
             nothing to do with this question: {outcome:?}"
        );
        assert_eq!(
            fs::read(&slot).ok().as_deref(),
            Some(&b"OUTSIDE CONTENT"[..]),
            "HARM: the slot was written too — the skip must land before any byte moves"
        );
        let _ = fs::remove_dir_all(&d);
    }

    /// **CPE-1913 round 2, security-audit finding F1: a directory junction at a FILE entry's name is a
    /// per-entry skip, not a whole-run abort — and the rest of the archive still extracts.**
    ///
    /// Round 1 made this a regression on Windows, and only on Windows. The leaf open carries
    /// `FILE_NON_DIRECTORY_FILE`, so a junction sitting at `note.txt` comes back
    /// `STATUS_FILE_IS_A_DIRECTORY` — nothing link-shaped — and the unclassified refusal was
    /// `policy: false`, which this loop turns into `return Err`. Measured by the Security Auditor on a
    /// two-entry zip:
    ///
    /// ```text
    /// main   Ok((done 1, skipped 1, [...is a link...]))   second entry delivered = true
    /// branch Err("...could not be opened for writing")    second entry delivered = FALSE
    /// ```
    ///
    /// Containment was never affected — 7,890 planted-link trials, zero escapes — so this is
    /// availability and a half-extracted folder, not an escape. It is still attacker-triggerable with
    /// the same precondition as the original bug, which is why it is fixed rather than recorded.
    ///
    /// **The bystander assertion is the whole test.** `ok.txt` is what separates "skipped the poisoned
    /// entry" from "abandoned the run"; a check on the refusal alone would have passed throughout the
    /// regression, because the refusal was there — it just took the archive down with it.
    ///
    /// Windows-gated: on Unix a directory at a leaf is refused by `child_file`'s `EISDIR` and a symlink
    /// by `link_at`, which has always classified correctly. This is the arm that had no classifier.
    #[cfg(windows)]
    #[test]
    fn cpe_1913_a_junction_at_a_file_entrys_name_skips_that_entry_and_extracts_the_rest() {
        let d = scratch("cpe1913-zip-leaf-junction");
        let dest = d.join("out");
        let elsewhere = d.join("elsewhere");
        fs::create_dir_all(&dest).unwrap();
        fs::create_dir_all(&elsewhere).unwrap();
        // The junction stands where the archive's `note.txt` entry wants to land.
        if !crate::fsutil::make_dir_link(&elsewhere, &dest.join("note.txt")) {
            crate::skip_notice!(
                "SKIPPING cpe_1913_a_junction_at_a_file_entrys_name_skips_that_entry_and_extracts_the_rest: \
                 could not stage a directory link. NOTHING on this run covered the leaf-junction \
                 abort regression"
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }

        let stage = d.join("stage");
        fs::create_dir_all(&stage).unwrap();
        fs::write(stage.join("note.txt"), b"ARCHIVED NOTE").unwrap();
        fs::write(stage.join("ok.txt"), b"ARCHIVED OK").unwrap();
        let archive = d.join("in.zip");
        compress_to_zip(
            &[
                stage.join("note.txt").to_string_lossy().to_string(),
                stage.join("ok.txt").to_string_lossy().to_string(),
            ],
            &archive.to_string_lossy(),
        )
        .unwrap();

        let cancel = AtomicBool::new(false);
        let outcome = extract_archive_streamed(
            &archive.to_string_lossy(),
            &dest.to_string_lossy(),
            &cancel,
            |_| {},
        );

        // HARM FIRST: nothing may have gone through the junction.
        assert!(
            !elsewhere.join("note.txt").exists() && fs::read_dir(&elsewhere).unwrap().count() == 0,
            "HARM: the extraction wrote through the junction standing at the entry's own name: \
             {outcome:?}"
        );
        let report = outcome.expect(
            "a link at ONE entry's name is a per-entry skip — aborting costs the user every other \
             entry and leaves a half-extracted folder (CPE-1913 round 2, F1)",
        );
        assert_eq!(report.skipped, 1, "the poisoned entry must be counted as skipped: {report:?}");
        assert!(
            report.errors.iter().any(|e| e.contains("is a link (a symlink, junction or other reparse point)")),
            "and refused as a LINK, which is what it is — an unclassified 'could not be opened' is the \
             refusal that carried policy: false and took the run down: {report:?}"
        );
        assert_eq!(
            fs::read(dest.join("ok.txt")).ok().as_deref(),
            Some(&b"ARCHIVED OK"[..]),
            "THE POINT: a skip costs ONE entry. ok.txt missing means the run was abandoned: {report:?}"
        );
        let _ = fs::remove_dir_all(&d);
    }

    /// **CPE-1913 round 2, the Reviewer's finding A, at the zip leg: an undescribable destination
    /// handle FAILS the entry — it does not write.** (The heading said ABORTS until CPE-1935 made this
    /// a per-entry failure; the paragraph three below already described the new verdict while the
    /// heading still announced the old one.)
    ///
    /// This is the property `cpe_1857_an_unreadable_probe_refuses_the_entry_rather_than_writing_it`
    /// held before round 1 deleted it, restored against the question the loop actually asks now. That
    /// test drove `entry_sink_action`'s `Unknown` arm through the **path** probe; this loop no longer
    /// asks the path anything, so the same fail-open moved to `handle_facts` returning `None` and round
    /// 1 let it fall through to the write.
    ///
    /// **A failure, not a skip, and the consistency argument is the reason.** `entry_sink_action`'s
    /// `Unknown` arm answers the same way for the tar and 7z legs — pinned by
    /// `cpe1935_an_unreadable_slot_is_a_recorded_entry_failure_on_both_tar_paths`, which is still in
    /// this file — so a zip entry that quietly skipped where a tar entry records a failure would be a
    /// new disagreement inside one module about one condition. The shared gate carries `policy: false`
    /// for this case for exactly that reason.
    ///
    /// **CPE-1935 changed what "not a skip" costs, not what it refuses.** This used to assert an `Err`
    /// from the whole extraction; it now asserts a counted `failed` entry and a bystander that still
    /// landed. The property under test is untouched — *a gate that cannot describe what it is about to
    /// write through must not write* — and the harm assertion below still runs first and still reads
    /// the slot's bytes.
    ///
    /// The fixture is an ordinary occupied slot, deliberately: nothing about it is a link or a hard
    /// link, so the *only* thing that can refuse it is the cannot-describe arm. If that arm goes, this
    /// test does not report a weaker refusal — it reports a successful overwrite.
    #[test]
    fn cpe_1913_an_undescribable_destination_handle_refuses_the_zip_entry() {
        let d = scratch("cpe1913-zip-blind-handle");
        let dest = d.join("out");
        fs::create_dir_all(&dest).unwrap();
        let slot = dest.join("note.txt");
        fs::write(&slot, b"ALREADY HERE").unwrap();
        let zip_path = d.join("aimed.zip");
        fs::write(&zip_path, craft_zip_with_entry_name("note.txt", b"ARCHIVE PAYLOAD")).unwrap();

        let outcome = {
            let _reset = crate::batch_media::ProbeReset::arm(
                crate::batch_media::ProbeInjection::HandleUndescribable,
            );
            extract_archive(&zip_path.to_string_lossy(), &dest.to_string_lossy())
        };

        // HARM FIRST: a destination the gate could not describe must not have been written through.
        assert_eq!(
            fs::read(&slot).ok().as_deref(),
            Some(&b"ALREADY HERE"[..]),
            "HARM: the gate could not describe the handle it was about to write through and extracted \
             the entry anyway — a guard that answers \"no\" when it cannot tell is a guard that is not \
             there: {outcome:?}"
        );
        let report = outcome
            .expect("CPE-1935: one refused entry is recorded, not raised as the whole run's error")
            .report;
        assert_eq!(
            (report.done, report.failed, report.skipped),
            (0, 1, 0),
            "an undescribable slot at a GATE is a refusal, not a silent pass and not a policy skip — \
             the same condition `entry_sink_action`'s `Unknown` arm answers for the tar and 7z legs: \
             {report:?}"
        );
        assert!(
            report.errors.iter().any(|e| e.contains("could not check how many names")),
            "and it must be THIS guard's wording, not an incidental failure from elsewhere in the run: \
             {:?}",
            report.errors
        );
        let _ = fs::remove_dir_all(&d);
    }

    /// **CPE-1857 Security-Auditor finding 1, half two — re-aimed by CPE-1913.**
    ///
    /// The finding was that `entry_sink_action` asked `batch_media::name_links`, a **path** probe, and
    /// folded "could not read the name" into "no name problem", writing the entry. The fix was to abort
    /// on `Unknown`, and this test staged that with `ProbeInjection::Unreadable` over an *ordinary*
    /// destination file — the injection was the only thing making the probe unreadable.
    ///
    /// **CPE-1913 removed the question from the zip leg rather than re-answering it.** The link count
    /// now comes off the **write handle** (`fsutil::claim_destination_handle` →
    /// `batch_media::handle_facts`), the same object the bytes would enter, so there is no path probe
    /// left in the decision for the injection to blind. Over an ordinary file the old fixture would now
    /// extract normally — which is correct, and is what an extraction over an existing file does — so
    /// asserting a refusal there would be asserting a bug.
    ///
    /// The test therefore keeps the injections and changes the fixture to a genuinely **hard-linked**
    /// slot, where a refusal is the right answer, and asserts the refusal happens *with both injections
    /// armed*. That is strictly stronger than what it replaced: an injection that no longer changes the
    /// outcome is evidence the outcome no longer depends on the thing injected.
    ///
    /// `entry_sink_action`'s `Unknown` arm is untouched and still covered for the tar and 7z legs by
    /// `cpe1935_an_unreadable_slot_is_a_recorded_entry_failure_on_both_tar_paths`. (CPE-1935 changed
    /// that arm's *verdict* from `Abort` to `Fail` — one recorded entry failure, the run continuing —
    /// which is why the test carries a new name; the arm itself still refuses to write.)
    #[test]
    fn cpe_1913_the_path_probe_injections_can_no_longer_blind_the_zip_hard_link_gate() {
        for injection in [
            crate::batch_media::ProbeInjection::DegenerateIdentity,
            crate::batch_media::ProbeInjection::Unreadable,
        ] {
            let d = scratch("cpe1857-unreadable-probe");
            let dest = d.join("out");
            let outside = d.join("outside");
            fs::create_dir_all(&dest).unwrap();
            fs::create_dir_all(&outside).unwrap();
            let victim = outside.join("victim.txt");
            fs::write(&victim, b"OUTSIDE CONTENT").unwrap();
            let slot = dest.join("note.txt");
            if fs::hard_link(&victim, &slot).is_err() {
                crate::skip_notice!(
                    "SKIPPING cpe_1913_the_path_probe_injections_can_no_longer_blind_the_zip_hard_link_gate: \
                     no hard-link support here — NOTHING on this run covered the zip leg's hard-link gate"
                );
                return;
            }
            // Liveness: the two names must really be one object, or the test certifies nothing.
            fs::write(&victim, b"OUTSIDE CONTENT").unwrap();
            assert_eq!(
                fs::read(&slot).ok().as_deref(),
                Some(&b"OUTSIDE CONTENT"[..]),
                "fixture is inert: the slot and the outside file are not one object"
            );

            let zip_path = d.join("aimed.zip");
            fs::write(&zip_path, craft_zip_with_entry_name("note.txt", b"ARCHIVE PAYLOAD")).unwrap();

            let outcome = {
                let _reset = crate::batch_media::ProbeReset::arm(injection);
                extract_archive(&zip_path.to_string_lossy(), &dest.to_string_lossy())
            };

            // HARM FIRST, off the filesystem.
            assert_eq!(
                fs::read(&victim).ok().as_deref(),
                Some(&b"OUTSIDE CONTENT"[..]),
                "HARM: the extraction wrote an archive entry's bytes through a hard link into a file \
                 outside the extraction folder: {outcome:?}"
            );
            let report = outcome.expect("a hard-linked slot is a per-entry skip, not a run failure").report;
            assert_eq!(report.done, 0, "nothing landed, so nothing may be counted: {report:?}");
            assert_eq!(report.skipped, 1, "the skip must be reported, not silent: {report:?}");
            assert!(
                report.errors.iter().any(|e| e.contains("hard-linked")),
                "the refusal must come from the HANDLE's link count — a message about an unreadable \
                 probe would mean the path probe is still in the decision: {report:?}"
            );
            let _ = fs::remove_dir_all(&d);
        }
    }

    #[test]
    fn extract_archive_unpacks_a_tar_gz() {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        let d = scratch("targz");
        let tgz = d.join("bundle.tar.gz");
        {
            let f = fs::File::create(&tgz).unwrap();
            let enc = GzEncoder::new(f, Compression::default());
            let mut b = tar::Builder::new(enc);
            let data = b"packed";
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_entry_type(tar::EntryType::Regular);
            header.set_cksum();
            b.append_data(&mut header, "note.txt", &data[..]).unwrap();
            b.into_inner().unwrap().finish().unwrap();
        }
        let out = d.join("out");
        extract_archive(&tgz.to_string_lossy(), &out.to_string_lossy()).unwrap();
        assert_eq!(fs::read_to_string(out.join("note.txt")).unwrap(), "packed");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn read_archive_entries_lists_tar_contents() {
        let d = scratch("tar_list");
        let tar_path = d.join("bundle.tar");
        {
            let f = fs::File::create(&tar_path).unwrap();
            let mut b = tar::Builder::new(f);
            let data = b"hi there";
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_entry_type(tar::EntryType::Regular);
            header.set_cksum();
            b.append_data(&mut header, "hello.txt", &data[..]).unwrap();
            b.finish().unwrap();
        }
        let names: Vec<String> = read_archive_entries(&tar_path.to_string_lossy()).unwrap().into_iter().map(|e| e.name).collect();
        assert!(names.iter().any(|n| n == "hello.txt"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn read_archive_entries_lists_gzip_single_file() {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        let d = scratch("gz_single");
        let gz_path = d.join("note.txt.gz");
        {
            let f = fs::File::create(&gz_path).unwrap();
            let mut enc = GzEncoder::new(f, Compression::default());
            enc.write_all(b"hello world").unwrap();
            enc.finish().unwrap();
        }
        let entries = read_archive_entries(&gz_path.to_string_lossy()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "note.txt", "name is the archive name minus .gz");
        assert_eq!(entries[0].size, 11, "ISIZE trailer is the uncompressed length");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn read_archive_entries_lists_rar_contents() {
        // Build a minimal synthetic RAR4 archive (mirrors crate::rar's own test fixtures, rebuilt here
        // rather than imported since those builders are private to that module) and drive it THROUGH
        // read_archive_entries's dispatch (CPE-1348) — proving the .rar branch is wired to
        // crate::rar::rar_entries, not just exercising rar_entries directly.
        const RAR4_MARKER: [u8; 7] = [0x52, 0x61, 0x72, 0x21, 0x1A, 0x07, 0x00];
        const RAR4_FILE_HEAD: u8 = 0x74;

        fn rar4_file_block(name: &str, unp_size: u32) -> Vec<u8> {
            let mut body = Vec::new();
            body.extend_from_slice(&0u32.to_le_bytes()); // pack_size
            body.extend_from_slice(&unp_size.to_le_bytes()); // unp_size
            body.push(0); // host_os
            body.extend_from_slice(&0u32.to_le_bytes()); // file_crc
            body.extend_from_slice(&0u32.to_le_bytes()); // ftime
            body.push(0); // unp_ver
            body.push(0); // method
            body.extend_from_slice(&(name.len() as u16).to_le_bytes()); // name_size
            body.extend_from_slice(&0u32.to_le_bytes()); // attr
            body.extend_from_slice(name.as_bytes());
            let head_size = (7 + body.len()) as u16;
            let mut block = Vec::new();
            block.extend_from_slice(&0u16.to_le_bytes()); // crc16 (unchecked)
            block.push(RAR4_FILE_HEAD);
            block.extend_from_slice(&0u16.to_le_bytes()); // flags
            block.extend_from_slice(&head_size.to_le_bytes());
            block.extend_from_slice(&body);
            block
        }

        let d = scratch("rar_list");
        let rar_path = d.join("bundle.rar");
        let mut buf = RAR4_MARKER.to_vec();
        buf.extend(rar4_file_block("hello.txt", 8));
        fs::write(&rar_path, &buf).unwrap();

        let entries = read_archive_entries(&rar_path.to_string_lossy()).unwrap();
        let hello = entries.iter().find(|e| e.name == "hello.txt").unwrap();
        assert!(!hello.is_dir);
        assert_eq!(hello.size, 8);
        let _ = fs::remove_dir_all(&d);
    }

    /// Build a minimal `.7z` fixture at `dest` containing `entries` (name, bytes), via `sevenz-rust`'s
    /// own writer — there is no `compress_to_7z` in this crate (CPE-1180 is extraction-only), so the test
    /// packs its own fixture the same way the crate would consume a real one.
    fn write_7z_fixture(dest: &Path, entries: &[(&str, &[u8])]) {
        let mut w = sevenz_rust::SevenZWriter::create(dest).unwrap();
        for (name, data) in entries {
            let mut entry = sevenz_rust::SevenZArchiveEntry::new();
            entry.name = (*name).to_string();
            w.push_archive_entry(entry, Some(std::io::Cursor::new(*data))).unwrap();
        }
        w.finish().unwrap();
    }

    #[test]
    fn extract_archive_entry_any_round_trips_tar_gz_and_7z() {
        // .tar.gz, built via the existing create fn.
        let d = scratch("any_targz");
        fs::create_dir_all(d.join("src/sub")).unwrap();
        fs::write(d.join("src/a.txt"), b"alpha").unwrap();
        fs::write(d.join("src/sub/b.txt"), b"beta").unwrap();
        let tgz = d.join("out.tar.gz");
        compress_to_targz(&[d.join("src").to_string_lossy().to_string()], &tgz.to_string_lossy()).unwrap();

        let tmp = extract_archive_entry_any(&tgz.to_string_lossy(), "src/sub/b.txt").unwrap();
        assert_eq!(fs::read(&tmp).unwrap(), b"beta");
        let _ = fs::remove_file(&tmp);

        // .7z, built via sevenz-rust's own writer (no create fn exists for 7z in this crate).
        let sevenz = d.join("out.7z");
        write_7z_fixture(&sevenz, &[("a.txt", b"alpha7z"), ("sub/b.txt", b"beta7z")]);
        let tmp = extract_archive_entry_any(&sevenz.to_string_lossy(), "sub/b.txt").unwrap();
        assert_eq!(fs::read(&tmp).unwrap(), b"beta7z");
        let _ = fs::remove_file(&tmp);
        let tmp = extract_archive_entry_any(&sevenz.to_string_lossy(), "a.txt").unwrap();
        assert_eq!(fs::read(&tmp).unwrap(), b"alpha7z");
        let _ = fs::remove_file(&tmp);

        // A missing entry is a clear error, not a silent empty file.
        assert!(extract_archive_entry_any(&sevenz.to_string_lossy(), "nope.txt").is_err());
        assert!(extract_archive_entry_any(&tgz.to_string_lossy(), "nope.txt").is_err());

        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn extract_archive_entry_any_round_trips_plain_tar_and_tgz_extension() {
        let d = scratch("any_tar");
        let tar_path = d.join("bundle.tar");
        {
            let f = fs::File::create(&tar_path).unwrap();
            let mut b = tar::Builder::new(f);
            let data = b"hi there";
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_entry_type(tar::EntryType::Regular);
            header.set_cksum();
            b.append_data(&mut header, "hello.txt", &data[..]).unwrap();
            b.finish().unwrap();
        }
        let tmp = extract_archive_entry_any(&tar_path.to_string_lossy(), "hello.txt").unwrap();
        assert_eq!(fs::read(&tmp).unwrap(), b"hi there");
        let _ = fs::remove_file(&tmp);

        // .tgz is the same gzip-tar format under a different extension.
        fs::create_dir_all(d.join("src")).unwrap();
        fs::write(d.join("src/note.txt"), b"tgz-note").unwrap();
        let tgz = d.join("bundle.tgz");
        compress_to_targz(&[d.join("src").to_string_lossy().to_string()], &tgz.to_string_lossy()).unwrap();
        let tmp = extract_archive_entry_any(&tgz.to_string_lossy(), "src/note.txt").unwrap();
        assert_eq!(fs::read(&tmp).unwrap(), b"tgz-note");
        let _ = fs::remove_file(&tmp);

        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn extract_archive_entry_any_delegates_zip_to_the_zip_extractor() {
        let d = scratch("any_zip");
        fs::write(d.join("a.txt"), b"alpha").unwrap();
        let zip_path = d.join("out.zip");
        compress_to_zip(&[d.join("a.txt").to_string_lossy().to_string()], &zip_path.to_string_lossy()).unwrap();
        let tmp = extract_archive_entry_any(&zip_path.to_string_lossy(), "a.txt").unwrap();
        assert_eq!(fs::read(&tmp).unwrap(), b"alpha");
        let _ = fs::remove_file(&tmp);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn extract_archive_entry_any_routes_rar_to_the_rar_extractor() {
        // Hand-build a minimal RAR4 with one STORED entry (the `rar` module's own builders are private
        // to its test module, so assemble the few bytes here) and prove `.rar` now routes to the RAR
        // extractor instead of the ZIP one (which used to fail).
        fn rar4_block(head_type: u8, body: &[u8], payload: &[u8]) -> Vec<u8> {
            let head_size = (7 + body.len()) as u16;
            let mut b = Vec::new();
            b.extend_from_slice(&0u16.to_le_bytes()); // crc16
            b.push(head_type);
            b.extend_from_slice(&0u16.to_le_bytes()); // flags
            b.extend_from_slice(&head_size.to_le_bytes());
            b.extend_from_slice(body);
            b.extend_from_slice(payload);
            b
        }
        let payload = b"rar stored bytes";
        let mut file_body = Vec::new();
        file_body.extend_from_slice(&(payload.len() as u32).to_le_bytes()); // pack_size
        file_body.extend_from_slice(&(payload.len() as u32).to_le_bytes()); // unp_size
        file_body.push(0); // host_os
        file_body.extend_from_slice(&0u32.to_le_bytes()); // crc
        file_body.extend_from_slice(&0u32.to_le_bytes()); // ftime
        file_body.push(0); // unp_ver
        file_body.push(0x30); // method = STORE
        file_body.extend_from_slice(&(b"note.txt".len() as u16).to_le_bytes()); // name_size
        file_body.extend_from_slice(&0u32.to_le_bytes()); // attr
        file_body.extend_from_slice(b"note.txt");

        let mut buf = vec![0x52, 0x61, 0x72, 0x21, 0x1A, 0x07, 0x00]; // RAR4 marker
        buf.extend(rar4_block(0x73, &[], &[])); // MAIN_HEAD
        buf.extend(rar4_block(0x74, &file_body, payload)); // FILE_HEAD (stored)

        let d = scratch("any_rar");
        let path = d.join("a.rar");
        fs::write(&path, &buf).unwrap();
        let tmp = extract_archive_entry_any(&path.to_string_lossy(), "note.txt").unwrap();
        assert_eq!(fs::read(&tmp).unwrap(), payload);
        let _ = fs::remove_file(&tmp);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn extract_archive_entry_any_rejects_a_traversal_inner_regardless_of_format() {
        let d = scratch("any_traversal");
        fs::write(d.join("a.txt"), b"alpha").unwrap();
        let tgz = d.join("out.tar.gz");
        compress_to_targz(&[d.join("a.txt").to_string_lossy().to_string()], &tgz.to_string_lossy()).unwrap();
        let sevenz = d.join("out.7z");
        write_7z_fixture(&sevenz, &[("a.txt", b"alpha")]);

        for bad in ["../evil.txt", "a/../../evil", "..\\evil"] {
            let err = extract_archive_entry_any(&tgz.to_string_lossy(), bad).unwrap_err();
            assert!(err.contains("unsafe"), "tgz: expected an unsafe-entry error, got {err:?}");
            let err = extract_archive_entry_any(&sevenz.to_string_lossy(), bad).unwrap_err();
            assert!(err.contains("unsafe"), "7z: expected an unsafe-entry error, got {err:?}");
        }
        let _ = fs::remove_dir_all(&d);
    }

    // -----------------------------------------------------------------------------------------
    // Streamed compress/extract with progress + cancel (CPE-1184)
    // -----------------------------------------------------------------------------------------

    /// Build a small source tree with enough files that a streamed run emits more than one progress
    /// tick, so cancellation tests have somewhere to stop mid-run.
    fn build_source_tree(d: &Path) -> String {
        fs::create_dir_all(d.join("src/sub")).unwrap();
        for i in 0..6 {
            fs::write(d.join(format!("src/f{i}.txt")), format!("payload-{i}").repeat(50)).unwrap();
        }
        fs::write(d.join("src/sub/nested.txt"), b"nested").unwrap();
        d.join("src").to_string_lossy().to_string()
    }

    #[test]
    fn compress_to_zip_streamed_reports_growing_progress_and_round_trips() {
        let d = scratch("stream_zip_compress");
        let src = build_source_tree(&d);
        let zip_path = d.join("out.zip");
        let cancel = AtomicBool::new(false);
        let mut ticks: Vec<ArchiveProgress> = Vec::new();

        let report =
            compress_to_zip_streamed(&[src], &zip_path.to_string_lossy(), &cancel, |p| ticks.push(p.clone())).unwrap();

        assert!(!report.cancelled);
        assert_eq!(report.done, 7, "6 files + 1 nested file"); // f0..f5 + sub/nested.txt
        assert!(ticks.len() >= 7, "expected a progress tick per file, got {}", ticks.len());
        // Progress is monotonically non-decreasing and the final tick reaches the totals.
        for w in ticks.windows(2) {
            assert!(w[1].done_bytes >= w[0].done_bytes);
            assert!(w[1].done_items >= w[0].done_items);
        }
        let last = ticks.last().unwrap();
        assert_eq!(last.done_items, last.total_items);
        assert_eq!(last.done_bytes, last.total_bytes);

        // Round-trips: the archive really contains what was streamed in.
        let names: Vec<String> =
            read_archive_entries(&zip_path.to_string_lossy()).unwrap().into_iter().map(|e| e.name.replace('\\', "/")).collect();
        assert!(names.iter().any(|n| n.ends_with("f0.txt")));
        assert!(names.iter().any(|n| n.ends_with("sub/nested.txt")));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn compress_to_zip_streamed_can_be_cancelled_mid_run_and_leaves_a_valid_partial_archive() {
        let d = scratch("stream_zip_cancel");
        let src = build_source_tree(&d);
        let zip_path = d.join("partial.zip");
        let cancel = AtomicBool::new(false);
        let mut count = 0u32;

        let report = compress_to_zip_streamed(&[src], &zip_path.to_string_lossy(), &cancel, |_| {
            count += 1;
            if count == 2 {
                cancel.store(true, Ordering::Relaxed);
            }
        })
        .unwrap();

        assert!(report.cancelled);
        assert!(report.done < 7, "cancellation should stop before every file is packed");
        // The partial file is still a valid, openable archive (writer.finish() ran regardless).
        let archive = zip::ZipArchive::new(fs::File::open(&zip_path).unwrap()).unwrap();
        assert!(archive.len() < 7);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn compress_to_targz_streamed_reports_progress_and_round_trips() {
        let d = scratch("stream_targz_compress");
        let src = build_source_tree(&d);
        let tgz = d.join("out.tar.gz");
        let cancel = AtomicBool::new(false);
        let mut ticks = 0u32;

        let report =
            compress_to_targz_streamed(&[src], &tgz.to_string_lossy(), &cancel, |_| ticks += 1).unwrap();

        assert!(!report.cancelled);
        assert_eq!(report.done, 7);
        assert!(ticks >= 7);
        let names: Vec<String> =
            read_archive_entries(&tgz.to_string_lossy()).unwrap().into_iter().map(|e| e.name.replace('\\', "/")).collect();
        assert!(names.iter().any(|n| n.ends_with("f3.txt")));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn compress_archive_streamed_dispatches_by_extension_like_the_one_shot_version() {
        let d = scratch("stream_dispatch");
        fs::write(d.join("f.txt"), b"x").unwrap();
        let src = d.join("f.txt").to_string_lossy().to_string();
        let cancel = AtomicBool::new(false);

        for ext in ["out.zip", "out.tar.gz", "out.tgz"] {
            let dest = d.join(ext).to_string_lossy().to_string();
            let report = compress_archive_streamed(std::slice::from_ref(&src), &dest, &cancel, |_| {})
                .unwrap_or_else(|e| panic!("{ext}: {e}"));
            assert_eq!(report.done, 1);
        }
        let err = compress_archive_streamed(&[src], &d.join("out.rar").to_string_lossy(), &cancel, |_| {}).unwrap_err();
        assert!(err.contains("unsupported"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn compress_to_zip_encrypted_streamed_round_trips_and_rejects_a_wrong_password() {
        let d = scratch("stream_encrypted");
        fs::write(d.join("secret.txt"), b"top secret").unwrap();
        let src = d.join("secret.txt").to_string_lossy().to_string();
        let zip = d.join("locked.zip");
        let cancel = AtomicBool::new(false);

        assert!(compress_to_zip_encrypted_streamed(&[], &zip.to_string_lossy(), "pw", &cancel, |_| {}).is_err());
        assert!(
            compress_to_zip_encrypted_streamed(std::slice::from_ref(&src), &zip.to_string_lossy(), "", &cancel, |_| {})
                .is_err(),
            "an empty password errors"
        );
        let report = compress_to_zip_encrypted_streamed(&[src], &zip.to_string_lossy(), "hunter2", &cancel, |_| {}).unwrap();
        assert_eq!(report.done, 1);

        let out = d.join("out");
        extract_zip_encrypted(&zip.to_string_lossy(), &out.to_string_lossy(), "hunter2").unwrap();
        assert_eq!(fs::read(out.join("secret.txt")).unwrap(), b"top secret");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn check_zip_password_detects_missing_and_wrong_passwords_fast() {
        let d = scratch("check_password");
        fs::write(d.join("secret.txt"), b"top secret").unwrap();
        let src = d.join("secret.txt").to_string_lossy().to_string();
        let zip = d.join("locked.zip");
        compress_to_zip_encrypted(&[src], &zip.to_string_lossy(), "hunter2").unwrap();

        assert!(check_zip_password(&zip.to_string_lossy(), None).is_err(), "no password -> needs one");
        assert!(check_zip_password(&zip.to_string_lossy(), Some("wrong")).is_err(), "wrong password rejected");
        assert!(check_zip_password(&zip.to_string_lossy(), Some("hunter2")).is_ok(), "right password accepted");

        // A plain (unencrypted) archive never needs a password, with or without one supplied.
        let plain = d.join("plain.zip");
        compress_to_zip(&[d.join("secret.txt").to_string_lossy().to_string()], &plain.to_string_lossy()).unwrap();
        assert!(check_zip_password(&plain.to_string_lossy(), None).is_ok());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn extract_archive_streamed_zip_round_trips_and_reports_progress() {
        let d = scratch("stream_zip_extract");
        let src = build_source_tree(&d);
        let zip_path = d.join("out.zip");
        compress_to_zip(&[src], &zip_path.to_string_lossy()).unwrap();

        let cancel = AtomicBool::new(false);
        let mut ticks: Vec<ArchiveProgress> = Vec::new();
        let out = d.join("unpacked");
        let report =
            extract_archive_streamed(&zip_path.to_string_lossy(), &out.to_string_lossy(), &cancel, |p| ticks.push(p.clone()))
                .unwrap();

        assert!(!report.cancelled);
        assert_eq!(report.done, 7, "6 files + 1 nested file");
        assert!(ticks.len() >= 7);
        // total_items is the zip's whole entry count (files + directory placeholders — 7 files + 2 dirs
        // = 9), known up front from the central directory before any entry is read.
        assert_eq!(ticks.first().unwrap().total_items, 9, "totals known up front for zip (central directory)");
        assert_eq!(fs::read_to_string(out.join("src/f0.txt")).unwrap(), "payload-0".repeat(50));
        assert_eq!(fs::read_to_string(out.join("src/sub/nested.txt")).unwrap(), "nested");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn extract_archive_streamed_zip_can_be_cancelled_mid_run() {
        let d = scratch("stream_zip_extract_cancel");
        let src = build_source_tree(&d);
        let zip_path = d.join("out.zip");
        compress_to_zip(&[src], &zip_path.to_string_lossy()).unwrap();

        let cancel = AtomicBool::new(false);
        let mut count = 0u32;
        let out = d.join("unpacked");
        let report = extract_archive_streamed(&zip_path.to_string_lossy(), &out.to_string_lossy(), &cancel, |_| {
            count += 1;
            if count == 2 {
                cancel.store(true, Ordering::Relaxed);
            }
        })
        .unwrap();

        assert!(report.cancelled);
        assert!(report.done < 7);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn extract_archive_streamed_targz_reports_pre_measured_totals() {
        let d = scratch("stream_targz_extract");
        let src = build_source_tree(&d);
        let tgz = d.join("out.tar.gz");
        compress_to_targz(&[src], &tgz.to_string_lossy()).unwrap();

        let cancel = AtomicBool::new(false);
        let mut ticks: Vec<ArchiveProgress> = Vec::new();
        let out = d.join("unpacked");
        let report =
            extract_archive_streamed(&tgz.to_string_lossy(), &out.to_string_lossy(), &cancel, |p| ticks.push(p.clone()))
                .unwrap();

        assert!(!report.cancelled);
        assert_eq!(report.done, 7);
        // The very FIRST emitted tick already knows the totals — proof of the tar pre-measurement pass,
        // not just a running item count that only reaches the total at the end.
        assert_eq!(ticks.first().unwrap().total_items, 7);
        assert!(ticks.first().unwrap().total_bytes > 0);
        assert_eq!(fs::read_to_string(out.join("src/f0.txt")).unwrap(), "payload-0".repeat(50));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn extract_archive_streamed_7z_round_trips_and_reports_progress() {
        let d = scratch("stream_7z_extract");
        let sevenz = d.join("out.7z");
        write_7z_fixture(&sevenz, &[("a.txt", b"alpha7z"), ("sub/b.txt", b"beta7z")]);

        let cancel = AtomicBool::new(false);
        let mut ticks: Vec<ArchiveProgress> = Vec::new();
        let out = d.join("unpacked");
        let report =
            extract_archive_streamed(&sevenz.to_string_lossy(), &out.to_string_lossy(), &cancel, |p| ticks.push(p.clone()))
                .unwrap();

        assert!(!report.cancelled);
        assert_eq!(report.done, 2);
        assert!(ticks.len() >= 2);
        assert_eq!(fs::read(out.join("a.txt")).unwrap(), b"alpha7z");
        assert_eq!(fs::read(out.join("sub/b.txt")).unwrap(), b"beta7z");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn extract_archive_streamed_gz_single_file_is_one_item() {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        let d = scratch("stream_gz_extract");
        let gz_path = d.join("note.txt.gz");
        {
            let f = fs::File::create(&gz_path).unwrap();
            let mut enc = GzEncoder::new(f, Compression::default());
            enc.write_all(b"hello world").unwrap();
            enc.finish().unwrap();
        }
        let cancel = AtomicBool::new(false);
        let out = d.join("unpacked");
        let report =
            extract_archive_streamed(&gz_path.to_string_lossy(), &out.to_string_lossy(), &cancel, |_| {}).unwrap();
        assert_eq!(report.done, 1);
        assert_eq!(fs::read_to_string(out.join("note.txt")).unwrap(), "hello world");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn extract_zip_encrypted_streamed_round_trips_and_rejects_a_wrong_password() {
        let d = scratch("stream_extract_encrypted");
        fs::write(d.join("secret.txt"), b"top secret").unwrap();
        let src = d.join("secret.txt").to_string_lossy().to_string();
        let zip = d.join("locked.zip");
        compress_to_zip_encrypted(&[src], &zip.to_string_lossy(), "hunter2").unwrap();

        let cancel = AtomicBool::new(false);
        let bad = d.join("bad");
        assert!(extract_zip_encrypted_streamed(&zip.to_string_lossy(), &bad.to_string_lossy(), "wrong", &cancel, |_| {})
            .is_err());

        let out = d.join("out");
        let report =
            extract_zip_encrypted_streamed(&zip.to_string_lossy(), &out.to_string_lossy(), "hunter2", &cancel, |_| {})
                .unwrap();
        assert_eq!(report.done, 1);
        assert_eq!(fs::read(out.join("secret.txt")).unwrap(), b"top secret");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn extract_archive_streamed_zip_extraction_does_not_escape_the_destination() {
        let d = scratch("stream_zip_slip");
        let zip_path = d.join("evil.zip");
        fs::write(&zip_path, craft_zip_with_entry_name("../escape.txt", b"pwned")).unwrap();

        let cancel = AtomicBool::new(false);
        let dest = d.join("out");
        // Same invariant as the one-shot extractor's guard test: either an error or a silent skip is
        // acceptable, but the traversal entry must never land outside `dest`.
        let _ = extract_archive_streamed(&zip_path.to_string_lossy(), &dest.to_string_lossy(), &cancel, |_| {});
        assert!(!d.join("escape.txt").exists());
        assert!(!dest.parent().unwrap().join("escape.txt").exists());
        let _ = fs::remove_dir_all(&d);
    }

    // -----------------------------------------------------------------------
    // CPE-1733 — the guarded rows of the create/write provenance table
    // -----------------------------------------------------------------------
    //
    // The table lives in the section comment above `temp_extract_target`; these are its teeth. Rows 1–5
    // and 17 are *recorded absences* (app-owned temp paths, and `create_dir_all`, which CPE-1729 measured
    // as non-destructive) and have nothing to assert. Rows 6–14 refuse the whole operation; rows 15–16
    // skip one entry and keep extracting, so they get their own legs below.

    /// **A "could not check" verdict is NOT a skip** (UAT finding 6) — the table, one row per verdict.
    ///
    /// Rows 15–16 skip an entry whose slot holds a link and keep extracting. Before this, they reached
    /// that decision through `refuse_link_at_new_file(..).is_err()`, which is `true` for **two** different
    /// verdicts: a confirmed link, and a slot whose `symlink_metadata` failed for some other reason. The
    /// second is an I/O failure, and treating it as a skip dropped a file **silently** and reported the
    /// extraction as a success — while every other I/O failure in the same loop was reported as a
    /// failure (a whole-run abort when this was written; one counted entry failure since CPE-1935).
    ///
    /// **This test covers only half of that fix, and the half it does not cover is the half the finding
    /// was about** (PR #906 review, round 4). `entry_slot_action` re-labels a verdict that has *already
    /// been classified*; the choice between "confirmed link" and "could not read it" is made in
    /// `fsutil::create_slot_link_from_stat`. The review mutated that other seam — `Err(_)` classified as
    /// `Link` instead of `Unknown`, which reinstates the finding exactly — and the whole suite stayed
    /// green. Its own leg is `fsutil::tests::an_unreadable_slot_is_unknown_never_a_confirmed_link`; the
    /// two together are the fix, and neither alone is.
    ///
    /// Both are pure-input tests for the same reason: the `Unknown` arm needs a slot that fails to stat
    /// with something other than `NotFound`, which cannot be staged on every platform this ships to — so
    /// with either mapping inline, the one arm that was wrong would again be the one arm nothing reaches.
    ///
    /// **CPE-1935 re-aimed the third assertion without softening it.** The `Unknown` arm was
    /// `EntrySlotAction::Abort`; it is now `Fail`. What this test has always been for — *an unreadable
    /// slot must never be mistaken for a link the guard chose to skip* — is unchanged and still the
    /// thing asserted: the two arms remain distinct, the entry is still not written, and the reason
    /// still reaches the user. Only the blast radius moved, from the whole archive to this entry.
    #[test]
    fn an_unreadable_entry_slot_is_a_failure_not_a_skip_like_a_link() {
        use crate::fsutil::CreateSlotLink;
        assert_eq!(entry_slot_action(CreateSlotLink::NotALink), EntrySlotAction::Write);
        assert_eq!(
            entry_slot_action(CreateSlotLink::Link("it is a link".into())),
            EntrySlotAction::Skip("it is a link".into()),
            "a confirmed link is a policy skip — the rest of the archive must still extract"
        );
        assert_eq!(
            entry_slot_action(CreateSlotLink::Unknown("could not check".into())),
            EntrySlotAction::Fail(EntryFailure::retryable("could not check")),
            "an unreadable slot must be a recorded FAILURE. Skipping it drops a file for a reason that \
             has nothing to do with the archive and still returns a clean report — the silent-success \
             shape this whole ticket family is about"
        );
    }

    /// **Row 1's hardening: a squatted `<pid>-<seq>` directory must be stepped over, not written into.**
    ///
    /// The exact shape the PR #906 review measured against the old `create_dir_all`: pre-create the
    /// directory `temp_extract_target` is about to claim, plant a link inside it at the archive-controlled
    /// leaf name pointing at a victim file, and extract. With `create_dir_all` the extraction walked into
    /// the squatted directory and truncated the victim through the link, returning `Ok`. With the
    /// exclusive `fs::create_dir` the name is skipped and the next sequence number used.
    ///
    /// The assertion is on **the victim's bytes**, not on the returned path — the bug this replaces
    /// returned a perfectly ordinary-looking `Ok(path)` while destroying a file somewhere else entirely.
    ///
    /// This drives `temp_extract_target` through the real public API (`extract_archive_entry`).
    ///
    /// # Why it squats a PRIVATE namespace rather than predicting a shared counter (CPE-1927)
    ///
    /// Two earlier versions both tried to predict which `e<seq>` name the extraction would claim, out of
    /// a counter (`EXTRACT_SEQ`) and a root (`SESSION_ROOT`) that **every sibling test that extracts
    /// anything shares**, and which cargo runs in parallel inside one process.
    ///
    /// - **v1** read the counter and squatted that one name. Its doc claimed a sibling consuming the
    ///   number first would make the leg "announce rather than pass quietly"; **there was no announce
    ///   mechanism**, so when the squat was missed the assertion was trivially true and the test passed in
    ///   silence. With row 1's guard fully removed, PR #906's review measured **two of three runs green**.
    /// - **v2** squatted a 64-wide contiguous block and retried five times, which narrowed the window but
    ///   kept the prediction. Measured on this suite (WSL, 32 cores, cargo's default parallelism): the
    ///   block **silently lost names to a sibling in 2 of 7 full-suite runs** — one run planted only 62 of
    ///   64 links — and in 1 of 7 the extraction was raced 20 names clean past the block, burning an
    ///   attempt. Five such attempts in a row end in `skip_notice!`, which is a **passing** test. Under
    ///   `--test-threads=1` the same run is bit-identical 25 times out of 25 (`start=37 end=101
    ///   landed=101 ours=64`), which is what "shared mutable fixture" looks like from the outside: green
    ///   either way, and only one of the two greens means anything.
    ///
    /// So this version **stops predicting**. It builds its own root and its own `AtomicU64` and drives
    /// the production path through [`ExtractNamespace`] — nothing else can number into either, so the
    /// squat is exact, `create_dir` on every one of the 64 names must succeed, every link is planted, and
    /// the extraction must land at **exactly** `e{SQUAT_BLOCK}`. The retry loop, the `ours` bookkeeping
    /// and the `skip_notice!` are all gone, and the sequence assertion tightened from `>=` to `==`: this
    /// test can no longer pass by being lucky, because there is nothing left to be lucky about.
    ///
    /// The victim assertion still runs before the `Result` is unwrapped, because the bug being guarded
    /// returns an ordinary-looking `Ok(path)` while destroying a file elsewhere.
    ///
    /// **What the private namespace does not prove, and what closes it:** that the hazard is staged at the
    /// address production actually numbers into. The last leg extracts once through the *real*
    /// `extract_archive_entry` and asserts the directory it gets is an `e…` child of `session_root()` —
    /// a race-free check, since it predicts no number.
    ///
    /// # Red-proof (CPE-1927), and the suite delta
    ///
    /// Two sabotages, each run 30× under cargo's default parallelism:
    ///
    /// - `create_dir` → `create_dir_all` in [`temp_extract_target_in`] — the actual CWE-377/CWE-59 bug
    ///   this row guards: **30/30 red**, on `e0`, with the victim assertion naming the damage.
    /// - the injected namespace ignored (fall back to the process globals): **30/30 red** on the landing
    ///   assertion. So the isolation is load-bearing, not decoration.
    ///
    /// **The first 30/30 is a floor check, not a gain — read it that way.** PR #906's review measured the
    /// equivalent sabotage green in 2 of 3 runs, but that was against **v1**, which `main` has not carried
    /// for a long time. CPE-1927's round-2 Reviewer ran the same sabotage against `main`'s **v2** and got
    /// **30/30 red (module) and 8/8 red (full suite)**. So this rewrite does not turn a leaky sabotage
    /// into a caught one: v2 caught it too. The 30/30 says only that the rewrite **did not lose** the
    /// catch, which is worth measuring and is not an improvement.
    ///
    /// **The improvement is determinism and coverage**, and those are the numbers above. v2 was green
    /// whether or not it proved anything: it silently lost names out of its squat block in 2 of 7
    /// full-suite runs measured here, and the round-2 Reviewer — running the harder shape, 124 attempts
    /// under 24 CPU hogs — measured a partly-armed block in **13 of those 124 attempts (~10%)** with no
    /// signal of any kind, plus the raced-clean-past shape directly (`start=83 end=147 landed=148
    /// ours=64 proven=false`). Neither is reachable now: there is no retry, no `ours` bookkeeping and no
    /// `skip_notice!`, every run arms all 64 names, and the landing assertion is `==`. A green means the
    /// same thing every time it appears, which is the property v2 lacked.
    ///
    /// The suite is otherwise **unchanged** — same test count before and after, one test in, one test out;
    /// what changed is that the test's outcome no longer depends on which sibling ran first.
    #[test]
    fn row1_a_squatted_temp_directory_is_stepped_over_not_written_into() {
        /// The squatted run of names. Far under `TEMP_TARGET_ATTEMPTS` (1024) so a walked-over block
        /// never turns into the "could not claim" error.
        const SQUAT_BLOCK: u64 = 64;

        let d = scratch("cpe1733_row1_squat");
        let src = d.join("a.txt");
        fs::write(&src, b"ARCHIVED A").unwrap();
        let zip = d.join("in.zip");
        compress_to_zip(&[src.to_string_lossy().to_string()], &zip.to_string_lossy()).unwrap();

        let victim = d.join("victim-outside-temp.bin");
        fs::write(&victim, b"VICTIM ORIGINAL").unwrap();

        // Probe for symlink privilege away from the block, so "cannot stage" and "raced out" stay
        // distinguishable (CPE-1717: a runner that *should* stage goes red rather than skipping).
        if !crate::fsutil::require_staged("live_file_symlink", true, stage_live_link(&victim, &d.join("probe"))) {
            crate::skip_notice!(
                "[CPE-1733] SKIPPED row 1's squat leg: this machine could not create a file symlink, so \
                 the CWE-377/CWE-59 shape was NOT covered on this run."
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }

        // This test's own extraction namespace: a root nothing else numbers into, and a counter nothing
        // else moves (CPE-1927). The hazard is unchanged — a same-user process, or this test, pre-creating
        // the name the extraction is about to claim — but it is now staged against a fixture this test
        // owns outright, so every step below is deterministic rather than probable.
        let root = d.join("session");
        fs::create_dir(&root).unwrap();
        let seq = std::sync::atomic::AtomicU64::new(0);

        for n in 0..SQUAT_BLOCK {
            let dir = root.join(format!("e{n}"));
            // `unwrap`, not `if is_ok()`: in a private root a name we cannot create is a bug in this
            // fixture, and the version this replaces shrugged that off and silently squatted less.
            fs::create_dir(&dir).unwrap();
            // The leaf name is archive-controlled: an attacker supplying the archive knows it.
            assert!(
                stage_live_link(&victim, &dir.join("a.txt")),
                "row 1: the probe above proved this machine can create file symlinks, so failing to plant \
                 one now means the block is only partly armed — which is how this test used to pass \
                 without covering anything"
            );
        }

        let outcome = extract_archive_entry_in(&zip.to_string_lossy(), "a.txt", Some((&root, &seq)));

        assert_eq!(
            fs::read(&victim).unwrap(),
            b"VICTIM ORIGINAL".to_vec(),
            "row 1: the extraction wrote through a link planted in a SQUATTED temp directory — \
             `create_dir_all` accepts a directory it did not create, so the leaf was never ours \
             (outcome was {outcome:?})"
        );
        let landed = outcome.expect("row 1: a squatted name must be stepped over, not fail the extraction");
        let landed_dir = Path::new(&landed).parent().unwrap().to_path_buf();

        assert_eq!(
            landed_dir,
            root.join(format!("e{SQUAT_BLOCK}")),
            "row 1: the extraction had to start at e0, find all {SQUAT_BLOCK} squatted names occupied and \
             step over every one of them, landing exactly at e{SQUAT_BLOCK}. Anything inside the block \
             means it claimed a directory it did not create — which is what makes rows 2–5's leaf \
             reachable by a pre-planted link — and anything past it means the walk skipped a name it \
             should have tried (landed {landed})"
        );
        assert_eq!(fs::read(&landed).unwrap(), b"ARCHIVED A".to_vec(), "row 1: and it must still extract");

        // The private namespace above proves the *walk*; this proves the walk happens where the hazard
        // can actually be staged — an `e<seq>` child of this process's session root. It predicts no
        // number, so it cannot be raced by a sibling extraction.
        let live = extract_archive_entry(&zip.to_string_lossy(), "a.txt").unwrap();
        let live_dir = Path::new(&live).parent().unwrap().to_path_buf();
        assert_eq!(
            live_dir.parent().unwrap(),
            session_root().unwrap(),
            "row 1: production must number inside this process's session root, or the squat above is \
             staged at an address the real extraction never looks at — a test that could only ever pass"
        );
        assert!(
            live_dir.file_name().unwrap().to_string_lossy().starts_with('e'),
            "row 1: and it must use the `e<seq>` names the squat imitates, not some other shape — {live_dir:?}"
        );
        let _ = fs::remove_dir_all(&live_dir);
        let _ = fs::remove_dir_all(&d);
    }

    // ─── CPE-1786: extraction directories own their lifetime ─────────────────────────────────────
    //
    // The bug these pin is not a wrong answer, it is an *unbounded* one, and the reason it survived two
    // tickets is that the obvious measurement cannot see it: `%TEMP%/cpe-archive` is a single top-level
    // entry, so 1,394,403 leaked directories inside it register as **one**. Every test below therefore
    // asserts on structure one level *down* — how many directories an extraction adds under the shared
    // root, and what reclaims them — rather than on the count of the root itself.

    /// **The shape of the fix, as an assertion: N extractions add ONE directory to the shared root.**
    ///
    /// Before CPE-1786 each extraction added its own `<pid>-<seq>` directly under `cpe-archive`, so N
    /// extractions were N new children of the shared root, forever. Now they are N children of *this
    /// process's* session root, which is one child of the shared root and is reclaimed as a unit. Both
    /// halves are asserted, because dropping either would be a regression in opposite directions:
    /// collapsing the per-extraction directories would bring back the CPE-1195 same-name race, and
    /// putting them back under the shared root would bring back the leak.
    ///
    /// # Why this one keeps the live globals (CPE-1927)
    ///
    /// CPE-1927 named this test alongside `row1_…` as sharing `EXTRACT_SEQ` and `SESSION_ROOT` with every
    /// parallel sibling, and the two answers are opposite on purpose. Row 1 was **predicting** a value out
    /// of the counter, which a sibling can invalidate; this test predicts nothing. Its two claims survive
    /// any interleaving by construction: `dirs.len() == N` because `fetch_add` hands out each number once,
    /// so N extractions get N distinct names no matter who else is drawing from it, and `parents.len() ==
    /// 1` because `SESSION_ROOT` is a `OnceLock` — a sibling can only ever observe the same root this
    /// test does. Handing it a private namespace would make it **vacuous**: "all extractions share one
    /// session root" is a claim about the process-global root, so measuring it anywhere else measures
    /// nothing. It stays on the live globals, and that is the fix, not an omission from it.
    #[test]
    fn cpe_1786_many_extractions_add_one_directory_to_the_shared_root() {
        const N: usize = 25;

        let d = scratch("cpe1786_session");
        let src = d.join("a.txt");
        fs::write(&src, b"ARCHIVED A").unwrap();
        let zip = d.join("in.zip");
        compress_to_zip(&[src.to_string_lossy().to_string()], &zip.to_string_lossy()).unwrap();

        let mut dirs = std::collections::BTreeSet::new();
        let mut parents = std::collections::BTreeSet::new();
        for _ in 0..N {
            let out = extract_archive_entry(&zip.to_string_lossy(), "a.txt").unwrap();
            let dir = Path::new(&out).parent().unwrap().to_path_buf();
            parents.insert(dir.parent().unwrap().to_path_buf());
            dirs.insert(dir);
        }

        assert_eq!(
            dirs.len(),
            N,
            "each extraction must still get its own private directory — that is CPE-1195's fix, and \
             collapsing it would re-race two concurrent extractions of same-named entries"
        );
        assert_eq!(
            parents.len(),
            1,
            "all {N} extractions must sit inside ONE session directory; {} distinct parents means they \
             are being added to the shared root again, which is the leak CPE-1786 measured at 1,394,403 \
             directories",
            parents.len()
        );

        let session = parents.into_iter().next().unwrap();
        assert_eq!(session, session_root().unwrap(), "and that parent must be this session's root");
        let name = session.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            name.starts_with('s') && is_our_temp_dir_name(&name),
            "the session directory {name} must carry the unguessable `s<pid>-<random>` name — a bare \
             `<pid>` is what a recycled PID collides with"
        );
        assert_eq!(
            session.parent().unwrap(),
            std::env::temp_dir().join(ARCHIVE_TEMP_ROOT),
            "and it must live under the shared root the frontend and the sweeper both expect"
        );

        for dir in dirs {
            let _ = fs::remove_dir_all(dir);
        }
    }

    /// The one load-bearing property of the session name, measured rather than asserted: **it varies**.
    ///
    /// The whole reason the session directory is not just `s<pid>` is that a recycled PID is exactly what
    /// CPE-1786 measured colliding, so the trailing half has to actually differ between calls. A name
    /// that repeated would turn the exclusive `create_dir` in [`claim_session_root`] into a permanent
    /// collision and drop every session into degraded mode.
    ///
    /// **What this does NOT measure** (PR #945 final verifier, and the reason this paragraph exists):
    /// it does not isolate `RandomState`'s contribution. [`session_dir_name`] also folds in a nanosecond
    /// clock reading, and with a fixed-seed `DefaultHasher` swapped in, this test **still passed** — all
    /// 64 distinct, on the clock term alone. So this is evidence for exactly one sentence, "the name
    /// varies", which is the operationally load-bearing one; it is not evidence about either input on its
    /// own, and the doc on `session_dir_name` no longer claims otherwise.
    #[test]
    fn cpe_1786_session_names_vary_between_calls() {
        let names: std::collections::BTreeSet<String> = (0..64).map(|_| session_dir_name()).collect();
        assert_eq!(names.len(), 64, "session names must not repeat — {names:?}");
        let prefix = format!("s{}-", std::process::id());
        for name in &names {
            assert!(name.starts_with(&prefix), "{name} must carry this process's id");
            assert!(is_our_temp_dir_name(name), "{name} must be a name the sweeper recognises as ours");
        }
    }

    /// The sweeper's licence to delete, pinned name by name. It runs `remove_dir_all` inside a directory
    /// shared with every other process on the machine — and, on a Unix `/tmp`, every other *user* — so
    /// "which names are ours" is a safety property, not a formatting detail.
    #[test]
    fn cpe_1786_only_names_this_module_creates_are_sweepable() {
        for ours in ["s1234-00000000deadbeef", "s7-0000000000000000", "e0", "e991", "4321-7", "1-1"] {
            assert!(is_our_temp_dir_name(ours), "{ours} is a name this module creates");
        }
        for theirs in [
            "",
            "s",
            "e",
            "s1234",              // no random half — not a shape this ever wrote
            "s1234-nothex",       // the random half must be hex
            "sabcd-00000000",     // the pid half must be digits
            "ea",                 // `e` + a non-number
            "cpe-archive-staging",
            "someone-elses-junk",
            "-7",
            "rustc-uplift",
        ] {
            assert!(
                !is_our_temp_dir_name(theirs),
                "{theirs} was NOT created by this module, and a sweeper that removes unrecognised names \
                 in a shared temp directory is a recursive delete pointed at somebody else's data"
            );
        }
    }

    /// **Cross-session reclamation: the half that stops growth between runs.** A dead session's directory
    /// is removed, this session's is not, a stranger's is not, and the pre-CPE-1786 `<pid>-<seq>` shape is
    /// reclaimed too — which is how the 1.39 million already on disk drain rather than being frozen.
    ///
    /// The age decision is driven by the `now` parameter rather than by sleeping or by forging a
    /// directory's mtime (which `std` has no portable API for), so the one-hour behaviour is actually
    /// asserted instead of being assumed from a shorter proxy.
    #[test]
    fn cpe_1786_sweep_reclaims_dead_sessions_and_leaves_everything_else() {
        let ttl = std::time::Duration::from_secs(60 * 60);
        let d = scratch("cpe1786_sweep");
        let root = d.to_path_buf();

        let keep = root.join("s999-00000000000000ff");
        fs::create_dir(&keep).unwrap();
        let dead: Vec<PathBuf> = (0..3)
            .map(|i| {
                let p = root.join(format!("s{i}-000000000000000{i}"));
                fs::create_dir(&p).unwrap();
                // A real extraction directory with a real file in it, so the removal is recursive.
                fs::create_dir(p.join("e0")).unwrap();
                fs::write(p.join("e0").join("a.txt"), b"leftover").unwrap();
                p
            })
            .collect();
        let legacy = root.join("4321-7"); // the pre-CPE-1786 shape
        fs::create_dir(&legacy).unwrap();
        let stranger = root.join("someone-elses-junk");
        fs::create_dir(&stranger).unwrap();
        let not_a_dir = root.join("1234-1"); // our name shape, but a file
        fs::write(&not_a_dir, b"not a directory").unwrap();

        let now = std::time::SystemTime::now();
        assert_eq!(
            sweep_stale_sessions(&root, &keep, now, ttl, 64, 32),
            0,
            "nothing here is an hour old yet — sweeping a session that might still be live is the one \
             way this mechanism could break a running instance"
        );

        let later = now + std::time::Duration::from_secs(2 * 60 * 60);
        assert_eq!(
            sweep_stale_sessions(&root, &keep, later, ttl, 64, 2),
            2,
            "the remove budget is what keeps this off the critical path of the first extraction; \
             ignoring it would let one launch pay for 1.39 million directories"
        );
        assert_eq!(
            sweep_stale_sessions(&root, &keep, later, ttl, 64, 32),
            2,
            "the remaining two stale directories go on the next pass"
        );

        for p in &dead {
            assert!(!p.exists(), "a session untouched for {ttl:?} must be reclaimed: {}", p.display());
        }
        assert!(!legacy.exists(), "the pre-CPE-1786 `<pid>-<seq>` shape must drain through the same sweep");
        assert!(keep.exists(), "this session's own root must never be swept by this session");
        assert!(stranger.exists(), "a name we did not create must be left alone");
        assert!(not_a_dir.exists(), "a file wearing one of our names is not a session directory");
    }

    /// The sweeper must not delete through a link. `%TEMP%` is a place a junction can be planted without
    /// privilege on Windows, and CPE-1693's PR #934 measured a bulk-delete mechanism following exactly
    /// that out of `%TEMP%` and destroying the far side (`robocopy /MIR`, even with `/XJ`). The entry is
    /// skipped outright rather than trusted to `remove_dir_all`'s reparse-point semantics.
    #[test]
    fn cpe_1786_sweep_never_deletes_through_a_link() {
        let d = scratch("cpe1786_sweep_link");
        let root = d.join("root");
        fs::create_dir(&root).unwrap();
        let victim = d.join("victim");
        fs::create_dir(&victim).unwrap();
        fs::write(victim.join("canary.txt"), b"CANARY").unwrap();

        let link = root.join("s4242-00000000deadbeef");
        if !crate::fsutil::make_dir_link(&victim, &link) {
            crate::skip_notice!(
                "[CPE-1786] SKIPPED the sweeper's link leg: this machine could not create a directory \
                 link, so the follow-a-junction-out-of-%TEMP% shape was NOT covered on this run."
            );
            return;
        }
        let keep = root.join("s1-0000000000000001");
        fs::create_dir(&keep).unwrap();

        let removed = sweep_stale_sessions(
            &root,
            &keep,
            std::time::SystemTime::now() + std::time::Duration::from_secs(2 * 60 * 60),
            std::time::Duration::from_secs(60 * 60),
            64,
            32,
        );

        assert_eq!(removed, 0, "a link wearing a session name must be skipped, not removed");
        assert!(
            victim.join("canary.txt").exists(),
            "the sweep followed a link out of the shared root and deleted the far side — the exact \
             mechanism CPE-1693's purge had to be rewritten to avoid"
        );
        assert!(fs::symlink_metadata(&link).is_ok(), "and the link itself is left where it is");
    }

    /// **The blocker from PR #945's review, as a test that can actually go red.**
    ///
    /// The test this replaces staged all 200 of its "batch" entries with the *identical* timestamp, so
    /// the batch was instantaneous and could never trip a time-based reap — the assertion held for every
    /// possible `max_live` and `grace`, including the broken implementation it was supposed to be
    /// guarding. A test that cannot fail is not evidence, and this one was sitting on the exact property
    /// under review.
    ///
    /// The fix is to stage the batch the way `FileList.svelte`'s alt-drag really stages one: entries
    /// spread **one second apart over 100 seconds**, i.e. spanning more than the grace, with the newest
    /// pushed just now because the loop is still running. Against the old oldest-entry-age rule the front
    /// is 100 s old, the cap is exceeded, and 36 directories are reclaimed out from under a drag that has
    /// not been handed to the OS yet — the silent data-loss path. Against the quiet rule nothing is
    /// touched, because the process started an extraction a moment ago.
    ///
    /// Verified red before the fix (`due.len()` was 36 here, and the batch shrank to 64 of its 100), and
    /// green after.
    #[test]
    fn cpe_1786_a_batch_still_being_staged_is_never_reclaimed_however_long_it_takes() {
        const CAP: usize = 64;
        const HARD_CAP: usize = 4096;
        let grace = std::time::Duration::from_secs(60);
        // Built forwards, never backwards: `Instant - Duration` can panic on a machine that booted
        // moments ago, which a CI runner genuinely is.
        let start = std::time::Instant::now();
        let per_entry = std::time::Duration::from_secs(1);

        // A 100-entry alt-drag, one second per entry (a large `.tar.gz` re-decodes from byte zero for
        // every entry — O(n²) — so seconds per entry is the realistic figure, not the pathological one).
        // "Now" is the moment the loop is about to stage entry 100; entry 0 is 100 seconds old.
        let mut batch: std::collections::VecDeque<(PathBuf, std::time::Instant)> =
            (0..100).map(|i| (PathBuf::from(format!("f{i}")), start + per_entry * i)).collect();
        let now = start + per_entry * 100;
        assert!(
            now.duration_since(batch.front().unwrap().1) > grace,
            "the batch must genuinely span more than the grace, or this test proves nothing — that is \
             the defect in the version it replaces"
        );

        let due = drain_reapable(&mut batch, CAP, HARD_CAP, grace, now);
        assert!(
            due.is_empty(),
            "{} directories were reclaimed from a drag-out that has not been handed to the OS yet. \
             `startFileDrag` is called after the loop, so those paths are copied by the OS and silently \
             dropped — with no error anywhere, because the extraction returned Ok before the reap",
            due.len()
        );
        assert_eq!(batch.len(), 100, "and the batch is intact");

        // The same queue once the burst is over: quiet for longer than the grace, so the cap applies.
        let after = now + grace + std::time::Duration::from_secs(1);
        let due = drain_reapable(&mut batch, CAP, HARD_CAP, grace, after);
        assert_eq!(due.len(), 36, "once quiet, 100 live against a cap of 64 leaves 36 due");
        assert_eq!(due[0], PathBuf::from("f0"), "and they go oldest-first");
        assert_eq!(batch.len(), CAP, "the cap is what a long session is bounded to");

        // Under the cap nothing goes, however old and however quiet.
        let mut small: std::collections::VecDeque<(PathBuf, std::time::Instant)> =
            (0..10).map(|i| (PathBuf::from(format!("s{i}")), start)).collect();
        assert!(
            drain_reapable(&mut small, CAP, HARD_CAP, grace, after).is_empty(),
            "under the cap nothing is reclaimed however old it is — an idle session keeps its previews"
        );
    }

    /// **The re-review's 601-entry probe, and the residual it measures — kept as a test precisely
    /// because it is the claim that was wrong twice.**
    ///
    /// The shipped comment used to say the residual needed "more than 512 entries each taking over a
    /// minute — eight hours of staging". That is a *sufficient* condition dressed as a *necessary* one.
    /// `quiet` reads only `live.back()`, so **one** inter-entry gap over the grace flips the gate for
    /// that single push, however fast everything else was — and the O(n²) re-decode means the long gaps
    /// arrive exactly when the queue is longest.
    ///
    /// Both halves are pinned here, so the boundary is a measurement rather than a sentence:
    ///
    /// - a batch that spans **hours** with every gap under the grace is untouched — this is what the
    ///   quiet gate really buys, and it is the common case;
    /// - a batch with **one** gap over the grace is *not* protected. That is the residual. It is
    ///   asserted, not wished away: if someone later finds a signal that closes it, this test tells them
    ///   they have, and until then nobody can read the comment and believe it takes eight hours.
    #[test]
    fn cpe_1786_the_quiet_gate_protects_a_slow_batch_but_one_long_gap_is_the_known_residual() {
        const CAP: usize = 512;
        const HARD_CAP: usize = 4096;
        let grace = std::time::Duration::from_secs(60);
        let start = std::time::Instant::now();

        // A 601-entry batch, every gap 100 ms: four hours of staging would still be safe, but this one
        // takes a minute. Nothing may be reclaimed — the process has never been quiet.
        let fast = std::time::Duration::from_millis(100);
        let mut batch: std::collections::VecDeque<(PathBuf, std::time::Instant)> =
            (0..601u32).map(|i| (PathBuf::from(format!("f{i}")), start + fast * i)).collect();
        let now = start + fast * 601;
        assert!(
            now.duration_since(batch.front().unwrap().1) > grace,
            "the batch must span more than the grace or this proves nothing"
        );
        assert!(
            drain_reapable(&mut batch, CAP, HARD_CAP, grace, now).is_empty(),
            "every gap is under the grace, so the process has never been quiet and the whole batch is \
             still being staged — nothing may be reclaimed however long the batch has run"
        );
        assert_eq!(batch.len(), 601);

        // Now the residual, in the re-reviewer's exact shape: the same batch, except entry 600 takes
        // 61 seconds to extract. Its directory is created (and timestamped) when that extraction
        // *starts*, so the gap the gate sees is the one at the **next** push — entry 601, 61 s later,
        // with entry 600 sitting at the back looking a minute stale. That single gap is
        // indistinguishable from the user going for coffee, so the gate opens and the cut to `max_live`
        // happens. THIS IS A KNOWN LOSS, pinned so it stays known.
        let mut probe: std::collections::VecDeque<(PathBuf, std::time::Instant)> =
            (0..601u32).map(|i| (PathBuf::from(format!("f{i}")), start + fast * i)).collect();
        let now = start + fast * 600 + std::time::Duration::from_secs(61);
        assert_eq!(
            now.duration_since(probe.front().unwrap().1).as_secs(),
            121,
            "the re-reviewer's probe is 121 s after the batch started — pinned so a later edit cannot \
             drift this away from the shape that was actually measured"
        );
        let due = drain_reapable(&mut probe, CAP, HARD_CAP, grace, now);
        assert_eq!(
            due.len(),
            601 - CAP,
            "the known residual: one inter-entry gap over the grace exposes the whole overflow. If this \
             assertion starts failing because a fix closed the hole, that is good news — update the \
             residual paragraph on `drain_reapable` and this doc, do not relax the number"
        );
        assert_eq!(due[0], PathBuf::from("f0"));
    }

    /// The invariant the re-review's prescribed two-line fix depends on, pinned so the reasoning behind
    /// *not* taking that fix cannot silently rot.
    ///
    /// The prescription was: when `quiet`, also require the popped entry's own age ≥ grace. Timestamps
    /// are `Instant::now()` taken **under the queue's own lock immediately before the push**, so the
    /// queue is in non-decreasing time order and `front` is never newer than `back`. Therefore
    /// `now - front ≥ now - back`, and whenever `quiet` holds (`now - back ≥ grace`) the extra condition
    /// is already satisfied for every entry in the queue — it can never reject anything. Shipping it
    /// would have added a condition that cannot fire: code-shaped version of the identical-timestamp
    /// test this ticket has already been caught by once.
    ///
    /// The real mitigation taken instead was raising [`REAP_GRACE`], which shrinks the boundary rather
    /// than decorating it. If a future change ever makes these timestamps non-monotonic (a different
    /// clock, an out-of-order push, a queue reordered on removal) this test goes red, and the
    /// prescription becomes live again.
    ///
    /// # It has to actually observe entries, and the first version did not
    ///
    /// As first written this locked the process-global queue and iterated whatever it happened to
    /// contain — which, instrumented by the PR #945 final verifier, was **nothing**:
    ///
    /// ```text
    /// [VERIFIER] monotonic test saw queue len = 0   (isolated run)
    /// [VERIFIER] monotonic test saw queue len = 0   (full lib run, 2229 tests)
    /// [VERIFIER] monotonic test saw queue len = 0   (full lib run, repeat)
    /// [VERIFIER] monotonic test saw queue len = 0   (full lib run, repeat)
    /// ```
    ///
    /// Four for four the assertion loop never executed. The only test that populates the queue does 25
    /// real zip extractions; this one finished in 0.00 s and reliably won the race. So the guard offered
    /// in exchange for declining the prescribed fix was **the code-shaped identical-timestamp test,
    /// inside the artifact built to stop identical-timestamp tests** — a vacuous pass, three rounds after
    /// the first one was caught.
    ///
    /// It now **pushes its own entries through the real [`note_extraction_dir`]** — several threads, so
    /// the ordering claim is exercised against genuine lock contention rather than a single-threaded
    /// sequence — and then asserts *both* that the order is non-decreasing *and* that it saw at least
    /// what it pushed. **An empty queue is now a failure**, which is the property the first version was
    /// missing. The count is well under [`MAX_LIVE_EXTRACTIONS`] so nothing is evicted, and the paths are
    /// real directories under this test's own scratch, so the best-effort removal in `note_extraction_dir`
    /// can never touch anything else even if a future change did evict them.
    ///
    /// # Why reading a process-global queue in parallel is safe here (CPE-1927 sweep)
    ///
    /// Written down because this is the one place in the crate that **reads a shared mutable global while
    /// siblings write it**, and the sweep for `EXTRACT_SEQ`'s shape nearly missed it — see the ticket.
    /// Both claims survive any interleaving, for two different reasons:
    ///
    /// - **Ordering.** [`note_extraction_dir`] samples `Instant::now()` *inside* the same critical section
    ///   that does the `push_back`, so timestamps enter the queue in the order the mutex grants it. The
    ///   non-decreasing property is enforced by the lock, not by luck — a sibling cannot interleave an
    ///   older timestamp behind a newer one. (Sampling the instant **before** taking the lock would break
    ///   exactly this; do not move it.)
    /// - **Count.** `seen` filters on this test's own scratch root and the assertion is `>=`, so sibling
    ///   pushes can only ever add entries this test ignores. The only thing that could *subtract* is
    ///   eviction, and [`drain_reapable`] cuts only above 512 (quiet) or 4096 (busy) entries after a
    ///   ten-minute gap — the whole lib suite pushes on the order of tens.
    #[test]
    fn cpe_1786_the_live_queue_is_monotonic_so_the_front_is_never_newer_than_the_back() {
        const THREADS: usize = 4;
        const PER_THREAD: usize = 8;
        const PUSHED: usize = THREADS * PER_THREAD;

        let d = scratch("cpe1786_monotonic");
        let root = d.to_path_buf();
        std::thread::scope(|s| {
            for t in 0..THREADS {
                let root = root.clone();
                s.spawn(move || {
                    for i in 0..PER_THREAD {
                        let dir = root.join(format!("t{t}-e{i}"));
                        fs::create_dir_all(&dir).unwrap();
                        note_extraction_dir(dir);
                    }
                });
            }
        });

        let live = LIVE_EXTRACTIONS.lock().unwrap_or_else(|e| e.into_inner());
        let mut previous: Option<std::time::Instant> = None;
        let mut seen = 0usize;
        for (path, started) in live.iter() {
            if let Some(previous) = previous {
                assert!(
                    *started >= previous,
                    "the live-extraction queue went backwards at {} — `drain_reapable`'s reasoning that \
                     the front is always at least as old as the back no longer holds, and the two-line \
                     guard declined in PR #945 stops being a no-op. Re-read its residual section.",
                    path.display()
                );
            }
            previous = Some(*started);
            if path.starts_with(&root) {
                seen += 1;
            }
        }
        assert!(
            seen >= PUSHED,
            "this test observed only {seen} of the {PUSHED} entries it pushed, so the ordering assertion \
             above proved little or nothing. An empty or truncated queue must FAIL here — a silent pass \
             is exactly the defect this test was rewritten to remove."
        );
    }

    /// The other end of the quiet rule: a caller that is **never** quiet must not be able to hold the
    /// reclamation open forever, or "bounded" is back to being a claim rather than a property. Past the
    /// hard cap it runs anyway — and cuts only down to the hard cap, the smallest cut that restores the
    /// bound, rather than all the way to `max_live`.
    #[test]
    fn cpe_1786_a_session_that_is_never_quiet_is_still_bounded() {
        const CAP: usize = 64;
        const HARD_CAP: usize = 4096;
        let grace = std::time::Duration::from_secs(60);
        let start = std::time::Instant::now();
        let per_entry = std::time::Duration::from_millis(10);

        let n = HARD_CAP + 100;
        let mut busy: std::collections::VecDeque<(PathBuf, std::time::Instant)> = (0..n)
            .map(|i| (PathBuf::from(format!("b{i}")), start + per_entry * (i as u32)))
            .collect();
        let now = start + per_entry * (n as u32);

        let due = drain_reapable(&mut busy, CAP, HARD_CAP, grace, now);
        assert_eq!(due.len(), 100, "only the overflow goes while the caller is still busy");
        assert_eq!(busy.len(), HARD_CAP, "and it is cut to the hard cap, not to the ordinary cap");
        assert_eq!(due[0], PathBuf::from("b0"), "oldest-first here too");
    }

    /// [`cleanup_extraction_scratch`]'s recursive delete refuses anything that is not a session
    /// directory. In degraded mode the process's "session root" *is* the shared `cpe-archive` root, and
    /// removing that on shutdown would delete every other running instance's extractions.
    #[test]
    fn cpe_1786_session_cleanup_refuses_the_shared_root() {
        let d = scratch("cpe1786_cleanup");
        let session = d.join("s1-0000000000000001");
        fs::create_dir(&session).unwrap();
        fs::write(session.join("a.txt"), b"x").unwrap();
        let shared = d.join(ARCHIVE_TEMP_ROOT);
        fs::create_dir(&shared).unwrap();
        fs::write(shared.join("a.txt"), b"x").unwrap();

        remove_session_tree(&shared);
        assert!(
            shared.exists(),
            "the shared root is what degraded mode holds; deleting it on shutdown would take other \
             instances' live extractions with it"
        );

        remove_session_tree(&session);
        assert!(!session.exists(), "and a real session directory is removed, tree and all");
    }

    /// **Degraded-mode shutdown removes what it recorded** (PR #945 review). The first version `clear()`ed
    /// the bookkeeping and then asked [`remove_session_tree`] to remove the shared root, which correctly
    /// refuses — so shutdown removed nothing *and* destroyed the only record of the `e<seq>` directories
    /// sitting directly under it. Both shapes are pinned here, because the normal one passing was what
    /// made the degraded one look covered.
    #[test]
    fn cpe_1786_shutdown_removes_the_recorded_directories_even_when_the_tree_is_refused() {
        let d = scratch("cpe1786_cleanup_degraded");

        // Degraded mode: "session root" IS the shared root, so the tree removal must refuse — and the
        // recorded directories are then the only thing that can be reclaimed.
        let shared = d.join(ARCHIVE_TEMP_ROOT);
        fs::create_dir(&shared).unwrap();
        let recorded: Vec<PathBuf> = (0..3)
            .map(|i| {
                let p = shared.join(format!("e{i}"));
                fs::create_dir(&p).unwrap();
                fs::write(p.join("a.txt"), b"extracted").unwrap();
                p
            })
            .collect();
        cleanup_session(Some(&shared), recorded.clone());
        for p in &recorded {
            assert!(!p.exists(), "shutdown must remove what it recorded: {}", p.display());
        }
        assert!(shared.exists(), "while still refusing to remove the shared root itself");

        // The ordinary shape: the whole session tree goes, recorded directories included.
        let session = d.join("s1-0000000000000001");
        fs::create_dir(&session).unwrap();
        let inner = session.join("e0");
        fs::create_dir(&inner).unwrap();
        cleanup_session(Some(&session), vec![inner]);
        assert!(!session.exists(), "and a real session tree goes in one piece");
    }

    /// **A recorded absence, closed by CPE-1758** (PR #906 review, finding 2 — the gap this test used to
    /// pin; see the section comment above for the "before" measurement).
    ///
    /// The table above says `guarded_join`'s traversal answer is already covered here by
    /// [`entry_name_is_safe`], and that is true *for traversal*. It used NOT to be true for the rest of
    /// what `guarded_join` carries: [`crate::transfer::is_safe_name`] fails closed on a `:` anywhere in a
    /// segment and on a leading `..`, and this module's check used to accept both. A ZIP entry named
    /// `file:stream` therefore used to reach rows 15–16's `File::create` and, on NTFS, disappear into an
    /// alternate data stream of a neighbouring file — measured: `fs::write("adsbase:stream")` → `Ok`,
    /// `adsbase` still 4 bytes, no visible file created.
    ///
    /// **This test used to assert the gap; CPE-1758 re-points it to assert the fix, in the same commit
    /// that closed the gap — never deleted.** It exists because a paragraph saying "we now cover `:`"
    /// rots the moment someone changes either function, whereas this fails. If this test goes red, either
    /// [`entry_name_is_safe`] or [`crate::transfer::is_safe_name`] moved and the section comment above
    /// (and `src/docs/explorer-archives.md`'s zip-slip bullet) needs to move with it.
    ///
    /// **Re-aimed once already by CPE-1744, which did NOT fix this** (that ticket closed the
    /// *containment* half of the `guarded_join` comparison, the intermediate-directory escape — a
    /// question about where a path lands — and deliberately left this half, a question about what a
    /// *segment* may be called). Renamed here from
    /// `entry_name_is_safe_accepts_shapes_transfers_is_safe_name_rejects` because that name described the
    /// disagreement this test now asserts is gone.
    ///
    /// This covers the platform-independent half of the fix — the shapes [`crate::transfer::is_safe_name`]
    /// itself rejects, which is every CI OS. The Windows-only half (reserved device names, trailing
    /// dot/space — [`crate::transfer::local_safe_segment`]'s job, which `is_safe_name` does not know
    /// about) is a separate, `#[cfg(windows)]`-gated test right below, so a regression in either
    /// half-guard goes red on its own row rather than being hidden behind the other.
    #[test]
    fn entry_name_is_safe_now_agrees_with_transfers_is_safe_name() {
        // (name, what this module says, what the transfer sink says)
        let rows: &[(&str, bool, bool)] = &[
            // These three are caught by `is_safe_name` ALONE — `local_safe_segment` never rewrites any of
            // them on any OS (no WINDOWS_UNSAFE_CHARS-only reason to single them out: "..evil" has no
            // unsafe char, isn't a device name, has no trailing dot/space), so if the `is_safe_name` call
            // were ever dropped from `entry_name_is_safe`, this row goes red on Linux and macOS too, not
            // just Windows.
            ("..evil", false, false), // leading `..` that is not a traversal component
            ("..:$DATA", false, false), // same, plus a colon
            // Colon is also a WINDOWS_UNSAFE_CHARS entry, so on Windows BOTH guards independently reject
            // this one — still a useful row: it is the exact CPE-1709/M7 measured bug shape.
            ("file:stream", false, false), // NTFS alternate data stream — CPE-1709's bug shape
            ("a/b.txt", true, false), // a separator: legal to us (we join it), never a single segment —
            // deliberately UNCHANGED disagreement: `is_safe_name` only ever judges one segment in
            // isolation and rejects anything containing `/`, while `entry_name_is_safe` is allowed to
            // accept a multi-segment relative path. A row here that started matching would mean
            // `entry_name_is_safe` regressed to rejecting ordinary nested entries.
            // Agreed rejections, so a change that broke BOTH would still red here rather than pass.
            ("..", false, false),
            ("../x", false, false),
            ("", false, false),
        ];
        for (name, ours, theirs) in rows {
            assert_eq!(
                entry_name_is_safe(name),
                *ours,
                "archive::entry_name_is_safe({name:?}) changed — if this un-does the CPE-1758 fix, update \
                 the table in this module's section comment (and src/docs/explorer-archives.md) too"
            );
            assert_eq!(
                crate::transfer::is_safe_name(name),
                *theirs,
                "transfer::is_safe_name({name:?}) changed — the recorded delta in this module's section \
                 comment is measured against it and is now stale"
            );
        }
    }

    /// **The Windows-only half of the CPE-1758 fix** — reserved DOS device names and a trailing run of
    /// `.`/space, which is [`crate::transfer::local_safe_segment`]'s job (via `windows_safe_segment`), not
    /// [`crate::transfer::is_safe_name`]'s: `is_safe_name` has no device-name or trailing-character logic
    /// at all, so none of these three shapes appear in
    /// `entry_name_is_safe_now_agrees_with_transfers_is_safe_name` above — they would pass `is_safe_name`
    /// on every OS. `#[cfg(windows)]` because `local_safe_segment` is the identity function everywhere
    /// else (`cfg!(windows)` inside it): `"con"`, `" sp "` and `"x."` are ordinary, legal filenames on
    /// Linux and macOS, so asserting `false` there would be asserting a Windows-only hazard as if it were
    /// universal — exactly the mistake CI's 3-OS matrix exists to catch (never assert Windows-only shapes
    /// unconditionally).
    ///
    /// Distinctive refusal, not `is_err()`: this asserts the boolean `entry_name_is_safe` returns
    /// directly, so there is no ambiguity with `File::create` independently failing on some of these
    /// shapes (CPE-1709 already measured that `CreateFileW` refuses a couple of the unsafe-char cases
    /// outright) — that failure mode is not exercised here at all, only the name predicate.
    #[test]
    #[cfg(windows)]
    fn entry_name_is_safe_rejects_windows_device_names_and_trailing_dot_space() {
        for name in ["con", "CON", "con.txt", "nul", " sp ", "x.", "trailing "] {
            assert!(
                !entry_name_is_safe(name),
                "entry_name_is_safe({name:?}) should be false on Windows — local_safe_segment would \
                 rewrite this segment, so it is one of the CPE-1758 shapes"
            );
            // The reason, not only the effect: pin that it's `local_safe_segment` doing the rejecting,
            // not `is_safe_name` (which does not know about device names or trailing runs at all) and not
            // some other accident — so this row cannot pass with the `local_safe_segment` check deleted
            // from `entry_name_is_safe` while a stray `is_safe_name` failure coincidentally covers it.
            assert!(
                crate::transfer::is_safe_name(name),
                "transfer::is_safe_name({name:?}) should be true — this shape is only unsafe via \
                 local_safe_segment, and if is_safe_name started rejecting it too this test would no \
                 longer isolate which guard is doing the work"
            );
            assert!(
                crate::transfer::local_safe_segment(name).as_ref() != name,
                "local_safe_segment({name:?}) should rewrite to different bytes — that rewrite is exactly \
                 what entry_name_is_safe now refuses instead of writing through"
            );
        }
    }

    /// **Regression: a real bug caught in review before it shipped.** An earlier version of
    /// [`entry_name_is_safe`] matched `crate::transfer::local_safe_segment`'s return on its `Cow`
    /// *variant* (`Cow::Owned` == "would rewrite" == reject) rather than comparing the rewritten bytes.
    /// `crate::transfer::windows_safe_segment`'s cheap pre-scan is deliberately over-broad — it allocates
    /// an `Owned` copy for ANY segment containing a bare `%` (so it can escape a pre-existing `%XX`
    /// sequence its own encoder could have emitted), even when that copy comes out byte-identical to the
    /// input. That "allocated but identical" case is exactly what a `Cow`-variant check cannot tell apart
    /// from a genuine rewrite, and the bug rejected an ordinary Hive/Athena partition-style name
    /// (`"city=A%2FB"`, the literal example CPE-1709 round 2 fixed the mangling of) and a plain `%` in a
    /// filename (`"50% off.txt"`) on Windows only — invisible to every CI leg, since `local_safe_segment`
    /// is the identity function on Linux/macOS and the pre-fix Windows test table had no `%` row.
    /// `#[cfg(windows)]` because the whole hazard is Windows-only by construction.
    #[test]
    #[cfg(windows)]
    fn entry_name_is_safe_does_not_reject_percent_names_that_round_trip_unchanged() {
        for name in
            ["50% off.txt", "report%2ffinal.txt", "city=A%2FB", "100%", "a%b", "ok/50% off.txt"]
        {
            assert_eq!(
                crate::transfer::local_safe_segment(name.rsplit('/').next().unwrap()).as_ref(),
                name.rsplit('/').next().unwrap(),
                "local_safe_segment({name:?}) allocated an Owned copy but it must be byte-identical to \
                 the input for this row to be testing the bug this test guards against"
            );
            assert!(
                entry_name_is_safe(name),
                "entry_name_is_safe({name:?}) should be true — local_safe_segment allocates a Cow::Owned \
                 for the bare '%' here but the bytes round-trip unchanged, so this is NOT one of the \
                 CPE-1758 unsafe shapes; a Cow-variant check would wrongly reject it"
            );
        }
    }

    /// **End-to-end: the actual CPE-1758 bug shape, through the real streamed-extraction entry point,
    /// asserting the filesystem BEFORE the `Result` is unwrapped** — exactly what the ticket's checklist
    /// demanded ("this whole family fails by returning `Ok`") and what the two predicate-only tests above
    /// do not cover on their own.
    ///
    /// Builds a ZIP with `"file:stream"` (the exact CPE-1709/M7 measured ADS shape) alongside a plain
    /// `"ok.txt"`, pre-creates a neighbour file named `"file"` in the destination (the base component an
    /// ADS write would attach to), and runs the real `extract_archive_streamed` — the function
    /// `start_archive_extract` calls for the shipping Extract button on a `.zip`. Before touching the
    /// `Result` at all: asserts the neighbour's bytes are untouched (no stream landed on it) and that no
    /// entry literally named `"file:stream"` exists. Only then unwraps and asserts `report.errors`
    /// actually names the skip — the assertion Finding 2 of the review showed was missing: a route that
    /// looks silent from the caller's `Result` alone is not the same as a route with no user-visible
    /// trace at all.
    #[test]
    #[cfg(windows)]
    fn ads_shaped_entry_is_skipped_end_to_end_and_recorded_not_silently_dropped() {
        let d = scratch("cpe1758_ads_e2e");

        let zip_path = d.join("evil.zip");
        {
            let file = fs::File::create(&zip_path).unwrap();
            let mut w = zip::ZipWriter::new(file);
            let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default();
            w.start_file("file:stream", opts).unwrap();
            w.write_all(b"ADS PAYLOAD").unwrap();
            w.start_file("ok.txt", opts).unwrap();
            w.write_all(b"ORDINARY FILE").unwrap();
            w.finish().unwrap();
        }

        let dest = d.join("out");
        fs::create_dir_all(&dest).unwrap();
        let neighbor = dest.join("file");
        fs::write(&neighbor, b"NEIGHBOR").unwrap();

        let cancel = AtomicBool::new(false);
        let outcome = extract_archive_streamed(&zip_path.to_string_lossy(), &dest.to_string_lossy(), &cancel, |_| {});

        // Filesystem effects, asserted BEFORE the Result is unwrapped.
        assert_eq!(
            fs::read(&neighbor).unwrap(),
            b"NEIGHBOR".to_vec(),
            "the neighbour file's bytes must be untouched — an alternate-data-stream write would leave \
             the base file's own length/content exactly as it was while a hidden stream held the payload, \
             so an unchanged read is necessary but the next assertion (no separate leaf) is what actually \
             distinguishes 'skipped entirely' from 'ADS attached'"
        );
        assert!(
            !dest.join("file:stream").exists(),
            "no entry literally named file:stream should exist at the destination"
        );

        let report = outcome.expect("one unsafe-named entry must not abort the rest of the extraction");
        assert!(
            report.errors.iter().any(|e| e == "file:stream: unsafe entry name, skipped"),
            "the skip must be RECORDED — this is the real surfacing route (ArchiveReport::errors, \
             rendered in the operations panel), not the unwired extract_plan::plan_extract path; got {:?}",
            report.errors
        );
        assert_eq!(
            fs::read(dest.join("ok.txt")).unwrap(),
            b"ORDINARY FILE".to_vec(),
            "the rest of the archive must still extract"
        );
    }

    /// **CPE-1961 round 3 (Reviewer F3): one planted alternate data stream must cost ONE entry, not the
    /// whole extraction.**
    ///
    /// Round 2's Security Auditor found that `claim_destination_handle`'s two carry-over refusals — "could
    /// not read what is at the destination" and `HandleCarryover::capture` failing — were
    /// `Refusal::failure`, i.e. `policy: false`, which the loop above matches as *a file the user asked
    /// for and did not get* and turns into `return Err`. Writing an alternate data stream needs only
    /// write access to the file, and `HandleCarryover::capture` refuses outright once the streams exceed
    /// `CARRY_CAP` (8 MiB). So **one 9 MiB stream planted on one pre-existing name inside the destination
    /// killed the entire extraction** — an attacker-triggerable denial of service that arrived *with* the
    /// carry-over and had to leave with it. Round 2 changed both to `policy: true`.
    ///
    /// **That fix shipped with nothing pinning it**, which is the same gap, one function over, that this
    /// ticket closed for the Unix-mode leg on exactly this argument.
    /// `ads_shaped_entry_is_skipped_end_to_end_and_recorded_not_silently_dropped` covers an ADS-shaped
    /// *entry name*, a different hazard on the other side of the write, and a search for `CARRY_CAP`
    /// found one hit — the constant. A future refactor could put `Refusal::failure` back and every gate
    /// in the tree would stay green.
    ///
    /// Asserts on the **filesystem**, in the order this family has learned to assert: the victim's own
    /// unnamed stream is untouched, both neighbours landed, no `.cpe-tmp` residue survives, and only then
    /// the counts and the recorded reason. `failed: 0` matters as much as `skipped: 1` — a refusal
    /// reclassified into the failure bucket is still a wrong answer even when it does not abort.
    ///
    /// **Red-proofed. Round 3's transcript here was PRE-REBASE evidence presented as re-taken, and
    /// round 4 re-ran it** (Reviewer Blocker 2). It said *"The whole extraction comes back `Err` and
    /// `after.txt` is never created"*, quoting the `.expect()` sentence below — and CPE-1935, merged
    /// into this branch's base by the round-3 rebase, had already deleted the `return Err` that
    /// sentence describes. Flipping the `HandleCarryover::capture` refusal in
    /// `claim_destination_handle` back to `policy: false`, re-run on the round-4 head (`Compiling
    /// cpe-server` seen):
    ///
    /// ```text
    /// cpe_1961_one_planted_alternate_data_stream_skips_its_entry_and_extracts_the_rest ... FAILED
    ///   two entries written, one refused as a policy skip, and NOTHING in the failed bucket … :
    ///   ArchiveReport { done: 2, failed: 1, skipped: 0, cancelled: false, errors:
    ///     ["victim.txt: …\out\victim.txt: its alternate data streams are larger than 8388608 bytes,
    ///       which this app will not copy across onto the replacement — nothing was written, and the
    ///       original is untouched. Nothing was written for this entry. The rest of the archive was
    ///       extracted; clear that and extract again to get this entry too."] }
    ///     left: (2, 1, 0)   right: (2, 0, 1)
    /// ```
    ///
    /// So: **`Ok`, not `Err`, and `after.txt` IS created.** The test still reds, on the classification
    /// assert, which is the assert that should carry it — `policy` no longer decides whether the run
    /// survives on this leg, only which bucket the entry lands in and therefore which sentence the user
    /// reads. The `.expect()` below is now a backstop for a regression nothing currently produces,
    /// which is why the counts assertion above it is doing the work.
    ///
    /// Windows-only because alternate data streams are.
    #[test]
    #[cfg(windows)]
    fn cpe_1961_one_planted_alternate_data_stream_skips_its_entry_and_extracts_the_rest() {
        let d = scratch("cpe1961-ads-carry-dos");
        let dest = d.join("out");
        fs::create_dir_all(&dest).unwrap();

        // A pre-existing file inside the destination, which the archive is about to overwrite — so the
        // claim runs with `created == false` and `HandleCarryover::capture` actually reads its streams.
        let victim = dest.join("victim.txt");
        fs::write(&victim, b"ORIGINAL").unwrap();
        // 9 MiB > CARRY_CAP (8 MiB). Writing this needs nothing but write access to `victim`.
        let ads = format!("{}:planted", victim.to_string_lossy());
        if fs::write(&ads, vec![0u8; 9 * 1024 * 1024]).is_err() {
            crate::skip_notice!(
                "SKIPPING cpe_1961_one_planted_alternate_data_stream_skips_its_entry_and_extracts_the_rest: \
                 this volume does not support alternate data streams (not NTFS). NOTHING on this run \
                 covered the carry-over denial-of-service classification"
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }

        // Three entries, with the poisoned one in the MIDDLE: `after.txt` is what tells "skipped one
        // entry" apart from "abandoned the run at the first refusal".
        let zip_path = d.join("in.zip");
        {
            let file = fs::File::create(&zip_path).unwrap();
            let mut w = zip::ZipWriter::new(file);
            let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default();
            w.start_file("before.txt", opts).unwrap();
            w.write_all(b"BEFORE").unwrap();
            w.start_file("victim.txt", opts).unwrap();
            w.write_all(b"REPLACEMENT").unwrap();
            w.start_file("after.txt", opts).unwrap();
            w.write_all(b"AFTER").unwrap();
            w.finish().unwrap();
        }

        let cancel = AtomicBool::new(false);
        let outcome =
            extract_archive_streamed(&zip_path.to_string_lossy(), &dest.to_string_lossy(), &cancel, |_| {});

        // Filesystem first, before the Result is unwrapped.
        assert_eq!(
            fs::read(&victim).unwrap(),
            b"ORIGINAL".to_vec(),
            "the refused entry must leave the destination exactly as it found it: {outcome:?}"
        );
        let residue: Vec<_> = fs::read_dir(&dest)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".cpe-tmp"))
            .collect();
        assert!(
            residue.is_empty(),
            "a refused claim must take its staging sibling with it — found {residue:?}: {outcome:?}"
        );

        let report = outcome.expect(
            "one planted alternate data stream must cost ONE entry. An Err here means SOME per-entry \
             path in the loop has regained run-abort semantics — round 2's `policy: false` did that \
             through the carry-over refusal, and CPE-1935 removed it; round 4 removed a second one on \
             the commit. The counts assertion below is what this test actually turns on now",
        );
        assert_eq!(
            (report.done, report.failed, report.skipped),
            (2, 0, 1),
            "two entries written, one refused as a policy skip, and NOTHING in the failed bucket — a \
             carry-over refusal reclassified as a failure is still the wrong answer: {report:?}"
        );
        assert!(
            report.errors.iter().any(|e| e.starts_with("victim.txt:")
                && e.contains("alternate data streams are larger than 8388608 bytes")),
            "and the skip must be RECORDED against the entry, with the reason the user can act on: \
             {:?}",
            report.errors
        );
        assert_eq!(
            fs::read(dest.join("before.txt")).ok().as_deref(),
            Some(&b"BEFORE"[..]),
            "the entry before the poisoned one must be written: {report:?}"
        );
        assert_eq!(
            fs::read(dest.join("after.txt")).ok().as_deref(),
            Some(&b"AFTER"[..]),
            "THE POINT: the entry AFTER the poisoned one must be written too. Missing means the run was \
             abandoned at the refusal rather than skipping past it: {report:?}"
        );
        let _ = fs::remove_dir_all(&d);
    }

    /// **CPE-1961 round 4 (Reviewer Blocker 1): a destination the commit cannot rename over costs ONE
    /// entry, not the whole extraction.**
    ///
    /// CPE-1961 adds a per-entry failure point this loop did not have — `sync_all` plus a rename that
    /// the filesystem can refuse — and round 3 gave it `claimed.commit().map_err(|r| r.why)?`, the only
    /// bare `?` left in the per-entry body. That is a **run abort**, inside the loop whose entire
    /// ticket (CPE-1935, merged one commit before this branch's base) exists to remove run aborts from
    /// it. Measured on the round-3 head against the base, same three-entry zip, `victim.txt` held open
    /// by another handle:
    ///
    /// ```text
    ///                          outcome                        before.txt  victim.txt   after.txt
    /// base 104b0bc5 (main)     Ok(done: 3)                    BEFORE      REPLACEMENT  AFTER
    /// head 9902e1f5 (round 3)  Err("…could not be replaced…")  BEFORE      ORIGINAL     ABSENT
    /// ```
    ///
    /// Refusing the *entry* is correct and stays: the user's `victim.txt` is intact and the reason is
    /// named. Aborting the archive is not, and it is a regression against `main`.
    ///
    /// # The fixture needs no race and no privilege
    ///
    /// A handle opened with `FILE_SHARE_READ | FILE_SHARE_WRITE` and **not** `FILE_SHARE_DELETE` — what
    /// a program not using Rust's `std` opens a file with by default, `std`'s own share mode being
    /// `READ|WRITE|DELETE`. `create_beneath`'s leaf open asks `FILE_GENERIC_WRITE` and shares all
    /// three, so the *claim* succeeds; the commit's
    /// `NtSetInformationFile(FileRenameInformation, ReplaceIfExists)` is what the holder's missing
    /// `FILE_SHARE_DELETE` refuses. Deterministic, unprivileged, and the shape a user actually hits —
    /// an editor or a viewer with the extracted file already open.
    ///
    /// **Windows-only, and the Linux leg is genuinely not constructible here rather than merely
    /// omitted.** `rename(2)` over an open file always succeeds on Linux, and the other half of the
    /// commit — `sync_all` — needs `ENOSPC` or an I/O error to fail, neither of which an unprivileged
    /// test can produce on demand. The `?` this pins was reachable on both platforms (`sync_all`
    /// returning `ENOSPC` under ext4's delayed allocation is cost row 1's own prediction); only the
    /// *fixture* is Windows-only. Nothing on the Linux matrix leg covers it.
    ///
    /// **Red-proofed** — see the transcript on the fix site in `extract_zip_archive_stream`.
    #[test]
    #[cfg(windows)]
    fn cpe_1961_a_destination_the_commit_cannot_replace_costs_one_entry_not_the_run() {
        use std::os::windows::fs::OpenOptionsExt;
        // Named here rather than pulled from `windows-sys`: this test is about the share mode a
        // *foreign* program picks, so the literal is the specification.
        const FILE_SHARE_READ: u32 = 0x0000_0001;
        const FILE_SHARE_WRITE: u32 = 0x0000_0002;

        let d = scratch("cpe1961-commit-refused");
        let dest = d.join("out");
        fs::create_dir_all(&dest).unwrap();
        let victim = dest.join("victim.txt");
        fs::write(&victim, b"ORIGINAL").unwrap();
        let hold = fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .open(&victim)
            .expect("holding the victim open for reading must succeed");

        // The poisoned name in the MIDDLE again: `after.txt` is the whole difference between "skipped
        // one entry" and "abandoned the run".
        let zip_path = d.join("in.zip");
        {
            let file = fs::File::create(&zip_path).unwrap();
            let mut w = zip::ZipWriter::new(file);
            let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default();
            w.start_file("before.txt", opts).unwrap();
            w.write_all(b"BEFORE").unwrap();
            w.start_file("victim.txt", opts).unwrap();
            w.write_all(b"REPLACEMENT").unwrap();
            w.start_file("after.txt", opts).unwrap();
            w.write_all(b"AFTER").unwrap();
            w.finish().unwrap();
        }

        let cancel = AtomicBool::new(false);
        let outcome =
            extract_archive_streamed(&zip_path.to_string_lossy(), &dest.to_string_lossy(), &cancel, |_| {});

        // Filesystem first, before the Result is unwrapped — every bug in this family returned a
        // Result that was less informative than the disk.
        assert_eq!(
            fs::read(&victim).unwrap(),
            b"ORIGINAL".to_vec(),
            "a refused commit must leave the destination exactly as it found it: {outcome:?}"
        );
        assert_eq!(
            fs::read(dest.join("before.txt")).ok().as_deref(),
            Some(&b"BEFORE"[..]),
            "the entry before the blocked one must be written: {outcome:?}"
        );
        assert_eq!(
            fs::read(dest.join("after.txt")).ok().as_deref(),
            Some(&b"AFTER"[..]),
            "THE POINT: the entry AFTER the blocked one must be written too. Missing means the commit \
             failure took the run down — CPE-1935's regression, reintroduced by CPE-1961's new failure \
             point: {outcome:?}"
        );
        let residue: Vec<_> = fs::read_dir(&dest)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".cpe-tmp"))
            .collect();
        assert!(
            residue.is_empty(),
            "a failed commit must take its staging sibling with it — found {residue:?}: {outcome:?}"
        );

        let report = outcome.expect(
            "one destination another process holds open must cost ONE entry. An Err here is a run \
             abort: the entries after the blocked one are never written, and the one error names \
             none of them",
        );
        assert_eq!(
            (report.done, report.failed, report.skipped),
            (2, 1, 0),
            "two entries written and the blocked one in the FAILED bucket — the user asked for a file \
             and did not get it, which is a failure and not a policy skip: {report:?}"
        );
        assert!(
            report.errors.iter().any(|e| e.starts_with("victim.txt:")
                && e.contains("could not be replaced by the staged copy")),
            "and the failure must be RECORDED against the entry, with the reason the user can act on \
             (close the program holding the file): {:?}",
            report.errors
        );
        drop(hold);
        let _ = fs::remove_dir_all(&d);
    }

    /// **CPE-1961 round 4 (Reviewer MAJOR 3): a long-but-legal entry name still extracts, and a refused
    /// staging create leaves nothing at the destination.**
    ///
    /// `staging_sibling_name` appends `.<pid>-<nanos>.cpe-tmp` — about 31 bytes — and round 3 appended
    /// it with **no length cap**, so a destination whose own name was legal but long stopped being
    /// writable at all. 244 characters here, comfortably under the 255 both `NAME_MAX` (ext4, APFS) and
    /// NTFS enforce; on `main` the same entry extracts normally.
    ///
    /// Two distinct regressions in one fixture, which is why both are asserted:
    ///
    /// 1. **The entry fails.** `"…nnn….txt.28088-…cpe-tmp" could not be created as a staging file (The
    ///    filename, directory name, or volume label syntax is incorrect. (os error 123))`. Closed by
    ///    capping the base name — see `fsutil::staging_sibling_name` for why truncation is the
    ///    conservative direction for the sweep that parses these names back apart.
    /// 2. **The refusal leaks a zero-byte stub.** `create_beneath` had already *created* the
    ///    destination, and the staging create's `?` returned before `ClaimedDestination` existed to own
    ///    the undo — so an empty file survived at a name that did not exist before the run, under a
    ///    message saying nothing was written. Closed by `fsutil::undo_created_destination`.
    ///
    /// **Not platform-gated.** `NAME_MAX` is 255 on ext4 too, so both halves reproduced on both.
    ///
    /// **Red-proof, both halves, run rather than asserted** (Windows `--lib`, `Compiling cpe-server`
    /// seen on each). Removing the cap from `staging_sibling_name` while keeping the undo:
    ///
    /// ```text
    /// cpe_1961_a_long_but_legal_entry_name_still_extracts ... FAILED
    ///   a legal entry name under NAME_MAX must extract, with its bytes … :
    ///   Ok(ArchiveReport { done: 2, failed: 1, skipped: 0, cancelled: false, errors:
    ///     ["nnn….txt: … the path component \"nnn….txt.35788-1787920395378223800.cpe-tmp\" could not
    ///       be created as a staging file (The filename, directory name, or volume label syntax is
    ///       incorrect. (os error 123)). Nothing was written for this entry …"] })
    ///     left: None            right: Some((true, 4))
    /// ```
    ///
    /// and removing **both** the cap and the `undo_created_destination` call on that arm — same
    /// refusal, same counts, one line different:
    ///
    /// ```text
    ///     left: Some((true, 0))  right: Some((true, 4))
    /// ```
    ///
    /// That `left` is the second half's whole evidence: the difference between the two runs is a
    /// zero-byte file at a name the user did not have before, under a message ending *"Nothing was
    /// written for this entry."* **With the cap in place that arm is
    /// no longer reachable from any input a test can construct** — what is left for it is a quota, a
    /// full disk, or a share that drops between the destination create and the staging create in the
    /// same directory. So the undo is a live backstop with no standing test, said here rather than
    /// implied by a green suite.
    #[test]
    fn cpe_1961_a_long_but_legal_entry_name_still_extracts() {
        let d = scratch("cpe1961-long-name");
        let dest = d.join("out");
        fs::create_dir_all(&dest).unwrap();
        // 244 bytes. The stamped suffix takes it past 255 without the cap, and nowhere near it with.
        let long = format!("{}.txt", "n".repeat(240));
        assert_eq!(long.len(), 244, "the fixture must stay under 255 and over 255-minus-the-stamp");

        let zip_path = d.join("in.zip");
        {
            let file = fs::File::create(&zip_path).unwrap();
            let mut w = zip::ZipWriter::new(file);
            let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default();
            w.start_file("before.txt", opts).unwrap();
            w.write_all(b"BEFORE").unwrap();
            w.start_file(&long, opts).unwrap();
            w.write_all(b"LONG").unwrap();
            w.start_file("after.txt", opts).unwrap();
            w.write_all(b"AFTER").unwrap();
            w.finish().unwrap();
        }

        let cancel = AtomicBool::new(false);
        let outcome =
            extract_archive_streamed(&zip_path.to_string_lossy(), &dest.to_string_lossy(), &cancel, |_| {});

        // Filesystem first. The zero-byte stub is the half that a counts-only assertion misses
        // entirely: the report says "failed", the disk says a new empty file.
        let landed = fs::metadata(dest.join(&long)).ok().map(|m| (m.is_file(), m.len()));
        assert_eq!(
            landed,
            Some((true, 4)),
            "a legal entry name under NAME_MAX must extract, with its bytes — `Some((true, 0))` is the \
             zero-byte stub a refused staging create used to leave behind, and `None` is the entry \
             simply failing: {outcome:?}"
        );
        assert_eq!(fs::read(dest.join(&long)).unwrap(), b"LONG".to_vec());
        let residue: Vec<_> = fs::read_dir(&dest)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".cpe-tmp"))
            .collect();
        assert!(residue.is_empty(), "no staging residue may survive a committed run: {residue:?}");

        let report = outcome.expect("a long-but-legal entry name must not fail the extraction");
        assert_eq!(
            (report.done, report.failed, report.skipped),
            (3, 0, 0),
            "all three entries land — the long name is ordinary user data, not a hostile input: \
             {report:?}"
        );
        let _ = fs::remove_dir_all(&d);
    }

    /// Gzip `raw`, so the row-13/14 legs can build a single-file gzip whose extraction leaf lands on the
    /// staged link.
    fn gzip_bytes(raw: &[u8]) -> Vec<u8> {
        let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        e.write_all(raw).unwrap();
        e.finish().unwrap()
    }

    /// A file for a compress row to pack, so the run actually reaches its `File::create(dest)`.
    fn row_src(d: &Path) -> String {
        let src = d.join("payload.txt");
        fs::write(&src, b"payload").unwrap();
        src.to_string_lossy().to_string()
    }

    /// **The whole assertion, in one place, so no row can be checked more weakly than its neighbours.**
    ///
    /// Three things matter here and each was got wrong somewhere in this ticket family first:
    ///
    /// 1. **Assert on the filesystem before unwrapping the `Result`.** Every bug in this family returned
    ///    `Ok` while destroying something, so the `Result` is the *least* informative witness — and if a
    ///    guard regresses to returning `Ok`, an `expect_err` placed first panics before the assertion that
    ///    names the damage ever runs.
    /// 2. **Pin OUR refusal, not merely a failure.** On an unprivileged Windows runner `make_dangling_link`
    ///    falls back to a dangling **junction**, and `File::create` on one of those fails by itself —
    ///    measured for this ticket: `Err("Access is denied. (os error 5)", kind PermissionDenied)`. An
    ///    `is_err()`-only leg would therefore stay green straight through a deleted guard. The substring
    ///    check is what makes this leg about the guard instead of about the OS.
    /// 3. **The link must survive.** A guard that deleted the link and then refused would also pass 1 and 2.
    #[track_caller]
    fn assert_row_refuses_a_dangling_link(n: u8, link_name: &str, run: fn(&Path) -> Result<(), String>) {
        let d = scratch(&format!("cpe1733_row{n}_dangling"));
        let link = d.join(link_name);
        if !crate::fsutil::make_dangling_link(&link) {
            crate::skip_notice!(
                "[CPE-1733] SKIPPED row {n}'s dangling-link leg: this machine could not stage a link at \
                 {}. Nothing about row {n}'s guard was covered on this run.",
                link.display()
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }
        let target = crate::fsutil::dangling_link_target(&link);
        assert!(!target.exists(), "row {n}: the staged link must be DANGLING or this leg proves nothing");

        let outcome = run(&d);

        assert!(
            !target.exists(),
            "row {n}: the link's target was created — the bytes went THROUGH the link into a path nobody \
             named (outcome was {outcome:?})"
        );
        assert!(
            fs::symlink_metadata(&link).map(|m| m.file_type().is_symlink()).unwrap_or(false),
            "row {n}: the link at {} must survive untouched (outcome was {outcome:?})",
            link.display()
        );
        let err = outcome.expect_err("a link at the destination must be refused, not written through");
        assert!(
            err.contains("is a link"),
            "row {n}: the refusal must be OURS. `is_err()` alone proves nothing here — a dangling junction \
             (the unprivileged-Windows staging fallback) fails `File::create` by itself with \"Access is \
             denied. (os error 5)\", so this leg would pass through a guard that had been deleted. Got: {err}"
        );
        let _ = fs::remove_dir_all(&d);
    }

    /// Declares every whole-operation-refusing row **once**: it generates one `#[test]` per row (so
    /// neutralising a single site turns a single, named test red) and the `GUARDED_ROWS` array the
    /// live-link leg below walks (so the two can never drift apart).
    macro_rules! guarded_rows {
        ($(($n:expr, $test:ident, $link:expr, $run:expr)),* $(,)?) => {
            const GUARDED_ROWS: &[(u8, &str, fn(&Path) -> Result<(), String>)] = &[$(($n, $link, $run)),*];
            $(
                #[test]
                fn $test() {
                    assert_row_refuses_a_dangling_link($n, $link, $run);
                }
            )*
        };
    }

    guarded_rows![
        (6, row6_compress_to_zip_refuses_a_link_at_its_destination, "out.zip", |d: &Path| {
            compress_to_zip(&[row_src(d)], &d.join("out.zip").to_string_lossy()).map(|_| ())
        }),
        (7, row7_create_empty_zip_refuses_a_link_at_its_destination, "empty.zip", |d: &Path| {
            create_empty_zip(&d.join("empty.zip").to_string_lossy()).map(|_| ())
        }),
        (8, row8_compress_to_targz_refuses_a_link_at_its_destination, "out.tar.gz", |d: &Path| {
            compress_to_targz(&[row_src(d)], &d.join("out.tar.gz").to_string_lossy()).map(|_| ())
        }),
        (9, row9_compress_to_zip_encrypted_refuses_a_link_at_its_destination, "out.zip", |d: &Path| {
            compress_to_zip_encrypted(&[row_src(d)], &d.join("out.zip").to_string_lossy(), "hunter2").map(|_| ())
        }),
        (10, row10_compress_to_zip_streamed_refuses_a_link_at_its_destination, "out.zip", |d: &Path| {
            let cancel = AtomicBool::new(false);
            compress_to_zip_streamed(&[row_src(d)], &d.join("out.zip").to_string_lossy(), &cancel, |_| {}).map(|_| ())
        }),
        (11, row11_compress_to_targz_streamed_refuses_a_link_at_its_destination, "out.tar.gz", |d: &Path| {
            let cancel = AtomicBool::new(false);
            compress_to_targz_streamed(&[row_src(d)], &d.join("out.tar.gz").to_string_lossy(), &cancel, |_| {})
                .map(|_| ())
        }),
        (12, row12_compress_to_zip_encrypted_streamed_refuses_a_link_at_its_destination, "out.zip", |d: &Path| {
            let cancel = AtomicBool::new(false);
            compress_to_zip_encrypted_streamed(
                &[row_src(d)],
                &d.join("out.zip").to_string_lossy(),
                "hunter2",
                &cancel,
                |_| {},
            )
            .map(|_| ())
        }),
        // Rows 13/14: the destination FOLDER is the user's and the leaf is the archive's own name minus
        // `.gz`, so the staged link is named `a` and the archive is `a.gz`.
        (13, row13_extract_archive_gz_refuses_a_link_at_its_leaf, "a", |d: &Path| {
            fs::write(d.join("a.gz"), gzip_bytes(b"decompressed bytes")).unwrap();
            extract_archive(&d.join("a.gz").to_string_lossy(), &d.to_string_lossy()).map(|_| ())
        }),
        (14, row14_extract_archive_streamed_gz_refuses_a_link_at_its_leaf, "a", |d: &Path| {
            fs::write(d.join("a.gz"), gzip_bytes(b"decompressed bytes")).unwrap();
            let cancel = AtomicBool::new(false);
            extract_archive_streamed(&d.join("a.gz").to_string_lossy(), &d.to_string_lossy(), &cancel, |_| {})
                .map(|_| ())
        }),
    ];

    /// **The live-link half, as one table over the same rows** (CPE-1718's live-link leg, generalised).
    ///
    /// The dangling legs above pass under a guard that only handled *live* links, and vice versa — they
    /// are different measured behaviours (`try_exists` answers `Ok(false)` for a dangling link and
    /// `Ok(true)` for a live one), and only the live case can destroy bytes that already exist. Measured
    /// for this ticket with no guard: `File::create` on a live link returns `Ok` and the link's target
    /// reads `"CLOBBERED"`.
    ///
    /// A live **file** symlink is the one thing this repo cannot fake (a junction is directory-only, a
    /// hard link is `is_symlink() == false` — CPE-1716), so this is one leg for all nine rows rather than
    /// nine skippable ones, and it routes through `require_staged` so a runner that *should* stage goes
    /// red instead of silently covering nothing (CPE-1717).
    ///
    /// **Platform boundary** (PR #906 review — every other figure in this ticket states one and this doc
    /// did not): the `"CLOBBERED"` figure above is Windows 11. The leg itself runs on all three CI OSes.
    ///
    /// **Scope:** this walks `GUARDED_ROWS`, which is rows 6–14 — the rows that refuse the whole
    /// operation. Rows 15–16 skip a single entry instead, so they cannot share this table's
    /// `expect_err`; their live-link leg is
    /// `rows_15_and_16_refuse_a_live_link_and_still_extract_the_rest` below, added in the round after the
    /// PR #906 review pointed out that saying they had none was not the same as giving them one.
    #[test]
    fn every_guarded_row_refuses_a_live_link_without_touching_its_target() {
        for (n, link_name, run) in GUARDED_ROWS {
            let d = scratch(&format!("cpe1733_row{n}_live"));
            let victim = d.join("victim-the-user-never-named.bin");
            fs::write(&victim, b"VICTIM ORIGINAL").unwrap();
            let link = d.join(link_name);
            let staged = {
                #[cfg(windows)]
                {
                    std::os::windows::fs::symlink_file(&victim, &link).is_ok()
                }
                #[cfg(unix)]
                {
                    std::os::unix::fs::symlink(&victim, &link).is_ok()
                }
            };
            if !crate::fsutil::require_staged("live_file_symlink", true, staged) {
                crate::skip_notice!(
                    "[CPE-1733] SKIPPED the live-link table: this machine could not create a file symlink \
                     at {}. The dangling legs pass under a live-link-blind guard, so NOTHING covered the \
                     live case on this run.",
                    link.display()
                );
                let _ = fs::remove_dir_all(&d);
                // CPE-1809: `continue`, not `return` — a staging hiccup on ONE row of `GUARDED_ROWS` must
                // not abandon testing the other eight; a `return` here made a bad run for row 6 silently
                // skip rows 7–14 too, with nothing saying so.
                continue;
            }

            let outcome = run(&d);

            assert_eq!(
                fs::read(&victim).unwrap(),
                b"VICTIM ORIGINAL".to_vec(),
                "row {n}: the link's target was rewritten — the write followed the link into a file the \
                 caller never named (outcome was {outcome:?})"
            );
            let err = outcome.expect_err("a live link at the destination must be refused");
            assert!(
                err.contains("is a link"),
                "row {n}: a LIVE link must be reported AS a link. Occupancy-first ordering would say \
                 \"already exists\" and send the user to delete a name that actually holds a link \
                 elsewhere — the failure `fsutil`'s CPE-1718 leg exists for. Got: {err}"
            );
            assert!(
                !err.contains("already exists"),
                "row {n}: and it must not fall through to an occupancy message: {err}"
            );
            let _ = fs::remove_dir_all(&d);
        }
    }

    /// Row 15: `extract_zip_encrypted` **skips** an entry whose name lands on a link and keeps extracting.
    ///
    /// The refusal shape differs from rows 6–14 on purpose — the name is the archive's, not the caller's,
    /// so one poisoned entry must not abort a legitimate extraction. The `Result` alone used to be
    /// useless as a witness (it was `Ok` either way when the guard worked), which is why the filesystem
    /// assertions below matter regardless — but **CPE-1837** gave the `Ok` value somewhere to put the
    /// refusal too, so this test now also pins that `report.errors` names the entry, exactly like row 16
    /// already does.
    #[test]
    fn row15_extract_zip_encrypted_skips_an_entry_that_lands_on_a_link() {
        let d = scratch("cpe1733_row15");
        let src_dir = d.join("src");
        fs::create_dir_all(&src_dir).unwrap();
        fs::write(src_dir.join("a.txt"), b"ARCHIVED A").unwrap();
        fs::write(src_dir.join("b.txt"), b"ARCHIVED B").unwrap();
        let zip = d.join("enc.zip");
        compress_to_zip_encrypted(
            &[
                src_dir.join("a.txt").to_string_lossy().to_string(),
                src_dir.join("b.txt").to_string_lossy().to_string(),
            ],
            &zip.to_string_lossy(),
            "hunter2",
        )
        .unwrap();

        let dest = d.join("out");
        fs::create_dir_all(&dest).unwrap();
        let link = dest.join("a.txt");
        if !crate::fsutil::make_dangling_link(&link) {
            crate::skip_notice!("[CPE-1733] SKIPPED row 15: could not stage a link at {}.", link.display());
            let _ = fs::remove_dir_all(&d);
            return;
        }
        let target = crate::fsutil::dangling_link_target(&link);

        let outcome = extract_zip_encrypted(&zip.to_string_lossy(), &dest.to_string_lossy(), "hunter2");

        assert!(
            !target.exists(),
            "row 15: the entry's bytes went THROUGH the link into {} (outcome was {outcome:?})",
            target.display()
        );
        assert!(
            fs::symlink_metadata(&link).map(|m| m.file_type().is_symlink()).unwrap_or(false),
            "row 15: the link must survive (outcome was {outcome:?})"
        );
        let outcome = outcome.expect("row 15: one skipped entry must not abort the extraction");
        assert!(
            outcome.report.errors.iter().any(|e| e.contains("a.txt") && e.contains("is a link")),
            "row 15 (CPE-1837): the skip must be RECORDED on the one-shot path too, not merely survived —
             got {:?}",
            outcome.report.errors
        );
        assert_eq!(
            fs::read(dest.join("b.txt")).unwrap(),
            b"ARCHIVED B".to_vec(),
            "row 15: the entries that did not hit a link must still extract"
        );
        let _ = fs::remove_dir_all(&d);
    }

    /// Row 16: the streamed twin of row 15 — same skip, and it has somewhere to record the reason, so this
    /// leg additionally pins the recorded message. Without that substring the leg would pass on a runner
    /// where the OS refused the write for its own reasons.
    #[test]
    fn row16_extract_zip_archive_stream_skips_and_records_an_entry_that_lands_on_a_link() {
        let d = scratch("cpe1733_row16");
        let src_dir = d.join("src");
        fs::create_dir_all(&src_dir).unwrap();
        fs::write(src_dir.join("a.txt"), b"ARCHIVED A").unwrap();
        fs::write(src_dir.join("b.txt"), b"ARCHIVED B").unwrap();
        let zip = d.join("plain.zip");
        compress_to_zip(
            &[
                src_dir.join("a.txt").to_string_lossy().to_string(),
                src_dir.join("b.txt").to_string_lossy().to_string(),
            ],
            &zip.to_string_lossy(),
        )
        .unwrap();

        let dest = d.join("out");
        fs::create_dir_all(&dest).unwrap();
        let link = dest.join("a.txt");
        if !crate::fsutil::make_dangling_link(&link) {
            crate::skip_notice!("[CPE-1733] SKIPPED row 16: could not stage a link at {}.", link.display());
            let _ = fs::remove_dir_all(&d);
            return;
        }
        let target = crate::fsutil::dangling_link_target(&link);

        let cancel = AtomicBool::new(false);
        let outcome = extract_archive_streamed(&zip.to_string_lossy(), &dest.to_string_lossy(), &cancel, |_| {});

        assert!(
            !target.exists(),
            "row 16: the entry's bytes went THROUGH the link into {} (outcome was {outcome:?})",
            target.display()
        );
        assert!(
            fs::symlink_metadata(&link).map(|m| m.file_type().is_symlink()).unwrap_or(false),
            "row 16: the link must survive (outcome was {outcome:?})"
        );
        let report = outcome.expect("row 16: one skipped entry must not abort the extraction");
        assert!(
            report.errors.iter().any(|e| e.contains("a.txt") && e.contains("is a link")),
            "row 16: the skip must be RECORDED, and recorded as OUR link refusal rather than as whatever \
             the OS happened to say — got {:?}",
            report.errors
        );
        assert_eq!(
            fs::read(dest.join("b.txt")).unwrap(),
            b"ARCHIVED B".to_vec(),
            "row 16: the entries that did not hit a link must still extract"
        );
        let _ = fs::remove_dir_all(&d);
    }

    // -----------------------------------------------------------------------
    // CPE-1733 round 2 — the legs the PR #906 review and UAT found missing
    // -----------------------------------------------------------------------

    /// Stage a **live** file symlink at `link` pointing at `victim`; `false` if this machine cannot.
    ///
    /// A live *file* symlink is the one thing this repo cannot fake (a junction is directory-only and a
    /// hard link answers `is_symlink() == false`, CPE-1716), so every caller pairs this with
    /// `fsutil::require_staged` — a runner that *should* be able to stage one goes red rather than
    /// silently covering nothing (CPE-1717).
    fn stage_live_link(victim: &Path, link: &Path) -> bool {
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_file(victim, link).is_ok()
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(victim, link).is_ok()
        }
    }

    /// `src/a.txt` + `src/b.txt` under `d`, as the source list the compressors take. `a.txt` is the entry
    /// aimed at the staged link; `b.txt` is the innocent bystander whose fate says whether the run
    /// **skipped one entry** or **abandoned the archive**.
    fn two_source_files(d: &Path) -> Vec<String> {
        let src = d.join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("a.txt"), b"ARCHIVED A").unwrap();
        fs::write(src.join("b.txt"), b"ARCHIVED B").unwrap();
        vec![
            src.join("a.txt").to_string_lossy().to_string(),
            src.join("b.txt").to_string_lossy().to_string(),
        ]
    }

    /// **Rows 15–16 against a LIVE link** (PR #906 review, non-blocking finding: "rows 15/16 have no
    /// live-link leg").
    ///
    /// The dangling legs above cannot show the thing that actually costs the user something. A dangling
    /// link has no target yet, so the worst a missing guard does is *create* a file; a **live** link has
    /// a victim with bytes in it, and `File::create` through one truncates them — measured for this
    /// ticket at `victim bytes = Some("CLOBBERED")`. The previous round answered this by writing down
    /// that the leg walked rows 6–14 only. A stated absence is still an absence: this is the leg.
    ///
    /// Ordering is deliberate and is the lesson this ticket family keeps re-learning: **the victim is
    /// asserted before the `Result` is unwrapped**, because these bugs return `Ok`. Unwrap first and the
    /// assertion that names the damage never runs.
    #[test]
    fn rows_15_and_16_refuse_a_live_link_and_still_extract_the_rest() {
        // (row, function under test, runs it, does it have somewhere to record the skip?)
        type Run = fn(&Path, &Path) -> Result<Vec<String>, String>;
        let rows: &[(u8, &str, Run, bool)] = &[
            (
                15,
                "extract_zip_encrypted",
                |d: &Path, dest: &Path| {
                    let zip = d.join("enc.zip");
                    compress_to_zip_encrypted(&two_source_files(d), &zip.to_string_lossy(), "hunter2")?;
                    extract_zip_encrypted(&zip.to_string_lossy(), &dest.to_string_lossy(), "hunter2")
                        .map(|outcome| outcome.report.errors)
                },
                // CPE-1837: `ArchiveExtractOutcome` now carries the same `ArchiveReport` the streamed
                // path does, so this leg can record the skip too — no longer the exception in the table.
                true,
            ),
            (
                16,
                "extract_zip_archive_stream",
                |d: &Path, dest: &Path| {
                    let zip = d.join("plain.zip");
                    compress_to_zip(&two_source_files(d), &zip.to_string_lossy())?;
                    let cancel = AtomicBool::new(false);
                    extract_archive_streamed(&zip.to_string_lossy(), &dest.to_string_lossy(), &cancel, |_| {})
                        .map(|r| r.errors)
                },
                true,
            ),
        ];

        for (n, label, run, records) in rows {
            let d = scratch(&format!("cpe1733_row{n}_livelink"));
            let victim = d.join("victim-the-user-never-named.bin");
            fs::write(&victim, b"VICTIM ORIGINAL").unwrap();
            let dest = d.join("out");
            fs::create_dir_all(&dest).unwrap();
            let link = dest.join("a.txt");
            if !crate::fsutil::require_staged("live_file_symlink", true, stage_live_link(&victim, &link)) {
                crate::skip_notice!(
                    "[CPE-1733] SKIPPED row {n}'s LIVE-link leg: this machine could not create a file \
                     symlink at {}. The dangling leg passes under a live-link-blind guard, so nothing \
                     covered the case that destroys existing bytes on this run.",
                    link.display()
                );
                let _ = fs::remove_dir_all(&d);
                // CPE-1809: `continue`, not `return` — the two rows stage independently, so a failure on
                // row 15 must not silently abandon row 16 too.
                continue;
            }

            let outcome = run(&d, &dest);

            assert_eq!(
                fs::read(&victim).unwrap(),
                b"VICTIM ORIGINAL".to_vec(),
                "row {n} ({label}): the entry's bytes went THROUGH the link and truncated a file outside \
                 the destination that nobody named (outcome was {outcome:?})"
            );
            assert!(
                fs::symlink_metadata(&link).map(|m| m.file_type().is_symlink()).unwrap_or(false),
                "row {n} ({label}): the link must survive untouched — a guard that deleted it and then \
                 skipped would pass the assertion above (outcome was {outcome:?})"
            );
            let errors = outcome
                .unwrap_or_else(|e| panic!("row {n} ({label}): one poisoned entry must not abort the run: {e}"));
            if *records {
                assert!(
                    errors.iter().any(|e| e.contains("a.txt") && e.contains("is a link")),
                    "row {n} ({label}): the skip must be RECORDED, and recorded as OUR link refusal rather \
                     than as whatever the OS happened to say — got {errors:?}"
                );
            }
            assert_eq!(
                fs::read(dest.join("b.txt")).unwrap(),
                b"ARCHIVED B".to_vec(),
                "row {n} ({label}): a skip must cost ONE entry — the rest of the archive still extracts"
            );
            let _ = fs::remove_dir_all(&d);
        }
    }

    /// **Rows 21–22: TAR no longer DESTROYS a link at an entry's name** (CPE-1759).
    ///
    /// This is the re-pointed `tar_extraction_destroys_a_link_at_an_entry_name_rather_than_following_it`,
    /// which pinned the hazard precisely so the fix would announce itself — and it did, on the
    /// "the slot must be a regular file" assertion, on both legs.
    ///
    /// **What the old behaviour was, mechanically**, because "destroys" is vague and the two things it
    /// could mean call for different fixes: `tar-0.4.46/src/entry.rs:644-662` opens the destination with
    /// `create_new(true)`, and on `AlreadyExists` — which a symlink at that name produces — calls
    /// `fs::remove_file(dst)` and retries. `remove_file` does not follow a symlink on any supported
    /// platform, so it **unlinks the user's link and writes a regular file in its place**. It never wrote
    /// *through* the link; the crate's own comment ("Ensure we write a new file rather than overwriting
    /// in-place which is attackable") says so. The victim's bytes were always safe. What was lost was the
    /// link — silently, with the call returning `Ok`, which is why nothing in the app could report it.
    ///
    /// Rows 15/16/19/20 answer the same input by refusing the entry and leaving the link alone. Rows
    /// 21–22 now do too, via `entry_sink_action` inside `tar_entry_refusal`.
    ///
    /// **Assertion order is deliberate**: the victim and the link are checked before the `Result` is
    /// unwrapped, because the defect this replaces returned `Ok`.
    #[test]
    fn rows_21_and_22_tar_refuse_a_link_at_an_entry_name_and_still_extract_the_rest() {
        // (label, runs it, does it have somewhere to record the skip?)
        type Run = fn(&Path, &Path) -> Result<Vec<String>, String>;
        let legs: &[(&str, Run, bool)] = &[
            (
                "row 21 one-shot extract_archive",
                |tgz: &Path, dest: &Path| {
                    extract_archive(&tgz.to_string_lossy(), &dest.to_string_lossy())
                        .map(|outcome| outcome.report.errors)
                },
                // CPE-1837: `ArchiveExtractOutcome` now carries the report, so this leg records too.
                true,
            ),
            (
                "row 22 extract_archive_streamed",
                |tgz: &Path, dest: &Path| {
                    let cancel = AtomicBool::new(false);
                    extract_archive_streamed(&tgz.to_string_lossy(), &dest.to_string_lossy(), &cancel, |_| {})
                        .map(|r| {
                            assert_eq!(r.skipped, 1, "the refusal must be COUNTED, not merely logged (CPE-1775); got {r:?}");
                            r.errors
                        })
                },
                true,
            ),
        ];

        for (label, run, records) in legs {
            let d = scratch("cpe1759_tar_link");
            let tgz = d.join("in.tar.gz");
            compress_to_targz(&two_source_files(&d), &tgz.to_string_lossy()).unwrap();
            let victim = d.join("victim-the-user-never-named.bin");
            fs::write(&victim, b"VICTIM ORIGINAL").unwrap();
            let dest = d.join("out");
            fs::create_dir_all(&dest).unwrap();
            let link = dest.join("a.txt");
            if !crate::fsutil::require_staged("live_file_symlink", true, stage_live_link(&victim, &link)) {
                crate::skip_notice!(
                    "[CPE-1759] SKIPPED the tar leg ({label}): could not stage a live link at {}. The \
                     tar leaf-link guard was NOT checked on this run.",
                    link.display()
                );
                let _ = fs::remove_dir_all(&d);
                // CPE-1809: `continue`, not `return` — the two legs stage independently, so a failure on
                // the one-shot leg must not silently abandon the streamed leg too.
                continue;
            }

            let outcome = run(&tgz, &dest);

            // ---- the harm, before the Result ----
            assert_eq!(
                fs::read(&victim).unwrap(),
                b"VICTIM ORIGINAL".to_vec(),
                "tar ({label}): the entry's bytes went THROUGH the link and truncated a file outside the \
                 destination that nobody named (outcome was {outcome:?})"
            );
            assert!(
                fs::symlink_metadata(&link).map(|m| m.file_type().is_symlink()).unwrap_or(false),
                "tar ({label}): the LINK ITSELF must survive. Before CPE-1759 this slot was a regular \
                 file holding the entry's bytes — `tar` unlinked the user's link and replaced it, \
                 silently, returning Ok. A guard that deleted the link and then skipped would pass the \
                 victim assertion above, so this is the one that names the defect (outcome was {outcome:?})"
            );
            assert_eq!(
                fs::read(&link).unwrap(),
                b"VICTIM ORIGINAL".to_vec(),
                "tar ({label}): and it must still point where the user pointed it — a link replaced by a \
                 link would be just as much of a loss"
            );

            let errors = outcome.unwrap_or_else(|e| {
                panic!("tar ({label}): one refused entry must not abort the run: {e}")
            });
            if *records {
                assert!(
                    errors.iter().any(|e| e.contains("a.txt") && e.contains("is a link")),
                    "tar ({label}): the skip must be RECORDED, and recorded as OUR link refusal rather \
                     than as whatever the OS or the tar crate happened to say — got {errors:?}"
                );
            }
            assert_eq!(
                fs::read(dest.join("b.txt")).unwrap(),
                b"ARCHIVED B".to_vec(),
                "tar ({label}): a refusal must cost ONE entry — the rest of the archive still extracts"
            );
            let _ = fs::remove_dir_all(&d);
        }
    }

    /// **CPE-1812: the leaf-link guard, independently pinned from the containment guard beside it.**
    ///
    /// `entry_sink_action` asks two different questions in order — "is a link already sitting at this
    /// name?" (the LEAF half, [`entry_slot_action`]) and, only if that passes, "does the resolved path
    /// stay inside `dest`?" (the CONTAINMENT half, [`crate::fsutil::confined_to`]). Every existing live-link
    /// fixture in this file — `rows_21_and_22` above, `rows_15_and_16`,
    /// `one_shot_and_streamed_zip_answer_a_link_at_an_entry_name_identically` — points its link's target
    /// **outside** `dest`. `confined_to` alone already refuses that shape (a live symlink's target is
    /// followed by `canonicalize`, and an outside target never resolves under `real_root`), so the LEAF
    /// half can be deleted from tar's dispatch and the whole suite still passes — found by PR #958's UAT,
    /// which applied the precise mutation (`tar_entry_refusal`'s `_ => entry_sink_action(dest, &out)`
    /// swapped for `_ => entry_dir_action(dest, &out)`) and observed **zero** new reds, on Linux as much
    /// as on Windows. The existing message-content assertions (`contains("is a link")`) do not catch it
    /// either: [`escaped_dest_message`]'s own prose happens to contain that exact substring too ("a folder
    /// on the way there **is a link**"), so the wording check the UAT trusted was never discriminating in
    /// the first place.
    ///
    /// **This leg isolates the leaf half by construction, not by wording.** The live symlink sits at the
    /// entry's own name exactly as the existing legs do, but its TARGET is a file already **inside**
    /// `dest` — so `confined_to` resolves it as contained. Under the guard-intact code the leaf check
    /// still fires first and refuses it regardless of where the target points (the leaf question is asked
    /// — and short-circuits — before containment is ever consulted), so the behaviour is unchanged: the
    /// entry is skipped, and the pre-existing link survives untouched. Remove the leaf half and
    /// containment alone ADMITS this entry — measured (not assumed): tar's own `create_new`-then-
    /// `remove_file`-and-retry unpack path (the CPE-1759 section comment above) unlinks the user's live
    /// link and writes an ordinary file in its place, so the leg reds on "the link itself must survive",
    /// not on the target's content — the link, not the file it pointed at, is what this hazard destroys
    /// for tar specifically. That loss, not a string, is what this leg checks.
    ///
    /// **Both tar and zip legs, one-shot and streamed** (four total): `entry_sink_action` is the SAME
    /// function all three sinks (tar, zip, 7z) call, but tar reaches it through a `match` with a default
    /// arm — the exact shape the UAT's mutation exploited — while zip calls it directly. A regression at
    /// either call site, or inside the function itself, should turn its own leg red independently.
    ///
    /// **Red-proof (stated so the claim is checked, not just made; re-run 2026-08-23):** reverting
    /// `tar_entry_refusal`'s `_ => entry_sink_action(dest, &out)` to `_ => entry_dir_action(dest, &out)`
    /// turned the "tar one-shot" leg here red (`the link itself must survive untouched (outcome was
    /// Ok([]))`) while `rows_21_and_22_tar_refuse_a_link_at_an_entry_name_and_still_extract_the_rest`
    /// above — the existing outside-pointing leg — stayed green, exactly the discrimination the ticket
    /// asked for. (The "tar streamed" leg goes through the identical mutated dispatch and was not reached
    /// only because the harness aborts a `#[test]` fn on its first panic, not because it is unaffected.)
    /// The ZIP legs are unaffected by that specific mutation (zip's call site is untouched by it), so
    /// verified separately: commenting out `entry_sink_action`'s leaf-check block entirely (isolating the
    /// zip legs by temporarily removing the tar ones, since the harness stops at the first panic) turned
    /// "zip one-shot" red on the OTHER assertion — the victim's bytes, measured `[65, 82, 67, 72, 73, 86,
    /// 69, 68, 32, 65]` ("ARCHIVED A") where `"VICTIM ORIGINAL"` was expected — confirming zip's mechanism
    /// really is write-THROUGH (not tar's unlink-and-replace), and that `entry_sink_action`'s leaf half is
    /// what stops it on both sinks that share the function.
    #[test]
    fn cpe1812_the_leaf_link_guard_is_pinned_independently_of_containment() {
        // (label, extension, run) — tar and zip, one-shot and streamed.
        type Run = fn(&Path, &Path) -> Result<Vec<String>, String>;
        let legs: &[(&str, &str, Run)] = &[
            ("tar one-shot", "tar.gz", |archive: &Path, dest: &Path| {
                extract_archive(&archive.to_string_lossy(), &dest.to_string_lossy())
                    .map(|o| o.report.errors)
            }),
            ("tar streamed", "tar.gz", |archive: &Path, dest: &Path| {
                let cancel = AtomicBool::new(false);
                extract_archive_streamed(&archive.to_string_lossy(), &dest.to_string_lossy(), &cancel, |_| {})
                    .map(|r| r.errors)
            }),
            ("zip one-shot", "zip", |archive: &Path, dest: &Path| {
                extract_archive(&archive.to_string_lossy(), &dest.to_string_lossy())
                    .map(|o| o.report.errors)
            }),
            ("zip streamed", "zip", |archive: &Path, dest: &Path| {
                let cancel = AtomicBool::new(false);
                extract_archive_streamed(&archive.to_string_lossy(), &dest.to_string_lossy(), &cancel, |_| {})
                    .map(|r| r.errors)
            }),
        ];

        for (label, ext, run) in legs {
            let d = scratch("cpe1812_leaf_inside");
            let archive = d.join(format!("in.{ext}"));
            if *ext == "tar.gz" {
                compress_to_targz(&two_source_files(&d), &archive.to_string_lossy()).unwrap();
            } else {
                compress_to_zip(&two_source_files(&d), &archive.to_string_lossy()).unwrap();
            }
            let dest = d.join("out");
            fs::create_dir_all(&dest).unwrap();
            // The victim lives INSIDE `dest` — the whole point of this leg. Containment alone would
            // admit this target; only the leaf-link guard can refuse it.
            let victim = dest.join("victim-inside-dest.bin");
            fs::write(&victim, b"VICTIM ORIGINAL").unwrap();
            let link = dest.join("a.txt");
            if !crate::fsutil::require_staged("live_file_symlink", true, stage_live_link(&victim, &link)) {
                crate::skip_notice!(
                    "[CPE-1812] SKIPPED the leaf-guard discrimination leg ({label}): could not stage a \
                     live link at {}. The leaf-vs-containment split was NOT independently checked on this \
                     run.",
                    link.display()
                );
                let _ = fs::remove_dir_all(&d);
                // CPE-1809: `continue`, not `return` — each leg stages its own link fresh, so a failure
                // on one must not abandon the rest of the table.
                continue;
            }

            let outcome = run(&archive, &dest);

            // ---- the harm, before the Result ----
            assert_eq!(
                fs::read(&victim).unwrap(),
                b"VICTIM ORIGINAL".to_vec(),
                "{label}: the entry's bytes went THROUGH the link into {}, a file INSIDE the user's own \
                 destination that the archive never named. Containment alone cannot catch this — the \
                 target resolves inside `dest` — so only the LEAF-link guard protects it \
                 (outcome was {outcome:?})",
                victim.display()
            );
            assert!(
                fs::symlink_metadata(&link).map(|m| m.file_type().is_symlink()).unwrap_or(false),
                "{label}: the link itself must survive untouched (outcome was {outcome:?})"
            );

            let errors = outcome.unwrap_or_else(|e| {
                panic!("{label}: one refused entry must not abort the run: {e}")
            });
            assert!(
                errors.iter().any(|e| e.contains("a.txt") && e.contains("is a link")),
                "{label}: the skip must be recorded as OUR link refusal — got {errors:?}"
            );
            assert_eq!(
                fs::read(dest.join("b.txt")).unwrap(),
                b"ARCHIVED B".to_vec(),
                "{label}: a refusal must cost ONE entry — the rest of the archive still extracts"
            );
            let _ = fs::remove_dir_all(&d);
        }
    }

    /// `src/a.txt` + `src/b.txt` + `src/c.txt`, in that archive order. **`b.txt` is the poisoned one, and
    /// its position in the middle is the whole design** — see
    /// `one_shot_and_streamed_zip_answer_a_link_at_an_entry_name_identically`.
    fn three_source_files(d: &Path) -> Vec<String> {
        let src = d.join("src");
        fs::create_dir_all(&src).unwrap();
        for n in ["a.txt", "b.txt", "c.txt"] {
            fs::write(src.join(n), format!("ARCHIVED {n}").as_bytes()).unwrap();
        }
        ["a.txt", "b.txt", "c.txt"].iter().map(|n| src.join(n).to_string_lossy().to_string()).collect()
    }

    /// **CPE-1759: the one-shot and streamed ZIP paths now answer a link at an entry's name the same
    /// way** — the re-pointed `one_shot_zip_extraction_aborts_everything_when_an_entry_lands_on_a_link`.
    ///
    /// That test pinned the divergence: `extract_archive` handed its zip branch to
    /// `zip::ZipArchive::extract`, which aborted the whole run, while `extract_archive_streamed` skipped
    /// the entry and extracted the rest. Two shipped paths, opposite answers to one input.
    ///
    /// # Why the poisoned entry is `b.txt` and not `a.txt`
    ///
    /// The old test staged the link at the archive's **first** entry, and that is why abort looked
    /// atomic: CPE-1744 recorded it as "the user gets a clear error and can retry into an empty folder",
    /// and the CPE-1773/1774 review confirmed the destination was empty. Both observations were true of
    /// that archive and false in general. `zip-2.4.2`'s `extract_internal` (`src/read.rs:897`) is a plain
    /// `for` loop with `?` on `safe_prepare_path`, so the refusal fires **mid-loop**. Measured on this
    /// branch before the fix, three entries with the poison second:
    ///
    /// ```text
    /// [M1] outcome                          = Err("invalid Zip archive: Invalid symlink target path")
    /// [M1] a.txt (BEFORE the poison) exists = true
    /// [M1] c.txt (AFTER  the poison) exists = false
    /// ```
    ///
    /// A half-extraction *and* an error, with nothing saying which half. So the ordering is not cosmetic:
    /// with the link at entry 0, a revert to abort still leaves an empty folder and a straightforward
    /// `!exists` assertion could not tell "aborted" from "skipped and wrote nothing". Asserting that the
    /// entry **after** the refusal also landed is what makes a revert red.
    #[test]
    fn one_shot_and_streamed_zip_answer_a_link_at_an_entry_name_identically() {
        // (label, runs it, does it have somewhere to record the skip?)
        type Run = fn(&Path, &Path) -> Result<Vec<String>, String>;
        let legs: &[(&str, Run, bool)] = &[
            (
                "one-shot extract_archive",
                |zip: &Path, dest: &Path| {
                    extract_archive(&zip.to_string_lossy(), &dest.to_string_lossy())
                        .map(|outcome| outcome.report.errors)
                },
                // CPE-1837: `ArchiveExtractOutcome` now carries the report, so this leg records too.
                true,
            ),
            (
                "extract_archive_streamed",
                |zip: &Path, dest: &Path| {
                    let cancel = AtomicBool::new(false);
                    extract_archive_streamed(&zip.to_string_lossy(), &dest.to_string_lossy(), &cancel, |_| {})
                        .map(|r| {
                            assert_eq!(r.skipped, 1, "one refusal, counted once (CPE-1775); got {r:?}");
                            assert_eq!(r.done, 2, "and the other two entries written; got {r:?}");
                            r.errors
                        })
                },
                true,
            ),
        ];

        for (label, run, records) in legs {
            let d = scratch("cpe1759_zip_align");
            let zip = d.join("in.zip");
            compress_to_zip(&three_source_files(&d), &zip.to_string_lossy()).unwrap();
            let victim = d.join("victim-the-user-never-named.bin");
            fs::write(&victim, b"VICTIM ORIGINAL").unwrap();
            let dest = d.join("out");
            fs::create_dir_all(&dest).unwrap();
            let link = dest.join("b.txt");
            if !crate::fsutil::require_staged("live_file_symlink", true, stage_live_link(&victim, &link)) {
                crate::skip_notice!(
                    "[CPE-1759] SKIPPED the ZIP alignment leg ({label}): could not stage a live link at \
                     {}. The one-shot/streamed alignment was NOT checked on this run.",
                    link.display()
                );
                let _ = fs::remove_dir_all(&d);
                // CPE-1809: `continue`, not `return` — the two legs stage independently in their own
                // scratch directory, so a failure on one must not silently abandon the other.
                continue;
            }

            let outcome = run(&zip, &dest);

            // ---- the harm, before the Result ----
            assert_eq!(
                fs::read(&victim).unwrap(),
                b"VICTIM ORIGINAL".to_vec(),
                "zip ({label}): the entry's bytes went THROUGH the link (outcome was {outcome:?})"
            );
            assert!(
                fs::symlink_metadata(&link).map(|m| m.file_type().is_symlink()).unwrap_or(false),
                "zip ({label}): the link must survive untouched (outcome was {outcome:?})"
            );

            // ---- the alignment itself ----
            let errors = outcome.unwrap_or_else(|e| {
                panic!(
                    "zip ({label}): one refused entry must not abort the run. This is the CPE-1759 \
                     decision going backwards; before it, the one-shot leg returned exactly this: {e}"
                )
            });
            assert_eq!(
                fs::read(dest.join("a.txt")).unwrap(),
                b"ARCHIVED a.txt".to_vec(),
                "zip ({label}): the entry BEFORE the refusal must be on disk"
            );
            assert_eq!(
                fs::read(dest.join("c.txt")).unwrap(),
                b"ARCHIVED c.txt".to_vec(),
                "zip ({label}): and so must the entry AFTER it. This is the assertion an abort fails: the \
                 crate's loop refuses mid-iteration, so `a.txt` lands, `c.txt` does not, and the caller \
                 gets an error that names neither"
            );
            if *records {
                assert!(
                    errors.iter().any(|e| e.contains("b.txt") && e.contains("is a link")),
                    "zip ({label}): the skip must be recorded as OUR link refusal, not as whatever the \
                     zip crate happened to say — an `is_err()`-shaped check here would stay green through \
                     a disk-full or permission failure that proves nothing about links. Got {errors:?}"
                );
            }
            let _ = fs::remove_dir_all(&d);
        }
    }

    /// **The `ErrorKind` decoding this module's link classifier rests on, measured rather than assumed.**
    ///
    /// Review round 2 asserted, in three places and a commit message, that Rust decodes
    /// `ERROR_PRIVILEGE_NOT_HELD` (1314) and `ERROR_ACCESS_DENIED` (5) to the same `PermissionDenied`,
    /// and that separating them was why the classifier reads raw codes. It does not, and this is the
    /// leg that keeps the correction true instead of leaving it as a second unmeasured story:
    ///
    /// ```text
    /// [K] raw     1 -> Uncategorized      [K] raw   120 -> Unsupported
    /// [K] raw     5 -> PermissionDenied   [K] raw    50 -> Uncategorized
    /// [K] raw  1314 -> Uncategorized
    /// ```
    ///
    /// The real reason is stronger and this asserts it directly: 1314, 1 and 50 decode to a kind with
    /// **no stable name** (`Uncategorized`, on a `#[non_exhaustive]` enum), so a kind-based classifier
    /// cannot express them at all. If a future toolchain gives any of them a nameable kind, this goes
    /// red and `link_creation_is_categorical`'s reasoning can be revisited — which is the only way a
    /// claim about someone else's mapping stays honest.
    #[cfg(windows)]
    #[test]
    fn cpe1759_the_windows_link_codes_have_no_nameable_error_kind() {
        use std::io::{Error, ErrorKind};
        for code in [ERROR_PRIVILEGE_NOT_HELD, 1, 50] {
            let kind = Error::from_raw_os_error(code).kind();
            assert_ne!(
                kind,
                ErrorKind::PermissionDenied,
                "raw {code} decoding to PermissionDenied would make the round-2 story true after all, \
                 and would mean 5 and {code} really are indistinguishable by kind"
            );
            assert_ne!(
                kind, ErrorKind::Unsupported,
                "and if raw {code} ever decodes to Unsupported, the raw-code arm for it is redundant"
            );
        }
        assert_eq!(
            Error::from_raw_os_error(5).kind(),
            ErrorKind::PermissionDenied,
            "ERROR_ACCESS_DENIED is the one that IS nameable — and it must stay a failure"
        );
    }

    /// **CPE-1759: the refusal/failure line for link creation, both sides, as a pure classifier.**
    ///
    /// The first version of this ticket had no such line — it mapped **every** `io::Error` from
    /// `create_entry_symlink` to a refusal whose text asserted the cause was the missing Windows symlink
    /// privilege, so a full disk or a read-only directory produced a green extraction advising the user
    /// to turn on Developer Mode, while `File::create` twelve lines away aborted on the same errors.
    ///
    /// This is a pure test for the reason `entry_slot_action`'s is: **none** of the arms that matter can
    /// be staged on any runner. 1314 needs an unprivileged Windows account; the no-link-support codes
    /// need a FAT volume mounted; `Unsupported` on POSIX needs a filesystem that answers `ENOSYS`. With
    /// the classification inline, the arms that were wrong would again be the arms nothing reaches.
    #[test]
    fn cpe1759_link_creation_separates_a_categorical_refusal_from_a_failure() {
        use std::io::{Error, ErrorKind};

        assert!(
            link_creation_is_categorical(&Error::new(ErrorKind::Unsupported, "no links here")),
            "a filesystem with no links at all is a refusal — the entry is impossible, not the write"
        );
        #[cfg(windows)]
        {
            assert!(
                link_creation_is_categorical(&Error::from_raw_os_error(ERROR_PRIVILEGE_NOT_HELD)),
                "ERROR_PRIVILEGE_NOT_HELD is the Windows symlink privilege, a property of the machine"
            );
            for code in WINDOWS_NO_LINK_SUPPORT {
                assert!(
                    link_creation_is_categorical(&Error::from_raw_os_error(*code)),
                    "raw {code} is a volume that cannot hold links — the case the in-app help promises \
                     a skip for, and the case round 2 shipped as an abort because it only matched \
                     `ErrorKind::Unsupported`, which none of these decode to"
                );
            }
            assert!(
                !link_creation_is_categorical(&Error::from_raw_os_error(5)),
                "...but ERROR_ACCESS_DENIED is an ordinary permission failure and must ABORT. It is the \
                 one Windows code here with a nameable kind, so a classifier rewritten to match \
                 `PermissionDenied` fails HERE and on the 1314 leg above — both, and for different \
                 reasons"
            );
        }
        #[cfg(unix)]
        {
            assert!(
                link_creation_is_categorical(&Error::from_raw_os_error(EPERM)),
                "EPERM is what `symlink(2)` documents for a filesystem that cannot hold links"
            );
            assert!(
                !link_creation_is_categorical(&Error::from_raw_os_error(13)),
                "...and EACCES is the write-permission failure, which must ABORT. THESE two are the \
                 genuine same-`ErrorKind` collision (both PermissionDenied) that round 2 wrongly \
                 attributed to the Windows pair — so on POSIX the raw code really is the only separator"
            );
        }
        for (label, e) in [
            ("disk full", Error::other("No space left on device")),
            ("not found", Error::from(ErrorKind::NotFound)),
            ("already exists", Error::from(ErrorKind::AlreadyExists)),
        ] {
            assert!(
                !link_creation_is_categorical(&e),
                "{label}: a failure of the write must ABORT, like `File::create`'s does in the same \
                 loop — reporting it as a skip returns Ok with an entry silently missing"
            );
        }
    }

    /// A fake error type shaped exactly like `tar-0.4.46`'s own `TarError` — `Display` shows only
    /// `desc`, `source()` returns the wrapped `io::Error` — used by every `wrap_like_tar_*` helper below
    /// to reproduce the crate's real wrap shapes rather than an approximation of them.
    #[derive(Debug)]
    struct FakeTarError {
        desc: String,
        io: std::io::Error,
    }
    impl std::fmt::Display for FakeTarError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            self.desc.fmt(f)
        }
    }
    impl std::error::Error for FakeTarError {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.io)
        }
    }

    /// Reproduces the **exact** two-level wrap `tar-0.4.46/src/entry.rs` puts around a symlink/hard-link
    /// creation failure inside `Entry::unpack_in` (`entry.rs:529-568` for the first wrap, `error.rs`'s
    /// `From<TarError> for Error` for the second) — not an approximation, the same shape, so tests using
    /// it exercise the real hazard [`recover_link_syscall_error`] exists for rather than a stand-in for
    /// it. `marker` is [`TAR_SYMLINK_MARKER`] or [`TAR_HARDLINK_MARKER`]; the entry name/destination in
    /// the outer wrap is a fixed, harmless `"x"` — see [`wrap_like_tar_unpack_in_with_attacker_outer`]
    /// for the variant that makes it attacker-controlled.
    fn wrap_like_tar_link_syscall_failure(raw: std::io::Error, marker: &str) -> std::io::Error {
        wrap_like_tar_unpack_in_with_attacker_outer(raw, marker, "x")
    }

    /// Same two-level wrap, but the OUTER `TarError::desc` — `unpack_in`'s
    /// `"failed to unpack `{file_dst}`"` — carries `file_dst_display` verbatim, standing in for an
    /// archive's own attacker-chosen entry path (CPE-1813 review round 1, blocker 1's regression tests).
    fn wrap_like_tar_unpack_in_with_attacker_outer(
        raw: std::io::Error,
        marker: &str,
        file_dst_display: &str,
    ) -> std::io::Error {
        // `unpack()`'s symlink/hard-link arm: `Error::new(err.kind(), format!("{err}{marker}{src} to {dst}"))`.
        let inner = std::io::Error::new(raw.kind(), format!("{raw}{marker}a to b"));
        let tar_err = FakeTarError { desc: format!("failed to unpack `{file_dst_display}`"), io: inner };
        // `unpack_in()`'s own wrap, via `TarError`'s `From<TarError> for Error`: `Error::new(t.io.kind(), t)`.
        std::io::Error::new(tar_err.io.kind(), tar_err)
    }

    /// Reproduces the **single-level** wrap `unpack_in` puts around `ensure_dir_created`'s
    /// parent-directory failure (`entry.rs:434`, `"failed to create `{parent}`"`) — the raw,
    /// unreformatted `io::Error` wrapped straight into a `TarError`, with no `"{err} when …"` text
    /// anywhere in the chain, unlike the genuine symlink/hard-link syscall wrap above (CPE-1813 review
    /// round 1, blocker 2's regression test). **Not** a model for `set_symlink_file_times`'s failure —
    /// see [`wrap_like_tar_mtime_failure`], which round 2 added because this one modelled that shape
    /// wrongly (round 1 review, blocker 2's own comment on this).
    fn wrap_like_tar_single_level_failure(raw: std::io::Error, outer_desc: &str) -> std::io::Error {
        let tar_err = FakeTarError { desc: outer_desc.to_string(), io: raw };
        std::io::Error::new(tar_err.io.kind(), tar_err)
    }

    /// **CPE-1813: [`recover_link_syscall_error`] reads back the code `tar`'s wrap discards, for both
    /// link kinds.**
    ///
    /// Measured directly rather than assumed: `raw_os_error()` on the wrapped error is `None` — the
    /// whole reason this function exists — and the recovered error's own `raw_os_error()`/`kind()`
    /// still equal the original.
    #[test]
    fn cpe1813_recover_link_syscall_error_reads_the_code_tar_rewrapped_away() {
        use std::io::ErrorKind;
        for marker in [TAR_SYMLINK_MARKER, TAR_HARDLINK_MARKER] {
            for code in [1, 13, 50, 120, 1314] {
                let raw = std::io::Error::from_raw_os_error(code);
                let wrapped = wrap_like_tar_link_syscall_failure(raw, marker);
                assert_eq!(
                    wrapped.raw_os_error(),
                    None,
                    "marker {marker:?}, os error {code}: precondition — tar's wrap must actually have \
                     discarded raw_os_error, or this test is not exercising the hazard it claims to"
                );
                let recovered = recover_link_syscall_error(&wrapped, marker)
                    .unwrap_or_else(|| panic!("marker {marker:?}, os error {code}: expected evidence"));
                assert_eq!(
                    recovered.raw_os_error(),
                    Some(code),
                    "marker {marker:?}, os error {code}: the code must be recoverable from the wrapped \
                     error's text, or every raw-code arm of `link_creation_is_categorical` is \
                     unreachable for TAR"
                );
            }
            // An `Unsupported`-kind failure has no "(os error N)" text at all — the kind must still
            // come through (so ZIP's `Unsupported` arm agrees), with no code invented.
            let unsupported = std::io::Error::new(ErrorKind::Unsupported, "no links here");
            let recovered = recover_link_syscall_error(&wrap_like_tar_link_syscall_failure(unsupported, marker), marker)
                .unwrap_or_else(|| panic!("marker {marker:?}: expected Unsupported-kind evidence"));
            assert_eq!(recovered.kind(), ErrorKind::Unsupported, "marker {marker:?}");
            assert_eq!(recovered.raw_os_error(), None, "marker {marker:?}: no code was ever parseable");
        }
        // Wrong marker for the shape actually present — e.g. asking for a hard-link's evidence in a
        // symlink failure — must find nothing, not misread the other kind's text.
        let symlink_shaped = wrap_like_tar_link_syscall_failure(std::io::Error::from_raw_os_error(1), TAR_SYMLINK_MARKER);
        assert!(
            recover_link_syscall_error(&symlink_shaped, TAR_HARDLINK_MARKER).is_none(),
            "a symlink-shaped wrap must not answer a hard-link marker query"
        );
    }

    /// **CPE-1829 — the one assertion in this file that pins `recover_link_syscall_error` against the
    /// REAL `tar` crate, not [`wrap_like_tar_link_syscall_failure`]'s double.**
    ///
    /// Every other test above builds its wrapped error from a `FakeTarError` shaped by hand out of the
    /// same [`TAR_HARDLINK_MARKER`]/[`TAR_SYMLINK_MARKER`] constants `recover_link_syscall_error` is
    /// meant to be checked against — so a future `tar` release that reworded its wrap text would make
    /// `recover_link_syscall_error` return `None` for every real archive, TAR would silently revert to
    /// the pre-CPE-1813 abort-the-whole-run behaviour, and every test in this file (including the ones
    /// above) would stay green, because the double would still agree with itself. This test drives a
    /// real [`tar::Entry::unpack_in`] to a genuine hard-link-creation failure instead, so that a reworded
    /// wrap goes red here.
    ///
    /// The destination name is pre-occupied so `fs::hard_link` genuinely fails with `AlreadyExists` —
    /// unlike the symlink arm (see the sibling test below), `tar`'s hard-link code has no
    /// remove-and-retry on an occupied name, so this is the one link kind guaranteed to fail the same
    /// way, un-gated, on every platform this suite runs on. Cross-checked against the Reviewer's own
    /// probe against `tar-0.4.46`'s source (this ticket's "What to do"): the real leaf renders
    /// `"Cannot create a file when that file already exists. (os error 183) when hard linking … to …"`
    /// on Windows; on POSIX the same occupied-name failure is `EEXIST` — both are `AlreadyExists`, which
    /// is all this test depends on (the exact OS code differs legitimately by platform, so it is not
    /// asserted).
    #[test]
    fn cpe1829_recover_link_syscall_error_pins_against_the_real_tar_crate_hardlink() {
        let d = scratch("cpe1829_real_tar_hardlink");
        let dest = d.join("out");
        fs::create_dir_all(&dest).unwrap();
        fs::write(dest.join("target.txt"), b"T").unwrap();
        // Pre-occupy the entry's own destination name so the real `fs::hard_link` genuinely fails,
        // rather than simulating the failure shape.
        fs::write(dest.join("hard"), b"occupied").unwrap();

        let bytes = craft_tar_with_hard_link("target.txt");
        let mut archive = tar::Archive::new(&bytes[..]);
        let mut checked = false;
        for entry in archive.entries().unwrap() {
            let mut entry = entry.unwrap();
            if entry.header().entry_type() != tar::EntryType::Link {
                continue;
            }
            let err = entry
                .unpack_in(&dest)
                .expect_err("the destination name is pre-occupied, so the real hard-link syscall must fail");
            assert_eq!(
                err.raw_os_error(),
                None,
                "precondition: tar's real wrap must discard raw_os_error the same way every double in \
                 this file claims it does, or this test is not exercising the hazard CPE-1829 is about"
            );
            let recovered = recover_link_syscall_error(&err, TAR_HARDLINK_MARKER).unwrap_or_else(|| {
                panic!(
                    "the REAL tar-0.4.46 crate's wrap did not carry TAR_HARDLINK_MARKER ({TAR_HARDLINK_MARKER:?}) \
                     where recover_link_syscall_error expects it — an upstream tar release reworded its wrap \
                     text, which is exactly the silent-revert CPE-1829 exists to catch. Real wrapped error: {err}"
                )
            });
            assert_eq!(
                recovered.kind(),
                std::io::ErrorKind::AlreadyExists,
                "recovered error's kind must match the real hard_link failure; got {recovered}"
            );
            checked = true;
        }
        assert!(checked, "the tar's hard-link entry must have been visited by the loop above");
        let _ = fs::remove_dir_all(&d);
    }

    /// **CPE-1829 — the [`TAR_SYMLINK_MARKER`] half of the real-crate pin, Unix only.**
    ///
    /// The ticket asks for this leg "if it can be triggered without `SeCreateSymbolicLinkPrivilege`" and
    /// to say so explicitly rather than write a leg that silently skips: on Windows, creating a symlink
    /// (even a dangling one) needs that privilege — administrator rights or Developer Mode — which this
    /// suite cannot assume the CI runner has, so **this leg does not run on Windows at all** (`#[cfg(unix)]`
    /// below, not a runtime skip). The hard-link test above already pins `TAR_HARDLINK_MARKER` against
    /// the real crate on every platform including Windows, so the real dependency is still exercised
    /// there — just not this marker.
    ///
    /// On Unix, plain symlink creation needs no privilege, so this drives it for real. The destination
    /// name is pre-occupied with a **directory**, not a file: `tar`'s symlink arm retries once via
    /// `remove_file` + a second `symlink` when the occupant is a plain file (its own overwrite contract,
    /// `entry.rs:561-568`) — which would silently succeed rather than failing. `remove_file` can never
    /// remove a directory, so that retry itself fails and the real syscall path genuinely errors.
    #[cfg(unix)]
    #[test]
    fn cpe1829_recover_link_syscall_error_pins_against_the_real_tar_crate_symlink() {
        let d = scratch("cpe1829_real_tar_symlink");
        let dest = d.join("out");
        fs::create_dir_all(&dest).unwrap();
        fs::create_dir_all(dest.join("link")).unwrap();

        let bytes = craft_tar_with_symlink("link", "target.txt");
        let mut archive = tar::Archive::new(&bytes[..]);
        let mut checked = false;
        for entry in archive.entries().unwrap() {
            let mut entry = entry.unwrap();
            if entry.header().entry_type() != tar::EntryType::Symlink {
                continue;
            }
            let err = entry.unpack_in(&dest).expect_err(
                "the destination name is occupied by a directory, so the real symlink attempt (and its \
                 remove-and-retry) must fail",
            );
            recover_link_syscall_error(&err, TAR_SYMLINK_MARKER).unwrap_or_else(|| {
                panic!(
                    "the REAL tar-0.4.46 crate's wrap did not carry TAR_SYMLINK_MARKER ({TAR_SYMLINK_MARKER:?}) \
                     where recover_link_syscall_error expects it — an upstream tar release reworded its wrap \
                     text, which is exactly the silent-revert CPE-1829 exists to catch. Real wrapped error: {err}"
                )
            });
            checked = true;
        }
        assert!(checked, "the tar's symlink entry must have been visited by the loop above");
        let _ = fs::remove_dir_all(&d);
    }

    /// **CPE-1813 review round 1, blocker 1 — a crafted entry name cannot forge a no-link-support
    /// refusal.**
    ///
    /// The Reviewer's own repro: an HONEST `ERROR_ACCESS_DENIED`/`EACCES`-shaped failure (must ABORT,
    /// nothing to do with link support) wrapped exactly as `tar` wraps it, but with the OUTER
    /// `TarError::desc` — `unpack_in`'s own `"failed to unpack `{file_dst}`"` — carrying an
    /// attacker-chosen entry path that happens to spell a categorical code's own text,
    /// `payload (os error 1)`. Before this fix, scraping `e.to_string()` (the outer level) directly
    /// found `1` — in [`WINDOWS_NO_LINK_SUPPORT`] — and skipped a genuine write failure. Now `e` is
    /// never inspected; only the inner, genuine syscall-error level is, so this must still ABORT, naming
    /// the real cause (5/13), not the forged one (1).
    #[test]
    fn cpe1813_a_crafted_entry_name_cannot_forge_a_link_support_refusal() {
        let (target, out) = (Path::new("some/target"), Path::new("dest/payload (os error 1)"));
        #[cfg(windows)]
        let (honest_code, forged_code) = (5, 1); // ERROR_ACCESS_DENIED vs. a WINDOWS_NO_LINK_SUPPORT code
        #[cfg(unix)]
        let (honest_code, forged_code) = (13, 1); // EACCES vs. EPERM

        assert!(
            !link_creation_is_categorical(&std::io::Error::from_raw_os_error(honest_code)),
            "precondition: the honest failure must not itself be categorical"
        );
        assert!(
            link_creation_is_categorical(&std::io::Error::from_raw_os_error(forged_code)),
            "precondition: the forged code must be one the classifier treats as categorical, or this \
             test proves nothing"
        );

        let attacker_named_e = wrap_like_tar_unpack_in_with_attacker_outer(
            std::io::Error::from_raw_os_error(honest_code),
            TAR_SYMLINK_MARKER,
            &format!("payload (os error {forged_code})"),
        );

        let outcome = tar_link_creation_outcome(target, out, &attacker_named_e, TAR_SYMLINK_MARKER);
        let msg = outcome.expect_err(
            "an honest write failure named through a hostile entry path must still ABORT — an Ok(Some) \
             here means the attacker's chosen file name silently converted a real failure into a skip"
        );
        assert!(
            !msg.contains(&format!("(os error {forged_code})"))
                || msg.contains(&format!("(os error {honest_code})")),
            "the abort must be driven by the REAL cause, not the forged one embedded in the entry name: \
             {msg}"
        );
    }

/// Reproduces the **genuine two-level** nesting `unpack_in` puts around
    /// `set_symlink_file_times`'s mtime-after-creation failure (CPE-1813 review round 2 — round 1's
    /// `wrap_like_tar_single_level_failure` modelled this as one level and it is not).
    ///
    /// `unpack()`'s mtime branch wraps the raw error in its OWN `TarError`
    /// (`"failed to set mtime for `{dst}`"`, `entry.rs:589`) — call it `mid` — and `unpack_in` wraps
    /// `mid` again in the outer `"failed to unpack `{file_dst}`"` `TarError`. So `mid`, not the raw
    /// error, is what sits one level down from the top (`e.source()`), and `mid`'s own rendered text is
    /// `"failed to set mtime for `{dst}`"` — `dst_display` here — which is the entry's own
    /// attacker-controlled destination path. This is what let `entry.rs:434`'s reasoning fail one arm
    /// short of correct: `dst_display` embedding a marker was a hazard `wrap_like_tar_single_level_failure`
    /// could not even construct, because it never modelled the level whose text contains it.
    fn wrap_like_tar_mtime_failure(raw: std::io::Error, dst_display: &str) -> std::io::Error {
        let mid_tar_err = FakeTarError { desc: format!("failed to set mtime for `{dst_display}`"), io: raw };
        let mid = std::io::Error::new(mid_tar_err.io.kind(), mid_tar_err);
        let outer_tar_err = FakeTarError { desc: "failed to unpack `x`".to_string(), io: mid };
        std::io::Error::new(outer_tar_err.io.kind(), outer_tar_err)
    }

    /// **CPE-1813 review round 1, blocker 2 — a parent-directory failure is never treated as a
    /// no-link-support refusal, even when its own raw code is genuinely categorical.**
    ///
    /// `ensure_dir_created` wraps its raw `io::Error` straight into a `TarError` with no
    /// `"{err} when …"` text — see [`wrap_like_tar_single_level_failure`]'s doc. A walk that trusted any
    /// `raw_os_error()` found anywhere in the chain would misread a genuine EPERM there as "this volume
    /// has no links" and skip a link entry whose creation never even ran. The code used here WOULD be
    /// categorical if it came from the actual syscall — the point is that it did not, and this must
    /// still ABORT.
    #[test]
    fn cpe1813_a_parent_dir_failure_is_never_a_link_support_refusal() {
        #[cfg(windows)]
        let categorical_code = WINDOWS_NO_LINK_SUPPORT[0];
        #[cfg(unix)]
        let categorical_code = EPERM;
        assert!(
            link_creation_is_categorical(&std::io::Error::from_raw_os_error(categorical_code)),
            "precondition: the code used below must be one the classifier treats as categorical when it \
             genuinely comes from the syscall, or this test proves nothing"
        );

        let (target, out) = (Path::new("some/target"), Path::new("dest/good_link"));
        let e = wrap_like_tar_single_level_failure(
            std::io::Error::from_raw_os_error(categorical_code),
            "failed to create `dest/parent`",
        );
        for marker in [TAR_SYMLINK_MARKER, TAR_HARDLINK_MARKER] {
            let outcome = tar_link_creation_outcome(target, out, &e, marker);
            assert!(
                outcome.is_err(),
                "ensure_dir_created: a failure with no `{marker:?}` evidence must ABORT even though its \
                 own raw code ({categorical_code}) would be categorical if it had come from the actual \
                 link syscall — got {outcome:?}"
            );
        }
    }

    /// **CPE-1813 review round 2, blocker 1 (finding 1) — an mtime failure on an entry whose OWN name
    /// embeds the syscall marker text is never treated as a no-link-support refusal.**
    ///
    /// This is the Reviewer's own repro, reproduced against the CORRECT two-level nesting (see
    /// [`wrap_like_tar_mtime_failure`]'s doc): a symlink that was already successfully created, whose
    /// **mtime set failed `Unsupported`**, named so `mid`'s own text —
    /// `"failed to set mtime for `dest/a when symlinking b`"` — contains [`TAR_SYMLINK_MARKER`] inside
    /// the entry's own attacker-controlled destination path. Round 1's fix anchored the CODE arm
    /// (`parse_os_error_code`'s `starts_with` check) but not the KIND-only fallback arm, so this used to
    /// recover `Some(io::Error{kind: Unsupported, ..})` and skip a link that was already on disk. The
    /// fix in round 2 is structural (see `recover_link_syscall_error`'s doc): `mid` is excluded because
    /// it is not a leaf, so the walk never reads its text at all.
    #[test]
    fn cpe1813_an_mtime_failure_named_to_embed_the_marker_is_never_a_link_support_refusal() {
        use std::io::ErrorKind;
        let (target, out) = (Path::new("some/target"), Path::new("dest/good_link"));
        let e = wrap_like_tar_mtime_failure(
            std::io::Error::new(ErrorKind::Unsupported, "not implemented"),
            "dest/a when symlinking b",
        );
        for marker in [TAR_SYMLINK_MARKER, TAR_HARDLINK_MARKER] {
            let outcome = tar_link_creation_outcome(target, out, &e, marker);
            assert!(
                outcome.is_err(),
                "set_symlink_file_times: an mtime failure on an entry named to embed `{marker:?}` in its \
                 own path must still ABORT — the symlink it names already exists on disk; got {outcome:?}"
            );
        }
    }

    /// **CPE-1813 review round 2, blocker 1 — audited beyond the Reviewer's own example:
    /// `validate_inside_dst`'s hard-link leg is the one other leaf-shaped wrap on this path, and it is
    /// excluded the same structural way.**
    ///
    /// `entry.rs:543` wraps a canonicalize failure on the hard link's own resolved target as
    /// `"{err} while canonicalizing {attacker-declared target}"` — a leaf (no further `source()`), like
    /// the genuine syscall wrap, so [`recover_link_syscall_error`]'s leaf check alone does not exclude
    /// it. Only reachable via a hard-link entry whose declared target does not resolve, but that is
    /// exactly the kind of narrow, unstaged trigger the mtime hole was before it was measured — excluded
    /// on the same principle rather than left as a theoretical gap.
    #[test]
    fn cpe1813_a_canonicalize_failure_named_to_embed_the_marker_is_never_a_link_support_refusal() {
        use std::io::ErrorKind;
        let (target, out) = (Path::new("some/target"), Path::new("dest/good_link"));
        // `Unsupported` rather than a realistic canonicalize failure (which is practically always
        // NotFound/PermissionDenied, neither categorical, and would abort anyway without the guard —
        // its own raw code is simply never in `WINDOWS_NO_LINK_SUPPORT`/`EPERM`): the guard has to be
        // exercised against the arm it actually protects, the kind-only fallback, not against a code
        // the classifier would reject on its own merits regardless of this guard's presence.
        let inner = std::io::Error::new(ErrorKind::Unsupported, "not implemented");
        let text = format!("{inner} while canonicalizing dest/a when hard linking b to c");
        let leaf = std::io::Error::new(inner.kind(), text);
        let outer_tar_err = FakeTarError { desc: "failed to unpack `x`".to_string(), io: leaf };
        let e = std::io::Error::new(outer_tar_err.io.kind(), outer_tar_err);

        let outcome = tar_link_creation_outcome(target, out, &e, TAR_HARDLINK_MARKER);
        assert!(
            outcome.is_err(),
            "a canonicalize failure on a hard-link target crafted to embed the marker text must still \
             ABORT — got {outcome:?}"
        );
    }

    /// **CPE-1813: TAR and ZIP must agree on the no-link-support refusal — checked as ONE test, per the
    /// ticket, so a divergence between the two formats' routing cannot hide behind two green per-format
    /// tests.**
    ///
    /// Neither trigger can be staged live on any CI runner (see
    /// `cpe1759_link_creation_separates_a_categorical_refusal_from_a_failure`'s doc for why), so this
    /// reproduces the SHAPE of error each format's link creation actually hands our code for the same
    /// underlying OS condition and checks both translations agree with the one shared classifier,
    /// [`link_creation_is_categorical`]. ZIP sees the raw syscall error directly
    /// (`create_entry_symlink` → [`materialise_entry_symlink`]); TAR sees it only after
    /// [`wrap_like_tar_link_syscall_failure`]'s two-level rewrap, translated back by
    /// [`tar_link_creation_outcome`]. A mutation to either side's routing — or to
    /// [`recover_link_syscall_error`] — that stops the two agreeing turns this red without needing to
    /// name which format broke.
    #[test]
    fn cpe1813_tar_and_zip_agree_on_the_no_link_support_refusal() {
        use std::io::ErrorKind;
        let (target, out) = (Path::new("some/target"), Path::new("dest/good_link"));

        // (label, raw OS code, must this be a REFUSAL on both formats?)
        let mut cases: Vec<(&str, i32, bool)> = Vec::new();
        #[cfg(windows)]
        {
            cases.push(("ERROR_PRIVILEGE_NOT_HELD", ERROR_PRIVILEGE_NOT_HELD, true));
            for code in WINDOWS_NO_LINK_SUPPORT {
                cases.push(("WINDOWS_NO_LINK_SUPPORT", *code, true));
            }
            cases.push(("ERROR_ACCESS_DENIED", 5, false));
        }
        #[cfg(unix)]
        {
            cases.push(("EPERM", EPERM, true));
            cases.push(("EACCES", 13, false));
        }

        for (label, code, must_refuse) in cases {
            let raw = std::io::Error::from_raw_os_error(code);
            let zip_refuses = link_creation_is_categorical(&raw);
            assert_eq!(
                zip_refuses, must_refuse,
                "{label} (os error {code}): the classifier itself disagrees with this test's own \
                 expectation — fix the case table, not the classifier"
            );

            let tar_wrapped =
                wrap_like_tar_link_syscall_failure(std::io::Error::from_raw_os_error(code), TAR_SYMLINK_MARKER);
            let tar_outcome = tar_link_creation_outcome(target, out, &tar_wrapped, TAR_SYMLINK_MARKER);
            let tar_refuses = matches!(tar_outcome, Ok(Some(_)));

            assert_eq!(
                tar_refuses, zip_refuses,
                "{label} (os error {code}): TAR and ZIP DISAGREE on whether this is a refusal. ZIP \
                 (direct syscall error) says {zip_refuses}; TAR (via `tar_link_creation_outcome` on \
                 the tar-shaped wrapped error) says {tar_refuses}. This is the exact divergence CPE-1813 \
                 exists to close — got {tar_outcome:?}"
            );
            // `tar_refuses == must_refuse` (asserted above) already pins `Ok(Some(_))` vs `Err(_)`; the
            // non-refusal leg additionally checks the abort message names the entry, the way
            // `link_creation_outcome`'s own abort message is required to.
            if !must_refuse {
                let msg = tar_outcome.expect_err("a non-refusal must abort, not silently succeed");
                assert!(
                    msg.contains("dest") || msg.contains("good_link"),
                    "{label}: the abort message must name the entry it died on: {msg}"
                );
            }
        }

        // The `ErrorKind::Unsupported` arm survives the tar wrap too — via the `e.kind()` fallback in
        // `recover_link_syscall_error` when no OS code can be parsed from the wrapped text.
        let unsupported = std::io::Error::new(ErrorKind::Unsupported, "no links here");
        assert!(
            link_creation_is_categorical(&unsupported),
            "precondition: ZIP treats this kind as categorical"
        );
        let tar_unsupported = wrap_like_tar_link_syscall_failure(unsupported, TAR_SYMLINK_MARKER);
        assert!(
            matches!(
                tar_link_creation_outcome(target, out, &tar_unsupported, TAR_SYMLINK_MARKER),
                Ok(Some(_))
            ),
            "TAR must treat the same `Unsupported`-kind error as a refusal too, via the `e.kind()` \
             fallback when no OS code can be recovered from the wrapped text"
        );
    }

    /// A tar with `a.txt`, a **link** entry `b` (`entry_type` — symlink or hard link) -> `target`, then
    /// `c.txt` — the poisoned entry in the middle, for [`three_source_files`]'s reason: it is what makes
    /// "the rest of the archive still extracts" a meaningful assertion rather than one an abort-at-entry-0
    /// would pass by accident. Parameterised over `entry_type` (CPE-1813 review round 2, finding 2) so
    /// the two seam tests below can pin BOTH link kinds, not just symlinks.
    fn craft_tar_with_link_in_the_middle(entry_type: tar::EntryType, target: &str) -> Vec<u8> {
        let mut b = tar::Builder::new(Vec::new());
        let mut append = |name: &str, entry_type: tar::EntryType, contents: &[u8], link: Option<&str>| {
            let mut h = tar::Header::new_gnu();
            h.set_size(contents.len() as u64);
            h.set_mode(0o644);
            h.set_entry_type(entry_type);
            if let Some(l) = link {
                h.set_link_name(l).unwrap();
            }
            h.set_cksum();
            b.append_data(&mut h, name, contents).unwrap();
        };
        append("a.txt", tar::EntryType::Regular, b"ARCHIVED a.txt", None);
        append("b", entry_type, &[], Some(target));
        append("c.txt", tar::EntryType::Regular, b"ARCHIVED c.txt", None);
        b.into_inner().unwrap()
    }

    /// **CPE-1813 review round 2, blocker 3 — `tar_unpack` (one-shot) genuinely routes a link-creation
    /// refusal through the shared classifier, pinned deterministically via [`tar_unpack_with`]'s
    /// injection seam rather than depending on this machine's OS to refuse a link.**
    ///
    /// See [`tar_unpack_with`]'s doc for why a probe-and-skip live trigger cannot pin this on every
    /// machine (measured on this box: Developer Mode is on, so the unprivileged symlink API this app
    /// uses succeeds even unelevated — the OS-level 1314 trigger is real, but not reachable from here).
    /// This injects a `WINDOWS_NO_LINK_SUPPORT`/`EPERM`-shaped `Err`, exactly as `unpack_in` would
    /// produce it, at entry `b` only — `a.txt` and `c.txt` unpack through the REAL, unmodified
    /// `entry.unpack_in`.
    ///
    /// **Both link kinds, not just symlinks (CPE-1813 review round 2, finding 2).** The injected error
    /// is shaped for entry `b`'s OWN kind (symlink → [`TAR_SYMLINK_MARKER`], hard link →
    /// [`TAR_HARDLINK_MARKER`]), so this only stays green if the production code picks the SAME marker
    /// the entry's kind demands — round 1 had this decision (`entry_type.is_hard_link()`) live at both
    /// call sites but nothing that could tell the two markers apart end to end; collapsing that decision
    /// to always `TAR_SYMLINK_MARKER` silently reverted every hard-link entry to the pre-CPE-1813 abort
    /// and every other test still passed.
    ///
    /// **Red-proof (stated here so the claim is checked, not just made):** reverting the `Some(target)`
    /// arm in [`tar_unpack_with`] back to `return Err(e.to_string())` — the exact pre-CPE-1813 defect —
    /// turns this test red on the `outcome.is_ok()` assertion for BOTH legs. Collapsing the marker
    /// selection in [`tar_unpack_with`] to always [`TAR_SYMLINK_MARKER`] turns only the hard-link leg red.
    #[test]
    fn cpe1813_tar_unpack_routes_a_link_creation_refusal_through_the_shared_classifier() {
        #[cfg(windows)]
        let code = WINDOWS_NO_LINK_SUPPORT[0];
        #[cfg(unix)]
        let code = EPERM;

        for (kind, entry_type, marker) in [
            ("symlink", tar::EntryType::Symlink, TAR_SYMLINK_MARKER),
            ("hard link", tar::EntryType::hard_link(), TAR_HARDLINK_MARKER),
        ] {
            let d = scratch("cpe1813_tar_unpack_seam");
            let bytes = craft_tar_with_link_in_the_middle(entry_type, "ok.txt");
            let dest = d.join("out");

            let outcome = tar_unpack_with(std::io::Cursor::new(bytes), &dest, |entry, root| {
                let name = entry.path().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
                if name == "b" {
                    Err(wrap_like_tar_link_syscall_failure(std::io::Error::from_raw_os_error(code), marker))
                } else {
                    entry.unpack_in(root)
                }
            });

            assert!(
                outcome.is_ok(),
                "{kind}: an injected no-link-support refusal at entry `b` must be a SKIP, not an aborted \
                 run — got {outcome:?}"
            );
            assert_eq!(
                fs::read(dest.join("a.txt")).unwrap(),
                b"ARCHIVED a.txt".to_vec(),
                "{kind}: the entry BEFORE the refused one must still be written"
            );
            assert_eq!(
                fs::read(dest.join("c.txt")).unwrap(),
                b"ARCHIVED c.txt".to_vec(),
                "{kind}: and so must the entry AFTER it — the assertion an abort fails"
            );
            assert!(
                fs::symlink_metadata(dest.join("b")).is_err(),
                "{kind}: the refused link entry must not have been written at all"
            );
            let _ = fs::remove_dir_all(&d);
        }
    }

    /// **CPE-1813 review round 2, blocker 3 — `extract_tar_stream` (the live, streamed path) genuinely
    /// routes a link-creation refusal through the shared classifier, and RECORDS it.** Same seam,
    /// per-link-kind legs, and reasoning as
    /// [`cpe1813_tar_unpack_routes_a_link_creation_refusal_through_the_shared_classifier`], via
    /// [`extract_tar_stream_with`] instead.
    ///
    /// **Red-proof:** reverting the `Some(target)` arm in [`extract_tar_stream_with`] back to
    /// `return Err(e.to_string())` turns this test red the same way for both legs — `outcome` becomes
    /// `Err`, not a report with `skipped == 1`. Collapsing the marker selection to always
    /// [`TAR_SYMLINK_MARKER`] turns only the hard-link leg red.
    #[test]
    fn cpe1813_extract_tar_stream_routes_a_link_creation_refusal_through_the_shared_classifier() {
        #[cfg(windows)]
        let code = WINDOWS_NO_LINK_SUPPORT[0];
        #[cfg(unix)]
        let code = EPERM;

        for (kind, entry_type, marker) in [
            ("symlink", tar::EntryType::Symlink, TAR_SYMLINK_MARKER),
            ("hard link", tar::EntryType::hard_link(), TAR_HARDLINK_MARKER),
        ] {
            let d = scratch("cpe1813_extract_tar_stream_seam");
            let bytes = craft_tar_with_link_in_the_middle(entry_type, "ok.txt");
            let dest = d.join("out");
            fs::create_dir_all(&dest).unwrap();
            let cancel = AtomicBool::new(false);

            let outcome = extract_tar_stream_with(
                std::io::Cursor::new(bytes),
                &dest,
                0,
                2,
                &cancel,
                &mut |_| {},
                |entry, root| {
                    let name = entry.path().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
                    if name == "b" {
                        Err(wrap_like_tar_link_syscall_failure(std::io::Error::from_raw_os_error(code), marker))
                    } else {
                        entry.unpack_in(root)
                    }
                },
            );

            let report = outcome.unwrap_or_else(|e| {
                panic!(
                    "{kind}: an injected no-link-support refusal at entry `b` must be a SKIP, not an \
                     aborted run: {e}"
                )
            });
            assert_eq!(report.skipped, 1, "{kind}: the refusal must be COUNTED; got {report:?}");
            assert_eq!(report.done, 2, "{kind}: and the other two entries written; got {report:?}");
            assert!(
                report.errors.iter().any(|e| e.contains('b') && e.contains("link")),
                "{kind}: the skip must be RECORDED with a reason naming the link; got {report:?}"
            );
            assert_eq!(fs::read(dest.join("a.txt")).unwrap(), b"ARCHIVED a.txt".to_vec());
            assert_eq!(fs::read(dest.join("c.txt")).unwrap(), b"ARCHIVED c.txt".to_vec());
            assert!(fs::symlink_metadata(dest.join("b")).is_err());
            let _ = fs::remove_dir_all(&d);
        }
    }

    /// **CPE-1759 review round 3: the routing a deleted fixture used to cover.**
    ///
    /// Round 2 removed a test that pinned the wrong behaviour — correctly — but nothing replaced the
    /// leg it *incidentally* covered, and the re-review measured the hole: mutating
    /// `link_creation_outcome`'s `Ok(Some(..))` arm to `Err(..)`, which turns every categorical refusal
    /// into an aborted extraction and undoes this ticket's entire point for link entries, **turned no
    /// test red**. Deleting a test that asserts the wrong thing still deletes whatever else it happened
    /// to hold up.
    ///
    /// Pure, because that is the only way to reach these arms at all (see the classifier test above),
    /// and asserting the *shape* — `Ok(Some)` vs `Err` — because that shape is the refusal/failure
    /// decision the loop then acts on.
    #[test]
    fn cpe1759_a_categorical_refusal_is_delivered_as_a_skip_not_an_abort() {
        use std::io::{Error, ErrorKind};
        let (target, out) = (Path::new("some/target"), Path::new("dest/good_link"));

        let refused = link_creation_outcome(target, out, &Error::new(ErrorKind::Unsupported, "nope"))
            .expect("a categorical refusal must be Ok(Some(..)) — a SKIP. As Err it aborts the archive");
        let refused = refused.expect("...and it must carry a reason, not be an Ok(None) silent success");
        assert!(
            refused.contains("some/target") && refused.contains("no links"),
            "and the reason must name the target and the cause: {refused}"
        );

        #[cfg(windows)]
        {
            let privilege =
                link_creation_outcome(target, out, &Error::from_raw_os_error(ERROR_PRIVILEGE_NOT_HELD))
                    .expect("the Windows privilege case is a SKIP")
                    .expect("with a reason");
            assert!(
                privilege.contains("Developer Mode") && !privilege.contains("no links"),
                "and it must name the privilege remedy, not the wrong one: {privilege}"
            );
        }

        let failed = link_creation_outcome(target, out, &Error::other("No space left on device"))
            .expect_err("a failure must be Err(..) — as Ok(Some(..)) the run returns success with the \
                         entry missing, which is the shape this whole ticket family is about");
        assert!(
            failed.contains("dest") && failed.contains("could not create the link"),
            "and the failure must name the path it died on: {failed}"
        );
    }

    /// **CPE-1759 review round 2: a link entry replaces an ordinary file, the way a file entry does.**
    ///
    /// `symlink`/`symlink_file` are exclusive-create, so they fail `AlreadyExists` over anything already
    /// at the name. The first version of this ticket reported that as *"this system would not create it
    /// — enable Developer Mode"*, skipped the entry, and **pinned it** with a test asserting the
    /// occupying file survived. All three were wrong: `entry_sink_action` has already proven the slot is
    /// not a link, and *"overwriting an ordinary existing file is unaffected — that stays allowed"* is
    /// this module's documented contract, honoured by `File::create`'s truncate for a file entry and by
    /// `tar`'s own `remove_file`-and-retry for its links.
    ///
    /// The **directory** leg is the other side of the same call: `remove_file` cannot remove a
    /// directory, that is the write failing rather than a guard refusing, and it is recorded as a
    /// failure — a *counted* one as of CPE-1935, where it used to end the whole run. The distinction the
    /// leg exists for is untouched: the message must be one of this module's two link-**write** failures
    /// and must NOT read as a refusal, because a classifier that swallowed it would say "Skipped".
    #[test]
    fn cpe1759_a_link_entry_overwrites_an_ordinary_file_but_a_directory_is_a_failure() {
        let probe = scratch("cpe1759_linkprobe");
        fs::write(probe.join("t.txt"), b"x").unwrap();
        if !crate::fsutil::require_staged(
            "live_file_symlink",
            cfg!(any(windows, unix)),
            stage_live_link(&probe.join("t.txt"), &probe.join("l")),
        ) {
            crate::skip_notice!(
                "[CPE-1759] SKIPPED the link-overwrite legs: this machine cannot create symlinks, so \
                 every outcome below would be the categorical refusal instead."
            );
            let _ = fs::remove_dir_all(&probe);
            return;
        }
        let _ = fs::remove_dir_all(&probe);

        for occupant in ["file", "dir"] {
            let d = scratch("cpe1759_linkover");
            let ap = d.join("link.zip");
            fs::write(&ap, craft_zip_with_symlink("good_link", "ok.txt")).unwrap();
            let dest = d.join("out");
            fs::create_dir_all(&dest).unwrap();
            let slot = dest.join("good_link");
            if occupant == "file" {
                fs::write(&slot, b"IN THE WAY").unwrap();
            } else {
                fs::create_dir_all(&slot).unwrap();
            }

            let outcome = extract_archive_streamed(
                &ap.to_string_lossy(),
                &dest.to_string_lossy(),
                &AtomicBool::new(false),
                |_| {},
            );

            if occupant == "dir" {
                let report = outcome.expect(
                    "CPE-1935: one entry the write could not deliver is recorded, not raised as the \
                     whole run's error",
                );
                assert_eq!(
                    (report.done, report.failed, report.skipped),
                    (1, 1, 0),
                    "a link entry that cannot displace a DIRECTORY is the write failing, not a guard \
                     refusing — it must be a counted FAILURE, the same class `File::create` on the same \
                     path would produce, and the fixture's bystander `ok.txt` must still land: \
                     {report:?}"
                );
                let err = report.errors.join(" | ");
                // **Which of our two failure messages this is, is platform-dependent, and that was
                // measured rather than assumed** (the first version of this assertion guessed, and went
                // red on Windows). `symlink_file` over an existing *directory* answers
                // `Access is denied. (os error 5)` on Windows — NOT `AlreadyExists` — so the retry
                // branch is never entered and the message is the creation one. POSIX `symlink(2)`
                // answers `EEXIST`, so the retry runs and `remove_file` fails on the directory,
                // producing the replacement message. **Only the Linux and macOS matrix legs exercise
                // that second path.** Both are ours and neither is reachable from an unrelated failure,
                // so accepting either keeps the assertion distinctive without asserting a platform
                // constant this cannot check.
                assert!(
                    err.contains("good_link")
                        && (err.contains("could not create the link") || err.contains("could not replace")),
                    "the failure must name the entry and be one of this module's own two link-write \
                     failures — an `is_err()` check would stay green through the guard firing for some \
                     entirely different reason. Got: {err}"
                );
                assert!(
                    !err.contains("Skipped"),
                    "and it must NOT be phrased as a refusal: a directory in the way is the write \
                     failing, and `link_creation_refusal`'s wording here would mean the classifier had \
                     swallowed it. Got: {err}"
                );
                let _ = fs::remove_dir_all(&d);
                continue;
            }

            let report = outcome.expect("replacing an ordinary file must not abort the extraction");
            assert!(
                fs::symlink_metadata(&slot).map(|m| m.file_type().is_symlink()).unwrap_or(false),
                "the ordinary file must be REPLACED by the archive's link. Leaving it in place was the \
                 first version's behaviour and it contradicted this module's own promise that \
                 overwriting an ordinary existing file stays allowed"
            );
            assert_eq!(
                fs::read_to_string(&slot).unwrap(),
                "ORDINARY",
                "...and the link must resolve to the archive's own file, not to the displaced bytes"
            );
            assert_eq!(
                (report.skipped, report.errors.len()),
                (0, 0),
                "an overwrite is not a refusal and must produce NO skip notice at all; got {report:?}"
            );
            let _ = fs::remove_dir_all(&d);
        }
    }

    /// **CPE-1759 review round 2 / CPE-1935: an unreadable slot is a recorded FAILURE on the tar paths,
    /// it is not skipped.** (Round 2 wrote "ABORTS"; CPE-1935 narrowed the verdict from a whole-run
    /// abort to one counted entry failure — the run continues, and this test now asserts that. The
    /// question the arm answers is unchanged: not a skip.)
    ///
    /// The `Abort` arm of `tar_entry_refusal` was **dead** before CPE-1759 — its only producer was
    /// `link_target_action`, which never returns it. Adding `entry_sink_action` made it live, and the
    /// first version of CPE-1759 wrote `Skip(m) | Abort(m) => Some(m)`, collapsing the two. That turned
    /// a slot whose `symlink_metadata` fails for a reason other than `NotFound` — a **failure**, per
    /// `EntrySlotAction`'s own doc, aborted by all three zip sinks — into a silent tar skip with the run
    /// returning `Ok`. UAT finding 6, reintroduced three functions from the comment warning about it.
    ///
    /// Staged with `deny_stat_of`, the same mechanism the rest of this crate uses for the arm no
    /// portable API can produce, and routed through `require_staged` so a runner that *should* manage it
    /// goes red rather than quietly covering nothing (CPE-1717).
    ///
    /// # CPE-1938 moved the denial one level down, and the reason is the whole point of this note
    ///
    /// `deny_stat_of(slot)` denies the slot's **parent** as well — list-directory on Windows, `chmod
    /// 0o000` on Unix — so with the slot directly in `dest` the denied directory *was* `dest`. That was
    /// harmless while the tar legs only ever touched paths; CPE-1938 gave them a root **handle**, opened
    /// once before the first entry, so an unopenable `dest` now aborts the run before any entry is
    /// classified. Measured when this test first went red: the abort arrived, but carrying
    /// `"the extraction folder … could not be opened (Access is denied. (os error 5))"` instead of the
    /// guard's `"could not check"` — the right class of answer from the wrong guard, which is exactly
    /// the pass-for-the-wrong-reason this test's own message warns about.
    ///
    /// So the fixture now puts the entry at `sub/a.txt` and denies `dest/sub`: `dest` stays openable,
    /// the root handle opens, and `entry_sink_action`'s `Unknown` arm is reached again — the behaviour
    /// under test is unchanged, only the staging moved. The archive is **crafted** rather than built
    /// from a source tree because `compress_to_targz` would emit a `sub/` **directory** entry ahead of
    /// the file, and that entry is answered by a different guard on each platform (containment on
    /// Windows, the component walk on Unix), so the file entry would never be the one that decides the
    /// test. `craft_tar_with_entry_name` emits the file entry and a bystander, and nothing else.
    ///
    /// The new failure mode — an unopenable `dest` — is not lost: it is
    /// `cpe1938_an_unopenable_extraction_folder_aborts_the_tar_and_7z_runs`.
    ///
    /// # CPE-1935 — what this test asserts now, and what it deliberately still asserts
    ///
    /// It used to `expect_err`: an unreadable slot ended the whole tar run. Under
    /// [`EntrySlotAction`]'s scope rule the evidence is about **one name**, so the entry is now a
    /// counted `failed` and the archive's other entries still extract. The distinction this test was
    /// written to defend — *an unreadable slot is not the same thing as a link the guard chose to skip*
    /// — is the reason it still checks `skipped == 0` and still insists on the guard's own `"could not
    /// check"` wording rather than any refusal: those two are the same red for opposite reasons.
    ///
    /// The bystander `ok.txt` (`craft_tar_with_entry_name` appends one, deliberately, *after* the
    /// poisoned entry) is what makes the new half checkable at all: before this ticket it was never
    /// written on either leg.
    #[test]
    fn cpe1935_an_unreadable_slot_is_a_recorded_entry_failure_on_both_tar_paths() {
        for streamed in [false, true] {
            let d = scratch("cpe1759_tar_unreadable");
            let tgz = d.join("in.tar.gz");
            fs::write(&tgz, gzip_bytes(&craft_tar_with_entry_name("sub/a.txt", b"ARCHIVED A"))).unwrap();
            let dest = d.join("out");
            fs::create_dir_all(dest.join("sub")).unwrap();
            let slot = dest.join("sub").join("a.txt");
            fs::write(&slot, b"PRE-EXISTING").unwrap();

            // **`parent` is `dest`, not the scratch root, and the distinction is load-bearing on the
            // platforms this cannot run on.** `deny_stat_of` denies `slot.parent()` — `dest/sub` since
            // CPE-1938 moved the entry a level down, `dest` before that — and
            // `undo_deny_stat_of`'s unix leg restores *only* the directory it is handed
            // (`fsutil.rs:2606-2611`; the Windows leg happens to also walk `target.parent()`, which is
            // what hid this). Handing it the scratch root left `dest` at `0o000` on Linux and macOS, so
            // the `remove_dir_all` below could not descend into it and the tree leaked — twice per run,
            // accumulating undeletable directories on CI runners. The root is removed separately.
            struct Restore<'a> {
                target: &'a Path,
                parent: &'a Path,
                root: &'a Path,
            }
            impl Drop for Restore<'_> {
                fn drop(&mut self) {
                    crate::fsutil::undo_deny_stat_of(self.target, self.parent);
                    let _ = fs::remove_dir_all(self.root);
                }
            }
            let sub = dest.join("sub");
            let _r = Restore { target: &slot, parent: &sub, root: &d };

            // CPE-1814: this used to be `if !deny_stat_of(&slot) { skip_notice!(..); return; }` — a
            // staging failure on the FIRST loop iteration (`streamed=false`) `return`ed out of the whole
            // `for streamed` loop, so a lenient local run that could not stage silently checked NEITHER
            // leg while the test still reported `ok`. `require_staged` inside `deny_stat_of` already
            // turns a staging failure red under CI (CPE-1717); the `assert!` below closes the matching
            // gap for a lenient local run — a fixture that cannot be staged is a broken test, not a
            // smaller one, so this fails loudly instead of quietly shrinking to zero legs checked.
            assert!(
                crate::fsutil::deny_stat_of(&slot),
                "[CPE-1814] could not deny stat of {} for the unreadable-slot tar leg (streamed={streamed}). \
                 NOTHING in this test would have covered that route — failing loudly rather than skipping.",
                slot.display()
            );

            let outcome = if streamed {
                extract_archive_streamed(
                    &tgz.to_string_lossy(),
                    &dest.to_string_lossy(),
                    &AtomicBool::new(false),
                    |_| {},
                )
            } else {
                extract_archive(&tgz.to_string_lossy(), &dest.to_string_lossy()).map(|o| o.report)
            };

            // CPE-1935, evidence first and on the filesystem: the bystander AFTER the poisoned entry
            // must be on disk. It never was before this ticket, on either leg.
            assert_eq!(
                fs::read(dest.join("ok.txt")).ok().as_deref(),
                Some(&b"ORDINARY"[..]),
                "(streamed={streamed}) an unreadable slot at ONE entry took the rest of the archive \
                 down with it: {outcome:?}"
            );
            let report = outcome.unwrap_or_else(|e| {
                panic!("(streamed={streamed}) one unreadable slot must not be the whole run's error: {e}")
            });
            assert_eq!(
                (report.done, report.failed, report.skipped),
                (1, 1, 0),
                "an unreadable slot is an I/O FAILURE, not a policy refusal: counting it as a skip says \
                 a guard chose to drop an entry for a reason that has nothing to do with the archive. \
                 Every zip sink records it as a failure and tar must too. (streamed={streamed}) \
                 {report:?}"
            );
            assert!(
                report.errors.iter().any(|e| e.contains("could not check")),
                "and it must be the GUARD's `Unknown` wording, not an incidental read failure from \
                 somewhere else in the run — those are the same red for opposite reasons and only this \
                 string tells them apart. Got: {:?}",
                report.errors
            );
        }
    }

    /// **CPE-1759: the unix mode bits `zip::ZipArchive::extract` restored, restored by our loop too.**
    ///
    /// This is the second of the two capabilities CPE-1744 measured as blocking the one-shot/streamed
    /// merge (`create_entry_symlink` covers the first). Routing `extract_archive`'s zip branch through
    /// the shared loop without this would have silently dropped the executable bit off every binary in
    /// every zip — a downgrade traded for a consistency fix, which is exactly what CPE-1744 declined to
    /// accept.
    ///
    /// `#[cfg(unix)]` because the bits do not exist on Windows, which means **only the Linux and macOS
    /// legs of the CI matrix can confirm it**; a green local Windows run says nothing about this one.
    #[cfg(unix)]
    #[test]
    fn cpe1759_zip_extraction_restores_unix_permission_bits_on_both_paths() {
        use std::os::unix::fs::PermissionsExt;
        for leg in ["one-shot", "streamed"] {
            let d = scratch("cpe1759_modes");
            let ap = d.join("modes.zip");
            {
                let mut w = zip::ZipWriter::new(fs::File::create(&ap).unwrap());
                let exec: zip::write::FileOptions<()> =
                    zip::write::FileOptions::default().unix_permissions(0o755);
                w.start_file("run.sh", exec).unwrap();
                w.write_all(b"#!/bin/sh\n").unwrap();
                let plain: zip::write::FileOptions<()> =
                    zip::write::FileOptions::default().unix_permissions(0o600);
                w.start_file("secret.txt", plain).unwrap();
                w.write_all(b"shh").unwrap();
                w.finish().unwrap();
            }
            let dest = d.join("out");
            if leg == "one-shot" {
                extract_archive(&ap.to_string_lossy(), &dest.to_string_lossy()).unwrap();
            } else {
                extract_archive_streamed(
                    &ap.to_string_lossy(),
                    &dest.to_string_lossy(),
                    &AtomicBool::new(false),
                    |_| {},
                )
                .unwrap();
            }
            for (name, mode) in [("run.sh", 0o755u32), ("secret.txt", 0o600u32)] {
                let got = fs::metadata(dest.join(name)).unwrap().permissions().mode() & 0o777;
                assert_eq!(
                    got, mode,
                    "{leg}: {name} must come out {mode:o}, not {got:o}. Asserting the exact mode rather \
                     than just the executable bit is what catches a pass that restores SOME mode: 0o600 \
                     is the leg that fails if the umask, not the archive, decided the answer"
                );
            }
            let _ = fs::remove_dir_all(&d);
        }
    }

    // -----------------------------------------------------------------------
    // CPE-1746 — 7z, the last extractor that followed a link, on both its call sites
    // -----------------------------------------------------------------------

    /// **Rows 19–20: 7z refuses a link at an entry's name, on BOTH call sites and BOTH link kinds**
    /// (CPE-1746).
    ///
    /// This is the re-pointed `sevenz_extraction_still_writes_through_a_link_until_cpe_1746`. That test
    /// pinned the hazard — `Ok(ArchiveReport { done: 2, errors: [] })` with a victim *outside* the
    /// destination holding `"ARCHIVED A"` — precisely so the guard would announce itself, and it did: it
    /// went red on the victim assertion the moment rows 19–20 landed. Re-pointed rather than deleted, so
    /// the same name-and-place keeps answering the same question with the opposite expectation.
    ///
    /// **Four legs, and each dimension is a separate measured behaviour rather than symmetry for its own
    /// sake:**
    ///
    /// - **Row 19 (`extract_archive`) and row 20 (`extract_archive_streamed`)** are two shipped 7z paths
    ///   with two identical closures. The ticket's checklist originally named only the streamed one; row 19
    ///   is a registered Tauri command in `bindings.gen.ts`, and fixing one alone would have reproduced the
    ///   one-shot/streamed ZIP divergence CPE-1759 tracks.
    /// - **Dangling and live links** fail differently: a dangling link has no bytes to lose, so it only
    ///   proves a file was *created* somewhere unnamed, while a live one is the case that *destroys*
    ///   existing content. A guard can pass one and miss the other (that is what `Ok(true)`-vs-`Err`
    ///   classification decides), so both run.
    ///
    /// **Assertion order is deliberate**: the victim and the link are checked **before** the `Result` is
    /// unwrapped, because the defect this replaces failed by returning `Ok`. Unwrap first and the
    /// assertions naming the damage never run.
    ///
    /// **The recorded message is pinned on `"writes THROUGH it"`, not `is_err()`.** That phrase occurs only
    /// in `fsutil::classify_create_slot`'s *confirmed-link* arm, so it separates our refusal from (a) the
    /// OS's own `Access is denied` on an unprivileged Windows runner staring at a dangling junction —
    /// measured for CPE-1733 — and (b) the `Unknown` arm's "could not check" wording, which must abort
    /// rather than skip.
    #[test]
    fn rows_19_and_20_sevenz_refuse_a_link_at_an_entry_name_and_still_extract_the_rest() {
        // (row, function under test, runs it, does it have somewhere to record the skip?)
        type Run = fn(&Path, &Path) -> Result<Vec<String>, String>;
        let rows: &[(u8, &str, Run, bool)] = &[
            (
                19,
                "extract_7z_safe via one-shot extract_archive",
                |sevenz: &Path, dest: &Path| {
                    extract_archive(&sevenz.to_string_lossy(), &dest.to_string_lossy())
                        .map(|outcome| outcome.report.errors)
                },
                // CPE-1837: `ArchiveExtractOutcome` now carries the report, so this leg records too.
                true,
            ),
            (
                20,
                "extract_7z_stream via extract_archive_streamed",
                |sevenz: &Path, dest: &Path| {
                    let cancel = AtomicBool::new(false);
                    extract_archive_streamed(&sevenz.to_string_lossy(), &dest.to_string_lossy(), &cancel, |_| {})
                        .map(|r| r.errors)
                },
                true,
            ),
        ];

        for (n, label, run, records) in rows {
            for live in [false, true] {
                let kind = if live { "live" } else { "dangling" };
                let d = scratch(&format!("cpe1746_row{n}_{kind}"));
                let sevenz = d.join("in.7z");
                write_7z_fixture(&sevenz, &[("a.txt", b"ARCHIVED A"), ("b.txt", b"ARCHIVED B")]);
                let dest = d.join("out");
                fs::create_dir_all(&dest).unwrap();
                let link = dest.join("a.txt");

                // Stage the link, and decide what "the thing outside the destination is untouched" means
                // for this kind: a live link has bytes that must still read `VICTIM ORIGINAL`; a dangling
                // one has a target that must still not exist at all.
                let victim: std::path::PathBuf = if live {
                    let v = d.join("victim-the-user-never-named.bin");
                    fs::write(&v, b"VICTIM ORIGINAL").unwrap();
                    if !crate::fsutil::require_staged("live_file_symlink", true, stage_live_link(&v, &link)) {
                        crate::skip_notice!(
                            "[CPE-1746] SKIPPED row {n} ({label}) LIVE-link leg: this machine could not \
                             create a file symlink at {}. The dangling leg passes under a guard that is \
                             blind to live links, so nothing covered the case that destroys existing bytes \
                             on this run.",
                            link.display()
                        );
                        let _ = fs::remove_dir_all(&d);
                        continue;
                    }
                    v
                } else {
                    if !crate::fsutil::make_dangling_link(&link) {
                        crate::skip_notice!(
                            "[CPE-1746] SKIPPED row {n} ({label}) DANGLING-link leg: could not stage a link \
                             at {}.",
                            link.display()
                        );
                        let _ = fs::remove_dir_all(&d);
                        continue;
                    }
                    crate::fsutil::dangling_link_target(&link)
                };

                let outcome = run(&sevenz, &dest);

                // The victim FIRST — the bug this replaces returned `Ok`, so anything after an unwrap
                // would never run when it regresses.
                if live {
                    assert_eq!(
                        fs::read(&victim).unwrap(),
                        b"VICTIM ORIGINAL".to_vec(),
                        "row {n} ({label}, live link): the entry's bytes went THROUGH the link and replaced \
                         the contents of {}, a file outside the destination that nobody named. That is the \
                         exact CPE-1746 hazard; if it is back, the guard in the 7z callback is gone \
                         (outcome was {outcome:?})",
                        victim.display()
                    );
                } else {
                    assert!(
                        !victim.exists(),
                        "row {n} ({label}, dangling link): the entry's bytes went THROUGH the link and \
                         CREATED {} — a path nobody named (outcome was {outcome:?})",
                        victim.display()
                    );
                }
                assert!(
                    fs::symlink_metadata(&link).map(|m| m.file_type().is_symlink()).unwrap_or(false),
                    "row {n} ({label}, {kind} link): the link must survive untouched — a guard that deleted \
                     it and then skipped (tar's behaviour) would pass the assertion above \
                     (outcome was {outcome:?})"
                );
                let errors = outcome.unwrap_or_else(|e| {
                    panic!(
                        "row {n} ({label}, {kind} link): a confirmed link SKIPS one entry, it does not abort \
                         the run — aborting is the `Unknown`/unreadable-slot arm only. Got: {e}"
                    )
                });
                if *records {
                    assert!(
                        errors.iter().any(|e| e.starts_with("a.txt: ") && e.contains("writes THROUGH it")),
                        "row {n} ({label}, {kind} link): the skip must be RECORDED against the entry, in OUR \
                         link wording rather than whatever the OS happened to say — an `is_err()`/any-error \
                         check here would stay green through the `Access is denied` an unprivileged Windows \
                         runner produces on a dangling junction all by itself. Got {errors:?}"
                    );
                }
                assert_eq!(
                    fs::read(dest.join("b.txt")).unwrap(),
                    b"ARCHIVED B".to_vec(),
                    "row {n} ({label}, {kind} link): a skip must cost ONE entry — the rest of the archive \
                     still extracts. b.txt missing means the run was abandoned, which is the one-shot ZIP \
                     behaviour CPE-1759 tracks aligning, not this one"
                );
                let _ = fs::remove_dir_all(&d);
            }
        }
    }

    /// **Why rows 19–20's abort arm is a captured `Option<String>` and not a `sevenz_rust::Error`**
    /// (CPE-1746) — the measurement behind that choice, pinned so it is checkable rather than asserted.
    ///
    /// The 7z callbacks must return `Result<bool, sevenz_rust::Error>`, so surfacing
    /// `EntrySlotAction::Abort` as an error of that type is the obvious shape. It is the wrong one:
    /// `sevenz-rust` 0.6.1 implements `Display for Error` as `Debug::fmt`, so the refusal comes back out of
    /// the call sites' `.map_err(|e| e.to_string())` wrapped in `Other(..)` with its quotes and every
    /// Windows path separator escaped. Rows 6–16 all show that wording verbatim.
    ///
    /// This pins the property, not the exact rendering: the message must **not** survive the round trip
    /// intact. If a future `sevenz-rust` gives `Error` a real `Display`, this goes red — and that is the
    /// signal that the captured-message indirection at both call sites can be simplified away.
    #[test]
    fn sevenz_error_display_would_mangle_our_refusal_wording() {
        let target = Path::new("out").join("a.txt");
        let msg = crate::fsutil::create_slot_link_from_stat(&Ok(true), &target);
        let msg = match msg {
            crate::fsutil::CreateSlotLink::Link(m) => m,
            other => panic!("a stat of Ok(true) is a confirmed link, not {other:?}"),
        };

        let round_tripped = sevenz_rust::Error::other(msg.clone()).to_string();

        assert_ne!(
            round_tripped, msg,
            "sevenz_rust::Error no longer mangles the message it is handed. Rows 19–20 carry their abort \
             message out of the callback in an `Option<String>` purely because it did — see \
             `sevenz_entry_slot_action`. If this crate's Display is now faithful, that indirection can go."
        );
        assert!(
            round_tripped.starts_with("Other(\""),
            "the recorded mangling is `Debug::fmt` (sevenz-rust 0.6.1 src/error.rs:74-78), which debug-quotes \
             the whole refusal. Got: {round_tripped}"
        );
        assert!(
            !round_tripped.contains(&format!("\"{}\"", target.display())),
            "and the quoted path fsutil puts in the message does not survive it — that is the user-visible \
             cost, not a cosmetic one. Got: {round_tripped}"
        );
    }

    // -----------------------------------------------------------------------
    // CPE-1744 — the intermediate-directory escape, and two wording defects
    // -----------------------------------------------------------------------

    /// The phrase [`escaped_dest_message`] puts in every containment refusal, asserted rather than
    /// re-typed at each site. It is deliberately **not** a substring of the leaf-link wording
    /// (`"is a link, and creating a file at a link's name writes THROUGH it"`), so a test that reds here
    /// has caught the containment guard specifically and not the CPE-1733 one standing in for it.
    const CONTAINMENT_MARKER: &str = "stay inside the extraction folder";

    /// Stage the exact shape CPE-1744 closed: a **live directory link** at `dest/sub` pointing at a folder
    /// outside `dest`, and an archive whose entries are addressed *through* it.
    ///
    /// Returns `(scratch, archive, dest, outside)`, or `None` after a loud skip when this machine cannot
    /// make a directory link. `fsutil::make_dir_link` already routes through `require_staged`, so a CI
    /// runner that *should* manage it goes red rather than announcing into a green log (CPE-1717) — and on
    /// Windows the privilege-free **junction** fallback is the realistic case anyway, which is why this
    /// hazard is an ordinary user's folder rather than an attacker scenario.
    ///
    /// `ok.txt` is the bystander: it is what separates "skipped the escaping entry" from "abandoned the
    /// run", the same job `b.txt` does for the CPE-1733/1746 legs. `deep_dir` adds a `sub/deeper/`
    /// **directory** entry, which is the only thing row 18's guard can be caught by — with it absent, a
    /// deleted `entry_dir_action` changes nothing observable because `dest/sub` already exists.
    fn stage_intermediate_dir_escape(
        tag: &str,
        kind: &str,
        deep_dir: bool,
    ) -> Option<(crate::fsutil::ScratchDir, PathBuf, PathBuf, PathBuf)> {
        let d = scratch(tag);
        let stage = d.join("stage");
        fs::create_dir_all(stage.join("sub")).unwrap();
        fs::write(stage.join("sub").join("leaf.txt"), b"ARCHIVED LEAF").unwrap();
        if deep_dir {
            fs::create_dir_all(stage.join("sub").join("deeper")).unwrap();
        }
        fs::write(stage.join("ok.txt"), b"ARCHIVED OK").unwrap();
        let sub = stage.join("sub").to_string_lossy().to_string();
        let ok = stage.join("ok.txt").to_string_lossy().to_string();
        let archive = match kind {
            "zip" => {
                let p = d.join("in.zip");
                compress_to_zip(&[sub, ok], &p.to_string_lossy()).unwrap();
                p
            }
            "encrypted" => {
                let p = d.join("in.zip");
                compress_to_zip_encrypted(&[sub, ok], &p.to_string_lossy(), "hunter2").unwrap();
                p
            }
            _ => {
                let p = d.join("in.7z");
                write_7z_fixture(&p, &[("sub/leaf.txt", b"ARCHIVED LEAF"), ("ok.txt", b"ARCHIVED OK")]);
                p
            }
        };
        let outside = d.join("outside-the-destination-the-user-chose");
        fs::create_dir_all(&outside).unwrap();
        let dest = d.join("out");
        fs::create_dir_all(&dest).unwrap();
        let link = dest.join("sub");
        if !crate::fsutil::make_dir_link(&outside, &link) {
            crate::skip_notice!(
                "[CPE-1744] SKIPPED: this machine could not stage a directory link at {}. The \
                 intermediate-directory escape — the largest of the three gaps CPE-1733 recorded — was NOT \
                 covered on this run.",
                link.display()
            );
            let _ = fs::remove_dir_all(&d);
            return None;
        }
        Some((d, archive, dest, outside))
    }

    /// **CPE-1913's harm test for the zip leg: a junction pointing back INSIDE the extraction folder.**
    ///
    /// Rows 15–20 above stage a directory link at `dest/sub` pointing **outside** the folder the user
    /// chose, which `entry_sink_action`'s `confined_to` has refused since CPE-1744. This stages the
    /// same link pointing at `dest/other` — still inside — and that guard says **yes**, correctly by
    /// its own contract: the write does stay inside the extraction folder. It just does not go where
    /// the archive said. `sub/leaf.txt` ended up as `other/leaf.txt`, `dest/sub` kept whatever it had,
    /// and the report said `done: 1, errors: []`.
    ///
    /// That is CPE-1912's shape at the archive leg, and no path check can see it: both paths resolve
    /// inside the root, no `..`, no absolute path, no race. What refuses it is the per-component walk,
    /// which asks "is this component a real directory" rather than "where does this path end up".
    ///
    /// **Both directions are run**, so the test also covers the outside case the old guard caught — a
    /// conversion that fixed the new shape and lost the old one would red here rather than in a
    /// different file.
    #[test]
    fn cpe_1913_a_junction_inside_the_extraction_folder_never_redirects_an_entry() {
        for point_outside in [true, false] {
            let d = scratch("cpe1913-zip-junction");
            let stage = d.join("stage");
            // **`deeper` is not decoration — CPE-1913 round 2, the Reviewer's finding B.** `sub/` is
            // the only directory entry without it, and `dest/sub` already exists (it IS the junction),
            // so sabotaging `create_dir_beneath` back to `create_dir_all` produced no observable debris
            // and this test stayed green. A directory entry *below* the junction is the one shape that
            // makes a by-path `create_dir_all` build something on the far side of it — exactly what
            // `row18` demonstrates for the outside-the-root case, staged here for the inside-the-root
            // case that is this ticket's whole thesis.
            fs::create_dir_all(stage.join("sub").join("deeper")).unwrap();
            fs::write(stage.join("sub").join("leaf.txt"), b"ARCHIVED LEAF").unwrap();
            fs::write(stage.join("ok.txt"), b"ARCHIVED OK").unwrap();
            let archive = d.join("in.zip");
            compress_to_zip(
                &[
                    stage.join("sub").to_string_lossy().to_string(),
                    stage.join("ok.txt").to_string_lossy().to_string(),
                ],
                &archive.to_string_lossy(),
            )
            .unwrap();

            let dest = d.join("out");
            fs::create_dir_all(&dest).unwrap();
            let elsewhere = if point_outside { d.join("outside") } else { dest.join("other") };
            fs::create_dir_all(&elsewhere).unwrap();
            if !crate::fsutil::make_dir_link(&elsewhere, &dest.join("sub")) {
                crate::skip_notice!(
                    "SKIPPING cpe_1913_a_junction_inside_the_extraction_folder_never_redirects_an_entry: \
                     could not stage a directory link. NOTHING on this run covered the archive leg's \
                     redirected-component hole"
                );
                let _ = fs::remove_dir_all(&d);
                return;
            }
            // Liveness: the fixture must really redirect, or the test certifies nothing.
            fs::write(dest.join("sub").join("liveness.txt"), b"through").unwrap();
            assert_eq!(
                fs::read(elsewhere.join("liveness.txt")).ok().as_deref(),
                Some(&b"through"[..]),
                "fixture is inert: the junction at dest/sub does not redirect \
                 (point_outside={point_outside})"
            );
            fs::remove_file(dest.join("sub").join("liveness.txt")).unwrap();

            let cancel = AtomicBool::new(false);
            let outcome = extract_archive_streamed(
                &archive.to_string_lossy(),
                &dest.to_string_lossy(),
                &cancel,
                |_| {},
            );

            // HARM FIRST, off the filesystem — every defect in this family failed by returning `Ok`.
            assert!(
                !elsewhere.join("leaf.txt").exists(),
                "HARM: the extraction wrote the archive entry's bytes through a junction at dest/sub \
                 into {}, which the archive never named (point_outside={point_outside})",
                elsewhere.display()
            );
            // **The DIRECTORY entry's own harm, which the file assertion above cannot see** (CPE-1913
            // round 2, finding B). `create_dir_all` is not destructive, so a redirected directory entry
            // writes no bytes — it silently builds the archive's tree shape somewhere the user never
            // named, and the deeper the archive nests the more of it goes out there. This is the
            // assertion that reddens when `create_dir_beneath` alone is sabotaged; without it the
            // directory guard was only ever proven by the file guard standing next to it.
            assert!(
                !elsewhere.join("deeper").exists(),
                "HARM: the extraction created the archive's `sub/deeper` directory through a junction \
                 at dest/sub, inside {}, which the archive never named (point_outside={point_outside})",
                elsewhere.display()
            );
            let report = outcome.expect("an escaping entry SKIPS; the rest of the archive still extracts");
            assert!(
                report.errors.iter().any(|e| {
                    e.starts_with("sub/leaf.txt: ")
                        && e.contains("is a link (a symlink, junction or other reparse point)")
                }),
                "the refusal must reach the report and name the redirecting COMPONENT as a link \
                 (point_outside={point_outside}): {report:?}"
            );
            // A skip costs one entry, not the archive.
            assert_eq!(
                fs::read(dest.join("ok.txt")).ok().as_deref(),
                Some(&b"ARCHIVED OK"[..]),
                "the bystander entry must still extract: {report:?}"
            );
            let _ = fs::remove_dir_all(&d);
        }
    }

    /// **Rows 15/16/19/20: an entry addressed through a symlinked INTERMEDIATE directory is refused**
    /// (CPE-1744) — the gap CPE-1733 measured, recorded as `LEAF ONLY`, and scoped out.
    ///
    /// Measured before the fix, one entry named `sub/leaf.txt` with `dest/sub` a live directory link:
    /// **five of the seven shipping extraction paths returned `Ok` with the bytes outside the folder the
    /// user chose, and not one of them said anything** — `Ok(ArchiveReport { done: 1, failed: 0, errors:
    /// [] })` on the streamed ones. That is a bigger blast radius than the 7z gap CPE-1746 fixed (one
    /// path), and it needs neither `..` nor an absolute path, so no textual check can see it: on Windows a
    /// **junction** stages it with no privilege at all.
    ///
    /// **Assertion order is the lesson this file keeps re-learning**: the bytes outside `dest` are checked
    /// **before** the `Result` is unwrapped, because every defect in this family failed by returning `Ok`.
    /// Unwrap first and the assertion that names the damage never runs.
    ///
    /// **The recorded message is pinned on [`CONTAINMENT_MARKER`], not `is_err()` and not "any error".**
    /// The leaf-link guard produces different wording, so this separates a containment refusal from
    /// CPE-1733's guard happening to catch the same input — and from the OS refusing on its own.
    #[test]
    fn rows_15_to_20_refuse_a_file_entry_addressed_through_a_symlinked_intermediate_directory() {
        // (row, label, archive kind, run, does it record the skip?)
        type Run = fn(&Path, &Path) -> Result<Vec<String>, String>;
        let sinks: &[(u8, &str, &str, Run, bool)] = &[
            (
                16,
                "extract_zip_archive_stream via extract_archive_streamed",
                "zip",
                |a: &Path, dest: &Path| {
                    let cancel = AtomicBool::new(false);
                    extract_archive_streamed(&a.to_string_lossy(), &dest.to_string_lossy(), &cancel, |_| {})
                        .map(|r| r.errors)
                },
                true,
            ),
            (
                15,
                "extract_zip_encrypted",
                "encrypted",
                |a: &Path, dest: &Path| {
                    extract_zip_encrypted(&a.to_string_lossy(), &dest.to_string_lossy(), "hunter2")
                        .map(|outcome| outcome.report.errors)
                },
                // CPE-1837: `ArchiveExtractOutcome` now carries the report, so this leg records too.
                true,
            ),
            (
                16,
                "extract_zip_encrypted_streamed",
                "encrypted",
                |a: &Path, dest: &Path| {
                    let cancel = AtomicBool::new(false);
                    extract_zip_encrypted_streamed(
                        &a.to_string_lossy(),
                        &dest.to_string_lossy(),
                        "hunter2",
                        &cancel,
                        |_| {},
                    )
                    .map(|r| r.errors)
                },
                true,
            ),
            (
                19,
                "extract_7z_safe via one-shot extract_archive",
                "7z",
                |a: &Path, dest: &Path| {
                    extract_archive(&a.to_string_lossy(), &dest.to_string_lossy())
                        .map(|outcome| outcome.report.errors)
                },
                // CPE-1837: `ArchiveExtractOutcome` now carries the report, so this leg records too.
                true,
            ),
            (
                20,
                "extract_7z_stream via extract_archive_streamed",
                "7z",
                |a: &Path, dest: &Path| {
                    let cancel = AtomicBool::new(false);
                    extract_archive_streamed(&a.to_string_lossy(), &dest.to_string_lossy(), &cancel, |_| {})
                        .map(|r| r.errors)
                },
                true,
            ),
        ];

        for (n, label, kind, run, records) in sinks {
            let Some((d, archive, dest, outside)) =
                stage_intermediate_dir_escape(&format!("cpe1744_row{n}_{kind}"), kind, false)
            else {
                // CPE-1809: `continue`, not `return` — each sink stages its own directory link fresh, so
                // a failure on one row must not silently abandon the rest of the table.
                continue;
            };

            let outcome = run(&archive, &dest);

            // FIRST — the bug returned `Ok`, so anything after an unwrap would never run when it regresses.
            assert!(
                !outside.join("leaf.txt").exists(),
                "row {n} ({label}): the entry's bytes landed at {}, OUTSIDE the folder the user chose. \
                 `dest/sub` is a directory link, so the leaf never existed and the leaf-only link guard saw \
                 nothing to refuse — that is exactly the CPE-1744 escape. If this is back, the per-component \
                 containment check in `entry_sink_action` is gone (outcome was {outcome:?})",
                outside.join("leaf.txt").display()
            );
            assert!(
                fs::symlink_metadata(dest.join("sub")).map(|m| m.file_type().is_symlink()).unwrap_or(false),
                "row {n} ({label}): the user's directory link must survive untouched — a guard that deleted \
                 it and then wrote would pass the assertion above (outcome was {outcome:?})"
            );

            let errors = outcome.unwrap_or_else(|e| {
                panic!(
                    "row {n} ({label}): an escaping entry SKIPS, it does not abort the run — aborting is the \
                     unreadable-slot arm only, and it would also mean the entries that were fine are lost. \
                     Got: {e}"
                )
            });
            if *records {
                // **Two markers, because the zip legs answer this by handle now and the 7z legs still
                // answer it by path (CPE-1913).** A zip or encrypted-zip entry is opened one component
                // at a time against the extraction folder's handle, so the refusal names the offending
                // component and says it is a **link** — strictly more than the old sentence, which
                // could only say the path "could not be shown to stay inside" the folder, because a
                // path resolution is all it had. The 7z legs still go through
                // `entry_sink_action`/`escaped_dest_message` and keep the original wording; when they
                // are converted, this splits back into one marker.
                //
                // Neither marker is shared boilerplate: `WHY_LINK`'s phrasing is deliberately absent
                // from the refusal tail every `open_beneath` message carries (CPE-1896 round 4 —
                // `"is a link"` used to appear in the boilerplate and two tests asserting it proved
                // nothing), and `CONTAINMENT_MARKER` is deliberately not a substring of the leaf-link
                // wording. So each still identifies the guard that fired, not just "something failed".
                let marker: &str = if *kind == "7z" {
                    CONTAINMENT_MARKER
                } else {
                    "is a link (a symlink, junction or other reparse point)"
                };
                assert!(
                    errors.iter().any(|e| e.starts_with("sub/leaf.txt: ") && e.contains(marker)),
                    "row {n} ({label}): the skip must be RECORDED against the entry, in OUR containment \
                     wording. Neither half is decorative: without the `sub/leaf.txt: ` prefix this passes on \
                     a note about some other entry, and without {marker:?} it passes on the \
                     leaf-link refusal or on whatever the OS happened to say. Got {errors:?}"
                );
            }
            assert_eq!(
                fs::read(dest.join("ok.txt")).unwrap(),
                b"ARCHIVED OK".to_vec(),
                "row {n} ({label}): a skip must cost ONE entry — the rest of the archive still extracts. \
                 ok.txt missing means the run was abandoned, which is the one-shot-ZIP/tar behaviour \
                 CPE-1759 tracks, not this one"
            );
            let _ = fs::remove_dir_all(&d);
        }
    }

    /// **Row 18: a DIRECTORY entry that would be created outside the extraction folder is refused too**
    /// (CPE-1744).
    ///
    /// A separate leg from the file one above, and not symmetry for its own sake: with only the file guard
    /// in place, `create_dir_all` still walks through `dest/sub` and creates `sub/deeper` **out in the
    /// escape target** before anything refuses. Nothing lands *in* it — `create_dir_all` is not
    /// destructive (CPE-1729) — but directories appear in a folder the user never named, and the deeper an
    /// archive nests the more of its tree gets built out there. Deleting `entry_dir_action` reds this leg
    /// and leaves the file leg above green, which is what makes them two tests rather than two asserts.
    ///
    /// **ZIP only.** This crate's 7z fixture writer (`write_7z_fixture`) emits file entries only, so a 7z
    /// directory entry cannot be staged here; rows 19–20 route theirs through the *same*
    /// `entry_dir_action` via `sevenz_entry_slot_action`, so the decision is covered — the 7z *plumbing*
    /// into it is not, and that is stated rather than implied.
    #[test]
    fn row18_refuses_a_directory_entry_that_would_be_created_outside_the_extraction_folder() {
        type Run = fn(&Path, &Path) -> Result<(), String>;
        let sinks: &[(u8, &str, &str, Run)] = &[
            (18, "extract_zip_archive_stream", "zip", |a: &Path, dest: &Path| {
                let cancel = AtomicBool::new(false);
                extract_archive_streamed(&a.to_string_lossy(), &dest.to_string_lossy(), &cancel, |_| {}).map(|_| ())
            }),
            (18, "extract_zip_encrypted", "encrypted", |a: &Path, dest: &Path| {
                extract_zip_encrypted(&a.to_string_lossy(), &dest.to_string_lossy(), "hunter2").map(|_| ())
            }),
        ];

        for (n, label, kind, run) in sinks {
            let Some((d, archive, dest, outside)) =
                stage_intermediate_dir_escape(&format!("cpe1744_row{n}_dir_{kind}"), kind, true)
            else {
                // CPE-1809: `continue`, not `return` — each sink stages its own directory link fresh, so
                // a failure on one row must not silently abandon the rest of the table.
                continue;
            };

            let outcome = run(&archive, &dest);

            assert!(
                !outside.join("deeper").exists(),
                "row {n} ({label}): `create_dir_all` built {} OUTSIDE the folder the user chose. A live \
                 directory link REDIRECTS even though it does not destroy (CPE-1729), and the archive's own \
                 folder names are what it redirects (outcome was {outcome:?})",
                outside.join("deeper").display()
            );
            assert!(
                fs::symlink_metadata(dest.join("sub")).map(|m| m.file_type().is_symlink()).unwrap_or(false),
                "row {n} ({label}): the user's directory link must survive untouched (outcome was {outcome:?})"
            );
            outcome.unwrap_or_else(|e| panic!("row {n} ({label}): an escaping entry skips, it does not abort: {e}"));
            assert_eq!(
                fs::read(dest.join("ok.txt")).unwrap(),
                b"ARCHIVED OK".to_vec(),
                "row {n} ({label}): the rest of the archive still extracts"
            );
            let _ = fs::remove_dir_all(&d);
        }
    }

    /// **Row 17: a DANGLING link at the extraction destination is reported as a link, not as a file that
    /// already exists** (CPE-1744, the wording half).
    ///
    /// `create_dir_all` on a dangling link fails, and takes the whole extraction with it. Measured:
    /// Windows `Err("Cannot create a file when that file already exists. (os error 183)")`, Linux
    /// `Err("File exists (os error 17)")`. Neither names the folder, and both send the user to delete a
    /// file that **does not exist** — what exists at that name is the link. That is the identical defect
    /// `fsutil::create_slot_refusal`'s doc calls out and that row 7 got a guard for; CPE-1733 left it
    /// because the *live*-link case must keep working, which is the next test.
    ///
    /// Pinned on our own phrasing rather than `is_err()`: the call failed before this change too, so an
    /// `is_err()` leg is green with the fix reverted and proves nothing.
    #[test]
    fn row17_a_dangling_link_at_the_extraction_destination_is_reported_as_a_link() {
        type Run = fn(&Path, &Path) -> Result<(), String>;
        let legs: &[(&str, Run)] = &[
            ("extract_archive", |zip: &Path, dest: &Path| {
                extract_archive(&zip.to_string_lossy(), &dest.to_string_lossy()).map(|_| ())
            }),
            ("extract_archive_streamed", |zip: &Path, dest: &Path| {
                let cancel = AtomicBool::new(false);
                extract_archive_streamed(&zip.to_string_lossy(), &dest.to_string_lossy(), &cancel, |_| {}).map(|_| ())
            }),
            ("extract_zip_encrypted", |zip: &Path, dest: &Path| {
                extract_zip_encrypted(&zip.to_string_lossy(), &dest.to_string_lossy(), "hunter2").map(|_| ())
            }),
            ("extract_zip_encrypted_streamed", |zip: &Path, dest: &Path| {
                let cancel = AtomicBool::new(false);
                extract_zip_encrypted_streamed(
                    &zip.to_string_lossy(),
                    &dest.to_string_lossy(),
                    "hunter2",
                    &cancel,
                    |_| {},
                )
                .map(|_| ())
            }),
        ];

        for (label, run) in legs {
            let d = scratch("cpe1744_row17_dangling");
            // One archive serves both the plain and the encrypted legs: the encrypted extractors read a
            // plain zip fine (the password is only consulted per entry), and the run never reaches an entry.
            let zip = d.join("in.zip");
            compress_to_zip_encrypted(&two_source_files(&d), &zip.to_string_lossy(), "hunter2").unwrap();
            let dest = d.join("dangling-destination");
            if !crate::fsutil::make_dangling_link(&dest) {
                crate::skip_notice!(
                    "[CPE-1744] SKIPPED row 17's dangling-destination leg ({label}): could not stage a link \
                     at {}. The misleading-wording fix was NOT covered on this run.",
                    dest.display()
                );
                let _ = fs::remove_dir_all(&d);
                // CPE-1809: `continue`, not `return` — all four legs stage independently, so a failure on
                // one must not silently abandon the other three.
                continue;
            }
            let target = crate::fsutil::dangling_link_target(&dest);

            let outcome = run(&zip, &dest);

            assert!(
                !target.exists(),
                "row 17 ({label}): the extraction created the link's target at {} — this fix is wording \
                 only and must not have changed where anything is written (outcome was {outcome:?})",
                target.display()
            );
            let err = outcome.expect_err("row 17: `create_dir_all` through a dangling link fails; that is unchanged");
            assert!(
                err.contains("is a link") && err.contains("leads nowhere"),
                "row 17 ({label}): the failure must say a LINK is in the way and that it leads nowhere. \
                 `is_err()` alone proves nothing — the call already failed before this change, with the OS's \
                 \"already exists\" wording that sends the user to delete a file that is not there. Got: {err}"
            );
            assert!(
                err.contains(&dest.display().to_string()),
                "row 17 ({label}): and it must NAME the folder. Not naming it is half the defect — the OS \
                 message names nothing at all. Got: {err}"
            );
            let _ = fs::remove_dir_all(&d);
        }
    }

    /// **…and row 17's LIVE-link case is untouched** — the constraint CPE-1733 named when it declined to
    /// guard this site, and the one an over-eager fix to the test above would break.
    ///
    /// `dest` is a folder the user **pointed at**, not a name being claimed, so following a link there is
    /// the right answer (`fsutil`'s claiming-vs-editing rule). Extracting into a folder reached through a
    /// shortcut must keep working, and the entries must land at the link's target. Without this leg,
    /// "refuse any link at `dest`" passes the whole suite while breaking a legitimate workflow.
    #[test]
    fn row17_a_live_link_at_the_extraction_destination_is_still_followed() {
        let d = scratch("cpe1744_row17_live");
        let zip = d.join("in.zip");
        compress_to_zip(&two_source_files(&d), &zip.to_string_lossy()).unwrap();
        let real = d.join("the-folder-the-user-pointed-at");
        fs::create_dir_all(&real).unwrap();
        let shortcut = d.join("shortcut-to-it");
        if !crate::fsutil::make_dir_link(&real, &shortcut) {
            crate::skip_notice!(
                "[CPE-1744] SKIPPED row 17's LIVE-link leg: could not stage a directory link at {}. Nothing \
                 checked that the wording fix left the follow-the-link case alone on this run.",
                shortcut.display()
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }

        let outcome = extract_archive(&zip.to_string_lossy(), &shortcut.to_string_lossy());

        assert_eq!(
            fs::read(real.join("a.txt")).unwrap(),
            b"ARCHIVED A".to_vec(),
            "row 17: extracting into a folder the user reached through a shortcut must still land the \
             entries at the shortcut's target. Refusing here would be `create_slot`'s claiming rule applied \
             to an editing case — the exact over-fix CPE-1733 declined (outcome was {outcome:?})"
        );
        outcome.expect("row 17: a live link at the destination is followed, not refused");
        let _ = fs::remove_dir_all(&d);
    }

    /// **Row 7's other half: `create_empty_zip` onto a plain existing file names the path** (CPE-1744).
    ///
    /// Row 7's CPE-1733 guard reworded only the *link* case. Onto an ordinary file this still returned the
    /// raw `Err("The file exists. (os error 80)")` — measured — which names neither the path nor which of
    /// the two files is meant, the same defect one step over from the one that guard was filed about.
    ///
    /// The existing file's bytes are asserted **before** the `Result`, because "atomic, never clobbers" is
    /// the actual contract and a message change must not have quietly become a behaviour change.
    #[test]
    fn row7_create_empty_zip_names_the_path_when_a_plain_file_already_holds_the_name() {
        let d = scratch("cpe1744_row7_occupied");
        let taken = d.join("New Compressed (zipped) Folder.zip");
        fs::write(&taken, b"NOT AN ARCHIVE, BUT MINE").unwrap();

        let outcome = create_empty_zip(&taken.to_string_lossy());

        assert_eq!(
            fs::read(&taken).unwrap(),
            b"NOT AN ARCHIVE, BUT MINE".to_vec(),
            "row 7: `create_new` is what makes this atomic and it must still refuse to clobber — this \
             ticket changed the message, not the belt (outcome was {outcome:?})"
        );
        let err = outcome.expect_err("row 7: an occupied name must still refuse");
        assert!(
            err.contains(&taken.display().to_string()),
            "row 7: the refusal must NAME the path. \"The file exists. (os error 80)\" — what this said \
             before — names neither the path nor which of the two files is meant. Got: {err}"
        );
        assert!(
            err.contains("already exists at that name"),
            "row 7: and it must be OUR sentence rather than the OS's. An `is_err()`/any-message check here \
             stays green through a straight revert, because the call already failed. Got: {err}"
        );
        assert!(
            !err.contains("is a link"),
            "row 7: an ordinary file must not be reported as a link — that is the mirror of the defect the \
             link guard exists for. Got: {err}"
        );
        let _ = fs::remove_dir_all(&d);
    }

    // -----------------------------------------------------------------------
    // CPE-1773 / CPE-1774 / CPE-1775 — the tar name guard, link targets, and the visible refusal
    // -----------------------------------------------------------------------

    /// A tar holding one regular-file entry with an arbitrary raw `name`.
    ///
    /// Hand-built through `tar::Header` rather than `Builder::append_path`, for
    /// `craft_zip_with_entry_name`'s reason: the archive *writer* is not where the hazard lives, and
    /// several of these names (`nul`, `con`, `x.`) cannot exist as real files on Windows to be packed
    /// from. The extractor is what is under test.
    fn craft_tar_with_entry_name(name: &str, data: &[u8]) -> Vec<u8> {
        let mut b = tar::Builder::new(Vec::new());
        let mut h = tar::Header::new_gnu();
        h.set_size(data.len() as u64);
        h.set_mode(0o644);
        h.set_cksum();
        b.append_data(&mut h, name, data).unwrap();
        // An innocent bystander, so every leg can tell "skipped one entry" from "abandoned the archive".
        let mut h2 = tar::Header::new_gnu();
        h2.set_size(8);
        h2.set_mode(0o644);
        h2.set_cksum();
        b.append_data(&mut h2, "ok.txt", &b"ORDINARY"[..]).unwrap();
        b.into_inner().unwrap()
    }

    /// A tar holding a **symlink** entry `name` -> `target`, plus the same bystander.
    fn craft_tar_with_symlink(name: &str, target: &str) -> Vec<u8> {
        let mut b = tar::Builder::new(Vec::new());
        let mut h = tar::Header::new_gnu();
        h.set_size(0);
        h.set_mode(0o777);
        h.set_entry_type(tar::EntryType::Symlink);
        h.set_link_name(target).unwrap();
        h.set_cksum();
        b.append_data(&mut h, name, std::io::empty()).unwrap();
        let mut h2 = tar::Header::new_gnu();
        h2.set_size(8);
        h2.set_mode(0o644);
        h2.set_cksum();
        b.append_data(&mut h2, "ok.txt", &b"ORDINARY"[..]).unwrap();
        b.into_inner().unwrap()
    }

    /// A zip holding a **symlink** entry `name` -> `target`, plus the same bystander.
    fn craft_zip_with_symlink(name: &str, target: &str) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let link: zip::write::FileOptions<()> =
            zip::write::FileOptions::default().unix_permissions(0o120_777);
        w.add_symlink(name, target, link).unwrap();
        let plain: zip::write::FileOptions<()> = zip::write::FileOptions::default();
        w.start_file("ok.txt", plain).unwrap();
        w.write_all(b"ORDINARY").unwrap();
        w.finish().unwrap().into_inner()
    }

    /// AES-256-encrypted twin of [`craft_zip_with_symlink`], for CPE-1807: `extract_zip_encrypted` used
    /// to be a fourth, unmerged loop with no symlink-entry handling at all, so this is what lets a test
    /// tell "the guard fires on a real ciphertext entry" apart from "the guard fires on plaintext, and
    /// nothing checks the encrypted path specifically".
    fn craft_zip_with_symlink_encrypted(name: &str, target: &str, password: &str) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let link: zip::write::FileOptions<()> = zip::write::FileOptions::default()
            .unix_permissions(0o120_777)
            .compression_method(zip::CompressionMethod::Deflated)
            .with_aes_encryption(zip::AesMode::Aes256, password);
        w.add_symlink(name, target, link).unwrap();
        let plain: zip::write::FileOptions<()> = zip::write::FileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .with_aes_encryption(zip::AesMode::Aes256, password);
        w.start_file("ok.txt", plain).unwrap();
        w.write_all(b"ORDINARY").unwrap();
        w.finish().unwrap().into_inner()
    }

    /// The six names CPE-1758 taught `entry_name_is_safe` to refuse, plus the payload that makes the
    /// first one visible. `x.` and `x ` and the device names are Windows-only shapes — `entry_name_is_safe`
    /// is the identity on other platforms for those — so the expectation is computed from the guard
    /// itself rather than hard-coded per name, which is what lets one table serve both platforms AND
    /// makes the tar-vs-zip agreement assertion below meaningful rather than circular (the two formats
    /// are compared to **each other**, not to the guard).
    const CPE_1773_NAMES: &[&str] = &["file:stream", "ok/file:stream", "..evil", "con", "nul", "x.", "x "];

    /// Does the extraction folder actually **list** a file for entry `name`?
    ///
    /// `Path::exists`/`fs::read` are the wrong question for this family and each is wrong differently:
    /// on Windows `fs::read("<dir>/nul")` opens the NUL **device** and returns `Ok(vec![])` whether or
    /// not anything was written, and `fs::read("<dir>/file:stream")` opens an alternate data stream that
    /// no directory listing shows. What the user meets is the listing, so that is what this asks — with
    /// the stream case checked separately at the call site, since a listing can never reveal one.
    fn dest_lists(dest: &Path, name: &str) -> bool {
        let out = dest.join(name);
        let (Some(parent), Some(leaf)) = (out.parent(), out.file_name()) else { return false };
        let Ok(rd) = fs::read_dir(parent) else { return false };
        rd.flatten().any(|e| e.file_name() == leaf)
    }

    /// **CPE-1773's core: tar and zip must answer identically, and the harm is asserted before the
    /// `Result` is unwrapped.**
    ///
    /// Measured on `main` through the real streamed path, i.e. what right-click → Extract did:
    ///
    /// ```text
    /// [M1 tar      STREAMED] Ok(done:1, errors:[])   ADS bytes = Some("ADS PAYLOAD 24 bytes ok!!")
    /// [M1 tar.gz   STREAMED] Ok(done:1, errors:[])   ADS bytes = Some("ADS PAYLOAD 24 bytes ok!!")
    /// [M1 "..evil"/"con"/"x."/"x "] Ok(done:1, errors:[])  — written literally
    /// [M1 "nul"]                    Err("failed to unpack `…\\nul`")  — took the whole archive down
    /// ```
    ///
    /// This family **fails by succeeding**, so every filesystem assertion runs before `expect`.
    #[test]
    fn cpe1773_tar_refuses_the_same_entry_names_as_zip_on_every_tar_flavour() {
        let d = scratch("cpe1773_tar_names");
        let payload = b"ADS PAYLOAD 24 bytes ok!!";

        for (i, name) in CPE_1773_NAMES.iter().enumerate() {
            let expected_refused = !entry_name_is_safe(name);

            // --- the zip answer, which is the reference behaviour the tar sinks must match ---
            let zip_path = d.join(format!("z{i}.zip"));
            fs::write(&zip_path, craft_zip_with_entry_name(name, payload)).unwrap();
            let zip_dest = d.join(format!("zd{i}"));
            let zip_report = extract_archive_streamed(
                &zip_path.to_string_lossy(),
                &zip_dest.to_string_lossy(),
                &AtomicBool::new(false),
                |_| {},
            );

            // --- the three tar flavours ---
            let tar_bytes = craft_tar_with_entry_name(name, payload);
            let flavours: [(&str, Vec<u8>); 3] = [
                ("tar", tar_bytes.clone()),
                ("tar.gz", gzip_bytes(&tar_bytes)),
                ("tgz", gzip_bytes(&tar_bytes)),
            ];
            for (ext, bytes) in flavours {
                let ap = d.join(format!("t{i}.{ext}"));
                fs::write(&ap, &bytes).unwrap();
                let dest = d.join(format!("td{i}_{}", ext.replace('.', "_")));
                fs::create_dir_all(&dest).unwrap();
                // The neighbour whose alternate data stream is where a `file:stream` entry's bytes land
                // on NTFS. Its own length must not move: an ADS write leaves the base file byte-identical.
                let neighbour = dest.join("file");
                fs::write(&neighbour, b"NEIGHBOUR").unwrap();

                let report = extract_archive_streamed(
                    &ap.to_string_lossy(),
                    &dest.to_string_lossy(),
                    &AtomicBool::new(false),
                    |_| {},
                );

                // ---- harm first, Result second ----
                assert_eq!(
                    fs::read(&neighbour).unwrap(),
                    b"NEIGHBOUR".to_vec(),
                    "{ext}/{name:?}: the neighbouring file's own bytes must be untouched"
                );
                if expected_refused {
                    assert!(
                        !dest_lists(&dest, name),
                        "{ext}/{name:?}: the refused entry must not appear in the extraction folder"
                    );
                    if name.contains(':') {
                        // The ADS-specific half. `read_dir` cannot see an alternate data stream at all
                        // — that is exactly why the original bug was invisible — so the only way to ask
                        // whether the payload landed is to open the stream by name.
                        assert!(
                            fs::read(dest.join(name)).is_err(),
                            "{ext}/{name:?}: on NTFS this name reads back as a hidden STREAM of the \
                             neighbouring file. A successful read here IS the bug: the user sees no \
                             file and the archive's bytes are on their disk anyway"
                        );
                    }
                }

                let report = report.expect(
                    "a refused entry must not abort the extraction — on main `nul` did exactly that, \
                     taking every other entry down with it (CPE-1773)",
                );
                assert_eq!(
                    fs::read(dest.join("ok.txt")).unwrap(),
                    b"ORDINARY".to_vec(),
                    "{ext}/{name:?}: the rest of the archive must still extract"
                );

                if expected_refused {
                    assert_eq!(
                        report.skipped, 1,
                        "{ext}/{name:?}: the refusal must be COUNTED (CPE-1775) — a count of 0 is the \
                         success toast that hides it; got {report:?}"
                    );
                    assert!(
                        report.errors.iter().any(|e| e.ends_with(": unsafe entry name, skipped")),
                        "{ext}/{name:?}: and RECORDED with the same words zip uses; got {:?}",
                        report.errors
                    );
                    assert_eq!(report.done, 1, "{ext}/{name:?}: only the bystander was written");
                } else {
                    assert_eq!(
                        report.skipped, 0,
                        "{ext}/{name:?}: a name this platform accepts must extract with NO new noise \
                         (CPE-1775's no-regression leg); got {report:?}"
                    );
                }

                // ---- and the two formats must agree, which is the assertion that stops them drifting ----
                //
                // Compared on the *verdict and its wording*, not on `done`: the zip reference archive
                // carries only the one entry (`craft_zip_with_entry_name` is a hand-built single-entry
                // zip, because the zip WRITER refuses several of these names) while the tar carries the
                // bystander too, so their `done` counts are not the same quantity. Both halves of what
                // the user actually meets — refused or not, and what the report says about it — are.
                let zr = zip_report.as_ref().expect("the zip reference run must succeed");
                assert_eq!(
                    (report.skipped, report.errors.clone()),
                    (zr.skipped, zr.errors.clone()),
                    "{ext}/{name:?}: TAR and ZIP must reach the same verdict for the same entry name, \
                     and SAY the same thing about it. The user does not think in sinks, and a \
                     divergence here is how CPE-1773 happened: zip refused this shape from CPE-1758 \
                     onward while tar wrote it into an alternate data stream. tar={report:?} zip={zr:?}"
                );
            }
        }
        let _ = fs::remove_dir_all(&d);
    }

    /// The other half of CPE-1773: the guard must not eat ordinary archives. `%` is here because
    /// CPE-1758's review caught an over-broad check that refused every `%`-containing name on Windows.
    #[test]
    fn cpe1773_tar_still_extracts_legitimate_entry_names() {
        let d = scratch("cpe1773_tar_ok");
        let names = [
            "a file with spaces.txt",
            "\u{4e2d}\u{6587}\u{540d}\u{79f0}.txt",
            "emoji \u{1f600}.txt",
            "archive.tar.gz.backup.txt",
            "deep/deeper/deepest/leaf.txt",
            "50% off.txt",
            "city=A%2FB.txt",
        ];
        let mut b = tar::Builder::new(Vec::new());
        for n in names {
            let mut h = tar::Header::new_gnu();
            h.set_size(4);
            h.set_mode(0o644);
            h.set_cksum();
            b.append_data(&mut h, n, &b"GOOD"[..]).unwrap();
        }
        let ap = d.join("good.tar");
        fs::write(&ap, b.into_inner().unwrap()).unwrap();
        let dest = d.join("out");

        let report = extract_archive_streamed(
            &ap.to_string_lossy(),
            &dest.to_string_lossy(),
            &AtomicBool::new(false),
            |_| {},
        )
        .expect("a tar of ordinary names must extract");

        for n in names {
            assert_eq!(
                fs::read(dest.join(n)).unwrap_or_default(),
                b"GOOD".to_vec(),
                "{n:?} is an ordinary name and must still extract; report was {report:?}"
            );
        }
        assert_eq!(
            (report.skipped, report.errors.len()),
            (0, 0),
            "no ordinary name may be refused, and an unremarkable extraction must produce NO skip \
             notice at all (CPE-1775: 'an extraction with nothing skipped is unchanged'); got {report:?}"
        );
        let _ = fs::remove_dir_all(&d);
    }

    /// The four escaping link-target shapes CPE-1774 lists, as `(label, target)` built against `outside`
    /// — a victim file that sits beside, not inside, the extraction folder.
    fn cpe1774_escaping_targets(outside: &Path) -> Vec<(&'static str, String)> {
        vec![
            ("plain-parent", format!("..{}victim.txt", std::path::MAIN_SEPARATOR)),
            ("absolute", outside.join("victim.txt").to_string_lossy().to_string()),
            ("dot-chain", "x/../../victim.txt".to_string()),
            ("mixed-separators", "..//..\\victim.txt".to_string()),
        ]
    }

    /// **CPE-1774, the zip half.** The Security Auditor's reproduction, re-run: a zip entry named
    /// `evil_link` (a name our guard accepts, and correctly — it is perfectly ordinary) whose stored
    /// content is a path that leaves the extraction folder. Measured on `main`:
    ///
    /// ```text
    /// [M2 zip ONE-SHOT] symlink_metadata(is_symlink) = Ok(true)
    ///                   read_link                    = Ok("..\\outside_secret.txt")
    ///                   read_to_string(THROUGH it)   = Ok("SECRET")
    /// ```
    ///
    /// A real OS link, in the user's folder, reading a file they never chose.
    ///
    /// **CPE-1759 moved this guard off the one-shot pre-pass and into the shared loop**, so the refusal
    /// is now a counted **skip** on both zip paths instead of an abort on one — hence the streamed leg
    /// (which the pre-pass never ran for at all, since it only sat in `extract_archive`) and the flipped
    /// outcome expectation. The three harm assertions are unchanged: they are what this test is for.
    #[test]
    fn cpe1774_a_zip_symlink_entry_whose_target_escapes_creates_no_link() {
        type Run = fn(&Path, &Path) -> Result<Option<ArchiveReport>, String>;
        let legs: &[(&str, Run)] = &[
            ("one-shot", |ap: &Path, dest: &Path| {
                // CPE-1837: the one-shot path now carries a report too, so this leg checks it like the
                // streamed one instead of opting out with `None`.
                extract_archive(&ap.to_string_lossy(), &dest.to_string_lossy()).map(|o| Some(o.report))
            }),
            ("streamed", |ap: &Path, dest: &Path| {
                extract_archive_streamed(
                    &ap.to_string_lossy(),
                    &dest.to_string_lossy(),
                    &AtomicBool::new(false),
                    |_| {},
                )
                .map(Some)
            }),
        ];

        let d = scratch("cpe1774_zip");
        let outside = d.join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("victim.txt"), b"SECRET").unwrap();

        for (label, target) in cpe1774_escaping_targets(&outside) {
            let ap = d.join(format!("z_{label}.zip"));
            fs::write(&ap, craft_zip_with_symlink("evil_link", &target)).unwrap();

            for (leg, run) in legs {
                let dest = outside.join(format!("dest_{label}_{leg}"));

                let outcome = run(&ap, &dest);

                // ---- the harm, before the Result ----
                let leaf = dest.join("evil_link");
                assert!(
                    !fs::symlink_metadata(&leaf).map(|m| m.file_type().is_symlink()).unwrap_or(false),
                    "{label} {leg}: no LINK may exist at the entry's name. On main this was Ok(true) with \
                     read_link = {:?}",
                    fs::read_link(&leaf)
                );
                assert!(
                    fs::read_to_string(&leaf).unwrap_or_default() != "SECRET",
                    "{label} {leg}: and reading the 'extracted file' must not return the victim's \
                     contents — that is the measurement the auditor took, and it is the one that matters \
                     even if the link's file type ever changes"
                );
                assert_eq!(
                    fs::read_to_string(outside.join("victim.txt")).unwrap(),
                    "SECRET",
                    "{label} {leg}: the victim itself must be untouched"
                );

                let report = outcome.unwrap_or_else(|e| {
                    panic!(
                        "{label} {leg}: a refused link entry is a SKIP on both zip paths as of CPE-1759, \
                         never an abort: {e}"
                    )
                });
                assert_eq!(
                    fs::read(dest.join("ok.txt")).unwrap(),
                    b"ORDINARY".to_vec(),
                    "{label} {leg}: and the rest of the archive still extracts — `ok.txt` sits AFTER the \
                     poisoned entry in this archive, so its absence is what an abort looks like"
                );
                if let Some(report) = report {
                    assert_eq!(
                        report.skipped, 1,
                        "{label} {leg}: the refusal must be COUNTED (CPE-1775); got {report:?}"
                    );
                    assert!(
                        report.errors.iter().any(|e| {
                            e.starts_with("evil_link: ") && e.contains("outside the extraction folder")
                        }),
                        "{label} {leg}: and record WHICH entry and WHY — an `is_err()`/`is_ok()` check \
                         would stay green through a straight revert of the guard; got {:?}",
                        report.errors
                    );
                }
            }
        }
        let _ = fs::remove_dir_all(&d);
    }

    /// **CPE-1807's leg of the table above: `extract_zip_encrypted` gets the identical containment
    /// guard, and this is what pins that rather than assuming it from the code shape.** Before CPE-1807
    /// this function had no [`link_target_action`] check at all -- a symlink entry was never refused, it
    /// was pushed through `File::create` + `io::copy` like any other entry, so the leaf ended up an
    /// ordinary file holding the escaping TARGET STRING as its content. That content is neither a real
    /// link nor the victim's bytes, so a weaker "no symlink" / "content != victim" pair of assertions
    /// passes on that regressed behaviour too -- the actual discriminator, and the one this test uses, is
    /// whether the leaf exists AT ALL: skipped (merged, correct) leaves nothing there; re-duplicated
    /// (regressed) leaves a text file. This reds on a straight revert of the CPE-1807 merge -- verified by
    /// restoring the deleted loop and re-running this test, which failed on all four target shapes before
    /// this comment was written. Same four escaping-target shapes as the plain-zip table above, run
    /// against a REAL AES-256 entry so the guard is proven to fire on ciphertext, not just on the
    /// unencrypted stand-in the rest of this table uses.
    #[test]
    fn cpe1807_encrypted_zip_symlink_entry_whose_target_escapes_creates_no_link() {
        let d = scratch("cpe1807_zip_encrypted");
        let outside = d.join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("victim.txt"), b"SECRET").unwrap();

        for (label, target) in cpe1774_escaping_targets(&outside) {
            let ap = d.join(format!("z_enc_{label}.zip"));
            fs::write(&ap, craft_zip_with_symlink_encrypted("evil_link", &target, "hunter2")).unwrap();
            let dest = outside.join(format!("dest_enc_{label}"));

            let outcome = extract_zip_encrypted(&ap.to_string_lossy(), &dest.to_string_lossy(), "hunter2");

            // ---- the harm, before the Result ----
            let leaf = dest.join("evil_link");
            assert!(
                !leaf.exists(),
                "{label} encrypted: a refused link entry must leave NOTHING at its name. A re-duplicated \
                 loop instead writes an ordinary file here holding the escaping target string -- present, \
                 not a symlink, and not equal to the victim's bytes, so this is the one assertion that \
                 actually distinguishes a skip from that regression."
            );
            assert!(
                !fs::symlink_metadata(&leaf).map(|m| m.file_type().is_symlink()).unwrap_or(false),
                "{label} encrypted: no LINK may exist at the entry's name either, belt-and-suspenders \
                 alongside the existence check above"
            );
            assert_eq!(
                fs::read_to_string(outside.join("victim.txt")).unwrap(),
                "SECRET",
                "{label} encrypted: the victim itself must be untouched"
            );
            let outcome = outcome.unwrap_or_else(|e| {
                panic!("{label} encrypted: a refused link entry is a SKIP, never an abort: {e}")
            });
            // CPE-1837: the one-shot path now records the refusal too.
            assert!(
                outcome.report.errors.iter().any(|e| {
                    e.starts_with("evil_link: ") && e.contains("outside the extraction folder")
                }),
                "{label} encrypted: the skip must be RECORDED on the one-shot path, naming the entry and \
                 why; got {:?}",
                outcome.report.errors
            );
            assert_eq!(
                fs::read(dest.join("ok.txt")).unwrap(),
                b"ORDINARY".to_vec(),
                "{label} encrypted: the rest of the archive still extracts -- ok.txt sits AFTER the \
                 poisoned entry, so its absence is what an abort looks like"
            );
        }
        let _ = fs::remove_dir_all(&d);
    }

    /// **CPE-1774, the tar half — the one the ticket could only reason about from crate source, and the
    /// one that turned out to be live in the shipping UI.** `Entry::unpack_in` canonicalisation-validates
    /// a HARD link's target (`validate_inside_dst`) and calls `symlink(&src, dst)` with the raw bytes for
    /// a SYMLINK. Measured on `main`, both tar paths, `evil_link` -> an absolute target:
    ///
    /// ```text
    /// [M3 tar ONE-SHOT] is_symlink = Ok(true)  read_to_string(THROUGH it) = Ok("SECRET")
    /// [M3 tar STREAMED] is_symlink = Ok(true)  read_to_string(THROUGH it) = Ok("SECRET")
    /// ```
    ///
    /// The streamed line is `start_archive_extract`'s own path, so unlike the zip case this one had a
    /// live caller.
    #[test]
    fn cpe1774_a_tar_symlink_entry_whose_target_escapes_creates_no_link_on_either_path() {
        let d = scratch("cpe1774_tar");
        let outside = d.join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("victim.txt"), b"SECRET").unwrap();

        for (label, target) in cpe1774_escaping_targets(&outside) {
            let ap = d.join(format!("t_{label}.tar"));
            fs::write(&ap, craft_tar_with_symlink("evil_link", &target)).unwrap();

            for streamed in [false, true] {
                let dest = outside.join(format!("dest_{label}_{streamed}"));
                let outcome: Result<Option<ArchiveReport>, String> = if streamed {
                    extract_archive_streamed(
                        &ap.to_string_lossy(),
                        &dest.to_string_lossy(),
                        &AtomicBool::new(false),
                        |_| {},
                    )
                    .map(Some)
                } else {
                    extract_archive(&ap.to_string_lossy(), &dest.to_string_lossy()).map(|_| None)
                };

                // ---- the harm, before the Result ----
                let leaf = dest.join("evil_link");
                assert!(
                    !fs::symlink_metadata(&leaf).map(|m| m.file_type().is_symlink()).unwrap_or(false),
                    "{label} streamed={streamed}: no LINK may exist at the entry's name; read_link was {:?}",
                    fs::read_link(&leaf)
                );
                assert!(
                    fs::read_to_string(&leaf).unwrap_or_default() != "SECRET",
                    "{label} streamed={streamed}: reading the 'extracted file' must not return the \
                     victim's contents"
                );
                assert_eq!(
                    fs::read_to_string(outside.join("victim.txt")).unwrap(),
                    "SECRET",
                    "{label} streamed={streamed}: the victim itself must be untouched"
                );

                let report = outcome.expect(
                    "a refused link entry is a SKIP on the tar paths (their contract is 'extract what is \
                     safe, keep going'), never an abort",
                );
                assert_eq!(
                    fs::read(dest.join("ok.txt")).unwrap(),
                    b"ORDINARY".to_vec(),
                    "{label} streamed={streamed}: the rest of the archive must still extract"
                );
                if let Some(report) = report {
                    assert_eq!(
                        report.skipped, 1,
                        "{label}: the streamed path must COUNT the refusal (CPE-1775); got {report:?}"
                    );
                    assert!(
                        report.errors.iter().any(|e| {
                            e.starts_with("evil_link: ") && e.contains("outside the extraction folder")
                        }),
                        "{label}: and record WHICH entry and WHY — the entry name is ordinary, so the \
                         target is the only thing that tells the user what happened; got {:?}",
                        report.errors
                    );
                }
            }
        }
        let _ = fs::remove_dir_all(&d);
    }

    /// **The other half of CPE-1774, and the leg that stops the two tests above from passing vacuously
    /// on a runner that cannot create symlinks at all.**
    ///
    /// A legitimate relative link pointing *inside* the extraction root must still be materialised. If
    /// this runner cannot make one, `require_staged` decides whether that is a legitimate skip or a red
    /// build (CPE-1717) — which is exactly the question "did the escape tests above verify anything?"
    #[test]
    fn cpe1774_a_legitimate_link_pointing_inside_the_extraction_root_still_extracts() {
        let d = scratch("cpe1774_ok");
        let probe_victim = d.join("probe_victim.txt");
        fs::write(&probe_victim, b"x").unwrap();
        let supported = stage_live_link(&probe_victim, &d.join("probe_link"));
        if !crate::fsutil::require_staged(
            "cpe1774_a_legitimate_link_pointing_inside_the_extraction_root_still_extracts",
            cfg!(any(windows, unix)),
            supported,
        ) {
            return;
        }

        for (label, bytes) in [
            ("zip", craft_zip_with_symlink("good_link", "ok.txt")),
            ("tar", craft_tar_with_symlink("good_link", "ok.txt")),
        ] {
            let ap = d.join(format!("good.{label}"));
            fs::write(&ap, &bytes).unwrap();
            let dest = d.join(format!("out_{label}"));
            extract_archive(&ap.to_string_lossy(), &dest.to_string_lossy())
                .unwrap_or_else(|e| panic!("{label}: a valid archive with an INTERNAL link must extract, \
                                            not be refused by CPE-1774's guard: {e}"));
            let leaf = dest.join("good_link");
            assert!(
                fs::symlink_metadata(&leaf).map(|m| m.file_type().is_symlink()).unwrap_or(false),
                "{label}: a link whose target stays inside the extraction folder must still be created — \
                 refusing every link entry was one of the three policies CPE-1774 offered and it is NOT \
                 the one taken, because source tarballs legitimately carry internal links"
            );
            assert_eq!(
                fs::read_to_string(&leaf).unwrap(),
                "ORDINARY",
                "{label}: and it must resolve to the archive's own file"
            );
        }
        let _ = fs::remove_dir_all(&d);
    }

    /// **CPE-1775's invariant, across every streamed skip path at once.**
    ///
    /// `ArchiveReport::skipped` is what the headline notice reads and `errors` is the reason behind it.
    /// Before this ticket only the second existed, and the frontend read it **only when `failed > 0`** —
    /// so a refused entry produced a plain "1 item extracted" toast with the count quietly one lower.
    /// This asserts the two halves stay in step whichever guard fired, which is what makes
    /// `ArchiveReport::skip` the only way to record a skip.
    #[test]
    fn cpe1775_skipped_counts_every_recorded_skip_on_every_streamed_path() {
        let d = scratch("cpe1775_counts");
        let outside = d.join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("victim.txt"), b"SECRET").unwrap();
        let escaping = format!("..{}victim.txt", std::path::MAIN_SEPARATOR);

        // One archive per guard, so a report that counts the wrong number names which guard drifted.
        let cases: Vec<(&str, String, Vec<u8>)> = vec![
            ("zip unsafe name", "z1.zip".into(), craft_zip_with_entry_name("file:stream", b"X")),
            ("zip traversal", "z2.zip".into(), craft_zip_with_entry_name("../escape.txt", b"X")),
            ("tar unsafe name", "t1.tar".into(), craft_tar_with_entry_name("file:stream", b"X")),
            ("tar escaping link", "t2.tar".into(), craft_tar_with_symlink("evil_link", &escaping)),
        ];

        for (label, file, bytes) in cases {
            let ap = d.join(&file);
            fs::write(&ap, &bytes).unwrap();
            let dest = outside.join(format!("d_{}", file.replace('.', "_")));
            let report = extract_archive_streamed(
                &ap.to_string_lossy(),
                &dest.to_string_lossy(),
                &AtomicBool::new(false),
                |_| {},
            )
            .unwrap_or_else(|e| panic!("{label}: a skip must never abort the run: {e}"));

            assert_eq!(
                report.skipped as usize,
                report.errors.len(),
                "{label}: every recorded reason must be counted and every count must have a reason — \
                 they are two halves of one record and a site that grows one without the other is \
                 exactly the CPE-1775 defect, re-made. Got {report:?}"
            );
            assert_eq!(
                report.skipped, 1,
                "{label}: this archive contains exactly one refusable entry. A 0 here means the guard \
                 stopped firing; a 2 means something ordinary is being refused. Got {report:?}"
            );
            assert_eq!(
                report.failed, 0,
                "{label}: a SKIP is not a FAILURE. Reusing `failed` was the shape CPE-1775 rejected, \
                 because it would misreport a genuine failure and vice versa. Got {report:?}"
            );
        }
        let _ = fs::remove_dir_all(&d);
    }

    // -----------------------------------------------------------------------
    // CPE-1774 round 2 — the escape the Windows-only matrix could not express
    // -----------------------------------------------------------------------

    /// **The containment base must be the directory the extractor really writes into.**
    ///
    /// `link_target_action` derives it from `out.parent()`, so `out` decides how deep the guard believes
    /// the link sits, and every level of disagreement buys the attacker one more `..`. The first version
    /// passed `dest.join(name.replace('\\', "/"))`, which on Unix invents depth that neither extractor
    /// creates: `Path::new("a\\b\\evil")` is ONE `Component::Normal` there, so the link lands directly in
    /// `dest` while the guard measured from `dest/a/b`.
    ///
    /// **Stated plainly: on Windows this test passes with or without the fix**, because `\` and `/` are
    /// both separators there, so `name.replace('\\', "/")` is a no-op and the buggy base and the correct
    /// one are the same path. It is not `#[cfg(unix)]` anyway, for two reasons that are worth the run:
    /// it pins the correct verdicts on both platforms, and its first assertion — that the verdict
    /// **changes** with `out`'s depth — is the mechanism itself, and that one *is* measurable here.
    /// The regression proof for the defect is the `#[cfg(unix)]` end-to-end test below.
    #[test]
    fn cpe1774_the_link_target_base_matches_what_the_extractor_creates() {
        let d = scratch("cpe1774_base");
        let dest = d.join("out");
        fs::create_dir_all(dest.join("a").join("b")).unwrap();
        fs::write(d.join("victim.txt"), b"SECRET").unwrap();

        // ---- the mechanism, demonstrated rather than asserted ----
        // One target, two `out` values differing only in depth, opposite verdicts. This is *why* `out`
        // is load-bearing: get its depth wrong by n and the attacker gets n extra levels of escape for
        // free. Measurable on every platform, because it varies `out` directly instead of relying on a
        // platform's separator rules to vary it.
        let one_up = Path::new("../victim.txt");
        assert!(
            matches!(link_target_action(&dest, &dest.join("evil"), one_up), EntrySlotAction::Skip(_)),
            "from a link directly in `dest`, `../victim.txt` leaves the extraction folder"
        );
        assert_eq!(
            link_target_action(&dest, &dest.join("a").join("b").join("evil"), one_up),
            EntrySlotAction::Write,
            "...and from two directories down, the very same target does not. So an `out` one level too \
             deep silently converts a refusal into a write — which is exactly what the pre-normalised \
             name did on POSIX"
        );

        // How deep the extractor REALLY puts this name, derived by the same component walk `unpack_in`
        // and `simplified_components` do rather than by asserting a per-platform constant: 1 on Unix
        // (`a\b\evil` is a single `Component::Normal`), 3 on Windows (`\` is a separator there). The
        // whole defect was the guard using a number of its own instead of this one.
        const NAME: &str = "a\\b\\evil";
        let depth = Path::new(NAME).components().count();
        assert_eq!(depth, if cfg!(windows) { 3 } else { 1 }, "sanity: the platform split this test rests on");

        // `depth` levels of `..` from the link's real directory lands exactly one level above `dest`.
        let escapes = format!("{}victim.txt", "../".repeat(depth));
        // One fewer stays inside it.
        let stays = format!("{}inside.txt", "../".repeat(depth - 1));

        let refused = tar_entry_refusal(&dest, NAME, TarEntryKind::Symlink(Path::new(&escapes)));
        // `Skip`, specifically — not "anything that isn't Write". An escaping link target is a
        // *refusal*; if it ever arrives as `Abort` the entry stops costing one entry and starts costing
        // the archive, which is the distinction CPE-1759 exists to keep straight.
        let EntrySlotAction::Skip(ref reason) = refused else {
            panic!(
                "a target that escapes from where the extractor ACTUALLY creates the link must be \
                 SKIPPED. Measured on Linux before the fix: this returned Write, because the guard \
                 resolved from an invented `dest/a/b` while `unpack_in` wrote the link straight into \
                 `dest` — every `..` was worth one level more of real escape than the guard accounted \
                 for. target={escapes:?}, got {refused:?}"
            )
        };
        assert!(
            reason.contains("outside the extraction folder"),
            "and refused as a link-target escape, not for some unrelated reason: {refused:?}"
        );
        assert_eq!(
            tar_entry_refusal(&dest, NAME, TarEntryKind::Symlink(Path::new(&stays))),
            EntrySlotAction::Write,
            "...while one level less — still inside the extraction folder from that same real directory \
             — must still be allowed, or this guard has become blanket-refuse-everything and the \
             assertion above proves nothing. target={stays:?}"
        );
        let _ = fs::remove_dir_all(&d);
    }

    /// **The end-to-end escape, through the real streamed path.** `#[cfg(unix)]` because Windows cannot
    /// express it: `\` is a separator there, so both spellings produce the same three components and the
    /// fake-depth trick has no purchase. That is exactly why the Windows-only measurement behind the
    /// first round of this ticket stayed green over a live hole — and why
    /// `cpe1774_a_tar_symlink_entry_whose_target_escapes_creates_no_link_on_either_path` cannot catch it
    /// either: that test varies the TARGET's spelling but always uses the single-component entry name
    /// `evil_link`.
    #[cfg(unix)]
    #[test]
    fn cpe1774_a_backslash_name_cannot_buy_fake_depth_for_its_link_target() {
        let d = scratch("cpe1774_fakedepth");
        let outside = d.join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("victim.txt"), b"SECRET").unwrap();
        let dest = outside.join("dest");

        // Real directory entries, so `dest/a/b` genuinely exists and `confined_to` RESOLVES rather than
        // failing closed for a reason unrelated to the defect.
        let archive = {
            let mut b = tar::Builder::new(Vec::new());
            for dir in ["a/", "a/b/"] {
                let mut h = tar::Header::new_gnu();
                h.set_size(0);
                h.set_mode(0o755);
                h.set_entry_type(tar::EntryType::Directory);
                h.set_cksum();
                b.append_data(&mut h, dir, std::io::empty()).unwrap();
            }
            // One component on Unix, named `a\b\evil`, landing DIRECTLY in `dest`.
            let mut h = tar::Header::new_gnu();
            h.set_size(0);
            h.set_mode(0o777);
            h.set_entry_type(tar::EntryType::Symlink);
            h.set_link_name("../../victim.txt").unwrap();
            h.set_cksum();
            b.append_data(&mut h, "a\\b\\evil", std::io::empty()).unwrap();
            let mut h2 = tar::Header::new_gnu();
            h2.set_size(8);
            h2.set_mode(0o644);
            h2.set_cksum();
            b.append_data(&mut h2, "ok.txt", &b"ORDINARY"[..]).unwrap();
            b.into_inner().unwrap()
        };
        let ap = d.join("fakedepth.tar");
        fs::write(&ap, archive).unwrap();

        let outcome = extract_archive_streamed(
            &ap.to_string_lossy(),
            &dest.to_string_lossy(),
            &AtomicBool::new(false),
            |_| {},
        );

        // ---- harm first ----
        let leaf = dest.join("a\\b\\evil");
        assert!(
            !fs::symlink_metadata(&leaf).map(|m| m.file_type().is_symlink()).unwrap_or(false),
            "no link may be created at the one-component name `a\\b\\evil`; read_link was {:?}",
            fs::read_link(&leaf)
        );
        assert!(
            fs::read_to_string(&leaf).unwrap_or_default() != "SECRET",
            "and reading it must not return the victim two levels above the extraction folder"
        );
        assert_eq!(
            fs::read_to_string(outside.join("victim.txt")).unwrap(),
            "SECRET",
            "the victim itself must be untouched"
        );

        let report = outcome.expect("a refused entry must not abort the streamed tar run");
        assert_eq!(
            report.skipped, 1,
            "the refusal must be counted, not merely happen (CPE-1775); got {report:?}"
        );
        assert!(
            report.errors.iter().any(|e| e.contains("outside the extraction folder")),
            "and recorded as a link-target escape; got {:?}",
            report.errors
        );
        assert_eq!(
            fs::read(dest.join("ok.txt")).unwrap(),
            b"ORDINARY".to_vec(),
            "the rest of the archive must still extract"
        );
        let _ = fs::remove_dir_all(&d);
    }

    /// Review nit 4: a link entry with no readable target used to be waved through, and `unpack_in` then
    /// failed with *"symlink destination is empty"*, which `extract_tar_stream` propagates with `?` —
    /// one crafted entry killing the whole streamed run, the exact failure mode CPE-1773 removed for
    /// `nul`. It is now a counted skip like every other refusal on this path.
    #[test]
    fn cpe1774_a_link_entry_with_no_target_is_skipped_not_fatal() {
        let d = scratch("cpe1774_emptylink");
        let mut b = tar::Builder::new(Vec::new());
        let mut h = tar::Header::new_gnu();
        h.set_size(0);
        h.set_mode(0o777);
        h.set_entry_type(tar::EntryType::Symlink);
        h.set_cksum(); // no link name set at all
        b.append_data(&mut h, "dangling", std::io::empty()).unwrap();
        let mut h2 = tar::Header::new_gnu();
        h2.set_size(8);
        h2.set_mode(0o644);
        h2.set_cksum();
        b.append_data(&mut h2, "ok.txt", &b"ORDINARY"[..]).unwrap();
        let ap = d.join("empty.tar");
        fs::write(&ap, b.into_inner().unwrap()).unwrap();
        let dest = d.join("out");

        let report = extract_archive_streamed(
            &ap.to_string_lossy(),
            &dest.to_string_lossy(),
            &AtomicBool::new(false),
            |_| {},
        )
        .expect("a link entry with no target must not take the whole run down");

        assert_eq!(
            fs::read(dest.join("ok.txt")).unwrap(),
            b"ORDINARY".to_vec(),
            "the rest of the archive must still extract"
        );
        assert_eq!(report.skipped, 1, "the refusal must be counted; got {report:?}");
        assert!(
            report.errors.iter().any(|e| e.contains("no target")),
            "and say what was wrong with it in the user's terms; got {:?}",
            report.errors
        );
        let _ = fs::remove_dir_all(&d);
    }

    /// A tar carrying one **hard-link** entry `hard -> target`, plus the usual `ok.txt` bystander.
    fn craft_tar_with_hard_link(target: &str) -> Vec<u8> {
        let mut b = tar::Builder::new(Vec::new());
        let mut h = tar::Header::new_gnu();
        h.set_size(0);
        h.set_mode(0o644);
        h.set_entry_type(tar::EntryType::Link);
        h.set_link_name(target).unwrap();
        h.set_cksum();
        b.append_data(&mut h, "hard", std::io::empty()).unwrap();
        let mut h2 = tar::Header::new_gnu();
        h2.set_size(8);
        h2.set_mode(0o644);
        h2.set_cksum();
        b.append_data(&mut h2, "ok.txt", &b"ORDINARY"[..]).unwrap();
        b.into_inner().unwrap()
    }

    /// **CPE-1759: an escaping HARD-link entry is a counted skip, not a dead run — and the line between
    /// a refusal and a failure, pinned on both sides.**
    ///
    /// CPE-1774 left hard links to `unpack_in`'s own `validate_inside_dst`, correctly for *safety*
    /// (nothing escapes either way) and silently on the question of *how the refusal arrives*. Measured
    /// on both tar paths before this ticket:
    ///
    /// ```text
    /// [HL escaping streamed=false] outcome=Err("failed to unpack `…\dst\hard`")  ok.txt=false
    /// [HL escaping streamed=true ] outcome=Err("failed to unpack `…\dst\hard`")  ok.txt=false
    /// ```
    ///
    /// One hostile entry, whole archive gone, `ok.txt` included, and a message naming a path and no
    /// reason — the exact shape CPE-1759 removed from the zip branch, hiding in the tar one.
    ///
    /// **The third leg pins the residual rather than leaving it to prose.** A hard link whose target is
    /// simply not there is still a FAILURE — `fs::hard_link` *failing*, not a guard *refusing* — and
    /// since CPE-1935 that means one counted `failed` entry with the rest of the archive extracted,
    /// where it used to end the run (this leg `expect_err`'d until this ticket; see the arm below).
    /// Unpredictable without attempting it, and the same treatment a `File::create` or `io::copy` failure
    /// gets at rows 15/16/19/20. If someone later converts that to a skip, this leg says so out loud
    /// instead of letting "refusals skip, failures are counted against their own entry" quietly stop
    /// being the rule.
    ///
    /// **CPE-1809: the failure-message assertion below pins OUR wrapper wording, not the bare word
    /// "hard".** The scratch directory this test used to run in was named `cpe1759_hardlink`, and every
    /// path this test touches lives under it — so `err.contains("hard")` was true from the *directory
    /// name* alone, before the error text said anything about the entry at all. Renamed to
    /// `cpe1759_tar_link_escape` (no "hard" substring anywhere in the path) and the assertion now also
    /// checks for [`tar_link_creation_outcome`]'s own fixed wrapper phrase `"could not create the link"`,
    /// which the fixture's naming can never supply.
    ///
    /// **Red-proof (re-run 2026-08-23):** replacing that wrapper's text with `format!("boom: {e}")` turned
    /// this test red — `...the failure must be OUR link-creation wrapper naming the entry it failed on...
    /// got boom: failed to unpack \`...\dst_missing-target-inside_false\hard\`` — and the panic message
    /// itself proves the OLD assertion would NOT have caught it: the entry's own name ("hard") is still
    /// the final path component even under the broken wording, so `err.contains("hard")` alone stays
    /// `true` on "boom: ..." too. Renaming the fixture closes the accidental match from the directory
    /// name; requiring the wrapper phrase closes the coincidental match from the entry's own name.
    #[test]
    fn cpe1759_an_escaping_tar_hard_link_is_skipped_while_a_missing_target_still_fails() {
        let d = scratch("cpe1759_tar_link_escape");
        let outside = d.join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("victim.txt"), b"SECRET").unwrap();

        // (label, target, is this a REFUSAL — one skip — or a FAILURE — one counted `failed`?)
        //
        // The absolute leg uses a SHORT absolute path rather than the scratch directory's own: a GNU tar
        // header's link-name field is 100 bytes, and `set_link_name` on the real temp path failed with
        // *"provided value is too long"* — but only when the suite ran in full, because the scratch
        // counter suffix pushed it over. A test whose fixture depends on how many tests ran before it is
        // not a test, so the length is taken out of the equation instead of being got away with.
        let abs_outside = if cfg!(windows) { "C:\\cpe_victim.txt" } else { "/cpe_victim.txt" };
        let cases: Vec<(&str, String, bool)> = vec![
            ("relative-parent", format!("..{}victim.txt", std::path::MAIN_SEPARATOR), true),
            ("absolute", abs_outside.to_string(), true),
            ("missing-target-inside", "not_in_this_archive.txt".to_string(), false),
        ];

        for (label, target, refusal) in cases {
            let ap = d.join(format!("h_{label}.tar"));
            fs::write(&ap, craft_tar_with_hard_link(&target)).unwrap();

            for streamed in [false, true] {
                let dest = outside.join(format!("dst_{label}_{streamed}"));
                let outcome: Result<Option<ArchiveReport>, String> = if streamed {
                    extract_archive_streamed(
                        &ap.to_string_lossy(),
                        &dest.to_string_lossy(),
                        &AtomicBool::new(false),
                        |_| {},
                    )
                    .map(Some)
                } else {
                    // CPE-1837: the one-shot path now carries a report too.
                    extract_archive(&ap.to_string_lossy(), &dest.to_string_lossy()).map(|o| Some(o.report))
                };

                // ---- the harm, before the Result, on every leg ----
                assert_eq!(
                    fs::read_to_string(outside.join("victim.txt")).unwrap(),
                    "SECRET",
                    "{label} streamed={streamed}: the victim must be untouched"
                );
                assert!(
                    !dest.join("hard").exists(),
                    "{label} streamed={streamed}: no link may be created at the entry's name"
                );

                if !refusal {
                    // **CPE-1935 moved this line, and moved it deliberately.** This used to
                    // `expect_err`: a hard link whose target does not exist ended the archive. It is
                    // still `fs::hard_link` FAILING rather than a guard refusing — which is the whole
                    // point of this leg and is why `failed`, not `skipped`, is what must be 1 — but
                    // under `EntrySlotAction`'s scope rule the evidence is about one entry, so the
                    // other entries are no longer paid for it.
                    let report = outcome
                        .unwrap_or_else(|e| {
                            panic!("{label} streamed={streamed}: one failing entry must not be the run's error: {e}")
                        })
                        .expect("both legs carry a report");
                    assert_eq!(
                        (report.failed, report.skipped),
                        (1, 0),
                        "{label} streamed={streamed}: a hard link whose target does not exist is \
                         `fs::hard_link` FAILING, not a guard refusing. A `skipped` here means that \
                         line moved and this module's refusal-versus-failure rule needs rewriting with \
                         it: {report:?}"
                    );
                    let err = report.errors.join(" | ");
                    assert!(
                        // CPE-1809: `err.contains("hard")` alone cannot fail — every path in this test
                        // lives under the `cpe1759_hardlink` scratch directory (renamed above), AND the
                        // entry's own name is literally "hard", so the substring was present in ANY error
                        // this test could produce, including a wrong one — see the red-proof on this
                        // function's doc. Pinned instead on `tar_link_creation_outcome`'s own fixed
                        // wrapper phrase (never supplied by a fixture or an entry name) together with the
                        // entry name.
                        err.contains("could not create the link") && err.contains("hard"),
                        "{label} streamed={streamed}: and the failure must be OUR link-creation wrapper \
                         naming the entry it failed on, not merely be some error — got {err}"
                    );
                    continue;
                }

                let report = outcome.unwrap_or_else(|e| {
                    panic!(
                        "{label} streamed={streamed}: an escaping hard link is a REFUSAL and must skip. \
                         Before CPE-1759 both legs returned exactly this: {e}"
                    )
                });
                assert_eq!(
                    fs::read(dest.join("ok.txt")).unwrap(),
                    b"ORDINARY".to_vec(),
                    "{label} streamed={streamed}: and the rest of the archive still extracts — `ok.txt` \
                     sits after the poisoned entry, so its absence is what the old abort looked like"
                );
                if let Some(report) = report {
                    assert_eq!(
                        report.skipped, 1,
                        "{label}: the refusal must be counted (CPE-1775); got {report:?}"
                    );
                    assert!(
                        report.errors.iter().any(|e| {
                            e.starts_with("hard: ") && e.contains("outside the extraction folder")
                        }),
                        "{label}: and recorded as a link-target escape naming the entry — an `is_ok()` \
                         check would stay green through a guard that skipped for any reason at all; got \
                         {:?}",
                        report.errors
                    );
                }
            }
        }
        let _ = fs::remove_dir_all(&d);
    }

    // -----------------------------------------------------------------------
    // CPE-1938 — the tar/7z inside-pointing junction, and the mode pass that followed links
    // -----------------------------------------------------------------------

    /// Stage the shape both CPE-1938 F-A tests use: an archive whose entries live under `sub/`, an
    /// extraction folder with a **directory link planted at `sub`**, and somewhere for that link to
    /// point. Returns `(scratch, archive, dest, elsewhere)`.
    ///
    /// **The link is planted at the REAL destination**, `dest/sub`, not at a stand-in inside a
    /// `tempfile::tempdir()`: a stand-in is unreachable by the production code and every assertion about
    /// it is unfalsifiable (CPE-1929). A runner that cannot plant a directory link **fails** rather
    /// than skipping — a control that silently returns green because it could not stage its own fixture
    /// proves nothing, invisibly (CPE-1952 round 2).
    ///
    /// **The failure comes from the `assert!` below, not from `make_dir_link` itself, and the
    /// distinction is CI-vs-local.** `make_dir_link` carries `supported_here = true`, so
    /// `fsutil::require_staged` panics on a staging failure only when `staging_is_strict()` — which
    /// follows `$CI` (and `CPE_STAGING_STRICT`). On a developer's machine it returns **`false`**
    /// instead, which is the deliberate loud-skip path for an environment the mechanism genuinely
    /// cannot work in. Wrapping the call in `assert!` is what makes the outcome red **either way**, so
    /// this fixture is never the thing that quietly certifies nothing.
    fn stage_inside_pointing_dir_link(
        tag: &str,
        kind: &str,
        point_inside: bool,
    ) -> (crate::fsutil::ScratchDir, PathBuf, PathBuf, PathBuf) {
        let d = scratch(tag);
        let stage = d.join("stage");
        fs::create_dir_all(stage.join("sub").join("deeper")).unwrap();
        fs::write(stage.join("sub").join("leaf.txt"), b"ARCHIVED LEAF").unwrap();
        fs::write(stage.join("ok.txt"), b"ARCHIVED OK").unwrap();
        let archive = if kind == "tar" {
            let p = d.join("in.tar.gz");
            compress_to_targz(
                &[
                    stage.join("sub").to_string_lossy().to_string(),
                    stage.join("ok.txt").to_string_lossy().to_string(),
                ],
                &p.to_string_lossy(),
            )
            .unwrap();
            p
        } else {
            let p = d.join("in.7z");
            write_7z_fixture(&p, &[("sub/leaf.txt", b"ARCHIVED LEAF"), ("ok.txt", b"ARCHIVED OK")]);
            p
        };
        let dest = d.join("out");
        fs::create_dir_all(&dest).unwrap();
        let elsewhere = if point_inside { dest.join("other") } else { d.join("outside") };
        fs::create_dir_all(&elsewhere).unwrap();
        assert!(
            crate::fsutil::make_dir_link(&elsewhere, &dest.join("sub")),
            "could not plant a directory link at {} — every assertion below would be vacuous, so this \
             is a failure rather than a skip",
            dest.join("sub").display()
        );
        // Liveness: the planted link must really redirect, or the fixture certifies nothing.
        fs::write(dest.join("sub").join("liveness.txt"), b"through").unwrap();
        assert_eq!(
            fs::read(elsewhere.join("liveness.txt")).ok().as_deref(),
            Some(&b"through"[..]),
            "the fixture is inert: the link at dest/sub does not redirect (point_inside={point_inside})"
        );
        fs::remove_file(dest.join("sub").join("liveness.txt")).unwrap();
        (d, archive, dest, elsewhere)
    }

    /// **CPE-1938 F-A — the sensitivity control: with no guard in front of them, the primitives the tar
    /// and 7z unpackers use write straight through the planted link.**
    ///
    /// This runs the *old* by-path pair — `fs::create_dir_all(dest/sub/deeper)` and
    /// `fs::File::create(dest/sub/leaf.txt)`, which is what `tar::Entry::unpack_in` and
    /// `sevenz_rust::default_entry_extract_fn` do — against the identical fixture the regression below
    /// uses, and asserts the attack **succeeds**. Without it the regression's green would be
    /// indistinguishable from a fixture that never redirected anything on this runner, which is CPE-1952
    /// round 2's lesson written as a test rather than as a comment.
    ///
    /// It runs on **all three OSes** and is not `#[ignore]`d: a junction needs no privilege on Windows
    /// and `symlink` always works on Linux and macOS, so a staging failure is a red build (see
    /// `stage_inside_pointing_dir_link`).
    ///
    /// **Both directions**, because the two halves of this ticket are about the difference: the
    /// outside-pointing link is what `confined_to` has refused since CPE-1744, the inside-pointing one
    /// is what it cannot see — and the primitives themselves do not distinguish them at all, which is
    /// precisely why the guard cannot be a path question.
    #[test]
    fn cpe1938_the_by_path_primitives_write_through_a_planted_link_in_both_directions() {
        for point_inside in [true, false] {
            let (d, _archive, dest, elsewhere) = stage_inside_pointing_dir_link(
                &format!("cpe1938-control-{point_inside}"),
                "tar",
                point_inside,
            );
            // Exactly what the unpackers do, in the order they do it.
            fs::create_dir_all(dest.join("sub").join("deeper")).unwrap();
            let mut f = fs::File::create(dest.join("sub").join("leaf.txt")).unwrap();
            f.write_all(b"ARCHIVED LEAF").unwrap();
            drop(f);

            assert_eq!(
                fs::read(elsewhere.join("leaf.txt")).ok().as_deref(),
                Some(&b"ARCHIVED LEAF"[..]),
                "CONTROL FAILED (point_inside={point_inside}): `File::create` did NOT write through the \
                 planted link into {}. The regression test's green would then mean nothing — nothing on \
                 this runner is redirecting, so nothing is being refused either",
                elsewhere.display()
            );
            assert!(
                elsewhere.join("deeper").exists(),
                "CONTROL FAILED (point_inside={point_inside}): `create_dir_all` did NOT build the \
                 archive's tree shape through the planted link into {}",
                elsewhere.display()
            );
            let _ = fs::remove_dir_all(&d);
        }
    }

    /// **CPE-1938 F-A: no tar or 7z leg redirects an entry through a link planted at one of its path
    /// components — including one that points back INSIDE the extraction folder.**
    ///
    /// # What was measured on `main` (Windows 11, a junction, no privilege needed)
    ///
    /// ```text
    /// [tar  one-shot  junction -> dest/other] Err("failed to unpack `…\out\sub`")
    ///                                         other/leaf.txt = "ARCHIVED LEAF"   <- payload redirected
    ///                                         other/deeper   = created           <- tree redirected
    /// [tar  streamed  junction -> dest/other] Err("failed to unpack `…\out\sub`")
    ///                                         nothing extracted at all, ok.txt included
    /// [7z   one-shot  junction -> dest/other] Ok(done: 2, skipped: 0, errors: [])
    ///                                         other/leaf.txt = "ARCHIVED LEAF"   <- silent success
    /// [7z   streamed  junction -> dest/other] Ok(done: 2, skipped: 0, errors: [])
    ///                                         other/leaf.txt = "ARCHIVED LEAF"   <- silent success
    /// ```
    ///
    /// The two 7z rows are the ones CPE-1938 was filed calling **inferred**; they are measured now, and
    /// they are the worst of the four — a clean report over a payload written where the archive never
    /// said. The tar rows fail two different ways from one cause: the one-shot leg does the harm and
    /// *then* errors (its directory entries are deferred to a second pass, which is what finally trips
    /// over the junction), and the streamed leg turns one planted junction into total denial.
    ///
    /// **The outside-pointing direction was already refused on all four legs** — `confined_to`, since
    /// CPE-1744 — which is why `point_inside` is a loop rather than a constant: a fix that closed the
    /// new shape and lost the old one reds here rather than somewhere else, and the two directions
    /// assert **different markers**, so each says which guard actually answered.
    ///
    /// # Red-proof — the CPE-1929 pair, run by hand on this branch (Windows, full `--lib` suite)
    ///
    /// - **Sabotage A, disable it** (`entry_component_action` returning `EntrySlotAction::Write`
    ///   unconditionally): **2427 passed, 1 failed** — this test, on the `point_inside = true` harm
    ///   assertion, naming the redirected bytes. Re-run with the loop pinned to `[false]`: **green**.
    ///   So the four outside-pointing legs are `confined_to`'s and the four inside-pointing ones are
    ///   this guard's, which is what "reachable" means here.
    /// - **Sabotage B, force its predicate to lie** (`create_dir_beneath`'s `policy` refusal mapped to
    ///   `Write`): **2427 passed, 1 failed**, same test. The guard is reached AND its answer decides —
    ///   the pair CPE-1929 asks for, and neither half came back green.
    /// - **Sabotage C, the mirror** (`entry_sink_action`'s and `entry_dir_action`'s `confined_to` arms
    ///   short-circuited): loop pinned to `[true]` → **green**; pinned to `[false]` → **red on the
    ///   marker assertion**, with the component walk's wording in `errors` instead of
    ///   `CONTAINMENT_MARKER`. Worth stating precisely, because it is the interesting result: with
    ///   containment gone the outside-pointing entry is still **refused** — the component walk is a
    ///   real backstop for it — so what sabotage C changes is *which guard answers*, not whether bytes
    ///   escape. Containment is therefore a **belt in front of** this walk for the intermediate-component
    ///   shape, not a guard this walk shadows: it runs first, it answers first, and this walk is still
    ///   the only thing that answers the inside-pointing case. Nothing here is unreachable, which is why
    ///   the ordering was left alone rather than reordered or deleted.
    ///
    /// **One test carries all of it, and that is a fact rather than a boast**: under both sabotages the
    /// rest of the 2,428-test suite stayed green, so nothing else in the crate covers this shape.
    ///
    /// **Assertions on the filesystem, before the `Result` is unwrapped** — every defect in this family
    /// failed by returning `Ok`, and two of the four returned `Err` *after* doing the damage, so an
    /// unwrap-first test would never have run the assertion that names it.
    #[test]
    fn cpe1938_tar_and_7z_never_redirect_an_entry_through_a_link_at_a_path_component() {
        type Run = fn(&Path, &Path) -> Result<ArchiveReport, String>;
        // (label, archive kind, run, does the fixture carry a DIRECTORY entry under `sub`?)
        let legs: &[(&str, &str, Run, bool)] = &[
            (
                "row 21 tar_unpack via extract_archive",
                "tar",
                |a: &Path, dest: &Path| {
                    extract_archive(&a.to_string_lossy(), &dest.to_string_lossy())
                        .map(|o| o.report)
                },
                true,
            ),
            (
                "row 22 extract_tar_stream via extract_archive_streamed",
                "tar",
                |a: &Path, dest: &Path| {
                    extract_archive_streamed(
                        &a.to_string_lossy(),
                        &dest.to_string_lossy(),
                        &AtomicBool::new(false),
                        |_| {},
                    )
                },
                true,
            ),
            (
                "row 19 extract_7z_safe via extract_archive",
                "7z",
                |a: &Path, dest: &Path| {
                    extract_archive(&a.to_string_lossy(), &dest.to_string_lossy())
                        .map(|o| o.report)
                },
                // `write_7z_fixture` emits file entries only, so there is no directory entry to redirect
                // — stated rather than asserted vacuously, the same way `row18` scopes itself to ZIP.
                false,
            ),
            (
                "row 20 extract_7z_stream via extract_archive_streamed",
                "7z",
                |a: &Path, dest: &Path| {
                    extract_archive_streamed(
                        &a.to_string_lossy(),
                        &dest.to_string_lossy(),
                        &AtomicBool::new(false),
                        |_| {},
                    )
                },
                false,
            ),
        ];

        for (label, kind, run, has_dir_entry) in legs {
            for point_inside in [true, false] {
                let (d, archive, dest, elsewhere) = stage_inside_pointing_dir_link(
                    &format!("cpe1938-{kind}-{point_inside}"),
                    kind,
                    point_inside,
                );
                let outcome = run(&archive, &dest);

                // HARM FIRST, off the filesystem.
                assert!(
                    !elsewhere.join("leaf.txt").exists(),
                    "{label} (point_inside={point_inside}): the entry's bytes were written through the \
                     link at dest/sub into {}, which the archive never named. Outcome was {outcome:?}",
                    elsewhere.display()
                );
                if *has_dir_entry {
                    assert!(
                        !elsewhere.join("deeper").exists(),
                        "{label} (point_inside={point_inside}): the archive's `sub/deeper` DIRECTORY was \
                         created through the link, inside {}. `create_dir_all` writes no bytes, so the \
                         file assertion above cannot see this one — it is the tree shape leaking out. \
                         Outcome was {outcome:?}",
                        elsewhere.display()
                    );
                }
                assert!(
                    fs::symlink_metadata(dest.join("sub"))
                        .map(|m| m.file_type().is_symlink())
                        .unwrap_or(false),
                    "{label} (point_inside={point_inside}): the user's own link must survive untouched — \
                     a guard that deleted it and then wrote would pass the assertions above"
                );

                let report = outcome.unwrap_or_else(|e| {
                    panic!(
                        "{label} (point_inside={point_inside}): a link at a component SKIPS the entry; \
                         the rest of the archive still extracts. Aborting the run is the unreadable-slot \
                         arm only, and on `main` this leg's abort came AFTER the damage. Got: {e}"
                    )
                });
                // **Which guard answered, not merely that something did.** Two markers, deliberately
                // non-overlapping: the component walk's wording (`WHY_LINK`, absent from the
                // `open_beneath` boilerplate for CPE-1896 round 4's reason) for the inside-pointing
                // link, and `CONTAINMENT_MARKER` for the outside-pointing one that `confined_to`
                // answers first. Asserting `is_err()` — or one shared marker — would pass whichever
                // guard fired, and would therefore not notice one of them becoming unreachable.
                let marker: &str = if point_inside {
                    "is a link (a symlink, junction or other reparse point)"
                } else {
                    CONTAINMENT_MARKER
                };
                assert!(
                    report
                        .errors
                        .iter()
                        .any(|e| e.starts_with("sub/leaf.txt: ") && e.contains(marker)),
                    "{label} (point_inside={point_inside}): the refusal must be RECORDED against the \
                     entry and carry {marker:?} — without the name prefix this passes on a note about \
                     some other entry, and without the marker it passes on whatever the OS happened to \
                     say. Got {:?}",
                    report.errors
                );
                assert_eq!(
                    fs::read(dest.join("ok.txt")).ok().as_deref(),
                    Some(&b"ARCHIVED OK"[..]),
                    "{label} (point_inside={point_inside}): a skip costs ONE entry, not the archive. \
                     `main`'s streamed tar leg lost ok.txt too, which is the denial half of this defect. \
                     Report: {report:?}"
                );
                let _ = fs::remove_dir_all(&d);
            }
        }
    }

    /// **CPE-1938: the tar and 7z legs now need the extraction folder to be OPENABLE, and say so.**
    ///
    /// The new failure mode CPE-1913 recorded for the zip leg, arriving on the other four legs with the
    /// root handle: a directory handle is what every component is resolved against, so a folder that can
    /// be written but not opened for read now fails the run with a named reason instead of extracting.
    /// Rare and loud rather than silent — the same trade CPE-1896 recorded for the backup destination —
    /// and it is a test rather than a sentence because
    /// `cpe1935_an_unreadable_slot_is_a_recorded_entry_failure_on_both_tar_paths` (named
    /// `cpe1759_an_unreadable_slot_aborts_both_tar_paths_rather_than_being_skipped` when this paragraph
    /// was written) used to stage exactly this shape by accident and would otherwise have been the only
    /// thing covering it.
    ///
    /// Staged with `deny_stat_of` on a file inside `dest`, which denies `dest` itself (list-directory on
    /// Windows, `chmod 0o000` on Unix) — the same mechanism, aimed one level up on purpose.
    #[test]
    fn cpe1938_an_unopenable_extraction_folder_aborts_the_tar_and_7z_runs() {
        for kind in ["tar", "7z"] {
            for streamed in [false, true] {
                let d = scratch(&format!("cpe1938-unopenable-{kind}-{streamed}"));
                let archive = if kind == "tar" {
                    let p = d.join("in.tar.gz");
                    fs::write(&p, gzip_bytes(&craft_tar_with_entry_name("a.txt", b"ARCHIVED A"))).unwrap();
                    p
                } else {
                    let p = d.join("in.7z");
                    write_7z_fixture(&p, &[("a.txt", b"ARCHIVED A")]);
                    p
                };
                let dest = d.join("out");
                fs::create_dir_all(&dest).unwrap();
                let marker_file = dest.join("marker.txt");
                fs::write(&marker_file, b"x").unwrap();

                struct Restore<'a> {
                    target: &'a Path,
                    parent: &'a Path,
                    root: &'a Path,
                }
                impl Drop for Restore<'_> {
                    fn drop(&mut self) {
                        crate::fsutil::undo_deny_stat_of(self.target, self.parent);
                        let _ = fs::remove_dir_all(self.root);
                    }
                }
                let _r = Restore { target: &marker_file, parent: &dest, root: &d };
                assert!(
                    crate::fsutil::deny_stat_of(&marker_file),
                    "could not deny access to {} — nothing in this leg would have been covered",
                    dest.display()
                );

                let outcome = if streamed {
                    extract_archive_streamed(
                        &archive.to_string_lossy(),
                        &dest.to_string_lossy(),
                        &AtomicBool::new(false),
                        |_| {},
                    )
                    .map(|r| format!("{r:?}"))
                } else {
                    extract_archive(&archive.to_string_lossy(), &dest.to_string_lossy())
                        .map(|o| format!("{:?}", o.report))
                };
                let err = outcome.expect_err(
                    "an extraction folder that cannot be opened has no handle for the per-component \
                     walk to resolve against, so nothing can be written into it in a way that can be \
                     checked — that is a failure, not a silent partial extraction",
                );
                assert!(
                    err.contains(&dest.to_string_lossy().to_string()) || err.contains("extraction folder"),
                    "{kind} streamed={streamed}: the refusal must NAME the folder the user chose, or the \
                     user has nothing to act on. Got: {err}"
                );
            }
        }
    }

    /// **CPE-1938 F-B — the sensitivity control: `fs::set_permissions` follows a link, so the pass that
    /// used to stand at the bottom of `extract_zip_archive_stream` really did chmod through one.**
    ///
    /// The old primitive, run directly on the shape the regression below stages: a symlink at the slot
    /// the loop wrote, pointing at a victim **outside** the extraction folder. If this ever stops
    /// changing the victim's mode, the regression's green means nothing — `chmod(2)`-follows-symlinks is
    /// the whole premise, and a runner where it did not hold would silently certify nothing.
    ///
    /// `#[cfg(unix)]` because the pass itself is: mode bits do not exist on Windows and that code does
    /// not compile there, so this is covered by the Linux and macOS legs of the matrix only — a green
    /// local Windows run says nothing about it. That asymmetry is forced by the defect, not chosen: the
    /// F-A control above runs on all three.
    #[cfg(unix)]
    #[test]
    fn cpe1938_the_old_path_addressed_mode_pass_chmods_through_a_planted_link() {
        use std::os::unix::fs::PermissionsExt;
        let d = scratch("cpe1938-modes-control");
        let dest = d.join("out");
        fs::create_dir_all(&dest).unwrap();
        let outside = d.join("outside");
        fs::create_dir_all(&outside).unwrap();
        let victim = outside.join("victim.sh");
        fs::write(&victim, b"#!/bin/sh\n").unwrap();
        fs::set_permissions(&victim, fs::Permissions::from_mode(0o644)).unwrap();
        let slot = dest.join("a.txt");
        std::os::unix::fs::symlink(&victim, &slot).unwrap();

        // The exact call the loop used to make, with the mode the ARCHIVE chose.
        fs::set_permissions(&slot, fs::Permissions::from_mode(0o777)).unwrap();

        assert_eq!(
            fs::metadata(&victim).unwrap().permissions().mode() & 0o7777,
            0o777,
            "CONTROL FAILED: `fs::set_permissions` did not follow the link out of the extraction folder \
             on this runner. The regression below would then be green for a reason that has nothing to \
             do with the fix"
        );
        assert!(
            fs::symlink_metadata(&slot).unwrap().file_type().is_symlink(),
            "CONTROL FAILED: the chmod replaced the link instead of following it — a different \
             mechanism, and the regression would be measuring the wrong thing"
        );
        let _ = fs::remove_dir_all(&d);
    }

    /// **CPE-1938 F-B: a slot swapped for a link after its bytes are written never moves the
    /// archive-chosen mode out of the extraction folder.**
    ///
    /// # What was measured on `main`, on real ext4
    ///
    /// ```text
    /// trials=60  swaps=60  MODES_CHANGED_OUTSIDE=60      victim 0o644 -> 0o777
    /// ```
    ///
    /// **60 out of 60** — not a narrow window. The pass was *deferred to the end of the archive*, so the
    /// window between a file's write and its chmod was the whole rest of the run; a swapper that fires
    /// once, the instant the slot appears, wins every time. The mode is the archive's, and `chmod(2)`
    /// applies whatever the link points at.
    ///
    /// # Why the swap is still expected to happen with the fix in
    ///
    /// `swaps` is asserted to be **every trial**, and that is the liveness half: the fix does not stop
    /// the attacker replacing the name — it stops the replacement mattering, because the mode goes onto
    /// the descriptor `claim_destination_handle` returned rather than onto the name. A version of this
    /// test that only asserted `MODES_CHANGED_OUTSIDE == 0` would pass on a runner where the swapper
    /// never got in, which is the invisible-green failure CPE-1952 round 2 is about.
    ///
    /// The swapper is bounded by a **deadline**, not by a stop flag, precisely so that assertion cannot
    /// flake into a false alarm on a loaded or single-core runner: the slot is a real file for the rest
    /// of the process, so a starved thread still stages its swap. That the window is real *during* the
    /// run is not asserted here — it is the sabotage measurement below, which is where timing evidence
    /// belongs.
    ///
    /// # Setuid — measured, not inferred (CPE-1938 round 2, Security Auditor)
    ///
    /// The ticket names setuid as the worst case. *This* fixture cannot request it: `zip`'s writer masks
    /// with `& 0o777` (`FileOptions::unix_permissions`, and its own `unix_permissions_bitmask` test says
    /// so), so a fixture built through that API tops out at `0o777`. **That mask is the writer's, not the
    /// format's** — `ZipFile::unix_mode` returns the archive's external attributes unmasked — so a
    /// hand-built STORED archive carrying external attributes `0o104755` answers the question directly.
    /// Round 1 wrote "inferred, not measured" here; round 2 measured it, on real ext4 with `TMPDIR`
    /// pointed off `/tmp` (see the methodology note below):
    ///
    /// ```text
    /// reader + extractor:  dest/a.txt lands as 0o4755        <- setuid is archive-controllable, end to end
    /// old primitive through a planted link: victim outside the root 0o644 -> 0o4755
    /// end to end, with `main`'s deferred path-addressed drain restored:
    ///     trials=20  swaps=20  MODES_CHANGED_OUTSIDE=20  SETUID_OUTSIDE=20
    /// this branch, unmodified:
    ///     trials=20  swaps=20  MODES_CHANGED_OUTSIDE=0   SETUID_OUTSIDE=0
    /// ```
    ///
    /// So F-B was a **privilege-escalation primitive on `main` at 20/20**, not merely a mode change,
    /// and this branch closes it.
    ///
    /// **The honest scope, which the numbers do not by themselves give.** `chmod(2)` succeeds only for
    /// the file's owner, so the victim must be owned by the extracting user. Extracting as root or as a
    /// service account makes this fatal; between two files of the same unprivileged user it is an
    /// integrity problem rather than an escalation. Stated here rather than left for the reader to
    /// infer in the generous direction.
    ///
    /// **Methodology note, because it silently invalidates a "real ext4" claim.**
    /// `std::env::temp_dir()` is `/tmp`, and on WSL `/tmp` is **tmpfs**, not ext4. Every number above
    /// was taken with `TMPDIR` overridden to an ext4 path; a run that does not override it is measuring
    /// tmpfs and should not be labelled otherwise.
    ///
    /// # Red-proof (CPE-1929's pair, run by hand on this branch)
    ///
    /// - **Disable the fix** (restore the deferred `modes` vector and the trailing path-addressed
    ///   `fs::set_permissions` loop, byte for byte as `main` had it): **red**, `MODES_CHANGED_OUTSIDE =
    ///   19 of 20` trials, on real ext4. Nineteen and not twenty because this test's filler is a
    ///   quarter of the standalone measurement's, so one trial's swapper lost a shorter window; the
    ///   assertion is `== 0`, so a single escape is still red. **Round 2's independent re-run of the
    ///   same sabotage came back 20/20** (Security Auditor, `TMPDIR` on ext4) — hotter, not cooler, so
    ///   the "one trial lost the window" reading is a floor rather than a ceiling and the 19 should not
    ///   be quoted as the rate.
    /// - **Force the predicate to lie**: this guard has no predicate to falsify — it is an *addressing*
    ///   change, not a test. `f.set_permissions` cannot be made to consult a name. The honest analogue
    ///   is the first sabotage plus `cpe1938_the_old_path_addressed_mode_pass_chmods_through_a_planted_link`,
    ///   which proves independently that `chmod(2)` really does follow a link on this runner; both are
    ///   here, and neither came back green.
    #[cfg(unix)]
    #[test]
    fn cpe1938_a_swapped_slot_never_moves_an_archive_chosen_mode_outside_the_root() {
        use std::os::unix::fs::PermissionsExt;
        const TRIALS: usize = 20;
        let mut changed_outside = 0usize;
        let mut swaps = 0usize;
        for t in 0..TRIALS {
            let d = scratch(&format!("cpe1938-modes-race-{t}"));
            let ap = d.join("modes.zip");
            {
                let mut w = zip::ZipWriter::new(fs::File::create(&ap).unwrap());
                let wide: zip::write::FileOptions<()> =
                    zip::write::FileOptions::default().unix_permissions(0o777);
                w.start_file("a.txt", wide).unwrap();
                w.write_all(b"PAYLOAD").unwrap();
                let plain: zip::write::FileOptions<()> =
                    zip::write::FileOptions::default().unix_permissions(0o644);
                // Filler, so the loop is still running when the swapper fires. On `main` this is what
                // made the window the whole rest of the archive.
                for i in 0..12 {
                    w.start_file(format!("filler{i}.bin"), plain).unwrap();
                    w.write_all(&vec![b'x'; 100_000]).unwrap();
                }
                w.finish().unwrap();
            }
            let dest = d.join("out");
            fs::create_dir_all(&dest).unwrap();
            let outside = d.join("outside");
            fs::create_dir_all(&outside).unwrap();
            let victim = outside.join("victim.sh");
            fs::write(&victim, b"#!/bin/sh\n").unwrap();
            fs::set_permissions(&victim, fs::Permissions::from_mode(0o644)).unwrap();

            let slot = dest.join("a.txt");
            // **Bounded by a deadline rather than by a stop flag, so `swaps` is deterministic.** The
            // swapper exits the moment it has replaced the slot; with the fix in, `a.txt` is a real
            // file for the whole run and stays one afterwards, so a thread that was starved during the
            // extraction still stages its swap and the liveness assertion below cannot flake. The
            // deadline is the hang guard for the case where nothing ever creates the slot.
            let swapper = {
                let (slot, victim) = (slot.clone(), victim.clone());
                std::thread::spawn(move || {
                    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
                    while std::time::Instant::now() < deadline {
                        if fs::symlink_metadata(&slot).map(|m| m.is_file()).unwrap_or(false) {
                            let _ = fs::remove_file(&slot);
                            return std::os::unix::fs::symlink(&victim, &slot).is_ok();
                        }
                        std::thread::yield_now();
                    }
                    false
                })
            };
            let _ = extract_archive_streamed(
                &ap.to_string_lossy(),
                &dest.to_string_lossy(),
                &AtomicBool::new(false),
                |_| {},
            );
            if swapper.join().unwrap() {
                swaps += 1;
            }
            // On the filesystem, and on the victim OUTSIDE the folder — never on the returned report.
            if fs::metadata(&victim).map(|m| m.permissions().mode() & 0o7777).unwrap_or(0o644) != 0o644 {
                changed_outside += 1;
            }
            let _ = fs::remove_dir_all(&d);
        }
        assert_eq!(
            changed_outside, 0,
            "an archive-chosen mode reached a file OUTSIDE the extraction folder in {changed_outside} of \
             {TRIALS} trials. On `main` this was 60/60; anything above zero means the mode is being \
             applied to a NAME again instead of to the descriptor the bytes went into"
        );
        assert_eq!(
            swaps, TRIALS,
            "the swapper won only {swaps} of {TRIALS} trials, so the assertion above is not evidence: a \
             run where nothing was ever swapped in would report zero escapes no matter what the \
             extraction did. The fix does not prevent the swap — it makes it irrelevant"
        );
    }

    /// **CPE-1961 round 2: the zip-extract Unix-mode leg, which had NO test at all.**
    ///
    /// # Why this exists
    ///
    /// Round 1 renamed the extraction loop's local `f` to `claimed` and moved the write to
    /// `claimed.file`, but left the `#[cfg(unix)]` mode block referring to the old `f` *and* below
    /// `claimed.commit()`, which consumes the claim. `crates/server` stopped compiling on Linux and
    /// macOS (`error[E0425]: cannot find value `f``) and **the whole Windows CI leg stayed green**,
    /// because the block is `#[cfg(unix)]` and a Windows build never parses past `#[cfg]`. Two
    /// reviewers found it by building on Linux; nothing in the suite could have. The gap is that this
    /// leg — an archive-chosen mode arriving on an extracted file — had no assertion anywhere, so even
    /// on Linux the only thing standing between it and silence was the compiler.
    ///
    /// # What it asserts, and which half is one-sided
    ///
    /// 1. **The mode lands.** Three entries with distinct modes come out wearing them. Deleting the
    ///    `#[cfg(unix)]` block turns this red on every Unix runner.
    /// 2. **The bytes land with it**, so a green mode assertion cannot be satisfied by an empty or
    ///    staged-but-uncommitted file.
    /// 3. **No `.cpe-tmp` residue survives** in the destination. A commit that silently failed, or a
    ///    claim dropped after a successful write, would leave one.
    /// 4. **The name is never observed holding CONTENT at a non-final mode** — the ordering half, and
    ///    the one the moved block is actually about. The staging file is born `0600`
    ///    (`create_staging_beneath`), the mode is applied to it while it is still nameless, and only
    ///    then does `commit()` give it the name. So the bytes and the final mode arrive at
    ///    `dest/exec.sh` in the same instant. A future edit that applied the mode after the commit
    ///    would make a *populated* `dest/exec.sh` observable at `0600` first.
    ///
    /// **Assertion 4 is about CONTENT-at-a-mode, not existence-at-a-mode, and the first draft got that
    /// wrong — the test caught it, which is the point of running one.** The draft asserted the name is
    /// never seen at anything but `0o755` and came back red with five observations of `0o644`. That is
    /// not a bug: `claim_destination_handle` opens the destination through `create_beneath`, which
    /// **creates the name** when it is absent (`created == true`, which is what `Drop` later removes on
    /// an abandoned claim) so the guards have an object to interrogate. That placeholder is an **empty
    /// file at the platform default `0o666 & ~umask`**, and it stands at the destination name for the
    /// whole of the caller's write. It is recorded in this ticket's cost list; it is not what this
    /// assertion is for, so the assertion is scoped to observations where `len > 0`.
    ///
    /// **This half is one-sided and says so rather than pretending otherwise**: the observer polls, so
    /// on a fast or loaded runner it may see the file only after the run finished, in which case it
    /// observes the final state and passes. It can produce a true red and cannot produce a trustworthy
    /// green on its own — which is why 1–3 carry the regression and this carries the ordering claim.
    ///
    /// # Red-proof (run by hand on Linux, CPE-1929's first half)
    ///
    /// - **Delete the `#[cfg(unix)]` mode block**: red on assertion 1, all three entries arriving at
    ///   `0o600` — the staging birth mode, which is itself the measurement behind the `0600` row in
    ///   this ticket's cost list.
    /// - **Move the block back below `claimed.commit()`**: does not compile, which is the defect this
    ///   test was written for and the reason the position is called out in a comment at the site.
    /// - **Assertion 4 on its own**: shown red above, by the placeholder, before it was scoped to
    ///   populated observations — so the observer demonstrably does see the file mid-run on this
    ///   runner rather than only after the extraction has finished.
    #[cfg(unix)]
    #[test]
    fn cpe1961_a_zip_entrys_unix_mode_lands_on_the_committed_file() {
        use std::os::unix::fs::PermissionsExt;
        const WANT: [(&str, u32); 3] = [("exec.sh", 0o755), ("private.txt", 0o600), ("plain.txt", 0o644)];

        let d = scratch("cpe1961-zip-mode");
        let ap = d.join("modes.zip");
        {
            let mut w = zip::ZipWriter::new(fs::File::create(&ap).unwrap());
            for (name, mode) in WANT {
                let o: zip::write::FileOptions<()> =
                    zip::write::FileOptions::default().unix_permissions(mode);
                w.start_file(name, o).unwrap();
                w.write_all(name.as_bytes()).unwrap();
            }
            // Filler AFTER the observed entry, so the extraction is still running when the observer
            // starts looking — the same trick, and the same reason, as the CPE-1938 race above.
            let plain: zip::write::FileOptions<()> =
                zip::write::FileOptions::default().unix_permissions(0o644);
            for i in 0..12 {
                w.start_file(format!("filler{i}.bin"), plain).unwrap();
                w.write_all(&vec![b'x'; 100_000]).unwrap();
            }
            w.finish().unwrap();
        }
        let dest = d.join("out");
        fs::create_dir_all(&dest).unwrap();

        // Assertion 4's observer. Bounded by a deadline, never by a flag, so it cannot hang a run.
        let watched = dest.join("exec.sh");
        let observer = {
            let watched = watched.clone();
            std::thread::spawn(move || {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
                let (mut seen, mut wrong) = (0usize, Vec::<u32>::new());
                while std::time::Instant::now() < deadline {
                    if let Ok(m) = fs::symlink_metadata(&watched) {
                        // `len > 0` is what distinguishes the COMMITTED file from the empty
                        // placeholder `create_beneath` left at the name at claim time — see the doc.
                        if m.len() > 0 {
                            seen += 1;
                            let mode = m.permissions().mode() & 0o7777;
                            if mode != 0o755 {
                                wrong.push(mode);
                            }
                            if seen > 4 {
                                break; // it is committed and has settled; stop burning the CPU
                            }
                        }
                    }
                    std::thread::yield_now();
                }
                (seen, wrong)
            })
        };

        let report = extract_archive_streamed(
            &ap.to_string_lossy(),
            &dest.to_string_lossy(),
            &AtomicBool::new(false),
            |_| {},
        )
        .expect("a plain archive of mode-carrying entries must extract");
        assert_eq!(
            report.done,
            (WANT.len() + 12) as u64,
            "every entry must be reported as extracted: {report:?}"
        );

        for (name, mode) in WANT {
            let p = dest.join(name);
            assert_eq!(
                fs::read(&p).unwrap_or_default(),
                name.as_bytes(),
                "{name}: the committed file must hold the entry's bytes — a mode on an empty or \
                 uncommitted file proves nothing"
            );
            assert_eq!(
                fs::metadata(&p).unwrap().permissions().mode() & 0o7777,
                mode,
                "{name}: the archive's mode did not reach the extracted file. `0600` here means the \
                 `#[cfg(unix)]` block stopped running and the file is wearing the staging birth mode"
            );
        }

        let residue: Vec<String> = fs::read_dir(&dest)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".cpe-tmp"))
            .collect();
        assert!(
            residue.is_empty(),
            "the extraction left staging siblings behind, so a commit did not happen or a claim was \
             dropped after a successful write: {residue:?}"
        );

        let (seen, wrong) = observer.join().unwrap();
        assert!(
            seen > 0,
            "the observer never saw {watched:?} holding any bytes, so assertion 4 measured nothing"
        );
        assert!(
            wrong.is_empty(),
            "the destination name was observed holding CONTENT at a non-final mode {wrong:?} — the \
             mode is being applied AFTER the commit, so there is a window in which the finished file \
             is readable under its real name at the staging birth mode"
        );
        let _ = fs::remove_dir_all(&d);
    }

    /// **CPE-1938 round 2: a component the filesystem refuses for an I/O reason stops the whole run,
    /// and this is the test that says so.**
    ///
    /// [`entry_component_action`]'s non-policy arm — `Err(r) => Abort` — was **uncovered**: the
    /// Security Auditor's CPE-1929 pair came back green in both halves (full `--lib` 2413/2 with the
    /// arm demoted to `Skip`, 2413/2 unmodified), so nothing in the suite could tell an abort from a
    /// skip there. It is an escalation relative to `main`, where `confined_to` failing closed degraded
    /// the same condition to a per-entry skip, so it is exactly the arm that must not be taken on
    /// trust. The argument for keeping it an abort is at the arm itself; this is the coverage.
    ///
    /// **A forced `EACCES`, not a raced `ENOENT`.** The Auditor reached this arm by accident, with a
    /// transient — which is evidence the arm is *reachable* but useless as a regression test. Making
    /// `dest` `r-xr-xr-x` after it exists leaves `open_extraction_root` able to open it for read (so
    /// the run gets past that, and this test cannot be satisfied by the *other* abort at
    /// `cpe1938_an_unopenable_extraction_folder_aborts_the_tar_and_7z_runs`) while `mkdirat` of the
    /// entry's `sub` component fails deterministically.
    ///
    /// Unix-only, and the deny is **verified rather than assumed**: `chmod` is advisory for `root`, so
    /// a container running as uid 0 would silently turn this into a green test that proves nothing
    /// (CPE-1717). The probe below turns that into a loud skip instead.
    #[cfg(unix)]
    #[test]
    fn cpe1938_a_component_the_filesystem_refuses_for_an_io_reason_stops_the_run() {
        use std::os::unix::fs::PermissionsExt;
        let d = scratch("cpe1938-io-refusal");
        let stage = d.join("stage");
        fs::create_dir_all(stage.join("sub")).unwrap();
        fs::write(stage.join("sub").join("leaf.txt"), b"ARCHIVED LEAF").unwrap();
        let archive = d.join("in.tar.gz");
        compress_to_targz(
            &[stage.join("sub").to_string_lossy().to_string()],
            &archive.to_string_lossy(),
        )
        .unwrap();

        let dest = d.join("out");
        fs::create_dir_all(&dest).unwrap();
        fs::set_permissions(&dest, fs::Permissions::from_mode(0o555)).unwrap();
        // Is the deny real on this machine? Running as root ignores it, and a green run would then be
        // vacuous — a loud skip, never a silent pass (Evidence Rules).
        if fs::create_dir(dest.join("probe")).is_ok() {
            let _ = writeln!(
                std::io::stderr(),
                "cpe1938_a_component_the_filesystem_refuses_for_an_io_reason_stops_the_run: SKIPPED — \
                 a 0o555 directory is still writable here (running as root?), so the I/O refusal this \
                 test needs cannot be staged"
            );
            let _ = fs::set_permissions(&dest, fs::Permissions::from_mode(0o755));
            let _ = fs::remove_dir_all(&d);
            return;
        }

        let err = extract_archive(&archive.to_string_lossy(), &dest.to_string_lossy())
            .expect_err("an I/O refusal at a component must stop the run, not be skipped past");
        assert!(
            err.contains("could not be opened") || err.contains("Permission denied"),
            "the abort must name the component and the reason: {err}"
        );

        let _ = fs::set_permissions(&dest, fs::Permissions::from_mode(0o755));
        let _ = fs::remove_dir_all(&d);
    }

    // -----------------------------------------------------------------------
    // CPE-1973 — the ZIP symlink-entry branch had no component walk
    // -----------------------------------------------------------------------

    /// **CPE-1973: a zip SYMLINK entry addressed through a planted, inside-pointing directory link is
    /// refused, and the file the link points at is left alone.**
    ///
    /// This is CPE-1938's defect on the one leg CPE-1938's first round said was already covered. The
    /// per-path table recorded rows 15/16/23 as "already handle-gated (CPE-1913)" and the residual note
    /// bounded the exposure to a *race* — but `create_beneath` is called only in the file branch and
    /// `create_dir_beneath` only under `entry.is_dir()`, so the symlink sub-branch reached a by-path
    /// `symlink` with its components unresolved. No race and no privilege are needed.
    ///
    /// **The harm is a DELETE, not a redirect**, which is why this is worse than the file-branch shape
    /// it mirrors. Measured on real ext4 against the branch before the walk was added, with
    /// `dest/other/victim` holding a real user file:
    ///
    /// ```text
    /// outcome = Ok(ArchiveReport { done: 2, failed: 0, skipped: 0, cancelled: false, errors: [] })
    /// dest/other/victim is now a symlink: true      link target: Some("benign.txt")
    /// its content reads back as: None               <- the user's file was DELETED
    /// ```
    ///
    /// `link_target_action` cannot catch it: `confined_to` canonicalises `dest/sub/victim` *through*
    /// the plant, lands in `dest/other`, and truthfully answers "inside". Then
    /// `materialise_entry_symlink` hits `AlreadyExists` and its `fs::remove_file(out)` re-resolves
    /// `sub` through the link. The old note called that retry harmless on the grounds that
    /// `create_entry_symlink` is exclusive-create; the retry is the clobber.
    ///
    /// **Runs on all three OSes.** The plant is `make_dir_link` (a privilege-free junction on Windows),
    /// and the refusal happens at the component walk, *before* `create_entry_symlink` — so this needs
    /// no symlink-creation privilege anywhere, unlike the entry it refuses.
    ///
    /// `ok.txt` is the bystander: it is what separates "skipped the one entry" from "abandoned the
    /// run", and it is why this asserts `done == 1` rather than only that the victim survived.
    #[test]
    fn cpe1973_a_zip_symlink_entry_is_never_created_through_a_planted_component_link() {
        let d = scratch("cpe1973-zip-symlink-component");
        let zp = d.join("in.zip");
        fs::write(&zp, craft_zip_with_symlink("sub/victim", "benign.txt")).unwrap();

        let dest = d.join("out");
        let other = dest.join("other");
        fs::create_dir_all(&other).unwrap();
        let victim = other.join("victim");
        fs::write(&victim, b"USER FILE").unwrap();

        assert!(
            crate::fsutil::make_dir_link(&other, &dest.join("sub")),
            "could not plant a directory link at {} — every assertion below would be vacuous, so this \
             is a failure rather than a skip (CPE-1717)",
            dest.join("sub").display()
        );
        // Liveness: the plant must really redirect, or the fixture certifies nothing (CPE-1952 r2).
        fs::write(dest.join("sub").join("liveness.txt"), b"through").unwrap();
        assert_eq!(
            fs::read(other.join("liveness.txt")).ok().as_deref(),
            Some(&b"through"[..]),
            "the fixture is inert: the link at dest/sub does not redirect"
        );

        let outcome = extract_archive(&zp.to_string_lossy(), &dest.to_string_lossy())
            .expect("a refused link entry is a skip, not a failure of the whole archive");

        // The harm, first and on its own terms: the user's file is still there, still a file.
        assert_eq!(
            fs::read(&victim).ok().as_deref(),
            Some(&b"USER FILE"[..]),
            "the archive's link entry reached {} through the planted link and destroyed a file the \
             archive never named",
            victim.display()
        );
        assert!(
            !fs::symlink_metadata(&victim).map(|m| m.file_type().is_symlink()).unwrap_or(false),
            "{} was replaced by the archive's symlink",
            victim.display()
        );
        // And it was a per-entry verdict, not a whole-run abort: the bystander still landed.
        assert_eq!(outcome.report.done, 1, "the bystander entry should still extract");
        assert_eq!(outcome.report.skipped, 1, "the link entry should be a counted, recorded skip");
        assert!(
            outcome.report.errors.iter().any(|e| e.contains(CONTAINMENT_MARKER)
                || e.contains("could not be opened")
                || e.contains("stands in for another name")),
            "the skip must say why, in the component walk's own wording: {:?}",
            outcome.report.errors
        );
        let _ = fs::remove_dir_all(&d);
    }

    // ===================================================================================
    // CPE-1935 — one unwritable entry must not take the run down.
    // ===================================================================================

    /// The three-entry fixture every leg below extracts: an entry **before** the blocker, the blocker's
    /// own name, and an entry **after** it. `zc.txt` is the whole test — it is the file the old
    /// all-or-nothing behaviour never wrote, and the one an assertion on the returned `Result` alone
    /// would say nothing about.
    const M1935_NAMES: [&str; 3] = ["a.txt", "blocked.txt", "zc.txt"];

    fn m1935_zip(dest: &Path, names: &[&str]) -> PathBuf {
        let p = dest.join("m.zip");
        let file = fs::File::create(&p).unwrap();
        let mut w = zip::ZipWriter::new(file);
        let opts: zip::write::FileOptions<()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for n in names {
            w.start_file(*n, opts).unwrap();
            w.write_all(format!("PAYLOAD {n}").as_bytes()).unwrap();
        }
        w.finish().unwrap();
        p
    }

    fn m1935_tar(dest: &Path, names: &[&str]) -> PathBuf {
        let p = dest.join("m.tar");
        let file = fs::File::create(&p).unwrap();
        let mut b = tar::Builder::new(file);
        for n in names {
            let body = format!("PAYLOAD {n}");
            let mut h = tar::Header::new_gnu();
            h.set_size(body.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            b.append_data(&mut h, n, body.as_bytes()).unwrap();
        }
        b.finish().unwrap();
        p
    }

    /// The [`M1935_NAMES`] fixture as a STORED zip with **one byte of the middle entry's payload
    /// flipped**, so that entry's recorded CRC no longer matches and `zip`'s own `Crc32Reader` fails the
    /// read partway through `io::copy`. Corrupting the data rather than the checksum field keeps every
    /// length in both headers correct, so the archive opens normally and only that one member is bad —
    /// which is the shape being tested.
    fn craft_zip_with_bad_crc_middle_entry() -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut w = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts: zip::write::FileOptions<()> =
                zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
            for n in M1935_NAMES {
                w.start_file(n, opts).unwrap();
                w.write_all(format!("PAYLOAD {n}").as_bytes()).unwrap();
            }
            w.finish().unwrap();
        }
        let needle = b"PAYLOAD blocked.txt";
        let at = buf
            .windows(needle.len())
            .position(|w| w == needle)
            .expect("a STORED entry's payload is in the file verbatim");
        buf[at + needle.len() - 1] ^= 0xFF;
        buf
    }

    fn m1935_7z(dest: &Path, names: &[&str]) -> PathBuf {
        let p = dest.join("m.7z");
        let bodies: Vec<(String, Vec<u8>)> =
            names.iter().map(|n| ((*n).to_string(), format!("PAYLOAD {n}").into_bytes())).collect();
        let refs: Vec<(&str, &[u8])> =
            bodies.iter().map(|(n, b)| (n.as_str(), b.as_slice())).collect();
        write_7z_fixture(&p, &refs);
        p
    }

    /// What is actually on disk, as a string, for the failure message — because the only thing this
    /// family's history has proved is that a healthy-looking verdict says nothing about the folder.
    fn m1935_state(dest: &Path, names: &[&str]) -> String {
        names
            .iter()
            .map(|n| {
                let p = dest.join(n);
                match fs::read(&p) {
                    Ok(b) => format!("{n}=FILE({})", String::from_utf8_lossy(&b)),
                    Err(_) => match fs::symlink_metadata(&p) {
                        Ok(m) if m.is_dir() => format!("{n}=DIR"),
                        Ok(_) => format!("{n}=OTHER"),
                        Err(_) => format!("{n}=ABSENT"),
                    },
                }
            })
            .collect::<Vec<_>>()
            .join("  ")
    }

    /// **CPE-1935: one entry that cannot be written must cost exactly that entry.**
    ///
    /// # What was measured, and why the assertions are on the filesystem
    ///
    /// The ticket was filed from PR #1050's UAT: a 27-entry archive over a read-only file returned one
    /// sentence naming `existing.txt`, and 23 of the 27 entries were already on disk with no record of
    /// which. Reproduced for this ticket across **six legs × two occupants**, on Windows and on real
    /// ext4 (`TMPDIR` off tmpfs) — twelve cells, and every cell that failed did so identically:
    ///
    /// ```text
    ///                          BEFORE                                          AFTER
    /// dir  zip one-shot   Err(…"blocked.txt" could not be opened…)  a=FILE b=DIR  zc=ABSENT | Ok(done 2, failed 1)  zc=FILE
    /// dir  zip streamed   Err(…same…)                               a=FILE b=DIR  zc=ABSENT | Ok(done 2, failed 1)  zc=FILE
    /// dir  tar one-shot   Err("failed to unpack `…/blocked.txt`")   a=FILE b=DIR  zc=ABSENT | Ok(done 2, failed 1)  zc=FILE
    /// dir  tar streamed   Err("failed to unpack `…/blocked.txt`")   a=FILE b=DIR  zc=ABSENT | Ok(done 2, failed 1)  zc=FILE
    /// dir  7z  one-shot   Err(Io(Os { code: 5 / 21 }, …))           a=FILE b=DIR  zc=ABSENT | Ok(done 2, failed 1)  zc=FILE
    /// dir  7z  streamed   Err(Io(Os { code: 5 / 21 }, …))           a=FILE b=DIR  zc=ABSENT | Ok(done 2, failed 1)  zc=FILE
    /// ro   zip one-shot   Err(…could not be opened for writing…)    a=FILE b=USER zc=ABSENT | Ok(done 2, failed 1)  zc=FILE
    /// ro   zip streamed   Err(…same…)                               a=FILE b=USER zc=ABSENT | Ok(done 2, failed 1)  zc=FILE
    /// ro   tar one-shot   Ok(done 3)  <- OVERWRITES the read-only file, both OSes            | unchanged
    /// ro   tar streamed   Ok(done 3)  <- ditto                                               | unchanged
    /// ro   7z  one-shot   Err(Io(Os { code: 5 / 13 }, …))           a=FILE b=USER zc=ABSENT | Ok(done 2, failed 1)  zc=FILE
    /// ro   7z  streamed   Err(Io(Os { code: 5 / 13 }, …))           a=FILE b=USER zc=ABSENT | Ok(done 2, failed 1)  zc=FILE
    /// ```
    ///
    /// `zc.txt` — the entry *after* the blocker — is the whole assertion. It was `ABSENT` in ten of
    /// twelve cells before this ticket while the caller held one error string naming only `blocked.txt`;
    /// nothing in the returned `Result` distinguished "wrote nothing" from "wrote everything up to
    /// here", which is why this test reads the folder and not the verdict.
    ///
    /// # The two cells that behave differently, kept rather than smoothed over
    ///
    /// **tar over a read-only file does not fail at all — it replaces the file.** `unpack_in` unlinks
    /// an existing name and recreates it (`tar-0.4.46/src/entry.rs:562-568`), and a read-only *file* is
    /// no barrier to unlinking it from a writable directory on either platform. So those two cells read
    /// `done 3` before and after. That is out of this ticket's scope (it is an overwrite policy
    /// question, not a partial-extraction one) and is asserted here as the standing measurement rather
    /// than hidden behind a uniform expectation — the legs were assumed to share behaviour once
    /// already, in CPE-1938, and did not.
    ///
    /// **It is now written down where the user can read it, which round 2 caught it not being.**
    /// `src/docs/explorer-archives.md` listed "a read-only file at the entry's name" under *Failed* —
    /// true of zip and 7z, false here — so the one format that silently destroys the file was the one
    /// the docs promised would refuse to. The exception is spelled out in that page's Refused/Failed
    /// section. Keeping the behaviour undocumented was the part that was not defensible; keeping the
    /// behaviour is.
    ///
    /// # What happens to what was already written: nothing, deliberately
    ///
    /// The alternative the ticket offered was abort-and-roll-back. It is refused here: the destination
    /// is a folder the **user** chose and this very fixture proves it can already hold the user's own
    /// files — `blocked.txt` is one. Nothing distinguishes a file this run wrote from a file that was
    /// there, so "roll back" would mean deleting on a guess, which is CPE-1972's rule verbatim (*an
    /// absence of information must never license a delete*). The mess was never the leftover files; it
    /// was that nothing enumerated them. With `done`/`failed`/`errors` filled in, a half-extraction is
    /// a described state instead of an unknown one, and the files the user asked for are still there.
    #[test]
    fn cpe1935_a_blocked_entry_never_takes_the_run_down() {
        let names = M1935_NAMES;
        let d = scratch("cpe1935-blocked");
        let never = AtomicBool::new(false);
        let mut cells = 0;

        for (occ, leg) in ["dir", "ro"].iter().flat_map(|o| {
            ["zip one-shot", "zip streamed", "tar one-shot", "tar streamed", "7z one-shot", "7z streamed"]
                .into_iter()
                .map(move |l| (*o, l))
        }) {
            let base = d.join(format!("{occ}-{}", leg.replace(' ', "_")));
            fs::create_dir_all(&base).unwrap();
            let dest = base.join("out");
            fs::create_dir_all(&dest).unwrap();
            let blocker = dest.join("blocked.txt");
            if occ == "dir" {
                fs::create_dir_all(&blocker).unwrap(); // occupant: a plain DIRECTORY
            } else {
                // occupant: an existing READ-ONLY file — the ticket's headline case.
                fs::write(&blocker, b"USER FILE").unwrap();
                let mut perm = fs::metadata(&blocker).unwrap().permissions();
                perm.set_readonly(true);
                fs::set_permissions(&blocker, perm).unwrap();
                // Is the deny real here? Running as root ignores it and a green cell would be vacuous —
                // a loud skip, never a silent pass (Evidence Rules).
                if fs::OpenOptions::new().write(true).open(&blocker).is_ok() {
                    let _ = writeln!(
                        std::io::stderr(),
                        "cpe1935_a_blocked_entry_never_takes_the_run_down: SKIPPED cell [ro {leg}] — a \
                         read-only file is still writable here (running as root?)"
                    );
                    continue;
                }
            }
            let ds = dest.to_string_lossy().to_string();

            let outcome: Result<ArchiveReport, String> = match leg {
                "zip one-shot" => {
                    let a = m1935_zip(&base, &names);
                    extract_archive(&a.to_string_lossy(), &ds).map(|o| o.report)
                }
                "zip streamed" => {
                    let a = m1935_zip(&base, &names);
                    extract_archive_streamed(&a.to_string_lossy(), &ds, &never, |_| {})
                }
                "tar one-shot" => {
                    let a = m1935_tar(&base, &names);
                    extract_archive(&a.to_string_lossy(), &ds).map(|o| o.report)
                }
                "tar streamed" => {
                    let a = m1935_tar(&base, &names);
                    extract_archive_streamed(&a.to_string_lossy(), &ds, &never, |_| {})
                }
                "7z one-shot" => {
                    let a = m1935_7z(&base, &names);
                    extract_archive(&a.to_string_lossy(), &ds).map(|o| o.report)
                }
                _ => {
                    let a = m1935_7z(&base, &names);
                    extract_archive_streamed(&a.to_string_lossy(), &ds, &never, |_| {})
                }
            };
            let disk = m1935_state(&dest, &names);
            let cell = format!("[{occ} {leg}]");

            // EVIDENCE FIRST, and on the FILESYSTEM: the entry after the blocker must be there. This is
            // the assertion the ticket asked for — a verdict-only check passes a half-extraction.
            assert_eq!(
                fs::read(dest.join("zc.txt")).ok().as_deref(),
                Some(&b"PAYLOAD zc.txt"[..]),
                "{cell} the entry AFTER the blocked one was never written — one unwritable entry took \
                 the rest of the archive down with it. outcome={outcome:?} DISK: {disk}"
            );
            assert_eq!(
                fs::read(dest.join("a.txt")).ok().as_deref(),
                Some(&b"PAYLOAD a.txt"[..]),
                "{cell} the entry BEFORE the blocked one is missing. DISK: {disk}"
            );

            let report = outcome.unwrap_or_else(|e| {
                panic!("{cell} one unwritable entry must not make the whole run an error: {e}\nDISK: {disk}")
            });

            // `tar` over a read-only FILE replaces it rather than failing — see this test's doc. Every
            // other cell must report the blocked entry as a failure, not a skip and not a success.
            let tar_overwrites = occ == "ro" && leg.starts_with("tar");
            if tar_overwrites {
                assert_eq!(report.done, 3, "{cell} tar unlinks and recreates: DISK {disk}");
                assert_eq!(report.failed, 0, "{cell} nothing failed on this leg: {report:?}");
            } else {
                assert_eq!(
                    (report.done, report.failed, report.skipped),
                    (2, 1, 0),
                    "{cell} the blocked entry must be ONE counted failure — not a skip (nobody chose \
                     it), not silence, not the whole run. DISK: {disk}"
                );
                let line = report
                    .errors
                    .iter()
                    .find(|e| e.starts_with("blocked.txt:"))
                    .unwrap_or_else(|| panic!("{cell} no reason names the entry: {:?}", report.errors));
                assert!(
                    line.contains(RETRY_HELPS),
                    "{cell} an unwritable occupant clears when the user clears it, so the reason must \
                     say re-running helps: {line}"
                );
                // The occupant is the user's, and nothing here may take it: not the write (it was
                // refused) and not a roll-back (there is none — see this test's doc).
                if occ == "dir" {
                    assert!(
                        fs::symlink_metadata(&blocker).map(|m| m.is_dir()).unwrap_or(false),
                        "{cell} the directory occupying the entry's name was disturbed. DISK: {disk}"
                    );
                } else {
                    assert_eq!(
                        fs::read(&blocker).ok().as_deref(),
                        Some(&b"USER FILE"[..]),
                        "{cell} the user's read-only file was overwritten. DISK: {disk}"
                    );
                }
            }
            cells += 1;

            // Clear the read-only bit so the scratch tree can actually be removed (CPE-1974: this
            // machine already carries 2,127 stray reparse points from tests that did not tidy up).
            if let Ok(m) = fs::metadata(&blocker) {
                let mut perm = m.permissions();
                #[allow(clippy::permissions_set_readonly_false)]
                perm.set_readonly(false);
                let _ = fs::set_permissions(&blocker, perm);
            }
        }
        assert!(cells >= 6, "only {cells} cells ran — the fixture staged nothing on most legs");
        let _ = fs::remove_dir_all(&d);
    }

    /// **CPE-1935 — "will re-running help?" is decided by the error's KIND, and it reaches the user.**
    ///
    /// The ticket asked for revert's transient/permanent distinction on this leg, *"telling the user
    /// which decides whether re-running is worth anything"*. Two halves, and this pins both: the
    /// classification ([`EntryFailure::from_write_error`]) and the sentence
    /// ([`ArchiveReport::fail`] appending [`RETRY_HELPS`] / [`RETRY_DOES_NOT_HELP`] from the flag rather
    /// than from the message's wording).
    ///
    /// **This test exists because the CPE-1929 pair said it had to.** Sabotage A — forcing the predicate
    /// open with `true || !matches!(..)`, so every failure claims to be retryable — left the whole
    /// `--lib` suite at **2434 passed / 0 failed**, i.e. nothing could tell the two answers apart and
    /// the classifier read as covered while being unreachable from any assertion. The end-to-end half is
    /// `cpe1935_a_corrupt_entry_fails_permanently_while_its_neighbours_land`, which drives a real
    /// `ErrorKind::InvalidInput` out of `zip`'s CRC check instead of constructing one.
    #[test]
    fn cpe1935_a_write_failure_says_whether_re_running_helps() {
        use std::io::ErrorKind;
        // The archive's own bytes are wrong — the same read produces the same answer next time.
        for kind in [ErrorKind::InvalidData, ErrorKind::UnexpectedEof, ErrorKind::InvalidInput] {
            assert!(
                !EntryFailure::from_write_error("x", &std::io::Error::from(kind)).retryable,
                "{kind:?} is the archive's fault, and telling the user to try again wastes their time"
            );
        }
        // The destination said no — the user can change that and run it again.
        for kind in [
            ErrorKind::PermissionDenied,
            ErrorKind::AlreadyExists,
            ErrorKind::NotFound,
            ErrorKind::Other,
        ] {
            assert!(
                EntryFailure::from_write_error("x", &std::io::Error::from(kind)).retryable,
                "{kind:?} is the destination refusing, and a user who is told it is hopeless loses a \
                 file they could have had"
            );
        }
        // ...and the two must READ differently, from the flag rather than from the sentence's wording.
        let mut report = ArchiveReport::default();
        report.fail("fixable.txt", &EntryFailure::from_write_error("no", &std::io::Error::from(ErrorKind::PermissionDenied)));
        report.fail("broken.txt", &EntryFailure::from_write_error("no", &std::io::Error::from(ErrorKind::InvalidData)));
        assert_eq!(report.failed, 2);
        assert!(report.errors[0].contains(RETRY_HELPS), "{:?}", report.errors[0]);
        assert!(report.errors[1].contains(RETRY_DOES_NOT_HELP), "{:?}", report.errors[1]);
        assert!(
            !report.errors[0].contains(RETRY_DOES_NOT_HELP) && !report.errors[1].contains(RETRY_HELPS),
            "the two next-step sentences must be distinguishable, not one a substring of the other: {:?}",
            report.errors
        );
    }

    /// **The failure's own sentence and its next-step clause do not run together** (round 2 nit).
    ///
    /// Round 1 joined the two with a bare space, and neither of the two commonest `why` strings ends in
    /// a terminator, so the panel showed ``…\blocked.txt` The rest of the archive was extracted…`` and
    /// `…(os error 5) The rest of the archive…`. Both of those exact shapes are cases here, taken from
    /// the messages the tar and zip legs actually produce rather than invented.
    #[test]
    fn cpe1935_a_failure_reason_and_its_next_step_are_two_sentences() {
        // `unpack_in`'s wrapper — a backtick-terminated path, the shape the tar legs hand up.
        let tar_ish = "failed to unpack `C:\\out\\blocked.txt`";
        assert_eq!(
            join_failure_sentence(tar_ish, RETRY_HELPS),
            format!("{tar_ish}. {RETRY_HELPS}"),
            "a reason that does not end itself must be given a full stop"
        );
        // A bare OS string — ends in its error code, not in punctuation.
        let os_ish = "Access is denied. (os error 5)";
        assert!(
            join_failure_sentence(os_ish, RETRY_HELPS).starts_with(&format!("{os_ish}. ")),
            "got {}",
            join_failure_sentence(os_ish, RETRY_HELPS)
        );
        // A reason that IS already a sentence keeps its own punctuation and gains no second stop.
        for ended in ["the disk is full.", "is it mounted?", "no space left!", "the cause:"] {
            let joined = join_failure_sentence(ended, RETRY_DOES_NOT_HELP);
            assert_eq!(joined, format!("{ended} {RETRY_DOES_NOT_HELP}"), "over-punctuated {ended:?}");
        }
        // Trailing whitespace is not a terminator, and an empty reason contributes nothing.
        assert_eq!(join_failure_sentence("boom  ", RETRY_HELPS), format!("boom. {RETRY_HELPS}"));
        assert_eq!(join_failure_sentence("", RETRY_HELPS), RETRY_HELPS);

        // And the whole way through `fail`, which is what the panel actually reads.
        let mut report = ArchiveReport::default();
        report.fail("blocked.txt", &EntryFailure::retryable(tar_ish.to_string()));
        assert!(
            report.errors[0].contains("`. The rest of the archive"),
            "the recorded line still runs the two sentences together: {:?}",
            report.errors[0]
        );
    }

    /// **CPE-1935 — a corrupt entry is a PERMANENT per-entry failure, and its neighbours still land.**
    ///
    /// The end-to-end half of the classifier above, and the one that proves the `io::copy` conversion in
    /// [`extract_zip_archive_stream`] is real rather than constructed: the fixture is a STORED zip whose
    /// middle entry carries a deliberately wrong CRC, so `zip`'s own checksum check fails the read with
    /// a genuine [`std::io::ErrorKind::InvalidInput`] partway through the copy. Before this ticket that
    /// `?` ended the archive; the entry after it was never written.
    ///
    /// It is also the one shape where "extract it again" would be a lie, which is why the assertion is
    /// on [`RETRY_DOES_NOT_HELP`] specifically and not merely on `failed == 1`.
    #[test]
    fn cpe1935_a_corrupt_entry_fails_permanently_while_its_neighbours_land() {
        let d = scratch("cpe1935-corrupt");
        let ap = d.join("corrupt.zip");
        // `craft_zip_with_entry_name` builds one STORED entry with a correct CRC; the helper below
        // rebuilds the same bytes for three entries and poisons the middle one's checksum.
        fs::write(&ap, craft_zip_with_bad_crc_middle_entry()).unwrap();
        let dest = d.join("out");

        let report = extract_archive_streamed(
            &ap.to_string_lossy(),
            &dest.to_string_lossy(),
            &AtomicBool::new(false),
            |_| {},
        )
        .expect("a corrupt ENTRY is not a corrupt archive — the other entries are still readable");

        // Filesystem first: the entry after the corrupt one.
        assert_eq!(
            fs::read(dest.join("zc.txt")).ok().as_deref(),
            Some(&b"PAYLOAD zc.txt"[..]),
            "one entry with a bad checksum took the rest of the archive down: {report:?}"
        );
        assert_eq!(
            (report.done, report.failed, report.skipped),
            (2, 1, 0),
            "the corrupt entry must be one counted failure: {report:?}"
        );
        let line = report
            .errors
            .iter()
            .find(|e| e.starts_with("blocked.txt:"))
            .unwrap_or_else(|| panic!("no reason names the corrupt entry: {:?}", report.errors));
        assert!(
            line.contains(RETRY_DOES_NOT_HELP),
            "a bad checksum is in the archive, not on the disk — telling the user to try again would be \
             a lie: {line}"
        );
        let _ = fs::remove_dir_all(&d);
    }

    /// Overwrite every byte Rust's lexer would read as a comment or as a string/char literal with a
    /// space, keeping newlines **and the exact byte length**, so offsets into the result still index
    /// the original file. Returns the masked copy.
    ///
    /// `crates/server` had no Rust source stripper — `src/lib/rustSource.ts` is the TypeScript one, and
    /// CPE-1933 rule 2 says not to grow a fresh copy of the rules per scanner — so this is the crate's
    /// one shared masker. **Masking rather than deleting is load-bearing:** the guard below needs two
    /// byte *ranges* out of the same text it scans, and a stripper that removes bytes would force every
    /// offset to be mapped back.
    ///
    /// Three shapes it exists to survive, all present in this file:
    /// - a `/*` **inside** a `///` doc comment (line 2326's `*.rs` / `*.ts`). Line comments are consumed
    ///   before block comments, so it cannot open a phantom block comment — the exact CPE-1950 shape
    ///   where a `<<` inside a quoted string opened a phantom heredoc in two scanners at once.
    /// - a `'` that opens a **lifetime** (`&'a`), not a char literal. A `'` is treated as a literal only
    ///   when a closing `'` is actually there.
    /// - a code fragment quoted inside a **string literal** — which is what the guard below trips over
    ///   if only comments are stripped, because its own pattern list quotes the fragments it hunts.
    fn mask_rust_comments_and_literals(src: &str) -> String {
        fn blank(out: &mut [u8], from: usize, to: usize) {
            for byte in &mut out[from..to] {
                if *byte != b'\n' && *byte != b'\r' {
                    *byte = b' ';
                }
            }
        }
        fn is_ident(c: u8) -> bool {
            c.is_ascii_alphanumeric() || c == b'_'
        }
        let b = src.as_bytes();
        let n = b.len();
        let mut out = b.to_vec();
        let mut i = 0usize;
        while i < n {
            // Raw strings (`r"..."`, `r#"..."#`, `br##"..."##`) first: their embedded quotes and
            // backslashes are not escapes, so the plain-string arm would end them in the wrong place.
            // This file has none today; the arm is here so the first one added does not silently
            // corrupt the mask.
            let raw = if b[i] == b'r'
                && (i == 0 || !is_ident(b[i - 1]) || (b[i - 1] == b'b' && (i < 2 || !is_ident(b[i - 2]))))
            {
                let mut h = i + 1;
                while h < n && b[h] == b'#' {
                    h += 1;
                }
                (h < n && b[h] == b'"').then_some((h - i - 1, h + 1))
            } else {
                None
            };
            if let Some((hashes, body)) = raw {
                let start = i;
                let mut j = body;
                loop {
                    assert!(j < n, "unterminated raw string at byte {start} of archive.rs");
                    if b[j] == b'"' {
                        let mut k = j + 1;
                        let mut seen = 0usize;
                        while k < n && seen < hashes && b[k] == b'#' {
                            k += 1;
                            seen += 1;
                        }
                        if seen == hashes {
                            j = k;
                            break;
                        }
                    }
                    j += 1;
                }
                blank(&mut out, start, j);
                i = j;
                continue;
            }
            match b[i] {
                b'/' if i + 1 < n && b[i + 1] == b'/' => {
                    let start = i;
                    while i < n && b[i] != b'\n' {
                        i += 1;
                    }
                    blank(&mut out, start, i);
                }
                b'/' if i + 1 < n && b[i + 1] == b'*' => {
                    let start = i;
                    let mut depth = 1usize;
                    i += 2;
                    while i < n && depth > 0 {
                        if b[i] == b'/' && i + 1 < n && b[i + 1] == b'*' {
                            depth += 1;
                            i += 2;
                        } else if b[i] == b'*' && i + 1 < n && b[i + 1] == b'/' {
                            depth -= 1;
                            i += 2;
                        } else {
                            i += 1;
                        }
                    }
                    assert_eq!(depth, 0, "unterminated block comment at byte {start} of archive.rs");
                    blank(&mut out, start, i);
                }
                b'"' => {
                    let start = i;
                    i += 1;
                    loop {
                        assert!(i < n, "unterminated string literal at byte {start} of archive.rs");
                        match b[i] {
                            b'\\' => i = (i + 2).min(n),
                            b'"' => {
                                i += 1;
                                break;
                            }
                            _ => i += 1,
                        }
                    }
                    blank(&mut out, start, i);
                }
                b'\'' => {
                    // Char literal only if a closing quote is really there; otherwise it is a lifetime,
                    // which is ordinary code and has to stay visible to the scan.
                    let close = if i + 1 < n && b[i + 1] == b'\\' {
                        // Step over exactly the escape, then expect the close. Scanning for "the next
                        // `'`" instead would stop on the *escaped* quote of `'\''` and leave the real
                        // closing quote unmasked — caught by the masker's own test, not by review.
                        let mut k = i + 2;
                        if k + 1 < n && b[k] == b'u' && b[k + 1] == b'{' {
                            while k < n && b[k] != b'}' {
                                k += 1;
                            }
                            k += 1;
                        } else if b.get(k) == Some(&b'x') {
                            k += 3;
                        } else {
                            k += 1;
                        }
                        (k < n && b[k] == b'\'').then_some(k)
                    } else {
                        // One char, which may be several UTF-8 bytes.
                        let mut k = i + 1;
                        while k < n && b[k] >= 0x80 {
                            k += 1;
                        }
                        if k == i + 1 {
                            k += 1;
                        }
                        (k < n && b[k] == b'\'').then_some(k)
                    };
                    match close {
                        Some(end) => {
                            blank(&mut out, i, end + 1);
                            i = end + 1;
                        }
                        None => i += 1,
                    }
                }
                _ => i += 1,
            }
        }
        String::from_utf8(out).expect("the mask only writes ASCII spaces, and only over whole tokens")
    }

    /// True when `tokens` appear at `at`, in order, separated by nothing but ASCII whitespace.
    fn tokens_at(b: &[u8], at: usize, tokens: &[&str]) -> bool {
        let mut i = at;
        for t in tokens {
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            if i > b.len() || !b[i..].starts_with(t.as_bytes()) {
                return false;
            }
            i += t.len();
        }
        true
    }

    /// The masker keeps offsets, hides comments and literals, and leaves code alone.
    ///
    /// Without this leg the guard below could pass **vacuously** by masking the whole file — the failure
    /// mode a source-scanning test is most prone to and least likely to notice. Every case is a shape
    /// that actually occurs in `archive.rs`.
    ///
    /// Each expectation is a **template of the same byte length as its input**: `#` means "this byte
    /// must come back a space", anything else means "this byte must come back unchanged". Hand-counting
    /// runs of spaces in a quoted string is how the first draft of this test failed on three of its own
    /// seven cases; a template makes the length a checked property instead of an eyeballed one.
    #[test]
    fn the_rust_masker_hides_comments_and_literals_while_keeping_offsets() {
        let cases: [(&str, &str); 8] = [
            // A trailing comment — the shape a whole-line-comment filter walks straight past, and the
            // one CPE-1933 rule 2 calls out by name.
            ("let x = 1; // a.errors.push(", "===========#################"),
            // A `/*` inside a line comment must not open a block comment. This is archive.rs's own
            // line 2326 (`src-tauri/**/*.rs`) in miniature: a naive strip-comments-then-scan would
            // swallow everything to the next `*/`, which in this file is nowhere.
            ("// see src/**/*.rs\nlet y = 2;\n", "##################\n==========\n"),
            // A real block comment, nested.
            ("a /* b /* c */ d */ e", "==#################=="),
            // A code fragment quoted in a string literal — the defect that reddened CI, in miniature.
            ("let s = \".errors.push(\";", "========###############="),
            // An escaped quote inside a string does not end it.
            ("let s = \"a\\\"b\"; z", "========######==="),
            // Char literal masked, lifetime left visible — and the escaped-quote char literal, whose
            // closing quote is the SECOND `'` after the backslash.
            ("fn f<'a>(c: char) { let q = '\\''; }", "============================####==="),
            // A unicode escape runs past its own braces.
            ("let u = '\\u{2014}'; ok", "========##########===="),
            // A multi-byte char literal: all five of its bytes blanked, neither neighbour touched.
            ("let e = '—'; ok", "========#####===="),
        ];
        for (input, template) in cases {
            assert_eq!(
                template.len(),
                input.len(),
                "the template for {input:?} is {} bytes against the input's {}",
                template.len(),
                input.len()
            );
            let want = String::from_utf8(
                input
                    .bytes()
                    .zip(template.bytes())
                    .map(|(c, t)| if t == b'#' { b' ' } else { c })
                    .collect::<Vec<u8>>(),
            )
            .expect("every kept byte in these cases is ASCII");
            let got = mask_rust_comments_and_literals(input);
            assert_eq!(got.len(), input.len(), "the mask changed the byte length of {input:?}");
            assert_eq!(got, want, "masking {input:?}");
        }
    }

    /// **CPE-1935 — the count and the reason can only be grown together, derived from the source.**
    ///
    /// [`ArchiveReport`]'s doc claimed from CPE-1775 to CPE-1935 that a test named
    /// `skipped_count_matches_the_recorded_reasons_on_every_streamed_skip_path` enforced this.
    /// **No commit in this repository has ever contained a test of that name**
    /// (`git log --all -S` on the function signature returns nothing). A green suite standing next to a
    /// claim about a test that is not there is CPE-1933's defect class, and this ticket added a second
    /// count (`failed`) to the same invariant, so the claim is replaced by a derivation.
    ///
    /// It reads **this file** and requires that `.skipped +=`, `.failed +=` and `.errors.push(` —
    /// **on any receiver** — appear only inside [`ArchiveReport::skip`] and [`ArchiveReport::fail`],
    /// which is what makes "the number and the list describe the same thing" a property of the code
    /// rather than a habit.
    ///
    /// **Round 2 fixed three defects in the first version of this guard, all three of the
    /// "did-not-run reads as found-nothing" family CLAUDE.md names.**
    /// - It matched `self.skipped +=` and friends *with the receiver spelled out*. Every extractor leg
    ///   holds a local `report`, not a `self`, so the mutation the guard existed to catch —
    ///   `report.failed += 1;` in `extract_zip_archive_stream` — could not be expressed in the shape it
    ///   scanned for. The Reviewer planted exactly that, plus a `report.errors.push(...)`, and the test
    ///   stayed **green**. The patterns are receiver-agnostic now.
    /// - It ended each helper span at `"\n    }\n"`, which occurs **0 times** in a CRLF checkout (the
    ///   CRLF spelling occurs 230), and fell back to `src.len()` — so on Windows every byte after
    ///   `fn skip` counted as "inside the helper" and roughly two thirds of the file, all four
    ///   extractors included, was silently exempt. Spans are now brace-matched over the mask, and a span
    ///   that cannot be located is a **panic**, never a fallback.
    /// - Stripping only `//` left its own pattern list — a string literal quoting the fragments — being
    ///   read as code, so the guard reported *itself* as the offender on any LF checkout. That is what
    ///   reddened this PR's Linux and macOS CI jobs. [`mask_rust_comments_and_literals`] hides literals
    ///   as well as comments.
    ///
    /// **Red-proofed after the rewrite**, both directions, on a real LF checkout and on the CRLF
    /// working tree:
    /// - the Reviewer's sabotage — `report.failed += 1;` and `report.errors.push("…".to_string());`
    ///   inside [`extract_zip_archive_stream`] — now **fails with 2 offenders**, naming both lines.
    /// - unsabotaged, the scan finds the four legitimate sites and no offenders. The `inside` count is
    ///   asserted too, so an over-eager mask cannot make this pass by finding nothing anywhere.
    #[test]
    fn archive_report_counts_and_reasons_can_only_be_grown_together() {
        let src = include_str!("archive.rs");
        let masked = mask_rust_comments_and_literals(src);
        assert_eq!(masked.len(), src.len(), "the mask must keep byte offsets usable");
        let mb = masked.as_bytes();

        // Each helper's body, brace-matched over the MASK (so a brace inside a string or a comment does
        // not count) and located by signature rather than by line number or by a line-ending-dependent
        // pattern. A span that cannot be found is fatal: exempting the rest of the file by falling back
        // to `src.len()` is precisely how this guard went blind on Windows the first time round.
        let body_span = |sig: &str| -> (usize, usize) {
            let at = masked.find(sig).unwrap_or_else(|| {
                panic!("`{sig}` is gone from archive.rs — this guard cannot say what is inside a helper \
                        that no longer exists, so it fails rather than exempting nothing")
            });
            let open = masked[at..]
                .find('{')
                .map(|o| at + o)
                .unwrap_or_else(|| panic!("no `{{` after `{sig}` — cannot locate its body"));
            let mut depth = 0usize;
            for (k, byte) in mb.iter().enumerate().skip(open) {
                match byte {
                    b'{' => depth += 1,
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            return (open, k);
                        }
                    }
                    _ => {}
                }
            }
            panic!("`{sig}`'s body has no matching `}}` — the span is unknown, so this guard fails \
                    rather than guessing at what it covers")
        };
        let skip = body_span("fn skip(&mut self, name: &str, reason: &str)");
        let fail = body_span("fn fail(&mut self, name: &str, f: &EntryFailure)");

        // Receiver-agnostic: `self.`, `report.`, `r.`, a field, a deref — anything ending in one of
        // these mutations counts. Whitespace between the tokens is allowed so a rustfmt line break
        // cannot hide a site.
        let patterns: [&[&str]; 3] = [&[".skipped", "+="], &[".failed", "+="], &[".errors", ".push", "("]];

        // Line starts, for a message that names a place a human can open.
        let line_of = |off: usize| src[..off].matches('\n').count() + 1;

        let (mut offenders, mut inside_skip, mut inside_fail) = (Vec::<String>::new(), 0usize, 0usize);
        for at in 0..mb.len() {
            // Every pattern starts with `.`; skipping the rest turns a 700 KB × 3-pattern sweep from
            // ten seconds of debug-build CI time into a fraction of one.
            if mb[at] != b'.' {
                continue;
            }
            let Some(pattern) = patterns.iter().find(|p| tokens_at(mb, at, p)) else { continue };
            if at >= skip.0 && at <= skip.1 {
                inside_skip += 1;
            } else if at >= fail.0 && at <= fail.1 {
                inside_fail += 1;
            } else {
                let line = line_of(at);
                let text = src.lines().nth(line - 1).unwrap_or("").trim();
                offenders.push(format!("  archive.rs:{line}  {}  in: {text}", pattern.concat()));
            }
        }

        assert!(
            offenders.is_empty(),
            "an ArchiveReport count or reason is grown outside `ArchiveReport::skip`/`fail`, so the \
             count the user reads and the list of reasons behind it can disagree. Route it through the \
             helper:\n{}",
            offenders.join("\n")
        );
        // Anti-vacuity (CPE-1932): a mask that blanked too much, or spans that swallowed the file,
        // would make the sweep above find nothing anywhere and pass. Both helpers must still be seen
        // doing both halves of the record.
        assert!(
            inside_skip >= 2 && inside_fail >= 2,
            "this guard scanned itself into silence: `skip` matched {inside_skip} of its 2 sites and \
             `fail` {inside_fail} of its 2. Something is wrong with the mask or the spans, not with the \
             code being guarded"
        );
    }
}
