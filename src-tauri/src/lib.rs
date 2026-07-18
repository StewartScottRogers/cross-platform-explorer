use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Live provider API-key verification + catalog egress for the AI Console sidecar (CPE-347/369/376).
/// Only compiled with the platform: without it nothing calls these, so the module would be dead
/// code under `-D warnings` (its pure logic is still unit-tested under the feature).
/// Pure window-geometry resolver for the CLI launch options (CPE-598) — core feature, always compiled.
mod geometry;
#[cfg(feature = "sidecar-platform")]
mod keyverify;
/// Host-brokered forge API egress for the repos sidecar (CPE-433). Same rationale as `keyverify`:
/// feature-gated, pure allow-list/URL-builder/SSRF core unit-tested under the feature.
#[cfg(feature = "sidecar-platform")]
mod forge_egress;
/// Host-brokered model-list egress for the AI Console (CPE-447) — allow-listed reseller `/models`
/// fetch on the sidecar's behalf; same feature-gating + no-SSRF rationale as `keyverify`.
#[cfg(feature = "sidecar-platform")]
mod models_egress;

/// Agent Board backend (CPE-520): read the repo's `Tickets/` folders as Kanban cards + move a card
/// between columns. Pure card/frontmatter logic lives here; the commands below do the file I/O.
mod ticket_board;

/// Read every ticket under `<root>/Tickets/{Backlog,Doing,Blocked,Deferred,Done}/CPE-*.md` into board
/// cards (CPE-520). Read-only; a malformed file is skipped, never fails the listing.
#[tauri::command]
fn board_cards(root: String) -> Vec<ticket_board::Card> {
    let tickets = std::path::Path::new(&root).join("Tickets");
    let mut cards = Vec::new();
    for col in ticket_board::COLUMNS {
        let Ok(entries) = std::fs::read_dir(tickets.join(col)) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if !name.starts_with("CPE-") || !name.ends_with(".md") {
                continue;
            }
            if let Ok(md) = std::fs::read_to_string(&p) {
                if let Some(card) = ticket_board::card_from(&md, col) {
                    cards.push(card);
                }
            }
        }
    }
    cards
}

/// Find the nearest project root at/above `start` — the closest ancestor dir with a `Tickets/` folder —
/// so the Agent Board can auto-open the project you're inside (CPE-554). `None` if none is found.
#[tauri::command]
fn find_project_root(start: String) -> Option<String> {
    ticket_board::nearest_project_root(std::path::Path::new(&start))
        .map(|p| p.to_string_lossy().into_owned())
}

/// Move ticket `id` to `to_column` (CPE-520): rewrite its `status:` frontmatter to match, then move the
/// file into that folder. The only writer. Refuses an unknown id/column and never clobbers an existing
/// file. A move to the current column is a no-op.
#[tauri::command]
fn board_move(root: String, id: String, to_column: String) -> Result<(), String> {
    let folder =
        ticket_board::folder_for_column(&to_column).ok_or_else(|| format!("unknown column '{to_column}'"))?;
    let status = ticket_board::status_for_column(&to_column).unwrap_or(folder);
    let tickets = std::path::Path::new(&root).join("Tickets");

    // Locate the ticket file: `<id>_*.md` in one of the columns.
    let prefix = format!("{id}_");
    let mut found: Option<(std::path::PathBuf, &'static str)> = None;
    for col in ticket_board::COLUMNS {
        let Ok(entries) = std::fs::read_dir(tickets.join(col)) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if name.starts_with(&prefix) && name.ends_with(".md") {
                found = Some((p, col));
                break;
            }
        }
        if found.is_some() {
            break;
        }
    }
    let (src, cur_col) = found.ok_or_else(|| format!("ticket {id} not found on the board"))?;
    if cur_col.eq_ignore_ascii_case(&to_column) {
        return Ok(()); // already there
    }

    let file_name = src.file_name().ok_or("bad source path")?.to_owned();
    let dest_dir = tickets.join(folder);
    std::fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;
    let dest = dest_dir.join(&file_name);
    if dest.exists() {
        return Err(format!("a ticket file already exists at {}", dest.display()));
    }
    let md = std::fs::read_to_string(&src).map_err(|e| e.to_string())?;
    std::fs::write(&src, ticket_board::set_status(&md, status)).map_err(|e| e.to_string())?;
    std::fs::rename(&src, &dest).map_err(|e| e.to_string())?;
    Ok(())
}

/// Collect archived Done tickets — those in **subdirectories** of `Tickets/Done/` (the dated
/// `YYYY/QN/…` folders `/ticketing-organize` produces). Top-level Done files are "recent" and are
/// returned by `board_cards`; anything nested is archived (CPE-531). Recursive.
fn collect_archived(dir: &std::path::Path, top_level: bool, out: &mut Vec<ticket_board::Card>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_archived(&p, false, out);
        } else if !top_level {
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if name.starts_with("CPE-") && name.ends_with(".md") {
                if let Ok(md) = std::fs::read_to_string(&p) {
                    if let Some(card) = ticket_board::card_from(&md, "Done") {
                        out.push(card);
                    }
                }
            }
        }
    }
}

/// The archived Done tickets (in dated `Done/**` subfolders) for the board's "show archived" affordance
/// (CPE-531). Kept separate from `board_cards` so the default board stays fast as Done grows.
#[tauri::command]
fn board_archived(root: String) -> Vec<ticket_board::Card> {
    let done = std::path::Path::new(&root).join("Tickets").join("Done");
    let mut out = Vec::new();
    collect_archived(&done, true, &mut out);
    out
}

/// List the repo's epics for the board's epic-organized view (CPE-530): active/proposed epics from
/// `Tickets/Epics/` + closed epics from `Tickets/Done/` (top level), each `epic`-tagged. Read-only.
#[tauri::command]
fn board_epics(root: String) -> Vec<ticket_board::Epic> {
    let tickets = std::path::Path::new(&root).join("Tickets");
    let mut epics = Vec::new();
    for sub in ["Epics", "Done"] {
        let Ok(entries) = std::fs::read_dir(tickets.join(sub)) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if !name.starts_with("CPE-") || !name.ends_with(".md") {
                continue;
            }
            if let Ok(md) = std::fs::read_to_string(&p) {
                if let Some(epic) = ticket_board::epic_from(&md) {
                    epics.push(epic);
                }
            }
        }
    }
    epics
}

/// Find a ticket's file `<id>_*.md` across the board columns.
fn find_ticket_file(root: &str, id: &str) -> Option<std::path::PathBuf> {
    let tickets = std::path::Path::new(root).join("Tickets");
    let prefix = format!("{id}_");
    for col in ticket_board::COLUMNS {
        let Ok(entries) = std::fs::read_dir(tickets.join(col)) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if name.starts_with(&prefix) && name.ends_with(".md") {
                return Some(p);
            }
        }
    }
    None
}

/// Toggle the `review` tag on ticket `id` (CPE-523) — drives the board's virtual Review lane.
#[tauri::command]
fn board_review(root: String, id: String, on: bool) -> Result<(), String> {
    let path = find_ticket_file(&root, &id).ok_or_else(|| format!("ticket {id} not found"))?;
    let md = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    std::fs::write(&path, ticket_board::set_review(&md, on)).map_err(|e| e.to_string())
}

/// Append a finding note to ticket `id` (CPE-523) — the affordance a dispatched agent (or the UI) uses
/// to record progress on a card.
#[tauri::command]
fn board_note(root: String, id: String, note: String) -> Result<(), String> {
    let path = find_ticket_file(&root, &id).ok_or_else(|| format!("ticket {id} not found"))?;
    let md = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    std::fs::write(&path, ticket_board::append_finding(&md, &note)).map_err(|e| e.to_string())
}

/// The workbench's view of a folder (CPE-526/535): whether it's a git repo, the branch, and the diff.
#[derive(serde::Serialize)]
struct WorkbenchDiff {
    is_repo: bool,
    branch: Option<String>,
    diff: String,
}

/// `git diff` (working tree vs HEAD) in `root` for the integrated workbench, with friendly edge cases
/// (CPE-535): a non-repo folder is a normal `is_repo:false` result (not an error), git-not-installed is
/// a distinct error, and an empty `root` is refused. An optional `path` limits it to one file. Read-only.
#[tauri::command]
fn workbench_diff(root: String, path: Option<String>) -> Result<WorkbenchDiff, String> {
    if root.trim().is_empty() {
        return Err("no-folder".to_string()); // opened on Home / no folder
    }
    // Is this a git work tree? Distinguishes not-a-repo (friendly) from git-missing (error).
    let inside = std::process::Command::new("git")
        .args(["-C", &root, "rev-parse", "--is-inside-work-tree"])
        .output()
        .map_err(|e| format!("git-missing: {e}"))?;
    if !inside.status.success() {
        return Ok(WorkbenchDiff { is_repo: false, branch: None, diff: String::new() });
    }
    let branch = std::process::Command::new("git")
        .args(["-C", &root, "rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());

    let mut args = vec!["-C".to_string(), root, "diff".to_string()];
    if let Some(p) = path.filter(|p| !p.is_empty()) {
        args.push("--".to_string());
        args.push(p);
    }
    let out = std::process::Command::new("git")
        .args(&args)
        .output()
        .map_err(|e| format!("Couldn't run git: {e}"))?;
    if out.status.success() {
        Ok(WorkbenchDiff { is_repo: true, branch, diff: String::from_utf8_lossy(&out.stdout).into_owned() })
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

#[derive(Serialize)]
pub struct DirEntry {
    name: String,
    path: String,
    is_dir: bool,
    size: u64,
    /// Last-modified time as milliseconds since the Unix epoch.
    /// `None` when the platform or filesystem does not report one.
    modified: Option<u64>,
    /// Lowercased file extension without the dot ("png"), empty for directories
    /// and extensionless files.
    extension: String,
    /// Hidden per the OS convention: the hidden attribute on Windows, a leading
    /// dot on POSIX.
    hidden: bool,
}

/// Per-item outcome of a bulk operation. Bulk file operations must NOT be
/// all-or-nothing and must not abort on the first failure: if 9 of 10 files
/// copy and one is locked, the user needs to know exactly which one failed.
#[derive(Serialize)]
pub struct OpResult {
    path: String,
    ok: bool,
    error: String,
}

impl OpResult {
    fn ok(path: &Path) -> Self {
        Self {
            path: path.to_string_lossy().to_string(),
            ok: true,
            error: String::new(),
        }
    }
    fn err(path: &Path, e: impl std::fmt::Display) -> Self {
        Self {
            path: path.to_string_lossy().to_string(),
            ok: false,
            error: e.to_string(),
        }
    }
}

/// Detailed metadata for the Properties dialog.
#[derive(Serialize)]
pub struct EntryInfo {
    name: String,
    path: String,
    is_dir: bool,
    size: u64,
    modified: Option<u64>,
    created: Option<u64>,
    readonly: bool,
    hidden: bool,
}

#[derive(Serialize)]
pub struct Place {
    /// Display name, e.g. "Documents" or "Local Disk (C:)".
    name: String,
    path: String,
    /// Logical kind, used by the UI to pick an icon:
    /// "desktop" | "documents" | "downloads" | "pictures" | "music" | "videos" | "drive" | "home".
    kind: String,
}

/// Convert a `SystemTime` into epoch milliseconds, if representable.
fn to_epoch_ms(t: SystemTime) -> Option<u64> {
    t.duration_since(UNIX_EPOCH).ok().map(|d| d.as_millis() as u64)
}

/// Lowercased extension without the dot; empty when there is none.
fn extension_of(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default()
}

/// Hidden per OS convention: the FILE_ATTRIBUTE_HIDDEN bit on Windows,
/// a leading dot on POSIX.
fn is_hidden(path: &Path, meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
        if meta.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0 {
            return true;
        }
    }
    #[cfg(not(windows))]
    {
        let _ = meta;
    }
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with('.'))
        .unwrap_or(false)
}

/// Would moving/copying `src` into `dest` put a directory inside itself?
/// Copying a folder into its own descendant recurses forever and shreds data —
/// this must be impossible, not merely discouraged.
fn is_self_or_descendant(src: &Path, dest: &Path) -> bool {
    let src = src.canonicalize();
    let dest = dest.canonicalize();
    match (src, dest) {
        (Ok(s), Ok(d)) => d == s || d.starts_with(&s),
        // If either path can't be canonicalized we cannot prove it is safe,
        // so refuse rather than risk it.
        _ => false,
    }
}

/// Pick a non-colliding name in `dir`, Explorer-style:
/// "report.txt" -> "report - Copy.txt" -> "report - Copy (2).txt".
/// We never overwrite an existing file — silent overwrite is data loss.
fn unique_target(dir: &Path, file_name: &str) -> PathBuf {
    let candidate = dir.join(file_name);
    if !candidate.exists() {
        return candidate;
    }

    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(file_name);
    let ext = path.extension().and_then(|e| e.to_str());

    let build = |suffix: &str| -> PathBuf {
        let name = match ext {
            Some(e) => format!("{stem}{suffix}.{e}"),
            None => format!("{stem}{suffix}"),
        };
        dir.join(name)
    };

    let first = build(" - Copy");
    if !first.exists() {
        return first;
    }
    for n in 2..10_000 {
        let p = build(&format!(" - Copy ({n})"));
        if !p.exists() {
            return p;
        }
    }
    // Pathological fallback; effectively unreachable.
    dir.join(format!("{file_name}.{}", std::process::id()))
}

/// Recursively copy a directory tree.
fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// List the immediate children of `path`.
#[tauri::command]
fn list_dir(path: String) -> Result<Vec<DirEntry>, String> {
    let mut out = Vec::new();
    let read = fs::read_dir(&path).map_err(|e| format!("{path}: {e}"))?;
    for entry in read {
        // Skip entries we can't read rather than failing the whole listing.
        let Ok(entry) = entry else { continue };
        let Ok(meta) = entry.metadata() else { continue };

        let entry_path = entry.path();
        let is_dir = meta.is_dir();

        out.push(DirEntry {
            hidden: is_hidden(&entry_path, &meta),
            name: entry.file_name().to_string_lossy().to_string(),
            path: entry_path.to_string_lossy().to_string(),
            is_dir,
            size: if is_dir { 0 } else { meta.len() },
            modified: meta.modified().ok().and_then(to_epoch_ms),
            extension: if is_dir {
                String::new()
            } else {
                extension_of(&entry_path)
            },
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Mutating file operations (CPE-030)
//
// Safety rules that these all obey:
//   * Delete goes to the OS Recycle Bin / Trash. Permanent delete is a separate,
//     explicitly-requested command.
//   * Nothing is ever silently overwritten. Collisions either error (rename,
//     create) or auto-rename (paste), never clobber.
//   * A directory can never be copied or moved into itself or a descendant.
//   * Bulk operations report per-item results rather than aborting on the first
//     failure.
// ---------------------------------------------------------------------------

/// Create a new directory `name` inside `path`.
#[tauri::command]
fn create_dir(path: String, name: String) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Name cannot be empty".to_string());
    }
    let target = Path::new(&path).join(name);
    if target.exists() {
        return Err(format!("\"{name}\" already exists"));
    }
    fs::create_dir(&target).map_err(|e| e.to_string())?;
    Ok(target.to_string_lossy().to_string())
}

/// Create a new empty file `name` inside `path` (CPE-254). Mirrors `create_dir`:
/// `create_new` fails atomically rather than clobbering an existing file.
#[tauri::command]
fn create_file(path: String, name: String) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Name cannot be empty".to_string());
    }
    let target = Path::new(&path).join(name);
    if target.exists() {
        return Err(format!("\"{name}\" already exists"));
    }
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
        .map_err(|e| e.to_string())?;
    Ok(target.to_string_lossy().to_string())
}

/// Write UTF-8 text back to a file, replacing its contents — for the content
/// editor. Returns the new byte length.
#[tauri::command]
fn write_file_text(path: String, contents: String) -> Result<u64, String> {
    fs::write(&path, contents.as_bytes()).map_err(|e| e.to_string())?;
    Ok(contents.len() as u64)
}

/// One entry inside an archive, for the archive preview.
#[derive(Serialize)]
pub struct ArchiveEntry {
    name: String,
    size: u64,
    is_dir: bool,
}

/// List the entries of a ZIP archive without extracting it.
fn zip_entries(path: &str) -> Result<Vec<ArchiveEntry>, String> {
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

/// A single-file gzip (not a .tar.gz) has no directory. Report the decompressed
/// file as one entry: its name is the archive name minus `.gz`, and its size is
/// the gzip trailer's ISIZE (uncompressed length modulo 2^32).
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
    Ok(vec![ArchiveEntry {
        name,
        size,
        is_dir: false,
    }])
}

/// List the entries of a 7-Zip archive via sevenz-rust (CPE-110).
fn sevenz_entries(path: &str) -> Result<Vec<ArchiveEntry>, String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let len = file.metadata().map_err(|e| e.to_string())?.len();
    let archive = sevenz_rust::Archive::read(&mut file, len, &[]).map_err(|e| e.to_string())?;
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
                Err(_) => continue,
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
                    out.push(ArchiveEntry {
                        name: format!("{full}/"),
                        size: 0,
                        is_dir: true,
                    });
                    stack.push((full, d));
                }
                DirectoryEntry::File(f) => {
                    let full = if prefix.is_empty() {
                        f.identifier.clone()
                    } else {
                        format!("{prefix}/{}", f.identifier)
                    };
                    out.push(ArchiveEntry {
                        name: full,
                        size: f.size() as u64,
                        is_dir: false,
                    });
                }
            }
        }
    }
    Ok(out)
}

/// List an archive's entries without extracting it, for the preview pane.
/// Dispatches by extension: ZIP family (zip/jar/apk/war/ear/ipa/xpi), TAR,
/// gzip-compressed TAR (.tar.gz/.tgz), single-file gzip (.gz), 7-Zip, and ISO.
/// Reads only the archive directory, so it stays cheap even for large archives.
#[tauri::command]
fn read_archive_entries(path: String) -> Result<Vec<ArchiveEntry>, String> {
    let lower = path.to_lowercase();
    if lower.ends_with(".tar") {
        let file = fs::File::open(&path).map_err(|e| e.to_string())?;
        tar_entries(file)
    } else if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        let file = fs::File::open(&path).map_err(|e| e.to_string())?;
        tar_entries(flate2::read::GzDecoder::new(file))
    } else if lower.ends_with(".gz") {
        gzip_single_entry(&path)
    } else if lower.ends_with(".7z") {
        sevenz_entries(&path)
    } else if lower.ends_with(".iso") {
        iso_entries(&path)
    } else {
        zip_entries(&path)
    }
}

// ---------------------------------------------------------------------------
// Structured binary previews (CPE-210/214/215/216/218)
//
// `read_preview_info` returns a human-readable text summary of a binary file,
// dispatched by extension. The frontend renders it read-only in the preview
// pane (the "info" provider kind). Each helper reads the file itself, so a
// corrupt file yields an Err (the pane shows a "can't preview" note) rather than
// hanging.
// ---------------------------------------------------------------------------

/// Classic hex + ASCII dump of the first `max` bytes (CPE-214).
fn hex_dump(path: &str, max: usize) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let n = bytes.len().min(max);
    let mut out = String::new();
    for (i, chunk) in bytes[..n].chunks(16).enumerate() {
        let mut hex = String::new();
        let mut ascii = String::new();
        for (j, b) in chunk.iter().enumerate() {
            hex.push_str(&format!("{b:02x} "));
            if j == 7 {
                hex.push(' ');
            }
            ascii.push(if b.is_ascii_graphic() || *b == b' ' {
                *b as char
            } else {
                '.'
            });
        }
        out.push_str(&format!("{:08x}  {hex:<49}|{ascii}|\n", i * 16));
    }
    if bytes.len() > n {
        out.push_str(&format!(
            "\n… {} more bytes (showing first {n}).\n",
            bytes.len() - n
        ));
    }
    Ok(out)
}

/// Summary of a Windows PE image (EXE/DLL) via goblin (CPE-216).
fn pe_info(path: &str) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let pe = goblin::pe::PE::parse(&bytes).map_err(|e| e.to_string())?;
    let mut out = String::new();
    out.push_str(if pe.is_64 {
        "PE32+ image (64-bit)\n"
    } else {
        "PE32 image (32-bit)\n"
    });
    out.push_str(&format!(
        "Machine: 0x{:04x}\n",
        pe.header.coff_header.machine
    ));
    out.push_str(&format!("Entry point: 0x{:x}\n", pe.entry));
    out.push_str(&format!("Sections: {}\n", pe.sections.len()));
    out.push_str(&format!(
        "Imports: {} symbols from {} libraries\n",
        pe.imports.len(),
        pe.libraries.len()
    ));
    out.push_str("\nSections:\n");
    for s in &pe.sections {
        let name = String::from_utf8_lossy(&s.name);
        out.push_str(&format!(
            "  {:<9} vaddr 0x{:08x}  vsize {}\n",
            name.trim_end_matches('\0'),
            s.virtual_address,
            s.virtual_size
        ));
    }
    if !pe.libraries.is_empty() {
        out.push_str("\nLinked libraries:\n");
        for lib in &pe.libraries {
            out.push_str(&format!("  {lib}\n"));
        }
    }
    Ok(out)
}

/// Summary of a MIDI file via midly (CPE-210).
fn midi_info(path: &str) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let smf = midly::Smf::parse(&bytes).map_err(|e| e.to_string())?;
    let mut out = String::new();
    out.push_str(&format!("MIDI format: {:?}\n", smf.header.format));
    out.push_str(&format!("Timing: {:?}\n", smf.header.timing));
    out.push_str(&format!("Tracks: {}\n", smf.tracks.len()));
    let total: usize = smf.tracks.iter().map(|t| t.len()).sum();
    out.push_str(&format!("Total events: {total}\n\n"));
    for (i, t) in smf.tracks.iter().enumerate() {
        let mut name = String::new();
        for ev in t.iter() {
            if let midly::TrackEventKind::Meta(midly::MetaMessage::TrackName(n)) = ev.kind {
                name = String::from_utf8_lossy(n).to_string();
                break;
            }
        }
        let suffix = if name.is_empty() {
            String::new()
        } else {
            format!(" — {name}")
        };
        out.push_str(&format!("  Track {}: {} events{suffix}\n", i + 1, t.len()));
    }
    Ok(out)
}

/// Disassemble a WebAssembly binary to its text form (WAT) via wasmprinter,
/// capped so a huge module can't flood the pane (CPE-215).
fn wasm_info(path: &str, max: usize) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let wat = wasmprinter::print_bytes(&bytes).map_err(|e| e.to_string())?;
    if wat.len() > max {
        let mut cut = max;
        while cut > 0 && !wat.is_char_boundary(cut) {
            cut -= 1;
        }
        Ok(format!(
            "{}\n\n… truncated ({cut} of {} bytes shown).\n",
            &wat[..cut],
            wat.len()
        ))
    } else {
        Ok(wat)
    }
}

/// Summary of a .torrent's bencode metadata via serde_bencode (CPE-218).
fn torrent_info(path: &str) -> Result<String, String> {
    use serde_bencode::value::Value;
    use std::collections::HashMap;

    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let val: Value = serde_bencode::from_bytes(&bytes).map_err(|e| e.to_string())?;
    let Value::Dict(top) = &val else {
        return Err("Not a bencode dictionary".to_string());
    };
    let get = |d: &'_ HashMap<Vec<u8>, Value>, k: &str| -> Option<Value> { d.get(k.as_bytes()).cloned() };
    let as_str = |v: &Value| match v {
        Value::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
        _ => String::new(),
    };
    let as_int = |v: &Value| match v {
        Value::Int(i) => *i,
        _ => 0,
    };

    let mut out = String::new();
    if let Some(a) = get(top, "announce") {
        out.push_str(&format!("Announce: {}\n", as_str(&a)));
    }
    if let Some(Value::Dict(info)) = get(top, "info") {
        if let Some(n) = get(&info, "name") {
            out.push_str(&format!("Name: {}\n", as_str(&n)));
        }
        if let Some(pl) = get(&info, "piece length") {
            out.push_str(&format!("Piece length: {} bytes\n", as_int(&pl)));
        }
        match get(&info, "files") {
            Some(Value::List(files)) => {
                out.push_str(&format!("Files: {}\n", files.len()));
                let mut total = 0i64;
                for f in &files {
                    if let Value::Dict(fd) = f {
                        let len = get(fd, "length").map(|v| as_int(&v)).unwrap_or(0);
                        total += len;
                        let parts = match get(fd, "path") {
                            Some(Value::List(ps)) => {
                                ps.iter().map(as_str).collect::<Vec<_>>().join("/")
                            }
                            _ => String::new(),
                        };
                        out.push_str(&format!("  {parts} ({len} bytes)\n"));
                    }
                }
                out.push_str(&format!("Total size: {total} bytes\n"));
            }
            _ => {
                if let Some(l) = get(&info, "length") {
                    out.push_str(&format!("Size: {} bytes (single file)\n", as_int(&l)));
                }
            }
        }
    }
    Ok(out)
}

// ---- Document text extraction (CPE-070/071/072/077) ----

