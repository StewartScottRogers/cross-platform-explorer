use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Build a `std::process::Command` that never flashes a console window on Windows (CPE-840).
///
/// Every helper process we run for its **output or side-effect** — `git status`/`diff`/`clone`, the
/// external opener (`cmd /C start`), `run_command`, the elevation `powershell` — must be spawned with
/// `CREATE_NO_WINDOW`, or Windows blinks a transient black console each time. The most visible symptom
/// was a console flashing on **every folder navigation** (the per-folder `git status` in
/// `forge_repo_status`). Use this instead of `Command::new` for those. Do **not** use it for commands
/// that are *meant* to open a window (`open_terminal`).
fn quiet_command(program: &str) -> std::process::Command {
    // `mut` is only used on Windows (below); on other targets the cfg block compiles out.
    #[allow(unused_mut)]
    let mut cmd = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd
}

/// Live provider API-key verification + catalog egress for the AI Console sidecar (CPE-347/369/376).
/// Only compiled with the platform: without it nothing calls these, so the module would be dead
/// code under `-D warnings` (its pure logic is still unit-tested under the feature).
/// Tauri adapter (`TauriCtx`) for the Server runtime seam. The `ServerCtx` trait itself and the
/// Tauri-free domain logic (location model, filesystem-provider abstraction) now live in the pure
/// `cpe-server` crate (CPE-815); this app is the thin adapter that supplies `TauriCtx` and dispatches
/// to it. `ServerCtx` is imported from the crate so `TauriCtx`'s methods resolve at call sites.
mod server_ctx;
use cpe_server::ctx::ServerCtx;
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

/// Agent Watch shadow-content store (CPE-743, epic CPE-727): a bounded, text-only baseline of files
/// under the watched tree, used to pair each write with its "before" content for Edit Diff Peek.
#[cfg(feature = "sidecar-platform")]
mod agent_shadow;

/// The session audit journal (CPE-800), pure window-geometry resolver (CPE-598), and Agent Board
/// backend (CPE-520) now live in the `cpe-server` crate (CPE-815); re-export their module paths so
/// existing `audit_journal::` / `geometry::` / `ticket_board::` references resolve unchanged.
use cpe_server::{audit_journal, geometry, ticket_board};
/// Shared FS utils (epoch-ms + streaming SHA-256) also live in `cpe-server` (CPE-815); re-export them
/// so the many `to_epoch_ms(…)` / `sha256_file(…)` call sites resolve unchanged.
use cpe_server::fsutil::to_epoch_ms;

/// Read every ticket under `<root>/Tickets/{Backlog,Doing,Blocked,Deferred,Done}/CPE-*.md` into board
/// cards (CPE-520). Read-only; a malformed file is skipped, never fails the listing.
#[tauri::command]
async fn board_cards(root: String) -> Vec<ticket_board::Card> {
    tauri::async_runtime::spawn_blocking(move || board_cards_impl(root))
        .await.unwrap()
}

fn board_cards_impl(root: String) -> Vec<ticket_board::Card> {
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
async fn find_project_root(start: String) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || find_project_root_impl(start))
        .await.unwrap()
}

fn find_project_root_impl(start: String) -> Option<String> {
    ticket_board::nearest_project_root(std::path::Path::new(&start))
        .map(|p| p.to_string_lossy().into_owned())
}

/// Move ticket `id` to `to_column` (CPE-520): rewrite its `status:` frontmatter to match, then move the
/// file into that folder. The only writer. Refuses an unknown id/column and never clobbers an existing
/// file. A move to the current column is a no-op.
#[tauri::command]
async fn board_move(root: String, id: String, to_column: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || board_move_impl(root, id, to_column))
        .await.map_err(|e| e.to_string())?
}

fn board_move_impl(root: String, id: String, to_column: String) -> Result<(), String> {
    let folder =
        ticket_board::folder_for_column(&to_column).ok_or_else(|| format!("unknown column '{to_column}'"))?;
    let status = ticket_board::status_for_column(&to_column).unwrap_or(folder);
    let tickets = std::path::Path::new(&root).join("Tickets");

    // Locate the ticket file: `<id>_*.md` in one of the columns. Recursive, so an archived Done ticket
    // (in a dated `Done/YYYY/…` subfolder) can still be reopened/moved (CPE-864).
    let prefix = format!("{id}_");
    let mut found: Option<(std::path::PathBuf, &'static str)> = None;
    for col in ticket_board::COLUMNS {
        if let Some(p) = find_ticket_file_recursive(&tickets.join(col), &prefix) {
            found = Some((p, col));
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

/// Find a ticket file `<prefix>*.md` anywhere under `dir` (recursively), so an archived Done ticket in a
/// dated subfolder is still locatable for a move/reopen (CPE-864).
fn find_ticket_file_recursive(dir: &std::path::Path, prefix: &str) -> Option<std::path::PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            if let Some(hit) = find_ticket_file_recursive(&p, prefix) {
                return Some(hit);
            }
        } else {
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if name.starts_with(prefix) && name.ends_with(".md") {
                return Some(p);
            }
        }
    }
    None
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
async fn board_archived(root: String) -> Vec<ticket_board::Card> {
    tauri::async_runtime::spawn_blocking(move || board_archived_impl(root))
        .await.unwrap()
}

fn board_archived_impl(root: String) -> Vec<ticket_board::Card> {
    let done = std::path::Path::new(&root).join("Tickets").join("Done");
    let mut out = Vec::new();
    collect_archived(&done, true, &mut out);
    out
}

/// List the repo's epics for the board's epic-organized view (CPE-530): active/proposed epics from
/// `Tickets/Epics/` + closed epics from `Tickets/Done/` (top level), each `epic`-tagged. Read-only.
#[tauri::command]
async fn board_epics(root: String) -> Vec<ticket_board::Epic> {
    tauri::async_runtime::spawn_blocking(move || board_epics_impl(root))
        .await.unwrap()
}

fn board_epics_impl(root: String) -> Vec<ticket_board::Epic> {
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
async fn board_review(root: String, id: String, on: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || board_review_impl(root, id, on))
        .await.map_err(|e| e.to_string())?
}

fn board_review_impl(root: String, id: String, on: bool) -> Result<(), String> {
    let path = find_ticket_file(&root, &id).ok_or_else(|| format!("ticket {id} not found"))?;
    let md = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    std::fs::write(&path, ticket_board::set_review(&md, on)).map_err(|e| e.to_string())
}

/// Append a finding note to ticket `id` (CPE-523) — the affordance a dispatched agent (or the UI) uses
/// to record progress on a card.
#[tauri::command]
async fn board_note(root: String, id: String, note: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || board_note_impl(root, id, note))
        .await.map_err(|e| e.to_string())?
}

fn board_note_impl(root: String, id: String, note: String) -> Result<(), String> {
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
async fn workbench_diff(root: String, path: Option<String>) -> Result<WorkbenchDiff, String> {
    tauri::async_runtime::spawn_blocking(move || workbench_diff_impl(root, path))
        .await.map_err(|e| e.to_string())?
}

fn workbench_diff_impl(root: String, path: Option<String>) -> Result<WorkbenchDiff, String> {
    if root.trim().is_empty() {
        return Err("no-folder".to_string()); // opened on Home / no folder
    }
    // Is this a git work tree? Distinguishes not-a-repo (friendly) from git-missing (error).
    let inside = quiet_command("git")
        .args(["-C", &root, "rev-parse", "--is-inside-work-tree"])
        .output()
        .map_err(|e| format!("git-missing: {e}"))?;
    if !inside.status.success() {
        return Ok(WorkbenchDiff { is_repo: false, branch: None, diff: String::new() });
    }
    let branch = quiet_command("git")
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
    let out = quiet_command("git")
        .args(&args)
        .output()
        .map_err(|e| format!("Couldn't run git: {e}"))?;
    if out.status.success() {
        Ok(WorkbenchDiff { is_repo: true, branch, diff: String::from_utf8_lossy(&out.stdout).into_owned() })
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

// The shared filesystem model types (DirEntry / OpResult / EntryInfo / Place) and the `extension_of` /
// `is_hidden` helpers now live in `cpe_server::model` (CPE-815); re-export them so the many
// construction/usage sites resolve unchanged.
use cpe_server::model::{extension_of, is_hidden, DirEntry, EntryInfo, OpResult, Place};

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
// Async so a listing on a slow drive runs off the main thread (CPE-760).
/// List a directory's entries. Model + the shared walker live in `cpe_server::listing` (CPE-815); this
/// is a thin `spawn_blocking` dispatcher.
#[tauri::command]
async fn list_dir(path: String) -> Result<Vec<DirEntry>, String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::listing::list_dir(&path))
        .await
        .map_err(|e| e.to_string())?
}

/// Registry of in-flight `list_dir_stream` walks' cancel flags, keyed by the frontend-supplied stream id,
/// so `cancel_dir_stream` can stop a walk the user has navigated away from (CPE-665). Mirrors the
/// transfer cancel registry.
static DIR_STREAM_CANCELS: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<u64, std::sync::Arc<std::sync::atomic::AtomicBool>>>,
> = std::sync::OnceLock::new();

fn dir_stream_registry(
) -> &'static std::sync::Mutex<std::collections::HashMap<u64, std::sync::Arc<std::sync::atomic::AtomicBool>>>
{
    DIR_STREAM_CANCELS.get_or_init(Default::default)
}

/// Streaming variant of `list_dir` (CPE-663, epic CPE-662): pushes `DirEntry` batches over an IPC channel
/// as the directory is read, so the frontend paints the first rows immediately instead of waiting for the
/// whole listing. `stream_id` (frontend-supplied, monotonic) registers a cancel flag polled each batch, so
/// a superseded walk stops promptly instead of reading a huge folder to completion (CPE-665). Returns the
/// total entry count once the walk completes (or is cancelled).
// Async so a listing on a slow/network drive streams from a blocking thread and never freezes the main
// thread (CPE-760). The `Channel` batches still arrive live; only the walk moves off the UI thread.
#[tauri::command]
async fn list_dir_stream(
    path: String,
    stream_id: u64,
    on_entry: tauri::ipc::Channel<Vec<DirEntry>>,
) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || list_dir_stream_impl(path, stream_id, on_entry))
        .await
        .map_err(|e| e.to_string())?
}

fn list_dir_stream_impl(
    path: String,
    stream_id: u64,
    on_entry: tauri::ipc::Channel<Vec<DirEntry>>,
) -> Result<usize, String> {
    use std::sync::atomic::Ordering;
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    dir_stream_registry().lock().unwrap().insert(stream_id, cancel.clone());
    let result = cpe_server::listing::stream_dir_entries(&path, cpe_server::listing::LIST_DIR_BATCH, |batch| {
        let _ = on_entry.send(batch);
        if cancel.load(Ordering::Relaxed) {
            std::ops::ControlFlow::Break(())
        } else {
            std::ops::ControlFlow::Continue(())
        }
    });
    dir_stream_registry().lock().unwrap().remove(&stream_id);
    result
}

/// Signal an in-flight `list_dir_stream` to stop at the next batch boundary (CPE-665). A no-op if the
/// stream already finished (its id is gone from the registry).
#[tauri::command]
fn cancel_dir_stream(stream_id: u64) {
    use std::sync::atomic::Ordering;
    if let Some(flag) = dir_stream_registry().lock().unwrap().get(&stream_id) {
        flag.store(true, Ordering::Relaxed);
    }
}

/// Metadata for a single path, or `None` if it can't be read (gone/unreadable). Used to build a listing
/// from an arbitrary set of paths (smart folders, CPE-667) rather than one directory's children.
fn entry_for_path(path: &str) -> Option<DirEntry> {
    let p = Path::new(path);
    let meta = fs::metadata(p).ok()?;
    let is_dir = meta.is_dir();
    Some(DirEntry {
        hidden: is_hidden(p, &meta),
        name: p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string()),
        path: path.to_string(),
        is_dir,
        size: if is_dir { 0 } else { meta.len() },
        modified: meta.modified().ok().and_then(to_epoch_ms),
        extension: if is_dir { String::new() } else { extension_of(p) },
    })
}

/// Stat a set of paths into `DirEntry` rows for a virtual listing (smart folders, CPE-667). Paths that
/// no longer exist or can't be read are silently skipped, so a smart folder self-heals as files move or
/// are deleted rather than showing dead rows.
#[tauri::command]
async fn entries_for_paths(paths: Vec<String>) -> Vec<DirEntry> {
    tauri::async_runtime::spawn_blocking(move || entries_for_paths_impl(paths))
        .await.unwrap()
}

fn entries_for_paths_impl(paths: Vec<String>) -> Vec<DirEntry> {
    paths.iter().filter_map(|p| entry_for_path(p)).collect()
}

/// The volume root of a Windows path for same-volume comparison (CPE-668): the drive (`C:`) or the UNC
/// share (`\\server\share`), or `None` if neither. Pure string logic — kept always-compiled (and unit
/// tested on every OS) even though only the Windows `same_volume` path calls it.
#[cfg_attr(not(windows), allow(dead_code))]
fn windows_volume_root(path: &str) -> Option<String> {
    let p = path.replace('/', "\\");
    if let Some(rest) = p.strip_prefix("\\\\") {
        // UNC: \\server\share\...  → \\server\share (case-insensitive).
        let mut parts = rest.splitn(3, '\\');
        let server = parts.next().filter(|s| !s.is_empty())?;
        let share = parts.next().filter(|s| !s.is_empty())?;
        return Some(format!("\\\\{server}\\{share}").to_lowercase());
    }
    let bytes = p.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        return Some(format!("{}:", (bytes[0] as char).to_ascii_uppercase()));
    }
    None
}

#[cfg(windows)]
fn paths_same_volume(a: &str, b: &str) -> bool {
    match (windows_volume_root(a), windows_volume_root(b)) {
        (Some(x), Some(y)) => x == y,
        _ => false, // unknown volume → treat as different so the caller copies (the safe default)
    }
}

#[cfg(not(windows))]
fn paths_same_volume(a: &str, b: &str) -> bool {
    use std::os::unix::fs::MetadataExt;
    // Compare device ids; when a path doesn't exist yet, fall back to its parent folder's device.
    fn dev(path: &str) -> Option<u64> {
        let p = Path::new(path);
        fs::metadata(p)
            .ok()
            .map(|m| m.dev())
            .or_else(|| p.parent().and_then(|pp| fs::metadata(pp).ok()).map(|m| m.dev()))
    }
    match (dev(a), dev(b)) {
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
}

/// Whether two paths live on the same volume/device, for the drag copy-vs-move rule (CPE-668, epic
/// CPE-661): same volume → move, different → copy. Best-effort — any uncertainty yields `false` so the
/// caller falls back to copy (which never loses the source).
#[tauri::command]
async fn same_volume(a: String, b: String) -> bool {
    tauri::async_runtime::spawn_blocking(move || same_volume_impl(a, b))
        .await.unwrap()
}

fn same_volume_impl(a: String, b: String) -> bool {
    paths_same_volume(&a, &b)
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

/// Reject a "name" that isn't a plain filename — a path separator or `.`/`..` would create/rename an
/// entry *outside* its folder via `join(..)` rather than in place. Defense in depth (the UI validates
/// too, but these commands are directly invokable). Shared by create_dir/create_file/rename_entry
/// (CPE-631/651).
fn valid_entry_name(name: &str) -> Result<(), String> {
    if name.contains('/') || name.contains('\\') || name == "." || name == ".." {
        return Err("Name can't contain a path separator".to_string());
    }
    Ok(())
}

/// Create a new directory `name` inside `path`.
#[tauri::command]
async fn create_dir(path: String, name: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || create_dir_impl(path, name))
        .await.map_err(|e| e.to_string())?
}

fn create_dir_impl(path: String, name: String) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Name cannot be empty".to_string());
    }
    valid_entry_name(name)?;
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
async fn create_file(path: String, name: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || create_file_impl(path, name))
        .await.map_err(|e| e.to_string())?
}

fn create_file_impl(path: String, name: String) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Name cannot be empty".to_string());
    }
    valid_entry_name(name)?;
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
async fn write_file_text(path: String, contents: String) -> Result<u64, String> {
    tauri::async_runtime::spawn_blocking(move || write_file_text_impl(path, contents))
        .await.map_err(|e| e.to_string())?
}