/// Decode the five predefined XML entities. Applied after tag stripping.
fn decode_xml_entities(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Strip XML/HTML tags to plain text, turning the given block/paragraph tags'
/// closing tags into newlines first. Good enough for a readable text preview of
/// office and ebook markup — not a full renderer.
fn strip_markup_to_text(markup: &str, para_tags: &[&str]) -> String {
    let mut s = markup.to_string();
    for t in para_tags {
        s = s.replace(&format!("</{t}>"), "\n");
    }
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    collapse_blank_lines(&decode_xml_entities(&out))
}

/// Collapse runs of 3+ newlines into 2 and trim, so stripped markup reads cleanly.
fn collapse_blank_lines(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut newlines = 0;
    for c in s.chars() {
        if c == '\n' {
            newlines += 1;
            if newlines <= 2 {
                out.push('\n');
            }
        } else if c == '\r' {
            // ignore
        } else {
            newlines = 0;
            out.push(c);
        }
    }
    out.trim().to_string()
}

/// Read one entry of a zip as UTF-8 text.
fn zip_read_text(path: &str, entry_name: &str) -> Result<String, String> {
    use std::io::Read;
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut entry = zip
        .by_name(entry_name)
        .map_err(|e| format!("{entry_name}: {e}"))?;
    let mut buf = String::new();
    entry.read_to_string(&mut buf).map_err(|e| e.to_string())?;
    Ok(buf)
}

/// Extract the body text of a DOCX (word/document.xml) (CPE-071).
fn docx_text(path: &str) -> Result<String, String> {
    let xml = zip_read_text(path, "word/document.xml")?;
    Ok(strip_markup_to_text(&xml, &["w:p"]))
}

/// Extract the body text of an ODT (content.xml) (CPE-072).
fn odt_text(path: &str) -> Result<String, String> {
    let xml = zip_read_text(path, "content.xml")?;
    Ok(strip_markup_to_text(&xml, &["text:p", "text:h"]))
}

/// Extract readable text from an EPUB's content documents in name order,
/// capped so a whole book can't flood the pane (CPE-077).
fn epub_text(path: &str) -> Result<String, String> {
    use std::io::Read;
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;

    let mut names: Vec<String> = Vec::new();
    for i in 0..zip.len() {
        if let Ok(entry) = zip.by_index(i) {
            let n = entry.name().to_string();
            let low = n.to_lowercase();
            if low.ends_with(".xhtml") || low.ends_with(".html") || low.ends_with(".htm") {
                names.push(n);
            }
        }
    }
    names.sort();

    let mut out = format!("EPUB — {} content document(s)\n\n", names.len());
    for n in &names {
        if out.len() > 128 * 1024 {
            out.push_str("\n… (truncated)\n");
            break;
        }
        if let Ok(mut entry) = zip.by_name(n) {
            let mut buf = String::new();
            if entry.read_to_string(&mut buf).is_ok() {
                let text = strip_markup_to_text(&buf, &["p", "h1", "h2", "h3", "h4", "div", "li", "br"]);
                if !text.trim().is_empty() {
                    out.push_str(text.trim());
                    out.push_str("\n\n");
                }
            }
        }
    }
    Ok(out)
}

/// Extract readable text from RTF: a small, dependency-free reader that drops
/// control words and the font/colour/style/info destinations, turning \par and
/// friends into newlines. Not a full RTF engine — enough for a text preview
/// (CPE-070).
fn rtf_text(path: &str) -> Result<String, String> {
    let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let bytes = raw.as_bytes();
    let mut out = String::new();
    let mut i = 0usize;
    let mut depth: i32 = 0;
    let mut skip_depth: i32 = -1; // depth of a destination group being skipped

    while i < bytes.len() {
        match bytes[i] {
            b'{' => {
                depth += 1;
                i += 1;
            }
            b'}' => {
                if skip_depth >= 0 && depth == skip_depth {
                    skip_depth = -1;
                }
                depth -= 1;
                i += 1;
            }
            b'\\' => {
                i += 1;
                if i >= bytes.len() {
                    break;
                }
                let n = bytes[i];
                if n == b'\'' && i + 2 < bytes.len() {
                    if skip_depth < 0 {
                        if let Ok(v) = u8::from_str_radix(&raw[i + 1..i + 3], 16) {
                            out.push(v as char);
                        }
                    }
                    i += 3;
                } else if n.is_ascii_alphabetic() {
                    let start = i;
                    while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                        i += 1;
                    }
                    let word = &raw[start..i];
                    // optional numeric parameter
                    if i < bytes.len() && (bytes[i] == b'-' || bytes[i].is_ascii_digit()) {
                        if bytes[i] == b'-' {
                            i += 1;
                        }
                        while i < bytes.len() && bytes[i].is_ascii_digit() {
                            i += 1;
                        }
                    }
                    // a single trailing space is part of the control word
                    if i < bytes.len() && bytes[i] == b' ' {
                        i += 1;
                    }
                    if skip_depth < 0 {
                        match word {
                            "par" | "line" | "sect" => out.push('\n'),
                            "tab" => out.push('\t'),
                            "fonttbl" | "colortbl" | "stylesheet" | "info" | "pict" | "object"
                            | "header" | "footer" | "generator" => skip_depth = depth,
                            _ => {}
                        }
                    }
                } else {
                    if skip_depth < 0 {
                        match n {
                            b'\\' | b'{' | b'}' => out.push(n as char),
                            b'~' => out.push(' '),
                            _ => {}
                        }
                    }
                    i += 1;
                }
            }
            b'\r' | b'\n' => i += 1,
            c => {
                if skip_depth < 0 && depth > 0 {
                    out.push(c as char);
                }
                i += 1;
            }
        }
    }
    Ok(collapse_blank_lines(&out))
}

// ---- Data formats (CPE-088/090/091) ----

/// Read-only summary of a SQLite database: its tables/views, each with a row
/// count and column list (CPE-088).
fn sqlite_info(path: &str) -> Result<String, String> {
    use rusqlite::{Connection, OpenFlags};
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;

    let items: Vec<(String, String)> = {
        let mut stmt = conn
            .prepare(
                "SELECT name, type FROM sqlite_master \
                 WHERE type IN ('table','view') AND name NOT LIKE 'sqlite_%' ORDER BY name",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| e.to_string())?;
        rows.filter_map(|r| r.ok()).collect()
    };

    let mut out = format!("SQLite database — {} table(s)/view(s)\n\n", items.len());
    for (name, kind) in &items {
        // Names come from sqlite_master; double-quote and escape for safety.
        let quoted = format!("\"{}\"", name.replace('"', "\"\""));
        let columns: Vec<String> = {
            let mut cols = Vec::new();
            if let Ok(mut stmt) = conn.prepare(&format!("PRAGMA table_info({quoted})")) {
                if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(1)) {
                    cols = rows.filter_map(|r| r.ok()).collect();
                }
            }
            cols
        };
        let detail = if kind == "table" {
            match conn.query_row(&format!("SELECT COUNT(*) FROM {quoted}"), [], |r| {
                r.get::<_, i64>(0)
            }) {
                Ok(c) => format!("{c} rows"),
                Err(_) => "unreadable".to_string(),
            }
        } else {
            "view".to_string()
        };
        out.push_str(&format!("{name} ({kind}) — {detail}\n"));
        if !columns.is_empty() {
            out.push_str(&format!("  columns: {}\n", columns.join(", ")));
        }
    }
    Ok(out)
}

/// Read-only text-grid preview of a spreadsheet workbook (XLSX/ODS) via
/// calamine: each sheet rendered as tab-separated rows, capped (CPE-090/091).
fn spreadsheet_info(path: &str) -> Result<String, String> {
    use calamine::{open_workbook_auto, Reader};
    const MAX_ROWS: usize = 100;
    const MAX_COLS: usize = 20;

    let mut wb = open_workbook_auto(path).map_err(|e| e.to_string())?;
    let names: Vec<String> = wb.sheet_names().iter().map(|s| s.to_string()).collect();
    let mut out = format!("Workbook — {} sheet(s): {}\n", names.len(), names.join(", "));

    for name in &names {
        if let Ok(range) = wb.worksheet_range(name) {
            let (h, w) = (range.height(), range.width());
            out.push_str(&format!("\n=== {name} ({h} x {w}) ===\n"));
            for row in range.rows().take(MAX_ROWS) {
                let cells: Vec<String> = row
                    .iter()
                    .take(MAX_COLS)
                    .map(|c| c.to_string())
                    .collect();
                out.push_str(&cells.join("\t"));
                out.push('\n');
            }
            if h > MAX_ROWS {
                out.push_str(&format!("… {} more rows\n", h - MAX_ROWS));
            }
        }
    }
    Ok(out)
}

/// Read-only schema summary of a Parquet file via the parquet crate's footer
/// metadata — no full column scan (CPE-089).
fn parquet_info(path: &str) -> Result<String, String> {
    use parquet::file::reader::{FileReader, SerializedFileReader};
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let reader = SerializedFileReader::new(file).map_err(|e| e.to_string())?;
    let meta = reader.metadata();
    let fmeta = meta.file_metadata();
    let schema = fmeta.schema_descr();
    let mut out = format!(
        "Parquet — {} rows, {} row group(s)\nCreated by: {}\n\nColumns ({}):\n",
        fmeta.num_rows(),
        meta.num_row_groups(),
        fmeta.created_by().unwrap_or("(unknown)"),
        schema.num_columns()
    );
    for i in 0..schema.num_columns() {
        let col = schema.column(i);
        out.push_str(&format!("  {} : {:?}\n", col.name(), col.physical_type()));
    }
    Ok(out)
}

/// Return a human-readable text summary of a binary file, dispatched by
/// extension. Rendered read-only by the preview pane's "info" provider.
#[tauri::command]
fn read_preview_info(path: String) -> Result<String, String> {
    let ext = extension_of(Path::new(&path));
    match ext.as_str() {
        "exe" | "dll" | "sys" | "efi" | "ocx" | "scr" | "cpl" => pe_info(&path),
        "torrent" => torrent_info(&path),
        "wasm" => wasm_info(&path, 256 * 1024),
        "mid" | "midi" => midi_info(&path),
        "rtf" => rtf_text(&path),
        "docx" => docx_text(&path),
        "odt" => odt_text(&path),
        "epub" => epub_text(&path),
        "sqlite" | "sqlite3" | "db" => sqlite_info(&path),
        "xlsx" | "xlsm" | "ods" => spreadsheet_info(&path),
        "parquet" => parquet_info(&path),
        // generic binary (.bin/.dat) and anything else routed here: hex dump
        _ => hex_dump(&path, 64 * 1024),
    }
}

/// Decode an image the webview can't render natively (TIFF, PSD) to a PNG
/// `data:` URL the <img> tag can show (CPE-099/101). PSD uses the psd crate's
/// flattened composite; TIFF uses the image crate. Capped by the source reader,
/// and errors (rather than hangs) on a corrupt file.
#[tauri::command]
fn read_image_data_url(path: String) -> Result<String, String> {
    use base64::Engine;
    use std::io::Cursor;

    let ext = extension_of(Path::new(&path));
    // Produce PNG bytes from the source format.
    let png: Vec<u8> = if ext == "psd" {
        let bytes = fs::read(&path).map_err(|e| e.to_string())?;
        let psd = psd::Psd::from_bytes(&bytes).map_err(|e| e.to_string())?;
        let rgba = psd.rgba();
        let buf = image::RgbaImage::from_raw(psd.width(), psd.height(), rgba)
            .ok_or("PSD pixel buffer size mismatch")?;
        let mut out = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(buf)
            .write_to(&mut out, image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        out.into_inner()
    } else {
        // TIFF (and any other image-crate-decodable format routed here)
        let img = image::open(&path).map_err(|e| e.to_string())?;
        let mut out = Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        out.into_inner()
    };

    let b64 = base64::engine::general_purpose::STANDARD.encode(&png);
    Ok(format!("data:image/png;base64,{b64}"))
}

/// Read a text file's contents for the preview pane, capped at `max_bytes` so a
/// huge file can never be slurped into memory. Errors (rather than truncating)
/// when the file is too large, unreadable, or not valid UTF-8 — the frontend
/// shows a "can't preview" state in that case.
#[tauri::command]
fn read_file_text(path: String, max_bytes: u64) -> Result<String, String> {
    let p = Path::new(&path);
    let meta = fs::metadata(p).map_err(|e| e.to_string())?;
    if meta.len() > max_bytes {
        return Err(format!(
            "File is too large to preview ({} bytes; limit {max_bytes}).",
            meta.len()
        ));
    }
    let bytes = fs::read(p).map_err(|e| e.to_string())?;
    String::from_utf8(bytes).map_err(|_| "File is not valid UTF-8 text.".to_string())
}

/// Rename a single entry in place. Returns the new path.
#[tauri::command]
fn rename_entry(path: String, new_name: String) -> Result<String, String> {
    let new_name = new_name.trim();
    if new_name.is_empty() {
        return Err("Name cannot be empty".to_string());
    }
    let src = Path::new(&path);
    let parent = src
        .parent()
        .ok_or_else(|| "Cannot rename a filesystem root".to_string())?;
    let target = parent.join(new_name);

    if target == src {
        return Ok(path.clone()); // no-op rename
    }
    if target.exists() {
        return Err(format!("\"{new_name}\" already exists"));
    }
    fs::rename(src, &target).map_err(|e| e.to_string())?;
    Ok(target.to_string_lossy().to_string())
}

/// Move entries to the OS Recycle Bin / Trash. Recoverable by the user.
#[tauri::command]
fn delete_to_trash(paths: Vec<String>) -> Vec<OpResult> {
    paths
        .iter()
        .map(|p| {
            let path = Path::new(p);
            match trash::delete(path) {
                Ok(()) => OpResult::ok(path),
                Err(e) => OpResult::err(path, e),
            }
        })
        .collect()
}

/// Can this platform restore items from the OS trash?
///
/// `trash::os_limited` (list + restore) is implemented on Windows and Linux but
/// NOT on macOS. The UI calls this so it can decide whether to offer undo-of-
/// delete at all. Offering an Undo that silently does nothing on one platform is
/// worse than not offering it — so we tell the truth instead of guessing.
#[tauri::command]
fn can_restore_from_trash() -> bool {
    cfg!(any(target_os = "windows", target_os = "linux"))
}

/// Restore previously-trashed items to their original paths.
#[cfg(any(target_os = "windows", target_os = "linux"))]
#[tauri::command]
fn restore_from_trash(paths: Vec<String>) -> Vec<OpResult> {
    use trash::os_limited::{list, restore_all};

    let all = match list() {
        Ok(v) => v,
        Err(e) => {
            return paths
                .iter()
                .map(|p| OpResult::err(Path::new(p), &e))
                .collect()
        }
    };

    let mut results = Vec::new();
    let mut to_restore = Vec::new();

    for p in &paths {
        let target = Path::new(p);

        // Never clobber: if something now occupies the original path, refuse
        // rather than overwrite it to satisfy an undo.
        if target.exists() {
            results.push(OpResult::err(
                target,
                "Something already exists at the original location",
            ));
            continue;
        }

        // Match the trashed item by the full path it was deleted from.
        let found = all
            .iter()
            .find(|item| item.original_parent.join(&item.name) == target);

        match found {
            Some(item) => {
                to_restore.push(item.clone());
                results.push(OpResult::ok(target));
            }
            None => results.push(OpResult::err(
                target,
                "Not found in the Recycle Bin — it may have been emptied",
            )),
        }
    }

    if !to_restore.is_empty() {
        if let Err(e) = restore_all(to_restore) {
            // The restore failed as a batch; report it against every item we
            // had intended to restore rather than falsely claiming success.
            return paths
                .iter()
                .map(|p| OpResult::err(Path::new(p), &e))
                .collect();
        }
    }

    results
}

/// macOS has no trash listing/restore API in the `trash` crate. Rather than
/// pretend, this returns a clear error — and the UI never reaches here, because
/// `can_restore_from_trash()` is false so delete is never pushed onto the undo
/// stack in the first place.
#[cfg(not(any(target_os = "windows", target_os = "linux")))]
#[tauri::command]
fn restore_from_trash(paths: Vec<String>) -> Vec<OpResult> {
    paths
        .iter()
        .map(|p| {
            OpResult::err(
                Path::new(p),
                "Restoring from the Trash isn't supported on this platform — open the Trash to recover it",
            )
        })
        .collect()
}

/// Permanently delete entries. Irreversible — the UI must confirm explicitly
/// before ever calling this.
#[tauri::command]
fn delete_permanent(paths: Vec<String>) -> Vec<OpResult> {
    paths
        .iter()
        .map(|p| {
            let path = Path::new(p);
            let result = if path.is_dir() {
                fs::remove_dir_all(path)
            } else {
                fs::remove_file(path)
            };
            match result {
                Ok(()) => OpResult::ok(path),
                Err(e) => OpResult::err(path, e),
            }
        })
        .collect()
}

/// Copy entries into `dest`, auto-renaming on collision.
#[tauri::command]
fn copy_entries(paths: Vec<String>, dest: String) -> Vec<OpResult> {
    let dest_dir = PathBuf::from(&dest);
    paths
        .iter()
        .map(|p| {
            let src = Path::new(p);
            let Some(file_name) = src.file_name().and_then(|n| n.to_str()) else {
                return OpResult::err(src, "Invalid file name");
            };
            if src.is_dir() && is_self_or_descendant(src, &dest_dir) {
                return OpResult::err(src, "Cannot copy a folder into itself");
            }
            let target = unique_target(&dest_dir, file_name);
            let result = if src.is_dir() {
                copy_dir_all(src, &target)
            } else {
                fs::copy(src, &target).map(|_| ())
            };
            match result {
                Ok(()) => OpResult::ok(&target),
                Err(e) => OpResult::err(src, e),
            }
        })
        .collect()
}

/// Move entries into `dest`, auto-renaming on collision. Falls back to
/// copy-then-delete when the move crosses a filesystem boundary (`fs::rename`
/// fails across volumes, e.g. C: -> Z:).
#[tauri::command]
fn move_entries(paths: Vec<String>, dest: String) -> Vec<OpResult> {
    let dest_dir = PathBuf::from(&dest);
    paths
        .iter()
        .map(|p| {
            let src = Path::new(p);
            let Some(file_name) = src.file_name().and_then(|n| n.to_str()) else {
                return OpResult::err(src, "Invalid file name");
            };
            if src.is_dir() && is_self_or_descendant(src, &dest_dir) {
                return OpResult::err(src, "Cannot move a folder into itself");
            }
            let target = unique_target(&dest_dir, file_name);

            if fs::rename(src, &target).is_ok() {
                return OpResult::ok(&target);
            }

            // Cross-volume move: copy, then remove the original only if the
            // copy fully succeeded. Never delete the source on a failed copy.
            let copied = if src.is_dir() {
                copy_dir_all(src, &target)
            } else {
                fs::copy(src, &target).map(|_| ())
            };
            match copied {
                Ok(()) => {
                    let removed = if src.is_dir() {
                        fs::remove_dir_all(src)
                    } else {
                        fs::remove_file(src)
                    };
                    match removed {
                        Ok(()) => OpResult::ok(&target),
                        Err(e) => OpResult::err(src, format!("Copied, but could not remove original: {e}")),
                    }
                }
                Err(e) => OpResult::err(src, e),
            }
        })
        .collect()
}

/// Move each `from` to an EXACT `to` path. Used by undo, which must restore an
/// item to its original name — auto-renaming here would defeat the point (undo
/// of "rename a -> b" must produce "a", not "a - Copy").
///
/// Refuses to overwrite: if `to` already exists, the undo fails loudly rather
/// than clobbering whatever now occupies that name.
#[tauri::command]
fn move_exact(pairs: Vec<(String, String)>) -> Vec<OpResult> {
    pairs
        .iter()
        .map(|(from, to)| {
            let src = Path::new(from);
            let dst = Path::new(to);
            if dst.exists() {
                return OpResult::err(
                    src,
                    format!(
                        "\"{}\" already exists",
                        dst.file_name().unwrap_or_default().to_string_lossy()
                    ),
                );
            }
            if let Some(parent) = dst.parent() {
                if !parent.exists() {
                    return OpResult::err(src, "The original folder no longer exists");
                }
            }
            match fs::rename(src, dst) {
                Ok(()) => OpResult::ok(dst),
                Err(e) => OpResult::err(src, e),
            }
        })
        .collect()
}

/// Detailed metadata for the Properties dialog.
#[tauri::command]
fn entry_info(path: String) -> Result<EntryInfo, String> {
    let p = Path::new(&path);
    let meta = fs::metadata(p).map_err(|e| format!("{path}: {e}"))?;
    Ok(EntryInfo {
        name: p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.clone()),
        path: path.clone(),
        is_dir: meta.is_dir(),
        size: if meta.is_dir() { 0 } else { meta.len() },
        modified: meta.modified().ok().and_then(to_epoch_ms),
        created: meta.created().ok().and_then(to_epoch_ms),
        readonly: meta.permissions().readonly(),
        hidden: is_hidden(p, &meta),
    })
}

/// Total size of a directory tree. Unreadable subtrees are skipped rather than
/// failing the whole calculation.
#[tauri::command]
fn dir_size(path: String) -> Result<u64, String> {
    fn walk(p: &Path) -> u64 {
        let Ok(read) = fs::read_dir(p) else { return 0 };
        let mut total = 0u64;
        for entry in read.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                // Don't recurse into symlinked dirs (CPE-611): a cycle would recurse until the thread
                // stack overflows and crashes the app. Matches `du`, which doesn't follow symlinks.
                if !entry_is_symlink(&entry) {
                    total += walk(&entry.path());
                }
            } else {
                total += meta.len();
            }
        }
        total
    }
    let p = Path::new(&path);
    if !p.exists() {
        return Err(format!("{path}: not found"));
    }
    Ok(walk(p))
}

/// Compute the SHA-256 checksum of a file, returned as lowercase hex (CPE-412). Streamed in fixed
/// chunks so a multi-GB file never loads into memory. A directory, missing, or unreadable path is an
/// `Err`, never a panic. Opt-in from the UI (hashing is I/O-bound) — never run automatically.
/// Stream a file through SHA-256 and return the lowercase hex digest. Shared by `hash_file` (CPE-412)
/// and the duplicate finder (CPE-420). 64 KiB chunks — a multi-GB file never loads into memory.
fn sha256_file(path: &Path) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    // Lowercase hex — one dependency fewer than pulling in `hex` for three lines.
    let mut hex = String::with_capacity(digest.len() * 2);
    for b in digest {
        use std::fmt::Write as _;
        let _ = write!(hex, "{b:02x}");
    }
    Ok(hex)
}

#[tauri::command]
fn hash_file(path: String) -> Result<String, String> {
    let p = Path::new(&path);
    if p.is_dir() {
        return Err(format!("{path}: is a folder"));
    }
    sha256_file(p).map_err(|e| format!("{path}: {e}"))
}

/// Line / word / character / byte counts for a text file (CPE-414). Lines follow `str::lines`
/// (a final unterminated line still counts); words are whitespace-separated; characters are Unicode
/// scalar values. Capped so analysing a file stays predictable; a non-UTF-8 (binary) file, a
/// directory, or an over-cap file is an `Err`. Opt-in from the UI, never automatic.
#[derive(serde::Serialize)]
struct TextStats {
    lines: u64,
    words: u64,
    chars: u64,
    bytes: u64,
}

/// Largest file the text-stats command will read into memory (keeps it fast/predictable).
const TEXT_STATS_MAX_BYTES: u64 = 25 * 1024 * 1024;

#[tauri::command]
fn text_stats(path: String) -> Result<TextStats, String> {
    let p = Path::new(&path);
    let meta = fs::metadata(p).map_err(|e| format!("{path}: {e}"))?;
    if meta.is_dir() {
        return Err(format!("{path}: is a folder"));
    }
    if meta.len() > TEXT_STATS_MAX_BYTES {
        return Err("file is too large to analyze (25 MB limit)".into());
    }
    let content = fs::read_to_string(p).map_err(|_| format!("{path}: not a text file"))?;
    Ok(TextStats {
        lines: content.lines().count() as u64,
        words: content.split_whitespace().count() as u64,
        chars: content.chars().count() as u64,
        bytes: content.len() as u64,
    })
}

/// Whether two files have identical content (CPE-418). Different sizes short-circuit to `false`;
/// otherwise the bytes are streamed and compared with an early exit on the first difference — cheaper
/// and collision-free versus hashing both. A directory or unreadable path is an `Err`, never a panic.
#[tauri::command]
fn files_identical(a: String, b: String) -> Result<bool, String> {
    use std::io::Read;
    let (pa, pb) = (Path::new(&a), Path::new(&b));
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

/// One content-search hit: the file, the 1-based line number, and the (trimmed, truncated) line.
#[derive(serde::Serialize)]
struct ContentMatch {
    path: String,
    line_number: u64,
    line: String,
}

/// The result of a content search (CPE-416): the hits, how many files were scanned, and whether a
/// cap was hit (so the UI can say "showing first N").
#[derive(serde::Serialize)]
struct ContentSearchResult {
    matches: Vec<ContentMatch>,
    files_scanned: u64,
    truncated: bool,
}

// Bounds that keep a content search fast + predictable regardless of the tree it's pointed at.
const SEARCH_MAX_MATCHES: usize = 1000;
const SEARCH_MAX_FILES: u64 = 20_000;
const SEARCH_MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;
const SEARCH_SNIPPET_MAX: usize = 400;

/// True if a byte slice looks binary (contains a NUL in the sniffed prefix) — skip such files.
fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8192).any(|&b| b == 0)
}

/// Search text files under `root` for lines containing `query` (CPE-416). Recursive, but bounded:
/// skips dot-directories (`.git`, `.venv`, …), binary/oversized files, and stops at match/file caps
/// (reporting `truncated`). Unreadable entries are skipped, never failing the whole search — the
/// same resilience as `list_dir`. Empty/whitespace `query` returns nothing.
#[tauri::command]
fn search_file_contents(
    root: String,
    query: String,
    case_sensitive: bool,
) -> Result<ContentSearchResult, String> {
    let needle = if case_sensitive { query.clone() } else { query.to_lowercase() };
    let mut result = ContentSearchResult { matches: Vec::new(), files_scanned: 0, truncated: false };
    if needle.trim().is_empty() {
        return Ok(result);
    }
    let root_path = Path::new(&root);
    if !root_path.is_dir() {
        return Err(format!("{root}: not a folder"));
    }

    // Explicit stack, not recursion — bounded memory, and matches `list_dir`'s skip-on-error ethos.
    let mut stack = vec![root_path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            if result.matches.len() >= SEARCH_MAX_MATCHES || result.files_scanned >= SEARCH_MAX_FILES {
                result.truncated = true;
                return Ok(result);
            }
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                // Skip dot-dirs (.git, .venv, node_modules-style noise starts with '.') and symlinked
                // dirs (avoid cycles, CPE-609) to stay fast.
                if !name.starts_with('.') && !entry_is_symlink(&entry) {
                    stack.push(path);
                }
                continue;
            }
            if !meta.is_file() || meta.len() > SEARCH_MAX_FILE_BYTES {
                continue;
            }
            let Ok(bytes) = fs::read(&path) else { continue };
            if looks_binary(&bytes) {
                continue;
            }
            result.files_scanned += 1;
            let text = String::from_utf8_lossy(&bytes);
            let path_str = path.to_string_lossy().into_owned();
            for (i, line) in text.lines().enumerate() {
                let hay = if case_sensitive { line.to_string() } else { line.to_lowercase() };
                if hay.contains(&needle) {
                    let mut snippet = line.trim().to_string();
                    if snippet.chars().count() > SEARCH_SNIPPET_MAX {
                        snippet = snippet.chars().take(SEARCH_SNIPPET_MAX).collect::<String>() + "…";
                    }
                    result.matches.push(ContentMatch {
                        path: path_str.clone(),
                        line_number: (i + 1) as u64,
                        line: snippet,
                    });
                    if result.matches.len() >= SEARCH_MAX_MATCHES {
                        result.truncated = true;
                        return Ok(result);
                    }
                }
            }
        }
    }
    Ok(result)
}

/// One filename-search hit (CPE-603): the full path, the bare name, and whether it's a folder.
#[derive(serde::Serialize)]
struct NameMatch {
    path: String,
    name: String,
    is_dir: bool,
}

/// The result of a filename search: the hits, how many directories were walked, and whether a cap
/// was hit (so the UI can say "showing the first results").
#[derive(serde::Serialize)]
struct NameSearchResult {
    matches: Vec<NameMatch>,
    dirs_scanned: u64,
    truncated: bool,
}

const NAME_SEARCH_MAX_MATCHES: usize = 2000;
const NAME_SEARCH_MAX_DIRS: u64 = 50_000;

/// Anchored wildcard match: `*` matches any run of characters, `?` exactly one. Both `name` and
/// `pattern` are assumed already lowercased. Iterative two-pointer backtracking — no regex
/// dependency, linear-ish, and pure so it's unit-testable.
fn glob_is_match(name: &str, pattern: &str) -> bool {
    let n: Vec<char> = name.chars().collect();
    let p: Vec<char> = pattern.chars().collect();
    let (mut i, mut j) = (0usize, 0usize);
    let (mut star, mut mark) = (None, 0usize);
    while i < n.len() {
        if j < p.len() && (p[j] == '?' || p[j] == n[i]) {
            i += 1;
            j += 1;
        } else if j < p.len() && p[j] == '*' {
            star = Some(j);
            mark = i;
            j += 1;
        } else if let Some(s) = star {
            j = s + 1;
            mark += 1;
            i = mark;
        } else {
            return false;
        }
    }
    while j < p.len() && p[j] == '*' {
        j += 1;
    }
    j == p.len()
}

/// Case-insensitive name match mirroring the frontend `matchesQuery` (CPE-603): a query containing
/// `*`/`?` is an anchored glob over the whole name; otherwise a plain substring. `query_lower` must
/// already be lowercased and non-empty (an empty query matches nothing — the caller gates on it).
fn name_matches(name: &str, query_lower: &str) -> bool {
    if query_lower.is_empty() {
        return false;
    }
    let n = name.to_lowercase();
    if query_lower.contains('*') || query_lower.contains('?') {
        glob_is_match(&n, query_lower)
    } else {
        n.contains(query_lower)
    }
}

/// Whether a directory entry is a symlink, without following it (CPE-609). Recursive walks use this to
/// avoid descending into symlinked directories: a symlink cycle (a link pointing at an ancestor) would
/// otherwise spin until the walk's caps, wasting work and truncating real results. Matches ripgrep's
/// default of not following symlinks. Symlinked *files* are unaffected — only descent is skipped.
fn entry_is_symlink(entry: &fs::DirEntry) -> bool {
    entry.file_type().map(|t| t.is_symlink()).unwrap_or(false)
}

/// Find files and folders under `root` whose NAME matches `query` (CPE-603). Recursive, but bounded
/// like `search_file_contents`: skips dot-directories, stops at match/dir caps (reporting
/// `truncated`), and skips unreadable directories rather than failing the whole search. Empty query
/// returns nothing; a non-folder root is an `Err`.
#[tauri::command]
fn find_files_by_name(root: String, query: String) -> Result<NameSearchResult, String> {
    let q = query.trim().to_lowercase();
    let mut result = NameSearchResult { matches: Vec::new(), dirs_scanned: 0, truncated: false };
    if q.is_empty() {
        return Ok(result);
    }
    let root_path = Path::new(&root);
    if !root_path.is_dir() {
        return Err(format!("{root}: not a folder"));
    }

    // Explicit stack, not recursion — bounded memory, and matches `list_dir`'s skip-on-error ethos.
    let mut stack = vec![root_path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        result.dirs_scanned += 1;
        if result.dirs_scanned > NAME_SEARCH_MAX_DIRS {
            result.truncated = true;
            return Ok(result);
        }
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let Ok(meta) = entry.metadata() else { continue };
            let is_dir = meta.is_dir();
            if name_matches(&name, &q) {
                result.matches.push(NameMatch {
                    path: path.to_string_lossy().into_owned(),
                    name: name.to_string(),
                    is_dir,
                });
                if result.matches.len() >= NAME_SEARCH_MAX_MATCHES {
                    result.truncated = true;
                    return Ok(result);
                }
            }
            // Descend into real sub-directories, skipping dot-dirs (.git, .venv, …) and symlinks
            // (avoid cycles) to stay fast. A symlinked dir is still reported as a match above.
            if is_dir && !name.starts_with('.') && !entry_is_symlink(&entry) {
                stack.push(path);
            }
        }
    }
    Ok(result)
}

/// A set of byte-identical files (CPE-420): their shared size + hash and every path.
#[derive(serde::Serialize)]
struct DupGroup {
    size: u64,
    hash: String,
    paths: Vec<String>,
}

/// The result of a duplicate scan: the groups (largest reclaimable space first), how many files were
/// considered, and whether the file cap was hit.
#[derive(serde::Serialize)]
struct DupResult {
    groups: Vec<DupGroup>,
    files_scanned: u64,
    truncated: bool,
}

const DUP_MAX_FILES: u64 = 50_000;

/// Find duplicate files under `root` (CPE-420). Efficient: group by size first (a unique size can't
/// be a duplicate), then SHA-256 only the size-collision candidates — most files are never read.
/// Skips dot-directories and empty files; unreadable entries are skipped (never failing the scan);
/// stops at a file cap (reporting `truncated`). Groups are sorted by reclaimable space (largest
/// first). A non-folder root is an `Err`.
#[tauri::command]
fn find_duplicates(root: String) -> Result<DupResult, String> {
    use std::collections::HashMap;
    let root_path = Path::new(&root);
    if !root_path.is_dir() {
        return Err(format!("{root}: not a folder"));
    }

    // Pass 1: group candidate files by size (skip-on-error like `list_dir`).
    let mut by_size: HashMap<u64, Vec<std::path::PathBuf>> = HashMap::new();
    let mut files_scanned = 0u64;
    let mut truncated = false;
    let mut stack = vec![root_path.to_path_buf()];
    'walk: while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                // Skip dot-dirs and symlinked dirs (avoid cycles, CPE-611).
                if !name.to_string_lossy().starts_with('.') && !entry_is_symlink(&entry) {
                    stack.push(path);
                }
                continue;
            }
            if !meta.is_file() || meta.len() == 0 {
                continue; // empty files are all "equal" — not useful to report
            }
            if files_scanned >= DUP_MAX_FILES {
                truncated = true;
                break 'walk;
            }
            files_scanned += 1;
            by_size.entry(meta.len()).or_default().push(path);
        }
    }

    // Pass 2: within each size collision, hash the candidates and group identical content.
    let mut groups: Vec<DupGroup> = Vec::new();
    for (size, paths) in by_size {
        if paths.len() < 2 {
            continue;
        }
        let mut by_hash: HashMap<String, Vec<String>> = HashMap::new();
        for p in &paths {
            if let Ok(h) = sha256_file(p) {
                by_hash.entry(h).or_default().push(p.to_string_lossy().into_owned());
            }
        }
        for (hash, group_paths) in by_hash {
            if group_paths.len() > 1 {
                groups.push(DupGroup { size, hash, paths: group_paths });
            }
        }
    }

    // Largest reclaimable space first: size × (copies − 1).
    groups.sort_by_key(|g| std::cmp::Reverse(g.size * (g.paths.len() as u64 - 1)));
    Ok(DupResult { groups, files_scanned, truncated })
}

/// Read `settings.json` from `dir`, returning `{}` when it's absent or
/// unreadable so the frontend always starts from a valid document.
fn read_settings_from(dir: &Path) -> String {
    fs::read_to_string(dir.join("settings.json")).unwrap_or_else(|_| "{}".to_string())
}

/// Write the settings document to `settings.json` in `dir`, creating `dir` if
/// needed.
fn write_settings_to(dir: &Path, contents: &str) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    fs::write(dir.join("settings.json"), contents.as_bytes()).map_err(|e| e.to_string())?;
    Ok(())
}

/// Read the single on-disk settings file (`settings.json` in the app config
/// dir). Returns `{}` when it doesn't exist yet, so the frontend can start from
/// defaults on a fresh install (CPE-226).
#[tauri::command]
fn read_settings(app: tauri::AppHandle) -> Result<String, String> {
    use tauri::Manager;
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    Ok(read_settings_from(&dir))
}

/// Write the single on-disk settings file, creating the config dir if needed
/// (CPE-226). `contents` is the full settings JSON document.
#[tauri::command]
fn write_settings(app: tauri::AppHandle, contents: String) -> Result<(), String> {
    use tauri::Manager;
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    write_settings_to(&dir, &contents)
}

/// Return the user's home directory.
#[tauri::command]
fn home_dir() -> Result<String, String> {
    dirs_home()
        .map(|p| p.to_string_lossy().to_string())
        .ok_or_else(|| "could not determine home directory".to_string())
}

/// Return the parent of `path`, or null if already at a root.
#[tauri::command]
fn parent_dir(path: String) -> Option<String> {
    Path::new(&path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
}

/// Available drives (Windows) or filesystem roots (Unix).
#[tauri::command]
fn list_drives() -> Vec<Place> {
    let mut drives = Vec::new();

    #[cfg(target_os = "windows")]
    {
        for letter in b'A'..=b'Z' {
            let root = format!("{}:\\", letter as char);
            if Path::new(&root).exists() {
                drives.push(Place {
                    name: format!("Local Disk ({}:)", letter as char),
                    path: root,
                    kind: "drive".to_string(),
                });
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        drives.push(Place {
            name: "File System".to_string(),
            path: "/".to_string(),
            kind: "drive".to_string(),
        });
    }

    drives
}

/// Free + total bytes on the volume containing `path`, for the status bar (CPE-403).
#[derive(serde::Serialize)]
struct DiskSpace {
    free: u64,
    total: u64,
}

/// Report free/total space on the volume that holds `path` (CPE-403). `free` is what's available to
/// the user (respects quotas). Non-fatal: returns an error string the frontend degrades on rather
/// than surfacing — a status-bar nicety must never break navigation.
#[tauri::command]
fn disk_space(path: String) -> Result<DiskSpace, String> {
    let free = fs4::available_space(&path).map_err(|e| e.to_string())?;
    let total = fs4::total_space(&path).map_err(|e| e.to_string())?;
    Ok(DiskSpace { free, total })
}

/// On Windows, look up a known folder's REAL location in the registry.
///
/// Windows "Known Folder redirection" lets OneDrive move Desktop, Documents,
/// Pictures, etc. anywhere at all. On a real machine Pictures resolved to
/// `C:\Users\<user>\OneDrive\Exteriors Cave Homes\Pictures` — a path no
/// `%USERPROFILE%\Pictures` or `%OneDrive%\Pictures` heuristic could ever guess.
/// Worse, Windows often leaves an empty stub at `%USERPROFILE%\Desktop`, so
/// probing the profile first returns the *wrong* folder rather than none.
///
/// `Shell Folders` holds fully-expanded paths (`User Shell Folders` holds
/// unexpanded `%USERPROFILE%` tokens), so we read the former.
///
/// `registry_name` is the value name, which is NOT the display name:
/// Documents is "Personal", Pictures is "My Pictures", Downloads is a GUID.
#[cfg(windows)]
fn known_folder_from_registry(registry_name: &str) -> Option<PathBuf> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Explorer\Shell Folders")
        .ok()?;
    let value: String = key.get_value(registry_name).ok()?;
    let path = PathBuf::from(value);
    if path.is_dir() {
        Some(path)
    } else {
        None
    }
}

#[cfg(not(windows))]
fn known_folder_from_registry(_registry_name: &str) -> Option<PathBuf> {
    None
}

/// Resolve a known folder: the registry (authoritative on Windows) first, then
/// the plain `<home>/<folder>` path as a fallback for POSIX and for any folder
/// Windows does not list.
fn resolve_known_folder(home: &Path, folder: &str, registry_name: &str) -> Option<PathBuf> {
    if let Some(p) = known_folder_from_registry(registry_name) {
        return Some(p);
    }
    let in_profile = home.join(folder);
    if in_profile.is_dir() {
        return Some(in_profile);
    }
    None
}

/// The user's well-known folders. Only folders that actually exist are returned,
/// so the sidebar never shows a link that leads nowhere.
#[tauri::command]
fn special_folders() -> Vec<Place> {
    let Some(home) = dirs_home() else {
        return Vec::new();
    };

    // (display name, icon kind, Windows registry value name)
    // The registry names are historical and do not match the display names:
    // Documents is "Personal", Pictures is "My Pictures", Videos is "My Video",
    // and Downloads is only exposed under its known-folder GUID.
    let candidates = [
        ("Desktop", "desktop", "Desktop"),
        ("Documents", "documents", "Personal"),
        (
            "Downloads",
            "downloads",
            "{374DE290-123F-4565-9164-39C4925E467B}",
        ),
        ("Pictures", "pictures", "My Pictures"),
        ("Music", "music", "My Music"),
        ("Videos", "videos", "My Video"),
    ];

    candidates
        .iter()
        .filter_map(|(folder, kind, registry_name)| {
            resolve_known_folder(&home, folder, registry_name).map(|p| Place {
                name: (*folder).to_string(),
                path: p.to_string_lossy().to_string(),
                kind: (*kind).to_string(),
            })
        })
        .collect()
}

// Small cross-platform home-dir resolver without an extra dependency.
fn dirs_home() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Normalise a git remote URL to a browsable https URL:
/// `git@github.com:owner/repo.git` / `ssh://git@host/owner/repo.git` /
/// `https://host/owner/repo.git` → `https://host/owner/repo`.
fn normalize_git_url(raw: &str) -> String {
    let mut u = raw.trim().to_string();
    if let Some(rest) = u.strip_prefix("git@") {
        if let Some((host, path)) = rest.split_once(':') {
            u = format!("https://{host}/{path}");
        }
    } else if let Some(rest) = u.strip_prefix("ssh://git@") {
        u = format!("https://{rest}");
    } else if let Some(rest) = u.strip_prefix("git://") {
        u = format!("https://{rest}");
    }
    if let Some(stripped) = u.strip_suffix(".git") {
        u = stripped.to_string();
    }
    u
}

/// Open a file or folder with its default OS application (CPE-240). Uses the OS
/// shell opener directly (Windows `start`, macOS `open`, Linux `xdg-open`) —
/// more reliable than the opener plugin, which wasn't launching apps for several
/// file types. For an executable (.exe/.cmd/.bat/…) this runs it.
#[tauri::command]
fn open_external(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let spawned = std::process::Command::new("cmd")
        .args(["/C", "start", "", &path])
        .spawn();
    #[cfg(target_os = "macos")]
    let spawned = std::process::Command::new("open").arg(&path).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let spawned = std::process::Command::new("xdg-open").arg(&path).spawn();

    spawned.map(|_| ()).map_err(|e| e.to_string())
}

/// Open the platform's terminal with its working directory set to `path`
/// (CPE-253). Windows prefers Windows Terminal and falls back to a fresh cmd
/// window; macOS uses Terminal.app; Linux tries the common emulators in turn.
#[tauri::command]
fn open_terminal(path: String) -> Result<(), String> {
    use std::process::Command;

    #[cfg(target_os = "windows")]
    {
        // Windows Terminal opens directly at a directory with -d.
        if Command::new("wt.exe").args(["-d", &path]).spawn().is_ok() {
            return Ok(());
        }
        // Fallback: a new cmd window whose working dir is `path`. `start ""`
        // spawns the window; current_dir sets where it opens.
        Command::new("cmd")
            .args(["/C", "start", "", "cmd"])
            .current_dir(&path)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .args(["-a", "Terminal", &path])
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // Try the common terminals in order; the first that launches wins.
        let candidates = ["x-terminal-emulator", "gnome-terminal", "konsole", "xterm"];
        for term in candidates {
            if Command::new(term).current_dir(&path).spawn().is_ok() {
                return Ok(());
            }
        }
        Err("no terminal emulator found".into())
    }
}

/// Extract a single entry from a ZIP to a temp file and return its path, so it
/// can be opened with its default app while browsing inside the archive
/// (CPE-242). Read-only: the temp copy is what opens, not the archived bytes.
#[tauri::command]
fn extract_archive_entry(zip: String, inner: String) -> Result<String, String> {
    let file = fs::File::open(&zip).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    // The frontend uses "/"; some zips store "\" — try the given name then the
    // backslash variant so extraction works either way.
    let backslashed = inner.replace('/', "\\");
    let idx = archive
        .index_for_name(&inner)
        .or_else(|| archive.index_for_name(&backslashed))
        .ok_or_else(|| format!("entry not found: {inner}"))?;
    let mut entry = archive.by_index(idx).map_err(|e| e.to_string())?;

    let base = std::path::Path::new(&inner)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .ok_or_else(|| "invalid entry name".to_string())?;
    let dir = std::env::temp_dir().join("cpe-archive");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let out = dir.join(&base);
    let mut w = fs::File::create(&out).map_err(|e| e.to_string())?;
    std::io::copy(&mut entry, &mut w).map_err(|e| e.to_string())?;
    Ok(out.to_string_lossy().to_string())
}

// ---------------------------------------------------------------------------
// Archive creation & extraction (CPE-251 / CPE-252)
//
// The browser (CPE-064/242) reads archives and extracts single entries to a
// temp file; these two commands complete the round trip — pack a selection into
// a new .zip, and unpack a whole archive to a folder. ZIP is the create format
// (universal, already a dependency); extraction dispatches by extension like
// `read_archive_entries`.
// ---------------------------------------------------------------------------

/// Recursively add `src` to an open zip under the archive path `name_in_zip`.
/// Directories become explicit entries so empty folders survive the round trip.
fn zip_add_path(
    writer: &mut zip::ZipWriter<fs::File>,
    src: &std::path::Path,
    name_in_zip: &str,
    opts: zip::write::SimpleFileOptions,
) -> Result<(), String> {
    let meta = fs::symlink_metadata(src).map_err(|e| e.to_string())?;
    if meta.is_dir() {
        writer
            .add_directory(format!("{name_in_zip}/"), opts)
            .map_err(|e| e.to_string())?;
        // Sort children for a stable, reproducible archive order.
        let mut children: Vec<_> = fs::read_dir(src)
            .map_err(|e| e.to_string())?
            .filter_map(|e| e.ok())
            .collect();
        children.sort_by_key(|e| e.file_name());
        for child in children {
            let child_name = child.file_name().to_string_lossy().to_string();
            zip_add_path(
                writer,
                &child.path(),
                &format!("{name_in_zip}/{child_name}"),
                opts,
            )?;
        }
    } else {
        writer
            .start_file(name_in_zip, opts)
            .map_err(|e| e.to_string())?;
        let mut f = fs::File::open(src).map_err(|e| e.to_string())?;
        std::io::copy(&mut f, writer).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Pack the given files/folders into a new deflated `.zip` at `dest` (CPE-251).
/// Each top-level selection keeps its own name at the archive root; folders are
/// added recursively. Returns the created zip path.
#[tauri::command]
fn compress_to_zip(paths: Vec<String>, dest: String) -> Result<String, String> {
    if paths.is_empty() {
        return Err("nothing to compress".into());
    }
    let file = fs::File::create(&dest).map_err(|e| e.to_string())?;
    let mut writer = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for p in &paths {
        let src = std::path::Path::new(p);
        let name = src
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .ok_or_else(|| format!("invalid path: {p}"))?;
        zip_add_path(&mut writer, src, &name, opts)?;
    }
    writer.finish().map_err(|e| e.to_string())?;
    Ok(dest)
}

/// Unpack a tar stream into `dest`.
fn tar_unpack<R: std::io::Read>(reader: R, dest: &std::path::Path) -> Result<(), String> {
    let mut archive = tar::Archive::new(reader);
    archive.unpack(dest).map_err(|e| e.to_string())
}

/// Extract an archive into `dest`, which is created if missing (CPE-252).
/// Dispatched by extension like `read_archive_entries`; zip extraction uses the
/// crate's path-safe extractor, so a malicious "zip-slip" entry can't escape.
#[tauri::command]
fn extract_archive(path: String, dest: String) -> Result<String, String> {
    fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    let dest_path = std::path::Path::new(&dest);
    let lower = path.to_lowercase();

    if lower.ends_with(".tar") {
        let file = fs::File::open(&path).map_err(|e| e.to_string())?;
        tar_unpack(file, dest_path)?;
    } else if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        let file = fs::File::open(&path).map_err(|e| e.to_string())?;
        tar_unpack(flate2::read::GzDecoder::new(file), dest_path)?;
    } else if lower.ends_with(".gz") {
        // A bare .gz holds a single file; its name is the archive name minus .gz.
        let stem = std::path::Path::new(&path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "extracted".to_string());
        let file = fs::File::open(&path).map_err(|e| e.to_string())?;
        let mut decoder = flate2::read::GzDecoder::new(file);
        let mut out = fs::File::create(dest_path.join(stem)).map_err(|e| e.to_string())?;
        std::io::copy(&mut decoder, &mut out).map_err(|e| e.to_string())?;
    } else if lower.ends_with(".7z") {
        sevenz_rust::decompress_file(&path, dest_path).map_err(|e| e.to_string())?;
    } else {
        // zip family (zip/jar/apk/war/…): the crate's extractor guards against
        // path traversal via ZipFile::enclosed_name.
        let file = fs::File::open(&path).map_err(|e| e.to_string())?;
        let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
        archive.extract(dest_path).map_err(|e| e.to_string())?;
    }
    Ok(dest)
}

/// Run an executable with elevation (CPE-241). On Windows this uses
/// `Start-Process -Verb RunAs`, which shows the UAC prompt. On other platforms
/// there is no standard per-launch elevation prompt, so it runs normally.
#[tauri::command]
fn run_as_admin(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        // Single-quote the path for PowerShell; escape any embedded quote.
        let escaped = path.replace('\'', "''");
        std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!("Start-Process -FilePath '{escaped}' -Verb RunAs"),
            ])
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "windows"))]
    {
        open_external(path)
    }
}

/// Read a repo's `.git/config` and return its origin remote as a browsable https
/// URL (folder-context plugins, CPE-235). A cheap single file read; returns None
/// if the folder isn't a repo or has no remote.
#[tauri::command]
fn git_remote_url(path: String) -> Option<String> {
    let cfg = std::path::Path::new(&path).join(".git").join("config");
    let text = std::fs::read_to_string(cfg).ok()?;

    let mut in_origin = false;
    let mut origin_url: Option<String> = None;
    let mut first_url: Option<String> = None;
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_origin = l.contains("remote \"origin\"");
            continue;
        }
        if let Some(rest) = l.strip_prefix("url") {
            if let Some(eq) = rest.find('=') {
                let value = rest[eq + 1..].trim().to_string();
                if first_url.is_none() {
                    first_url = Some(value.clone());
                }
                if in_origin {
                    origin_url = Some(value);
                }
            }
        }
    }
    origin_url.or(first_url).map(|u| normalize_git_url(&u))
}

// ---------------------------------------------------------------------------
// Sidecar platform integration (ADR 0001 / CPE-260), feature-gated.
//
// Everything below is compiled ONLY with `--features sidecar-platform`. With the
// feature off (the default), none of this — and none of the `sidecar-host` crate —
// is part of the build, so the plain explorer stays byte-for-byte as it was. That is
// the delete-test (CPE-272): remove the sidecars and the explorer is unaffected.
// ---------------------------------------------------------------------------

/// List the ids of sidecars registered in the bundled + user registry directories.
/// A first, minimal seam into the platform host; the pane mount and supervisor wiring
/// build on this (CPE-271 onward).
/// The registry directories the platform reads sidecar manifests from: the bundled
/// catalog (shipped with the app) + a user-writable dir under app config.
#[cfg(feature = "sidecar-platform")]
fn sidecar_dirs(app: &tauri::AppHandle) -> Vec<PathBuf> {
    use tauri::Manager;
    let mut dirs = Vec::new();
    if let Ok(resource) = app.path().resource_dir() {
        dirs.push(resource.join("sidecars"));
    }
    if let Ok(config) = app.path().app_config_dir() {
        dirs.push(config.join("sidecars"));
    }
    // Dev fallback: `tauri dev` (debug) has no bundled `sidecars/` resource dir, so the host
    // registry wouldn't know the sidecar and consent would be skipped (CPE-364). Point at the
    // source-tree manifests, guarded by their existence so they're inert in a bundled release.
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    for p in [
        manifest.join("../sidecar/ai-console"),
        PathBuf::from("sidecar/ai-console"),
        // The Repositories sidecar is a registered tenant too (CPE-432): the host discovers +
        // manages it (enable/disable, contract-compat) via the generic registry. v1 surfaces forge
        // natively, so no bespoke launch UI is wired — but it is bundled + registered behind the feature.
        manifest.join("../sidecar/repos"),
        PathBuf::from("sidecar/repos"),
    ] {
        if p.join("sidecar.json").exists() {
            dirs.push(p);
        }
    }
    dirs
}

/// Candidate paths of *this app's* bundled AI Console sidecar binary — the bundled resource copy
/// and the user-config copy. Used to scope the orphan-daemon sweep (CPE-483) tightly to our own
/// binary so it never touches an unrelated `ai-console` elsewhere.
#[cfg(feature = "sidecar-platform")]
fn sidecar_ai_console_exes(app: &tauri::AppHandle) -> Vec<PathBuf> {
    use tauri::Manager;
    let exe_name = if cfg!(windows) { "ai-console.exe" } else { "ai-console" };
    let mut exes = Vec::new();
    if let Ok(resource) = app.path().resource_dir() {
        exes.push(resource.join("sidecars").join(exe_name));
    }
    if let Ok(config) = app.path().app_config_dir() {
        exes.push(config.join("sidecars").join(exe_name));
    }
    exes
}