fn write_file_text_impl(path: String, contents: String) -> Result<u64, String> {
    fs::write(&path, contents.as_bytes()).map_err(|e| e.to_string())?;
    Ok(contents.len() as u64)
}

// The archive-listing domain (ArchiveEntry + per-format listers + the extension dispatcher) now lives
// in `cpe_server::archive` (CPE-815); the `read_archive_entries` command below dispatches to it.

/// List an archive's entries without extracting it, for the preview pane. Model lives in
/// `cpe_server::archive` (CPE-815); this is a thin `spawn_blocking` dispatcher.
#[tauri::command]
async fn read_archive_entries(path: String) -> Result<Vec<cpe_server::archive::ArchiveEntry>, String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::archive::read_archive_entries(&path))
        .await.map_err(|e| e.to_string())?
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

// The structured binary previews (hex / PE / MIDI / wasm / torrent) now live in
// `cpe_server::binary_preview` (CPE-815); the `read_preview_info` dispatcher below calls into them.


// Document text extraction (RTF/DOCX/ODT/EPUB) now lives in `cpe_server::doc_text` (CPE-815); the
// `read_preview_info` dispatcher calls into it.

// Structured-data previews (SQLite/spreadsheet/Parquet) now live in `cpe_server::data_preview`
// (CPE-815); the `read_preview_info` dispatcher calls into it.

/// Preview-info readers (PE/MIDI/wasm/torrent/docx/…) parse the WHOLE file, so refuse an absurdly
/// large one up front rather than slurping it into memory (CPE-634). Generous — these are metadata
/// previews of normally-small files. A missing/unreadable file is left for the reader to report.
const PREVIEW_INFO_MAX_BYTES: u64 = 128 * 1024 * 1024;

fn ensure_previewable_size(path: &str, cap: u64) -> Result<(), String> {
    match fs::metadata(path) {
        Ok(m) if m.len() > cap => {
            Err(format!("File is too large to preview ({} bytes; limit {cap}).", m.len()))
        }
        _ => Ok(()),
    }
}

/// Return a human-readable text summary of a binary file, dispatched by
/// extension. Rendered read-only by the preview pane's "info" provider.
#[tauri::command]
async fn read_preview_info(path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || read_preview_info_impl(path))
        .await.map_err(|e| e.to_string())?
}

fn read_preview_info_impl(path: String) -> Result<String, String> {
    ensure_previewable_size(&path, PREVIEW_INFO_MAX_BYTES)?;
    let ext = extension_of(Path::new(&path));
    match ext.as_str() {
        "exe" | "dll" | "sys" | "efi" | "ocx" | "scr" | "cpl" => cpe_server::binary_preview::pe_info(&path),
        "torrent" => cpe_server::binary_preview::torrent_info(&path),
        "wasm" => cpe_server::binary_preview::wasm_info(&path, 256 * 1024),
        "mid" | "midi" => cpe_server::binary_preview::midi_info(&path),
        "rtf" => cpe_server::doc_text::rtf_text(&path),
        "docx" => cpe_server::doc_text::docx_text(&path),
        "odt" => cpe_server::doc_text::odt_text(&path),
        "epub" => cpe_server::doc_text::epub_text(&path),
        "sqlite" | "sqlite3" | "db" => cpe_server::data_preview::sqlite_info(&path),
        "xlsx" | "xlsm" | "ods" => cpe_server::data_preview::spreadsheet_info(&path),
        "parquet" => cpe_server::data_preview::parquet_info(&path),
        // generic binary (.bin/.dat) and anything else routed here: hex dump
        _ => cpe_server::binary_preview::hex_dump(&path, 64 * 1024),
    }
}

// Structured-data browser (CPE-849, epic CPE-721): list a data file's sources (SQLite tables/views,
// Excel/ODS sheets), read a page of typed rows, and run a read-only SQLite query — the interactive-grid
// counterparts to `read_preview_info`'s text summary. Thin async dispatchers into `cpe_server::data_browser`.
#[tauri::command]
async fn data_browser_sources(path: String) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::data_browser::sources(&path))
        .await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn data_browser_page(
    path: String,
    source: String,
    offset: usize,
    limit: usize,
) -> Result<cpe_server::data_browser::Page, String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::data_browser::page(&path, &source, offset, limit))
        .await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn data_browser_query(
    path: String,
    sql: String,
    offset: usize,
    limit: usize,
) -> Result<cpe_server::data_browser::Page, String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::data_browser::query(&path, &sql, offset, limit))
        .await.map_err(|e| e.to_string())?
}

/// Decode an image the webview can't render natively (TIFF, PSD) to a PNG
/// `data:` URL the <img> tag can show (CPE-099/101). PSD uses the psd crate's
/// flattened composite; TIFF uses the image crate. Capped by the source reader,
/// and errors (rather than hangs) on a corrupt file.
/// Transcode TIFF/PSD to a PNG `data:` URL. Model lives in `cpe_server::image_preview` (CPE-815); the
/// command caps the source size first, then dispatches.
#[tauri::command]
async fn read_image_data_url(path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        ensure_previewable_size(&path, PREVIEW_INFO_MAX_BYTES)?;
        cpe_server::image_preview::read_image_data_url(&path)
    })
    .await
    .map_err(|e| e.to_string())?
}


/// A PNG thumbnail of an image file as a `data:` URL the `<img>` tag can show (CPE-642), served from
/// an mtime-keyed on-disk cache (CPE-644). Bounded by the preview size cap so a huge image can't
/// exhaust memory. Errors (rather than hangs) on a non-image, so the frontend falls back to an icon.
#[tauri::command]
fn thumbnail(app: tauri::AppHandle, path: String, max_edge: u32) -> Result<String, String> {
    use base64::Engine;
    ensure_previewable_size(&path, PREVIEW_INFO_MAX_BYTES)?;
    let png = match server_ctx::TauriCtx::new(&app).app_cache_dir() {
        Ok(dir) => cpe_server::thumbnail::thumbnail_cached(&dir.join("thumbnails"), Path::new(&path), max_edge)?,
        Err(_) => cpe_server::thumbnail::make_thumbnail_png(Path::new(&path), max_edge)?, // no cache dir
    };
    Ok(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png)))
}

/// Read a text file's contents for the preview pane, capped at `max_bytes` so a
/// huge file can never be slurped into memory. Errors (rather than truncating)
/// when the file is too large, unreadable, or not valid UTF-8 — the frontend
/// shows a "can't preview" state in that case.
#[tauri::command]
async fn read_file_text(path: String, max_bytes: u64) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || read_file_text_impl(path, max_bytes))
        .await.map_err(|e| e.to_string())?
}

fn read_file_text_impl(path: String, max_bytes: u64) -> Result<String, String> {
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

/// Read a byte range of a file without loading the whole file — backs the hex inspector's paging
/// (CPE-772, epic CPE-719). Seeks to `offset` (past EOF yields an empty slice, not an error, so the
/// viewer can page freely) and reads up to `len` bytes, clamped to EOF.
#[tauri::command]
async fn read_file_range(path: String, offset: u64, len: u64) -> Result<Vec<u8>, String> {
    tauri::async_runtime::spawn_blocking(move || read_file_range_impl(path, offset, len))
        .await
        .map_err(|e| e.to_string())?
}

fn read_file_range_impl(path: String, offset: u64, len: u64) -> Result<Vec<u8>, String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = fs::File::open(&path).map_err(|e| e.to_string())?;
    let total = f.metadata().map_err(|e| e.to_string())?.len();
    if offset >= total {
        return Ok(Vec::new());
    }
    let want = len.min(total - offset);
    f.seek(SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; want as usize];
    f.read_exact(&mut buf).map_err(|e| e.to_string())?;
    Ok(buf)
}

/// Total byte length of a file (CPE-772) — lets the hex viewer size its scrollbar without reading.
#[tauri::command]
async fn file_len(path: String) -> Result<u64, String> {
    tauri::async_runtime::spawn_blocking(move || {
        fs::metadata(&path).map(|m| m.len()).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Set POSIX permission bits (chmod) on a file, returning the prior low-9-bit mode for undo (CPE-785).
/// Unix only — Windows uses attribute toggles (`set_readonly` + future attrs) instead.
#[cfg(unix)]
#[tauri::command]
async fn set_permissions(path: String, mode: u32) -> Result<u32, String> {
    tauri::async_runtime::spawn_blocking(move || set_permissions_impl(path, mode))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(unix)]
fn set_permissions_impl(path: String, mode: u32) -> Result<u32, String> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(&path).map_err(|e| e.to_string())?.permissions();
    let prior = perms.mode() & 0o777;
    perms.set_mode((perms.mode() & !0o777) | (mode & 0o777));
    fs::set_permissions(&path, perms).map_err(|e| e.to_string())?;
    Ok(prior)
}

#[cfg(not(unix))]
#[tauri::command]
async fn set_permissions(path: String, mode: u32) -> Result<u32, String> {
    let _ = (path, mode);
    Err("POSIX permissions aren't available on this platform.".to_string())
}

/// Toggle a file's read-only flag (cross-platform), returning the prior state for undo (CPE-785).
#[tauri::command]
async fn set_readonly(path: String, readonly: bool) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || set_readonly_impl(path, readonly))
        .await
        .map_err(|e| e.to_string())?
}

fn set_readonly_impl(path: String, readonly: bool) -> Result<bool, String> {
    let mut perms = fs::metadata(&path).map_err(|e| e.to_string())?.permissions();
    let prior = perms.readonly();
    perms.set_readonly(readonly);
    fs::set_permissions(&path, perms).map_err(|e| e.to_string())?;
    Ok(prior)
}

fn ft_from_ms(ms: i64) -> filetime::FileTime {
    let secs = ms.div_euclid(1000);
    let nanos = (ms.rem_euclid(1000) * 1_000_000) as u32;
    filetime::FileTime::from_unix_time(secs, nanos)
}
fn ms_from_ft(ft: filetime::FileTime) -> i64 {
    ft.unix_seconds() * 1000 + i64::from(ft.nanoseconds() / 1_000_000)
}

/// Set a file's modified/accessed timestamps (CPE-785). Each is optional (unchanged when `None`); returns
/// the prior `(modified, accessed)` as epoch-ms for undo. Cross-platform via the `filetime` crate.
#[tauri::command]
async fn set_file_times(
    path: String,
    modified_ms: Option<i64>,
    accessed_ms: Option<i64>,
) -> Result<(i64, i64), String> {
    tauri::async_runtime::spawn_blocking(move || set_file_times_impl(path, modified_ms, accessed_ms))
        .await
        .map_err(|e| e.to_string())?
}

fn set_file_times_impl(
    path: String,
    modified_ms: Option<i64>,
    accessed_ms: Option<i64>,
) -> Result<(i64, i64), String> {
    let meta = fs::metadata(&path).map_err(|e| e.to_string())?;
    let prior_m = filetime::FileTime::from_last_modification_time(&meta);
    let prior_a = filetime::FileTime::from_last_access_time(&meta);
    let m = modified_ms.map(ft_from_ms).unwrap_or(prior_m);
    let a = accessed_ms.map(ft_from_ms).unwrap_or(prior_a);
    filetime::set_file_times(&path, a, m).map_err(|e| e.to_string())?;
    Ok((ms_from_ft(prior_m), ms_from_ft(prior_a)))
}

/// Toggle a Windows file attribute (`hidden` / `system` / `archive`), returning the prior state for undo
/// (CPE-785). Windows only.
#[cfg(windows)]
#[tauri::command]
async fn set_file_attribute(path: String, attr: String, value: bool) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || set_file_attribute_impl(path, attr, value))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(windows)]
fn set_file_attribute_impl(path: String, attr: String, value: bool) -> Result<bool, String> {
    use windows::core::HSTRING;
    use windows::Win32::Storage::FileSystem::{
        GetFileAttributesW, SetFileAttributesW, FILE_ATTRIBUTE_ARCHIVE, FILE_ATTRIBUTE_HIDDEN,
        FILE_ATTRIBUTE_SYSTEM, FILE_FLAGS_AND_ATTRIBUTES,
    };
    let flag = match attr.as_str() {
        "hidden" => FILE_ATTRIBUTE_HIDDEN.0,
        "system" => FILE_ATTRIBUTE_SYSTEM.0,
        "archive" => FILE_ATTRIBUTE_ARCHIVE.0,
        other => return Err(format!("unknown attribute: {other}")),
    };
    let wide = HSTRING::from(path.as_str());
    // SAFETY: `wide` is a valid, NUL-terminated wide string for the duration of both calls.
    let cur = unsafe { GetFileAttributesW(&wide) };
    if cur == u32::MAX {
        return Err("couldn't read file attributes".to_string());
    }
    let prior = cur & flag != 0;
    let next = if value { cur | flag } else { cur & !flag };
    unsafe { SetFileAttributesW(&wide, FILE_FLAGS_AND_ATTRIBUTES(next)) }.map_err(|e| e.to_string())?;
    Ok(prior)
}

#[cfg(not(windows))]
#[tauri::command]
async fn set_file_attribute(path: String, attr: String, value: bool) -> Result<bool, String> {
    let _ = (path, attr, value);
    Err("Windows file attributes aren't available on this platform.".to_string())
}

// ---- Read a file's editable attributes (CPE-786, epic CPE-710) ------------------------------------
// Current state for the attributes editor: Windows readonly/hidden/system/archive from GetFileAttributesW;
// POSIX readonly (owner-write bit) + the octal mode string. The write side (set_readonly /
// set_file_attribute / set_permissions, CPE-785) already exists — this is the missing read so the editor
// can show current values before toggling.

/// A file's editable attributes. Windows fills the four flag bits; POSIX fills `mode` (octal) + readonly.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FileAttributes {
    readonly: bool,
    hidden: bool,
    system: bool,
    archive: bool,
    /// POSIX permission bits as an octal string (e.g. "644"); `None` on Windows.
    mode: Option<String>,
}

#[tauri::command]
async fn read_attributes(path: String) -> Result<FileAttributes, String> {
    tauri::async_runtime::spawn_blocking(move || read_attributes_impl(&path))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(windows)]
fn read_attributes_impl(path: &str) -> Result<FileAttributes, String> {
    use windows::core::HSTRING;
    use windows::Win32::Storage::FileSystem::GetFileAttributesW;
    // Documented FILE_ATTRIBUTE_* bits (stable Win32 constants): READONLY=0x1, HIDDEN=0x2, SYSTEM=0x4,
    // ARCHIVE=0x20 — matched numerically to avoid the windows-crate feature-gated const imports.
    let wide = HSTRING::from(path);
    // SAFETY: `wide` is a valid NUL-terminated wide string for the call.
    let attrs = unsafe { GetFileAttributesW(&wide) };
    if attrs == u32::MAX {
        return Err("couldn't read file attributes".to_string());
    }
    Ok(FileAttributes {
        readonly: attrs & 0x1 != 0,
        hidden: attrs & 0x2 != 0,
        system: attrs & 0x4 != 0,
        archive: attrs & 0x20 != 0,
        mode: None,
    })
}

#[cfg(not(windows))]
fn read_attributes_impl(path: &str) -> Result<FileAttributes, String> {
    use std::os::unix::fs::PermissionsExt;
    let meta = fs::metadata(path).map_err(|e| e.to_string())?;
    let mode = meta.permissions().mode() & 0o777;
    let hidden = Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with('.'))
        .unwrap_or(false);
    Ok(FileAttributes {
        readonly: mode & 0o200 == 0, // no owner-write bit
        hidden,
        system: false,
        archive: false,
        mode: Some(format!("{mode:o}")),
    })
}

/// Rename a single entry in place. Returns the new path.
#[tauri::command]
async fn rename_entry(path: String, new_name: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || rename_entry_impl(path, new_name))
        .await.map_err(|e| e.to_string())?
}