/// Sweep leftover `ai-console --session-daemon` orphans at startup (CPE-483). Runs before the host
/// spawns any daemon of its own, so every match is one this host does not own — safe to reap. Also
/// clears the stale daemon port file. Best-effort and never fatal: a failed sweep only logs.
#[cfg(feature = "sidecar-platform")]
fn reap_orphan_session_daemons_on_startup(app: &tauri::AppHandle) {
    let exes = sidecar_ai_console_exes(app);
    let port_file = sidecar_host::reaper::default_session_daemon_port_file();
    let report = sidecar_host::reaper::reap_orphan_session_daemons(&exes, Some(&port_file));
    if !report.killed_pids.is_empty() || report.port_file_removed {
        eprintln!(
            "cpe: reaped {} orphan session-daemon(s) at startup{}",
            report.killed_pids.len(),
            if report.port_file_removed { "; cleared stale port file" } else { "" },
        );
    }
}

/// Directory holding the persisted capability-consent store (CPE-296).
#[cfg(feature = "sidecar-platform")]
fn consent_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;
    app.path()
        .app_config_dir()
        .map(|c| c.join("sidecars"))
        .map_err(|e| e.to_string())
}

/// List the ids of sidecars registered in the bundled + user registry directories.
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn sidecar_registry_ids(app: tauri::AppHandle) -> Vec<String> {
    sidecar_host::registry::Registry::load_from_dirs(&sidecar_dirs(&app))
        .all()
        .map(|m| m.id.clone())
        .collect()
}

/// A sidecar's requested capabilities plus the persisted consent decision (CPE-296):
/// which are already granted, and which are still undecided (need a consent prompt).
#[cfg(feature = "sidecar-platform")]
#[derive(serde::Serialize)]
struct ConsentState {
    requested: Vec<sidecar_contract::Capability>,
    granted: Vec<sidecar_contract::Capability>,
    undecided: Vec<sidecar_contract::Capability>,
}

#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn sidecar_consent_state(app: tauri::AppHandle, id: String) -> Result<ConsentState, String> {
    let reg = sidecar_host::registry::Registry::load_from_dirs(&sidecar_dirs(&app));
    let requested = reg
        .get(&id)
        .map(|m| m.capabilities.clone())
        .ok_or_else(|| format!("no sidecar '{id}' in the registry"))?;
    let store = sidecar_host::consent::ConsentStore::load(&consent_dir(&app)?);
    let granted: Vec<_> = store.granted(&id).into_iter().collect();
    let undecided = store.undecided(&id, &requested);
    Ok(ConsentState { requested, granted, undecided })
}

/// Record the user's consent decision: `granted` are approved, the remaining `decided`
/// capabilities are denied. Persisted so the user is asked once per capability (CPE-296).
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn sidecar_set_consent(
    app: tauri::AppHandle,
    id: String,
    granted: Vec<sidecar_contract::Capability>,
    decided: Vec<sidecar_contract::Capability>,
) -> Result<(), String> {
    let mut store = sidecar_host::consent::ConsentStore::load(&consent_dir(&app)?);
    let granted_set = granted.into_iter().collect();
    store
        .record(&id, &granted_set, &decided)
        .map_err(|e| e.to_string())
}

/// Revoke a previously-granted capability (management UI, CPE-274/296). Takes effect on
/// the sidecar's next launch.
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn sidecar_revoke_capability(
    app: tauri::AppHandle,
    id: String,
    capability: sidecar_contract::Capability,
) -> Result<(), String> {
    let mut store = sidecar_host::consent::ConsentStore::load(&consent_dir(&app)?);
    store.revoke(&id, capability).map_err(|e| e.to_string())
}

/// One row in the platform management UI (CPE-274): a registered sidecar with its
/// identity, contract compatibility, running/enabled state, and consent picture.
#[cfg(feature = "sidecar-platform")]
#[derive(serde::Serialize)]
struct SidecarInfo {
    id: String,
    name: String,
    version: String,
    contract: String,
    compatible: bool,
    running: bool,
    enabled: bool,
    requested: Vec<sidecar_contract::Capability>,
    granted: Vec<sidecar_contract::Capability>,
}

/// List registered sidecars with version, contract compatibility, running/enabled state,
/// and granted capabilities — the data behind the management panel (CPE-274).
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn sidecar_details(app: tauri::AppHandle, state: tauri::State<AiConsoleState>) -> Result<Vec<SidecarInfo>, String> {
    use sidecar_contract::CONTRACT_VERSION;
    let reg = sidecar_host::registry::Registry::load_from_dirs(&sidecar_dirs(&app));
    let consent = sidecar_host::consent::ConsentStore::load(&consent_dir(&app)?);
    let enablement = sidecar_host::enablement::EnablementStore::load(&consent_dir(&app)?);
    let ai_running = state.conn.lock().map(|g| g.is_some()).unwrap_or(false);

    Ok(reg
        .all()
        .map(|m| {
            let cv = &m.contract_version;
            SidecarInfo {
                id: m.id.clone(),
                name: m.name.clone(),
                version: m.version.clone(),
                contract: format!("{}.{}", cv.major, cv.minor),
                compatible: cv.major == CONTRACT_VERSION.major && cv.minor <= CONTRACT_VERSION.minor,
                running: m.id == "ai-console" && ai_running,
                enabled: enablement.is_enabled(&m.id),
                requested: m.capabilities.clone(),
                granted: consent.granted(&m.id).into_iter().collect(),
            }
        })
        .collect())
}

/// Stop a running sidecar (management UI). Dropping the connection reaps the process.
/// Only the AI Console is currently spawnable; a no-op for others.
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn sidecar_stop(id: String, state: tauri::State<AiConsoleState>) -> Result<(), String> {
    if id == "ai-console" {
        *state.conn.lock().map_err(|_| "state lock poisoned")? = None;
        *state.url.lock().map_err(|_| "state lock poisoned")? = None; // no reuse once stopped (CPE-464)
        state.log(sidecar_host::observability::LogLevel::Info, "stopped by user");
    }
    Ok(())
}

/// Close a single AI Console session (CPE-489) — the left-pane Agents "Close this session". Routes to
/// the console's own per-session close endpoint over its loopback UI server
/// (`{url}/api/session/{id}/close`); the console then emits an `ended` for that session, pruning its
/// leaf while the others keep running. A no-op if the console isn't running (no URL yet).
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn sidecar_close_session(session_id: String, state: tauri::State<AiConsoleState>) -> Result<(), String> {
    // Session ids are simple tokens (`s1`, `s12`). Refuse anything else so it can never reshape the
    // loopback URL path (no traversal / injection into the request line).
    if session_id.is_empty()
        || !session_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("invalid session id".into());
    }
    let url = { state.url.lock().map_err(|_| "state lock poisoned")?.clone() };
    let Some(base) = url else { return Ok(()) }; // console not running → nothing to close
    let target = format!("{}/api/session/{session_id}/close", base.trim_end_matches('/'));
    ureq::post(&target)
        .timeout(std::time::Duration::from_secs(5))
        .call()
        .map_err(|e| format!("close session failed: {e}"))?;
    state.log(sidecar_host::observability::LogLevel::Info, format!("closed session {session_id} by user"));
    Ok(())
}

/// Enable or disable a sidecar (CPE-274). Disabling stops it (if running) and prevents it
/// from starting until re-enabled. Independent per sidecar — never touches others.
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn sidecar_set_enabled(
    app: tauri::AppHandle,
    state: tauri::State<AiConsoleState>,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    let mut store = sidecar_host::enablement::EnablementStore::load(&consent_dir(&app)?);
    store.set_enabled(&id, enabled).map_err(|e| e.to_string())?;
    if !enabled && id == "ai-console" {
        *state.conn.lock().map_err(|_| "state lock poisoned")? = None; // stop it now
        *state.url.lock().map_err(|_| "state lock poisoned")? = None; // CPE-464
        state.log(sidecar_host::observability::LogLevel::Info, "disabled by user");
    }
    Ok(())
}

/// Holds the live AI Console sidecar connection for the app's lifetime so it keeps
/// running while its UI pane is mounted, plus the observability state the management
/// panel surfaces (CPE-323): a bounded ring buffer of recent lifecycle log lines and the
/// last error that stopped it from starting. Both are populated by
/// `sidecar_start_ai_console` and read (redacted) by `sidecar_diagnostics`.
#[cfg(feature = "sidecar-platform")]
struct AiConsoleState {
    /// Handle to the live AI Console connection (CPE-349): a servicing thread owns the real
    /// `ProcessConnection`; this is the control handle. `Some` still means "running", and
    /// setting the slot to `None` (stop/disable) drops it, signalling the thread to exit and
    /// reap the child — preserving the previous stop semantics.
    conn: std::sync::Mutex<Option<ConsoleConn>>,
    logs: sidecar_host::observability::LogCapture,
    last_error: std::sync::Mutex<Option<String>>,
    /// The running sidecar's served UI URL (CPE-464) — so reopening the console reuses the live
    /// sidecar (and its sessions) instead of spawning a fresh one. `None` when not running.
    url: std::sync::Mutex<Option<String>>,
    /// The **host-owned** session daemon (CPE-309 S4): agent PTYs live in this separate, long-lived
    /// process so they survive the UI sidecar being restarted/toggled. Owned by the host (this state
    /// lives for the app's lifetime), spawned with a hidden console so Windows ConPTY produces output.
    /// `None` until first started; reaped when this state drops (app exit).
    daemon: std::sync::Mutex<Option<HostSessionDaemon>>,
}

/// A session daemon process the host spawned + owns (CPE-309 S4). Dropping it reaps the child.
#[cfg(feature = "sidecar-platform")]
struct HostSessionDaemon {
    child: std::process::Child,
    port: u16,
}

#[cfg(feature = "sidecar-platform")]
impl Drop for HostSessionDaemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Control handle for the AI Console servicing thread (CPE-349). Dropping it asks the thread
/// to stop; the thread then drops the underlying connection, which reaps the child.
#[cfg(feature = "sidecar-platform")]
struct ConsoleConn {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    _thread: std::thread::JoinHandle<()>,
}

#[cfg(feature = "sidecar-platform")]
impl Drop for ConsoleConn {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Service the AI Console sidecar's inbound capability requests (CPE-349). The sidecar sends
/// `secrets.*` (CPE-344) over the channel; without this loop those requests are never
/// answered and the launcher's Keys panel times out. Reads a request, dispatches it through a
/// broker holding the granted providers (the OS keychain secrets provider on Windows), and
/// writes the response back with the same correlation id. Exits on stop or when the sidecar
/// closes the connection, dropping the connection (which reaps the child).
/// Open the native folder dialog (on the main thread) and return `{ path }` — the response
/// to the sandboxed launcher's `host.pick_folder` request (CPE-354). `path` is null when the
/// user cancels.
#[cfg(feature = "sidecar-platform")]
fn pick_folder_response(app: &tauri::AppHandle, params: &serde_json::Value) -> sidecar_contract::Response {
    use serde_json::json;
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = std::sync::mpsc::channel();
    let mut builder = app.dialog().file();
    // Open at the launcher's current Project folder when it still exists — a typo or a since-deleted
    // path just falls back to the OS default rather than erroring.
    if let Some(start) = params.get("start").and_then(|v| v.as_str()) {
        let p = std::path::Path::new(start);
        if p.is_dir() {
            builder = builder.set_directory(p);
        }
    }
    builder.pick_folder(move |folder| {
        let _ = tx.send(folder);
    });
    let path = rx
        .recv()
        .ok()
        .flatten()
        .and_then(|f| f.into_path().ok())
        .map(|p| p.to_string_lossy().into_owned());
    sidecar_contract::Response { result: Ok(json!({ "path": path })) }
}

/// Verify a provider API key on the sidecar's behalf (CPE-347) — the response to a sandboxed
/// `host.verify_key` request. The URL is chosen host-side from an allow-list (see `keyverify`),
/// never from the request, so this can't be turned into a general fetch. Returns
/// `{ valid, live, detail }`.
#[cfg(feature = "sidecar-platform")]
fn verify_key_response(params: &serde_json::Value) -> sidecar_contract::Response {
    use serde_json::json;
    let provider = params.get("provider").and_then(|v| v.as_str()).unwrap_or("");
    let key = params.get("key").and_then(|v| v.as_str()).unwrap_or("");
    let (valid, live, detail) = keyverify::verify_live(provider, key);
    sidecar_contract::Response { result: Ok(json!({ "valid": valid, "live": live, "detail": detail })) }
}

/// Fetch a reseller's model list on the AI Console's behalf (CPE-447) — the response to a sandboxed
/// `host.list_models { reseller, token? }`. The endpoint is chosen host-side from an allow-list
/// (`models_egress`), never from the request, so it can't become a general fetch. Returns
/// `{ ok, status, body }` on a completed call, or `{ ok:false, error }` otherwise. The token is
/// never logged or echoed.
#[cfg(feature = "sidecar-platform")]
fn list_models_response(params: &serde_json::Value) -> sidecar_contract::Response {
    use serde_json::json;
    let reseller = params.get("reseller").and_then(|v| v.as_str()).unwrap_or("");
    let token = params.get("token").and_then(|v| v.as_str());
    match models_egress::list_models(reseller, token) {
        Ok((status, body)) => {
            sidecar_contract::Response { result: Ok(json!({ "ok": true, "status": status, "body": body })) }
        }
        Err(e) => sidecar_contract::Response { result: Ok(json!({ "ok": false, "error": format!("{e:?}") })) },
    }
}

/// Perform an allow-listed forge API call on the repos sidecar's behalf (CPE-433) — the response to
/// a sandboxed `host.forge_request { provider, method, path, host?, token?, body? }`. The URL is
/// built host-side from the provider allow-list (`forge_egress`), never from the request, so this is
/// not a general fetch (no SSRF). Returns `{ ok, status, body }` on a completed call, or
/// `{ ok:false, error }` for a refused/failed one. The token is never logged or echoed.
///
/// Not yet wired into a request router: the repos sidecar's own host connection lands with CPE-432
/// AC3 (host launch/supervision). This handler is ready to drop into that connection's dispatch,
/// exactly as `verify_key_response` sits in the AI Console connection.
#[cfg(feature = "sidecar-platform")]
#[allow(dead_code)]
fn forge_request_response(params: &serde_json::Value) -> sidecar_contract::Response {
    use serde_json::json;
    let provider = params.get("provider").and_then(|v| v.as_str()).unwrap_or("");
    let method = params.get("method").and_then(|v| v.as_str()).unwrap_or("GET");
    let path = params.get("path").and_then(|v| v.as_str()).unwrap_or("");
    let host = params.get("host").and_then(|v| v.as_str());
    let token = params.get("token").and_then(|v| v.as_str());
    let body = params.get("body").and_then(|v| v.as_str());
    match forge_egress::forge_request(provider, method, host, path, token, body) {
        Ok((status, body)) => {
            sidecar_contract::Response { result: Ok(json!({ "ok": true, "status": status, "body": body })) }
        }
        Err(e) => sidecar_contract::Response {
            result: Ok(json!({ "ok": false, "error": format!("{e:?}") })),
        },
    }
}

/// Trusted first-party public keys (hex) for agent-catalog signatures (CPE-376/377/380). The
/// matching private seed is the `CPE_CATALOG_SIGNING_KEY` release secret. Empty here would mean the
/// catalog-update feature is dormant (nothing trusted); a real key activates it.
#[cfg(feature = "sidecar-platform")]
const CATALOG_TRUSTED_KEYS: &[&str] =
    &["5b18ad467b37b7c06556000f15359a845bd85790ece91de110a337890d017130"];

/// The writable agent-catalog dir on this machine — where fetched, verified manifests land and
/// where the sidecar loads them from. Both the fetch handler and the sidecar (via env) agree on it.
#[cfg(feature = "sidecar-platform")]
fn catalog_dir(app: &tauri::AppHandle) -> PathBuf {
    use tauri::Manager;
    app.path()
        .app_data_dir()
        .map(|d| d.join("ai-console-catalog"))
        .unwrap_or_else(|_| std::env::temp_dir().join("cpe-ai-console-catalog"))
}

/// The GitHub owner/repo whose Releases carry the signed catalog bundles.
#[cfg(feature = "sidecar-platform")]
const CATALOG_REPO: &str = "StewartScottRogers/cross-platform-explorer";

/// The catalog source base URL — the app's GitHub Releases `latest/download/` by default (the
/// signed bundle rides next to the installer), overridable via `CPE_CATALOG_URL`.
#[cfg(feature = "sidecar-platform")]
fn catalog_url() -> String {
    std::env::var("CPE_CATALOG_URL").unwrap_or_else(|_| {
        format!("https://github.com/{CATALOG_REPO}/releases/latest/download/")
    })
}

/// Whether a release tag is safe to splice into a URL path (CPE-383): a version tag's characters
/// only — no `/`, `..`, scheme, or whitespace — so a chosen tag can never escape the releases path
/// (defence-in-depth, even though tags come from our own enumerated list).
#[cfg(feature = "sidecar-platform")]
fn is_safe_release_tag(tag: &str) -> bool {
    !tag.is_empty()
        && tag.len() <= 64
        && tag.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+'))
}

/// The `releases/download/<tag>/` base for a **specific** published version (CPE-383) — not
/// `latest`. Honours a `CPE_CATALOG_URL` override's origin for tests by only applying to the
/// default GitHub host.
#[cfg(feature = "sidecar-platform")]
fn catalog_url_for_tag(tag: &str) -> String {
    if let Ok(base) = std::env::var("CPE_CATALOG_URL") {
        // Test/override hook: swap a trailing `latest/download/` for this tag if present.
        return base.replace("latest/download/", &format!("download/{tag}/"));
    }
    format!("https://github.com/{CATALOG_REPO}/releases/download/{tag}/")
}

/// The GitHub Releases API URL listing published versions (CPE-383). Host-built from a constant —
/// the sidecar never supplies it — so it is a fixed **allow-listed** egress (threat model §7), a
/// read-only public GET with no secret.
#[cfg(feature = "sidecar-platform")]
fn github_releases_api() -> String {
    std::env::var("CPE_CATALOG_RELEASES_API")
        .unwrap_or_else(|_| format!("https://api.github.com/repos/{CATALOG_REPO}/releases?per_page=30"))
}

/// Parse the GitHub Releases API JSON into the catalog versions the rollback picker offers: each
/// published release that actually carries a catalog bundle (a `catalog-index.json` asset), with a
/// safe tag. Pure + unit-tested — the network fetch is a thin wrapper.
#[cfg(feature = "sidecar-platform")]
fn parse_release_versions(body: &[u8]) -> Vec<serde_json::Value> {
    use serde_json::json;
    let Ok(parsed) = serde_json::from_slice::<serde_json::Value>(body) else { return vec![] };
    let Some(rels) = parsed.as_array() else { return vec![] };
    rels.iter()
        .filter_map(|r| {
            let tag = r.get("tag_name")?.as_str()?;
            if !is_safe_release_tag(tag) {
                return None;
            }
            let has_catalog = r
                .get("assets")
                .and_then(|a| a.as_array())
                .is_some_and(|a| {
                    a.iter().any(|x| x.get("name").and_then(|n| n.as_str()) == Some("catalog-index.json"))
                });
            if !has_catalog {
                return None;
            }
            Some(json!({
                "tag": tag,
                "publishedAt": r.get("published_at").and_then(|p| p.as_str()).unwrap_or(""),
                "prerelease": r.get("prerelease").and_then(|p| p.as_bool()).unwrap_or(false),
            }))
        })
        .collect()
}

/// Response to the sandboxed `host.fetch_catalog` request (CPE-376): download the signed catalog
/// bundle from GitHub Releases and apply it (gated by CPE-372/373). Never errors the channel — a
/// failure comes back as `indexOk:false` with a message.
#[cfg(feature = "sidecar-platform")]
fn fetch_catalog_response(
    app: &tauri::AppHandle,
    params: &serde_json::Value,
) -> sidecar_contract::Response {
    use serde_json::json;
    // Pinned agents the sidecar asked us to skip (CPE-378).
    let pinned: Vec<String> = str_list(params, "pinned");
    // CPE-383: an optional specific version to roll back to, and the agents allowed to downgrade to
    // it. `tag` absent ⇒ the normal `latest` fetch with no downgrade.
    let tag = params.get("tag").and_then(|v| v.as_str()).map(str::to_string);
    let allow_downgrade: Vec<String> = str_list(params, "agents");
    let body = do_fetch_catalog(app, &pinned, tag.as_deref(), &allow_downgrade)
        .unwrap_or_else(|e| json!({ "indexOk": false, "applied": [], "rejected": 0, "error": e }));
    sidecar_contract::Response { result: Ok(body) }
}

/// Collect a JSON array field of strings from `params` (empty if absent/malformed).
#[cfg(feature = "sidecar-platform")]
fn str_list(params: &serde_json::Value, key: &str) -> Vec<String> {
    params
        .get(key)
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

/// Response to `host.list_catalog_versions` (CPE-383): enumerate prior published catalog versions
/// from the GitHub Releases API. Never errors the channel — a failure/offline comes back as an empty
/// list with a message.
#[cfg(feature = "sidecar-platform")]
fn list_catalog_versions_response(params: &serde_json::Value) -> sidecar_contract::Response {
    use serde_json::json;
    let _ = params;
    let body = match list_catalog_versions() {
        Ok(versions) => json!({ "versions": versions }),
        Err(e) => json!({ "versions": [], "error": e }),
    };
    sidecar_contract::Response { result: Ok(body) }
}

/// Enumerate published catalog versions via the GitHub Releases API (CPE-383). Offline ⇒ empty list
/// (never a surprise call). Proxy-aware via the shared `catalog_http_get`.
#[cfg(feature = "sidecar-platform")]
fn list_catalog_versions() -> Result<Vec<serde_json::Value>, String> {
    if keyverify::is_offline(std::env::var("CPE_OFFLINE").ok()) {
        return Ok(vec![]);
    }
    let body = catalog_http_get(&github_releases_api())?;
    Ok(parse_release_versions(&body))
}

#[cfg(feature = "sidecar-platform")]
fn do_fetch_catalog(
    app: &tauri::AppHandle,
    pinned: &[String],
    tag: Option<&str>,
    allow_downgrade: &[String],
) -> Result<serde_json::Value, String> {
    use serde_json::json;
    if keyverify::is_offline(std::env::var("CPE_OFFLINE").ok()) {
        return Ok(json!({ "indexOk": false, "applied": [], "rejected": 0, "offline": true }));
    }
    let keys: Vec<String> = CATALOG_TRUSTED_KEYS.iter().map(|s| s.to_string()).collect();
    // CPE-383: a specific prior version fetches from `releases/download/<tag>/` (not `latest`), and
    // its `agents` are allowed to downgrade. A malformed tag is refused (no URL-path escape).
    let base = match tag {
        Some(t) => {
            if !is_safe_release_tag(t) {
                return Err(format!("unsafe release tag: {t}"));
            }
            catalog_url_for_tag(t)
        }
        None => catalog_url(),
    };
    let dir = catalog_dir(app);
    let staging = std::env::temp_dir().join(format!("cpe-catalog-stage-{}", std::process::id()));
    std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;

    // Index + its detached signature.
    let index_bytes = catalog_http_get(&format!("{base}catalog-index.json"))?;
    let index_sig = catalog_http_get(&format!("{base}catalog-index.json.sig"))?;
    std::fs::write(staging.join("index.json"), &index_bytes).map_err(|e| e.to_string())?;
    std::fs::write(staging.join("index.json.sig"), &index_sig).map_err(|e| e.to_string())?;

    // Each listed manifest + its signature.
    let index = sidecar_host::catalog::CatalogIndex::from_json(&String::from_utf8_lossy(&index_bytes))?;
    for entry in &index.entries {
        let m = catalog_http_get(&format!("{base}{}.json", entry.id))?;
        let s = catalog_http_get(&format!("{base}{}.json.sig", entry.id))?;
        std::fs::write(staging.join(format!("{}.json", entry.id)), &m).map_err(|e| e.to_string())?;
        std::fs::write(staging.join(format!("{}.json.sig", entry.id)), &s).map_err(|e| e.to_string())?;
    }

    // Apply with anti-rollback against the persisted version map (last-known-good on failure).
    let vpath = dir.join("versions.json");
    let mut versions = sidecar_host::catalog::load_versions(&vpath);
    let report = sidecar_host::catalog::apply_bundle_with(
        &staging,
        &dir,
        &keys,
        &mut versions,
        pinned,
        allow_downgrade,
    );
    let _ = sidecar_host::catalog::save_versions(&vpath, &versions);
    let _ = std::fs::remove_dir_all(&staging);
    Ok(json!({ "indexOk": report.index_ok, "applied": report.applied, "rejected": report.rejected.len() }))
}

/// One allow-listed HTTPS GET for a catalog asset (CPE-376), proxy/offline-aware (reuses CPE-369).
/// The host builds every URL from `catalog_url()` — the sidecar never supplies one (no SSRF).
#[cfg(feature = "sidecar-platform")]
fn catalog_http_get(url: &str) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let host = keyverify::host_of(url);
    let mut builder = ureq::AgentBuilder::new();
    if let Some(p) = keyverify::resolve_proxy(host, |k| std::env::var(k).ok()) {
        if let Ok(px) = ureq::Proxy::new(&p) {
            builder = builder.proxy(px);
        }
    }
    let resp = builder
        .build()
        .get(url)
        .timeout(std::time::Duration::from_secs(20))
        // GitHub (Releases API + downloads) requires a User-Agent; the Accept keeps the API on its
        // stable JSON contract. Harmless for the plain download host.
        .set("User-Agent", "cross-platform-explorer")
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("fetch failed: {e}"))?;
    let mut buf = Vec::new();
    resp.into_reader().read_to_end(&mut buf).map_err(|e| e.to_string())?;
    Ok(buf)
}