fn rename_entry_impl(path: String, new_name: String) -> Result<String, String> {
    let new_name = new_name.trim();
    if new_name.is_empty() {
        return Err("Name cannot be empty".to_string());
    }
    // A rename is name-only — reject a separator/traversal (CPE-631, shared guard).
    valid_entry_name(new_name)?;
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
async fn delete_to_trash(paths: Vec<String>) -> Vec<OpResult> {
    tauri::async_runtime::spawn_blocking(move || delete_to_trash_impl(paths))
        .await.unwrap()
}

fn delete_to_trash_impl(paths: Vec<String>) -> Vec<OpResult> {
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
async fn can_restore_from_trash() -> bool {
    tauri::async_runtime::spawn_blocking(can_restore_from_trash_impl)
        .await.unwrap()
}

fn can_restore_from_trash_impl() -> bool {
    cfg!(any(target_os = "windows", target_os = "linux"))
}

/// Restore previously-trashed items to their original paths.
#[cfg(any(target_os = "windows", target_os = "linux"))]
#[tauri::command]
async fn restore_from_trash(paths: Vec<String>) -> Vec<OpResult> {
    tauri::async_runtime::spawn_blocking(move || restore_from_trash_impl(paths))
        .await.unwrap()
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn restore_from_trash_impl(paths: Vec<String>) -> Vec<OpResult> {
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
/// `can_restore_from_trash_impl()` is false so delete is never pushed onto the undo
/// stack in the first place.
#[cfg(not(any(target_os = "windows", target_os = "linux")))]
#[tauri::command]
async fn restore_from_trash(paths: Vec<String>) -> Vec<OpResult> {
    tauri::async_runtime::spawn_blocking(move || restore_from_trash_impl(paths))
        .await.unwrap()
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn restore_from_trash_impl(paths: Vec<String>) -> Vec<OpResult> {
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
async fn delete_permanent(paths: Vec<String>) -> Vec<OpResult> {
    tauri::async_runtime::spawn_blocking(move || delete_permanent_impl(paths))
        .await.unwrap()
}

fn delete_permanent_impl(paths: Vec<String>) -> Vec<OpResult> {
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
async fn copy_entries(paths: Vec<String>, dest: String) -> Vec<OpResult> {
    tauri::async_runtime::spawn_blocking(move || copy_entries_impl(paths, dest))
        .await.unwrap()
}

/// Copy `src` into `dest_dir` (auto-renaming on collision), returning the path actually written. The
/// single source of truth for a copy-into-folder, shared by the bulk copy command and the watch executor.
fn do_copy_into(src: &Path, dest_dir: &Path) -> Result<PathBuf, String> {
    let file_name = src
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "Invalid file name".to_string())?;
    if src.is_dir() && is_self_or_descendant(src, dest_dir) {
        return Err("Cannot copy a folder into itself".to_string());
    }
    let target = unique_target(dest_dir, file_name);
    let result = if src.is_dir() {
        copy_dir_all(src, &target)
    } else {
        fs::copy(src, &target).map(|_| ())
    };
    result.map(|()| target).map_err(|e| e.to_string())
}

/// Move `src` into `dest_dir` (auto-renaming on collision), returning the path actually written. Falls
/// back to copy-then-delete across filesystem boundaries (never deletes the source on a failed copy).
/// Shared by the bulk move command and the watch executor.
fn do_move_into(src: &Path, dest_dir: &Path) -> Result<PathBuf, String> {
    let file_name = src
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "Invalid file name".to_string())?;
    if src.is_dir() && is_self_or_descendant(src, dest_dir) {
        return Err("Cannot move a folder into itself".to_string());
    }
    let target = unique_target(dest_dir, file_name);
    if fs::rename(src, &target).is_ok() {
        return Ok(target);
    }
    // Cross-volume move: copy, then remove the original only if the copy fully succeeded.
    let copied = if src.is_dir() {
        copy_dir_all(src, &target)
    } else {
        fs::copy(src, &target).map(|_| ())
    };
    copied.map_err(|e| e.to_string())?;
    let removed = if src.is_dir() {
        fs::remove_dir_all(src)
    } else {
        fs::remove_file(src)
    };
    removed.map_err(|e| format!("Copied, but could not remove original: {e}"))?;
    Ok(target)
}

fn copy_entries_impl(paths: Vec<String>, dest: String) -> Vec<OpResult> {
    let dest_dir = PathBuf::from(&dest);
    paths
        .iter()
        .map(|p| {
            let src = Path::new(p);
            match do_copy_into(src, &dest_dir) {
                Ok(target) => OpResult::ok(&target),
                Err(e) => OpResult::err(src, e),
            }
        })
        .collect()
}

/// Move entries into `dest`, auto-renaming on collision. Falls back to
/// copy-then-delete when the move crosses a filesystem boundary (`fs::rename`
/// fails across volumes, e.g. C: -> Z:).
#[tauri::command]
async fn move_entries(paths: Vec<String>, dest: String) -> Vec<OpResult> {
    tauri::async_runtime::spawn_blocking(move || move_entries_impl(paths, dest))
        .await.unwrap()
}

fn move_entries_impl(paths: Vec<String>, dest: String) -> Vec<OpResult> {
    let dest_dir = PathBuf::from(&dest);
    paths
        .iter()
        .map(|p| {
            let src = Path::new(p);
            match do_move_into(src, &dest_dir) {
                Ok(target) => OpResult::ok(&target),
                Err(e) => OpResult::err(src, e),
            }
        })
        .collect()
}

// ---- Watched-folder action executor (CPE-794, epic CPE-734) ---------------------------------------
// Executes the resolved action pipeline the frontend planner (watchRules.planForEntry, CPE-793) produced
// for a file that landed in a watched folder — deterministic filesystem moves only (move / copy / rename;
// the `tag` action is app metadata applied via the tag store, not here). Actions run in order over the
// file, so a `move`/`rename` updates the working path for later steps and a `copy` leaves the original in
// place; each step yields a per-action `OpResult` (never all-or-nothing). Reuses `do_move_into` /
// `do_copy_into` / `rename_entry_impl`. The live `notify` watcher that *fires* this (with oscillation
// guarding) is the integration tail — this is the headless, unit-tested core.

/// One resolved watch action to execute: `kind` is `move` | `copy` | `rename`; `resolved` is the
/// destination directory (move/copy) or the new file name (rename), already expanded by the planner.
#[derive(serde::Deserialize)]
struct WatchAction {
    kind: String,
    resolved: String,
}

/// Execute a landed file's resolved action pipeline. See the module comment. Async per the commands rule.
#[tauri::command]
async fn run_watch_actions(path: String, actions: Vec<WatchAction>) -> Vec<OpResult> {
    tauri::async_runtime::spawn_blocking(move || run_watch_actions_impl(path, actions))
        .await
        .unwrap_or_default()
}

fn run_watch_actions_impl(path: String, actions: Vec<WatchAction>) -> Vec<OpResult> {
    let mut current = PathBuf::from(&path);
    let mut out = Vec::with_capacity(actions.len());
    for action in &actions {
        let result: Result<PathBuf, String> = match action.kind.as_str() {
            "move" => do_move_into(&current, Path::new(&action.resolved)),
            "copy" => do_copy_into(&current, Path::new(&action.resolved)),
            "rename" => rename_entry_impl(current.to_string_lossy().to_string(), action.resolved.clone())
                .map(PathBuf::from),
            other => Err(format!("unknown watch action: {other}")),
        };
        match result {
            Ok(new_path) => {
                out.push(OpResult::ok(&new_path));
                // move/rename relocate the file; a copy leaves the original where it is.
                if action.kind == "move" || action.kind == "rename" {
                    current = new_path;
                }
            }
            Err(e) => out.push(OpResult::err(&current, e)),
        }
    }
    out
}

// ---- Transfer engine (CPE-620, epic CPE-613) -------------------------------------------------
// A streamed copy/move engine with byte-level progress, cancellation, and a per-batch conflict
// policy. The pure core (`run_transfer`) takes a progress closure + a cancel flag so it is fully
// unit-testable headlessly; the async `start_transfer` command is the thin tail that spawns it on a
// thread and forwards progress as Tauri events.

/// Whether a batch copies or moves its sources.
#[derive(Clone, Copy, PartialEq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum TransferKind {
    Copy,
    Move,
}

/// How a name collision at the destination is resolved for the whole batch.
#[derive(Clone, Copy, PartialEq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum ConflictPolicy {
    /// Replace the existing entry.
    Overwrite,
    /// Leave the existing entry; don't transfer this source.
    Skip,
    /// Keep both — auto-number the new one ("name (2)").
    Keepboth,
}

/// A progress snapshot emitted while a transfer runs.
#[derive(Clone, serde::Serialize)]
struct TransferProgress {
    id: u64,
    total_bytes: u64,
    done_bytes: u64,
    total_items: u64,
    done_items: u64,
    current: String,
}

/// The final outcome of a transfer.
#[derive(Clone, Default, serde::Serialize)]
struct TransferReport {
    id: u64,
    transferred: u64,
    skipped: u64,
    failed: u64,
    cancelled: bool,
    errors: Vec<String>,
}

/// Sum the byte size + file count under `p`, skip-on-error and without following symlinked dirs
/// (cycle-safe, like the other walks — CPE-609/611). Used to seed the progress totals.
fn measure_one(p: &Path, bytes: &mut u64, files: &mut u64) {
    match fs::metadata(p) {
        Ok(m) if m.is_dir() => {
            let Ok(rd) = fs::read_dir(p) else { return };
            for e in rd.flatten() {
                if e.file_type().map(|t| t.is_symlink()).unwrap_or(false) {
                    *files += 1; // count the link itself, don't descend
                    continue;
                }
                measure_one(&e.path(), bytes, files);
            }
        }
        Ok(m) => {
            *bytes += m.len();
            *files += 1;
        }
        Err(_) => {}
    }
}

/// Resolve a collision at `base_target` per `policy`. `Some(path)` is where to write; `None` means
/// skip this source (policy `Skip` with an existing target). `Overwrite` removes the existing entry.
fn resolve_conflict(base_target: &Path, policy: ConflictPolicy) -> Option<PathBuf> {
    if !base_target.exists() {
        return Some(base_target.to_path_buf());
    }
    match policy {
        ConflictPolicy::Skip => None,
        ConflictPolicy::Keepboth => {
            let dir = base_target.parent().unwrap_or_else(|| Path::new("."));
            let name = base_target.file_name().and_then(|n| n.to_str()).unwrap_or("file");
            Some(unique_target(dir, name))
        }
        ConflictPolicy::Overwrite => {
            if base_target.is_dir() {
                let _ = fs::remove_dir_all(base_target);
            } else {
                let _ = fs::remove_file(base_target);
            }
            Some(base_target.to_path_buf())
        }
    }
}

/// Copy one file, streamed in fixed chunks, advancing `prog.done_bytes` and emitting a throttled
/// progress event. Returns `Ok(false)` if cancelled mid-file (the partial dest is left for the
/// caller's policy to overwrite next run — we don't delete, to stay predictable).
fn stream_copy_file(
    src: &Path,
    dst: &Path,
    cancel: &std::sync::atomic::AtomicBool,
    prog: &mut TransferProgress,
    emit: &mut dyn FnMut(&TransferProgress),
    last_emit: &mut u64,
) -> std::io::Result<bool> {
    use std::io::{Read, Write};
    use std::sync::atomic::Ordering;
    let mut r = fs::File::open(src)?;
    let mut w = fs::File::create(dst)?;
    let mut buf = vec![0u8; 128 * 1024];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Ok(false);
        }
        let n = r.read(&mut buf)?;
        if n == 0 {
            break;
        }
        w.write_all(&buf[..n])?;
        prog.done_bytes += n as u64;
        if prog.done_bytes - *last_emit >= 512 * 1024 {
            *last_emit = prog.done_bytes;
            emit(prog);
        }
    }
    w.flush()?;
    Ok(true)
}

/// Recursively copy `src` -> `dst`, streaming each file. Returns `false` only when **cancelled** (the
/// caller stops the whole batch); per-item errors are recorded in `report` and don't abort the tree
/// (same skip-on-error ethos as `list_dir`). Symlinked directories are not descended (cycle-safe).
#[allow(clippy::too_many_arguments)]
fn copy_tree_streamed(
    src: &Path,
    dst: &Path,
    cancel: &std::sync::atomic::AtomicBool,
    prog: &mut TransferProgress,
    emit: &mut dyn FnMut(&TransferProgress),
    last_emit: &mut u64,
    report: &mut TransferReport,
) -> bool {
    use std::sync::atomic::Ordering;
    let ft = match fs::symlink_metadata(src) {
        Ok(m) => m.file_type(),
        Err(e) => {
            report.failed += 1;
            report.errors.push(format!("{}: {e}", src.display()));
            return true;
        }
    };
    if ft.is_dir() {
        if let Err(e) = fs::create_dir_all(dst) {
            report.failed += 1;
            report.errors.push(format!("{}: {e}", dst.display()));
            return true;
        }
        let Ok(rd) = fs::read_dir(src) else { return true };
        for e in rd.flatten() {
            if cancel.load(Ordering::Relaxed) {
                return false;
            }
            let child = e.path();
            if !copy_tree_streamed(&child, &dst.join(e.file_name()), cancel, prog, emit, last_emit, report) {
                return false;
            }
        }
        true
    } else {
        prog.current = src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        match stream_copy_file(src, dst, cancel, prog, emit, last_emit) {
            Ok(true) => {
                prog.done_items += 1;
                emit(prog);
                true
            }
            Ok(false) => false,
            Err(e) => {
                report.failed += 1;
                report.errors.push(format!("{}: {e}", src.display()));
                prog.done_items += 1;
                true
            }
        }
    }
}

/// Run a whole transfer batch. Pure + headless: `cancel` is polled between chunks and `emit` receives
/// progress snapshots. Returns the final report. A `Move` uses a same-volume rename fast path and
/// only deletes a source after its copy fully succeeds (never on partial failure).
fn run_transfer(
    id: u64,
    sources: &[PathBuf],
    dest_dir: &Path,
    kind: TransferKind,
    policy: ConflictPolicy,
    cancel: &std::sync::atomic::AtomicBool,
    mut emit: impl FnMut(&TransferProgress),
) -> TransferReport {
    use std::sync::atomic::Ordering;
    let measured: Vec<(u64, u64)> = sources
        .iter()
        .map(|s| {
            let (mut b, mut f) = (0, 0);
            measure_one(s, &mut b, &mut f);
            (b, f)
        })
        .collect();
    let mut prog = TransferProgress {
        id,
        total_bytes: measured.iter().map(|(b, _)| b).sum(),
        done_bytes: 0,
        total_items: measured.iter().map(|(_, f)| f).sum(),
        done_items: 0,
        current: String::new(),
    };
    let mut report = TransferReport { id, ..Default::default() };
    let mut last_emit = 0u64;
    emit(&prog);

    for (src, (sb, sf)) in sources.iter().zip(measured.iter()) {
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            break;
        }
        let Some(name) = src.file_name().and_then(|n| n.to_str()) else {
            report.failed += 1;
            report.errors.push(format!("{}: invalid name", src.display()));
            continue;
        };
        if src.is_dir() && is_self_or_descendant(src, dest_dir) {
            report.failed += 1;
            report.errors.push(format!("{name}: can't transfer a folder into itself"));
            continue;
        }
        let target = match resolve_conflict(&dest_dir.join(name), policy) {
            Some(t) => t,
            None => {
                report.skipped += 1;
                prog.done_bytes += sb;
                prog.done_items += sf;
                emit(&prog);
                continue;
            }
        };
        // Same-volume move: an atomic rename, no byte streaming needed.
        if kind == TransferKind::Move && fs::rename(src, &target).is_ok() {
            report.transferred += 1;
            prog.done_bytes += sb;
            prog.done_items += sf;
            last_emit = prog.done_bytes;
            emit(&prog);
            continue;
        }
        let failed_before = report.failed;
        if !copy_tree_streamed(src, &target, cancel, &mut prog, &mut emit, &mut last_emit, &mut report) {
            report.cancelled = true;
            break;
        }
        report.transferred += 1;
        // For a (cross-volume) move, delete the source only if its copy had zero failures.
        if kind == TransferKind::Move && report.failed == failed_before {
            let _ = if src.is_dir() { fs::remove_dir_all(src) } else { fs::remove_file(src) };
        }
    }
    prog.current.clear();
    emit(&prog);
    report
}