/// The GitHub release tag carrying the signed model-catalog snapshot (CPE-450/451): a single,
/// continuously-updated release whose assets are `models-index.json` (the canonical JSON) and
/// `models-index.json.sig` (its detached ed25519 signature, hex).
#[cfg(feature = "sidecar-platform")]
const MODEL_SNAPSHOT_TAG: &str = "model-catalog";

/// The `releases/download/model-catalog/` base for the published model snapshot (CPE-451). Reuses
/// `catalog_url_for_tag` so a `CPE_CATALOG_URL` test override applies the same way as for the agent
/// catalog.
#[cfg(feature = "sidecar-platform")]
fn model_snapshot_url() -> String {
    catalog_url_for_tag(MODEL_SNAPSHOT_TAG)
}

/// Response to the sandboxed `host.fetch_model_snapshot` request (CPE-451): download the published
/// model-catalog snapshot (`models-index.json` + its detached `.sig`) from the `model-catalog`
/// GitHub release using the SAME allow-listed / proxy / offline machinery as `do_fetch_catalog`.
///
/// The host deliberately does NOT verify the signature — the AI Console owns the crypto
/// (`model_snapshot::verify_snapshot`) and is the sole trust boundary; this handler only fetches the
/// raw bytes. Never errors the channel — a failure/offline comes back as `{ ok:false, error }`, and
/// success as `{ ok:true, index, sig }` (both raw strings).
#[cfg(feature = "sidecar-platform")]
fn fetch_model_snapshot_response(params: &serde_json::Value) -> sidecar_contract::Response {
    use serde_json::json;
    let _ = params;
    let body = match fetch_model_snapshot() {
        Ok((index, sig)) => json!({ "ok": true, "index": index, "sig": sig }),
        Err(e) => json!({ "ok": false, "error": e }),
    };
    sidecar_contract::Response { result: Ok(body) }
}

/// Fetch the two snapshot assets from the `model-catalog` release (CPE-451). Offline ⇒ a clean
/// error (never a surprise call). Each URL is host-built from `model_snapshot_url()` — the sidecar
/// never supplies one (no SSRF) — and rides the shared proxy-aware `catalog_http_get`.
#[cfg(feature = "sidecar-platform")]
fn fetch_model_snapshot() -> Result<(String, String), String> {
    if keyverify::is_offline(std::env::var("CPE_OFFLINE").ok()) {
        return Err("offline".to_string());
    }
    let base = model_snapshot_url();
    let index = catalog_http_get(&format!("{base}models-index.json"))?;
    let sig = catalog_http_get(&format!("{base}models-index.json.sig"))?;
    let index = String::from_utf8(index).map_err(|e| format!("snapshot index not utf-8: {e}"))?;
    let sig = String::from_utf8(sig).map_err(|e| format!("snapshot signature not utf-8: {e}"))?;
    Ok((index, sig))
}

#[cfg(feature = "sidecar-platform")]
fn serve_ai_console_requests(
    mut conn: sidecar_host::supervisor::ProcessConnection,
    granted: std::collections::BTreeSet<sidecar_contract::Capability>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    storage_base: std::path::PathBuf,
    app: tauri::AppHandle,
) {
    use sidecar_contract::{Envelope, Message};
    use sidecar_host::conformance::SidecarChannel;

    let mut broker = sidecar_host::broker::Broker::new();
    // The keychain-backed secrets provider: Windows Credential Manager, macOS Keychain, or Linux
    // Secret Service (CPE-268/322). On any other target the broker simply has no secrets provider
    // and denies cleanly.
    #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
    broker.register_provider(Box::new(sidecar_host::providers::secrets::SecretsProvider::new(
        sidecar_host::providers::secrets::KeyringBackend,
    )));
    // Storage: a private per-sidecar directory the console persists its presets under (CPE-352).
    broker.register_provider(Box::new(sidecar_host::providers::storage::StorageProvider::new(
        storage_base,
    )));
    broker.set_grants("ai-console", granted);

    loop {
        if stop.load(std::sync::atomic::Ordering::SeqCst) {
            break;
        }
        match conn.recv() {
            Ok(env) => {
                let env_id = env.id;
                match env.message {
                    Message::Request(req) => {
                        // host.pick_folder is a host UI action, not a brokered capability — handle
                        // it directly by opening the native folder dialog (CPE-354).
                        let resp = if req.method == "host.pick_folder" {
                            pick_folder_response(&app, &req.params)
                        } else if req.method == "host.verify_key" {
                            // A live key check against an allow-listed provider endpoint (CPE-347),
                            // not a brokered capability — handle it directly.
                            verify_key_response(&req.params)
                        } else if req.method == "host.list_models" {
                            // Allow-listed reseller model-list fetch (CPE-447), host-side endpoint —
                            // not a brokered capability; handle it directly like verify_key.
                            list_models_response(&req.params)
                        } else if req.method == "host.fetch_catalog" {
                            // Fetch + apply the signed catalog bundle from GitHub Releases (CPE-376);
                            // an optional `tag`+`agents` rolls chosen agents back to a prior version
                            // (CPE-383).
                            fetch_catalog_response(&app, &req.params)
                        } else if req.method == "host.list_catalog_versions" {
                            // Enumerate prior published catalog versions for the rollback picker
                            // (CPE-383) — a host-built GitHub Releases API GET, no sidecar URL.
                            list_catalog_versions_response(&req.params)
                        } else if req.method == "host.fetch_model_snapshot" {
                            // Download the signed model-catalog snapshot from the `model-catalog`
                            // release (CPE-451). The host only fetches raw bytes; the console
                            // verifies the ed25519 signature + anti-rollback before adopting it.
                            fetch_model_snapshot_response(&req.params)
                        } else {
                            broker.dispatch("ai-console", &req)
                        };
                        if conn.send(&Envelope::new(env_id, Message::Response(resp))).is_err() {
                            break; // sidecar's stdin closed
                        }
                    }
                    // Agent Watch (CPE-396): the console announces session start/end as a
                    // `session:<json>` Status. Forward it to the frontend so the explorer can list
                    // active agent sessions and locate their Project folders.
                    Message::Event(sidecar_contract::Event::Status { state })
                        if state.starts_with("session:") =>
                    {
                        use tauri::Emitter;
                        let _ = app.emit("ai-console://session", state);
                    }
                    // Agent Watch reads (CPE-405): the console reports a file the agent READ (parsed
                    // from its tool-output stream, since an FS watcher can't see reads) as an
                    // `fs-read:<json {path}>` Status. Merge it into the SAME `fs-activity` channel as
                    // watcher mutations, tagged `kind:"read"`, so the timeline + row annotations show
                    // it. Malformed payloads are ignored (never block the terminal).
                    Message::Event(sidecar_contract::Event::Status { state })
                        if state.starts_with("fs-read:") =>
                    {
                        use tauri::Emitter;
                        if let Ok(v) =
                            serde_json::from_str::<serde_json::Value>(&state["fs-read:".len()..])
                        {
                            if let Some(path) = v.get("path").and_then(|p| p.as_str()) {
                                let _ = app.emit(
                                    "ai-console://fs-activity",
                                    serde_json::json!([{ "kind": "read", "path": path }]),
                                );
                            }
                        }
                    }
                    // Other non-request frames (Lifecycle, other Status) need no reply here.
                    _ => {}
                }
            }
            // A poll timeout is normal — loop to re-check `stop`. Anything else means the
            // sidecar closed the connection.
            Err(e) if e.contains("timed out") => continue,
            Err(_) => break,
        }
    }
}

// --- Agent Watch: filesystem-activity watcher (CPE-398) --------------------------------

/// The live filesystem watcher for Agent Watch — at most one at a time, on the currently-watched
/// agent's Project folder. Holding the `notify` watcher here keeps it alive; dropping it (a new
/// watch, or `agent_watch_stop`) stops watching AND ends its emitter thread (the event channel
/// closes). Off means off (AGENT-WATCH.md): with nothing watched there is no watcher and no thread.
#[cfg(feature = "sidecar-platform")]
#[derive(Default)]
struct AgentWatchState {
    current: std::sync::Mutex<Option<AgentWatch>>,
}

#[cfg(feature = "sidecar-platform")]
struct AgentWatch {
    _watcher: notify::RecommendedWatcher,
    #[allow(dead_code)]
    path: String,
}

/// Map a raw `notify` event to the coarse Agent Watch activity kind, or `None` to ignore it.
/// Reads (`Access`) are deliberately dropped — a Windows watcher can't see them anyway, so reads
/// are out of scope here (they'd need the agent's own tool stream; see CPE-398).
#[cfg(feature = "sidecar-platform")]
fn classify_fs_event(kind: &notify::EventKind) -> Option<&'static str> {
    use notify::event::ModifyKind;
    use notify::EventKind::*;
    match kind {
        Create(_) => Some("created"),
        Modify(ModifyKind::Name(_)) => Some("renamed"),
        Modify(_) => Some("modified"),
        Remove(_) => Some("removed"),
        _ => None, // Access / Other
    }
}

/// Coalescing emitter: fold raw watcher events per-path over a short window and flush batches to
/// the frontend as `ai-console://fs-activity`. Bounded so a big refactor can't flood the UI — the
/// pending set is capped and flushed early when full. Ends when the channel closes (watcher dropped).
#[cfg(feature = "sidecar-platform")]
fn fs_activity_pump(
    app: tauri::AppHandle,
    rx: std::sync::mpsc::Receiver<notify::Result<notify::Event>>,
) {
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::mpsc::RecvTimeoutError;
    use std::time::{Duration, Instant};
    use tauri::Emitter;

    const FLUSH: Duration = Duration::from_millis(200);
    const CAP: usize = 500;
    let mut pending: HashMap<String, &'static str> = HashMap::new();
    let mut last_flush = Instant::now();

    let flush = |app: &tauri::AppHandle, pending: &mut HashMap<String, &'static str>| {
        if pending.is_empty() {
            return;
        }
        let items: Vec<_> =
            pending.drain().map(|(path, kind)| json!({ "kind": kind, "path": path })).collect();
        let _ = app.emit("ai-console://fs-activity", items);
    };

    loop {
        match rx.recv_timeout(FLUSH) {
            Ok(Ok(event)) => {
                if let Some(kind) = classify_fs_event(&event.kind) {
                    for p in event.paths {
                        // A `removed` wins over a same-window `created`/`modified` so a file the
                        // agent creates then deletes reads as gone, not as churn.
                        let path = p.to_string_lossy().into_owned();
                        let slot = pending.entry(path).or_insert(kind);
                        if kind == "removed" || *slot != "removed" {
                            *slot = kind;
                        }
                    }
                }
                if pending.len() >= CAP {
                    flush(&app, &mut pending);
                    last_flush = Instant::now();
                }
            }
            Ok(Err(_)) => {} // a watch error — ignore, keep pumping
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                flush(&app, &mut pending);
                break;
            }
        }
        if last_flush.elapsed() >= FLUSH {
            flush(&app, &mut pending);
            last_flush = Instant::now();
        }
    }
}

/// Start watching an agent's Project folder for filesystem activity (CPE-398). Replaces any
/// existing watch. Non-fatal: returns an error string the caller can surface. A missing folder
/// (e.g. a since-deleted path) is rejected rather than silently watching nothing.
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn agent_watch_start(
    app: tauri::AppHandle,
    state: tauri::State<AgentWatchState>,
    path: String,
) -> Result<(), String> {
    use notify::{RecursiveMode, Watcher};
    let dir = std::path::Path::new(&path);
    if !dir.is_dir() {
        return Err(format!("not a folder: {path}"));
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |res| {
        let _ = tx.send(res);
    })
    .map_err(|e| e.to_string())?;
    watcher.watch(dir, RecursiveMode::Recursive).map_err(|e| e.to_string())?;
    // Dropping the watcher (below, when we replace `current`, or on stop) closes `rx` and ends
    // this thread — no separate stop signal needed.
    std::thread::spawn(move || fs_activity_pump(app, rx));
    *state.current.lock().unwrap() = Some(AgentWatch { _watcher: watcher, path });
    Ok(())
}

/// Stop watching (CPE-398). Dropping the stored watcher ends its emitter thread. Idempotent.
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn agent_watch_stop(state: tauri::State<AgentWatchState>) {
    *state.current.lock().unwrap() = None;
}

#[cfg(feature = "sidecar-platform")]
impl Default for AiConsoleState {
    fn default() -> Self {
        Self {
            conn: std::sync::Mutex::new(None),
            // A small ring buffer: enough recent lines to diagnose a failed launch or
            // crash without growing without bound (CPE-298).
            logs: sidecar_host::observability::LogCapture::new(200),
            last_error: std::sync::Mutex::new(None),
            url: std::sync::Mutex::new(None),
            daemon: std::sync::Mutex::new(None),
        }
    }
}

#[cfg(feature = "sidecar-platform")]
impl AiConsoleState {
    /// Append a lifecycle log line for the AI Console.
    fn log(&self, level: sidecar_host::observability::LogLevel, message: impl Into<String>) {
        self.logs.push(sidecar_host::observability::LogRecord {
            correlation_id: 0,
            sidecar_id: "ai-console".into(),
            level,
            message: message.into(),
        });
    }

    /// Record (and log) the error that stopped the AI Console from starting, then return
    /// it so callers can `?`/propagate the same string to the frontend.
    fn fail(&self, message: impl Into<String>) -> String {
        let message = message.into();
        self.log(sidecar_host::observability::LogLevel::Error, message.clone());
        if let Ok(mut slot) = self.last_error.lock() {
            *slot = Some(message.clone());
        }
        message
    }

    /// Clear the last-error marker after a successful start.
    fn clear_error(&self) {
        if let Ok(mut slot) = self.last_error.lock() {
            *slot = None;
        }
    }

    /// Ensure the host-owned session daemon (CPE-309 S4) is running and return its loopback port.
    /// Reuses a live one; (re)spawns `<bin> --session-daemon` if absent or dead. Spawned with a
    /// hidden console (`CREATE_NO_WINDOW`, matching how the UI sidecar is spawned — so ConPTY works)
    /// and owned by the host, so it outlives the UI sidecar being restarted/toggled. Returns `None`
    /// on any failure so the caller falls back to in-process sessions rather than blocking a launch.
    fn ensure_session_daemon(&self, bin: &str) -> Option<u16> {
        use std::io::{BufRead, BufReader};
        let mut guard = self.daemon.lock().ok()?;
        if let Some(d) = guard.as_mut() {
            if matches!(d.child.try_wait(), Ok(None)) {
                return Some(d.port); // still alive → reuse
            }
        }
        let mut cmd = std::process::Command::new(bin);
        cmd.arg("--session-daemon")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW — hidden console so ConPTY has output
        }
        let mut child = cmd.spawn().ok()?;
        let stdout = child.stdout.take()?;
        let mut line = String::new();
        BufReader::new(stdout).read_line(&mut line).ok()?;
        let port = line.trim().strip_prefix("PORT ").and_then(|p| p.parse::<u16>().ok())?;
        self.log(
            sidecar_host::observability::LogLevel::Info,
            format!("session daemon ready on 127.0.0.1:{port}"),
        );
        *guard = Some(HostSessionDaemon { child, port });
        Some(port)
    }
}

/// Locate the bundled `ai-console` sidecar binary. Order: explicit override env var, the
/// app's resource dir, then a dev-tree fallback. Returns an error string if not found so
/// the caller can degrade gracefully rather than panic.
#[cfg(feature = "sidecar-platform")]
fn resolve_ai_console_bin(app: &tauri::AppHandle) -> Result<String, String> {
    use tauri::Manager;
    let exe = if cfg!(windows) { "ai-console.exe" } else { "ai-console" };

    if let Ok(p) = std::env::var("CPE_AICONSOLE_BIN") {
        if Path::new(&p).exists() {
            return Ok(p);
        }
    }
    if let Ok(resource) = app.path().resource_dir() {
        let p = resource.join("sidecars").join(exe);
        if p.exists() {
            return Ok(p.to_string_lossy().into_owned());
        }
    }
    // Dev fallback: resolve relative to THIS crate (src-tauri) at compile time, not the
    // runtime CWD — `cargo tauri dev` runs the app with cwd = src-tauri, so a plain
    // relative path would miss. `../sidecar/ai-console/target/<profile>/<exe>`.
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    for profile in ["debug", "release"] {
        for base in [manifest.join("../sidecar/ai-console/target"), PathBuf::from("sidecar/ai-console/target")] {
            let p = base.join(profile).join(exe);
            if p.exists() {
                return Ok(p.to_string_lossy().into_owned());
            }
        }
    }
    Err(format!("ai-console binary ('{exe}') not found"))
}

/// Spawn (or reuse) the AI Console sidecar, complete the handshake, and return the URL of
/// the UI it serves so the frontend can mount it in an iframe pane (CPE-271). Non-fatal:
/// returns an error string that the UI surfaces, never panicking the explorer.
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn sidecar_start_ai_console(
    app: tauri::AppHandle,
    state: tauri::State<AiConsoleState>,
) -> Result<String, String> {
    use sidecar_contract::{Event, Message, CONTRACT_VERSION};
    use sidecar_host::conformance::SidecarChannel; // brings `.recv()` into scope
    use sidecar_host::observability::LogLevel;
    use sidecar_host::supervisor::{handshake, spawn_process_with_env};
    use tauri::Manager; // for app.path()

    // Respect the enable/disable toggle (CPE-274): a disabled sidecar must not start.
    if !sidecar_host::enablement::EnablementStore::load(&consent_dir(&app)?).is_enabled("ai-console") {
        return Err(state.fail("the AI Console is disabled"));
    }

    // Reuse a still-running sidecar (CPE-464). Closing + reopening the AI Console window must NOT
    // spawn a second sidecar — that would drop the old one and kill its live agent sessions. If one
    // is already running, return its served URL so the reopened window loads the SAME sidecar and
    // reattaches to the live sessions (CPE-461).
    if state.conn.lock().map(|g| g.is_some()).unwrap_or(false) {
        if let Some(url) = state.url.lock().ok().and_then(|g| g.clone()) {
            state.log(LogLevel::Info, "reusing running ai-console");
            return Ok(url);
        }
    }

    state.log(LogLevel::Info, "starting ai-console");
    let bin = resolve_ai_console_bin(&app).map_err(|e| state.fail(e))?;
    // Tell the sidecar where the (fetched) catalog lives + which key to trust, so it loads and can
    // reload verified updates (CPE-376). Empty keys until CPE-377 ⇒ nothing is trusted (dormant).
    let cat_dir = catalog_dir(&app);
    let _ = std::fs::create_dir_all(&cat_dir);
    let cat_dir_str = cat_dir.to_string_lossy().into_owned();
    let cat_keys = CATALOG_TRUSTED_KEYS.join(",");
    // CPE-309 S4: the host-owned session daemon (sessions survive a UI-sidecar restart) is **opt-in**
    // behind `CPE_AICONSOLE_DAEMON=1`. It is NOT the default because in the real GUI the daemon path
    // still shows no PTY output (black terminal) — a deeper issue than the console flag, still being
    // diagnosed. The default is the proven in-process engine, so the AI Console always works. When
    // opted in, the daemon addr is passed and the sidecar routes sessions to it.
    let daemon_addr = if std::env::var("CPE_AICONSOLE_DAEMON").is_ok() {
        state.ensure_session_daemon(&bin).map(|port| format!("127.0.0.1:{port}"))
    } else {
        None
    };
    let mut cat_env = vec![
        ("CPE_AICONSOLE_CATALOG", cat_dir_str.as_str()),
        ("CPE_AICONSOLE_CATALOG_KEYS", cat_keys.as_str()),
    ];
    if let Some(addr) = daemon_addr.as_deref() {
        cat_env.push(("CPE_AICONSOLE_SESSION_DAEMON_ADDR", addr));
    }
    let mut conn =
        spawn_process_with_env(&bin, &[], &cat_env).map_err(|e| state.fail(format!("spawn failed: {e}")))?;
    let token = conn.launch_token().to_string();

    // Grant only what the user consented to (CPE-296). The frontend prompts for any
    // undecided capability before calling this; whatever isn't granted is simply withheld,
    // and the sidecar degrades gracefully (it still serves its UI, capability calls are
    // denied by the broker).
    let consented = sidecar_host::consent::ConsentStore::load(&consent_dir(&app)?).granted("ai-console");
    handshake(&mut conn, CONTRACT_VERSION, &consented, Some(&token))
        .map_err(|e| state.fail(format!("handshake failed: {e:?}")))?;
    state.log(LogLevel::Info, "handshake ok");

    // Read a bounded number of frames for the `ui:<url>` announcement.
    let mut url = None;
    for _ in 0..20 {
        match conn.recv() {
            Ok(env) => {
                if let Message::Event(Event::Status { state }) = env.message {
                    if let Some(u) = state.strip_prefix("ui:") {
                        url = Some(u.to_string());
                        break;
                    }
                }
            }
            Err(_) => break,
        }
    }
    let url = url.ok_or_else(|| state.fail("the AI Console did not announce a UI"))?;
    state.log(LogLevel::Info, format!("ui ready at {url}"));
    state.clear_error(); // a clean start clears any prior failure marker

    // Hand the connection to a servicing thread so the sidecar's capability requests (secrets
    // for the Keys panel) are actually answered (CPE-349). The control handle in state keeps
    // "running" = is_some() and lets stop/disable end it by dropping the handle.
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let thread_stop = stop.clone();
    // Host-owned base for the sidecar's private storage (presets etc.); the provider roots
    // each sidecar's dir under it by id.
    let storage_base = app
        .path()
        .app_data_dir()
        .map(|d| d.join("sidecar-storage"))
        .unwrap_or_else(|_| std::env::temp_dir().join("cpe-sidecar-storage"));
    let app_for_thread = app.clone();
    let thread = std::thread::spawn(move || {
        serve_ai_console_requests(conn, consented, thread_stop, storage_base, app_for_thread)
    });
    *state.conn.lock().map_err(|_| "state lock poisoned")? = Some(ConsoleConn { stop, _thread: thread });
    // Remember the served URL so a reopen reuses this same sidecar (CPE-464).
    *state.url.lock().map_err(|_| "state lock poisoned")? = Some(url.clone());
    Ok(url)
}

/// One redacted log line in a diagnostics response (CPE-323).
#[cfg(feature = "sidecar-platform")]
#[derive(serde::Serialize)]
struct DiagLogLine {
    /// Severity, snake_case (`info` / `warn` / `error` / …).
    level: String,
    /// The log message, run through the redactor — never contains a secret.
    message: String,
}

/// A sidecar's health for the management panel (CPE-323): running state, the last error
/// that stopped it (if any), and recent log lines. Every string here is redacted.
#[cfg(feature = "sidecar-platform")]
#[derive(serde::Serialize)]
struct SidecarDiagnostics {
    id: String,
    running: bool,
    /// The last start/crash error, redacted, or `None` if the sidecar is healthy.
    last_error: Option<String>,
    /// Recent log lines, oldest first, already redacted.
    logs: Vec<DiagLogLine>,
}

/// Return a sidecar's last error and recent, REDACTED log lines for the management panel
/// (CPE-323). Only the AI Console currently produces live logs; other registered sidecars
/// return an empty (but valid) diagnostics record so the panel can render uniformly.
///
/// Redaction is defence-in-depth: every message runs through
/// [`Redactor::redact_log_line`], which masks registered secrets *and* heuristic secret
/// shapes (API-key prefixes, bearer tokens, `sensitive_key=value`), so a secret can never
/// surface here even if one reached a log line.
/// Browse a remote repo's tree for the Repositories left-pane view (CPE-434/435). Uses the
/// host-brokered, allow-listed forge egress (`forge_egress`) — public GitHub needs no token; an
/// optional token enables private repos. `repo` is `owner/name`, `path` is a subfolder (or empty for
/// the root). Returns folders-first entries, or an actionable error message.
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn forge_browse(
    provider: String,
    repo: String,
    path: Option<String>,
    token: Option<String>,
) -> Result<Vec<forge_egress::RepoEntry>, String> {
    let sub = path.unwrap_or_default();
    let api_path = forge_egress::browse_path(&provider, &repo, &sub);
    let (status, body) =
        forge_egress::forge_request(&provider, "GET", None, &api_path, token.as_deref(), None)
            .map_err(|e| format!("Couldn't reach the repo ({e:?})."))?;
    if !(200..300).contains(&status) {
        return Err(match status {
            404 => format!("Repo '{repo}' not found (or private — add a token)."),
            401 | 403 => "Access denied — check the token.".to_string(),
            s => format!("Couldn't browse '{repo}': HTTP {s}."),
        });
    }
    Ok(forge_egress::parse_browse(&provider, &body))
}

/// Map a known forge `provider` to its **fixed** clone host and the username git expects alongside a
/// token. The host is chosen here, never taken from the caller — the same SSRF-hygiene rule as
/// `forge_egress`: a caller supplies `owner/name`, never a scheme or host. `None` means we don't
/// clone from this provider (e.g. self-hosted kinds with no fixed host). Matched by leading segment
/// so `github-personal` still maps to github.com, while `github-enterprise` is refused.
#[cfg(feature = "sidecar-platform")]
fn clone_host(provider: &str) -> Option<(&'static str, &'static str)> {
    let p = provider.to_ascii_lowercase();
    let is = |needle: &str| p == needle || p.starts_with(&format!("{needle}-"));
    // Self-hosted kinds have no fixed clone host — refuse them before the hosted-prefix checks.
    if is("github-enterprise") || is("gitea") || is("forgejo") {
        None
    } else if is("github") {
        Some(("github.com", "x-access-token"))
    } else if is("gitlab") {
        Some(("gitlab.com", "oauth2"))
    } else if is("bitbucket") {
        Some(("bitbucket.org", "x-token-auth"))
    } else if is("codeberg") {
        Some(("codeberg.org", "oauth2"))
    } else {
        None
    }
}

/// True if `repo` is a safe `owner/name` slug (optionally deeper for GitLab subgroups): at least two
/// non-empty `[A-Za-z0-9._-]` segments, no `..`, no leading `-`. Anything else must not be
/// interpolated into the clone URL.
#[cfg(feature = "sidecar-platform")]
fn is_safe_repo_slug(repo: &str) -> bool {
    let segs: Vec<&str> = repo.split('/').collect();
    segs.len() >= 2
        && segs.iter().all(|s| {
            !s.is_empty()
                && *s != ".."
                && !s.starts_with('-')
                && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        })
}

/// True if `token` is url-safe enough to embed in the clone URL's userinfo without breaking the
/// authority or smuggling a second URL. Deliberately strict — a real PAT is `[A-Za-z0-9_.~-]`.
#[cfg(feature = "sidecar-platform")]
fn is_safe_token(token: &str) -> bool {
    !token.is_empty()
        && token.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '~'))
}

/// Build the hardened `git clone` argv for `(provider, repo, target_dir, token?)`. Pure and cleanly
/// testable: the clone URL is assembled host-side from the fixed provider host, then handed to the
/// **already-tested** hardened builder in the repos crate (threat-model §C: empty hooksPath, no
/// ext/file transports, no fsmonitor, no submodule recursion, `--` before url/target). A token is
/// injected as userinfo for a private clone; on any failure the token value is NEVER echoed.
#[cfg(feature = "sidecar-platform")]
fn build_git_clone(
    provider: &str,
    repo: &str,
    target_dir: &str,
    token: Option<&str>,
) -> Result<Vec<String>, String> {
    let (host, token_user) = clone_host(provider)
        .ok_or_else(|| format!("Cloning isn't supported for provider '{provider}'."))?;
    let slug = repo.trim().trim_matches('/');
    if !is_safe_repo_slug(slug) {
        return Err("Repository must be in 'owner/name' form.".to_string());
    }
    // URL built host-side from the fixed host — the caller never supplies a scheme/host.
    let url = match token {
        Some(t) => {
            if !is_safe_token(t) {
                // Never echo the token itself in the error.
                return Err("The access token contains unsupported characters.".to_string());
            }
            format!("https://{token_user}:{t}@{host}/{slug}.git")
        }
        None => format!("https://{host}/{slug}.git"),
    };
    repos::build_clone_args(&repos::CloneRequest {
        url,
        target_dir: target_dir.to_string(),
        depth: None,
        branch: None,
    })
    .map_err(|e| match e {
        repos::CloneError::BadUrl => "The clone URL was rejected as unsafe.".to_string(),
        repos::CloneError::BadTarget => {
            "The target must be an absolute path to a fresh, non-repo folder.".to_string()
        }
        repos::CloneError::BadRef => "The requested branch name was rejected.".to_string(),
    })
}

/// Clone a remote repo (CPE-436) from a known forge `provider` into `target_dir`. The clone URL is
/// built host-side from the provider allow-list (`clone_host`) — the caller supplies only `owner/name`,
/// never a scheme or host (SSRF hygiene, as in `forge_egress`). git runs with the hardened argv from
/// the repos crate (threat-model §C). An optional `token` clones a private repo: it is injected into
/// the URL for git and is NEVER logged — and is scrubbed from any git error text before it is returned.
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn forge_clone(
    provider: String,
    repo: String,
    target_dir: String,
    token: Option<String>,
) -> Result<String, String> {
    let args = build_git_clone(&provider, &repo, &target_dir, token.as_deref())?;
    let output = std::process::Command::new("git")
        .args(&args)
        .output()
        .map_err(|e| format!("Couldn't run git: {e}"))?;
    if output.status.success() {
        Ok(format!("Cloned {} into {target_dir}.", repo.trim().trim_matches('/')))
    } else {
        let mut stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        // Defence-in-depth: never surface the token, even if git echoed the URL in an error.
        if let Some(t) = token.as_deref() {
            if !t.is_empty() {
                stderr = stderr.replace(t, "***");
            }
        }
        if stderr.is_empty() {
            stderr = format!("git clone failed (exit {:?}).", output.status.code());
        }
        Err(stderr)
    }
}

// --- Generic Git provider + consent-based host admission (CPE-498) ------------------------------

/// Where the Generic-Git egress allow-list persists: a JSON array of admitted hostnames under the app
/// data dir. A host lands here ONLY after the user explicitly consents in the UI; it is the host-side
/// gate `forge_clone_url` checks before letting git reach an arbitrary (self-hosted) host — no wildcard,
/// no silent admission (threat-model Q5).
#[cfg(feature = "sidecar-platform")]
fn admitted_hosts_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    use tauri::Manager;
    let dir = app.path().app_data_dir().map_err(|_| "no app data dir".to_string())?;
    Ok(dir.join("forge-admitted-hosts.json"))
}

/// The consented Generic-Git egress allow-list (normalized hosts). Missing/corrupt ⇒ empty (fail
/// closed: an unreadable list admits nothing).
#[cfg(feature = "sidecar-platform")]
fn load_admitted_hosts(app: &tauri::AppHandle) -> std::collections::BTreeSet<String> {
    let path = match admitted_hosts_path(app) {
        Ok(p) => p,
        Err(_) => return Default::default(),
    };
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).ok())
        .map(|v| v.into_iter().map(|h| repos::normalize_host(&h)).collect())
        .unwrap_or_default()
}