/// Registry of live transfers' cancel flags, keyed by transfer id, so `cancel_transfer` can signal a
/// running `start_transfer` thread.
static TRANSFER_CANCELS: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<u64, std::sync::Arc<std::sync::atomic::AtomicBool>>>,
> = std::sync::OnceLock::new();
static TRANSFER_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn transfer_registry(
) -> &'static std::sync::Mutex<std::collections::HashMap<u64, std::sync::Arc<std::sync::atomic::AtomicBool>>> {
    TRANSFER_CANCELS.get_or_init(Default::default)
}

/// Start a copy/move on a background thread, returning its id immediately. Progress is emitted as
/// `transfer://progress` events and the final `TransferReport` as `transfer://done` (CPE-620).
#[tauri::command]
fn start_transfer(
    app: tauri::AppHandle,
    sources: Vec<String>,
    dest: String,
    kind: TransferKind,
    policy: ConflictPolicy,
) -> u64 {
    use std::sync::atomic::Ordering;
    let id = TRANSFER_SEQ.fetch_add(1, Ordering::Relaxed);
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    transfer_registry().lock().unwrap().insert(id, cancel.clone());
    let srcs: Vec<PathBuf> = sources.iter().map(PathBuf::from).collect();
    let dest_dir = PathBuf::from(dest);
    let ctx = server_ctx::TauriCtx::new(&app);
    std::thread::spawn(move || {
        let report = run_transfer(id, &srcs, &dest_dir, kind, policy, &cancel, |p| {
            let _ = ctx.emit_json("transfer://progress", serde_json::to_value(p).unwrap_or_default());
        });
        let _ = ctx.emit_json("transfer://done", serde_json::to_value(&report).unwrap_or_default());
        transfer_registry().lock().unwrap().remove(&id);
    });
    id
}

/// Signal a running transfer to stop at the next chunk boundary (CPE-620).
#[tauri::command]
fn cancel_transfer(id: u64) {
    use std::sync::atomic::Ordering;
    if let Some(flag) = transfer_registry().lock().unwrap().get(&id) {
        flag.store(true, Ordering::Relaxed);
    }
}

/// Move each `from` to an EXACT `to` path. Used by undo, which must restore an
/// item to its original name — auto-renaming here would defeat the point (undo
/// of "rename a -> b" must produce "a", not "a - Copy").
///
/// Refuses to overwrite: if `to` already exists, the undo fails loudly rather
/// than clobbering whatever now occupies that name.
#[tauri::command]
async fn move_exact(pairs: Vec<(String, String)>) -> Vec<OpResult> {
    tauri::async_runtime::spawn_blocking(move || move_exact_impl(pairs))
        .await.unwrap()
}

fn move_exact_impl(pairs: Vec<(String, String)>) -> Vec<OpResult> {
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

/// Detailed metadata for the Properties dialog. Model lives in `cpe_server::model` (CPE-815); this is a
/// thin `spawn_blocking` dispatcher.
#[tauri::command]
async fn entry_info(path: String) -> Result<EntryInfo, String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::model::entry_info(&path))
        .await.map_err(|e| e.to_string())?
}

/// Image dimensions + basic EXIF for the Properties dialog (CPE-659). Model lives in
/// `cpe_server::image_preview` (CPE-815); this is a thin `spawn_blocking` dispatcher.
#[tauri::command]
async fn image_meta(path: String) -> Result<cpe_server::image_preview::ImageMeta, String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::image_preview::image_meta(&path))
        .await.map_err(|e| e.to_string())?
}

/// Recursive counts + size of a directory tree, for the Properties dialog (CPE-649): number of files,
/// number of sub-folders, and total bytes. Cycle-safe (doesn't follow symlinked dirs) and bounded —
/// stops at a large entry cap (reporting `truncated`) so it can't spin on a pathological tree.
/// Model lives in `cpe_server::folder_stats` (CPE-815); this is a thin `spawn_blocking` dispatcher.
#[tauri::command]
async fn folder_stats(path: String) -> Result<cpe_server::folder_stats::FolderStats, String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::folder_stats::compute(&path))
        .await.map_err(|e| e.to_string())?
}

/// Total recursive size of a directory tree in bytes. Model lives in `cpe_server::disk_usage`
/// (CPE-815); this is a thin `spawn_blocking` dispatcher.
#[tauri::command]
async fn dir_size(path: String) -> Result<u64, String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::disk_usage::dir_size(&path))
        .await.map_err(|e| e.to_string())?
}

/// The immediate children of `path`, each with its recursive size — the per-child breakdown the treemap
/// needs for the space analyzer (CPE-749). Model lives in `cpe_server::disk_usage` (CPE-815); this is a
/// thin `spawn_blocking` dispatcher.
#[tauri::command]
async fn dir_children_sizes(path: String) -> Result<Vec<cpe_server::disk_usage::ChildSize>, String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::disk_usage::dir_children_sizes(&path))
        .await.map_err(|e| e.to_string())?
}

/// Compute the SHA-256 checksum of a file, returned as lowercase hex (CPE-412). Streamed in fixed
/// chunks so a multi-GB file never loads into memory. A directory, missing, or unreadable path is an
/// `Err`, never a panic. Opt-in from the UI (hashing is I/O-bound) — never run automatically.
#[tauri::command]
async fn hash_file(path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::checksum::hash_file(&path))
        .await.map_err(|e| e.to_string())?
}

// ---- Backup copy engine (CPE-797, epic CPE-736) ---------------------------------------------------
// Executes a plan produced by the frontend `planBackup` (src/lib/backup.ts, CPE-796): copy new files,
// overwrite changed ones, and — in mirror mode — delete files the plan flagged as extraneous, verifying
// each written file by sha256. The plan lists are **relative paths** under the source/dest roots, so this
// engine never widens the blast radius beyond `dest_root`. Per-file `OpResult` (never all-or-nothing) so a
// single locked file doesn't sink the whole run. Reuses `sha256_file` for verification. This is the
// headless, deterministic core — streamed progress + the on-drive-connect scheduler are a follow-up child.

// The backup copy engine (safe-join, verified copy, the plan executor) now lives in `cpe_server::backup`
// (CPE-821). The two commands below are thin dispatchers; the streaming one keeps its `ipc::Channel` in
// this adapter and feeds the extracted walker.

/// Streamed backup run (CPE-798 live progress): sends each file's `OpResult` over `on_result` in small
/// batches as it completes. Returns the total number of results emitted.
#[tauri::command]
async fn apply_backup_plan_stream(
    source_root: String,
    dest_root: String,
    copy: Vec<String>,
    update: Vec<String>,
    delete: Vec<String>,
    verify: bool,
    on_result: tauri::ipc::Channel<Vec<OpResult>>,
) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut batch: Vec<OpResult> = Vec::new();
        let mut total = 0usize;
        cpe_server::backup::apply_backup_plan_walk(&source_root, &dest_root, &copy, &update, &delete, verify, |r| {
            total += 1;
            batch.push(r);
            if batch.len() >= 16 {
                let _ = on_result.send(std::mem::take(&mut batch));
            }
        });
        if !batch.is_empty() {
            let _ = on_result.send(batch);
        }
        total
    })
    .await
    .map_err(|e| e.to_string())
}

/// Execute a backup plan (CPE-797). Model lives in `cpe_server::backup` (CPE-821); thin dispatcher.
#[tauri::command]
async fn apply_backup_plan(
    source_root: String,
    dest_root: String,
    copy: Vec<String>,
    update: Vec<String>,
    delete: Vec<String>,
    verify: bool,
) -> Vec<OpResult> {
    tauri::async_runtime::spawn_blocking(move || {
        cpe_server::backup::apply_backup_plan(&source_root, &dest_root, &copy, &update, &delete, verify)
    })
    .await
    .unwrap_or_default()
}

/// Recursively checksum every file under `path` into a baseline manifest — the on-demand baseline for
/// the integrity guard (CPE-791). Symlinks are not followed and unreadable files are skipped; the result
/// is sorted by path for a stable diff. Model lives in `cpe_server::checksum` (CPE-815).
#[tauri::command]
async fn checksum_folder(path: String) -> Result<Vec<cpe_server::checksum::ChecksumEntry>, String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::checksum::checksum_folder(&path))
        .await
        .map_err(|e| e.to_string())?
}

// ---- Folder-tree scan for the compare view (CPE-779, epic CPE-722) --------------------------------
// Recursively scan a folder into a nested tree the frontend `diffTrees` (CPE-777) consumes: files carry
// size + epoch-ms mtime (what the diff compares on), dirs carry children. Symlinks aren't followed and
// unreadable entries/dirs are skipped (matching `checksum_walk`/`dir_size`). Bounded by `max_depth` so a
// pathological tree can't blow the stack or the payload; beyond the cap a dir is returned with no children.

/// Scan the children of `path` into a `CompareNode`-shaped tree (CPE-779). Model lives in
/// `cpe_server::compare` (CPE-815); this is a thin `spawn_blocking` dispatcher.
#[tauri::command]
async fn scan_tree(path: String, max_depth: u32) -> Result<Vec<cpe_server::compare::TreeNode>, String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::compare::scan_tree(&path, max_depth))
        .await
        .map_err(|e| e.to_string())?
}

/// Create a symbolic link at `link_path` pointing to `target` (CPE-802, epic CPE-715). On Windows a
/// directory target makes a dir-symlink, else a file-symlink; the OS error is returned on failure (e.g.
/// Windows symlink creation without Developer Mode / admin), so the UI can prompt for elevation.
#[tauri::command]
async fn create_symlink(target: String, link_path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::links::create_symlink(&target, &link_path))
        .await
        .map_err(|e| e.to_string())?
}

/// Create a hardlink at `link_path` for the same file data as `target` (CPE-802). Cross-platform.
/// Model lives in `cpe_server::links` (CPE-815); this is a thin `spawn_blocking` dispatcher.
#[tauri::command]
async fn create_hard_link(target: String, link_path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::links::create_hard_link(&target, &link_path))
        .await
        .map_err(|e| e.to_string())?
}

/// Classify the drive a path lives on (CPE-805, epic CPE-716) — fixed / removable / network / cdrom / ram
/// / unknown — so the sidebar can badge removable & network drives. Windows uses `GetDriveTypeW`; unix
/// returns a best-effort `fixed` for now (richer classification is a follow-up).
#[tauri::command]
async fn drive_type(path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || drive_type_impl(&path))
        .await
        .map_err(|e| e.to_string())
}

#[cfg(windows)]
fn drive_type_impl(path: &str) -> String {
    use windows::core::HSTRING;
    use windows::Win32::Storage::FileSystem::GetDriveTypeW;
    // GetDriveTypeW wants a root like "C:\"; derive it from a drive-letter path, else pass the path.
    let root = if path.len() >= 2 && path.as_bytes()[1] == b':' {
        format!("{}:\\", &path[..1])
    } else {
        path.to_string()
    };
    let wide = HSTRING::from(root);
    // SAFETY: `wide` is a valid NUL-terminated wide string for the call. The returned values are the
    // stable, documented DRIVE_* Win32 constants (2=removable, 3=fixed, 4=remote, 5=cdrom, 6=ramdisk).
    match unsafe { GetDriveTypeW(&wide) } {
        2 => "removable",
        3 => "fixed",
        4 => "network",
        5 => "cdrom",
        6 => "ram",
        _ => "unknown",
    }
    .to_string()
}

#[cfg(not(windows))]
fn drive_type_impl(_path: &str) -> String {
    // Best-effort until unix mount-type classification lands (a follow-up).
    "fixed".to_string()
}

// ---- Session audit journal (CPE-800, epic CPE-733) ------------------------------------------------
// Thin I/O shell over the `audit_journal` module: record an Agent Watch activity event to a durable
// per-session JSON-lines journal under the app-data dir, and list / read past sessions back for the
// history browser + export (CPE-799 / CPE-801). Async (spawn_blocking) per the async-commands rule; the
// journal is only touched when the frontend records activity, so it costs nothing when Agent Watch is off.

/// Resolve (and create) the journal directory under the app-data dir.
fn audit_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = server_ctx::TauriCtx::new(app).app_data_dir()?.join("audit");
    Ok(dir)
}

/// Append one filesystem-activity event to its session journal (bounded/rotated). `ts` is stamped here
/// (server-side epoch ms) so callers can't skew the log.
#[tauri::command]
async fn audit_record(
    app: tauri::AppHandle,
    session: String,
    kind: String,
    path: String,
    detail: Option<String>,
) -> Result<(), String> {
    let dir = audit_dir(&app)?;
    let ts = to_epoch_ms(SystemTime::now()).unwrap_or(0);
    let event = audit_journal::AuditEvent { ts, session, kind, path, detail };
    tauri::async_runtime::spawn_blocking(move || {
        audit_journal::record(&dir, &event, audit_journal::MAX_EVENTS_PER_SESSION)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// List the session ids that have a persisted journal (most useful sorted; newest-first is the UI's job).
#[tauri::command]
async fn audit_sessions(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    let dir = audit_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || audit_journal::list_sessions(&dir))
        .await
        .map_err(|e| e.to_string())
}

/// Read every event for one past session back (append order; malformed lines skipped).
#[tauri::command]
async fn audit_read(
    app: tauri::AppHandle,
    session: String,
) -> Result<Vec<audit_journal::AuditEvent>, String> {
    let dir = audit_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || audit_journal::read_session(&dir, &session))
        .await
        .map_err(|e| e.to_string())
}

/// Line / word / character / byte counts for a text file (CPE-414). Lines follow `str::lines`
/// (a final unterminated line still counts); words are whitespace-separated; characters are Unicode
/// scalar values. Capped so analysing a file stays predictable; a non-UTF-8 (binary) file, a
/// directory, or an over-cap file is an `Err`. Opt-in from the UI, never automatic.
/// Model lives in `cpe_server::text_stats` (CPE-815); this is a thin `spawn_blocking` dispatcher.
#[tauri::command]
async fn text_stats(path: String) -> Result<cpe_server::text_stats::TextStats, String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::text_stats::compute(&path))
        .await.map_err(|e| e.to_string())?
}

/// Whether two files have identical content (CPE-418). Different sizes short-circuit to `false`;
/// otherwise the bytes are streamed and compared with an early exit on the first difference — cheaper
/// and collision-free versus hashing both. A directory or unreadable path is an `Err`, never a panic.
/// Model lives in `cpe_server::compare` (CPE-815); this is a thin `spawn_blocking` dispatcher.
#[tauri::command]
async fn files_identical(a: String, b: String) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::compare::files_identical(&a, &b))
        .await.map_err(|e| e.to_string())?
}

/// Search text files under `root` for lines containing `query` (CPE-416). Model lives in
/// `cpe_server::content_search` (CPE-815); this is a thin `spawn_blocking` dispatcher.
#[tauri::command]
async fn search_file_contents(
    root: String,
    query: String,
    case_sensitive: bool,
) -> Result<cpe_server::content_search::ContentSearchResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        cpe_server::content_search::search_file_contents(&root, &query, case_sensitive)
    })
    .await
    .map_err(|e| e.to_string())?
}

// The filename-search domain (types, glob/brace matching, and the shared streaming walker) now lives in
// `cpe_server::name_search` (CPE-815); the two commands below dispatch to it.

/// Find files/folders under `root` whose name matches `query`. Model lives in
/// `cpe_server::name_search` (CPE-815); this is a thin `spawn_blocking` dispatcher.
#[tauri::command]
async fn find_files_by_name(
    root: String,
    query: String,
) -> Result<cpe_server::name_search::NameSearchResult, String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::name_search::find_files_by_name(&root, &query))
        .await.map_err(|e| e.to_string())?
}

/// Streaming variant of `find_files_by_name` (CPE-666, epic CPE-662): pushes batches of hits over an IPC
/// channel as the tree is walked. The transport (`ipc::Channel`) stays in this adapter; the walk itself
/// is the shared `cpe_server::name_search::walk_name_matches` (CPE-815). The returned result carries the
/// final `dirs_scanned` + `truncated` with empty `matches` (those were streamed).
#[tauri::command]
fn find_files_by_name_stream(
    root: String,
    query: String,
    on_match: tauri::ipc::Channel<Vec<cpe_server::name_search::NameMatch>>,
) -> Result<cpe_server::name_search::NameSearchResult, String> {
    let stats = cpe_server::name_search::walk_name_matches(
        &root,
        &query,
        cpe_server::name_search::NAME_SEARCH_BATCH,
        |batch| {
            let _ = on_match.send(batch);
            std::ops::ControlFlow::Continue(())
        },
    )?;
    Ok(cpe_server::name_search::NameSearchResult {
        matches: Vec::new(),
        dirs_scanned: stats.dirs_scanned,
        truncated: stats.truncated,
    })
}

/// Find duplicate files under `root` (CPE-420) — size-then-hash two-pass scan. Model lives in
/// `cpe_server::duplicates` (CPE-815); this is a thin `spawn_blocking` dispatcher.
#[tauri::command]
async fn find_duplicates(root: String) -> Result<cpe_server::duplicates::DupResult, String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::duplicates::find_duplicates(&root))
        .await.map_err(|e| e.to_string())?
}

/// Read `settings.json` from `dir`, returning `{}` when it's absent or
/// unreadable so the frontend always starts from a valid document.
/// Read the single on-disk settings file (`settings.json` in the app config dir). Returns `{}` when it
/// doesn't exist yet, so the frontend can start from defaults on a fresh install (CPE-226). The model
/// lives in `cpe_server::settings` (CPE-815); this is a thin dispatcher.
#[tauri::command]
fn read_settings(app: tauri::AppHandle) -> Result<String, String> {
    cpe_server::settings::load(&server_ctx::TauriCtx::new(&app))
}

/// Write the single on-disk settings file, creating the config dir if needed (CPE-226). `contents` is
/// the full settings JSON document.
#[tauri::command]
fn write_settings(app: tauri::AppHandle, contents: String) -> Result<(), String> {
    cpe_server::settings::save(&server_ctx::TauriCtx::new(&app), &contents)
}

// ---- Tag store (CPE-635, epic CPE-614) -------------------------------------------------------
// The model + persistence now live in the pure Server crate (`cpe_server::tags`, CPE-815); the
// commands below are one-line dispatchers that build a `TauriCtx` and call into it.
use cpe_server::tags::TagStore;

/// The whole tag store (path → {tags,label}); `{}` on a fresh install.
#[tauri::command]
fn load_tags(app: tauri::AppHandle) -> Result<TagStore, String> {
    cpe_server::tags::load(&server_ctx::TauriCtx::new(&app))
}

/// Replace one path's tags + label and persist. Returns the updated whole store.
#[tauri::command]
fn set_tags(app: tauri::AppHandle, path: String, tags: Vec<String>, label: String) -> Result<TagStore, String> {
    cpe_server::tags::set(&server_ctx::TauriCtx::new(&app), &path, tags, label)
}

/// Every tag with its usage count (most-used first).
#[tauri::command]
fn tag_counts(app: tauri::AppHandle) -> Result<Vec<(String, usize)>, String> {
    cpe_server::tags::counts(&server_ctx::TauriCtx::new(&app))
}

/// Rename a tag across every path (CPE-646); an empty `new` deletes it. Returns the updated store.
#[tauri::command]
fn rename_tag(app: tauri::AppHandle, old: String, new: String) -> Result<TagStore, String> {
    cpe_server::tags::rename_tag(&server_ctx::TauriCtx::new(&app), &old, &new)
}

/// Remove a tag from every path (CPE-646). Returns the updated store.
#[tauri::command]
fn delete_tag(app: tauri::AppHandle, tag: String) -> Result<TagStore, String> {
    cpe_server::tags::delete_tag(&server_ctx::TauriCtx::new(&app), &tag)
}

/// Re-key a path's tags/label after an in-app rename or move (CPE-650), so tags follow the file.
/// Returns the updated store. A no-op when the old path had no tags.
#[tauri::command]
fn retag_path(app: tauri::AppHandle, from: String, to: String) -> Result<TagStore, String> {
    cpe_server::tags::retag(&server_ctx::TauriCtx::new(&app), &from, &to)
}

/// Import a previously-exported tag store (JSON), merged into the current one (CPE-640). Non-
/// destructive: existing tags are kept, imported tags unioned in. Returns the merged store. (Export
/// is just `load_tags` + `JSON.stringify` on the frontend.)
#[tauri::command]
fn import_tags(app: tauri::AppHandle, json: String) -> Result<TagStore, String> {
    cpe_server::tags::import(&server_ctx::TauriCtx::new(&app), &json)
}

/// Return the user's home directory.
#[tauri::command]
async fn home_dir() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(home_dir_impl)
        .await.map_err(|e| e.to_string())?
}

fn home_dir_impl() -> Result<String, String> {
    dirs_home()
        .map(|p| p.to_string_lossy().to_string())
        .ok_or_else(|| "could not determine home directory".to_string())
}

/// Return the parent of `path`, or null if already at a root.
#[tauri::command]
async fn parent_dir(path: String) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || parent_dir_impl(path))
        .await.unwrap()
}

fn parent_dir_impl(path: String) -> Option<String> {
    Path::new(&path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
}

/// Available drives (Windows) or filesystem roots (Unix).
#[tauri::command]
async fn list_drives() -> Vec<Place> {
    tauri::async_runtime::spawn_blocking(list_drives_impl)
        .await.unwrap()
}

fn list_drives_impl() -> Vec<Place> {
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
// Async so `disk_space` on a slow/network drive runs off the main thread (CPE-760).
#[tauri::command]
async fn disk_space(path: String) -> Result<DiskSpace, String> {
    tauri::async_runtime::spawn_blocking(move || disk_space_impl(path))
        .await
        .map_err(|e| e.to_string())?
}

fn disk_space_impl(path: String) -> Result<DiskSpace, String> {
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
async fn special_folders() -> Vec<Place> {
    tauri::async_runtime::spawn_blocking(special_folders_impl)
        .await.unwrap()
}

fn special_folders_impl() -> Vec<Place> {
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
async fn open_external(path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || open_external_impl(path))
        .await.map_err(|e| e.to_string())?
}

fn open_external_impl(path: String) -> Result<(), String> {
    // On Windows this hands `path` to `cmd /C start`, which re-parses its arguments — a `"` in the
    // path could break out of the quoting and inject a command. Real Windows paths can't contain `"`
    // (it's a reserved character) and neither URLs nor paths need raw control characters, so refuse
    // them: this closes the injection surface without changing how anything legitimate opens (CPE-629).
    if path.contains('"') || path.chars().any(char::is_control) {
        return Err("refusing to open a path with invalid characters".into());
    }
    #[cfg(target_os = "windows")]
    let spawned = quiet_command("cmd")
        .args(["/C", "start", "", &path])
        .spawn();
    #[cfg(target_os = "macos")]
    let spawned = quiet_command("open").arg(&path).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let spawned = quiet_command("xdg-open").arg(&path).spawn();

    spawned.map(|_| ()).map_err(|e| e.to_string())
}

/// Open the platform's terminal with its working directory set to `path`
/// (CPE-253). Windows prefers Windows Terminal and falls back to a fresh cmd
/// window; macOS uses Terminal.app; Linux tries the common emulators in turn.
#[tauri::command]
async fn open_terminal(path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || open_terminal_impl(path))
        .await.map_err(|e| e.to_string())?
}

fn open_terminal_impl(path: String) -> Result<(), String> {
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

// ---- User-defined command exec (CPE-783, epic CPE-711) --------------------------------------------
// Runs a user's resolved command line (built by userCommands.resolveCommand / cmdTemplate, CPE-781) and
// returns its captured output + exit code. Executed through the platform shell (`cmd /C` on Windows,
// `sh -c` elsewhere) so a normal command string with pipes/quotes works as the user expects. This is an
// external-process launch, so the frontend MUST confirm the resolved command with the user BEFORE calling
// (the ticket's hard requirement) — this backend is the thin, gated executor, never invoked implicitly.
// Output is capped per stream so a chatty command can't balloon memory.

/// Captured result of a user command run.
#[derive(serde::Serialize)]
struct CommandOutput {
    stdout: String,
    stderr: String,
    /// Process exit code, or `None` if it was terminated by a signal.
    code: Option<i32>,
    /// True when either stream was truncated at the cap.
    truncated: bool,
}

/// Max bytes captured per stream (stdout/stderr) before truncation.
const COMMAND_OUTPUT_CAP: usize = 1024 * 1024;

/// Truncate raw bytes to `cap` then lossily decode (a split multibyte at the cut becomes U+FFFD, never a
/// panic). Returns the string and whether it was truncated.
fn capped_string(mut bytes: Vec<u8>, cap: usize) -> (String, bool) {
    let truncated = bytes.len() > cap;
    if truncated {
        bytes.truncate(cap);
    }
    (String::from_utf8_lossy(&bytes).into_owned(), truncated)
}

/// Run a resolved user command line through the platform shell and capture its output (CPE-783). The
/// frontend confirms the command with the user first — see the module comment. Async per the commands rule.
#[tauri::command]
async fn run_command(command: String, cwd: Option<String>) -> Result<CommandOutput, String> {
    tauri::async_runtime::spawn_blocking(move || run_command_impl(command, cwd))
        .await
        .map_err(|e| e.to_string())?
}

fn run_command_impl(command: String, cwd: Option<String>) -> Result<CommandOutput, String> {
    if command.trim().is_empty() {
        return Err("Command is empty".to_string());
    }
    #[cfg(windows)]
    let mut cmd = {
        let mut c = quiet_command("cmd");
        c.args(["/C", &command]);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = quiet_command("sh");
        c.args(["-c", &command]);
        c
    };
    if let Some(dir) = cwd.as_deref().filter(|d| !d.is_empty()) {
        cmd.current_dir(dir);
    }
    let output = cmd.output().map_err(|e| e.to_string())?;
    let (stdout, o_trunc) = capped_string(output.stdout, COMMAND_OUTPUT_CAP);
    let (stderr, e_trunc) = capped_string(output.stderr, COMMAND_OUTPUT_CAP);
    Ok(CommandOutput {
        stdout,
        stderr,
        code: output.status.code(),
        truncated: o_trunc || e_trunc,
    })
}

/// Extract a single entry from a ZIP to a temp file and return its path, so it
/// can be opened with its default app while browsing inside the archive
/// (CPE-242). Read-only: the temp copy is what opens, not the archived bytes.
#[tauri::command]
async fn extract_archive_entry(zip: String, inner: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::archive::extract_archive_entry(&zip, &inner))
        .await.map_err(|e| e.to_string())?
}

// Archive creation & extraction (CPE-251/252/242) now live in `cpe_server::archive` (CPE-822); the
// commands below are thin dispatchers.

/// Pack the given files/folders into a new deflated `.zip` at `dest` (CPE-251). Model lives in
/// `cpe_server::archive` (CPE-822); thin dispatcher.
#[tauri::command]
async fn compress_to_zip(paths: Vec<String>, dest: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::archive::compress_to_zip(&paths, &dest))
        .await.map_err(|e| e.to_string())?
}

/// Extract an archive into `dest` (CPE-252), guarded against zip-slip for every format. Model lives in
/// `cpe_server::archive` (CPE-822); thin dispatcher.
#[tauri::command]
async fn extract_archive(path: String, dest: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::archive::extract_archive(&path, &dest))
        .await.map_err(|e| e.to_string())?
}

/// Run an executable with elevation (CPE-241). On Windows this uses
/// `Start-Process -Verb RunAs`, which shows the UAC prompt. On other platforms
/// there is no standard per-launch elevation prompt, so it runs normally.
#[tauri::command]
async fn run_as_admin(path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || run_as_admin_impl(path))
        .await.map_err(|e| e.to_string())?
}

fn run_as_admin_impl(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        // Single-quote the path for PowerShell; escape any embedded quote.
        let escaped = path.replace('\'', "''");
        quiet_command("powershell")
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
        open_external_impl(path)
    }
}

/// Read a repo's `.git/config` and return its origin remote as a browsable https
/// URL (folder-context plugins, CPE-235). A cheap single file read; returns None
/// if the folder isn't a repo or has no remote.
#[tauri::command]
async fn git_remote_url(path: String) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || git_remote_url_impl(path))
        .await.unwrap()
}

fn git_remote_url_impl(path: String) -> Option<String> {
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
        // The Agent Board sidecar (CPE-850): the out-of-process Kanban over Tickets/. Bundled +
        // registered behind the feature so it appears in the sidecar manager alongside the others.
        manifest.join("../sidecar/agent-board"),
        PathBuf::from("sidecar/agent-board"),
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
    server_ctx::TauriCtx::new(app)
        .app_config_dir()
        .map(|c| c.join("sidecars"))
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

/// Whether a sidecar `id`'s launchable binary actually resolves — the "missing binary" health signal
/// (CPE-863). Generic over id (the exe name equals the sidecar id): checks the bundled resource copy,
/// then the dev source-tree targets, mirroring the per-sidecar resolvers. Returns the path if found.
#[cfg(feature = "sidecar-platform")]
fn resolve_sidecar_bin(app: &tauri::AppHandle, id: &str) -> Option<PathBuf> {
    use tauri::Manager;
    let exe = if cfg!(windows) { format!("{id}.exe") } else { id.to_string() };
    if let Ok(resource) = app.path().resource_dir() {
        let p = resource.join("sidecars").join(&exe);
        if p.exists() {
            return Some(p);
        }
    }
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    for profile in ["debug", "release"] {
        for base in [
            manifest.join(format!("../sidecar/{id}/target")),
            PathBuf::from(format!("sidecar/{id}/target")),
        ] {
            let p = base.join(profile).join(&exe);
            if p.exists() {
                return Some(p);
            }
        }
    }
    None
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
    /// Whether the sidecar's launchable binary actually resolves (CPE-863) — false = missing binary.
    binary_ok: bool,
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
                binary_ok: resolve_sidecar_bin(&app, &m.id).is_some(),
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

/// What a repair attempt did, for the management panel (CPE-863). `binary_ok` is the re-checked binary
/// presence after the repair; `actions` are the plain-language steps taken.
#[cfg(feature = "sidecar-platform")]
#[derive(serde::Serialize)]
struct SidecarRepair {
    id: String,
    binary_ok: bool,
    actions: Vec<String>,
}

/// Best-effort self-heal for a sidecar (CPE-863, epic CPE-862 L1): reap orphan session-daemons that may
/// be holding the binary/port, drop a wedged connection, and clear the stored last-error so a stuck
/// sidecar can start clean — then re-check whether its binary resolves. A genuinely missing binary can't
/// be restored here (that's L2); it is reported honestly via `binary_ok = false` so the UI can say so.
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn sidecar_repair(
    app: tauri::AppHandle,
    id: String,
    state: tauri::State<AiConsoleState>,
) -> Result<SidecarRepair, String> {
    let mut actions = Vec::new();
    if id == "ai-console" {
        // Orphan `--session-daemon` processes survive the UI and file-lock the binary (CPE-483); reaping
        // them clears the most common "stale / won't update / won't start" cause.
        reap_orphan_session_daemons_on_startup(&app);
        actions.push("reaped orphan session daemons".into());
        if let Ok(mut g) = state.conn.lock() {
            if g.is_some() {
                *g = None;
                actions.push("dropped a wedged connection".into());
            }
        }
        if let Ok(mut g) = state.url.lock() {
            *g = None;
        }
        state.clear_error();
        actions.push("cleared the last error".into());
    }
    let binary_ok = resolve_sidecar_bin(&app, &id).is_some();
    if !binary_ok {
        actions.push("binary is missing — reinstall required (auto-restore is coming in L2)".into());
    }
    Ok(SidecarRepair { id, binary_ok, actions })
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
    server_ctx::TauriCtx::new(app)
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

/// Read a file as UTF-8 text for shadowing (CPE-743), or `None` if it isn't a suitable text file:
/// not a regular file, larger than `cap` bytes, unreadable, or not valid UTF-8 (binary). Cheap
/// bail-outs first (metadata is one stat) so the pump never slurps a huge or binary file.
#[cfg(feature = "sidecar-platform")]
fn read_text_capped(path: &str, cap: usize) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() as usize > cap {
        return None;
    }
    String::from_utf8(std::fs::read(path).ok()?).ok()
}

/// Coalescing emitter: fold raw watcher events per-path over a short window and flush batches to
/// the frontend as `ai-console://fs-activity`. Bounded so a big refactor can't flood the UI — the
/// pending set is capped and flushed early when full. Ends when the channel closes (watcher dropped).
///
/// Alongside the activity batch it maintains a [`agent_shadow::ShadowStore`] and, at each flush,
/// pairs every created/modified path with its cached "before" content, emitting `{path, before,
/// after}` records on `ai-console://fs-diff` (CPE-743) so the frontend can show what each write
/// changed (Edit Diff Peek, epic CPE-727). The store lives for the pump's lifetime and is freed when
/// the watcher is dropped — off means off.
#[cfg(feature = "sidecar-platform")]
fn fs_activity_pump(
    app: tauri::AppHandle,
    rx: std::sync::mpsc::Receiver<notify::Result<notify::Event>>,
) {
    use std::collections::HashMap;
    use std::sync::mpsc::RecvTimeoutError;
    use std::time::{Duration, Instant};

    const FLUSH: Duration = Duration::from_millis(200);
    const CAP: usize = 500;
    let mut pending: HashMap<String, &'static str> = HashMap::new();
    let mut shadow = agent_shadow::ShadowStore::new();
    let mut last_flush = Instant::now();

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
                    flush_fs_batch(&app, &mut pending, &mut shadow);
                    last_flush = Instant::now();
                }
            }
            Ok(Err(_)) => {} // a watch error — ignore, keep pumping
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                flush_fs_batch(&app, &mut pending, &mut shadow);
                break;
            }
        }
        if last_flush.elapsed() >= FLUSH {
            flush_fs_batch(&app, &mut pending, &mut shadow);
            last_flush = Instant::now();
        }
    }
}