#[cfg(feature = "sidecar-platform")]
fn save_admitted_hosts(
    app: &tauri::AppHandle,
    hosts: &std::collections::BTreeSet<String>,
) -> Result<(), String> {
    let path = admitted_hosts_path(app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(&hosts.iter().collect::<Vec<_>>())
        .map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

/// What the Generic-Git consent prompt needs: the parsed host, transport, credential-stripped URL,
/// and whether the host is already admitted.
#[cfg(feature = "sidecar-platform")]
#[derive(serde::Serialize)]
struct GenericRemoteInfo {
    host: String,
    /// "https" | "ssh".
    scheme: String,
    /// The remote with any embedded credentials stripped — safe to display.
    url: String,
    admitted: bool,
}

/// Parse an arbitrary git URL for the Generic-Git add flow (CPE-498): returns its host + a
/// credential-stripped URL + whether that host is already in the consent allow-list. Read-only — it
/// never admits anything. An unsupported transport is an error the UI can show.
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn forge_generic_remote(app: tauri::AppHandle, url: String) -> Result<GenericRemoteInfo, String> {
    let r = repos::parse_remote(&url)
        .ok_or_else(|| "Not a supported git URL (use https://, ssh://, or user@host:path).".to_string())?;
    let admitted = load_admitted_hosts(&app).contains(&r.host);
    Ok(GenericRemoteInfo {
        scheme: match r.scheme {
            repos::RemoteScheme::Https => "https",
            repos::RemoteScheme::Ssh => "ssh",
        }
        .to_string(),
        host: r.host,
        url: r.url,
        admitted,
    })
}

/// The Generic-Git egress allow-list — hosts the user has consented to reach (CPE-498).
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn forge_admitted_hosts(app: tauri::AppHandle) -> Vec<String> {
    load_admitted_hosts(&app).into_iter().collect()
}

/// Admit ONE host after explicit user consent (CPE-498). Never a wildcard and never a URL: exactly the
/// normalized host is stored, so consenting to `a.example.com` never admits `b.example.com`. Idempotent.
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn forge_admit_host(app: tauri::AppHandle, host: String) -> Result<(), String> {
    let host = repos::normalize_host(&host);
    if host.is_empty()
        || host.contains('*')
        || host.contains('/')
        || host.contains(char::is_whitespace)
    {
        return Err("Refusing to admit an invalid or wildcard host.".to_string());
    }
    let mut hosts = load_admitted_hosts(&app);
    hosts.insert(host);
    save_admitted_hosts(&app, &hosts)
}

/// Revoke a host from the Generic-Git allow-list (management; CPE-498).
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn forge_forget_host(app: tauri::AppHandle, host: String) -> Result<(), String> {
    let host = repos::normalize_host(&host);
    let mut hosts = load_admitted_hosts(&app);
    hosts.remove(&host);
    save_admitted_hosts(&app, &hosts)
}

/// Build the hardened `git clone` argv for an arbitrary git URL (Generic Git, CPE-498). Pure and
/// testable: parse → (host, cred-stripped url), refuse a non-admitted host, inject an https token as
/// userinfo, then defer to the repos crate's hardened builder. `admitted` is passed in so this stays
/// pure — the command below checks the persisted allow-list.
#[cfg(feature = "sidecar-platform")]
fn build_generic_clone(
    url: &str,
    target_dir: &str,
    token: Option<&str>,
    admitted: bool,
) -> Result<Vec<String>, String> {
    let r = repos::parse_remote(url)
        .ok_or_else(|| "Not a supported git URL (use https://, ssh://, or user@host:path).".to_string())?;
    if !admitted {
        return Err(format!(
            "Host '{}' hasn't been granted access. Grant it, then try again.",
            r.host
        ));
    }
    // A token applies only to https (ssh authenticates via the agent/keys). Injected as userinfo and
    // NEVER logged; scrubbed from any error by the caller.
    let clone_url = match (r.scheme, token) {
        (repos::RemoteScheme::Https, Some(t)) => {
            if !is_safe_token(t) {
                return Err("The access token contains unsupported characters.".to_string());
            }
            r.url.replacen("https://", &format!("https://{t}@"), 1)
        }
        _ => r.url,
    };
    repos::build_clone_args(&repos::CloneRequest {
        url: clone_url,
        target_dir: target_dir.to_string(),
        depth: None,
        branch: None,
    })
    .map_err(|e| match e {
        repos::CloneError::BadUrl => "The clone URL was rejected as unsafe.".to_string(),
        repos::CloneError::BadTarget => {
            "The target must be an absolute path to a fresh, non-repo folder.".to_string()
        }
        repos::CloneError::BadRef => "The requested branch name was rejected.".to_string(),
    })
}

/// Clone an ARBITRARY https/ssh git URL into `target_dir` (Generic Git, CPE-498) — the self-hosted /
/// unknown-forge path. Gated on the URL's host being in the consent allow-list; a non-admitted host is
/// refused (no silent admission). git runs with the repos crate's hardened argv; an https `token` is
/// injected for a private clone and is scrubbed from any error text.
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn forge_clone_url(
    app: tauri::AppHandle,
    url: String,
    target_dir: String,
    token: Option<String>,
) -> Result<String, String> {
    let admitted = repos::parse_remote(&url)
        .map(|r| load_admitted_hosts(&app).contains(&r.host))
        .unwrap_or(false);
    let args = build_generic_clone(&url, &target_dir, token.as_deref(), admitted)?;
    let output = std::process::Command::new("git")
        .args(&args)
        .output()
        .map_err(|e| format!("Couldn't run git: {e}"))?;
    if output.status.success() {
        Ok(format!("Cloned into {target_dir}."))
    } else {
        let mut stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if let Some(t) = token.as_deref() {
            if !t.is_empty() {
                stderr = stderr.replace(t, "***");
            }
        }
        if stderr.is_empty() {
            stderr = format!("git clone failed (exit {:?}).", output.status.code());
        }
        Err(stderr)
    }
}

/// Keychain "service" for forge tokens (CPE-439) — kept apart from sidecar secrets so a GitHub
/// token never collides with a sidecar's namespace. The account is the provider id.
#[cfg(feature = "sidecar-platform")]
const FORGE_TOKEN_SERVICE: &str = "com.cross-platform-explorer.forge";

/// Store a forge access token in the OS keychain so browse/clone don't need it re-typed (CPE-439).
/// Reuses the host's `KeyringBackend` (Windows Credential Manager / macOS Keychain / Linux Secret
/// Service). The token is never logged.
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn forge_set_token(provider: String, token: String) -> Result<(), String> {
    use sidecar_host::providers::secrets::{KeyringBackend, SecretBackend};
    if provider.trim().is_empty() {
        return Err("missing provider".into());
    }
    KeyringBackend.set(FORGE_TOKEN_SERVICE, &provider, &token)
}

/// Fetch the stored forge token for `provider` (CPE-439), or `None`. The value is returned only to
/// the app's own frontend over the IPC boundary; it is never logged.
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn forge_get_token(provider: String) -> Result<Option<String>, String> {
    use sidecar_host::providers::secrets::{KeyringBackend, SecretBackend};
    KeyringBackend.get(FORGE_TOKEN_SERVICE, &provider)
}

/// Forget a provider's stored forge token (CPE-439).
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn forge_delete_token(provider: String) -> Result<(), String> {
    use sidecar_host::providers::secrets::{KeyringBackend, SecretBackend};
    KeyringBackend.delete(FORGE_TOKEN_SERVICE, &provider)
}

/// The git sync state of a local folder for the two-way-mirror status bar (CPE-462), flattened from
/// the repos crate's `RepoState` + safe `SyncPlan` for the frontend. `is_repo` is false for a
/// non-repo (or when `git` isn't available).
#[cfg(feature = "sidecar-platform")]
#[derive(Default, serde::Serialize)]
struct RepoSyncStatus {
    is_repo: bool,
    branch: Option<String>,
    upstream: Option<String>,
    ahead: u32,
    behind: u32,
    dirty: bool,
    /// Planned safe steps: any of `pull-ff` / `pull-merge` / `pull-rebase` / `push`.
    actions: Vec<String>,
    up_to_date: bool,
    conflicts_possible: bool,
    blocked: Option<String>,
    warnings: Vec<String>,
    /// True when the working tree currently has unmerged files (a merge/rebase left conflicts) — the
    /// status bar surfaces a "Resolve…" entry into the CPE-496 resolver.
    conflicted: bool,
}

/// Report the git sync status of `path` (CPE-462) — read-only. Runs `git status --porcelain=v2
/// --branch`, parses it (`repos::parse_status`), and plans a **safe** two-way sync
/// (`repos::plan_sync`, never force). Used by the explorer's status bar to show ahead/behind and
/// offer Pull/Push. A non-repo (or no `git`) returns `is_repo:false`.
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn forge_repo_status(path: String, on_diverge: Option<String>) -> RepoSyncStatus {
    use repos::SyncAction;
    let output = std::process::Command::new("git")
        .args(["-C", &path, "status", "--porcelain=v2", "--branch"])
        .output();
    let out = match output {
        Ok(o) if o.status.success() => o,
        _ => return RepoSyncStatus::default(), // not a repo, or git unavailable
    };
    let stdout = String::from_utf8_lossy(&out.stdout);
    let state = repos::parse_status(&stdout);
    let conflicted = !repos::parse_conflicts(&stdout).is_empty(); // CPE-496 resolver entry point
    // The dry-run PREVIEW reflects the caller's chosen on-diverge policy (CPE-495); safe-by-default
    // (never force). Absent ⇒ the merge default (as the quick status-bar Pull/Push uses).
    let policy = repos::SyncPolicy {
        on_diverge: match on_diverge.as_deref() {
            Some("rebase") => repos::DivergePolicy::Rebase,
            Some("manual") => repos::DivergePolicy::Manual,
            _ => repos::DivergePolicy::Merge,
        },
        allow_force: false,
    };
    let plan = repos::plan_sync(&state, &policy);
    let actions = plan
        .actions
        .iter()
        .map(|a| match a {
            SyncAction::PullFastForward => "pull-ff",
            SyncAction::PullMerge => "pull-merge",
            SyncAction::PullRebase => "pull-rebase",
            SyncAction::Push => "push",
        }
        .to_string())
        .collect();
    RepoSyncStatus {
        is_repo: true,
        branch: state.branch,
        upstream: state.upstream,
        ahead: state.ahead,
        behind: state.behind,
        dirty: state.dirty,
        actions,
        up_to_date: plan.up_to_date,
        conflicts_possible: plan.conflicts_possible,
        blocked: plan.blocked,
        warnings: plan.warnings,
        conflicted,
    }
}

/// Execute one **safe** sync step on `path` (CPE-462): `pull` fast-forwards only (never clobbers
/// local work), `push` pushes without force. Anything that could rewrite history is refused —
/// diverged histories surface in `forge_repo_status` for the user to resolve. Returns git's output.
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn forge_sync(path: String, action: String) -> Result<String, String> {
    let args: Vec<&str> = match action.as_str() {
        // Safe pulls: fast-forward only never risks local work; merge/rebase reconcile a divergence and
        // MAY conflict — git returns non-zero and we surface its output for the user to resolve
        // (CPE-495/496). None of these ever force-push (there is no force action).
        "pull" | "pull-ff" => vec!["-C", &path, "pull", "--ff-only"],
        "pull-merge" => vec!["-C", &path, "pull", "--no-rebase"],
        "pull-rebase" => vec!["-C", &path, "pull", "--rebase"],
        "push" => vec!["-C", &path, "push"],
        other => return Err(format!("unsupported sync action '{other}'")),
    };
    let out = std::process::Command::new("git").args(&args).output().map_err(|e| e.to_string())?;
    if out.status.success() {
        let s = String::from_utf8_lossy(&out.stdout);
        Ok(if s.trim().is_empty() { format!("{action} ok") } else { s.trim().to_string() })
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

// --- In-app conflict resolver (CPE-496) ---------------------------------------------------------

/// One conflicted file for the resolver UI.
#[cfg(feature = "sidecar-platform")]
#[derive(serde::Serialize)]
struct ConflictFile {
    path: String,
    /// snake_case kind (`both_modified`, `added_by_us`, …).
    code: String,
    /// Human label ("both modified", …).
    label: String,
}

/// The repo's conflict state for the resolver (CPE-496): which reconcile is in progress (`merge` /
/// `rebase` / `none`) and the list of unmerged files with their kind.
#[cfg(feature = "sidecar-platform")]
#[derive(serde::Serialize)]
struct ConflictState {
    /// "merge" | "rebase" | "none".
    operation: String,
    files: Vec<ConflictFile>,
}

/// Which reconcile git is mid-way through, by the marker files/dirs it leaves in `.git`.
#[cfg(feature = "sidecar-platform")]
fn merge_operation(path: &str) -> &'static str {
    let git = std::path::Path::new(path).join(".git");
    if git.join("rebase-merge").exists() || git.join("rebase-apply").exists() {
        "rebase"
    } else if git.join("MERGE_HEAD").exists() {
        "merge"
    } else {
        "none"
    }
}

/// Report the current conflict state (CPE-496) — read-only. Lists unmerged files from
/// `git status --porcelain=v2` and detects any in-progress merge/rebase.
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn forge_conflict_state(path: String) -> ConflictState {
    let out = std::process::Command::new("git")
        .args(["-C", &path, "status", "--porcelain=v2"])
        .output();
    let files = match out {
        Ok(o) if o.status.success() => repos::parse_conflicts(&String::from_utf8_lossy(&o.stdout))
            .into_iter()
            .map(|c| ConflictFile {
                path: c.path,
                code: c.kind.code().to_string(),
                label: c.kind.label().to_string(),
            })
            .collect(),
        _ => Vec::new(),
    };
    ConflictState { operation: merge_operation(&path).to_string(), files }
}

/// The three stage versions of a conflicted file (CPE-496): `base` (stage 1, the common ancestor),
/// `ours` (stage 2), `theirs` (stage 3), plus `merged` — the current working-tree content **with**
/// conflict markers. A stage absent for this conflict kind (e.g. add/add has no base) is `None`. Each
/// is capped so a huge/binary file can't wedge the UI.
#[cfg(feature = "sidecar-platform")]
#[derive(serde::Serialize)]
struct ConflictVersions {
    base: Option<String>,
    ours: Option<String>,
    theirs: Option<String>,
    merged: Option<String>,
    /// True when any side was omitted for being binary or over the size cap.
    truncated: bool,
}

/// Max bytes we surface per version — big enough for real source files, small enough to stay snappy.
#[cfg(feature = "sidecar-platform")]
const CONFLICT_MAX_BYTES: usize = 512 * 1024;

/// Read one git stage of a path as UTF-8 text, or `None` if that stage is absent, binary, or too big.
#[cfg(feature = "sidecar-platform")]
fn read_stage(path: &str, stage: u8, file: &str, truncated: &mut bool) -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["-C", path, "show", &format!(":{stage}:{file}")])
        .output()
        .ok()?;
    if !out.status.success() {
        return None; // stage doesn't exist for this conflict kind
    }
    if out.stdout.len() > CONFLICT_MAX_BYTES || out.stdout.contains(&0) {
        *truncated = true;
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn forge_conflict_versions(path: String, file: String) -> ConflictVersions {
    let mut truncated = false;
    let base = read_stage(&path, 1, &file, &mut truncated);
    let ours = read_stage(&path, 2, &file, &mut truncated);
    let theirs = read_stage(&path, 3, &file, &mut truncated);
    // The working-tree copy (with `<<<<<<<`/`=======`/`>>>>>>>` markers) is the merge starting point.
    let merged = std::fs::read(std::path::Path::new(&path).join(&file))
        .ok()
        .and_then(|b| {
            if b.len() > CONFLICT_MAX_BYTES || b.contains(&0) {
                truncated = true;
                None
            } else {
                Some(String::from_utf8_lossy(&b).into_owned())
            }
        });
    ConflictVersions { base, ours, theirs, merged, truncated }
}

/// True if `file` is a safe repo-relative path to stage a resolution into: non-empty, relative, and
/// with no `..` component or drive/UNC prefix — so a resolution can never write outside the repo.
#[cfg(feature = "sidecar-platform")]
fn is_safe_repo_relative(file: &str) -> bool {
    use std::path::{Component, Path};
    !file.is_empty()
        && !Path::new(file).is_absolute()
        && !file.contains(':') // reject Windows drive / stream prefixes
        && Path::new(file)
            .components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

/// Stage a resolved file (CPE-496): write `content` to `<repo>/<file>` and `git add` it. The path is
/// confined to the repo — a `..`/absolute `file` is refused so a resolution can't write outside it.
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn forge_resolve_file(path: String, file: String, content: String) -> Result<(), String> {
    if !is_safe_repo_relative(&file) {
        return Err("Refusing an unsafe file path.".to_string());
    }
    let full = std::path::Path::new(&path).join(&file);
    std::fs::write(&full, content).map_err(|e| format!("Couldn't write the file: {e}"))?;
    let out = std::process::Command::new("git")
        .args(["-C", &path, "add", "--", &file])
        .output()
        .map_err(|e| format!("Couldn't run git: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Continue the in-progress merge/rebase after conflicts are staged (CPE-496). Runs the right
/// continuation with `GIT_EDITOR=true` so it never blocks on an editor. Fails (surfacing git's
/// message) if files remain unmerged.
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn forge_conflict_continue(path: String) -> Result<String, String> {
    let op = merge_operation(&path);
    let args: Vec<&str> = match op {
        "rebase" => vec!["-C", &path, "rebase", "--continue"],
        "merge" => vec!["-C", &path, "commit", "--no-edit"],
        _ => return Err("No merge or rebase is in progress.".to_string()),
    };
    let out = std::process::Command::new("git")
        .args(&args)
        .env("GIT_EDITOR", "true") // never open an interactive editor
        .output()
        .map_err(|e| format!("Couldn't run git: {e}"))?;
    if out.status.success() {
        Ok(format!("{op} completed"))
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Abort the in-progress merge/rebase (CPE-496), restoring the pre-sync state so **no work is lost**.
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn forge_conflict_abort(path: String) -> Result<String, String> {
    let op = merge_operation(&path);
    let args: Vec<&str> = match op {
        "rebase" => vec!["-C", &path, "rebase", "--abort"],
        "merge" => vec!["-C", &path, "merge", "--abort"],
        _ => return Err("No merge or rebase is in progress.".to_string()),
    };
    let out = std::process::Command::new("git")
        .args(&args)
        .output()
        .map_err(|e| format!("Couldn't run git: {e}"))?;
    if out.status.success() {
        Ok(format!("{op} aborted — restored to the pre-sync state"))
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn sidecar_diagnostics(
    id: String,
    state: tauri::State<AiConsoleState>,
) -> Result<SidecarDiagnostics, String> {
    use sidecar_host::observability::Redactor;

    let redactor = Redactor::new();
    let is_ai_console = id == "ai-console";
    let running = is_ai_console && state.conn.lock().map(|g| g.is_some()).unwrap_or(false);

    let last_error = if is_ai_console {
        state
            .last_error
            .lock()
            .ok()
            .and_then(|g| g.clone())
            .map(|e| redactor.redact_log_line(&e))
    } else {
        None
    };

    let logs = if is_ai_console {
        state
            .logs
            .recent()
            .into_iter()
            .map(|r| DiagLogLine {
                level: format!("{:?}", r.level).to_ascii_lowercase(),
                message: redactor.redact_log_line(&r.message),
            })
            .collect()
    } else {
        Vec::new()
    };

    Ok(SidecarDiagnostics { id, running, last_error, logs })
}

/// Apply CLI window-geometry flags (CPE-600) to the main window, over whatever `tauri-plugin-window-state`
/// restored — so precedence is `CLI flag > saved state > default`. Monitors have no work-area API in
/// Tauri, so the full monitor bounds are used and the pure resolver clamps the window fully on-screen.
/// A parse/geometry error exits non-zero (never a mangled window); nothing requested → leave as restored.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn apply_cli_geometry(app: &tauri::AppHandle) {
    use tauri::Manager;
    use tauri_plugin_cli::CliExt;

    let Ok(matches) = app.cli().matches() else { return };
    let args = match geometry::parse_args(&|k| matches.args.get(k).map(|a| a.value.clone())) {
        Ok(a) => a,
        Err(msg) => {
            eprintln!("geometry: {msg}");
            std::process::exit(2);
        }
    };
    let requested = args.x.is_some() || args.y.is_some() || args.width.is_some() || args.height.is_some()
        || args.position.is_some() || args.monitor.is_some() || args.maximized || args.fullscreen;
    if !requested {
        return; // no geometry flags — keep the restored/default window
    }

    let Some(win) = app.get_webview_window("main") else { return };
    let monitors: Vec<geometry::WorkArea> = win
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| {
            let s = m.scale_factor();
            let (p, sz) = (m.position(), m.size());
            geometry::WorkArea {
                x: (p.x as f64 / s).round() as i32,
                y: (p.y as f64 / s).round() as i32,
                width: (sz.width as f64 / s).round() as u32,
                height: (sz.height as f64 / s).round() as u32,
                scale: s,
            }
        })
        .collect();

    let scale = win.scale_factor().unwrap_or(1.0);
    let cur_pos = win.outer_position().ok();
    let cur_size = win.inner_size().ok();
    let default = geometry::Rect {
        x: cur_pos.map(|p| (p.x as f64 / scale).round() as i32).unwrap_or(0),
        y: cur_pos.map(|p| (p.y as f64 / scale).round() as i32).unwrap_or(0),
        width: cur_size.map(|s| (s.width as f64 / scale).round() as u32).unwrap_or(1000),
        height: cur_size.map(|s| (s.height as f64 / scale).round() as u32).unwrap_or(700),
    };

    match geometry::resolve(&args, &monitors, default) {
        Ok(r) => {
            for w in &r.warnings {
                eprintln!("geometry: {w}");
            }
            let _ = win.set_size(tauri::LogicalSize::new(r.rect.width as f64, r.rect.height as f64));
            let _ = win.set_position(tauri::LogicalPosition::new(r.rect.x as f64, r.rect.y as f64));
            if r.maximized {
                let _ = win.maximize();
            }
            if r.fullscreen {
                let _ = win.set_fullscreen(true);
            }
        }
        Err(e) => {
            eprintln!("geometry: {e}");
            std::process::exit(2);
        }
    }
}

pub fn run() {
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init());

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        builder = builder
            .plugin(tauri_plugin_cli::init()) // window-geometry launch flags (CPE-599)
            .plugin(tauri_plugin_process::init())
            .plugin(tauri_plugin_updater::Builder::new().build())
            // Remember window size/position/maximized across restarts (CPE-228).
            // The plugin auto-restores each config window on launch and auto-saves
            // on exit, writing its own `.window-state.json`. Builder::default()
            // uses StateFlags::all(), so maximized state is restored too.
            .plugin(tauri_plugin_window_state::Builder::default().build());
    }

    // Keep the screen awake for as long as the app is open (CPE-225). We hold a
    // single keep-awake assertion for the app's whole lifetime: created here on
    // the main thread, owned by the run-loop callback below, and dropped — which
    // releases it — the instant that loop ends, i.e. when the app quits. On a
    // hard crash the OS releases the assertion on process death, so nothing
    // lingers either way. Desktop-only: mobile has no such assertion. A failure
    // to acquire is logged, not fatal — the explorer still works, the screen just
    // isn't held awake.
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let keep_awake = keepawake::Builder::default()
        .display(true)
        .reason("Cross-Platform Explorer is open")
        .app_name("Cross-Platform Explorer")
        .app_reverse_domain("com.cross-platform-explorer.app")
        .create()
        .map_err(|e| eprintln!("keep-awake: could not inhibit screen lock: {e}"))
        .ok();

    // Hold the AI Console sidecar connection in managed state (feature-gated).
    #[cfg(feature = "sidecar-platform")]
    {
        builder = builder.manage(AiConsoleState::default());
        // Agent Watch's filesystem watcher lives here (CPE-398); empty until a folder is watched.
        builder = builder.manage(AgentWatchState::default());
    }

    // Startup setup: apply any CLI window-geometry flags (CPE-600) over the window-state-restored
    // window, and — with the platform on — reap orphaned `ai-console --session-daemon` processes left by
    // a prior run before they can lock the sidecar binary during an update (CPE-483).
    builder = builder.setup(|_app| {
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        apply_cli_geometry(_app.handle());
        #[cfg(feature = "sidecar-platform")]
        reap_orphan_session_daemons_on_startup(_app.handle());
        Ok(())
    });

    let app = builder
        .invoke_handler(tauri::generate_handler![
            list_dir,
            find_project_root,
            board_cards,
            board_epics,
            board_archived,
            board_move,
            board_review,
            board_note,
            workbench_diff,
            home_dir,
            parent_dir,
            list_drives,
            disk_space,
            special_folders,
            create_dir,
            read_file_text,
            write_file_text,
            read_archive_entries,
            read_preview_info,
            read_image_data_url,
            read_settings,
            write_settings,
            rename_entry,
            delete_to_trash,
            delete_permanent,
            can_restore_from_trash,
            restore_from_trash,
            copy_entries,
            move_entries,
            move_exact,
            entry_info,
            dir_size,
            hash_file,
            text_stats,
            search_file_contents,
            find_files_by_name,
            files_identical,
            find_duplicates,
            git_remote_url,
            open_external,
            run_as_admin,
            extract_archive_entry,
            compress_to_zip,
            extract_archive,
            open_terminal,
            create_file,
            #[cfg(feature = "sidecar-platform")]
            sidecar_registry_ids,
            #[cfg(feature = "sidecar-platform")]
            sidecar_consent_state,
            #[cfg(feature = "sidecar-platform")]
            sidecar_set_consent,
            #[cfg(feature = "sidecar-platform")]
            sidecar_revoke_capability,
            #[cfg(feature = "sidecar-platform")]
            sidecar_details,
            #[cfg(feature = "sidecar-platform")]
            sidecar_stop,
            #[cfg(feature = "sidecar-platform")]
            sidecar_close_session,
            #[cfg(feature = "sidecar-platform")]
            sidecar_set_enabled,
            #[cfg(feature = "sidecar-platform")]
            sidecar_start_ai_console,
            #[cfg(feature = "sidecar-platform")]
            sidecar_diagnostics,
            #[cfg(feature = "sidecar-platform")]
            agent_watch_start,
            #[cfg(feature = "sidecar-platform")]
            agent_watch_stop,
            #[cfg(feature = "sidecar-platform")]
            forge_browse,
            #[cfg(feature = "sidecar-platform")]
            forge_clone,
            #[cfg(feature = "sidecar-platform")]
            forge_generic_remote,
            #[cfg(feature = "sidecar-platform")]
            forge_admitted_hosts,
            #[cfg(feature = "sidecar-platform")]
            forge_admit_host,
            #[cfg(feature = "sidecar-platform")]
            forge_forget_host,
            #[cfg(feature = "sidecar-platform")]
            forge_clone_url,
            #[cfg(feature = "sidecar-platform")]
            forge_set_token,
            #[cfg(feature = "sidecar-platform")]
            forge_get_token,
            #[cfg(feature = "sidecar-platform")]
            forge_delete_token,
            #[cfg(feature = "sidecar-platform")]
            forge_repo_status,
            #[cfg(feature = "sidecar-platform")]
            forge_sync,
            #[cfg(feature = "sidecar-platform")]
            forge_conflict_state,
            #[cfg(feature = "sidecar-platform")]
            forge_conflict_versions,
            #[cfg(feature = "sidecar-platform")]
            forge_resolve_file,
            #[cfg(feature = "sidecar-platform")]
            forge_conflict_continue,
            #[cfg(feature = "sidecar-platform")]
            forge_conflict_abort
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(move |_app_handle, _event| {
        // Owning `keep_awake` here keeps the assertion alive for the entire run
        // loop; it is dropped (and the screen lock re-enabled) when the loop
        // ends. The reference just anchors the capture — see the comment above.
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        let _ = &keep_awake;
    });
}

// NOTE: clippy's `items_after_test_module` lint requires the test module to be
// the LAST item in the file. Keep it here, at the bottom.
#[cfg(all(test, feature = "sidecar-platform"))]
mod agent_watch_tests {
    use super::classify_fs_event;
    use notify::event::{CreateKind, ModifyKind, RemoveKind, RenameMode};
    use notify::EventKind;

    #[test]
    fn classify_maps_mutations_and_ignores_reads() {
        // Mutations become the coarse Agent Watch kinds (CPE-398)…
        assert_eq!(classify_fs_event(&EventKind::Create(CreateKind::File)), Some("created"));
        assert_eq!(classify_fs_event(&EventKind::Remove(RemoveKind::File)), Some("removed"));
        assert_eq!(
            classify_fs_event(&EventKind::Modify(ModifyKind::Name(RenameMode::Both))),
            Some("renamed"),
        );
        assert_eq!(
            classify_fs_event(&EventKind::Modify(ModifyKind::Data(notify::event::DataChange::Content))),
            Some("modified"),
        );
        // …and reads / unknowns are dropped (a Windows watcher can't see reads anyway).
        assert_eq!(classify_fs_event(&EventKind::Access(notify::event::AccessKind::Read)), None);
        assert_eq!(classify_fs_event(&EventKind::Other), None);
    }

    // --- Catalog version rollback (CPE-383) --------------------------------------------
    use super::{catalog_url_for_tag, is_safe_release_tag, parse_release_versions};

    #[test]
    fn a_release_tag_must_be_url_safe() {
        assert!(is_safe_release_tag("v0.2.0"));
        assert!(is_safe_release_tag("2026.07.14-rc1"));
        // No traversal, separators, scheme, or spaces — a chosen tag can never escape the path.
        assert!(!is_safe_release_tag("../secret"));
        assert!(!is_safe_release_tag("v1/../.."));
        assert!(!is_safe_release_tag("a b"));
        assert!(!is_safe_release_tag(""));
    }

    #[test]
    fn a_tag_url_targets_the_specific_release_not_latest() {
        // Default host: the `download/<tag>/` path, never `latest`.
        std::env::remove_var("CPE_CATALOG_URL");
        let u = catalog_url_for_tag("v0.1.9");
        assert!(u.ends_with("/releases/download/v0.1.9/"), "{u}");
        assert!(!u.contains("latest"));
    }

    #[test]
    fn parse_release_versions_keeps_only_catalog_bearing_releases_with_safe_tags() {
        let body = br#"[
            {"tag_name":"v0.3.0","published_at":"2026-07-14T00:00:00Z","prerelease":false,
             "assets":[{"name":"catalog-index.json"},{"name":"app.msi"}]},
            {"tag_name":"v0.2.0","published_at":"2026-07-01T00:00:00Z","prerelease":true,
             "assets":[{"name":"app.msi"}]},
            {"tag_name":"../evil","published_at":"","assets":[{"name":"catalog-index.json"}]}
        ]"#;
        let got = parse_release_versions(body);
        // Only v0.3.0 qualifies: v0.2.0 has no catalog asset, and the traversal tag is refused.
        assert_eq!(got.len(), 1);
        assert_eq!(got[0]["tag"], "v0.3.0");
        assert_eq!(got[0]["prerelease"], false);
        // Malformed input never panics — just yields nothing.
        assert!(parse_release_versions(b"not json").is_empty());
    }

    // --- Clone argv/URL construction (CPE-436) -----------------------------------------
    use super::build_git_clone;

    #[test]
    fn a_public_clone_builds_the_https_url_host_side_with_all_hardening() {
        let args = build_git_clone("github", "octocat/hello", "/tmp/hello", None).unwrap();
        let j = args.join(" ");
        // The hardened flags from the reused repos builder are present (threat-model §C).
        assert!(j.contains("-c core.hooksPath="));
        assert!(j.contains("-c protocol.ext.allow=never"));
        assert!(j.contains("-c protocol.file.allow=never"));
        assert!(j.contains("-c core.fsmonitor=false"));
        assert!(j.contains("--recurse-submodules=no"));
        // URL + target come after `--` so neither parses as an option; host is built host-side.
        let dd = args.iter().position(|a| a == "--").unwrap();
        assert_eq!(
            &args[dd + 1..],
            &["https://github.com/octocat/hello.git".to_string(), "/tmp/hello".to_string()]
        );
    }

    #[test]
    fn a_private_clone_embeds_the_token_as_userinfo() {
        let args =
            build_git_clone("github", "me/private", "/tmp/p", Some("ghp_SECRET123")).unwrap();
        let dd = args.iter().position(|a| a == "--").unwrap();
        assert_eq!(&args[dd + 1], "https://x-access-token:ghp_SECRET123@github.com/me/private.git");
    }

    #[test]
    fn other_hosted_providers_map_to_their_fixed_clone_host() {
        let g = build_git_clone("gitlab", "grp/proj", "/tmp/g", None).unwrap();
        assert!(g.iter().any(|a| a == "https://gitlab.com/grp/proj.git"));
        let b = build_git_clone("bitbucket", "team/repo", "/tmp/b", Some("tok123")).unwrap();
        assert!(b.iter().any(|a| a == "https://x-token-auth:tok123@bitbucket.org/team/repo.git"));
        let c = build_git_clone("codeberg", "o/r", "/tmp/c", None).unwrap();
        assert!(c.iter().any(|a| a == "https://codeberg.org/o/r.git"));
    }

    #[test]
    fn unknown_provider_bad_repo_bad_target_and_bad_token_are_refused_safely() {
        // An unknown / self-hosted provider has no fixed clone host.
        assert!(build_git_clone("myspace", "a/b", "/tmp/x", None).is_err());
        assert!(build_git_clone("github-enterprise", "a/b", "/tmp/x", None).is_err());
        // A repo that isn't `owner/name` must not be interpolated into a URL.
        assert!(build_git_clone("github", "notaslug", "/tmp/x", None).is_err());
        assert!(build_git_clone("github", "a/../evil", "/tmp/x", None).is_err());
        // A relative target is refused by the hardened builder.
        assert!(build_git_clone("github", "a/b", "relative", None).is_err());
        // A token with url-unsafe chars is refused — and its value is never echoed in the error.
        let e = build_git_clone("github", "a/b", "/tmp/x", Some("bad tok@evil")).unwrap_err();
        assert!(!e.contains("bad tok@evil"));
    }

    // --- Generic Git provider + consent-gated admission (CPE-498) -----------------------
    use super::build_generic_clone;

    #[test]
    fn generic_clone_refuses_a_non_admitted_host() {
        // Even a perfectly valid URL is refused until its host is admitted (no silent admission).
        let e = build_generic_clone("https://git.acme.io/o/r.git", "/tmp/r", None, false).unwrap_err();
        assert!(e.contains("git.acme.io"));
        assert!(e.contains("granted access"));
    }

    #[test]
    fn generic_clone_of_an_admitted_host_builds_hardened_argv() {
        let args =
            build_generic_clone("https://git.acme.io/o/r.git", "/tmp/r", None, true).unwrap();
        let j = args.join(" ");
        assert!(j.contains("-c protocol.ext.allow=never"));
        assert!(j.contains("-c protocol.file.allow=never"));
        let dd = args.iter().position(|a| a == "--").unwrap();
        assert_eq!(&args[dd + 1], "https://git.acme.io/o/r.git");
    }

    #[test]
    fn generic_clone_injects_an_https_token_as_userinfo() {
        let args =
            build_generic_clone("https://git.acme.io/o/r.git", "/tmp/r", Some("tok_123"), true)
                .unwrap();
        let dd = args.iter().position(|a| a == "--").unwrap();
        assert_eq!(&args[dd + 1], "https://tok_123@git.acme.io/o/r.git");
    }

    #[test]
    fn generic_clone_does_not_inject_a_token_for_ssh() {
        // ssh authenticates via the agent — the token is ignored, not embedded in the URL.
        let args =
            build_generic_clone("ssh://git@git.acme.io/o/r.git", "/tmp/r", Some("tok_123"), true)
                .unwrap();
        assert!(args.iter().all(|a| !a.contains("tok_123")));
    }

    #[test]
    fn generic_clone_rejects_bad_urls_and_unsafe_tokens() {
        assert!(build_generic_clone("git://git.acme.io/o/r", "/tmp/r", None, true).is_err());
        assert!(build_generic_clone("ext::sh -c evil", "/tmp/r", None, true).is_err());
        // An unsafe token is refused and never echoed.
        let e = build_generic_clone("https://git.acme.io/o/r.git", "/tmp/r", Some("bad tok@x"), true)
            .unwrap_err();
        assert!(!e.contains("bad tok@x"));
    }

    // --- Conflict-resolution path safety (CPE-496) --------------------------------------
    use super::is_safe_repo_relative;

    #[test]
    fn a_resolution_writes_only_inside_the_repo() {
        // Ordinary repo-relative paths are fine.
        assert!(is_safe_repo_relative("src/app.rs"));
        assert!(is_safe_repo_relative("a/b/c.txt"));
        assert!(is_safe_repo_relative("./file.rs"));
        // Anything that could escape the repo is refused.
        assert!(!is_safe_repo_relative(""));
        assert!(!is_safe_repo_relative("../outside.rs"));
        assert!(!is_safe_repo_relative("a/../../etc/passwd"));
        assert!(!is_safe_repo_relative("/etc/passwd"));
        assert!(!is_safe_repo_relative("C:\\Windows\\system32"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parent_dir_returns_the_parent() {
        assert_eq!(
            parent_dir("/home/user/docs".to_string()),
            Some("/home/user".to_string())
        );
    }

    #[test]
    fn parent_dir_at_root_returns_none() {
        assert_eq!(parent_dir("/".to_string()), None);
    }

    #[test]
    fn list_dir_errors_on_a_missing_path() {
        assert!(list_dir("/definitely/not/a/real/path/xyz".to_string()).is_err());
    }

    #[test]
    fn list_dir_lists_a_real_directory() {
        let dir = std::env::temp_dir();
        assert!(list_dir(dir.to_string_lossy().to_string()).is_ok());
    }

    #[test]
    fn home_dir_resolves() {
        assert!(home_dir().is_ok());
    }

    #[test]
    fn extension_is_lowercased_and_dotless() {
        assert_eq!(extension_of(Path::new("/a/b/Photo.PNG")), "png");
        assert_eq!(extension_of(Path::new("/a/b/archive.tar.gz")), "gz");
    }

    #[test]
    fn extension_is_empty_when_absent() {
        assert_eq!(extension_of(Path::new("/a/b/README")), "");
    }

    #[test]
    fn epoch_ms_of_unix_epoch_is_zero() {
        assert_eq!(to_epoch_ms(UNIX_EPOCH), Some(0));
    }

    #[test]
    fn epoch_ms_is_monotonic_for_later_times() {
        use std::time::Duration;
        let later = UNIX_EPOCH + Duration::from_millis(1_500);
        assert_eq!(to_epoch_ms(later), Some(1_500));
    }

    #[test]
    fn list_drives_returns_at_least_one_root() {
        assert!(!list_drives().is_empty(), "there is always at least one root");
    }

    #[test]
    fn disk_space_reports_sensible_free_and_total() {
        // The temp dir always exists on any runner; free must never exceed total (CPE-403).
        let d = disk_space(std::env::temp_dir().to_string_lossy().into_owned()).unwrap();
        assert!(d.total > 0, "a real volume has non-zero capacity");
        assert!(d.free <= d.total, "free ({}) cannot exceed total ({})", d.free, d.total);
    }

    #[test]
    fn special_folders_all_exist_and_are_labelled() {
        for place in special_folders() {
            assert!(Path::new(&place.path).is_dir(), "{} should exist", place.path);
            assert!(!place.kind.is_empty());
            assert!(!place.name.is_empty());
        }
    }

    #[test]
    fn known_folder_falls_back_to_the_profile_path() {
        // Use a registry value name that cannot exist, so the registry lookup
        // misses and the profile-relative fallback is exercised on every OS.
        let tmp = std::env::temp_dir();
        let sub = tmp.join("cpe_known_folder_test");
        std::fs::create_dir_all(&sub).expect("create temp subdir");

        let found = resolve_known_folder(&tmp, "cpe_known_folder_test", "CpeNoSuchRegistryValue");
        assert_eq!(found, Some(sub.clone()));

        let _ = std::fs::remove_dir(&sub);
    }

    #[test]
    fn known_folder_returns_none_when_it_exists_nowhere() {
        let tmp = std::env::temp_dir();
        assert_eq!(
            resolve_known_folder(
                &tmp,
                "cpe_definitely_missing_folder_xyz",
                "CpeNoSuchRegistryValue"
            ),
            None
        );
    }

    #[cfg(windows)]
    #[test]
    fn registry_lookup_misses_cleanly_for_an_unknown_value() {
        assert_eq!(known_folder_from_registry("CpeNoSuchRegistryValue"), None);
    }

    #[cfg(windows)]
    #[test]
    fn registry_resolves_the_desktop_known_folder() {
        // Desktop is always present in Shell Folders on a real Windows session.
        let desktop = known_folder_from_registry("Desktop");
        assert!(desktop.is_some(), "Desktop should resolve from the registry");
        assert!(desktop.unwrap().is_dir());
    }

    // ---- file operations (CPE-030) ----

    /// Unique scratch dir per test, so tests don't collide when run in parallel.
    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cpe_test_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    #[test]
    fn read_file_text_returns_contents_within_the_cap() {
        let d = scratch("read_ok");
        let f = d.join("note.txt");
        fs::write(&f, b"hello world").unwrap();
        let r = read_file_text(f.to_string_lossy().to_string(), 1024);
        assert_eq!(r.unwrap(), "hello world");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn read_file_text_errors_when_over_the_cap() {
        let d = scratch("read_big");
        let f = d.join("big.txt");
        fs::write(&f, vec![b'x'; 200]).unwrap();
        let r = read_file_text(f.to_string_lossy().to_string(), 100);
        assert!(r.is_err(), "a file over the cap must error, not truncate");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn read_file_text_errors_on_invalid_utf8() {
        let d = scratch("read_bin");
        let f = d.join("blob.bin");
        fs::write(&f, [0xff, 0xfe, 0x00, 0x01]).unwrap();
        let r = read_file_text(f.to_string_lossy().to_string(), 1024);
        assert!(r.is_err(), "non-UTF-8 content must error");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn write_file_text_replaces_contents() {
        let d = scratch("write_txt");
        let f = d.join("note.txt");
        fs::write(&f, b"old text").unwrap();
        let n = write_file_text(f.to_string_lossy().to_string(), "brand new".to_string()).unwrap();
        assert_eq!(n, 9);
        assert_eq!(fs::read_to_string(&f).unwrap(), "brand new");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn read_archive_entries_lists_zip_contents_without_extracting() {
        use std::io::Write;
        let d = scratch("zip_list");
        let zip_path = d.join("bundle.zip");
        {
            let f = fs::File::create(&zip_path).unwrap();
            let mut w = zip::ZipWriter::new(f);
            let opts = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            w.start_file("hello.txt", opts).unwrap();
            w.write_all(b"hi there").unwrap();
            w.add_directory("sub/", opts).unwrap();
            w.finish().unwrap();
        }

        let entries =
            read_archive_entries(zip_path.to_string_lossy().to_string()).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"hello.txt"), "should list the file entry");
        let file = entries.iter().find(|e| e.name == "hello.txt").unwrap();
        assert_eq!(file.size, 8, "size is the uncompressed length");
        assert!(!file.is_dir);
        assert!(
            entries.iter().any(|e| e.is_dir),
            "the directory entry should be flagged is_dir"
        );
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn read_archive_entries_errors_on_a_non_zip() {
        let d = scratch("zip_bad");
        let f = d.join("notazip.zip");
        fs::write(&f, b"this is not a zip file").unwrap();
        assert!(read_archive_entries(f.to_string_lossy().to_string()).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn compress_then_extract_round_trips_files_and_folders() {
        let d = scratch("zip_roundtrip");
        // A file and a folder-with-nested-file to prove recursion and layout.
        let top = d.join("top.txt");
        fs::write(&top, b"top-level").unwrap();
        let sub = d.join("dir");
        fs::create_dir_all(sub.join("nested")).unwrap();
        fs::write(sub.join("a.txt"), b"alpha").unwrap();
        fs::write(sub.join("nested").join("b.txt"), b"beta").unwrap();

        // Compress both selections into one zip.
        let zip_path = d.join("out.zip");
        compress_to_zip(
            vec![
                top.to_string_lossy().to_string(),
                sub.to_string_lossy().to_string(),
            ],
            zip_path.to_string_lossy().to_string(),
        )
        .unwrap();

        // The archive lists the expected entries at the right paths.
        let names: Vec<String> = read_archive_entries(zip_path.to_string_lossy().to_string())
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert!(names.iter().any(|n| n == "top.txt"));
        assert!(names.iter().any(|n| n == "dir/a.txt"));
        assert!(names.iter().any(|n| n == "dir/nested/b.txt"));

        // Extract it back out and confirm the bytes survived.
        let out = d.join("unpacked");
        extract_archive(
            zip_path.to_string_lossy().to_string(),
            out.to_string_lossy().to_string(),
        )
        .unwrap();
        assert_eq!(fs::read_to_string(out.join("top.txt")).unwrap(), "top-level");
        assert_eq!(fs::read_to_string(out.join("dir").join("a.txt")).unwrap(), "alpha");
        assert_eq!(
            fs::read_to_string(out.join("dir").join("nested").join("b.txt")).unwrap(),
            "beta"
        );
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn compress_to_zip_rejects_an_empty_selection() {
        let d = scratch("zip_empty");
        let zip_path = d.join("empty.zip");
        assert!(compress_to_zip(vec![], zip_path.to_string_lossy().to_string()).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn extract_archive_unpacks_a_tar_gz() {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        let d = scratch("targz_extract");
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
        extract_archive(
            tgz.to_string_lossy().to_string(),
            out.to_string_lossy().to_string(),
        )
        .unwrap();
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
        let entries = read_archive_entries(tar_path.to_string_lossy().to_string()).unwrap();
        let file = entries.iter().find(|e| e.name == "hello.txt").unwrap();
        assert_eq!(file.size, 8, "size is the uncompressed length");
        assert!(!file.is_dir);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn read_archive_entries_lists_gzip_single_file() {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;
        let d = scratch("gz_single");
        let gz_path = d.join("note.txt.gz");
        {
            let f = fs::File::create(&gz_path).unwrap();
            let mut enc = GzEncoder::new(f, Compression::default());
            enc.write_all(b"hello world").unwrap();
            enc.finish().unwrap();
        }
        let entries = read_archive_entries(gz_path.to_string_lossy().to_string()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "note.txt", "name is the archive name minus .gz");
        assert_eq!(entries[0].size, 11, "ISIZE trailer is the uncompressed length");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn hex_dump_formats_offsets_and_ascii() {
        let d = scratch("hex");
        let f = d.join("blob.bin");
        fs::write(&f, b"AB\x00\xff").unwrap();
        let dump = hex_dump(&f.to_string_lossy(), 64).unwrap();
        assert!(dump.contains("00000000"), "has an offset column");
        assert!(dump.contains("41 42 00 ff"), "has the hex bytes");
        assert!(dump.contains("|AB..|"), "has the ASCII gutter with dots for non-print");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn wasm_info_disassembles_an_empty_module() {
        let d = scratch("wasm");
        let f = d.join("m.wasm");
        // The 8-byte empty WebAssembly module: magic "\0asm" + version 1.
        fs::write(&f, [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]).unwrap();
        let wat = wasm_info(&f.to_string_lossy(), 4096).unwrap();
        assert!(wat.contains("module"), "prints the (module) wat form");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn torrent_info_reads_bencode_metadata() {
        let d = scratch("torrent");
        let f = d.join("x.torrent");
        // d8:announce9:http://t/4:infod6:lengthi100e4:name3:foo12:piece lengthi16384eee
        let bytes = b"d8:announce9:http://t/4:infod6:lengthi100e4:name3:foo12:piece lengthi16384eee";
        fs::write(&f, bytes).unwrap();
        let info = torrent_info(&f.to_string_lossy()).unwrap();
        assert!(info.contains("Name: foo"), "extracts the name");
        assert!(info.contains("http://t/"), "extracts the announce URL");
        assert!(info.contains("single file"), "reports the single-file length");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn pe_info_errors_on_a_non_pe() {
        let d = scratch("pe_bad");
        let f = d.join("notpe.exe");
        fs::write(&f, b"MZ but not really a PE").unwrap();
        assert!(pe_info(&f.to_string_lossy()).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[cfg(windows)]
    #[test]
    fn pe_info_parses_a_real_windows_binary() {
        // The test executable itself is a PE on Windows.
        let exe = std::env::current_exe().unwrap();
        let info = pe_info(&exe.to_string_lossy()).unwrap();
        assert!(info.contains("PE32"), "identifies the PE image");
        assert!(info.contains("Sections:"), "lists sections");
    }

    #[test]
    fn rtf_text_extracts_body_and_drops_control_words() {
        let d = scratch("rtf");
        let f = d.join("doc.rtf");
        let rtf = r"{\rtf1\ansi{\fonttbl{\f0 Arial;}}\f0\fs24 Hello \b world\b0.\par Second line.}";
        fs::write(&f, rtf).unwrap();
        let text = rtf_text(&f.to_string_lossy()).unwrap();
        assert!(text.contains("Hello world."), "body text extracted: {text:?}");
        assert!(text.contains("Second line."), "second paragraph present");
        assert!(!text.contains("fonttbl"), "font table dropped");
        assert!(!text.contains("Arial"), "font table contents dropped");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn docx_text_extracts_paragraph_text() {
        use std::io::Write;
        let d = scratch("docx");
        let f = d.join("doc.docx");
        {
            let file = fs::File::create(&f).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let opts = zip::write::SimpleFileOptions::default();
            zip.start_file("word/document.xml", opts).unwrap();
            let xml = r#"<?xml version="1.0"?><w:document><w:body><w:p><w:r><w:t>Hello</w:t></w:r><w:r><w:t> world</w:t></w:r></w:p><w:p><w:r><w:t>Next &amp; last</w:t></w:r></w:p></w:body></w:document>"#;
            zip.write_all(xml.as_bytes()).unwrap();
            zip.finish().unwrap();
        }
        let text = docx_text(&f.to_string_lossy()).unwrap();
        assert!(text.contains("Hello world"), "runs joined within a paragraph: {text:?}");
        assert!(text.contains("Next & last"), "entities decoded");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn sqlite_info_lists_tables_rows_and_columns() {
        use rusqlite::Connection;
        let d = scratch("sqlite");
        let f = d.join("test.db");
        {
            let conn = Connection::open(&f).unwrap();
            conn.execute_batch(
                "CREATE TABLE people (id INTEGER PRIMARY KEY, name TEXT);
                 INSERT INTO people (name) VALUES ('Ann'), ('Bo');",
            )
            .unwrap();
        }
        let info = sqlite_info(&f.to_string_lossy()).unwrap();
        assert!(info.contains("people (table) — 2 rows"), "table + row count: {info:?}");
        assert!(info.contains("columns: id, name"), "column list present");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn spreadsheet_info_renders_cells_as_a_grid() {
        use rust_xlsxwriter::Workbook;
        let d = scratch("xlsx");
        let f = d.join("book.xlsx");
        {
            let mut wb = Workbook::new();
            let sheet = wb.add_worksheet();
            sheet.write_string(0, 0, "Name").unwrap();
            sheet.write_string(0, 1, "Age").unwrap();
            sheet.write_string(1, 0, "Ann").unwrap();
            sheet.write_number(1, 1, 30.0).unwrap();
            wb.save(&f).unwrap();
        }
        let info = spreadsheet_info(&f.to_string_lossy()).unwrap();
        assert!(info.contains("Workbook — 1 sheet"), "sheet count: {info:?}");
        assert!(info.contains("Name\tAge"), "header row rendered tab-separated");
        assert!(info.contains("Ann\t30"), "data row rendered");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn parquet_info_errors_on_a_non_parquet() {
        let d = scratch("parquet_bad");
        let f = d.join("x.parquet");
        fs::write(&f, b"not a parquet file").unwrap();
        assert!(parquet_info(&f.to_string_lossy()).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn read_image_data_url_transcodes_tiff_to_png() {
        let d = scratch("tiff");
        let f = d.join("a.tiff");
        let mut img = image::RgbaImage::new(2, 2);
        img.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        img.save_with_format(&f, image::ImageFormat::Tiff).unwrap();
        let url = read_image_data_url(f.to_string_lossy().to_string()).unwrap();
        assert!(url.starts_with("data:image/png;base64,"), "returns a PNG data URL");
        assert!(url.len() > 40, "carries encoded bytes");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn read_image_data_url_errors_on_a_corrupt_psd() {
        let d = scratch("psd_bad");
        let f = d.join("a.psd");
        fs::write(&f, b"not a real psd").unwrap();
        assert!(read_image_data_url(f.to_string_lossy().to_string()).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn read_archive_entries_errors_on_a_non_iso() {
        let d = scratch("iso_bad");
        let f = d.join("x.iso");
        fs::write(&f, vec![0u8; 4096]).unwrap();
        assert!(read_archive_entries(f.to_string_lossy().to_string()).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn read_archive_entries_errors_on_a_non_7z() {
        let d = scratch("sevenz_bad");
        let f = d.join("x.7z");
        fs::write(&f, b"not a 7z archive").unwrap();
        assert!(read_archive_entries(f.to_string_lossy().to_string()).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn read_preview_info_dispatches_by_extension() {
        let d = scratch("dispatch");
        let f = d.join("thing.bin");
        fs::write(&f, b"\x01\x02\x03").unwrap();
        // .bin -> hex dump path
        let out = read_preview_info(f.to_string_lossy().to_string()).unwrap();
        assert!(out.contains("01 02 03"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn settings_round_trip_and_default_to_empty_object() {
        let d = scratch("settings");
        // Absent file → "{}" default (never errors on a fresh install).
        assert_eq!(read_settings_from(&d), "{}");
        // Write then read back the exact document.
        let doc = r#"{"cpe.view":"list","cpe.showHidden":true}"#;
        write_settings_to(&d, doc).unwrap();
        assert_eq!(read_settings_from(&d), doc);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn write_settings_creates_the_config_dir() {
        let d = scratch("settings_mkdir").join("nested/config");
        assert!(!d.exists());
        write_settings_to(&d, "{}").unwrap();
        assert!(d.join("settings.json").exists());
        let _ = fs::remove_dir_all(d.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn create_dir_rejects_an_empty_name() {
        let d = scratch("create_empty");
        let r = create_dir(d.to_string_lossy().to_string(), "   ".to_string());
        assert!(r.is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn create_dir_refuses_to_clobber_an_existing_name() {
        let d = scratch("create_dup");
        let p = d.to_string_lossy().to_string();
        assert!(create_dir(p.clone(), "thing".into()).is_ok());
        let second = create_dir(p, "thing".into());
        assert!(second.is_err(), "must not silently overwrite");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn create_file_makes_an_empty_file() {
        let d = scratch("create_file");
        let created =
            create_file(d.to_string_lossy().to_string(), "New Text Document.txt".into()).unwrap();
        assert!(std::path::Path::new(&created).is_file());
        assert_eq!(fs::metadata(&created).unwrap().len(), 0, "file starts empty");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn create_file_refuses_to_clobber_existing_content() {
        let d = scratch("create_file_dup");
        let p = d.to_string_lossy().to_string();
        // Pre-existing file with content must not be truncated by a New file.
        fs::write(d.join("note.txt"), b"important").unwrap();
        assert!(create_file(p, "note.txt".into()).is_err());
        assert_eq!(fs::read_to_string(d.join("note.txt")).unwrap(), "important");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn rename_refuses_to_clobber_an_existing_name() {
        let d = scratch("rename_dup");
        fs::write(d.join("a.txt"), b"a").unwrap();
        fs::write(d.join("b.txt"), b"b").unwrap();

        let r = rename_entry(
            d.join("a.txt").to_string_lossy().to_string(),
            "b.txt".into(),
        );
        assert!(r.is_err(), "renaming onto an existing file must fail");
        // b.txt must be untouched.
        assert_eq!(fs::read(d.join("b.txt")).unwrap(), b"b");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn rename_moves_the_file() {
        let d = scratch("rename_ok");
        fs::write(d.join("a.txt"), b"a").unwrap();
        let r = rename_entry(
            d.join("a.txt").to_string_lossy().to_string(),
            "c.txt".into(),
        );
        assert!(r.is_ok());
        assert!(d.join("c.txt").exists());
        assert!(!d.join("a.txt").exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn unique_target_appends_copy_suffixes_instead_of_overwriting() {
        let d = scratch("unique");
        assert_eq!(unique_target(&d, "x.txt"), d.join("x.txt"));

        fs::write(d.join("x.txt"), b"1").unwrap();
        assert_eq!(unique_target(&d, "x.txt"), d.join("x - Copy.txt"));

        fs::write(d.join("x - Copy.txt"), b"2").unwrap();
        assert_eq!(unique_target(&d, "x.txt"), d.join("x - Copy (2).txt"));

        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn unique_target_handles_extensionless_names() {
        let d = scratch("unique_noext");
        fs::write(d.join("README"), b"1").unwrap();
        assert_eq!(unique_target(&d, "README"), d.join("README - Copy"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn copy_auto_renames_rather_than_overwriting() {
        let d = scratch("copy_same");
        fs::write(d.join("f.txt"), b"original").unwrap();

        let results = copy_entries(
            vec![d.join("f.txt").to_string_lossy().to_string()],
            d.to_string_lossy().to_string(),
        );
        assert!(results[0].ok, "{}", results[0].error);
        // The original must be untouched.
        assert_eq!(fs::read(d.join("f.txt")).unwrap(), b"original");
        assert!(d.join("f - Copy.txt").exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn copying_a_folder_into_itself_is_refused() {
        let d = scratch("copy_self");
        let inner = d.join("inner");
        fs::create_dir_all(inner.join("deep")).unwrap();

        // inner -> inner/deep  is a descendant: must be refused, not recursed.
        let results = copy_entries(
            vec![inner.to_string_lossy().to_string()],
            inner.join("deep").to_string_lossy().to_string(),
        );
        assert!(!results[0].ok, "copying a folder into its descendant must fail");
        assert!(results[0].error.contains("itself"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn copy_dir_all_copies_the_whole_tree() {
        let d = scratch("copy_tree");
        let src = d.join("src");
        fs::create_dir_all(src.join("a/b")).unwrap();
        fs::write(src.join("a/b/leaf.txt"), b"leaf").unwrap();

        let dst = d.join("dst");
        copy_dir_all(&src, &dst).unwrap();
        assert_eq!(fs::read(dst.join("a/b/leaf.txt")).unwrap(), b"leaf");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn move_relocates_and_removes_the_original() {
        let d = scratch("move_ok");
        let from = d.join("from");
        let to = d.join("to");
        fs::create_dir_all(&from).unwrap();
        fs::create_dir_all(&to).unwrap();
        fs::write(from.join("m.txt"), b"m").unwrap();

        let results = move_entries(
            vec![from.join("m.txt").to_string_lossy().to_string()],
            to.to_string_lossy().to_string(),
        );
        assert!(results[0].ok, "{}", results[0].error);
        assert!(to.join("m.txt").exists());
        assert!(!from.join("m.txt").exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn bulk_ops_report_per_item_instead_of_aborting_on_first_failure() {
        let d = scratch("bulk");
        let to = d.join("to");
        fs::create_dir_all(&to).unwrap();
        fs::write(d.join("good.txt"), b"g").unwrap();

        let results = copy_entries(
            vec![
                d.join("good.txt").to_string_lossy().to_string(),
                d.join("missing.txt").to_string_lossy().to_string(),
            ],
            to.to_string_lossy().to_string(),
        );
        assert_eq!(results.len(), 2, "every item must get a result");
        assert!(results[0].ok);
        assert!(!results[1].ok, "the missing file must be reported, not skipped");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn entry_info_reports_metadata() {
        let d = scratch("info");
        fs::write(d.join("i.txt"), b"12345").unwrap();
        let info = entry_info(d.join("i.txt").to_string_lossy().to_string()).unwrap();
        assert_eq!(info.name, "i.txt");
        assert!(!info.is_dir);
        assert_eq!(info.size, 5);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn hash_file_matches_the_known_sha256_vector_and_rejects_folders() {
        let d = scratch("hash");
        // The canonical SHA-256("abc") test vector.
        fs::write(d.join("abc.txt"), b"abc").unwrap();
        let hex = hash_file(d.join("abc.txt").to_string_lossy().to_string()).unwrap();
        assert_eq!(hex, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        // A directory and a missing path are errors, not panics.
        assert!(hash_file(d.to_string_lossy().to_string()).is_err());
        assert!(hash_file(d.join("nope.txt").to_string_lossy().to_string()).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn text_stats_counts_lines_words_chars_bytes() {
        let d = scratch("stats");
        // 2 lines, 3 words, 16 chars (incl. 2 newlines), 16 bytes (all ASCII).
        fs::write(d.join("t.txt"), b"hello world\nfoo\n").unwrap();
        let s = text_stats(d.join("t.txt").to_string_lossy().to_string()).unwrap();
        assert_eq!((s.lines, s.words, s.chars, s.bytes), (2, 3, 16, 16));
        // A final unterminated line still counts (str::lines semantics).
        fs::write(d.join("u.txt"), b"a\nb").unwrap();
        assert_eq!(text_stats(d.join("u.txt").to_string_lossy().to_string()).unwrap().lines, 2);
        // Non-UTF-8 (binary) and a folder are errors, not panics.
        fs::write(d.join("bin"), [0xff, 0xfe, 0x00]).unwrap();
        assert!(text_stats(d.join("bin").to_string_lossy().to_string()).is_err());
        assert!(text_stats(d.to_string_lossy().to_string()).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn search_file_contents_finds_matches_recursively_and_skips_noise() {
        let d = scratch("search");
        fs::write(d.join("a.txt"), b"hello world\nsecond line\nNEEDLE here\n").unwrap();
        fs::create_dir_all(d.join("sub")).unwrap();
        fs::write(d.join("sub").join("b.md"), b"nothing\nfound the needle deep\n").unwrap();
        // A binary file with the text — must be skipped.
        fs::write(d.join("c.bin"), b"needle\x00binary").unwrap();
        // A dot-dir — must be skipped.
        fs::create_dir_all(d.join(".git")).unwrap();
        fs::write(d.join(".git").join("x"), b"needle in git").unwrap();

        let r = search_file_contents(d.to_string_lossy().to_string(), "needle".into(), false).unwrap();
        let paths: Vec<String> = r.matches.iter().map(|m| m.path.replace('\\', "/")).collect();
        // Case-insensitive: matches "NEEDLE" (a.txt) and "needle" (sub/b.md); NOT the binary or .git.
        assert_eq!(r.matches.len(), 2, "got {paths:?}");
        assert!(paths.iter().any(|p| p.ends_with("a.txt")));
        assert!(paths.iter().any(|p| p.ends_with("sub/b.md")));
        assert!(!paths.iter().any(|p| p.contains(".git") || p.ends_with("c.bin")));
        // Line numbers are 1-based.
        let a = r.matches.iter().find(|m| m.path.ends_with("a.txt")).unwrap();
        assert_eq!(a.line_number, 3);
        assert_eq!(a.line, "NEEDLE here");

        // Case-sensitive excludes the uppercase hit.
        let cs = search_file_contents(d.to_string_lossy().to_string(), "needle".into(), true).unwrap();
        assert_eq!(cs.matches.len(), 1);
        assert!(cs.matches[0].path.replace('\\', "/").ends_with("sub/b.md"));

        // Empty query and a non-folder root behave sanely.
        assert_eq!(search_file_contents(d.to_string_lossy().to_string(), "  ".into(), false).unwrap().matches.len(), 0);
        assert!(search_file_contents(d.join("a.txt").to_string_lossy().to_string(), "x".into(), false).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn name_matches_does_substring_and_glob() {
        // Plain query = case-insensitive substring.
        assert!(name_matches("Report.pdf", "report"));
        assert!(name_matches("Report.pdf", "port"));
        assert!(!name_matches("Report.pdf", "xls"));
        // Glob = anchored over the whole name.
        assert!(name_matches("photo.png", "*.png"));
        assert!(!name_matches("photo.pngx", "*.png")); // anchored: trailing chars fail
        assert!(name_matches("a1b2.txt", "a?b?.txt"));
        assert!(!name_matches("ab.txt", "a?b?.txt")); // '?' needs exactly one char
        assert!(name_matches("anything", "*")); // lone star matches all
        assert!(name_matches("IMG_2024.jpg", "img_*.jpg")); // case-insensitive glob
        // Empty query matches nothing (caller gates).
        assert!(!name_matches("x", ""));
    }

    #[test]
    fn find_files_by_name_walks_recursively_and_skips_dot_dirs() {
        let d = scratch("namesearch");
        fs::write(d.join("report.txt"), b"").unwrap();
        fs::create_dir_all(d.join("sub")).unwrap();
        fs::write(d.join("sub").join("report-2.txt"), b"").unwrap();
        fs::create_dir_all(d.join("reports")).unwrap(); // a matching FOLDER
        // A dot-dir with a matching file — both dir and file must be skipped.
        fs::create_dir_all(d.join(".git")).unwrap();
        fs::write(d.join(".git").join("report.log"), b"").unwrap();

        let r = find_files_by_name(d.to_string_lossy().to_string(), "report".into()).unwrap();
        let names: Vec<&str> = r.matches.iter().map(|m| m.name.as_str()).collect();
        // report.txt, sub/report-2.txt, and the "reports" folder — never anything under .git.
        assert_eq!(r.matches.len(), 3, "got {names:?}");
        assert!(names.contains(&"report.txt"));
        assert!(names.contains(&"report-2.txt"));
        assert!(r.matches.iter().any(|m| m.name == "reports" && m.is_dir));
        assert!(!r.matches.iter().any(|m| m.path.contains(".git")));

        // Glob query.
        let g = find_files_by_name(d.to_string_lossy().to_string(), "*.txt".into()).unwrap();
        assert_eq!(g.matches.len(), 2);
        assert!(g.matches.iter().all(|m| m.name.ends_with(".txt")));

        // Empty query and a non-folder root behave sanely.
        assert_eq!(find_files_by_name(d.to_string_lossy().to_string(), "  ".into()).unwrap().matches.len(), 0);
        assert!(find_files_by_name(d.join("report.txt").to_string_lossy().to_string(), "x".into()).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn recursive_walks_skip_symlinked_dirs_and_do_not_cycle() {
        let d = scratch("symlinkcycle");
        fs::create_dir_all(d.join("real")).unwrap();
        fs::write(d.join("real").join("target.txt"), b"needle").unwrap();
        // Create a symlink 'loop' -> the scratch root itself (a cycle). Skip the test where symlink
        // creation is unprivileged (Windows without Developer Mode / admin) — the fix still compiles
        // and the non-symlink paths are covered elsewhere.
        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_dir(&d, d.join("loop")).is_ok();
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(&d, d.join("loop")).is_ok();
        if !made {
            let _ = fs::remove_dir_all(&d);
            return;
        }

        // Without the symlink skip, the 'loop' link re-enters the root forever until the 50k-dir cap
        // (truncated=true, dirs_scanned huge). With it, the walk terminates immediately.
        let r = find_files_by_name(d.to_string_lossy().to_string(), "target".into()).unwrap();
        assert!(!r.truncated, "walk hit its cap — the symlink cycle was not skipped");
        assert!(r.dirs_scanned < 100, "walked too many dirs ({}) — cycle not skipped", r.dirs_scanned);
        assert!(r.matches.iter().any(|m| m.name == "target.txt"));

        let c = search_file_contents(d.to_string_lossy().to_string(), "needle".into(), false).unwrap();
        assert!(!c.truncated, "content search hit its cap — symlink cycle not skipped");
        assert!(c.matches.iter().any(|m| m.path.replace('\\', "/").ends_with("real/target.txt")));

        // dir_size must NOT stack-overflow on the cycle (it recurses). The invariant is simply that it
        // *terminates* with a small, finite total — without the CPE-611 fix it recurses until the thread
        // stack overflows and aborts the whole test binary. Don't assert an exact byte count: it isn't
        // portable (Linux counts the symlink entry's target-path length, ~31 bytes; Windows reports 0),
        // so bound it instead: at least the real file (6 bytes), and nowhere near a runaway.
        let sz = dir_size(d.to_string_lossy().to_string()).unwrap();
        assert!((6..100_000).contains(&sz), "dir_size should terminate small on a cycle, got {sz}");

        // find_duplicates likewise terminates (one file, no dupes, not truncated).
        let dup = find_duplicates(d.to_string_lossy().to_string()).unwrap();
        assert!(!dup.truncated, "find_duplicates hit its cap — symlink cycle not skipped");

        // remove_dir_all removes the symlink itself without following it.
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn files_identical_compares_content_and_short_circuits_on_size() {
        let d = scratch("cmp");
        fs::write(d.join("a"), b"same content here").unwrap();
        fs::write(d.join("b"), b"same content here").unwrap();
        fs::write(d.join("c"), b"same content HERE").unwrap(); // same length, different bytes
        fs::write(d.join("e"), b"different length entirely").unwrap();
        let p = |n: &str| d.join(n).to_string_lossy().to_string();
        assert_eq!(files_identical(p("a"), p("b")), Ok(true));
        assert_eq!(files_identical(p("a"), p("c")), Ok(false)); // same size, differing byte
        assert_eq!(files_identical(p("a"), p("e")), Ok(false)); // different size
        // Empty files are identical; a folder or missing path errors.
        fs::write(d.join("z1"), b"").unwrap();
        fs::write(d.join("z2"), b"").unwrap();
        assert_eq!(files_identical(p("z1"), p("z2")), Ok(true));
        assert!(files_identical(p("a"), d.to_string_lossy().to_string()).is_err());
        assert!(files_identical(p("a"), p("nope")).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn find_duplicates_groups_identical_files_and_ignores_unique_sizes() {
        let d = scratch("dups");
        fs::create_dir_all(d.join("sub")).unwrap();
        // Three identical files across subfolders (a 3-way group).
        for n in ["one.txt", "sub/two.txt", "sub/three.txt"] {
            fs::write(d.join(n), b"duplicate payload").unwrap();
        }
        // A same-SIZE-but-different file — must NOT group with the above.
        fs::write(d.join("decoy.txt"), b"DUPLICATE payloaD").unwrap(); // 17 bytes, like the others
        // A unique file — never hashed, never grouped.
        fs::write(d.join("unique.txt"), b"i am one of a kind").unwrap();
        // Empty files are ignored.
        fs::write(d.join("empty1"), b"").unwrap();
        fs::write(d.join("empty2"), b"").unwrap();

        let r = find_duplicates(d.to_string_lossy().to_string()).unwrap();
        assert_eq!(r.groups.len(), 1, "exactly one duplicate group");
        let g = &r.groups[0];
        assert_eq!(g.paths.len(), 3, "the 3-way group");
        assert_eq!(g.size, 17);
        let names: Vec<String> = g.paths.iter().map(|p| p.replace('\\', "/")).collect();
        assert!(names.iter().any(|p| p.ends_with("one.txt")));
        assert!(names.iter().any(|p| p.ends_with("sub/two.txt")));
        assert!(!names.iter().any(|p| p.ends_with("decoy.txt")));

        // No-duplicate folder → empty; a non-folder root → Err.
        let d2 = scratch("nodups");
        fs::write(d2.join("only.txt"), b"solo").unwrap();
        assert!(find_duplicates(d2.to_string_lossy().to_string()).unwrap().groups.is_empty());
        assert!(find_duplicates(d.join("one.txt").to_string_lossy().to_string()).is_err());
        let _ = fs::remove_dir_all(&d);
        let _ = fs::remove_dir_all(&d2);
    }

    #[test]
    fn dir_size_sums_the_tree() {
        let d = scratch("dirsize");
        fs::create_dir_all(d.join("sub")).unwrap();
        fs::write(d.join("a.bin"), vec![0u8; 100]).unwrap();
        fs::write(d.join("sub/b.bin"), vec![0u8; 50]).unwrap();

        let total = dir_size(d.to_string_lossy().to_string()).unwrap();
        assert_eq!(total, 150);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn move_exact_restores_to_the_original_name() {
        let d = scratch("move_exact");
        fs::write(d.join("b.txt"), b"x").unwrap();

        let results = move_exact(vec![(
            d.join("b.txt").to_string_lossy().to_string(),
            d.join("a.txt").to_string_lossy().to_string(),
        )]);
        assert!(results[0].ok, "{}", results[0].error);
        assert!(d.join("a.txt").exists());
        assert!(!d.join("b.txt").exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn move_exact_refuses_to_overwrite() {
        let d = scratch("move_exact_clobber");
        fs::write(d.join("a.txt"), b"keep").unwrap();
        fs::write(d.join("b.txt"), b"other").unwrap();

        let results = move_exact(vec![(
            d.join("b.txt").to_string_lossy().to_string(),
            d.join("a.txt").to_string_lossy().to_string(),
        )]);
        assert!(!results[0].ok, "undo must not clobber an existing file");
        assert_eq!(fs::read(d.join("a.txt")).unwrap(), b"keep");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn dotfiles_are_hidden_on_posix_convention() {
        let d = scratch("hidden");
        let p = d.join(".secret");
        fs::write(&p, b"x").unwrap();
        let meta = fs::metadata(&p).unwrap();
        assert!(is_hidden(&p, &meta));

        let visible = d.join("plain.txt");
        fs::write(&visible, b"x").unwrap();
        let vmeta = fs::metadata(&visible).unwrap();
        assert!(!is_hidden(&visible, &vmeta));
        let _ = fs::remove_dir_all(&d);
    }
}