/// Flush the coalesced window: emit the `fs-activity` batch, and — for created/modified paths — read
/// their current text, pair it with the shadow baseline, and emit any `fs-diff` records (CPE-743). A
/// removed/renamed-away path (or one that became binary/oversized) drops its baseline. Drains
/// `pending`.
#[cfg(feature = "sidecar-platform")]
fn flush_fs_batch(
    app: &tauri::AppHandle,
    pending: &mut std::collections::HashMap<String, &'static str>,
    shadow: &mut agent_shadow::ShadowStore,
) {
    use serde_json::json;
    use tauri::Emitter;

    if pending.is_empty() {
        return;
    }
    let mut activity = Vec::with_capacity(pending.len());
    let mut diffs = Vec::new();
    for (path, kind) in pending.drain() {
        // Diff bookkeeping first (borrows `path`), then move `path` into the activity item below.
        let record = match kind {
            "created" | "modified" => match read_text_capped(&path, agent_shadow::MAX_FILE_BYTES) {
                Some(content) if kind == "created" => shadow.on_created(&path, content),
                Some(content) => shadow.on_modified(&path, content),
                None => {
                    // Gone, binary, or oversized: drop any stale baseline, emit no diff.
                    shadow.forget(&path);
                    None
                }
            },
            "removed" => {
                shadow.forget(&path);
                None
            }
            _ => None,
        };
        if let Some(r) = record {
            diffs.push(json!({ "path": r.path, "before": r.before, "after": r.after }));
        }
        activity.push(json!({ "kind": kind, "path": path }));
    }
    let _ = app.emit("ai-console://fs-activity", activity);
    if !diffs.is_empty() {
        let _ = app.emit("ai-console://fs-diff", diffs);
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

// ---- Watched-folder rules: live folder watcher (CPE-794, epic CPE-734) ----------------------------
// A separate notify watcher over the user's configured folders that emits coarse `folder-watch` events
// {path, kind} so the frontend can run watch rules (planForEntry → run_watch_actions) on a landed file.
// Sidecar-gated for the same reason as Agent Watch — the plain explorer pulls no watcher machinery. The
// executor (`run_watch_actions`) and rule matching stay in the plain build; only the live *trigger* is here.

#[cfg(feature = "sidecar-platform")]
#[derive(Default)]
struct FolderWatchState {
    current: std::sync::Mutex<Option<notify::RecommendedWatcher>>,
}

/// Coalescing emitter for the folder watcher: fold raw events per-path over a short window and flush
/// `folder-watch` batches of `{path, kind}` to the frontend. Ends when the channel closes (watcher dropped).
#[cfg(feature = "sidecar-platform")]
fn folder_watch_pump(app: tauri::AppHandle, rx: std::sync::mpsc::Receiver<notify::Result<notify::Event>>) {
    use std::collections::HashMap;
    use std::sync::mpsc::RecvTimeoutError;
    use std::time::{Duration, Instant};
    use serde_json::json;
    use tauri::Emitter;

    const FLUSH: Duration = Duration::from_millis(250);
    let mut pending: HashMap<String, &'static str> = HashMap::new();
    let mut last_flush = Instant::now();
    let flush = |app: &tauri::AppHandle, pending: &mut HashMap<String, &'static str>| {
        if pending.is_empty() {
            return;
        }
        let batch: Vec<_> = pending
            .drain()
            .map(|(path, kind)| json!({ "path": path, "kind": kind }))
            .collect();
        let _ = app.emit("folder-watch", batch);
    };

    loop {
        match rx.recv_timeout(FLUSH) {
            Ok(Ok(event)) => {
                if let Some(kind) = classify_fs_event(&event.kind) {
                    for p in event.paths {
                        let path = p.to_string_lossy().into_owned();
                        let slot = pending.entry(path).or_insert(kind);
                        if kind == "removed" || *slot != "removed" {
                            *slot = kind;
                        }
                    }
                }
            }
            Ok(Err(_)) => {}
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

/// Start (or replace) the watched-folder watcher over `paths` (CPE-794). Missing folders are skipped;
/// an empty/all-missing set is a no-op stop. Non-recursive-safe: each folder is watched recursively.
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn folder_watch_start(
    app: tauri::AppHandle,
    state: tauri::State<FolderWatchState>,
    paths: Vec<String>,
) -> Result<usize, String> {
    use notify::{RecursiveMode, Watcher};
    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |res| {
        let _ = tx.send(res);
    })
    .map_err(|e| e.to_string())?;
    let mut watched = 0usize;
    for p in &paths {
        if std::path::Path::new(p).is_dir()
            && watcher.watch(std::path::Path::new(p), RecursiveMode::Recursive).is_ok()
        {
            watched += 1;
        }
    }
    if watched == 0 {
        *state.current.lock().unwrap() = None; // nothing to watch → ensure stopped
        return Ok(0);
    }
    std::thread::spawn(move || folder_watch_pump(app, rx));
    *state.current.lock().unwrap() = Some(watcher);
    Ok(watched)
}

/// Stop the watched-folder watcher (CPE-794). Idempotent.
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn folder_watch_stop(state: tauri::State<FolderWatchState>) {
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

/// Resolve the Agent Board sidecar binary (CPE-853): `CPE_AGENTBOARD_BIN`, then the bundled
/// `sidecars/agent-board[.exe]` resource, then a dev fallback next to this crate. Mirrors
/// `resolve_ai_console_bin`.
#[cfg(feature = "sidecar-platform")]
fn resolve_agent_board_bin(app: &tauri::AppHandle) -> Result<String, String> {
    use tauri::Manager;
    let exe = if cfg!(windows) { "agent-board.exe" } else { "agent-board" };

    if let Ok(p) = std::env::var("CPE_AGENTBOARD_BIN") {
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
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    for profile in ["debug", "release"] {
        for base in [manifest.join("../sidecar/agent-board/target"), PathBuf::from("sidecar/agent-board/target")] {
            let p = base.join(profile).join(exe);
            if p.exists() {
                return Ok(p.to_string_lossy().into_owned());
            }
        }
    }
    Err(format!("agent-board binary ('{exe}') not found"))
}

/// Spawn the Agent Board sidecar, complete the handshake, and return the URL of the Kanban UI it serves
/// so the frontend can frame it in a window (CPE-853, epic CPE-850). The board reads `Tickets/` under
/// `root` (passed as `CPE_BOARD_ROOT`; falls back to the sidecar's own cwd when absent). The window
/// singleton (by label) prevents duplicate launches, so this deliberately keeps the connection alive on a
/// detached servicing thread rather than a managed reuse state. Non-fatal: returns an error string the UI
/// surfaces.
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
fn sidecar_start_agent_board(app: tauri::AppHandle, root: Option<String>) -> Result<String, String> {
    use sidecar_contract::{Event, Message, CONTRACT_VERSION};
    use sidecar_host::conformance::SidecarChannel; // brings `.recv()` into scope
    use sidecar_host::supervisor::{handshake, spawn_process_with_env};

    if !sidecar_host::enablement::EnablementStore::load(&consent_dir(&app)?).is_enabled("agent-board") {
        return Err("the Agent Board sidecar is disabled".to_string());
    }

    let bin = resolve_agent_board_bin(&app)?;
    let root = root.unwrap_or_default();
    let env: Vec<(&str, &str)> = if root.is_empty() { vec![] } else { vec![("CPE_BOARD_ROOT", root.as_str())] };

    let mut conn = spawn_process_with_env(&bin, &[], &env).map_err(|e| format!("spawn failed: {e}"))?;
    let token = conn.launch_token().to_string();
    let consented = sidecar_host::consent::ConsentStore::load(&consent_dir(&app)?).granted("agent-board");
    handshake(&mut conn, CONTRACT_VERSION, &consented, Some(&token))
        .map_err(|e| format!("handshake failed: {e:?}"))?;

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
    let url = url.ok_or_else(|| "the Agent Board sidecar did not announce a UI".to_string())?;

    // Keep the connection alive for the sidecar's lifetime on a detached thread (dropping it would close
    // the sidecar's stdin and stop it serving). The board makes no host requests, so we just drain frames
    // until it exits.
    std::thread::spawn(move || {
        while conn.recv().is_ok() {}
    });

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
async fn forge_browse(
    provider: String,
    repo: String,
    path: Option<String>,
    token: Option<String>,
) -> Result<Vec<forge_egress::RepoEntry>, String> {
    tauri::async_runtime::spawn_blocking(move || forge_browse_impl(provider, repo, path, token))
        .await.map_err(|e| e.to_string())?
}

#[cfg(feature = "sidecar-platform")]
fn forge_browse_impl(
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
async fn forge_clone(
    provider: String,
    repo: String,
    target_dir: String,
    token: Option<String>,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || forge_clone_impl(provider, repo, target_dir, token))
        .await.map_err(|e| e.to_string())?
}

#[cfg(feature = "sidecar-platform")]
fn forge_clone_impl(
    provider: String,
    repo: String,
    target_dir: String,
    token: Option<String>,
) -> Result<String, String> {
    let args = build_git_clone(&provider, &repo, &target_dir, token.as_deref())?;
    let output = quiet_command("git")
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
    let dir = server_ctx::TauriCtx::new(app)
        .app_data_dir()
        .map_err(|_| "no app data dir".to_string())?;
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
    let output = quiet_command("git")
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
async fn forge_set_token(provider: String, token: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || forge_set_token_impl(provider, token))
        .await.map_err(|e| e.to_string())?
}

#[cfg(feature = "sidecar-platform")]
fn forge_set_token_impl(provider: String, token: String) -> Result<(), String> {
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
async fn forge_get_token(provider: String) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || forge_get_token_impl(provider))
        .await.map_err(|e| e.to_string())?
}

#[cfg(feature = "sidecar-platform")]
fn forge_get_token_impl(provider: String) -> Result<Option<String>, String> {
    use sidecar_host::providers::secrets::{KeyringBackend, SecretBackend};
    KeyringBackend.get(FORGE_TOKEN_SERVICE, &provider)
}

/// Forget a provider's stored forge token (CPE-439).
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
async fn forge_delete_token(provider: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || forge_delete_token_impl(provider))
        .await.map_err(|e| e.to_string())?
}

#[cfg(feature = "sidecar-platform")]
fn forge_delete_token_impl(provider: String) -> Result<(), String> {
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
// Async so a slow `git status` (e.g. a repo on a slow/network drive) runs on a blocking thread instead of
// freezing the main thread and every other command queued behind it (CPE-760).
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
async fn forge_repo_status(path: String, on_diverge: Option<String>) -> RepoSyncStatus {
    tauri::async_runtime::spawn_blocking(move || forge_repo_status_impl(path, on_diverge))
        .await
        .unwrap_or_default()
}

#[cfg(feature = "sidecar-platform")]
fn forge_repo_status_impl(path: String, on_diverge: Option<String>) -> RepoSyncStatus {
    use repos::SyncAction;
    let output = quiet_command("git")
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
async fn forge_sync(path: String, action: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || forge_sync_impl(path, action))
        .await.map_err(|e| e.to_string())?
}

#[cfg(feature = "sidecar-platform")]
fn forge_sync_impl(path: String, action: String) -> Result<String, String> {
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
    let out = quiet_command("git").args(&args).output().map_err(|e| e.to_string())?;
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
async fn forge_conflict_state(path: String) -> ConflictState {
    tauri::async_runtime::spawn_blocking(move || forge_conflict_state_impl(path))
        .await.unwrap()
}

#[cfg(feature = "sidecar-platform")]
fn forge_conflict_state_impl(path: String) -> ConflictState {
    let out = quiet_command("git")
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
    let out = quiet_command("git")
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
async fn forge_conflict_versions(path: String, file: String) -> ConflictVersions {
    tauri::async_runtime::spawn_blocking(move || forge_conflict_versions_impl(path, file))
        .await.unwrap()
}

#[cfg(feature = "sidecar-platform")]
fn forge_conflict_versions_impl(path: String, file: String) -> ConflictVersions {
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
async fn forge_resolve_file(path: String, file: String, content: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || forge_resolve_file_impl(path, file, content))
        .await.map_err(|e| e.to_string())?
}

#[cfg(feature = "sidecar-platform")]
fn forge_resolve_file_impl(path: String, file: String, content: String) -> Result<(), String> {
    if !is_safe_repo_relative(&file) {
        return Err("Refusing an unsafe file path.".to_string());
    }
    let full = std::path::Path::new(&path).join(&file);
    std::fs::write(&full, content).map_err(|e| format!("Couldn't write the file: {e}"))?;
    let out = quiet_command("git")
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
async fn forge_conflict_continue(path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || forge_conflict_continue_impl(path))
        .await.map_err(|e| e.to_string())?
}

#[cfg(feature = "sidecar-platform")]
fn forge_conflict_continue_impl(path: String) -> Result<String, String> {
    let op = merge_operation(&path);
    let args: Vec<&str> = match op {
        "rebase" => vec!["-C", &path, "rebase", "--continue"],
        "merge" => vec!["-C", &path, "commit", "--no-edit"],
        _ => return Err("No merge or rebase is in progress.".to_string()),
    };
    let out = quiet_command("git")
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
async fn forge_conflict_abort(path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || forge_conflict_abort_impl(path))
        .await.map_err(|e| e.to_string())?
}

#[cfg(feature = "sidecar-platform")]
fn forge_conflict_abort_impl(path: String) -> Result<String, String> {
    let op = merge_operation(&path);
    let args: Vec<&str> = match op {
        "rebase" => vec!["-C", &path, "rebase", "--abort"],
        "merge" => vec!["-C", &path, "merge", "--abort"],
        _ => return Err("No merge or rebase is in progress.".to_string()),
    };
    let out = quiet_command("git")
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
            // The plugin auto-saves on exit and restores on launch, writing its own
            // `.window-state.json`; Builder::default() uses StateFlags::all(), so
            // maximized state is restored too. `main` is skipped from the automatic
            // on-ready restore because setup() creates that window and restores it
            // explicitly (CPE-608), so restore ordering vs CLI geometry is deterministic.
            .plugin(
                tauri_plugin_window_state::Builder::default()
                    .skip_initial_state("main")
                    .build(),
            );
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
        // The watched-folder rules watcher (CPE-794); empty until the user configures watched folders.
        builder = builder.manage(FolderWatchState::default());
    }

    // Startup setup: create the main window in Rust (CPE-608) so its webview can inject a
    // `Cache-Control: no-store` header — WebView2 otherwise heuristically caches the served frontend, and
    // a cached (unhashed) `index.html` pins the app to a stale JS bundle after an auto-update. Then
    // restore the saved geometry (CPE-228) and apply any CLI window-geometry flags over it (CPE-600).
    // With the platform on, also reap orphaned `ai-console --session-daemon` processes left by a prior run
    // before they can lock the sidecar binary during an update (CPE-483).
    builder = builder.setup(|app| {
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            use tauri::{WebviewUrl, WebviewWindowBuilder};
            use tauri_plugin_window_state::{StateFlags, WindowExt};

            let win = WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
                .title("Cross-Platform Explorer")
                .inner_size(1000.0, 700.0)
                .min_inner_size(600.0, 400.0)
                .on_web_resource_request(|_request, response| {
                    // Local assets, so no-store costs nothing; applied on every response for consistency.
                    response.headers_mut().insert(
                        tauri::http::header::CACHE_CONTROL,
                        tauri::http::HeaderValue::from_static("no-store"),
                    );
                })
                .build()?;

            // `main` is in the window-state plugin's skip_initial_state list, so restore its saved
            // geometry here (deterministic) BEFORE the CLI flags override it — restore then override.
            let _ = win.restore_state(StateFlags::all());
            apply_cli_geometry(app.handle());
        }
        #[cfg(feature = "sidecar-platform")]
        reap_orphan_session_daemons_on_startup(app.handle());
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
            read_file_range,
            file_len,
            set_permissions,
            set_readonly,
            set_file_times,
            set_file_attribute,
            read_attributes,
            write_file_text,
            read_archive_entries,
            read_preview_info,
            data_browser_sources,
            data_browser_page,
            data_browser_query,
            read_image_data_url,
            thumbnail,
            read_settings,
            write_settings,
            load_tags,
            set_tags,
            tag_counts,
            rename_tag,
            delete_tag,
            import_tags,
            retag_path,
            rename_entry,
            delete_to_trash,
            delete_permanent,
            can_restore_from_trash,
            restore_from_trash,
            copy_entries,
            move_entries,
            run_watch_actions,
            start_transfer,
            cancel_transfer,
            move_exact,
            entry_info,
            image_meta,
            list_dir_stream,
            cancel_dir_stream,
            entries_for_paths,
            same_volume,
            find_files_by_name_stream,
            dir_size,
            dir_children_sizes,
            folder_stats,
            hash_file,
            apply_backup_plan,
            apply_backup_plan_stream,
            checksum_folder,
            scan_tree,
            create_symlink,
            create_hard_link,
            drive_type,
            audit_record,
            audit_sessions,
            audit_read,
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
            run_command,
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
            sidecar_repair,
            #[cfg(feature = "sidecar-platform")]
            sidecar_close_session,
            #[cfg(feature = "sidecar-platform")]
            sidecar_set_enabled,
            #[cfg(feature = "sidecar-platform")]
            sidecar_start_ai_console,
            #[cfg(feature = "sidecar-platform")]
            sidecar_start_agent_board,
            #[cfg(feature = "sidecar-platform")]
            sidecar_diagnostics,
            #[cfg(feature = "sidecar-platform")]
            agent_watch_start,
            #[cfg(feature = "sidecar-platform")]
            agent_watch_stop,
            #[cfg(feature = "sidecar-platform")]
            folder_watch_start,
            #[cfg(feature = "sidecar-platform")]
            folder_watch_stop,
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
            parent_dir_impl("/home/user/docs".to_string()),
            Some("/home/user".to_string())
        );
    }

    #[test]
    fn parent_dir_at_root_returns_none() {
        assert_eq!(parent_dir_impl("/".to_string()), None);
    }

    // list_dir + stream_dir_entries walker tests moved with the code to `cpe_server::listing` (CPE-815).

    #[test]
    fn cancel_dir_stream_sets_the_registered_flag() {
        use std::sync::atomic::Ordering;
        let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        dir_stream_registry().lock().unwrap().insert(999_001, flag.clone());
        cancel_dir_stream(999_001);
        assert!(flag.load(Ordering::Relaxed), "cancel should set the stream's flag");
        cancel_dir_stream(999_002); // unknown id is a harmless no-op
        dir_stream_registry().lock().unwrap().remove(&999_001);
    }


    #[test]
    fn home_dir_resolves() {
        assert!(home_dir_impl().is_ok());
    }

    // extension_of / is_hidden tests moved with the code to `cpe_server::model` (CPE-815).

    // epoch-ms conversion tests moved with `to_epoch_ms` to `cpe_server::fsutil` (CPE-815).

    #[test]
    fn list_drives_returns_at_least_one_root() {
        assert!(!list_drives_impl().is_empty(), "there is always at least one root");
    }

    #[test]
    fn disk_space_reports_sensible_free_and_total() {
        // The temp dir always exists on any runner; free must never exceed total (CPE-403).
        let d = disk_space_impl(std::env::temp_dir().to_string_lossy().into_owned()).unwrap();
        assert!(d.total > 0, "a real volume has non-zero capacity");
        assert!(d.free <= d.total, "free ({}) cannot exceed total ({})", d.free, d.total);
    }

    #[test]
    fn special_folders_all_exist_and_are_labelled() {
        for place in special_folders_impl() {
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
        let r = read_file_text_impl(f.to_string_lossy().to_string(), 1024);
        assert_eq!(r.unwrap(), "hello world");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn read_file_text_errors_when_over_the_cap() {
        let d = scratch("read_big");
        let f = d.join("big.txt");
        fs::write(&f, vec![b'x'; 200]).unwrap();
        let r = read_file_text_impl(f.to_string_lossy().to_string(), 100);
        assert!(r.is_err(), "a file over the cap must error, not truncate");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn read_file_range_reads_and_clamps() {
        let d = scratch("read_range");
        let f = d.join("bin.dat");
        fs::write(&f, b"0123456789").unwrap(); // 10 bytes
        let p = f.to_string_lossy().to_string();
        // exact interior range
        assert_eq!(read_file_range_impl(p.clone(), 2, 3).unwrap(), b"234".to_vec());
        // len past EOF clamps to what's left
        assert_eq!(read_file_range_impl(p.clone(), 8, 100).unwrap(), b"89".to_vec());
        // whole file
        assert_eq!(read_file_range_impl(p.clone(), 0, 10).unwrap(), b"0123456789".to_vec());
        // offset AT eof -> empty (not an error)
        assert_eq!(read_file_range_impl(p.clone(), 10, 5).unwrap(), Vec::<u8>::new());
        // offset PAST eof -> empty (not an error)
        assert_eq!(read_file_range_impl(p.clone(), 999, 5).unwrap(), Vec::<u8>::new());
        // zero len -> empty
        assert_eq!(read_file_range_impl(p, 0, 0).unwrap(), Vec::<u8>::new());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn set_readonly_toggles_and_returns_prior() {
        let d = scratch("set_ro");
        let f = d.join("f.txt");
        fs::write(&f, b"x").unwrap();
        let p = f.to_string_lossy().to_string();
        // set read-only; prior was writable (false)
        assert!(!set_readonly_impl(p.clone(), true).unwrap());
        assert!(fs::metadata(&f).unwrap().permissions().readonly());
        // clear it; prior was read-only (true)
        assert!(set_readonly_impl(p, false).unwrap());
        assert!(!fs::metadata(&f).unwrap().permissions().readonly());
        let _ = fs::remove_dir_all(&d);
    }

    // checksum-folder tests moved with the code to `cpe_server::checksum` (CPE-815).

    // link-forge (symlink/hardlink) tests moved with the code to `cpe_server::links` (CPE-815).

    #[cfg(windows)]
    #[test]
    fn drive_type_classifies_system_drive_as_fixed() {
        let cwd = std::env::current_dir().unwrap();
        assert_eq!(drive_type_impl(&cwd.to_string_lossy()), "fixed");
    }

    #[cfg(not(windows))]
    #[test]
    fn drive_type_unix_fallback() {
        assert_eq!(drive_type_impl("/"), "fixed");
    }

    #[cfg(unix)]
    #[test]
    fn set_permissions_chmods_and_returns_prior_mode() {
        use std::os::unix::fs::PermissionsExt;
        let d = scratch("set_perm");
        let f = d.join("f.txt");
        fs::write(&f, b"x").unwrap();
        fs::set_permissions(&f, fs::Permissions::from_mode(0o644)).unwrap();
        let p = f.to_string_lossy().to_string();
        // chmod 600; prior mode returned is 0o644
        assert_eq!(set_permissions_impl(p, 0o600).unwrap(), 0o644);
        assert_eq!(fs::metadata(&f).unwrap().permissions().mode() & 0o777, 0o600);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn set_file_times_sets_modified_and_returns_prior() {
        let d = scratch("set_times");
        let f = d.join("f.txt");
        fs::write(&f, b"x").unwrap();
        let p = f.to_string_lossy().to_string();
        let target_ms = 1_600_000_000_000i64; // 2020-09-13
        let (prior_m, _prior_a) = set_file_times_impl(p, Some(target_ms), None).unwrap();
        assert!(prior_m > 0, "prior modified time should be the file's original mtime");
        let meta = fs::metadata(&f).unwrap();
        let now_m = ms_from_ft(filetime::FileTime::from_last_modification_time(&meta));
        // allow slack for filesystem timestamp resolution
        assert!((now_m - target_ms).abs() < 2000, "modified time not set (got {now_m})");
        let _ = fs::remove_dir_all(&d);
    }

    #[cfg(windows)]
    #[test]
    fn set_file_attribute_toggles_hidden_and_returns_prior() {
        let d = scratch("set_attr");
        let f = d.join("f.txt");
        fs::write(&f, b"x").unwrap();
        let p = f.to_string_lossy().to_string();
        // set hidden; prior was not hidden
        assert!(!set_file_attribute_impl(p.clone(), "hidden".to_string(), true).unwrap());
        // clear hidden; prior was hidden
        assert!(set_file_attribute_impl(p.clone(), "hidden".to_string(), false).unwrap());
        // unknown attribute errors cleanly
        assert!(set_file_attribute_impl(p, "bogus".to_string(), true).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn read_file_text_errors_on_invalid_utf8() {
        let d = scratch("read_bin");
        let f = d.join("blob.bin");
        fs::write(&f, [0xff, 0xfe, 0x00, 0x01]).unwrap();
        let r = read_file_text_impl(f.to_string_lossy().to_string(), 1024);
        assert!(r.is_err(), "non-UTF-8 content must error");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn write_file_text_replaces_contents() {
        let d = scratch("write_txt");
        let f = d.join("note.txt");
        fs::write(&f, b"old text").unwrap();
        let n = write_file_text_impl(f.to_string_lossy().to_string(), "brand new".to_string()).unwrap();
        assert_eq!(n, 9);
        assert_eq!(fs::read_to_string(&f).unwrap(), "brand new");
        let _ = fs::remove_dir_all(&d);
    }

    // archive listing/create/extract tests moved with the code to `cpe_server::archive` (CPE-815/822).

    #[test]
    fn read_archive_entries_errors_on_a_non_zip() {
        let d = scratch("zip_bad");
        let f = d.join("notazip.zip");
        fs::write(&f, b"this is not a zip file").unwrap();
        assert!(cpe_server::archive::read_archive_entries(&f.to_string_lossy()).is_err());
        let _ = fs::remove_dir_all(&d);
    }


    #[test]
    fn open_external_refuses_shell_injection_characters() {
        // A `"` (impossible in a real Windows path) or a control char is refused before reaching the
        // shell — these are the only characters that could break `cmd /C start`'s quoting. We only
        // assert the rejection path; a normal path would actually launch the OS opener (a side effect).
        assert!(open_external_impl("a\" & calc.exe & \"b".into()).is_err());
        assert!(open_external_impl("x\ny".into()).is_err());
        assert!(open_external_impl("tab\there".into()).is_err());
    }


    #[test]
    fn hex_dump_formats_offsets_and_ascii() {
        let d = scratch("hex");
        let f = d.join("blob.bin");
        fs::write(&f, b"AB\x00\xff").unwrap();
        let dump = cpe_server::binary_preview::hex_dump(&f.to_string_lossy(), 64).unwrap();
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
        let wat = cpe_server::binary_preview::wasm_info(&f.to_string_lossy(), 4096).unwrap();
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
        let info = cpe_server::binary_preview::torrent_info(&f.to_string_lossy()).unwrap();
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
        assert!(cpe_server::binary_preview::pe_info(&f.to_string_lossy()).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[cfg(windows)]
    #[test]
    fn pe_info_parses_a_real_windows_binary() {
        // The test executable itself is a PE on Windows.
        let exe = std::env::current_exe().unwrap();
        let info = cpe_server::binary_preview::pe_info(&exe.to_string_lossy()).unwrap();
        assert!(info.contains("PE32"), "identifies the PE image");
        assert!(info.contains("Sections:"), "lists sections");
    }

    // doc-text (rtf/docx/odt/epub) tests moved with the code to `cpe_server::doc_text` (CPE-815).

    // structured-data preview tests moved with the code to `cpe_server::data_preview` (CPE-815).

    #[test]
    fn windows_volume_root_extracts_drive_and_unc() {
        assert_eq!(windows_volume_root(r"C:\Users\a\file.txt"), Some("C:".into()));
        assert_eq!(windows_volume_root("c:/users/a"), Some("C:".into())); // forward slashes + lowercase
        assert_eq!(windows_volume_root(r"D:\"), Some("D:".into()));
        assert_eq!(windows_volume_root(r"\\server\share\dir\f"), Some(r"\\server\share".to_lowercase()));
        assert_eq!(windows_volume_root(r"\\Server\Share"), Some(r"\\server\share".into()));
        assert_eq!(windows_volume_root("relative/path"), None);
        assert_eq!(windows_volume_root(r"\\server"), None); // share missing
    }

    #[test]
    fn same_volume_true_for_a_path_and_itself() {
        // Two paths under the same scratch dir are on one volume on every platform.
        let d = scratch("samevol");
        fs::write(d.join("a.txt"), b"x").unwrap();
        fs::write(d.join("b.txt"), b"y").unwrap();
        let a = d.join("a.txt").to_string_lossy().to_string();
        let b = d.join("b.txt").to_string_lossy().to_string();
        assert!(same_volume_impl(a.clone(), b));
        // A path vs itself is trivially the same volume.
        assert!(same_volume_impl(a.clone(), a));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn entries_for_paths_stats_existing_and_skips_missing() {
        let d = scratch("entriesforpaths");
        fs::write(d.join("a.txt"), b"hi").unwrap();
        fs::create_dir(d.join("sub")).unwrap();
        let a = d.join("a.txt").to_string_lossy().to_string();
        let sub = d.join("sub").to_string_lossy().to_string();
        let gone = d.join("nope.txt").to_string_lossy().to_string();
        let out = entries_for_paths_impl(vec![a.clone(), sub.clone(), gone]);
        // The missing path is skipped; the two real ones come back with correct kinds.
        assert_eq!(out.len(), 2);
        assert!(out.iter().any(|e| e.name == "a.txt" && !e.is_dir && e.extension == "txt"));
        assert!(out.iter().any(|e| e.name == "sub" && e.is_dir));
        let _ = fs::remove_dir_all(&d);
    }

    // image-preview (transcode + metadata/EXIF) tests moved with the code to `cpe_server::image_preview`
    // (CPE-815).

    #[test]
    fn read_archive_entries_errors_on_a_non_iso() {
        let d = scratch("iso_bad");
        let f = d.join("x.iso");
        fs::write(&f, vec![0u8; 4096]).unwrap();
        assert!(cpe_server::archive::read_archive_entries(&f.to_string_lossy()).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn read_archive_entries_errors_on_a_non_7z() {
        let d = scratch("sevenz_bad");
        let f = d.join("x.7z");
        fs::write(&f, b"not a 7z archive").unwrap();
        assert!(cpe_server::archive::read_archive_entries(&f.to_string_lossy()).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn read_preview_info_dispatches_by_extension() {
        let d = scratch("dispatch");
        let f = d.join("thing.bin");
        fs::write(&f, b"\x01\x02\x03").unwrap();
        // .bin -> hex dump path
        let out = read_preview_info_impl(f.to_string_lossy().to_string()).unwrap();
        assert!(out.contains("01 02 03"));
        let _ = fs::remove_dir_all(&d);
    }

    // The settings + tag-store model/persistence tests moved with the code to `cpe_server::settings`
    // and `cpe_server::tags` (CPE-815).

    #[test]
    fn create_dir_rejects_an_empty_name() {
        let d = scratch("create_empty");
        let r = create_dir_impl(d.to_string_lossy().to_string(), "   ".to_string());
        assert!(r.is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn create_rejects_path_separators_and_traversal() {
        let d = scratch("create_sep");
        fs::create_dir_all(&d).unwrap();
        let dir = d.to_string_lossy().to_string();
        for bad in ["../evil", "sub/x", "a\\b", "..", "."] {
            assert!(create_dir_impl(dir.clone(), bad.to_string()).is_err(), "create_dir must reject {bad:?}");
            assert!(create_file_impl(dir.clone(), bad.to_string()).is_err(), "create_file must reject {bad:?}");
        }
        // Nothing escaped the folder.
        assert!(!d.parent().unwrap().join("evil").exists());
        // A normal name still works.
        assert!(create_dir_impl(dir.clone(), "ok".to_string()).is_ok());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn create_dir_refuses_to_clobber_an_existing_name() {
        let d = scratch("create_dup");
        let p = d.to_string_lossy().to_string();
        assert!(create_dir_impl(p.clone(), "thing".into()).is_ok());
        let second = create_dir_impl(p, "thing".into());
        assert!(second.is_err(), "must not silently overwrite");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn create_file_makes_an_empty_file() {
        let d = scratch("create_file");
        let created =
            create_file_impl(d.to_string_lossy().to_string(), "New Text Document.txt".into()).unwrap();
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
        assert!(create_file_impl(p, "note.txt".into()).is_err());
        assert_eq!(fs::read_to_string(d.join("note.txt")).unwrap(), "important");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn rename_refuses_to_clobber_an_existing_name() {
        let d = scratch("rename_dup");
        fs::write(d.join("a.txt"), b"a").unwrap();
        fs::write(d.join("b.txt"), b"b").unwrap();

        let r = rename_entry_impl(
            d.join("a.txt").to_string_lossy().to_string(),
            "b.txt".into(),
        );
        assert!(r.is_err(), "renaming onto an existing file must fail");
        // b.txt must be untouched.
        assert_eq!(fs::read(d.join("b.txt")).unwrap(), b"b");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn rename_refuses_a_path_separator_or_traversal() {
        let d = scratch("rename_sep");
        fs::write(d.join("a.txt"), b"a").unwrap();
        let p = d.join("a.txt").to_string_lossy().to_string();
        for bad in ["../evil.txt", "sub/b.txt", "a\\b.txt", "..", "."] {
            assert!(rename_entry_impl(p.clone(), bad.into()).is_err(), "must reject {bad:?}");
        }
        // The file stays put and nothing escaped the folder.
        assert!(d.join("a.txt").exists());
        assert!(!d.parent().unwrap().join("evil.txt").exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn rename_moves_the_file() {
        let d = scratch("rename_ok");
        fs::write(d.join("a.txt"), b"a").unwrap();
        let r = rename_entry_impl(
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

        let results = copy_entries_impl(
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
        let results = copy_entries_impl(
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

    fn wa(kind: &str, resolved: &str) -> WatchAction {
        WatchAction { kind: kind.to_string(), resolved: resolved.to_string() }
    }

    #[test]
    fn read_attributes_reflects_readonly_toggle() {
        let d = scratch("attrs");
        let f = d.join("a.txt");
        fs::write(&f, b"x").unwrap();
        let p = f.to_string_lossy().to_string();
        // a fresh file is writable (not readonly)
        let before = read_attributes_impl(&p).unwrap();
        assert!(!before.readonly);
        // make it read-only via the platform-appropriate write path, then re-read
        set_readonly_impl(p.clone(), true).unwrap();
        assert!(read_attributes_impl(&p).unwrap().readonly);
        // a normal file isn't hidden by a leading dot / attribute
        assert!(!before.hidden);
        set_readonly_impl(p.clone(), false).unwrap(); // restore so cleanup can delete it
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn run_command_captures_stdout_and_zero_exit() {
        // `echo hello` works under both cmd /C and sh -c; output has a trailing newline (\r\n or \n).
        let out = run_command_impl("echo hello".to_string(), None).unwrap();
        assert!(out.stdout.contains("hello"), "stdout was {:?}", out.stdout);
        assert_eq!(out.code, Some(0));
        assert!(!out.truncated);
    }

    #[test]
    fn run_command_reports_a_nonzero_exit_code() {
        // `exit 3` sets the shell's exit status on both platforms.
        let out = run_command_impl("exit 3".to_string(), None).unwrap();
        assert_eq!(out.code, Some(3));
    }

    #[test]
    fn run_command_rejects_an_empty_command() {
        assert!(run_command_impl("   ".to_string(), None).is_err());
    }

    #[test]
    fn capped_string_truncates_at_the_byte_cap() {
        let (s, trunc) = capped_string(vec![b'a'; 100], 10);
        assert_eq!(s.len(), 10);
        assert!(trunc);
        let (s2, trunc2) = capped_string(vec![b'a'; 5], 10);
        assert_eq!(s2, "aaaaa");
        assert!(!trunc2);
    }

    #[test]
    fn watch_actions_move_copy_rename_over_a_landed_file() {
        let d = scratch("watch_exec");
        let src_dir = d.join("in");
        let sorted = d.join("sorted");
        fs::create_dir_all(&src_dir).unwrap();
        fs::create_dir_all(&sorted).unwrap();

        // move: file lands in `in/`, rule moves it into `sorted/`.
        let f = src_dir.join("a.txt");
        fs::write(&f, b"hi").unwrap();
        let r = run_watch_actions_impl(f.to_string_lossy().to_string(), vec![wa("move", &sorted.to_string_lossy())]);
        assert!(r.iter().all(|x| x.ok), "{r:?}");
        assert!(!f.exists() && sorted.join("a.txt").exists()); // moved, original gone

        // copy: original stays put, a copy appears in `sorted/`.
        let g = src_dir.join("b.txt");
        fs::write(&g, b"yo").unwrap();
        run_watch_actions_impl(g.to_string_lossy().to_string(), vec![wa("copy", &sorted.to_string_lossy())]);
        assert!(g.exists() && sorted.join("b.txt").exists()); // both exist

        // rename: in place, new name in same dir.
        let h = src_dir.join("c.log");
        fs::write(&h, b"x").unwrap();
        run_watch_actions_impl(h.to_string_lossy().to_string(), vec![wa("rename", "c.txt")]);
        assert!(!h.exists() && src_dir.join("c.txt").exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn watch_actions_pipeline_threads_the_updated_path() {
        // rename → move: the move must act on the *renamed* file, so the pipeline threads the new path.
        let d = scratch("watch_pipe");
        let src_dir = d.join("in");
        let dest = d.join("out");
        fs::create_dir_all(&src_dir).unwrap();
        fs::create_dir_all(&dest).unwrap();
        let f = src_dir.join("raw.dat");
        fs::write(&f, b"data").unwrap();

        let r = run_watch_actions_impl(
            f.to_string_lossy().to_string(),
            vec![wa("rename", "final.dat"), wa("move", &dest.to_string_lossy())],
        );
        assert!(r.iter().all(|x| x.ok), "{r:?}");
        assert!(dest.join("final.dat").exists()); // renamed THEN moved under the new name
        assert!(!src_dir.join("raw.dat").exists() && !src_dir.join("final.dat").exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn watch_actions_report_unknown_action_per_step_without_aborting() {
        let d = scratch("watch_unknown");
        fs::create_dir_all(&d).unwrap();
        let f = d.join("a.txt");
        fs::write(&f, b"z").unwrap();
        let r = run_watch_actions_impl(
            f.to_string_lossy().to_string(),
            vec![wa("frobnicate", "whatever"), wa("rename", "b.txt")],
        );
        assert_eq!(r.len(), 2);
        assert!(!r[0].ok); // unknown action errored
        assert!(r[1].ok); // but the pipeline continued and the rename still ran
        assert!(d.join("b.txt").exists());
        let _ = fs::remove_dir_all(&d);
    }

    // scan_tree tests moved with the code to `cpe_server::compare` (CPE-815).

    // backup-engine tests moved with the code to `cpe_server::backup` (CPE-821).

    #[test]
    fn run_transfer_copies_a_tree_and_reports_byte_progress() {
        let d = scratch("xfer_copy");
        fs::create_dir_all(d.join("src/sub")).unwrap();
        fs::write(d.join("src/a.txt"), b"hello").unwrap(); // 5 bytes
        fs::write(d.join("src/sub/b.txt"), b"world!!").unwrap(); // 7 bytes
        fs::create_dir_all(d.join("dst")).unwrap();
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let mut last_done = 0u64;
        let report = run_transfer(
            1,
            &[d.join("src")],
            &d.join("dst"),
            TransferKind::Copy,
            ConflictPolicy::Keepboth,
            &cancel,
            |p| last_done = p.done_bytes,
        );
        assert_eq!(report.transferred, 1);
        assert_eq!(report.failed, 0);
        assert!(!report.cancelled);
        assert_eq!(fs::read(d.join("dst/src/a.txt")).unwrap(), b"hello");
        assert_eq!(fs::read(d.join("dst/src/sub/b.txt")).unwrap(), b"world!!");
        // Byte counts here are file *content* lengths (portable, unlike dir/symlink sizes): 5 + 7.
        assert_eq!(last_done, 12);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn run_transfer_honours_conflict_policies() {
        let d = scratch("xfer_conf");
        fs::write(d.join("a.txt"), b"NEW").unwrap();
        fs::create_dir_all(d.join("dst")).unwrap();
        fs::write(d.join("dst/a.txt"), b"OLD").unwrap();
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let src = || vec![d.join("a.txt")];

        // Skip: the existing file is untouched.
        let r = run_transfer(1, &src(), &d.join("dst"), TransferKind::Copy, ConflictPolicy::Skip, &cancel, |_| {});
        assert_eq!(r.skipped, 1);
        assert_eq!(fs::read(d.join("dst/a.txt")).unwrap(), b"OLD");

        // Keep both: a non-colliding copy is created; the original stays.
        let r = run_transfer(2, &src(), &d.join("dst"), TransferKind::Copy, ConflictPolicy::Keepboth, &cancel, |_| {});
        assert_eq!(r.transferred, 1);
        assert_eq!(fs::read(d.join("dst/a.txt")).unwrap(), b"OLD");
        assert!(d.join("dst/a - Copy.txt").exists(), "keep-both should auto-number");

        // Overwrite: the existing file is replaced.
        let r = run_transfer(3, &src(), &d.join("dst"), TransferKind::Copy, ConflictPolicy::Overwrite, &cancel, |_| {});
        assert_eq!(r.transferred, 1);
        assert_eq!(fs::read(d.join("dst/a.txt")).unwrap(), b"NEW");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn run_transfer_move_removes_the_source() {
        let d = scratch("xfer_move");
        fs::write(d.join("m.txt"), b"data").unwrap();
        fs::create_dir_all(d.join("dst")).unwrap();
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let r = run_transfer(1, &[d.join("m.txt")], &d.join("dst"), TransferKind::Move, ConflictPolicy::Keepboth, &cancel, |_| {});
        assert_eq!(r.transferred, 1);
        assert!(!d.join("m.txt").exists(), "move should remove the source");
        assert_eq!(fs::read(d.join("dst/m.txt")).unwrap(), b"data");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn run_transfer_cancel_stops_before_copying() {
        let d = scratch("xfer_cancel");
        fs::write(d.join("a.txt"), b"x").unwrap();
        fs::create_dir_all(d.join("dst")).unwrap();
        let cancel = std::sync::atomic::AtomicBool::new(true); // pre-cancelled
        let r = run_transfer(1, &[d.join("a.txt")], &d.join("dst"), TransferKind::Copy, ConflictPolicy::Keepboth, &cancel, |_| {});
        assert!(r.cancelled);
        assert_eq!(r.transferred, 0);
        assert!(!d.join("dst/a.txt").exists());
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

        let results = move_entries_impl(
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

        let results = copy_entries_impl(
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

    // entry_info tests moved with the code to `cpe_server::model` (CPE-815).

    // hash_file tests moved with the code to `cpe_server::checksum` (CPE-815).

    #[test]
    fn ensure_previewable_size_rejects_oversized_files() {
        let d = scratch("previewcap");
        fs::write(d.join("f"), vec![0u8; 2000]).unwrap();
        let p = d.join("f").to_string_lossy().to_string();
        assert!(ensure_previewable_size(&p, 1000).is_err(), "2000 > 1000 must be refused");
        assert!(ensure_previewable_size(&p, 5000).is_ok(), "2000 < 5000 is fine");
        // A missing file is the reader's problem, not this guard's.
        assert!(ensure_previewable_size(&d.join("nope").to_string_lossy(), 1000).is_ok());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn hex_dump_caps_output_at_max_bytes() {
        let d = scratch("hexcap");
        fs::write(d.join("f.bin"), vec![0xABu8; 10_000]).unwrap();
        // 32 bytes = two 16-byte rows; a third row offset (00000020) must not appear.
        let out = cpe_server::binary_preview::hex_dump(&d.join("f.bin").to_string_lossy(), 32).unwrap();
        assert!(out.contains("00000000") && out.contains("00000010"));
        assert!(!out.contains("00000020"), "dumped past the max");
        assert!(out.contains("ab ab"), "bytes rendered");
        let _ = fs::remove_dir_all(&d);
    }

    // text-stats tests moved with the code to `cpe_server::text_stats` (CPE-815).

    // content-search tests moved with the code to `cpe_server::content_search` (CPE-815).

    // name_matches / expand_braces / find_files_by_name tests moved with the code to
    // `cpe_server::name_search` (CPE-815).

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
        let r = cpe_server::name_search::find_files_by_name(&d.to_string_lossy(), "target").unwrap();
        assert!(!r.truncated, "walk hit its cap — the symlink cycle was not skipped");
        assert!(r.dirs_scanned < 100, "walked too many dirs ({}) — cycle not skipped", r.dirs_scanned);
        assert!(r.matches.iter().any(|m| m.name == "target.txt"));

        let c = cpe_server::content_search::search_file_contents(&d.to_string_lossy(), "needle", false).unwrap();
        assert!(!c.truncated, "content search hit its cap — symlink cycle not skipped");
        assert!(c.matches.iter().any(|m| m.path.replace('\\', "/").ends_with("real/target.txt")));

        // dir_size must NOT stack-overflow on the cycle (it recurses). The invariant is simply that it
        // *terminates* with a small, finite total — without the CPE-611 fix it recurses until the thread
        // stack overflows and aborts the whole test binary. Don't assert an exact byte count: it isn't
        // portable (Linux counts the symlink entry's target-path length, ~31 bytes; Windows reports 0),
        // so bound it instead: at least the real file (6 bytes), and nowhere near a runaway.
        let sz = cpe_server::disk_usage::dir_size(&d.to_string_lossy()).unwrap();
        assert!((6..100_000).contains(&sz), "dir_size should terminate small on a cycle, got {sz}");

        // find_duplicates likewise terminates (one file, no dupes, not truncated).
        let dup = cpe_server::duplicates::find_duplicates(&d.to_string_lossy()).unwrap();
        assert!(!dup.truncated, "find_duplicates hit its cap — symlink cycle not skipped");

        // remove_dir_all removes the symlink itself without following it.
        let _ = fs::remove_dir_all(&d);
    }

    // files_identical tests moved with the code to `cpe_server::compare` (CPE-815).

    // find_duplicates tests moved with the code to `cpe_server::duplicates` (CPE-815).

    // dir_size / dir_children_sizes tests moved with the code to `cpe_server::disk_usage` (CPE-815).

    // folder_stats tests moved with the code to `cpe_server::folder_stats` (CPE-815).

    #[test]
    fn move_exact_restores_to_the_original_name() {
        let d = scratch("move_exact");
        fs::write(d.join("b.txt"), b"x").unwrap();

        let results = move_exact_impl(vec![(
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

        let results = move_exact_impl(vec![(
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
