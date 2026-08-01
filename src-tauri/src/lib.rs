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
use cpe_server::{audit_journal, geometry, metrics_journal, ticket_board};
/// Shared FS utils (epoch-ms + streaming SHA-256) also live in `cpe-server` (CPE-815); re-export them
/// so the many `to_epoch_ms(…)` / `sha256_file(…)` call sites resolve unchanged.
use cpe_server::fsutil::to_epoch_ms;

/// Read every ticket under `<root>/Ticketing/Tickets/{Backlog,Doing,Blocked,Deferred,Done}/CPE-*.md` into board
/// cards (CPE-520). Read-only; a malformed file is skipped, never fails the listing.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn board_cards(root: String) -> Vec<ticket_board::Card> {
    tauri::async_runtime::spawn_blocking(move || board_cards_impl(root))
        .await.unwrap()
}

fn board_cards_impl(root: String) -> Vec<ticket_board::Card> {
    let tickets = std::path::Path::new(&root).join("Ticketing").join("Tickets");
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

/// Find the nearest project root at/above `start` — the closest ancestor dir with a `Ticketing/` folder —
/// so the Agent Board can auto-open the project you're inside (CPE-554). `None` if none is found.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn board_move(root: String, id: String, to_column: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || board_move_impl(root, id, to_column))
        .await.map_err(|e| e.to_string())?
}

fn board_move_impl(root: String, id: String, to_column: String) -> Result<(), String> {
    let folder =
        ticket_board::folder_for_column(&to_column).ok_or_else(|| format!("unknown column '{to_column}'"))?;
    let status = ticket_board::status_for_column(&to_column).unwrap_or(folder);
    let tickets = std::path::Path::new(&root).join("Ticketing").join("Tickets");

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

/// Collect archived Done tickets — those in **subdirectories** of `Ticketing/Tickets/Done/` (the dated
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn board_archived(root: String) -> Vec<ticket_board::Card> {
    tauri::async_runtime::spawn_blocking(move || board_archived_impl(root))
        .await.unwrap()
}

fn board_archived_impl(root: String) -> Vec<ticket_board::Card> {
    let done = std::path::Path::new(&root).join("Ticketing").join("Tickets").join("Done");
    let mut out = Vec::new();
    collect_archived(&done, true, &mut out);
    out
}

/// List the repo's epics for the board's epic-organized view (CPE-530): active/proposed epics from
/// `Ticketing/Epics/` + closed epics from `Ticketing/Tickets/Done/` (top level), each `epic`-tagged.
/// Read-only. Note the two live at different depths since CPE-1128 (Epics is a sibling of Tickets).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn board_epics(root: String) -> Vec<ticket_board::Epic> {
    tauri::async_runtime::spawn_blocking(move || board_epics_impl(root))
        .await.unwrap()
}

fn board_epics_impl(root: String) -> Vec<ticket_board::Epic> {
    let base = std::path::Path::new(&root).join("Ticketing");
    let mut epics = Vec::new();
    // Epics is a top-level sibling queue; closed epics live in the status-flow's Done (CPE-1128).
    for dir in [base.join("Epics"), base.join("Tickets").join("Done")] {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
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
    // Search ALL of Ticketing/ recursively, so a card in the sibling Epics/ or Sprints/ queues or an
    // archived Tickets/Done/** subfolder resolves too — not just the five workflow columns (CPE-966,
    // CPE-1128). The `{id}_` prefix (with the underscore) keeps `CPE-6` from matching `CPE-616`.
    let base = std::path::Path::new(root).join("Ticketing");
    find_ticket_file_recursive(&base, &format!("{id}_"))
}

/// Toggle the `review` tag on ticket `id` (CPE-523) — drives the board's virtual Review lane.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn board_note(root: String, id: String, note: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || board_note_impl(root, id, note))
        .await.map_err(|e| e.to_string())?
}

fn board_note_impl(root: String, id: String, note: String) -> Result<(), String> {
    let path = find_ticket_file(&root, &id).ok_or_else(|| format!("ticket {id} not found"))?;
    let md = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    std::fs::write(&path, ticket_board::append_finding(&md, &note)).map_err(|e| e.to_string())
}

/// Emit a machine-readable **directive** into ticket `id` (CPE-961) — the board→agent communication seam:
/// an instruction an agent (local or an external one sharing the repo/folder) can read, act on, and answer
/// in the ticket. `target` is the intended agent (`any` if blank); `when` is an ISO-8601 timestamp from the
/// caller. Appends under `## Agent Directives`.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn board_directive(root: String, id: String, target: String, text: String, when: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || board_directive_impl(root, id, target, text, when))
        .await
        .map_err(|e| e.to_string())?
}

fn board_directive_impl(root: String, id: String, target: String, text: String, when: String) -> Result<(), String> {
    let path = find_ticket_file(&root, &id).ok_or_else(|| format!("ticket {id} not found"))?;
    let md = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    std::fs::write(&path, ticket_board::append_directive(&md, &when, &target, &text)).map_err(|e| e.to_string())
}

/// Full detail for one Agent Board card (CPE-959): its ordered frontmatter fields + markdown body + the
/// folder under `Ticketing/` it lives in — for the card-detail popup. Works for tickets and epics alike.
#[derive(serde::Serialize)]
#[cfg_attr(feature = "specta-bindings", derive(specta::Type))]
struct CardDetail {
    id: String,
    /// Folder under `Ticketing/` (e.g. "Tickets/Backlog", "Epics", "Tickets/Done/2026/Q3/July/Week-30").
    location: String,
    /// Ordered frontmatter `(key, value)` pairs.
    fields: Vec<(String, String)>,
    /// The markdown body after the frontmatter.
    body: String,
}

/// Read one card's full detail by id, from anywhere under `Ticketing/` (CPE-959). `None` if not found.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn board_card_detail(root: String, id: String) -> Option<CardDetail> {
    tauri::async_runtime::spawn_blocking(move || board_card_detail_impl(root, id))
        .await
        .unwrap_or(None)
}

fn board_card_detail_impl(root: String, id: String) -> Option<CardDetail> {
    let path = find_ticket_file(&root, &id)?;
    let md = std::fs::read_to_string(&path).ok()?;
    let (fields, body) = ticket_board::detail_from(&md);
    let base = std::path::Path::new(&root).join("Ticketing");
    let location = path
        .parent()
        .and_then(|par| par.strip_prefix(&base).ok())
        .map(|rel| rel.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    Some(CardDetail { id, location, fields, body })
}

/// The workbench's view of a folder (CPE-526/535): whether it's a git repo, the branch, and the diff.
#[derive(serde::Serialize)]
#[cfg_attr(feature = "specta-bindings", derive(specta::Type))]
struct WorkbenchDiff {
    is_repo: bool,
    branch: Option<String>,
    diff: String,
}

/// `git diff` (working tree vs HEAD) in `root` for the integrated workbench, with friendly edge cases
/// (CPE-535): a non-repo folder is a normal `is_repo:false` result (not an error), git-not-installed is
/// a distinct error, and an empty `root` is refused. An optional `path` limits it to one file. Read-only.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
// Secure-delete shred (CPE-1240, epic CPE-738): `ShredScheme` picks the overwrite pattern
// (`secure_delete`, pure planning), `ShredResult` is the per-path OpResult-shaped outcome the
// `shred_paths` command returns (`secure_shred`, the disk-backed engine).
use cpe_server::secure_delete::ShredScheme;
use cpe_server::secure_shred::ShredResult;

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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn list_dir(path: String) -> Result<Vec<DirEntry>, String> {
    cpe_server::fs_route::require_local(&path)?;
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn list_dir_stream(
    path: String,
    stream_id: u64,
    on_entry: tauri::ipc::Channel<Vec<DirEntry>>,
) -> Result<usize, String> {
    cpe_server::fs_route::require_local(&path)?;
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
    // This is the arbitrary-path (smart folder) stat, not the hot `list_dir` walk, so a `symlink_metadata`
    // call here (instead of `list_dir`'s free `file_type()` off the walk's own read) is fine — it's an
    // extra syscall per *smart-folder row*, not per listed folder entry.
    let is_symlink = fs::symlink_metadata(p).map(|m| m.file_type().is_symlink()).unwrap_or(false);
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
        is_symlink,
    })
}

/// Stat a set of paths into `DirEntry` rows for a virtual listing (smart folders, CPE-667). Paths that
/// no longer exist or can't be read are silently skipped, so a smart folder self-heals as files move or
/// are deleted rather than showing dead rows.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn create_dir(app: tauri::AppHandle, path: String, name: String) -> Result<String, String> {
    // Attribute the resulting watcher event to the user, not an agent (CPE-1101). No-op off-feature.
    note_app_op(&app, || vec![Path::new(&path).join(name.trim()).to_string_lossy().into_owned()]);
    tauri::async_runtime::spawn_blocking(move || create_dir_impl(path, name))
        .await.map_err(|e| e.to_string())?
}

fn create_dir_impl(path: String, name: String) -> Result<String, String> {
    cpe_server::fs_route::require_local(&path)?;
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn create_file(app: tauri::AppHandle, path: String, name: String) -> Result<String, String> {
    note_app_op(&app, || vec![Path::new(&path).join(name.trim()).to_string_lossy().into_owned()]);
    tauri::async_runtime::spawn_blocking(move || create_file_impl(path, name))
        .await.map_err(|e| e.to_string())?
}

fn create_file_impl(path: String, name: String) -> Result<String, String> {
    cpe_server::fs_route::require_local(&path)?;
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

/// Create a new file `name` inside `path` seeded with `content` (CPE-1161) — used by the New ▸ menu
/// for stub templates that must be valid on creation (e.g. Rich Text `{\rtf1\ansi }`). Mirrors
/// `create_file`'s validation and atomic `create_new` (fails rather than clobbering).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn create_file_with_content(
    app: tauri::AppHandle,
    path: String,
    name: String,
    content: String,
) -> Result<String, String> {
    note_app_op(&app, || vec![Path::new(&path).join(name.trim()).to_string_lossy().into_owned()]);
    tauri::async_runtime::spawn_blocking(move || create_file_with_content_impl(path, name, content))
        .await.map_err(|e| e.to_string())?
}

fn create_file_with_content_impl(path: String, name: String, content: String) -> Result<String, String> {
    cpe_server::fs_route::require_local(&path)?;
    let name = name.trim();
    if name.is_empty() {
        return Err("Name cannot be empty".to_string());
    }
    valid_entry_name(name)?;
    let target = Path::new(&path).join(name);
    if target.exists() {
        return Err(format!("\"{name}\" already exists"));
    }
    use std::io::Write;
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
        .map_err(|e| e.to_string())?;
    f.write_all(content.as_bytes()).map_err(|e| e.to_string())?;
    Ok(target.to_string_lossy().to_string())
}

/// Create a new *valid empty* `.zip` archive `name` inside `path` (CPE-1161) — the New ▸ menu's
/// Compressed (zipped) Folder. An empty file is not a valid zip, so the actual archive writing is
/// delegated to `cpe-server` (where the `zip` crate lives); validation mirrors `create_file`.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn create_empty_zip(app: tauri::AppHandle, path: String, name: String) -> Result<String, String> {
    note_app_op(&app, || vec![Path::new(&path).join(name.trim()).to_string_lossy().into_owned()]);
    tauri::async_runtime::spawn_blocking(move || create_empty_zip_impl(path, name))
        .await.map_err(|e| e.to_string())?
}

fn create_empty_zip_impl(path: String, name: String) -> Result<String, String> {
    cpe_server::fs_route::require_local(&path)?;
    let name = name.trim();
    if name.is_empty() {
        return Err("Name cannot be empty".to_string());
    }
    valid_entry_name(name)?;
    let target = Path::new(&path).join(name);
    if target.exists() {
        return Err(format!("\"{name}\" already exists"));
    }
    cpe_server::archive::create_empty_zip(&target.to_string_lossy())
}

/// Write UTF-8 text back to a file, replacing its contents — for the content
/// editor. Returns the new byte length.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn write_file_text(app: tauri::AppHandle, path: String, contents: String) -> Result<u64, String> {
    note_app_op(&app, || vec![path.clone()]);
    tauri::async_runtime::spawn_blocking(move || write_file_text_impl(path, contents))
        .await.map_err(|e| e.to_string())?
}

fn write_file_text_impl(path: String, contents: String) -> Result<u64, String> {
    cpe_server::fs_route::require_local(&path)?;
    fs::write(&path, contents.as_bytes()).map_err(|e| e.to_string())?;
    Ok(contents.len() as u64)
}

// The archive-listing domain (ArchiveEntry + per-format listers + the extension dispatcher) now lives
// in `cpe_server::archive` (CPE-815); the `read_archive_entries` command below dispatches to it.

/// List an archive's entries without extracting it, for the preview pane. Model lives in
/// `cpe_server::archive` (CPE-815); this is a thin `spawn_blocking` dispatcher.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn data_browser_sources(path: String) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::data_browser::sources(&path))
        .await.map_err(|e| e.to_string())?
}

#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn read_image_data_url(path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        ensure_previewable_size(&path, PREVIEW_INFO_MAX_BYTES)?;
        cpe_server::image_preview::read_image_data_url(&path)
    })
    .await
    .map_err(|e| e.to_string())?
}


/// A PNG thumbnail of an image file as a `data:` URL the `<img>` tag can show (CPE-642), served from
/// an mtime-keyed on-disk cache (CPE-644). Also covers `.svg` (rasterized) and `.ttf`/`.otf`/`.woff`
/// glyph-sheet specimens (CPE-1236) — the format dispatch lives entirely in
/// `cpe_server::thumb_source`, so this stays a thin one-line-per-branch delegate. Bounded by the
/// preview size cap so a huge image can't exhaust memory. Errors (rather than hangs) on an unsupported
/// or malformed source, so the frontend falls back to an icon.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn thumbnail(app: tauri::AppHandle, path: String, max_edge: u32) -> Result<String, String> {
    use base64::Engine;
    ensure_previewable_size(&path, PREVIEW_INFO_MAX_BYTES)?;
    let png = match server_ctx::TauriCtx::new(&app).app_cache_dir() {
        Ok(dir) => cpe_server::thumbnail::thumbnail_cached(&dir.join("thumbnails"), Path::new(&path), max_edge)?,
        Err(_) => cpe_server::thumbnail::make_thumbnail_png(Path::new(&path), max_edge)?, // no cache dir
    };
    Ok(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png)))
}

/// Per-stream cancel flags for `thumbnails_stream`, keyed by the frontend-supplied `stream_id` — mirrors
/// `index_build`'s registry and the `DIR_STREAM_CANCELS` pattern in STREAMING.md. A scroll-superseded
/// batch (a new visible window landed while the previous one was still draining) is cancelled by setting
/// its flag; the drain loop in `cpe_server::thumb_pipeline::run_thumb_batch` checks it between requests
/// and simply abandons the rest of its per-call queue (no removal API needed — see that module's docs).
fn thumb_stream_cancels(
) -> &'static std::sync::Mutex<std::collections::HashMap<u64, std::sync::Arc<std::sync::atomic::AtomicBool>>>
{
    static CANCELS: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<u64, std::sync::Arc<std::sync::atomic::AtomicBool>>>,
    > = std::sync::OnceLock::new();
    CANCELS.get_or_init(Default::default)
}

/// RAII guard that removes a `thumb_stream_cancels()` entry when dropped (CPE-1239). Panic-safe: if the
/// `spawn_blocking` batch in `thumbnails_stream` panics, `.await` returns a `JoinError` and the `?` below
/// returns early *before* a plain "remove at the end" line would run — but `Drop` still fires during that
/// unwind, so the entry is always removed instead of leaking one `HashMap` slot per panicking batch.
struct ThumbStreamCancelGuard(u64);

impl Drop for ThumbStreamCancelGuard {
    fn drop(&mut self) {
        thumb_stream_cancels().lock().unwrap().remove(&self.0);
    }
}

/// Stream thumbnails for a batch of visible/prefetch-margin tiles through the priority queue + shared
/// in-memory cache (CPE-1237, epic CPE-718): wires the previously-orphaned `thumb_queue` (CPE-950,
/// Visible > Prefetch > Background scheduling) and `thumb_cache` (CPE-939, dual-budget LRU) into a real
/// dispatch path — `cpe_server::thumb_pipeline::run_thumb_batch` owns the whole enqueue/drain/compute
/// loop, this command is just the thin Tauri wiring: resolve the on-disk cache dir, hand it the
/// `thumbnail_cached` decoder as `compute`, and forward each streamed `ThumbResult` over `on_thumb` as it
/// lands (STREAMING.md). `stream_id` registers a cancel flag `cancel_thumbnails_stream` can trip when the
/// frontend's visible window moves on before this batch finishes draining. Async + `spawn_blocking` — the
/// decode work is real file + CPU work and must never run on the UI thread.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn thumbnails_stream(
    app: tauri::AppHandle,
    requests: Vec<cpe_server::thumb_pipeline::ThumbRequest>,
    stream_id: u64,
    on_thumb: tauri::ipc::Channel<cpe_server::thumb_pipeline::ThumbResult>,
    state: tauri::State<'_, cpe_server::thumb_pipeline::ThumbCacheService>,
) -> Result<usize, String> {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let service = state.inner().clone();
    let cancel = Arc::new(AtomicBool::new(false));
    thumb_stream_cancels().lock().unwrap().insert(stream_id, cancel.clone());
    // Guarantees removal below even if the `spawn_blocking` batch panics (CPE-1239) — see the guard's docs.
    let _cancel_guard = ThumbStreamCancelGuard(stream_id);
    let cache_dir = server_ctx::TauriCtx::new(&app).app_cache_dir().ok().map(|d| d.join("thumbnails"));

    let emitted = tauri::async_runtime::spawn_blocking(move || {
        let n = cpe_server::thumb_pipeline::run_thumb_batch(
            &requests,
            service.store(),
            |path, edge| match &cache_dir {
                Some(dir) => cpe_server::thumbnail::thumbnail_cached(dir, path, edge),
                None => cpe_server::thumbnail::make_thumbnail_png(path, edge),
            },
            || cancel.load(Ordering::Relaxed),
            |result| {
                let _ = on_thumb.send(result);
            },
        );
        n
    })
    .await
    .map_err(|e| e.to_string())?;

    Ok(emitted)
}

/// Cancel an in-flight `thumbnails_stream` batch (CPE-1237) — the frontend calls this with the previous
/// generation's `stream_id` when the visible window moves on before that batch finished draining, so
/// requests for tiles that already scrolled away stop competing with the new visible-priority ones.
/// Idempotent: a no-op for an unknown or already-finished `stream_id`.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn cancel_thumbnails_stream(stream_id: u64) {
    use std::sync::atomic::Ordering;
    if let Some(flag) = thumb_stream_cancels().lock().unwrap().get(&stream_id) {
        flag.store(true, Ordering::Relaxed);
    }
}

/// Read a text file's contents for the preview pane, capped at `max_bytes` so a
/// huge file can never be slurped into memory. Errors (rather than truncating)
/// when the file is too large, unreadable, or not valid UTF-8 — the frontend
/// shows a "can't preview" state in that case.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn read_file_text(path: String, max_bytes: u64) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || read_file_text_impl(path, max_bytes))
        .await.map_err(|e| e.to_string())?
}

fn read_file_text_impl(path: String, max_bytes: u64) -> Result<String, String> {
    cpe_server::fs_route::require_local(&path)?;
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

/// Aggregate code-intelligence bundle — outline, fold ranges, indent depths, and minimap rows — for a
/// file whose text the frontend already has loaded (CPE-1089, epic CPE-724). Pure/in-memory (no fs, no
/// path); the previewer sends `text` straight from `read_file_text`'s result instead of a second read.
/// Sync: `cpe_server::code_intel::analyze` is a fast in-memory scan over the already-loaded text, not an
/// fs/subprocess/network call.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn code_intel(
    text: String,
    lang: String,
    tab_width: Option<usize>,
    minimap_buckets: Option<usize>,
) -> cpe_server::code_intel::CodeIntel {
    cpe_server::code_intel::analyze(&text, &lang, tab_width.unwrap_or(4), minimap_buckets.unwrap_or(120))
}

/// Read a byte range of a file without loading the whole file — backs the hex inspector's paging
/// (CPE-772, epic CPE-719). Seeks to `offset` (past EOF yields an empty slice, not an error, so the
/// viewer can page freely) and reads up to `len` bytes, clamped to EOF.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn read_file_range(path: String, offset: u64, len: u64) -> Result<Vec<u8>, String> {
    tauri::async_runtime::spawn_blocking(move || read_file_range_impl(path, offset, len))
        .await
        .map_err(|e| e.to_string())?
}

fn read_file_range_impl(path: String, offset: u64, len: u64) -> Result<Vec<u8>, String> {
    cpe_server::fs_route::require_local(&path)?;
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn set_permissions(path: String, mode: u32) -> Result<u32, String> {
    let _ = (path, mode);
    Err("POSIX permissions aren't available on this platform.".to_string())
}

/// Toggle a file's read-only flag (cross-platform), returning the prior state for undo (CPE-785).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", derive(specta::Type))]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn rename_entry(app: tauri::AppHandle, path: String, new_name: String) -> Result<String, String> {
    // Both the old name (removed) and the new name (created) are the user's doing (CPE-1101).
    note_app_op(&app, || {
        let mut targets = vec![path.clone()];
        if let Some(parent) = Path::new(&path).parent() {
            targets.push(parent.join(new_name.trim()).to_string_lossy().into_owned());
        }
        targets
    });
    let ctx = server_ctx::TauriCtx::new(&app);
    tauri::async_runtime::spawn_blocking(move || rename_entry_impl(&ctx, path, new_name))
        .await.map_err(|e| e.to_string())?
}

/// Rename `path` to `new_name` in place. `ctx` re-keys any tag-store entries under `path` — the
/// file/folder's own entry and, for a directory, every tagged descendant's — to the new path so
/// tags follow the rename instead of orphaning at the old path (CPE-1222); it likewise re-keys any
/// scheduled-snapshot catalog entry under `path` to the new path (CPE-1225). Best-effort: a tag-store
/// or schedule-catalog write failure never fails an otherwise-successful filesystem rename (there's
/// nothing sane to roll back to, and the original frontend-side migration was already best-effort).
fn rename_entry_impl(ctx: &dyn ServerCtx, path: String, new_name: String) -> Result<String, String> {
    cpe_server::fs_route::require_local(&path)?;
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
    let target_str = target.to_string_lossy().to_string();
    let _ = cpe_server::tags::retag(ctx, &path, &target_str);
    let _ = cpe_server::snapshot_schedule::reschedule(ctx, &path, &target_str);
    Ok(target_str)
}

/// Move entries to the OS Recycle Bin / Trash. Recoverable by the user.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn delete_to_trash(app: tauri::AppHandle, paths: Vec<String>) -> Vec<OpResult> {
    note_app_op(&app, || paths.clone());
    tauri::async_runtime::spawn_blocking(move || delete_to_trash_impl(paths))
        .await.unwrap()
}

fn delete_to_trash_impl(paths: Vec<String>) -> Vec<OpResult> {
    paths
        .iter()
        .map(|p| {
            let path = Path::new(p);
            if let Err(e) = cpe_server::fs_route::require_local(p) {
                return OpResult::err(path, e);
            }
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn delete_permanent(app: tauri::AppHandle, paths: Vec<String>) -> Vec<OpResult> {
    // Same as its sibling `delete_to_trash` — a permanent delete is the user's doing too, not the
    // owning agent session (CPE-1102, fast-follow on CPE-1101).
    note_app_op(&app, || paths.clone());
    tauri::async_runtime::spawn_blocking(move || delete_permanent_impl(paths))
        .await.unwrap()
}

fn delete_permanent_impl(paths: Vec<String>) -> Vec<OpResult> {
    paths
        .iter()
        .map(|p| {
            let path = Path::new(p);
            if let Err(e) = cpe_server::fs_route::require_local(p) {
                return OpResult::err(path, e);
            }
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

/// Securely delete (shred) entries: overwrite each file's bytes pass-by-pass per `scheme`, then remove
/// it. **Never** routed through the Recycle Bin / Trash — that would defeat the point, since the whole
/// reason to shred instead of `delete_permanent` is that ordinary deletion (trashed OR permanent) still
/// leaves the bytes recoverable on disk until overwritten. The UI must show an honest confirm — this is
/// PERMANENT/NON-RECOVERABLE and, per `secure_delete::plan_shred`'s caveats, overwriting is best-effort
/// (SSD wear-levelling / copy-on-write / journaling filesystems can leave remnants) — before ever
/// calling this.
///
/// Thin dispatcher into `cpe_server::secure_shred::shred_paths` (CPE-1240, epic CPE-738); skip-and-
/// report, so one path's failure doesn't lose the others' results. Async + `spawn_blocking` per
/// CPE-760/761 — a multi-pass overwrite is heavy, potentially slow disk I/O and must not freeze the
/// main thread.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn shred_paths(app: tauri::AppHandle, paths: Vec<String>, scheme: ShredScheme) -> Vec<ShredResult> {
    // Same as its siblings `delete_to_trash`/`delete_permanent` — a shred is the user's doing too, not
    // the owning agent session (CPE-1102 pattern).
    note_app_op(&app, || paths.clone());
    tauri::async_runtime::spawn_blocking(move || cpe_server::secure_shred::shred_paths(&paths, scheme))
        .await
        .unwrap()
}

/// Copy entries into `dest`, auto-renaming on collision.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn copy_entries(app: tauri::AppHandle, paths: Vec<String>, dest: String) -> Vec<OpResult> {
    // Best-effort: record each expected `dest/<name>` target (auto-rename on collision may differ) so
    // the user-copied file reads `actor:"user"` (CPE-1101).
    note_app_op(&app, || {
        paths
            .iter()
            .filter_map(|p| Path::new(p).file_name().map(|n| Path::new(&dest).join(n)))
            .map(|t| t.to_string_lossy().into_owned())
            .collect()
    });
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
/// Shared by the bulk move command and the watch executor. `ctx` re-keys any tag-store entries under
/// `src` to the actually-written path on success (CPE-1222) — best-effort, same rationale as
/// `rename_entry_impl` — and likewise re-keys any scheduled-snapshot catalog entry under `src`
/// (CPE-1225).
fn do_move_into(ctx: &dyn ServerCtx, src: &Path, dest_dir: &Path) -> Result<PathBuf, String> {
    let file_name = src
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "Invalid file name".to_string())?;
    if src.is_dir() && is_self_or_descendant(src, dest_dir) {
        return Err("Cannot move a folder into itself".to_string());
    }
    let target = unique_target(dest_dir, file_name);
    if fs::rename(src, &target).is_ok() {
        let _ = cpe_server::tags::retag(ctx, &src.to_string_lossy(), &target.to_string_lossy());
        let _ = cpe_server::snapshot_schedule::reschedule(
            ctx,
            &src.to_string_lossy(),
            &target.to_string_lossy(),
        );
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
    let _ = cpe_server::tags::retag(ctx, &src.to_string_lossy(), &target.to_string_lossy());
    let _ = cpe_server::snapshot_schedule::reschedule(
        ctx,
        &src.to_string_lossy(),
        &target.to_string_lossy(),
    );
    Ok(target)
}

fn copy_entries_impl(paths: Vec<String>, dest: String) -> Vec<OpResult> {
    let dest_dir = PathBuf::from(&dest);
    paths
        .iter()
        .map(|p| {
            let src = Path::new(p);
            if let Err(e) =
                cpe_server::fs_route::require_local(p).and(cpe_server::fs_route::require_local(&dest))
            {
                return OpResult::err(src, e);
            }
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn move_entries(app: tauri::AppHandle, paths: Vec<String>, dest: String) -> Vec<OpResult> {
    // A move touches both the source (removed) and the `dest/<name>` target (created) (CPE-1101).
    note_app_op(&app, || {
        let mut targets: Vec<String> = paths
            .iter()
            .filter_map(|p| Path::new(p).file_name().map(|n| Path::new(&dest).join(n)))
            .map(|t| t.to_string_lossy().into_owned())
            .collect();
        targets.extend(paths.iter().cloned());
        targets
    });
    let ctx = server_ctx::TauriCtx::new(&app);
    tauri::async_runtime::spawn_blocking(move || move_entries_impl(&ctx, paths, dest))
        .await.unwrap()
}

fn move_entries_impl(ctx: &dyn ServerCtx, paths: Vec<String>, dest: String) -> Vec<OpResult> {
    let dest_dir = PathBuf::from(&dest);
    paths
        .iter()
        .map(|p| {
            let src = Path::new(p);
            if let Err(e) =
                cpe_server::fs_route::require_local(p).and(cpe_server::fs_route::require_local(&dest))
            {
                return OpResult::err(src, e);
            }
            match do_move_into(ctx, src, &dest_dir) {
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
#[cfg_attr(feature = "specta-bindings", derive(specta::Type))]
struct WatchAction {
    kind: String,
    resolved: String,
}

/// Execute a landed file's resolved action pipeline. See the module comment. Async per the commands rule.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn run_watch_actions(app: tauri::AppHandle, path: String, actions: Vec<WatchAction>) -> Vec<OpResult> {
    // This pipeline is app-driven watch automation, not the owning agent session (CPE-1102). Record
    // each step's planned destination up front, best-effort: a `move`/`rename` updates the simulated
    // "current" path for the next step (mirroring `run_watch_actions_impl`), while the real executor's
    // collision auto-rename may pick a different name — a miss there just falls back to the session id.
    note_app_op(&app, || plan_watch_action_targets(&path, &actions));
    let ctx = server_ctx::TauriCtx::new(&app);
    tauri::async_runtime::spawn_blocking(move || run_watch_actions_impl(&ctx, path, actions))
        .await
        .unwrap_or_default()
}

/// Pure best-effort planner for `run_watch_actions`' app-op ledger entries (CPE-1102): simulate the
/// pipeline's resolved destination for each step without touching the filesystem, mirroring
/// `run_watch_actions_impl`'s `current`-path threading (`move`/`rename` relocate it, `copy` doesn't).
/// Split out from the command so it's unit-testable without a `tauri::AppHandle`.
fn plan_watch_action_targets(path: &str, actions: &[WatchAction]) -> Vec<String> {
    let mut current = PathBuf::from(path);
    let mut targets = Vec::with_capacity(actions.len());
    for action in actions {
        match action.kind.as_str() {
            "move" | "copy" => {
                let Some(name) = current.file_name() else { continue };
                let dest = Path::new(&action.resolved).join(name);
                targets.push(dest.to_string_lossy().into_owned());
                if action.kind == "move" {
                    current = dest;
                }
            }
            "rename" => {
                let Some(parent) = current.parent() else { continue };
                let dest = parent.join(action.resolved.trim());
                targets.push(dest.to_string_lossy().into_owned());
                current = dest;
            }
            _ => {}
        }
    }
    targets
}

fn run_watch_actions_impl(ctx: &dyn ServerCtx, path: String, actions: Vec<WatchAction>) -> Vec<OpResult> {
    let mut current = PathBuf::from(&path);
    let mut out = Vec::with_capacity(actions.len());
    for action in &actions {
        let result: Result<PathBuf, String> = match action.kind.as_str() {
            "move" => do_move_into(ctx, &current, Path::new(&action.resolved)),
            "copy" => do_copy_into(&current, Path::new(&action.resolved)),
            "rename" => rename_entry_impl(ctx, current.to_string_lossy().to_string(), action.resolved.clone())
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
#[cfg_attr(feature = "specta-bindings", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
enum TransferKind {
    Copy,
    Move,
}

/// How a name collision at the destination is resolved for the whole batch.
#[derive(Clone, Copy, PartialEq, serde::Deserialize)]
#[cfg_attr(feature = "specta-bindings", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
enum ConflictPolicy {
    /// Replace the existing entry.
    Overwrite,
    /// Leave the existing entry; don't transfer this source.
    Skip,
    /// Keep both — auto-number the new one ("name (2)").
    Keepboth,
}

/// What operation produced a `TransferProgress`/`TransferReport` row, for the operations panel's label
/// and icon (CPE-1184). Distinct from `TransferKind` (which only steers the copy/move engine's own
/// behaviour) — this is a pure UI/reporting discriminant, extended here to cover archive compress/
/// extract now routed through the same queue (`start_archive_compress`/`start_archive_extract`).
#[derive(Clone, Copy, PartialEq, Default, serde::Serialize)]
#[cfg_attr(feature = "specta-bindings", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
enum TransferOp {
    #[default]
    Copy,
    Move,
    Compress,
    Extract,
}

impl From<TransferKind> for TransferOp {
    fn from(k: TransferKind) -> Self {
        match k {
            TransferKind::Copy => TransferOp::Copy,
            TransferKind::Move => TransferOp::Move,
        }
    }
}

/// A progress snapshot emitted while a transfer runs.
#[derive(Clone, serde::Serialize)]
#[cfg_attr(feature = "specta-bindings", derive(specta::Type))]
struct TransferProgress {
    id: u64,
    op: TransferOp,
    total_bytes: u64,
    done_bytes: u64,
    total_items: u64,
    done_items: u64,
    current: String,
}

impl TransferProgress {
    /// Translate an archive engine's [`cpe_server::archive::ArchiveProgress`] into the shape the
    /// operations panel already understands, attaching the app-assigned transfer `id` + `op` the
    /// domain layer doesn't (and shouldn't) know about (CPE-1184).
    fn from_archive(id: u64, op: TransferOp, p: &cpe_server::archive::ArchiveProgress) -> Self {
        TransferProgress {
            id,
            op,
            total_bytes: p.total_bytes,
            done_bytes: p.done_bytes,
            total_items: p.total_items,
            done_items: p.done_items,
            current: p.current.clone(),
        }
    }
}

/// The final outcome of a transfer.
#[derive(Clone, Default, serde::Serialize)]
#[cfg_attr(feature = "specta-bindings", derive(specta::Type))]
struct TransferReport {
    id: u64,
    op: TransferOp,
    transferred: u64,
    skipped: u64,
    failed: u64,
    cancelled: bool,
    errors: Vec<String>,
}

impl TransferReport {
    /// Translate an archive engine's [`cpe_server::archive::ArchiveReport`] into the shape the
    /// operations panel already understands (CPE-1184). `skipped` stays 0 — archive ops don't have a
    /// per-item conflict policy like copy/move; an unsafe/zip-slip entry is recorded in `errors`
    /// instead (mirroring the one-shot extractors' silent-skip).
    fn from_archive(id: u64, op: TransferOp, r: cpe_server::archive::ArchiveReport) -> Self {
        TransferReport {
            id,
            op,
            transferred: r.done,
            skipped: 0,
            failed: r.failed,
            cancelled: r.cancelled,
            errors: r.errors,
        }
    }

    /// A report for an archive op that failed before or during the run (couldn't even start, or the
    /// underlying `?` aborted) — same shape a `Err(e)` from the one-shot commands used to surface,
    /// just delivered as a `transfer://done` event instead of a rejected promise (CPE-1184).
    fn archive_failed(id: u64, op: TransferOp, e: String) -> Self {
        TransferReport { id, op, transferred: 0, skipped: 0, failed: 1, cancelled: false, errors: vec![e] }
    }
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
        op: TransferOp::from(kind),
        total_bytes: measured.iter().map(|(b, _)| b).sum(),
        done_bytes: 0,
        total_items: measured.iter().map(|(_, f)| f).sum(),
        done_items: 0,
        current: String::new(),
    };
    let mut report = TransferReport { id, op: TransferOp::from(kind), ..Default::default() };
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
    // Record the planned per-source destination up front, before the transfer thread starts, so its
    // watcher events read `actor:"user"` (CPE-1102). Best-effort: the engine's collision auto-rename
    // (`resolve_conflict`) may pick a different name than `dest/<name>`, in which case the miss just
    // falls back to the session id — still honest. A Move also removes each source.
    note_app_op(&app, || {
        let mut targets: Vec<String> = sources
            .iter()
            .filter_map(|p| Path::new(p).file_name().map(|n| Path::new(&dest).join(n)))
            .map(|t| t.to_string_lossy().into_owned())
            .collect();
        if kind == TransferKind::Move {
            targets.extend(sources.iter().cloned());
        }
        targets
    });
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

/// Signal a running transfer to stop at the next chunk boundary (CPE-620). Also cancels an archive
/// compress/extract queued via [`start_archive_compress`]/[`start_archive_extract`] — they share this
/// same registry, so no separate cancel command was needed for them (CPE-1184).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn cancel_transfer(id: u64) {
    use std::sync::atomic::Ordering;
    if let Some(flag) = transfer_registry().lock().unwrap().get(&id) {
        flag.store(true, Ordering::Relaxed);
    }
}

// ---- Archive compress/extract through the transfer queue (CPE-1184, epic CPE-705) ------------
//
// `compress_to_zip`/`compress_archive`/`compress_to_zip_encrypted`/`extract_archive`/
// `extract_zip_encrypted` above are one-shot blocking calls — fine for a small archive, but a large one
// freezes the UI with no progress and no way to cancel, against the streaming-liveness convention. The
// two commands below queue the SAME work through `cpe_server::archive`'s streamed functions on a
// background thread, reusing the copy/move transfer engine's `transfer://progress`/`transfer://done`
// events + `TRANSFER_CANCELS` registry + id sequence, so archive ops show up in the same operations
// panel, are cancellable the same way, and queue alongside copies/moves. The one-shot commands above are
// left untouched for any other caller.

/// Start a compress of `paths` into `dest` through the transfer queue (CPE-1184): streams per-entry
/// progress and is cancellable exactly like a copy/move. `password` (non-empty) uses the AES-256
/// encrypted zip path; otherwise the format is picked by `dest`'s extension (`.zip`/`.tar.gz`/`.tgz`),
/// mirroring [`compress_archive`]. Returns the new transfer's id immediately — progress/completion
/// arrive as `transfer://progress`/`transfer://done` events, same as `start_transfer`.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn start_archive_compress(
    app: tauri::AppHandle,
    paths: Vec<String>,
    dest: String,
    password: Option<String>,
) -> Result<u64, String> {
    if paths.is_empty() {
        return Err("nothing to compress".into());
    }
    use std::sync::atomic::Ordering;
    let id = TRANSFER_SEQ.fetch_add(1, Ordering::Relaxed);
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    transfer_registry().lock().unwrap().insert(id, cancel.clone());
    note_app_op(&app, || vec![dest.clone()]);
    let ctx = server_ctx::TauriCtx::new(&app);
    std::thread::spawn(move || {
        let emit = |p: &cpe_server::archive::ArchiveProgress| {
            let payload = TransferProgress::from_archive(id, TransferOp::Compress, p);
            let _ = ctx.emit_json("transfer://progress", serde_json::to_value(payload).unwrap_or_default());
        };
        let result = match password.filter(|p| !p.is_empty()) {
            Some(pw) => cpe_server::archive::compress_to_zip_encrypted_streamed(&paths, &dest, &pw, &cancel, emit),
            None => cpe_server::archive::compress_archive_streamed(&paths, &dest, &cancel, emit),
        };
        let report = match result {
            Ok(r) => TransferReport::from_archive(id, TransferOp::Compress, r),
            Err(e) => TransferReport::archive_failed(id, TransferOp::Compress, e),
        };
        let _ = ctx.emit_json("transfer://done", serde_json::to_value(&report).unwrap_or_default());
        transfer_registry().lock().unwrap().remove(&id);
    });
    Ok(id)
}

/// Start an extract of `path` into `dest` through the transfer queue (CPE-1184): streams per-entry
/// progress and is cancellable exactly like a copy/move. `password` (non-empty) uses the AES-zip path;
/// otherwise the format is auto-detected by extension, mirroring [`extract_archive`]. Before queuing,
/// a zip-family archive's password is validated up-front via
/// [`cpe_server::archive::check_zip_password`] — a cheap header-only check — so the frontend's existing
/// password-prompt-and-retry flow keeps its original try/catch shape (an instant rejection) instead of
/// round-tripping through a transfer id + completion event just to learn a password was wrong. That
/// check opens the zip and reads its central directory (real disk I/O), so it runs off the main thread
/// via `spawn_blocking` per CPE-760/761 — hence this command is `async`, matching every sibling archive
/// command. Returns the new transfer's id immediately once the check passes.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn start_archive_extract(
    app: tauri::AppHandle,
    path: String,
    dest: String,
    password: Option<String>,
) -> Result<u64, String> {
    let lower = path.to_lowercase();
    let is_zip_family = !(lower.ends_with(".tar")
        || lower.ends_with(".tar.gz")
        || lower.ends_with(".tgz")
        || lower.ends_with(".gz")
        || lower.ends_with(".7z"));
    if is_zip_family {
        // Real disk I/O (open zip + read central directory), so run it off the main thread via
        // spawn_blocking (CPE-760/761), matching extract_archive and every other archive command.
        let path_check = path.clone();
        let pw_check = password.clone();
        tauri::async_runtime::spawn_blocking(move || {
            cpe_server::archive::check_zip_password(&path_check, pw_check.as_deref())
        })
        .await
        .map_err(|e| e.to_string())??;
    }
    use std::sync::atomic::Ordering;
    let id = TRANSFER_SEQ.fetch_add(1, Ordering::Relaxed);
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    transfer_registry().lock().unwrap().insert(id, cancel.clone());
    // Coarse best-effort record (CPE-1102), mirroring `extract_archive`: record just the `dest` root.
    note_app_op(&app, || vec![dest.clone()]);
    let ctx = server_ctx::TauriCtx::new(&app);
    std::thread::spawn(move || {
        let emit = |p: &cpe_server::archive::ArchiveProgress| {
            let payload = TransferProgress::from_archive(id, TransferOp::Extract, p);
            let _ = ctx.emit_json("transfer://progress", serde_json::to_value(payload).unwrap_or_default());
        };
        let result = match password.filter(|p| !p.is_empty()) {
            Some(pw) => cpe_server::archive::extract_zip_encrypted_streamed(&path, &dest, &pw, &cancel, emit),
            None => cpe_server::archive::extract_archive_streamed(&path, &dest, &cancel, emit),
        };
        let report = match result {
            Ok(r) => TransferReport::from_archive(id, TransferOp::Extract, r),
            Err(e) => TransferReport::archive_failed(id, TransferOp::Extract, e),
        };
        let _ = ctx.emit_json("transfer://done", serde_json::to_value(&report).unwrap_or_default());
        transfer_registry().lock().unwrap().remove(&id);
    });
    Ok(id)
}

/// Move each `from` to an EXACT `to` path. Used by undo, which must restore an
/// item to its original name — auto-renaming here would defeat the point (undo
/// of "rename a -> b" must produce "a", not "a - Copy").
///
/// Refuses to overwrite: if `to` already exists, the undo fails loudly rather
/// than clobbering whatever now occupies that name.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn move_exact(app: tauri::AppHandle, pairs: Vec<(String, String)>) -> Vec<OpResult> {
    // Undo/redo restores exact names — both the `from` (removed) and `to` (created) are ours (CPE-1101).
    note_app_op(&app, || {
        let mut targets = Vec::with_capacity(pairs.len() * 2);
        for (from, to) in &pairs {
            targets.push(from.clone());
            targets.push(to.clone());
        }
        targets
    });
    let ctx = server_ctx::TauriCtx::new(&app);
    tauri::async_runtime::spawn_blocking(move || move_exact_impl(&ctx, pairs))
        .await.unwrap()
}

/// `ctx` re-keys any tag-store entries under each successfully-moved `from` to its `to` (CPE-1222) —
/// including undo's move-back, which previously never migrated tags at all — and likewise re-keys any
/// scheduled-snapshot catalog entry under `from` (CPE-1225).
fn move_exact_impl(ctx: &dyn ServerCtx, pairs: Vec<(String, String)>) -> Vec<OpResult> {
    pairs
        .iter()
        .map(|(from, to)| {
            let src = Path::new(from);
            let dst = Path::new(to);
            if let Err(e) =
                cpe_server::fs_route::require_local(from).and(cpe_server::fs_route::require_local(to))
            {
                return OpResult::err(src, e);
            }
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
                Ok(()) => {
                    let _ = cpe_server::tags::retag(ctx, from, to);
                    let _ = cpe_server::snapshot_schedule::reschedule(ctx, from, to);
                    OpResult::ok(dst)
                }
                Err(e) => OpResult::err(src, e),
            }
        })
        .collect()
}

/// Plan a batch-media job (CPE-1092, epic CPE-723): the backend enablement for the Batch-Media dialog
/// (GUI #2). Validates the job (rejects an empty op list, a bad rotate angle, an empty convert extension
/// or rename template) and, when valid, computes each input's collision-safe planned output path + a
/// one-line summary — this IS the dialog's live preview data. Pure/in-memory; thin dispatcher into
/// `cpe_server::batch_media`, kept `async` for convention parity with the other filesystem commands.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn batch_media_plan(
    job: cpe_server::batch_media::BatchJob,
    inputs: Vec<String>,
) -> Result<Vec<cpe_server::batch_media::PlannedItem>, String> {
    cpe_server::batch_media::validate(&job)?;
    Ok(cpe_server::batch_media::plan(&job, &inputs))
}

/// Streamed batch-media execute (CPE-1092): runs a previously computed plan on a blocking thread and
/// sends each file's `OpResult` over `on_result` in batches of 16 as it completes, mirroring
/// `apply_backup_plan_stream` above. `on_result` is a raw transport channel — the Batch-Media dialog
/// renders its own progress, so this is not routed through the busy-cursor wrapper. Returns the aggregate
/// `BatchReport` once every item has run. No cancellation in v1.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn batch_media_execute_stream(
    items: Vec<cpe_server::batch_media::PlannedItem>,
    job: cpe_server::batch_media::BatchJob,
    on_result: tauri::ipc::Channel<Vec<OpResult>>,
) -> Result<cpe_server::batch_execute::BatchReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut batch: Vec<OpResult> = Vec::new();
        let report = cpe_server::batch_execute::execute_plan_walk(&items, &job, |r| {
            batch.push(r);
            if batch.len() >= 16 {
                let _ = on_result.send(std::mem::take(&mut batch));
            }
        });
        if !batch.is_empty() {
            let _ = on_result.send(batch);
        }
        report
    })
    .await
    .map_err(|e| e.to_string())
}

/// Detailed metadata for the Properties dialog. Model lives in `cpe_server::model` (CPE-815); this is a
/// thin `spawn_blocking` dispatcher.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn entry_info(path: String) -> Result<EntryInfo, String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::model::entry_info(&path))
        .await.map_err(|e| e.to_string())?
}

/// Image dimensions + basic EXIF for the Properties dialog (CPE-659). Model lives in
/// `cpe_server::image_preview` (CPE-815); this is a thin `spawn_blocking` dispatcher.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn image_meta(path: String) -> Result<cpe_server::image_preview::ImageMeta, String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::image_preview::image_meta(&path))
        .await.map_err(|e| e.to_string())?
}

/// Read *all* of a file's editable metadata for the Metadata Studio (CPE-1041, epic CPE-725), dispatched by
/// extension across the read codecs (ID3/Vorbis/EXIF/PDF/video). Thin `spawn_blocking` dispatcher into
/// `cpe_server::media_meta::read_all`; a kind with no codec yields an empty list.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn metadata_read(path: String) -> Result<Vec<cpe_server::media_meta_edit::MetaField>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
        let ext = std::path::Path::new(&path)
            .extension().and_then(|e| e.to_str()).unwrap_or("").to_string();
        Ok(cpe_server::media_meta::read_all(&ext, &bytes))
    })
    .await.map_err(|e| e.to_string())?
}

/// Whether a file's format has a metadata write-back codec today (mp3/flac) — the studio uses this to offer
/// editing vs read-only view (CPE-1041). Thin dispatcher into `cpe_server::media_meta::is_writable`.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn metadata_writable(path: String) -> Result<bool, String> {
    let ext = std::path::Path::new(&path)
        .extension().and_then(|e| e.to_str()).unwrap_or("").to_string();
    Ok(cpe_server::media_meta::is_writable(&ext))
}

/// Apply `edits` to a file's metadata and save them back atomically (CPE-1041, epic CPE-725): read current
/// fields, apply the edit policy, re-serialise with the format's write codec, then write via a temp file +
/// rename so a mid-write failure never truncates the original. Returns the re-read fields so the studio
/// refreshes. `Err` for a format with no writer yet.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn metadata_write(
    path: String,
    edits: Vec<cpe_server::media_meta_edit::MetaEdit>,
) -> Result<Vec<cpe_server::media_meta_edit::MetaField>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
        let ext = std::path::Path::new(&path)
            .extension().and_then(|e| e.to_str()).unwrap_or("").to_string();
        let out = cpe_server::media_meta::write_back(&ext, &bytes, &edits)?;
        // Atomic save: write a sibling temp file, then rename over the original (same dir → rename is
        // atomic on all three OSes). A crash mid-write leaves the original intact. The temp name carries a
        // per-write nanosecond stamp so two concurrent saves (or a stale temp from a prior crash) can't
        // collide on the same sibling path.
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let tmp = format!("{path}.{stamp}.cpe-meta-tmp");
        std::fs::write(&tmp, &out).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp); // don't leave the temp behind on a failed rename
            e.to_string()
        })?;
        Ok(cpe_server::media_meta::read_all(&ext, &out))
    })
    .await.map_err(|e| e.to_string())?
}

/// Every metadata column the details-view picker can offer (CPE-1145, epic CPE-707): a stable id (for
/// `column_config` persistence), a friendly label, the typed `MetaColumn` to hand back to
/// `metadata_column_cells`, and the extensions it applies to. In-memory enumeration only, no I/O — thin
/// dispatcher into `cpe_server::column_cells::available_columns`.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn metadata_columns_available() -> Vec<cpe_server::column_cells::AvailableColumn> {
    cpe_server::column_cells::available_columns()
}

/// Streamed metadata-column fill for a listing (CPE-1145, epic CPE-707): for each of `paths`, reads a
/// capped header and extracts `column` (via `cpe_server::column_extract::extract_column`), pushing
/// batches of `{ path, cell, display }` over `on_cell` as they resolve — so a column added to a big
/// listing paints visible rows fast (STREAMING.md). Skip-on-error per file: an unreadable/undecodable
/// file yields an empty cell, never a failed batch. Async + `spawn_blocking` (the walk does real file
/// I/O). Shares `stream_column_cells` with the collect-to-vec variant below, so the two can never
/// diverge. Returns the total number of cells emitted.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn metadata_column_cells(
    paths: Vec<String>,
    column: cpe_server::column_extract::MetaColumn,
    on_cell: tauri::ipc::Channel<Vec<cpe_server::column_cells::MetadataCell>>,
) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut emitted = 0usize;
        cpe_server::column_cells::stream_column_cells(
            &paths,
            column,
            cpe_server::column_cells::COLUMN_CELLS_BATCH,
            |batch| {
                emitted += batch.len();
                let _ = on_cell.send(batch);
                std::ops::ControlFlow::Continue(())
            },
        );
        Ok(emitted)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Collect-to-vec variant of `metadata_column_cells` (tests + non-streaming callers): returns every cell
/// directly instead of streaming batches. Same walk, so results always match `metadata_column_cells`.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn metadata_column_cells_collect(
    paths: Vec<String>,
    column: cpe_server::column_extract::MetaColumn,
) -> Vec<cpe_server::column_cells::MetadataCell> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::column_cells::column_cells(&paths, column))
        .await
        .unwrap_or_default()
}

/// Recursive counts + size of a directory tree, for the Properties dialog (CPE-649): number of files,
/// number of sub-folders, and total bytes. Cycle-safe (doesn't follow symlinked dirs) and bounded —
/// stops at a large entry cap (reporting `truncated`) so it can't spin on a pathological tree.
/// Model lives in `cpe_server::folder_stats` (CPE-815); this is a thin `spawn_blocking` dispatcher.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn folder_stats(path: String) -> Result<cpe_server::folder_stats::FolderStats, String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::folder_stats::compute(&path))
        .await.map_err(|e| e.to_string())?
}

/// Total recursive size of a directory tree in bytes. Model lives in `cpe_server::disk_usage`
/// (CPE-815); this is a thin `spawn_blocking` dispatcher.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn dir_size(path: String) -> Result<u64, String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::disk_usage::dir_size(&path))
        .await.map_err(|e| e.to_string())?
}

/// The immediate children of `path`, each with its recursive size — the per-child breakdown the treemap
/// needs for the space analyzer (CPE-749). Model lives in `cpe_server::disk_usage` (CPE-815); this is a
/// thin `spawn_blocking` dispatcher.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn dir_children_sizes(path: String) -> Result<Vec<cpe_server::disk_usage::ChildSize>, String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::disk_usage::dir_children_sizes(&path))
        .await.map_err(|e| e.to_string())?
}

/// Streaming variant of `dir_children_sizes` (CPE-706, streaming-liveness convention): pushes each direct
/// child's recursive size over an IPC channel as it's computed (in parallel), so the space-analyzer
/// treemap fills in progressively instead of blocking on the whole scan. Async + `spawn_blocking` so the
/// scan never freezes the UI thread (CPE-760); the walk is the shared `cpe_server::disk_usage::
/// stream_children_sizes` (CPE-815). Children arrive in completion order; the reactive treemap re-lays-out.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn dir_children_sizes_stream(
    path: String,
    on_child: tauri::ipc::Channel<Vec<cpe_server::disk_usage::ChildSize>>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        cpe_server::disk_usage::stream_children_sizes(&path, |cs| {
            let _ = on_child.send(vec![cs]);
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Compute the SHA-256 checksum of a file, returned as lowercase hex (CPE-412). Streamed in fixed
/// chunks so a multi-GB file never loads into memory. A directory, missing, or unreadable path is an
/// `Err`, never a panic. Opt-in from the UI (hashing is I/O-bound) — never run automatically.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn apply_backup_plan_stream(
    source_root: String,
    dest_root: String,
    copy: Vec<String>,
    update: Vec<String>,
    delete_paths: Vec<String>,
    verify: bool,
    on_result: tauri::ipc::Channel<Vec<OpResult>>,
) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut batch: Vec<OpResult> = Vec::new();
        let mut total = 0usize;
        cpe_server::backup::apply_backup_plan_walk(&source_root, &dest_root, &copy, &update, &delete_paths, verify, |r| {
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn apply_backup_plan(
    source_root: String,
    dest_root: String,
    copy: Vec<String>,
    update: Vec<String>,
    delete_paths: Vec<String>,
    verify: bool,
) -> Vec<OpResult> {
    tauri::async_runtime::spawn_blocking(move || {
        cpe_server::backup::apply_backup_plan(&source_root, &dest_root, &copy, &update, &delete_paths, verify)
    })
    .await
    .unwrap_or_default()
}

/// Recursively checksum every file under `path` into a baseline manifest — the on-demand baseline for
/// the integrity guard (CPE-791). Symlinks are not followed and unreadable files are skipped; the result
/// is sorted by path for a stable diff. Model lives in `cpe_server::checksum` (CPE-815).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn checksum_folder(path: String) -> Result<Vec<cpe_server::checksum::ChecksumEntry>, String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::checksum::checksum_folder(&path))
        .await
        .map_err(|e| e.to_string())?
}

/// Re-scan `path` and classify it against a stored `baseline` in one backend pass (CPE-870, epic CPE-737):
/// returns only the compact integrity report (changed paths) instead of shipping the whole manifest to the
/// webview to diff — so large trees stay responsive, and verification can run headlessly. Model in
/// `cpe_server::checksum` (CPE-815).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn verify_folder(
    path: String,
    baseline: Vec<cpe_server::checksum::ChecksumEntry>,
) -> Result<cpe_server::checksum::IntegrityReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let current = cpe_server::checksum::checksum_folder(&path)?;
        Ok(cpe_server::checksum::verify_manifest(&baseline, &current))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Verify every baselined folder in one pass (CPE-871, epic CPE-737): re-scan + classify each against its
/// stored baseline, returning a report per folder. A folder that can't be scanned (deleted/unmounted) is
/// skipped rather than failing the whole sweep — the "monitor all my folders" one-shot behind the
/// "Verify all baselined folders" action. Returns only the compact reports.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn verify_all_baselines(
    baselines: std::collections::HashMap<String, Vec<cpe_server::checksum::ChecksumEntry>>,
) -> Result<std::collections::HashMap<String, cpe_server::checksum::IntegrityReport>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut out = std::collections::HashMap::new();
        for (path, baseline) in baselines {
            if let Ok(current) = cpe_server::checksum::checksum_folder(&path) {
                out.insert(path, cpe_server::checksum::verify_manifest(&baseline, &current));
            }
        }
        out
    })
    .await
    .map_err(|e| e.to_string())
}

// ---- Folder-tree scan for the compare view (CPE-779, epic CPE-722) --------------------------------
// Recursively scan a folder into a nested tree the frontend `diffTrees` (CPE-777) consumes: files carry
// size + epoch-ms mtime (what the diff compares on), dirs carry children. Symlinks aren't followed and
// unreadable entries/dirs are skipped (matching `checksum_walk`/`dir_size`). Bounded by `max_depth` so a
// pathological tree can't blow the stack or the payload; beyond the cap a dir is returned with no children.

/// Scan the children of `path` into a `CompareNode`-shaped tree (CPE-779). Model lives in
/// `cpe_server::compare` (CPE-815); this is a thin `spawn_blocking` dispatcher.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn scan_tree(path: String, max_depth: u32) -> Result<Vec<cpe_server::compare::TreeNode>, String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::compare::scan_tree(&path, max_depth))
        .await
        .map_err(|e| e.to_string())?
}

/// Create a symbolic link at `link_path` pointing to `target` (CPE-802, epic CPE-715). On Windows a
/// directory target makes a dir-symlink, else a file-symlink; the OS error is returned on failure (e.g.
/// Windows symlink creation without Developer Mode / admin), so the UI can prompt for elevation.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn create_symlink(target: String, link_path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::links::create_symlink(&target, &link_path))
        .await
        .map_err(|e| e.to_string())?
}

/// Create a hardlink at `link_path` for the same file data as `target` (CPE-802). Cross-platform.
/// Model lives in `cpe_server::links` (CPE-815); this is a thin `spawn_blocking` dispatcher.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn create_hard_link(target: String, link_path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::links::create_hard_link(&target, &link_path))
        .await
        .map_err(|e| e.to_string())?
}

/// Create a Windows directory junction at `link_path` pointing to `target` (CPE-1210, epic CPE-715). A
/// junction needs no Developer Mode / elevation (unlike a symlink) but only ever targets a directory.
/// On non-Windows this always returns a clear "Windows-only" error — no reparse-point concept exists
/// there. Model in `cpe_server::links` (CPE-815); this is a thin `spawn_blocking` dispatcher.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn create_junction(target: String, link_path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::links::create_junction(&target, &link_path))
        .await
        .map_err(|e| e.to_string())?
}

/// Inspect `path` — is it a symlink, its target, and whether that target is missing (a broken link)
/// (CPE-804, epic CPE-715). Never fails. Model in `cpe_server::links` (CPE-815).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn link_status(path: String) -> cpe_server::links::LinkStatus {
    tauri::async_runtime::spawn_blocking(move || cpe_server::links::link_status(&path))
        .await
        .unwrap_or_default()
}

/// Suggest a repair target for a broken symlink (CPE-1027, epic CPE-715): reads `broken_link`'s stored
/// target's basename and searches `search_roots` (in order, each a bounded-depth walk) for the first
/// matching entry. Returns `None` if `broken_link` isn't a readable symlink or nothing matches. Model in
/// `cpe_server::links` (CPE-815); this is a thin `spawn_blocking` dispatcher.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn suggest_repair(broken_link: String, search_roots: Vec<String>) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || {
        let roots: Vec<&str> = search_roots.iter().map(String::as_str).collect();
        cpe_server::links::suggest_repair(&broken_link, &roots)
    })
    .await
    .unwrap_or(None)
}

/// Install the "Open in Cross-Platform Explorer" shell integration for the current user (CPE-1020, epic
/// CPE-712) — writes the registry entries for the running exe. Windows-only today; other OSes return an
/// error. Logic in `cpe_server::shell_menu`.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn install_shell_integration() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?.to_string_lossy().into_owned();
    tauri::async_runtime::spawn_blocking(move || {
        cpe_server::shell_menu::install_shell_integration(&exe, "Cross-Platform Explorer")
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Remove the "Open in CPE" shell integration for the current user (CPE-1020). Idempotent — safe when
/// nothing is installed. Logic in `cpe_server::shell_menu`.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn uninstall_shell_integration() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(cpe_server::shell_menu::uninstall_shell_integration)
        .await
        .map_err(|e| e.to_string())?
}

/// Whether the "Open in CPE" shell integration is currently installed for this user (CPE-1020) — so the
/// Settings toggle (CPE-1023) can reflect true state. Logic in `cpe_server::shell_menu`.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn shell_integration_installed() -> bool {
    tauri::async_runtime::spawn_blocking(cpe_server::shell_menu::shell_integration_installed)
        .await
        .unwrap_or(false)
}

/// Claim an OS-wide global hotkey that summons Spotlight (CPE-1215, epic CPE-704): pressing `chord`
/// (Tauri accelerator syntax, e.g. `"CommandOrControl+Shift+Space"`) fires the `spotlight:open` event,
/// which the CPE-1216 overlay listens for — so Spotlight can be opened even while the main window is
/// hidden/unfocused. Driven ONLY by the Settings toggle (never a launch-time permission prompt,
/// [[avoid-modal-permission-popups]]); the plugin itself is always initialized but registers nothing
/// until this is called. Idempotent: any stale registration under the same chord is cleared first, so
/// re-registering (e.g. after an app restart with the setting already on) never errors as "already
/// registered". Desktop-only — mobile has no global-shortcut surface.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn register_spotlight_hotkey(app: tauri::AppHandle, chord: String) -> Result<(), String> {
    use tauri::Emitter;
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    tauri::async_runtime::spawn_blocking(move || {
        let gs = app.global_shortcut();
        let _ = gs.unregister(chord.as_str()); // idempotent: drop any stale registration first
        gs.on_shortcut(chord.as_str(), |ah, _shortcut, event| {
            if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                let _ = ah.emit("spotlight:open", ());
            }
        })
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Release the Spotlight global hotkey registered by [`register_spotlight_hotkey`] (CPE-1215, epic
/// CPE-704) — called when the Settings toggle goes off, so an unused hotkey costs the OS nothing.
/// Idempotent: unregistering a chord that isn't currently claimed (already off, or a chord that failed
/// to register) is treated as success rather than surfaced as an error, since the caller's goal —
/// "this chord is not claimed by us" — is already true.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn unregister_spotlight_hotkey(app: tauri::AppHandle, chord: String) -> Result<(), String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    tauri::async_runtime::spawn_blocking(move || {
        let _ = app.global_shortcut().unregister(chord.as_str());
    })
    .await
    .map_err(|e| e.to_string())
}

/// Classify the drive a path lives on (CPE-805, epic CPE-716) — fixed / removable / network / cdrom / ram
/// / unknown — so the sidebar can badge removable & network drives. Windows uses `GetDriveTypeW`; unix
/// returns a best-effort `fixed` for now (richer classification is a follow-up).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn audit_record(
    app: tauri::AppHandle,
    session: String,
    kind: String,
    path: String,
    detail: Option<String>,
) -> Result<(), String> {
    let dir = audit_dir(&app)?;
    let ts = to_epoch_ms(SystemTime::now()).unwrap_or(0);
    let event = audit_journal::AuditEvent { ts, session, kind, path, actor: None, detail };
    tauri::async_runtime::spawn_blocking(move || {
        audit_journal::record(&dir, &event, audit_journal::MAX_EVENTS_PER_SESSION)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// List the session ids that have a persisted journal (most useful sorted; newest-first is the UI's job).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn audit_sessions(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    let dir = audit_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || audit_journal::list_sessions(&dir))
        .await
        .map_err(|e| e.to_string())
}

/// Read every event for one past session back (append order; malformed lines skipped).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn audit_read(
    app: tauri::AppHandle,
    session: String,
) -> Result<Vec<audit_journal::AuditEvent>, String> {
    let dir = audit_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || audit_journal::read_session(&dir, &session))
        .await
        .map_err(|e| e.to_string())
}

// ---- Per-session cost/activity metrics journal (CPE-1113, epic CPE-731 slice b) -------------------
// A SIBLING of the audit journal above — same durable-JSONL pattern, coarser grain (one row per session,
// not per event) and a SEPARATE file (`agent-metrics/history.jsonl`, never the audit file). Written only
// when a watched session ENDS (the frontend flushes one record from its live accumulator before the
// stores clear); `metrics_history` is pull-only (read when the cost dashboard opens). Advisory /
// best-effort figures — never billing. Costs nothing when Agent Watch is off ("off means off").

/// Resolve (and create) the metrics-journal directory under the app-data dir (sibling of `audit_dir`).
fn metrics_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = server_ctx::TauriCtx::new(app).app_data_dir()?.join("agent-metrics");
    Ok(dir)
}

/// Append one ended session's metrics row to the bounded/rotated history journal. The record is built
/// frontend-side from the live accumulator at session end (advisory/best-effort figures).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn metrics_record(
    app: tauri::AppHandle,
    rec: metrics_journal::SessionMetricsRecord,
) -> Result<(), String> {
    let dir = metrics_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        metrics_journal::append(&dir, &rec, metrics_journal::MAX_SESSIONS)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Read every persisted session metrics row back (append order; malformed lines skipped). Pull-only —
/// called when the cross-session cost dashboard opens (CPE-731c). Missing journal → empty.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn metrics_history(
    app: tauri::AppHandle,
) -> Result<Vec<metrics_journal::SessionMetricsRecord>, String> {
    let dir = metrics_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || metrics_journal::read_all(&dir))
        .await
        .map_err(|e| e.to_string())
}

/// Everything the Replay tab needs for one session, pulled once on tab-open (CPE-1110, epic CPE-728
/// slice **c**): the session's durable audit journal packaged for replay
/// (`replay_session::load_replay` — events in journal order + `(min,max)` ts bounds + whole-run
/// summary) plus its baseline snapshot (`replay_baseline::read_baseline`). The frontend seeds the fold
/// from `baseline` and re-derives the folder listing per scrub tick entirely client-side
/// (`src/lib/replayFold.ts`), so scrubbing costs no IPC round-trip per tick.
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "specta-bindings", derive(specta::Type))]
struct ReplayLoad {
    /// The session's events (journal order) + `(min,max)` ts bounds + whole-run summary.
    replay: cpe_server::replay_session::ReplayData,
    /// The pre-existing-entries snapshot captured at watch-start, or `None` if the session never wrote
    /// one — the fold then degrades to events-only reconstruction (an empty seed).
    baseline: Option<cpe_server::replay_baseline::Baseline>,
}

/// Load a past session's replay data + baseline for the Replay tab. Thin `spawn_blocking` shell over the
/// two already-tested `cpe_server` readers (mirrors `audit_read`). PULL-ONLY — called when the Replay
/// tab opens; nothing runs while it's closed, so replay is zero-cost with Agent Watch off (off-means-off).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn replay_load(app: tauri::AppHandle, session: String) -> Result<ReplayLoad, String> {
    let dir = audit_dir(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let replay = cpe_server::replay_session::load_replay(&dir, &session);
        let baseline = cpe_server::replay_baseline::read_baseline(&dir, &session);
        ReplayLoad { replay, baseline }
    })
    .await
    .map_err(|e| e.to_string())
}

// ---- Checkpoint & rollback (CPE-1123, epic CPE-732) -----------------------------------------------
// Thin `spawn_blocking` dispatchers over `cpe_server::checkpoint_store` — the whole checkpoint/rollback
// engine lives in the Server; these commands just resolve `TauriCtx` (which owns the per-root store dir
// under the app-data dir) and delegate. Async per the async-commands guardrail; the store is only touched
// when the user takes/reverts a checkpoint, so it costs nothing when unused (off-means-off).

/// Capture the tree under `root` into its per-root checkpoint store + record a labelled index entry.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn checkpoint_create(
    app: tauri::AppHandle,
    root: String,
    label: String,
) -> Result<cpe_server::checkpoint_store::CheckpointCreated, String> {
    let ctx = server_ctx::TauriCtx::new(&app);
    tauri::async_runtime::spawn_blocking(move || {
        cpe_server::checkpoint_store::checkpoint_create(&ctx, &root, &label)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The checkpoints recorded for `root`, newest-first.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn checkpoint_list(
    app: tauri::AppHandle,
    root: String,
) -> Result<Vec<cpe_server::checkpoint_store::Checkpoint>, String> {
    let ctx = server_ctx::TauriCtx::new(&app);
    tauri::async_runtime::spawn_blocking(move || cpe_server::checkpoint_store::checkpoint_list(&ctx, &root))
        .await
        .map_err(|e| e.to_string())?
}

/// Preview reverting `root` to checkpoint `manifest_id`: restore-plan summary + a drift report so the UI
/// can warn before touching disk. Reads only; nothing destructive.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn checkpoint_preview_revert(
    app: tauri::AppHandle,
    root: String,
    manifest_id: String,
    session: Option<String>,
) -> Result<cpe_server::checkpoint_store::RevertPreview, String> {
    let ctx = server_ctx::TauriCtx::new(&app);
    // `Some(session)` folds the watched agent's own touched-set so only paths changed OUTSIDE that
    // agent count as drift (CPE-1151); `None` keeps the conservative "every diverging path is drift"
    // behaviour for callers with no watched session.
    tauri::async_runtime::spawn_blocking(move || {
        cpe_server::checkpoint_store::checkpoint_preview_revert(
            &ctx,
            &root,
            &manifest_id,
            session.as_deref(),
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Revert the whole tree under `root` to checkpoint `manifest_id` (skip-on-error honoured).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn checkpoint_revert(
    app: tauri::AppHandle,
    root: String,
    manifest_id: String,
) -> Result<cpe_server::checkpoint_store::RevertOutcome, String> {
    let ctx = server_ctx::TauriCtx::new(&app);
    tauri::async_runtime::spawn_blocking(move || {
        cpe_server::checkpoint_store::checkpoint_revert(&ctx, &root, &manifest_id)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Cherry-revert a single `path` under `root` to its state in checkpoint `manifest_id`.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn checkpoint_revert_one(
    app: tauri::AppHandle,
    root: String,
    manifest_id: String,
    path: String,
) -> Result<cpe_server::checkpoint_store::RevertOutcome, String> {
    let ctx = server_ctx::TauriCtx::new(&app);
    tauri::async_runtime::spawn_blocking(move || {
        cpe_server::checkpoint_store::checkpoint_revert_one(&ctx, &root, &manifest_id, &path)
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---- Snapshot retention prune (CPE-1196, epic CPE-735) --------------------------------------------
// Thin `spawn_blocking` dispatchers over `cpe_server::checkpoint_store`'s retention wrappers, which in
// turn delegate to `cpe_server::snapshot_prune` (pure store-dir logic) + `snapshot_capture::prune` (the
// manifest-deleted-first invariant, unchanged). `preview` never touches disk; `apply` does.

/// Preview retention-thinning `root`'s checkpoints under `policy`: which manifests would be kept vs.
/// pruned, and the store's current footprint. Read-only.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn snapshot_prune_preview(
    app: tauri::AppHandle,
    root: String,
    policy: cpe_server::snapshot_retention::RetentionPolicy,
) -> Result<cpe_server::snapshot_prune::RetentionPreview, String> {
    let ctx = server_ctx::TauriCtx::new(&app);
    tauri::async_runtime::spawn_blocking(move || {
        cpe_server::checkpoint_store::checkpoint_prune_preview(&ctx, &root, &policy)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Actually retention-prune `root`'s checkpoints to `policy` (+ an optional total-store-byte cap).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn snapshot_prune_apply(
    app: tauri::AppHandle,
    root: String,
    policy: cpe_server::snapshot_retention::RetentionPolicy,
    max_total_bytes: Option<u64>,
) -> Result<cpe_server::snapshot_prune::RetentionApplyResult, String> {
    let ctx = server_ctx::TauriCtx::new(&app);
    tauri::async_runtime::spawn_blocking(move || {
        cpe_server::checkpoint_store::checkpoint_prune_apply(&ctx, &root, &policy, max_total_bytes)
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---- Snapshot per-file diff (CPE-1197 backend half, epic CPE-735) ----------------------------------

/// Diff `rel_path` between checkpoint `manifest_id` (before) and its live state under `root` (after), for
/// the restore-preview's "Open diff" affordance. Reuses `DiffPeek.svelte` on the frontend (a separate,
/// parallel ticket wires that half in).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn checkpoint_diff_file(
    app: tauri::AppHandle,
    root: String,
    manifest_id: String,
    rel_path: String,
) -> Result<cpe_server::checkpoint_store::FileDiff, String> {
    let ctx = server_ctx::TauriCtx::new(&app);
    tauri::async_runtime::spawn_blocking(move || {
        cpe_server::checkpoint_store::checkpoint_diff_file(&ctx, &root, &manifest_id, &rel_path)
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---- Snapshot schedule (CPE-1198, epic CPE-735) ----------------------------------------------------
// Thin dispatchers over `cpe_server::snapshot_schedule`: a persisted per-folder rule store (CRUD) plus
// `snapshot_run_due`, which captures every due root and retention-prunes it. No timer, no UI — a future
// ticket (CPE-1199) supplies the background timer that calls `snapshot_run_due` on an interval.

/// Every stored snapshot-schedule rule.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn snapshot_schedule_list(
    app: tauri::AppHandle,
) -> Result<Vec<cpe_server::snapshot_schedule::ScheduleRule>, String> {
    let ctx = server_ctx::TauriCtx::new(&app);
    tauri::async_runtime::spawn_blocking(move || cpe_server::snapshot_schedule::list_rules(&ctx))
        .await
        .map_err(|e| e.to_string())?
}

/// Save (insert or replace by `root`) a snapshot-schedule rule.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn snapshot_schedule_set(
    app: tauri::AppHandle,
    rule: cpe_server::snapshot_schedule::ScheduleRule,
) -> Result<(), String> {
    let ctx = server_ctx::TauriCtx::new(&app);
    tauri::async_runtime::spawn_blocking(move || cpe_server::snapshot_schedule::set_rule(&ctx, rule))
        .await
        .map_err(|e| e.to_string())?
}

/// Remove `root`'s snapshot-schedule rule, if any.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn snapshot_schedule_remove(app: tauri::AppHandle, root: String) -> Result<(), String> {
    let ctx = server_ctx::TauriCtx::new(&app);
    tauri::async_runtime::spawn_blocking(move || cpe_server::snapshot_schedule::remove_rule(&ctx, &root))
        .await
        .map_err(|e| e.to_string())?
}

/// Capture every root due for a scheduled snapshot (per its rule's interval) and retention-prune it.
/// `now`/`last_run_times` are caller-supplied (epoch seconds) so this stays a deterministic single pass;
/// no timer lives here.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn snapshot_run_due(
    app: tauri::AppHandle,
    now: u64,
    last_run_times: std::collections::BTreeMap<String, u64>,
) -> Result<Vec<cpe_server::snapshot_schedule::RunDueOutcome>, String> {
    let ctx = server_ctx::TauriCtx::new(&app);
    tauri::async_runtime::spawn_blocking(move || {
        cpe_server::snapshot_schedule::snapshot_run_due(&ctx, now, &last_run_times)
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---- Background scheduled-snapshot timer (CPE-1199, epic CPE-735) ----------------------------------
// The app's first generic periodic task beyond the `notify` watchers. A single dedicated thread wakes
// on a deliberately un-tight cadence and runs one `snapshot_schedule_tick` per wake, holding the
// last-run bookkeeping in memory for the app's lifetime. Everything security-sensitive (auto-capture +
// auto-prune) is opt-in per folder via the Settings UI, and OFF MEANS OFF: with no enabled rule every
// wake is a single cheap catalog read that early-returns before touching a folder or the disk.

/// How often the background scheduled-snapshot timer wakes to check for due folders (CPE-1199). A
/// deliberately un-tight cadence: rule intervals are minutes-to-days, and with no enabled rule a wake
/// costs only one cheap catalog read that early-returns, so 60s keeps idle cost negligible while still
/// honouring a rule's interval closely enough.
const SNAPSHOT_SCHEDULE_TICK: std::time::Duration = std::time::Duration::from_secs(60);

/// The current wall-clock time in epoch seconds (the `now` the scheduler's pure core is injected with).
fn now_epoch_s() -> u64 {
    SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// One scheduler tick, pure over an injected [`ServerCtx`] + clock so it's headlessly testable
/// (CPE-1199). **Off means off:** if no rule is enabled the catalog read early-returns before any
/// capture or disk write — a folderless / all-disabled app pays only this one cheap read per wake, and
/// `last_run_times` is left untouched. Otherwise it captures every root due at `now` (via
/// [`cpe_server::snapshot_schedule::snapshot_run_due`], which also retention-prunes each) and records
/// each captured root's run time in `last_run_times`, returning the roots captured this tick. A listing
/// or capture error is swallowed (returns empty) so a bad folder can never propagate out of the timer.
fn snapshot_schedule_tick(
    ctx: &dyn ServerCtx,
    now: u64,
    last_run_times: &mut std::collections::BTreeMap<String, u64>,
) -> Vec<String> {
    // Off means off: a listing error, an empty catalog, or an all-disabled catalog does nothing at all
    // (no capture, no disk growth, no bookkeeping change) — verified by construction here.
    match cpe_server::snapshot_schedule::list_rules(ctx) {
        Ok(rules) if rules.iter().any(|r| r.enabled) => {}
        _ => return Vec::new(),
    }
    let Ok(outcomes) = cpe_server::snapshot_schedule::snapshot_run_due(ctx, now, last_run_times) else {
        return Vec::new();
    };
    let captured: Vec<String> = outcomes.into_iter().map(|o| o.root).collect();
    for root in &captured {
        last_run_times.insert(root.clone(), now);
    }
    captured
}

/// Spawn the background scheduled-snapshot timer (CPE-1199, epic CPE-735). A single dedicated thread
/// sleeps [`SNAPSHOT_SCHEDULE_TICK`] between wakes and runs one [`snapshot_schedule_tick`] per wake,
/// keeping the last-run map in memory for the app's lifetime (so re-arming an already-run rule waits
/// its full interval, rather than re-capturing every 60s). It never blocks startup (spawned, never
/// joined) and swallows per-tick errors, so nothing here can crash the app. Off means off: with no
/// enabled rule each wake is one cheap catalog read that early-returns.
///
/// Bookkeeping is in-memory only, so on a fresh launch a never-run enabled rule is due immediately and
/// captures on the first wake — the intended "snapshot when you open the app" behaviour, kept bounded
/// by each rule's own retention policy.
fn spawn_snapshot_schedule_timer(app: tauri::AppHandle) {
    let _ = std::thread::Builder::new().name("cpe-snapshot-scheduler".into()).spawn(move || {
        let mut last_run_times: std::collections::BTreeMap<String, u64> = std::collections::BTreeMap::new();
        loop {
            std::thread::sleep(SNAPSHOT_SCHEDULE_TICK);
            let ctx = server_ctx::TauriCtx::new(&app);
            let _ = snapshot_schedule_tick(&ctx, now_epoch_s(), &mut last_run_times);
        }
    });
}

/// Line / word / character / byte counts for a text file (CPE-414). Lines follow `str::lines`
/// (a final unterminated line still counts); words are whitespace-separated; characters are Unicode
/// scalar values. Capped so analysing a file stays predictable; a non-UTF-8 (binary) file, a
/// directory, or an over-cap file is an `Err`. Opt-in from the UI, never automatic.
/// Model lives in `cpe_server::text_stats` (CPE-815); this is a thin `spawn_blocking` dispatcher.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn text_stats(path: String) -> Result<cpe_server::text_stats::TextStats, String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::text_stats::compute(&path))
        .await.map_err(|e| e.to_string())?
}

/// Inspect a selected file for the Properties panel: detect its text encoding + line endings, its true
/// type from the magic bytes, and flag a content/extension mismatch (a disguised file). Reads only the
/// file's leading bytes (capped). Model lives in `cpe_server::inspect` (CPE-1009); thin `spawn_blocking`
/// dispatcher that supplies the bytes + name.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn inspect_file(path: String) -> Result<cpe_server::inspect::FileInspection, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use std::io::Read;
        let name = std::path::Path::new(&path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut f = std::fs::File::open(&path).map_err(|e| e.to_string())?;
        // A 64 KiB leading sample is plenty for encoding/magic-bytes/line-ending detection.
        let mut buf = vec![0u8; 64 * 1024];
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        buf.truncate(n);
        Ok(cpe_server::inspect::inspect_bytes(&name, &buf))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Spotlight fuzzy search + aggregation (CPE-1214, epic CPE-704): rank each `sources` group against
/// `query` with `spotlight::rank`, group by kind, keep each section's top `per_kind_cap`, and return
/// non-empty sections ordered by kind priority (Action → Folder → File → Recent). Pure + infallible —
/// still `spawn_blocking`'d since ranking many candidates is real CPU work. Model lives in
/// `cpe_server::spotlight_results::aggregate` (CPE-948); this is a thin dispatcher.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn spotlight_search(
    query: String,
    sources: Vec<(cpe_server::spotlight_results::ResultKind, Vec<String>)>,
    per_kind_cap: usize,
) -> Vec<cpe_server::spotlight_results::SpotSection> {
    tauri::async_runtime::spawn_blocking(move || spotlight_search_impl(query, sources, per_kind_cap))
        .await
        .unwrap()
}

fn spotlight_search_impl(
    query: String,
    sources: Vec<(cpe_server::spotlight_results::ResultKind, Vec<String>)>,
    per_kind_cap: usize,
) -> Vec<cpe_server::spotlight_results::SpotSection> {
    cpe_server::spotlight_results::aggregate(&query, &sources, per_kind_cap)
}

/// Spotlight frecency ranking (CPE-1214, epic CPE-704): rank `visits` best-first by frecency
/// (visit count × recency decay) as of `now_s`, returning up to `limit` paths — the overlay's
/// empty-query default view. Pure + infallible. Model lives in
/// `cpe_server::spotlight_frecency::rank_frecent` (CPE-952); this is a thin dispatcher.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn spotlight_frecent(
    visits: Vec<cpe_server::spotlight_frecency::Visit>,
    now_s: u64,
    limit: usize,
) -> Vec<String> {
    tauri::async_runtime::spawn_blocking(move || spotlight_frecent_impl(visits, now_s, limit))
        .await
        .unwrap()
}

fn spotlight_frecent_impl(
    visits: Vec<cpe_server::spotlight_frecency::Visit>,
    now_s: u64,
    limit: usize,
) -> Vec<String> {
    cpe_server::spotlight_frecency::rank_frecent(&visits, now_s, limit)
}

/// Whether two files have identical content (CPE-418). Different sizes short-circuit to `false`;
/// otherwise the bytes are streamed and compared with an early exit on the first difference — cheaper
/// and collision-free versus hashing both. A directory or unreadable path is an `Err`, never a panic.
/// Model lives in `cpe_server::compare` (CPE-815); this is a thin `spawn_blocking` dispatcher.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn files_identical(a: String, b: String) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::compare::files_identical(&a, &b))
        .await.map_err(|e| e.to_string())?
}

/// Search text files under `root` for lines containing `query` (CPE-416). Model lives in
/// `cpe_server::content_search` (CPE-815); this is a thin `spawn_blocking` dispatcher.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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

/// Streaming variant of `search_file_contents` (CPE-662, streaming-liveness convention): pushes batches of
/// line matches over an IPC channel as the tree is walked, so a slow content search shows results live
/// instead of blocking on the whole scan. Async + `spawn_blocking` so the walk never freezes the UI thread
/// (CPE-760); the walk is the shared `cpe_server::content_search::stream_file_contents` (CPE-815). The
/// returned result carries the final `files_scanned` + `truncated` with empty `matches` (those streamed).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn search_file_contents_stream(
    root: String,
    query: String,
    case_sensitive: bool,
    on_match: tauri::ipc::Channel<Vec<cpe_server::content_search::ContentMatch>>,
) -> Result<cpe_server::content_search::ContentSearchResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let stats = cpe_server::content_search::stream_file_contents(
            &root,
            &query,
            case_sensitive,
            cpe_server::content_search::CONTENT_SEARCH_BATCH,
            |batch| {
                let _ = on_match.send(batch);
                std::ops::ControlFlow::Continue(())
            },
        )?;
        Ok(cpe_server::content_search::ContentSearchResult {
            matches: Vec::new(),
            files_scanned: stats.files_scanned,
            truncated: stats.truncated,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---- Instant-search index (CPE-1137, epic CPE-703) ------------------------------------------------
// Thin dispatchers over `cpe_server::index_service::IndexService` (held in managed state). The engine
// (crawl / on-disk store / trigram-pruned search) is all in `cpe-server`; these commands only resolve the
// per-volume index directory under app-data, keep the `ipc::Channel` transport in this adapter (per
// `docs/design/STREAMING.md`), and run every crawl/search on a blocking thread (async + `spawn_blocking`,
// the async-all-commands rule). Off means off: the service is empty until `index_build` is invoked —
// nothing crawls or loads at startup.

/// Resolve the per-volume index directory under app-data (`<app_data>/index`), sibling of `audit`.
fn index_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = server_ctx::TauriCtx::new(app).app_data_dir()?.join("index");
    Ok(dir)
}

/// Registry of in-flight `index_build` crawls' cancel flags, keyed by volume id, so a re-issued build for
/// the same volume cancels the prior crawl (mirrors `DIR_STREAM_CANCELS`).
static INDEX_BUILD_CANCELS: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<u64, std::sync::Arc<std::sync::atomic::AtomicBool>>>,
> = std::sync::OnceLock::new();

fn index_build_registry(
) -> &'static std::sync::Mutex<std::collections::HashMap<u64, std::sync::Arc<std::sync::atomic::AtomicBool>>>
{
    INDEX_BUILD_CANCELS.get_or_init(Default::default)
}

/// Crawl `root` into a resident index for `volume_id`, persist it to `<app_data>/index/<volume_id>.idx`,
/// and stream `BuildStats` progress over `on_progress` as the crawl advances. Re-issuing a build for the
/// same volume cancels the prior crawl. Returns the final `BuildStats`.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn index_build(
    app: tauri::AppHandle,
    state: tauri::State<'_, cpe_server::index_service::IndexService>,
    root: String,
    volume_id: u64,
    on_progress: tauri::ipc::Channel<cpe_server::index::BuildStats>,
) -> Result<cpe_server::index::BuildStats, String> {
    use std::sync::atomic::Ordering;
    let dir = index_dir(&app)?;
    let svc = state.inner().clone();
    let root_for_watch = root.clone(); // build_root consumes `root`; the watcher needs it after

    // Cancel any crawl already running for this volume, then register our own flag.
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let mut reg = index_build_registry().lock().unwrap();
        if let Some(prev) = reg.get(&volume_id) {
            prev.store(true, Ordering::Relaxed);
        }
        reg.insert(volume_id, cancel.clone());
    }
    let cancel_id = cancel.clone(); // for the still-ours check after the crawl

    let result = tauri::async_runtime::spawn_blocking(move || {
        svc.build_root(&root, volume_id, &dir, &cancel, |stats| {
            let _ = on_progress.send(stats);
        })
    })
    .await
    .map_err(|e| e.to_string())?;

    // Deregister — but only if a newer build hasn't already replaced our flag.
    {
        let mut reg = index_build_registry().lock().unwrap();
        if reg.get(&volume_id).is_some_and(|f| std::sync::Arc::ptr_eq(f, &cancel_id)) {
            reg.remove(&volume_id);
        }
    }

    // Arm (or re-arm) the live watcher — but ONLY if this build actually became resident. A build
    // whose `cancel` flag reads true here was superseded by a newer same-volume build (see
    // `IndexService::build_root`'s docs) and never touched the resident map; starting a watcher for
    // it would race the newer build's own `index_watch_start` with a stale root. CPE-1138.
    if result.is_ok() && !cancel_id.load(Ordering::Relaxed) {
        index_watch_start(&app, volume_id, &root_for_watch);
    }
    result
}

/// Streamed instant search across all resident volumes (lazily loading any persisted-but-not-resident
/// volume from disk first). Parses `query` via `index_query`, merges + ranks the hits, and streams them in
/// batches over `on_hit`; the frontend supersedes an in-flight search by generation token (STREAMING.md).
/// Returns the total number of hits emitted. Shares `search_all` with `index_search_collect` so the two
/// can never diverge.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn index_search(
    app: tauri::AppHandle,
    state: tauri::State<'_, cpe_server::index_service::IndexService>,
    query: String,
    limit: usize,
    on_hit: tauri::ipc::Channel<Vec<cpe_server::index::IndexHit>>,
) -> Result<usize, String> {
    let dir = index_dir(&app)?;
    let svc = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut emitted = 0usize;
        svc.stream_search(&dir, &query, limit, cpe_server::index_service::SEARCH_BATCH, |batch| {
            emitted += batch.len();
            let _ = on_hit.send(batch);
            std::ops::ControlFlow::Continue(())
        });
        Ok(emitted)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Collect-to-vec variant of `index_search` (tests + non-streaming callers): returns the ranked hits
/// directly instead of streaming them. Same `search_all` ranking, so results match `index_search`.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn index_search_collect(
    app: tauri::AppHandle,
    state: tauri::State<'_, cpe_server::index_service::IndexService>,
    query: String,
    limit: usize,
) -> Result<Vec<cpe_server::index::IndexHit>, String> {
    let dir = index_dir(&app)?;
    let svc = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || svc.search_all(&dir, &query, limit))
        .await
        .map_err(|e| e.to_string())
}

/// Resident volumes + their entry counts / truncated flags, for the UI to show index state. In-memory
/// only (no disk scan), so a fresh service reports an empty list.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn index_status(
    state: tauri::State<'_, cpe_server::index_service::IndexService>,
) -> Vec<cpe_server::index_service::VolumeStatus> {
    state.status()
}

/// Drop `volume_id` from memory and delete its on-disk index file. Returns whether the volume was
/// resident. Also stops (CPE-1138) any live watcher for the volume — off means off: a dropped volume
/// leaves no watcher thread/handle behind, resident or not (a `stop` on an unarmed volume is a no-op).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn index_drop(
    app: tauri::AppHandle,
    state: tauri::State<'_, cpe_server::index_service::IndexService>,
    volume_id: u64,
) -> Result<bool, String> {
    let dir = index_dir(&app)?;
    let svc = state.inner().clone();
    let removed = tauri::async_runtime::spawn_blocking(move || svc.drop_volume(&dir, volume_id))
        .await
        .map_err(|e| e.to_string())?;
    index_watch_stop(&app, volume_id);
    Ok(removed)
}

/// Free every resident volume from memory (on-disk files are kept; a later `index_search` reloads them).
/// Also stops (CPE-1138) every live watcher — off means off.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn index_clear(app: tauri::AppHandle, state: tauri::State<'_, cpe_server::index_service::IndexService>) {
    state.clear();
    index_watch_stop_all(&app);
}

/// Find duplicate files under `root` (CPE-420) — size-then-hash two-pass scan. Model lives in
/// `cpe_server::duplicates` (CPE-815); this is a thin `spawn_blocking` dispatcher.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn find_duplicates(root: String) -> Result<cpe_server::duplicates::DupResult, String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::duplicates::find_duplicates(&root))
        .await.map_err(|e| e.to_string())?
}

/// Streaming variant of `find_duplicates` (CPE-420, streaming-liveness convention): pushes each confirmed
/// batch of duplicate groups over an IPC channel as pass 2 hashes them, so a slow scan surfaces groups
/// progressively instead of blocking on the whole size-then-hash sweep. Async + `spawn_blocking` so the
/// scan never freezes the UI thread (CPE-760); the walk is the shared `cpe_server::duplicates::
/// stream_duplicates` (CPE-815). The returned result carries the final `files_scanned` + `truncated` with
/// empty `groups` (those streamed, in discovery order — the frontend re-sorts each batch).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn find_duplicates_stream(
    root: String,
    on_group: tauri::ipc::Channel<Vec<cpe_server::duplicates::DupGroup>>,
) -> Result<cpe_server::duplicates::DupResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let stats = cpe_server::duplicates::stream_duplicates(&root, |batch| {
            let _ = on_group.send(batch);
            std::ops::ControlFlow::Continue(())
        })?;
        Ok(cpe_server::duplicates::DupResult {
            groups: Vec::new(),
            files_scanned: stats.files_scanned,
            truncated: stats.truncated,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Find near-duplicate (visually-similar) images under `root` (CPE-1200/1201, epic CPE-997) — walk +
/// dHash + single-link cluster. Model lives in `cpe_server::image_similarity`; this is a thin
/// `spawn_blocking` dispatcher, the perceptual complement of `find_duplicates`.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn find_similar_images(root: String) -> Result<cpe_server::image_similarity::SimResult, String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::image_similarity::find_similar_images(&root))
        .await
        .map_err(|e| e.to_string())?
}

/// Streaming variant of `find_similar_images` (CPE-1200/1201, streaming-liveness convention): pushes the
/// near-duplicate groups over an IPC channel so the UI thread never blocks on the walk + decode + cluster
/// (CPE-760). Image clustering is a whole-set operation (single-link needs every hash first), so the
/// groups arrive as one batch after the walk completes — the frontend still flips `loading` off on it.
/// The walk is the shared `cpe_server::image_similarity::stream_similar_images`; the returned result
/// carries the final `files_scanned` + `truncated` with empty `groups` (those streamed).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn find_similar_images_stream(
    root: String,
    on_group: tauri::ipc::Channel<Vec<cpe_server::image_similarity::SimGroup>>,
) -> Result<cpe_server::image_similarity::SimResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let stats = cpe_server::image_similarity::stream_similar_images(
            &root,
            cpe_server::image_similarity::DEFAULT_MAX_DISTANCE,
            |batch| {
                let _ = on_group.send(batch);
                std::ops::ControlFlow::Continue(())
            },
        )?;
        Ok(cpe_server::image_similarity::SimResult {
            groups: Vec::new(),
            files_scanned: stats.files_scanned,
            truncated: stats.truncated,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Find near-duplicate (near-identical wording) text documents under `root` (CPE-1204, epic CPE-997
/// stretch) — walk + SimHash + single-link cluster. Model lives in `cpe_server::document_similarity`;
/// this is a thin `spawn_blocking` dispatcher, the textual complement of `find_similar_images`. A
/// modest-sized collect-to-vec command (not streamed): SimHash clustering is a whole-set operation like
/// image similarity, and this scope (plain-text notes/READMEs) is smaller than a full folder walk.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn find_similar_documents(root: String) -> Result<cpe_server::document_similarity::DocSimResult, String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::document_similarity::find_similar_documents(&root))
        .await
        .map_err(|e| e.to_string())?
}

/// Find near-identical folders under `root` (CPE-1204, epic CPE-997 stretch) — walk + per-folder
/// content-hash set + Jaccard single-link cluster. Model lives in `cpe_server::folder_similarity_scan`
/// (adapter) / `cpe_server::folder_similarity` (pure core); this is a thin `spawn_blocking` dispatcher.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn find_similar_folders(root: String) -> Result<cpe_server::folder_similarity_scan::FolderSimResult, String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::folder_similarity_scan::find_similar_folders(&root))
        .await
        .map_err(|e| e.to_string())?
}

// ---- Rules-based auto-organize (CPE-1142, epic CPE-979) -------------------------------------------
// Propose -> checkpoint -> apply, wired over the built+tested `cpe_server::organize` planner. The listing
// + checkpoint + move glue lives in `cpe_server::organize_apply` (CPE-815 pattern); these commands are
// thin `spawn_blocking` dispatchers. `organize_plan`/`organize_clutter` are read-only previews; only
// `organize_apply` touches disk, and it always checkpoints `dir` first so the whole reorg is one undo.

/// Propose an auto-organize plan for `dir` under `rule` — one [`cpe_server::organize::MoveProposal`] per
/// file. Read-only: moves nothing.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn organize_plan(
    dir: String,
    rule: cpe_server::organize::OrganizeRule,
) -> Result<Vec<cpe_server::organize::MoveProposal>, String> {
    cpe_server::fs_route::require_local(&dir)?;
    tauri::async_runtime::spawn_blocking(move || cpe_server::organize_apply::organize_plan(&dir, rule))
        .await
        .map_err(|e| e.to_string())?
}

/// Flag likely-clutter files in `dir` (zero-byte / installer / partial-download / backup leftovers).
/// Read-only suggestion surface — never an auto-action.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn organize_clutter(dir: String) -> Result<Vec<cpe_server::organize::ClutterFinding>, String> {
    cpe_server::fs_route::require_local(&dir)?;
    tauri::async_runtime::spawn_blocking(move || cpe_server::organize_apply::organize_clutter(&dir))
        .await
        .map_err(|e| e.to_string())?
}

/// Apply an auto-organize plan for `dir` under `rule`: checkpoint `dir` first (one-undo for the whole
/// reorg), then create each proposal's target subfolder and move the file into it (skip-on-error per
/// file). Nothing runs until this is called explicitly — the dialog only calls it after the user reviews
/// `organize_plan`'s preview and clicks Apply.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn organize_apply(
    app: tauri::AppHandle,
    dir: String,
    rule: cpe_server::organize::OrganizeRule,
) -> Result<cpe_server::organize_apply::OrganizeApplyOutcome, String> {
    cpe_server::fs_route::require_local(&dir)?;
    let ctx = server_ctx::TauriCtx::new(&app);
    let outcome = tauri::async_runtime::spawn_blocking(move || cpe_server::organize_apply::organize_apply(&ctx, &dir, rule))
        .await
        .map_err(|e| e.to_string())??;
    // Both the vacated original path and the new target path are ours (CPE-1101), mirroring move_exact.
    note_app_op(&app, || {
        outcome.results.iter().filter(|r| r.ok).map(|r| r.path.clone()).collect()
    });
    Ok(outcome)
}

/// Read `settings.json` from `dir`, returning `{}` when it's absent or
/// unreadable so the frontend always starts from a valid document.
/// Read the single on-disk settings file (`settings.json` in the app config dir). Returns `{}` when it
/// doesn't exist yet, so the frontend can start from defaults on a fresh install (CPE-226). The model
/// lives in `cpe_server::settings` (CPE-815); this is a thin dispatcher.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn read_settings(app: tauri::AppHandle) -> Result<String, String> {
    cpe_server::settings::load(&server_ctx::TauriCtx::new(&app))
}

/// Write the single on-disk settings file, creating the config dir if needed (CPE-226). `contents` is
/// the full settings JSON document.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn write_settings(app: tauri::AppHandle, contents: String) -> Result<(), String> {
    cpe_server::settings::save(&server_ctx::TauriCtx::new(&app), &contents)
}

// ---- Tag store (CPE-635, epic CPE-614) -------------------------------------------------------
// The model + persistence now live in the pure Server crate (`cpe_server::tags`, CPE-815); the
// commands below are one-line dispatchers that build a `TauriCtx` and call into it.
use cpe_server::tags::TagStore;

/// The whole tag store (path → {tags,label}); `{}` on a fresh install.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn load_tags(app: tauri::AppHandle) -> Result<TagStore, String> {
    cpe_server::tags::load(&server_ctx::TauriCtx::new(&app))
}

/// Replace one path's tags + label and persist. Returns the updated whole store.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn set_tags(app: tauri::AppHandle, path: String, tags: Vec<String>, label: String) -> Result<TagStore, String> {
    cpe_server::tags::set(&server_ctx::TauriCtx::new(&app), &path, tags, label)
}

/// Every tag with its usage count (most-used first).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn tag_counts(app: tauri::AppHandle) -> Result<Vec<(String, usize)>, String> {
    cpe_server::tags::counts(&server_ctx::TauriCtx::new(&app))
}

/// Rename a tag across every path (CPE-646); an empty `new` deletes it. Returns the updated store.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn rename_tag(app: tauri::AppHandle, old: String, new_name: String) -> Result<TagStore, String> {
    cpe_server::tags::rename_tag(&server_ctx::TauriCtx::new(&app), &old, &new_name)
}

/// Remove a tag from every path (CPE-646). Returns the updated store.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn delete_tag(app: tauri::AppHandle, tag: String) -> Result<TagStore, String> {
    cpe_server::tags::delete_tag(&server_ctx::TauriCtx::new(&app), &tag)
}

/// Re-key a path's tags/label after an in-app rename or move (CPE-650), so tags follow the file.
/// Returns the updated store. A no-op when the old path had no tags.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn retag_path(app: tauri::AppHandle, from: String, to: String) -> Result<TagStore, String> {
    cpe_server::tags::retag(&server_ctx::TauriCtx::new(&app), &from, &to)
}

/// Import a previously-exported tag store (JSON), merged into the current one (CPE-640). Non-
/// destructive: existing tags are kept, imported tags unioned in. Returns the merged store. (Export
/// is just `load_tags` + `JSON.stringify` on the frontend.)
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn import_tags(app: tauri::AppHandle, json: String) -> Result<TagStore, String> {
    cpe_server::tags::import(&server_ctx::TauriCtx::new(&app), &json)
}

// ---- Folder templates (CPE-837, epic CPE-740) ------------------------------------------------
// Thin dispatchers into `cpe_server::folder_template` (core + store built in CPE-835/836). `capture`
// and `stamp` do real filesystem work → async + `spawn_blocking`; the config-store ops match the sync
// tag-store pattern (a small `templates.json` in the config dir, reached via `TauriCtx`).
use cpe_server::folder_template::{Catalog as TemplateCatalog, Template, TemplateSummary};

/// Capture a folder's structure into a reusable template (not yet saved to the catalog).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn template_capture(path: String, name: String) -> Result<Template, String> {
    tauri::async_runtime::spawn_blocking(move || {
        cpe_server::folder_template::capture(std::path::Path::new(&path), name)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Save (insert or replace by name) a template and persist. Returns the updated catalog.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn template_save(app: tauri::AppHandle, template: Template) -> Result<TemplateCatalog, String> {
    cpe_server::folder_template::save(&server_ctx::TauriCtx::new(&app), template)
}

/// Every stored template's name + node counts, for the gallery.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn template_list(app: tauri::AppHandle) -> Result<Vec<TemplateSummary>, String> {
    cpe_server::folder_template::list(&server_ctx::TauriCtx::new(&app))
}

/// One stored template by name (`None` if absent).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn template_load(app: tauri::AppHandle, name: String) -> Result<Option<Template>, String> {
    cpe_server::folder_template::load(&server_ctx::TauriCtx::new(&app), &name)
}

/// Delete a stored template by name and persist. Returns the updated catalog.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn template_delete(app: tauri::AppHandle, name: String) -> Result<TemplateCatalog, String> {
    cpe_server::folder_template::delete(&server_ctx::TauriCtx::new(&app), &name)
}

/// Stamp `template` into `dest` with `{token}` variable substitution. Returns the created paths.
/// Path-safe and never clobbers an existing file (enforced in the core).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn template_stamp(
    app: tauri::AppHandle,
    template: Template,
    dest: String,
    vars: std::collections::BTreeMap<String, String>,
) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let result = cpe_server::folder_template::stamp(&template, std::path::Path::new(&dest), &vars)
            .map(|paths| paths.into_iter().map(|p| p.display().to_string()).collect::<Vec<_>>());
        // Record the actually-created paths right after the stamp (their exact set — including nested
        // dirs/files — is only known once `stamp` runs), still well within the ledger's TTL ahead of
        // the watcher events it just triggered (CPE-1102).
        if let Ok(created) = &result {
            note_app_op(&app, || created.clone());
        }
        result
    })
    .await
    .map_err(|e| e.to_string())?
}

/// A single template's JSON, for sharing/export.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn template_export(template: Template) -> Result<String, String> {
    cpe_server::folder_template::export(&template)
}

/// Import a template (or a whole catalog) from JSON, merged by name. Returns the updated catalog.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn template_import(app: tauri::AppHandle, json: String) -> Result<TemplateCatalog, String> {
    cpe_server::folder_template::import(&server_ctx::TauriCtx::new(&app), &json)
}

// ---- Action macros (CPE-938/951/1033/1187/1188/1194, epic CPE-739) ---------------------------
// Thin dispatchers into `cpe_server::macro_store` (CRUD/persistence, CPE-1033) and the CPE-1187
// `macro_run::resolve` executor (resolution + collision-safe naming + scope guard + inverse
// model). The RUN command bridges the resolved plan to the existing `rename_entry`/`move_exact`
// primitives (plus the tag store and a media re-encode for `convert`), applying the whole run
// atomically — a failure partway rolls back everything already applied via the recorded inverses,
// so a macro run is never left half-done. `macro_undo` replays a completed run's inverses in
// reverse to reverse it later. Imported macros never auto-run: `macro_import` only writes to the
// persisted catalog — running is always a separate, explicit `macro_run` call from the UI.
//
// CPE-1194 fixed two undo-fidelity gaps found in the CPE-1187/1188 review (PR #498): a `convert`
// step now trashes its pre-convert original (instead of deleting it) so undo restores the exact
// original bytes from the trash rather than lossily re-encoding; and `macro_apply_run` snapshots
// the pre-run tag state so a `tag` step's undo never strips a label the path already had.
use cpe_server::action_macro::{ActionMacro, PlannedOp};
use cpe_server::macro_run::ResolvedRun;
use cpe_server::macro_store::{Catalog as MacroCatalog, MacroSummary};

/// Save (insert or replace by name) a macro and persist. Returns the updated catalog.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn macro_save(app: tauri::AppHandle, macro_: ActionMacro) -> Result<MacroCatalog, String> {
    cpe_server::macro_store::save(&server_ctx::TauriCtx::new(&app), macro_)
}

/// Every stored macro's name + step count, for the gallery.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn macro_list(app: tauri::AppHandle) -> Result<Vec<MacroSummary>, String> {
    cpe_server::macro_store::list(&server_ctx::TauriCtx::new(&app))
}

/// One stored macro by name (`None` if absent).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn macro_load(app: tauri::AppHandle, name: String) -> Result<Option<ActionMacro>, String> {
    cpe_server::macro_store::load(&server_ctx::TauriCtx::new(&app), &name)
}

/// Delete a stored macro by name and persist. Returns the updated catalog.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn macro_delete(app: tauri::AppHandle, name: String) -> Result<MacroCatalog, String> {
    cpe_server::macro_store::delete(&server_ctx::TauriCtx::new(&app), &name)
}

/// A single macro's JSON, for sharing/export.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn macro_export(macro_: ActionMacro) -> Result<String, String> {
    cpe_server::macro_store::export(&macro_)
}

/// Import a macro (or a whole catalog) from JSON, merged by name into the persisted store. Returns
/// the updated catalog. **Never runs anything** — running is always the separate, explicit
/// `macro_run` call the UI makes after the user picks the imported macro.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn macro_import(app: tauri::AppHandle, json: String) -> Result<MacroCatalog, String> {
    cpe_server::macro_store::import(&server_ctx::TauriCtx::new(&app), &json)
}

/// Expand `macro_` over `inputs` into the flat, unresolved `PlannedOp` preview list (CPE-938) — the
/// macro-run confirm dialog's dry-run data. Pure; no collision-dedupe or scope check yet (those are
/// `macro_run`'s job, via CPE-1187's `macro_run::resolve`).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn macro_plan(macro_: ActionMacro, inputs: Vec<String>) -> Result<Vec<PlannedOp>, String> {
    cpe_server::action_macro::validate(&macro_)?;
    Ok(cpe_server::action_macro::plan(&macro_, &inputs))
}

/// Run `macro_` over `inputs`, scoped to `root`: resolves + scope-checks via CPE-1187
/// (`macro_run::resolve`), then physically applies each op. All-or-nothing — if any step fails
/// partway, everything already applied is rolled back automatically via the recorded inverses
/// before the error is returned. On success, returns the applied `ResolvedRun`; hang onto it (the
/// frontend keeps it in memory) and pass it to `macro_undo` to reverse the whole run later.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn macro_run(
    app: tauri::AppHandle,
    macro_: ActionMacro,
    inputs: Vec<String>,
    root: String,
) -> Result<ResolvedRun, String> {
    let resolved = cpe_server::macro_run::resolve(&macro_, &inputs, &root).map_err(|errs| errs.join("; "))?;
    // Every path a resolved op touches is the user's doing (CPE-1101), same as rename_entry/move_exact.
    note_app_op(&app, || {
        resolved
            .ops
            .iter()
            .flat_map(|op| [op.from.clone(), op.to.clone()])
            .collect()
    });
    let ctx = server_ctx::TauriCtx::new(&app);
    tauri::async_runtime::spawn_blocking(move || macro_apply_run(&ctx, resolved))
        .await
        .map_err(|e| e.to_string())?
}

/// Undo a previously-applied `ResolvedRun` (as returned by `macro_run`) by replaying its inverses
/// in reverse order.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn macro_undo(app: tauri::AppHandle, run: ResolvedRun) -> Result<(), String> {
    note_app_op(&app, || {
        run.inverses
            .iter()
            .flat_map(|inv| [inv.from.clone(), inv.to.clone()])
            .collect()
    });
    let ctx = server_ctx::TauriCtx::new(&app);
    tauri::async_runtime::spawn_blocking(move || macro_apply_inverses(&ctx, &run))
        .await
        .map_err(|e| e.to_string())?
}

/// Apply every op in `run.ops`, in order; if one fails, immediately replay the inverses of
/// whatever already succeeded (in reverse), so the run is all-or-nothing, then return the original
/// failure. On success, returns `run` — the caller's undo handle for `macro_undo` — with one
/// correction (CPE-1194): just before applying each `tag` op, the pre-run tag state at its path is
/// snapshotted, and if the label was already present, that op's inverse is rewritten from `"untag"`
/// to `"tag"` (a no-op restore) so undo never strips a label the user had before the run.
fn macro_apply_run(ctx: &dyn ServerCtx, mut run: ResolvedRun) -> Result<ResolvedRun, String> {
    for applied in 0..run.ops.len() {
        let op = run.ops[applied].clone();
        if op.kind == "tag" && macro_tag_already_present(ctx, &op.from, &op.detail) {
            run.inverses[applied].kind = "tag".to_string();
        }
        if let Err(e) = macro_apply_op(ctx, &op.from, &op.kind, &op.detail, &op.to) {
            // Roll back the already-applied ops in reverse. Collect any rollback failures rather
            // than discarding them: if an inverse itself fails (e.g. a file was externally moved
            // or locked between apply and rollback), the filesystem is left partially modified and
            // the caller MUST be told the truth — never claim "(rolled back)" when it didn't.
            let mut rollback_errs = Vec::new();
            for inv in run.inverses[..applied].iter().rev() {
                if let Err(re) = macro_apply_op(ctx, &inv.from, &inv.kind, &inv.detail, &inv.to) {
                    rollback_errs.push(re);
                }
            }
            if rollback_errs.is_empty() {
                return Err(format!("macro run failed at step {}: {e} (rolled back)", applied + 1));
            }
            return Err(format!(
                "macro run failed at step {}: {e}; ROLLBACK ALSO FAILED for {} of {} inverse op(s) \
                 ({}) — the filesystem may be partially modified",
                applied + 1,
                rollback_errs.len(),
                applied,
                rollback_errs.join("; "),
            ));
        }
    }
    Ok(run)
}

/// Replay `run.inverses` in reverse order — the undo of a completed `macro_run`.
fn macro_apply_inverses(ctx: &dyn ServerCtx, run: &ResolvedRun) -> Result<(), String> {
    for inv in run.inverses.iter().rev() {
        macro_apply_op(ctx, &inv.from, &inv.kind, &inv.detail, &inv.to)?;
    }
    Ok(())
}

/// Bridge one resolved (or inverse) op to its real primitive. `kind` is `"rename"` / `"move"` /
/// `"convert"` / `"tag"` / `"untag"` (the inverse-only kind of a tag step) / `"restore_convert"`
/// (the inverse-only kind of a convert step, per `macro_run::InverseOp`; CPE-1194).
fn macro_apply_op(ctx: &dyn ServerCtx, from: &str, kind: &str, detail: &str, to: &str) -> Result<(), String> {
    match kind {
        "rename" => {
            let new_name = Path::new(to)
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| format!("bad rename target {to:?}"))?
                .to_string();
            rename_entry_impl(ctx, from.to_string(), new_name).map(|_| ())
        }
        "move" => {
            let results = move_exact_impl(ctx, vec![(from.to_string(), to.to_string())]);
            match results.into_iter().next() {
                Some(r) if r.ok => Ok(()),
                Some(r) => Err(r.error),
                None => Err("move produced no result".to_string()),
            }
        }
        "convert" => macro_convert_in_place(from, to, detail),
        "restore_convert" => macro_restore_converted(from, to),
        "tag" => macro_add_tag(ctx, from, detail),
        "untag" => macro_remove_tag(ctx, from, detail),
        other => Err(format!("unknown macro op kind {other:?}")),
    }
}

/// Re-encode the image at `from` to the `detail` extension and write it at `to`, then route the
/// original to the OS trash — a macro `Convert` step is in-place from the user's perspective (one
/// file, new extension), unlike the Batch-Media dialog's non-destructive-by-default siblings. A
/// no-op when `to == from`. Non-image bytes or an unsupported target extension fail loudly rather
/// than silently corrupting the file; `macro_apply_run` rolls the whole run back on any such failure.
///
/// **The original is trashed, not permanently deleted (CPE-1194).** Routing it through the OS
/// Recycle Bin / Trash (same primitive as `delete_to_trash`) rather than `fs::remove_file` means the
/// `"restore_convert"` inverse (`macro_restore_converted`) can restore the exact original bytes from
/// the trash on undo — a byte-exact restore, not a second lossy re-encode.
fn macro_convert_in_place(from: &str, to: &str, detail: &str) -> Result<(), String> {
    if from == to {
        return Ok(());
    }
    let bytes = fs::read(from).map_err(|e| format!("could not read {from}: {e}"))?;
    let converted = cpe_server::batch_transform::apply_ops(
        &bytes,
        &[cpe_server::batch_media::MediaOp::Convert { to_ext: detail.to_string() }],
    )?;
    fs::write(to, converted).map_err(|e| format!("could not write {to}: {e}"))?;
    trash::delete(from).map_err(|e| format!("could not trash {from}: {e}"))?;
    Ok(())
}

/// Undo of a `Convert` step (CPE-1194, the `"restore_convert"` inverse kind): restores the
/// pre-convert original's exact bytes from the OS trash — where `macro_convert_in_place` routed them
/// — instead of re-encoding back to the source extension, so a PNG→JPG→undo round-trip yields the
/// real original bytes, not a second, lossier re-encode. `from` is the converted file (produced by
/// the forward step, removed here); `to` is the original file's path (restored here). A no-op when
/// `from == to` (the forward step never touched anything, so nothing was ever trashed).
///
/// If the platform can't programmatically restore from the trash (macOS — see
/// `can_restore_from_trash_impl`), this fails loudly with that platform's honest error rather than
/// silently leaving the original lost; the converted file is left in place in that case (never
/// destroy the only remaining copy when the restore didn't happen).
fn macro_restore_converted(from: &str, to: &str) -> Result<(), String> {
    if from == to {
        return Ok(());
    }
    let restored = restore_from_trash_impl(vec![to.to_string()]);
    match restored.into_iter().next() {
        Some(r) if r.ok => {}
        Some(r) => return Err(r.error),
        None => return Err("trash restore produced no result".to_string()),
    }
    // The converted file is no longer needed. Trash it too rather than permanently deleting it —
    // an undo path must never do a silent permanent delete.
    trash::delete(from).map_err(|e| format!("could not trash converted file {from}: {e}"))?;
    Ok(())
}

/// Add `label` to the tags already at `path` (union — a macro `Tag` step never clobbers existing
/// tags), preserving the path's colour label.
fn macro_add_tag(ctx: &dyn ServerCtx, path: &str, label: &str) -> Result<(), String> {
    let store = cpe_server::tags::load(ctx)?;
    let entry = store.get(path);
    let mut tags: Vec<String> = entry.map(|e| e.tags().to_vec()).unwrap_or_default();
    let existing_label = entry.map(|e| e.label().to_string()).unwrap_or_default();
    if !tags.iter().any(|t| t == label) {
        tags.push(label.to_string());
    }
    cpe_server::tags::set(ctx, path, tags, existing_label).map(|_| ())
}

/// Whether `path` already carries `label` in the tag store, right now (CPE-1194) — used by
/// `macro_apply_run` to snapshot pre-run tag state before a `tag` step runs, so its inverse can be
/// corrected to a no-op when the label pre-existed. Defaults to `false` (treats a load failure the
/// same as "not tagged"; a subsequent real tag-store operation surfaces the failure properly).
fn macro_tag_already_present(ctx: &dyn ServerCtx, path: &str, label: &str) -> bool {
    cpe_server::tags::load(ctx)
        .ok()
        .and_then(|store| store.get(path).map(|e| e.tags().iter().any(|t| t == label)))
        .unwrap_or(false)
}

/// Remove `label` from `path`'s tags (the inverse of `macro_add_tag`), preserving the other tags
/// and the colour label. A no-op when the path has no tag-store entry at all.
fn macro_remove_tag(ctx: &dyn ServerCtx, path: &str, label: &str) -> Result<(), String> {
    let store = cpe_server::tags::load(ctx)?;
    let Some(entry) = store.get(path) else {
        return Ok(());
    };
    let tags: Vec<String> = entry.tags().iter().filter(|t| *t != label).cloned().collect();
    let existing_label = entry.label().to_string();
    cpe_server::tags::set(ctx, path, tags, existing_label).map(|_| ())
}

// ---- Native tags bridge (CPE-828, epic CPE-717) ----------------------------------------------
// Sync a path's CPE tags with the OS-native store (macOS Finder tags / Windows NTFS ADS / Linux xattr)
// via the tested `cpe_server::native_bridge` orchestration (CPE-830). Thin dispatchers over its ctx
// entry points; both degrade to a no-op on a filesystem that can't store native metadata.

/// The human name of this OS's native tag store ("Finder tags" / "NTFS alternate data streams" / …).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn native_tags_name() -> String {
    cpe_server::native_bridge::native_name()
}

/// Pull `path`'s native tags into the CPE store (non-destructive) and persist. Returns the updated store.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn native_tags_pull(app: tauri::AppHandle, path: String) -> Result<TagStore, String> {
    cpe_server::native_bridge::pull_ctx(&server_ctx::TauriCtx::new(&app), std::path::Path::new(&path))
}

/// Push `path`'s CPE tags out to native file metadata (the CPE store is authoritative).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn native_tags_push(app: tauri::AppHandle, path: String) -> Result<(), String> {
    cpe_server::native_bridge::push_ctx(&server_ctx::TauriCtx::new(&app), std::path::Path::new(&path))
}

/// Return the user's home directory.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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

/// Network / mapped / SMB shares for the Home "Shared" tab (CPE-1163).
///
/// Returns the OS-enumerated network drives (Windows mapped drives, Unix SMB/NFS mounts) merged with
/// the user's own added locations (`user_added`, persisted by the frontend like favorites). The pure
/// parsing + merging lives in `cpe_server::net_share`; this command only runs the platform command
/// and feeds its stdout in. **Time-bounded:** the enumeration subprocess is capped (below), so a
/// dead/offline mapped server can never hang the Home view — a timeout yields an empty enumeration
/// and the user-added rows still list.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn list_network_shares(user_added: Vec<String>) -> Vec<cpe_server::net_share::NetShare> {
    tauri::async_runtime::spawn_blocking(move || list_network_shares_impl(user_added))
        .await
        .unwrap_or_default()
}

fn list_network_shares_impl(user_added: Vec<String>) -> Vec<cpe_server::net_share::NetShare> {
    cpe_server::net_share::combine_shares(enumerate_os_shares(), &user_added)
}

/// Best-effort, platform-aware enumeration of the network mounts the OS already knows about. Empty is
/// always an acceptable result (unsupported platform, no mounts, or a timed-out probe).
fn enumerate_os_shares() -> Vec<cpe_server::net_share::NetShare> {
    #[cfg(target_os = "windows")]
    {
        // `net use` reads the local redirector table (it does not contact the servers), but bound it
        // anyway so nothing on the Home path can ever wait on a wedged process.
        run_bounded_capture("net", &["use"], std::time::Duration::from_secs(3))
            .map(|o| cpe_server::net_share::parse_net_use(&o))
            .unwrap_or_default()
    }
    #[cfg(target_os = "linux")]
    {
        // `/proc/mounts` is a synthetic kernel file — a plain, non-blocking read.
        std::fs::read_to_string("/proc/mounts")
            .map(|o| cpe_server::net_share::parse_proc_mounts(&o))
            .unwrap_or_default()
    }
    #[cfg(target_os = "macos")]
    {
        run_bounded_capture("/sbin/mount", &[], std::time::Duration::from_secs(3))
            .map(|o| cpe_server::net_share::parse_macos_mount(&o))
            .unwrap_or_default()
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        Vec::new()
    }
}

/// Run `program args…`, returning its captured stdout, but abandon (and kill) it after `timeout` so a
/// hung network command can never block the caller. The stdout is drained on a helper thread; if the
/// timeout fires first we kill the child, which closes the pipe and frees the thread. Used only where
/// the enumeration shells out (Windows `net use`, macOS `mount`).
#[cfg(any(target_os = "windows", target_os = "macos"))]
fn run_bounded_capture(program: &str, args: &[&str], timeout: std::time::Duration) -> Option<String> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    use std::sync::mpsc;

    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW — no console flash on the desktop app.
    }

    let mut child = cmd.spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = String::new();
        let _ = stdout.read_to_string(&mut buf);
        let _ = tx.send(buf);
    });

    match rx.recv_timeout(timeout) {
        Ok(buf) => {
            let _ = child.wait();
            Some(buf)
        }
        Err(_) => {
            // Timed out: kill the child (closing the pipe frees the reader thread) and give up.
            let _ = child.kill();
            let _ = child.wait();
            None
        }
    }
}

/// Disconnect a mapped network drive — the Shared tab's "Disconnect" action for a mapped drive
/// (CPE-1163). Windows-only (`net use <drive> /delete /y`), best-effort + time-bounded; on any other
/// platform, or for a `path` that isn't a drive-letter root, it returns a clear error the UI surfaces.
/// User-added locations use the frontend "Remove" path instead (pure list pruning, no command).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn disconnect_network_share(path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || disconnect_network_share_impl(path))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(target_os = "windows")]
fn disconnect_network_share_impl(path: String) -> Result<(), String> {
    let trimmed = path.trim();
    let bytes = trimmed.as_bytes();
    if bytes.len() < 2 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b':' {
        return Err(format!("Not a mapped drive: {path}"));
    }
    let drive = format!("{}:", bytes[0] as char);
    let output = run_bounded_output(
        "net",
        &["use", &drive, "/delete", "/y"],
        std::time::Duration::from_secs(8),
    )
    .ok_or_else(|| "Disconnecting the drive timed out.".to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let msg = stderr.trim();
        Err(if msg.is_empty() {
            format!("Couldn't disconnect {drive}.")
        } else {
            msg.to_string()
        })
    }
}

#[cfg(not(target_os = "windows"))]
fn disconnect_network_share_impl(_path: String) -> Result<(), String> {
    Err("Disconnecting network drives is only supported on Windows.".to_string())
}

/// Run `program args…` to completion, capturing its full [`Output`](std::process::Output), but give up
/// after `timeout` so a wedged command can't block. Used for the short, local `net use /delete` so its
/// exit status (success/failure) can be reported — unlike [`run_bounded_capture`], which only needs
/// stdout. Windows-only (its sole caller is the disconnect command).
#[cfg(target_os = "windows")]
fn run_bounded_output(
    program: &str,
    args: &[&str],
    timeout: std::time::Duration,
) -> Option<std::process::Output> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    use std::sync::mpsc;

    let child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .spawn()
        .ok()?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    match rx.recv_timeout(timeout) {
        Ok(Ok(output)) => Some(output),
        _ => None,
    }
}

/// Free + total bytes on the volume containing `path`, for the status bar (CPE-403).
#[derive(serde::Serialize)]
#[cfg_attr(feature = "specta-bindings", derive(specta::Type))]
struct DiskSpace {
    free: u64,
    total: u64,
}

/// Report free/total space on the volume that holds `path` (CPE-403). `free` is what's available to
/// the user (respects quotas). Non-fatal: returns an error string the frontend degrades on rather
/// than surfacing — a status-bar nicety must never break navigation.
// Async so `disk_space` on a slow/network drive runs off the main thread (CPE-760).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", derive(specta::Type))]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn extract_archive_entry(app: tauri::AppHandle, zip: String, inner: String) -> Result<String, String> {
    // Record the exact temp-file target `cpe_server::archive::extract_archive_entry` will write
    // (mirroring its own `dir.join(base)` — CPE-1102), best-effort: if `inner`'s last path component
    // can't be read (e.g. an empty name), the write itself will fail too, so skipping the record here
    // is harmless.
    note_app_op(&app, || {
        Path::new(&inner)
            .file_name()
            .map(|base| {
                std::env::temp_dir()
                    .join("cpe-archive")
                    .join(base)
                    .to_string_lossy()
                    .into_owned()
            })
            .into_iter()
            .collect()
    });
    tauri::async_runtime::spawn_blocking(move || cpe_server::archive::extract_archive_entry(&zip, &inner))
        .await.map_err(|e| e.to_string())?
}

/// Extract a single entry from any supported non-zip archive (tar/tar.gz/tgz/7z; zip delegates to the
/// same underlying extractor as [`extract_archive_entry`]) to a temp file and return its path, so a leaf
/// inside a tar/7z archive can be opened the same way a zip leaf already can (CPE-1180, unblocks
/// CPE-1181). Read-only: the temp copy is what opens, not the archived bytes.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn extract_archive_entry_any(app: tauri::AppHandle, path: String, inner: String) -> Result<String, String> {
    // Record the exact temp-file target `cpe_server::archive::extract_archive_entry_any` will write
    // (mirroring `extract_archive_entry`'s note above — CPE-1102/1180), best-effort.
    note_app_op(&app, || {
        Path::new(&inner)
            .file_name()
            .map(|base| {
                std::env::temp_dir()
                    .join("cpe-archive")
                    .join(base)
                    .to_string_lossy()
                    .into_owned()
            })
            .into_iter()
            .collect()
    });
    tauri::async_runtime::spawn_blocking(move || cpe_server::archive::extract_archive_entry_any(&path, &inner))
        .await.map_err(|e| e.to_string())?
}

// Archive creation & extraction (CPE-251/252/242) now live in `cpe_server::archive` (CPE-822); the
// commands below are thin dispatchers.

/// Pack the given files/folders into a new deflated `.zip` at `dest` (CPE-251). Model lives in
/// `cpe_server::archive` (CPE-822); thin dispatcher.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn compress_to_zip(paths: Vec<String>, dest: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::archive::compress_to_zip(&paths, &dest))
        .await.map_err(|e| e.to_string())?
}

/// Extract an archive into `dest` (CPE-252), guarded against zip-slip for every format. Model lives in
/// `cpe_server::archive` (CPE-822); thin dispatcher.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn extract_archive(app: tauri::AppHandle, path: String, dest: String) -> Result<String, String> {
    // Coarse best-effort record (CPE-1102): the individual extracted entry paths aren't known until the
    // archive is actually read, so record just the `dest` root (which `extract_archive` itself
    // `create_dir_all`s first) rather than trying to enumerate every member up front.
    note_app_op(&app, || vec![dest.clone()]);
    tauri::async_runtime::spawn_blocking(move || cpe_server::archive::extract_archive(&path, &dest))
        .await.map_err(|e| e.to_string())?
}

/// Pack files/folders into `dest`, choosing the archive format by `dest`'s extension (`.zip` or
/// `.tar.gz`/`.tgz`) (CPE-908/1141). Model lives in `cpe_server::archive` (CPE-822); thin dispatcher.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn compress_archive(paths: Vec<String>, dest: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::archive::compress_archive(&paths, &dest))
        .await.map_err(|e| e.to_string())?
}

/// Pack files/folders into a password-protected (AES-256) `.zip` at `dest` (CPE-909/1141). Model lives
/// in `cpe_server::archive` (CPE-822); thin dispatcher.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn compress_to_zip_encrypted(paths: Vec<String>, dest: String, password: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::archive::compress_to_zip_encrypted(&paths, &dest, &password))
        .await.map_err(|e| e.to_string())?
}

/// Extract a password-protected `.zip` at `path` into `dest` with `password` (CPE-909/1141). Model
/// lives in `cpe_server::archive` (CPE-822); thin dispatcher.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn extract_zip_encrypted(app: tauri::AppHandle, path: String, dest: String, password: String) -> Result<String, String> {
    // Coarse best-effort record (CPE-1102), mirroring `extract_archive`: record just the `dest` root.
    note_app_op(&app, || vec![dest.clone()]);
    tauri::async_runtime::spawn_blocking(move || cpe_server::archive::extract_zip_encrypted(&path, &dest, &password))
        .await.map_err(|e| e.to_string())?
}

/// Run an executable with elevation (CPE-241). On Windows this uses
/// `Start-Process -Verb RunAs`, which shows the UAC prompt. On other platforms
/// there is no standard per-launch elevation prompt, so it runs normally.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
        // The Agent Board sidecar (CPE-850): the out-of-process Kanban over Ticketing/. Bundled +
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", derive(specta::Type))]
struct ConsentState {
    requested: Vec<sidecar_contract::Capability>,
    granted: Vec<sidecar_contract::Capability>,
    undecided: Vec<sidecar_contract::Capability>,
}

#[cfg(feature = "sidecar-platform")]
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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

/// The sidecars we ship a pristine restore copy for (CPE-867). Matches the `.pristine` bundle entries.
#[cfg(feature = "sidecar-platform")]
const RESTORABLE_SIDECARS: [&str; 3] = ["ai-console", "agent-board", "repos"];

/// Restore a sidecar's executable from its never-executed `.pristine` copy when the exe is missing or its
/// bytes differ from the pristine (CPE-867 L2). Because nothing ever *runs* a `.pristine`, a locked-file
/// install skip (CPE-483) can't leave *it* stale — so it's a trustworthy restore source that self-heals a
/// missing/stale exe. Pure enough to unit-test. Returns `Ok(true)` if it restored; `Ok(false)` when the exe
/// is already current or there is no pristine (e.g. dev builds, where this is a no-op).
#[cfg(feature = "sidecar-platform")]
fn restore_sidecar_from_pristine(exe: &Path, pristine: &Path) -> std::io::Result<bool> {
    if !pristine.exists() {
        return Ok(false);
    }
    let pristine_bytes = std::fs::read(pristine)?;
    if exe.exists() {
        if let Ok(current) = std::fs::read(exe) {
            if current == pristine_bytes {
                return Ok(false); // already current — nothing to heal
            }
        }
    }
    std::fs::write(exe, &pristine_bytes)?;
    Ok(true)
}

/// Startup self-heal (CPE-867 L2): for each bundled sidecar, restore its runtime exe from the pristine copy
/// if it's missing or stale. Best-effort — a failure only logs. Runs AFTER the orphan-daemon reap so a
/// daemon that was file-locking the exe is already gone.
#[cfg(feature = "sidecar-platform")]
fn restore_stale_sidecars_on_startup(app: &tauri::AppHandle) {
    use tauri::Manager;
    let Ok(resource) = app.path().resource_dir() else { return };
    let dir = resource.join("sidecars");
    for id in RESTORABLE_SIDECARS {
        let exe_name = if cfg!(windows) { format!("{id}.exe") } else { id.to_string() };
        let exe = dir.join(&exe_name);
        let pristine = dir.join(format!("{id}.pristine"));
        match restore_sidecar_from_pristine(&exe, &pristine) {
            Ok(true) => eprintln!("cpe: self-healed sidecar '{id}' from its pristine copy (CPE-867)"),
            Ok(false) => {}
            Err(e) => eprintln!("cpe: could not restore sidecar '{id}': {e}"),
        }
    }
}

/// Run `op(attempt)` up to `attempts` times, sleeping `base_delay * attempt` between tries (linear
/// backoff), returning the first `Ok` or the last `Err` (CPE-868 L3). `sleep` is injected so this is pure
/// to unit-test. Sleeps *between* attempts, never after the last one.
#[cfg(feature = "sidecar-platform")]
fn retry_with_backoff<T, E>(
    attempts: usize,
    base_delay: std::time::Duration,
    mut sleep: impl FnMut(std::time::Duration),
    mut op: impl FnMut(usize) -> Result<T, E>,
) -> Result<T, E> {
    let attempts = attempts.max(1);
    let mut last_err = None;
    for attempt in 1..=attempts {
        match op(attempt) {
            Ok(v) => return Ok(v),
            Err(e) => {
                last_err = Some(e);
                if attempt < attempts {
                    sleep(base_delay.saturating_mul(attempt as u32));
                }
            }
        }
    }
    Err(last_err.expect("at least one attempt ran"))
}

#[cfg(all(test, feature = "sidecar-platform"))]
mod restore_tests {
    use super::{restore_sidecar_from_pristine, retry_with_backoff};

    #[test]
    fn retry_returns_first_ok_and_sleeps_between_tries() {
        let (mut sleeps, mut tries) = (0u32, 0u32);
        let r: Result<&str, &str> = retry_with_backoff(
            5,
            std::time::Duration::from_millis(1),
            |_| sleeps += 1,
            |attempt| { tries += 1; if attempt >= 3 { Ok("ok") } else { Err("transient") } },
        );
        assert_eq!(r, Ok("ok"));
        assert_eq!(tries, 3);
        assert_eq!(sleeps, 2); // slept after attempts 1 and 2, not after the successful 3rd
    }

    #[test]
    fn retry_returns_last_err_after_exhausting_attempts() {
        let mut sleeps = 0u32;
        let r: Result<&str, i32> = retry_with_backoff(
            3,
            std::time::Duration::from_millis(1),
            |_| sleeps += 1,
            |attempt| Err(attempt as i32),
        );
        assert_eq!(r, Err(3)); // the last error is surfaced
        assert_eq!(sleeps, 2); // no sleep after the final attempt
    }

    #[test]
    fn restores_when_missing_or_stale_and_noops_when_current() {
        let tmp = tempfile::tempdir().unwrap();
        let exe = tmp.path().join("ai-console.exe");
        let pristine = tmp.path().join("ai-console.pristine");

        // No pristine (e.g. a dev build) → no-op.
        assert!(!restore_sidecar_from_pristine(&exe, &pristine).unwrap());

        std::fs::write(&pristine, b"NEW-BINARY-BYTES").unwrap();
        // Exe missing → restored from pristine.
        assert!(restore_sidecar_from_pristine(&exe, &pristine).unwrap());
        assert_eq!(std::fs::read(&exe).unwrap(), b"NEW-BINARY-BYTES");
        // Exe already current → no-op.
        assert!(!restore_sidecar_from_pristine(&exe, &pristine).unwrap());
        // Exe stale (a locked-file install skip left old bytes) → restored.
        std::fs::write(&exe, b"OLD-STALE-BYTES").unwrap();
        assert!(restore_sidecar_from_pristine(&exe, &pristine).unwrap());
        assert_eq!(std::fs::read(&exe).unwrap(), b"NEW-BINARY-BYTES");
    }
}

/// One row in the platform management UI (CPE-274): a registered sidecar with its
/// identity, contract compatibility, running/enabled state, and consent picture.
#[cfg(feature = "sidecar-platform")]
#[derive(serde::Serialize)]
#[cfg_attr(feature = "specta-bindings", derive(specta::Type))]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", derive(specta::Type))]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
                                // The reporting session IS the actor for a read (CPE-1101); the
                                // sidecar now stamps `sessionId` on the announcement (mirrors the
                                // `cost:` frame). Fall back to "unknown" for an older payload that
                                // predates the tag, so the read still lands attributed-but-honest.
                                let actor = v
                                    .get("sessionId")
                                    .and_then(|s| s.as_str())
                                    .unwrap_or("unknown");
                                let _ = app.emit(
                                    "ai-console://fs-activity",
                                    serde_json::json!([{ "kind": "read", "path": path, "actor": actor }]),
                                );
                            }
                        }
                    }
                    // Agent Watch cost ledger (CPE-1097): the console taps its own PTY output for a
                    // provider-reported usage/cost line (best-effort, advisory — see `usage.rs`) and
                    // announces it as a `cost:<json {sessionId, inputTokens, outputTokens, costUsd}>`
                    // Status whenever the scanned figures change. Forward it verbatim as
                    // `ai-console://agent-cost` so the cost-ledger panel (CPE-1098) can render it.
                    // Malformed payloads are ignored (never block the terminal).
                    Message::Event(sidecar_contract::Event::Status { state })
                        if state.starts_with("cost:") =>
                    {
                        use tauri::Emitter;
                        // Typed, validated payload: parse into a struct so a malformed/partial frame is
                        // DROPPED rather than forwarded (more defensive than a bare Value), and the
                        // cost-ledger panel (CPE-1098) has a stable shape to consume. Advisory only
                        // (best-effort PTY scrape — see `usage.rs`), never billing.
                        #[derive(Clone, serde::Deserialize, serde::Serialize)]
                        #[serde(rename_all = "camelCase")]
                        struct AgentCostEvent {
                            session_id: String,
                            input_tokens: u64,
                            output_tokens: u64,
                            cost_usd: f64,
                        }
                        if let Ok(ev) =
                            serde_json::from_str::<AgentCostEvent>(&state["cost:".len()..])
                        {
                            let _ = app.emit("ai-console://agent-cost", ev);
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

/// The live filesystem watchers for Agent Watch — one per running agent session, keyed by
/// `sessionId` (CPE-1099). Watching every running session concurrently backs the conflict radar;
/// each stored `notify` watcher (plus its pump thread) stays alive only while its key is present.
/// Removing a key (or clearing the map) drops the watcher AND ends its emitter thread (the event
/// channel closes). Off means off (AGENT-WATCH.md): an **empty map is zero watchers and zero
/// threads** — with no session running nothing here is allocated or spinning.
#[cfg(feature = "sidecar-platform")]
#[derive(Default)]
struct AgentWatchState {
    watches: std::sync::Mutex<std::collections::HashMap<String, AgentWatch>>,
    /// App-op ledger for the per-event **actor** tag (CPE-1101): the explorer records the target
    /// path(s) of a user-initiated file op here *just before* mutating, so `flush_fs_batch` can tag
    /// the resulting watcher event `actor:"user"` instead of the owning session id. Entries are
    /// `(normalized path, when-recorded)` and age out **in-line** during flush (fresh = within
    /// `APP_OP_TTL`) — deliberately **no background thread and no timer**. Empty when no user op is
    /// in flight, so it stays a zero-cost Mutex; and `note_app_op` is a compile-time no-op without
    /// this feature, so the plain explorer never even records.
    app_ops: std::sync::Mutex<std::collections::VecDeque<(String, std::time::Instant)>>,
}

#[cfg(feature = "sidecar-platform")]
impl AgentWatchState {
    /// Arm (or re-arm) a session's watch. Re-inserting an existing key drops the old `AgentWatch`
    /// (its watcher tx closes → pump exits) before storing the new one; a new key leaves the other
    /// watches running.
    fn arm(&self, session_id: String, watch: AgentWatch) {
        self.watches.lock().unwrap().insert(session_id, watch);
    }

    /// Disarm just one session — drop its watch (pump thread exits on the closed channel). Idempotent.
    fn disarm(&self, session_id: &str) {
        self.watches.lock().unwrap().remove(session_id);
    }

    /// Disarm every session at once (whole Agent Deck stopped). The map is emptied → 0 threads.
    fn disarm_all(&self) {
        self.watches.lock().unwrap().clear();
    }

    #[cfg(test)]
    fn armed_count(&self) -> usize {
        self.watches.lock().unwrap().len()
    }

    #[cfg(test)]
    fn is_empty(&self) -> bool {
        self.watches.lock().unwrap().is_empty()
    }
}

#[cfg(feature = "sidecar-platform")]
struct AgentWatch {
    _watcher: notify::RecommendedWatcher,
    #[allow(dead_code)]
    path: String,
}

// --- Agent Watch: per-event actor tags (CPE-1101) --------------------------------------
// The conflict radar (CPE-1100) needs to know WHO touched a path — the owning agent session, the
// user (via the explorer's own file ops), or an unattributable process ("unknown"). We tag every
// fs-activity / fs-diff item with an `actor` string:
//   * a `sessionId`  — default: the pump that saw the event owns its session id;
//   * `"user"`       — the event matches a path the explorer itself just mutated (the app-op ledger);
//   * `"unknown"`    — the owning session's watch is already gone (a pump-drain race on shutdown).

/// How long an app-op ledger entry stays "fresh": a watcher event within this window of the recorded
/// mutation is attributed to the user. Long enough to bridge the ~200ms pump coalesce plus disk lag,
/// short enough that an unrelated later write to the same path is NOT mis-tagged.
#[cfg(feature = "sidecar-platform")]
const APP_OP_TTL: std::time::Duration = std::time::Duration::from_millis(2000);

/// Hard cap on the app-op ledger so a burst of file ops can't grow it without bound (it's normally
/// tiny and drained by matching flushes). Oldest entries are dropped first.
#[cfg(feature = "sidecar-platform")]
const APP_OP_CAP: usize = 512;

/// Normalise a path for cross-source comparison: unify separators and (on the case-insensitive
/// Windows filesystem) fold case, so a ledger entry recorded by the explorer matches the absolute
/// path the `notify` watcher later emits regardless of `\` vs `/` or casing.
#[cfg(feature = "sidecar-platform")]
fn normalize_op_path(p: &str) -> String {
    let s = p.replace('\\', "/");
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s
    }
}

/// Record the target path(s) of a user-initiated explorer file op in the app-op ledger, *just before*
/// the mutation, so the resulting watcher event is attributed to `"user"` (CPE-1101). Called at the
/// explorer's file-op command sites (rename / delete / copy / move / move_exact / create_dir /
/// create_file / write_file_text).
///
/// `paths` is a **closure** returning the target paths, not an eager slice, on purpose: in the plain
/// (non-`sidecar-platform`) build this whole function is a no-op that **never calls the closure**, so
/// the plain explorer pays nothing — not even the cost of computing the target paths. Off means off.
#[cfg(feature = "sidecar-platform")]
#[inline]
fn note_app_op(app: &tauri::AppHandle, paths: impl FnOnce() -> Vec<String>) {
    use tauri::Manager;
    let Some(state) = app.try_state::<AgentWatchState>() else {
        return;
    };
    let now = std::time::Instant::now();
    let mut ledger = state.app_ops.lock().unwrap();
    // Age out stale entries in-line (no timer) — the front is always the oldest.
    while let Some((_, t)) = ledger.front() {
        if now.duration_since(*t) > APP_OP_TTL {
            ledger.pop_front();
        } else {
            break;
        }
    }
    for p in paths() {
        if ledger.len() >= APP_OP_CAP {
            ledger.pop_front();
        }
        ledger.push_back((normalize_op_path(&p), now));
    }
}

/// The plain-build no-op: the closure is **never invoked**, so the target-path computation at the call
/// site is elided and the explorer pays nothing (AgentWatchState doesn't even exist here).
#[cfg(not(feature = "sidecar-platform"))]
#[inline]
fn note_app_op(_app: &tauri::AppHandle, _paths: impl FnOnce() -> Vec<String>) {}

/// Resolve the `actor` for one watcher event path: a fresh app-op ledger match → `"user"` (the entry
/// is **consumed** so it can't attribute a second event); otherwise the owning `session_id`, or
/// `"unknown"` when that session's watch is already gone (a drain race — better than a dead id).
/// Pure over its inputs so it's unit-testable without any Tauri `AppHandle`.
#[cfg(feature = "sidecar-platform")]
fn resolve_actor(
    ledger: &mut std::collections::VecDeque<(String, std::time::Instant)>,
    session_present: bool,
    session_id: &str,
    path: &str,
    now: std::time::Instant,
) -> String {
    let norm = normalize_op_path(path);
    if let Some(idx) = ledger
        .iter()
        .position(|(p, t)| *p == norm && now.duration_since(*t) <= APP_OP_TTL)
    {
        ledger.remove(idx);
        return "user".to_string();
    }
    if session_present {
        session_id.to_string()
    } else {
        "unknown".to_string()
    }
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

/// CPE-1117: insert a single path into the coalesce window as `"renamed"`, honouring the same
/// "removed wins" rule as the main pump loop (a rename that's then deleted in-window reads as gone).
#[cfg(feature = "sidecar-platform")]
fn mark_renamed(pending: &mut std::collections::HashMap<String, &'static str>, path: String) {
    let slot = pending.entry(path).or_insert("renamed");
    if *slot != "removed" {
        *slot = "renamed";
    }
}

/// CPE-1117: record a fully-resolved rename `from`→`to`. The *target* becomes the coalesced
/// `"renamed"` activity item, and its source is stashed in `rename_from[to]` for `flush_fs_batch`
/// to attach as `from`/`to`. `flush` only reads `rename_from` for items still `"renamed"` at flush
/// time, so a target that's removed-in-window silently drops its now-stale source (no false pair).
#[cfg(feature = "sidecar-platform")]
fn record_rename_pair(
    pending: &mut std::collections::HashMap<String, &'static str>,
    rename_from: &mut std::collections::HashMap<String, String>,
    from: String,
    to: String,
) {
    mark_renamed(pending, to.clone());
    rename_from.insert(to, from);
}

/// CPE-1117: fold one raw rename event into the coalesce window, capturing its source→target pair.
///
/// `notify` reports renames in one of two shapes and this handles both:
///   * a single `RenameMode::Both` event whose `paths` are `[from, to]` (macOS FSEvents and some
///     backends) — paired immediately;
///   * a `RenameMode::From` then `RenameMode::To` split sharing a tracker *cookie*
///     (Windows `ReadDirectoryChangesW`, Linux inotify) — the source is held in `pending_by_cookie`
///     until its partner arrives inside the same ~200ms flush window.
///
/// ## Fidelity ceiling (decide-and-log, CPE-1117)
/// Where the platform cannot give us a pair we degrade to today's single-path `"renamed"` — no
/// crash, no fabricated pair. That covers: `RenameMode::Any`/`Other` events (some Linux inotify
/// backends never emit `Both` *or* a cookie); a `From`/`To` with no tracker cookie to correlate on;
/// and a `From` whose matching `To` never arrives (moved *out of* the watched tree, or split across
/// a flush boundary — see [`fold_orphan_renames`]). Competing-rename detection (CPE-1118) simply
/// sees fewer pairs on those platforms; it never sees a wrong one.
#[cfg(feature = "sidecar-platform")]
fn handle_rename_event(
    event: &notify::Event,
    pending: &mut std::collections::HashMap<String, &'static str>,
    rename_from: &mut std::collections::HashMap<String, String>,
    pending_by_cookie: &mut std::collections::HashMap<usize, String>,
) {
    use notify::event::{ModifyKind, RenameMode};
    use notify::EventKind::Modify;

    // classify_fs_event only routes `Modify(Name(_))` here; default defensively to `Any`.
    let mode = match event.kind {
        Modify(ModifyKind::Name(m)) => m,
        _ => RenameMode::Any,
    };
    let mut paths = event.paths.iter().map(|p| p.to_string_lossy().into_owned());

    match mode {
        RenameMode::Both => {
            // Paths are `[from, to]` in that exact order (notify contract). Pair only when both are
            // present; a malformed single-path `Both` degrades to single-path.
            match (paths.next(), paths.next()) {
                (Some(from), Some(to)) => record_rename_pair(pending, rename_from, from, to),
                (Some(only), None) => mark_renamed(pending, only),
                _ => {}
            }
        }
        RenameMode::From => match (event.attrs.tracker(), paths.next()) {
            // Hold the source until its `To` partner arrives (correlated by cookie).
            (Some(cookie), Some(from)) => {
                pending_by_cookie.insert(cookie, from);
            }
            // No cookie to pair on — emit the source as a single-path rename now.
            (None, Some(from)) => mark_renamed(pending, from),
            _ => {}
        },
        RenameMode::To => match (event.attrs.tracker(), paths.next()) {
            (Some(cookie), Some(to)) => match pending_by_cookie.remove(&cookie) {
                Some(from) => record_rename_pair(pending, rename_from, from, to),
                // A `To` whose `From` we never saw — single-path fallback.
                None => mark_renamed(pending, to),
            },
            (None, Some(to)) => mark_renamed(pending, to),
            _ => {}
        },
        // `Any`/`Other`: the backend didn't tell us direction — emit each path single-path.
        _ => {
            for p in paths {
                mark_renamed(pending, p);
            }
        }
    }
}

/// CPE-1117: any rename source still waiting for its `To` when the window flushes degrades to a
/// single-path `"renamed"` (fidelity fallback — see [`handle_rename_event`]). Called at the top of
/// every `flush_fs_batch` so an unmatched `From` is never lost and never lingers past its window.
#[cfg(feature = "sidecar-platform")]
fn fold_orphan_renames(
    pending: &mut std::collections::HashMap<String, &'static str>,
    pending_by_cookie: &mut std::collections::HashMap<usize, String>,
) {
    for (_cookie, from) in pending_by_cookie.drain() {
        mark_renamed(pending, from);
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
    session_id: String,
) {
    use std::collections::HashMap;
    use std::sync::mpsc::RecvTimeoutError;
    use std::time::{Duration, Instant};

    const FLUSH: Duration = Duration::from_millis(200);
    const CAP: usize = 500;
    let mut pending: HashMap<String, &'static str> = HashMap::new();
    // CPE-1117: a renamed *target* path -> its *source* path, drained by `flush_fs_batch` to attach
    // `from`/`to` to the renamed activity item (absent target => single-path fallback).
    let mut rename_from: HashMap<String, String> = HashMap::new();
    // CPE-1117: half-seen renames awaiting their partner, keyed by notify's tracker cookie (the
    // Windows/Linux `From`-then-`To` split). Paired within the flush window by `handle_rename_event`;
    // orphans fall back to single-path in `fold_orphan_renames`.
    let mut pending_by_cookie: HashMap<usize, String> = HashMap::new();
    let mut shadow = agent_shadow::ShadowStore::new();
    let mut last_flush = Instant::now();

    loop {
        match rx.recv_timeout(FLUSH) {
            Ok(Ok(event)) => {
                if let Some(kind) = classify_fs_event(&event.kind) {
                    if kind == "renamed" {
                        // CPE-1117: capture the source->target pair (or degrade gracefully to a
                        // single-path `renamed`); see `handle_rename_event` for the fidelity ceiling.
                        handle_rename_event(
                            &event,
                            &mut pending,
                            &mut rename_from,
                            &mut pending_by_cookie,
                        );
                    } else {
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
                }
                // Cap on either the emit queue or the (normally tiny) in-flight rename map, so a
                // flood of unpaired `From` events can't grow memory unbounded between timer flushes.
                if pending.len() >= CAP || pending_by_cookie.len() >= CAP {
                    flush_fs_batch(
                        &app,
                        &mut pending,
                        &mut rename_from,
                        &mut pending_by_cookie,
                        &mut shadow,
                        &session_id,
                    );
                    last_flush = Instant::now();
                }
            }
            Ok(Err(_)) => {} // a watch error — ignore, keep pumping
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                flush_fs_batch(
                    &app,
                    &mut pending,
                    &mut rename_from,
                    &mut pending_by_cookie,
                    &mut shadow,
                    &session_id,
                );
                break;
            }
        }
        if last_flush.elapsed() >= FLUSH {
            flush_fs_batch(
                &app,
                &mut pending,
                &mut rename_from,
                &mut pending_by_cookie,
                &mut shadow,
                &session_id,
            );
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
    rename_from: &mut std::collections::HashMap<String, String>,
    pending_by_cookie: &mut std::collections::HashMap<usize, String>,
    shadow: &mut agent_shadow::ShadowStore,
    session_id: &str,
) {
    use serde_json::json;
    use tauri::{Emitter, Manager};

    // CPE-1117: fold any rename source whose matching `To` never arrived within this window into the
    // batch as a single-path `renamed` (fidelity fallback) *before* deciding there's nothing to flush.
    fold_orphan_renames(pending, pending_by_cookie);

    if pending.is_empty() {
        // No `renamed` items => any stray captured sources are moot; drop them so nothing leaks
        // into a later window and gets mis-attached.
        rename_from.clear();
        return;
    }
    // Per-event actor tag (CPE-1101). The owning session id is the default; a fresh app-op ledger
    // match upgrades an event to "user"; and if this pump's session key is already gone (a shutdown
    // drain race) we emit "unknown" rather than a dead id. State is always managed under this feature,
    // but we degrade gracefully to the session id if it somehow isn't.
    let now = std::time::Instant::now();
    // One epoch-ms stamp for this drained batch (server-side, so the persisted log can't be skewed).
    let ts = to_epoch_ms(SystemTime::now()).unwrap_or(0);
    let watch_state = app.try_state::<AgentWatchState>();
    let session_present = watch_state
        .as_ref()
        .map(|s| s.watches.lock().unwrap().contains_key(session_id))
        .unwrap_or(true);

    let mut activity = Vec::with_capacity(pending.len());
    let mut diffs = Vec::new();
    // CPE-1108: durable per-event journal for replay — built alongside the live emit below.
    let mut events: Vec<audit_journal::AuditEvent> = Vec::with_capacity(pending.len());
    for (path, kind) in pending.drain() {
        let actor = match watch_state.as_ref() {
            Some(s) => {
                let mut ledger = s.app_ops.lock().unwrap();
                resolve_actor(&mut ledger, session_present, session_id, &path, now)
            }
            None if session_present => session_id.to_string(),
            None => "unknown".to_string(),
        };
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
            diffs.push(json!({ "path": r.path, "before": r.before, "after": r.after, "actor": actor.clone() }));
        }
        // Persist this event (CPE-1108) before `path`/`actor` move into the live activity item.
        events.push(audit_journal::AuditEvent {
            ts,
            session: session_id.to_string(),
            kind: kind.to_string(),
            path: path.clone(),
            actor: Some(actor.clone()),
            detail: None,
        });
        // CPE-1117: attach the captured source for a paired rename; a `renamed` item with no
        // recorded source (unpaired on this platform/event) stays single-path — no `from`/`to`.
        let renamed_from = if kind == "renamed" { rename_from.remove(&path) } else { None };
        let mut item = json!({ "kind": kind, "path": path, "actor": actor });
        if let Some(from) = renamed_from {
            // `path` (now the item's `path`) is the rename target; surface it as `to` too so the
            // competing-rename fold (CPE-1118) reads `from`/`to` directly.
            item["to"] = item["path"].clone();
            item["from"] = json!(from);
        }
        activity.push(item);
    }
    // Drop any leftover captured sources whose target was removed/superseded in-window (so it was
    // never emitted as `renamed`) — never carry a source into a later window.
    rename_from.clear();
    // Append the batch to the durable audit journal (CPE-1108) so the session's full ordered event
    // log survives beyond the 300-cap in-memory timeline for replay. This lives only on the pump
    // thread, so nothing is written when no session is armed (off means off). A journal-write error
    // must never break the live pump/activity view: log it and carry on.
    match audit_dir(app) {
        Ok(dir) => {
            if let Err(e) =
                audit_journal::record_many(&dir, &events, audit_journal::MAX_EVENTS_PER_SESSION)
            {
                eprintln!("[agent-watch] audit journal append failed (swallowed): {e}");
            }
        }
        Err(e) => eprintln!("[agent-watch] audit dir unavailable, skipping journal append: {e}"),
    }
    let _ = app.emit("ai-console://fs-activity", activity);
    if !diffs.is_empty() {
        let _ = app.emit("ai-console://fs-diff", diffs);
    }
}

/// Start watching one agent session's Project folder for filesystem activity (CPE-398/1099). Keyed
/// by `session_id`: this ADDs a watch (it does not replace the others), so every running session is
/// watched concurrently. Re-arming the same `session_id` drops that session's prior watch. Non-fatal:
/// returns an error string the caller can surface. A missing folder (e.g. a since-deleted path) is
/// rejected rather than silently watching nothing.
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn agent_watch_start(
    app: tauri::AppHandle,
    state: tauri::State<AgentWatchState>,
    session_id: String,
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
    // Dropping the watcher (when this session's key is removed/re-armed, or the map is cleared)
    // closes `rx` and ends this thread — no separate stop signal needed.
    // The pump owns its session id so every event it flushes can default `actor` to it (CPE-1101).
    let pump_session = session_id.clone();
    // CPE-1109 (epic CPE-728, slice b): capture a bounded baseline snapshot of the watched tree once,
    // here at watch-start, so replay can reconstruct pre-existing untouched files (which never emit an
    // event). Off-means-off — this runs ONLY inside agent_watch_start; no armed watch => no capture.
    // Representation B: a separate <session>.baseline.json beside the journal, never synthetic events in
    // the activity log. Runs on its own thread (a large walk must not delay the watch going live) and a
    // capture/write failure is logged + swallowed, degrading replay to events-only rather than breaking
    // watching.
    {
        let baseline_app = app.clone();
        let baseline_session = session_id.clone();
        let baseline_path = path.clone();
        std::thread::spawn(move || {
            let baseline = cpe_server::replay_baseline::capture(&baseline_path);
            match audit_dir(&baseline_app) {
                Ok(dir) => {
                    if let Err(e) =
                        cpe_server::replay_baseline::write_baseline(&dir, &baseline_session, &baseline)
                    {
                        eprintln!("[agent-watch] baseline write failed (swallowed): {e}");
                    }
                }
                Err(e) => eprintln!("[agent-watch] audit dir unavailable, skipping baseline: {e}"),
            }
        });
    }
    std::thread::spawn(move || fs_activity_pump(app, rx, pump_session));
    state.arm(session_id, AgentWatch { _watcher: watcher, path });
    Ok(())
}

/// Stop watching one session (CPE-398/1099). Dropping that session's stored watcher ends its emitter
/// thread; the other sessions keep watching. Idempotent.
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn agent_watch_stop(state: tauri::State<AgentWatchState>, session_id: String) {
    state.disarm(&session_id);
}

/// Stop watching every session at once (CPE-1099) — used when the whole Agent Deck is stopped. Clears
/// the map, so all watchers drop and all pump threads exit: back to zero threads (off means off).
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn agent_watch_stop_all(state: tauri::State<AgentWatchState>) {
    state.disarm_all();
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn folder_watch_stop(state: tauri::State<FolderWatchState>) {
    *state.current.lock().unwrap() = None;
}

// --- Instant index: live watcher adapter (CPE-1138, epic CPE-703) ----------------------------
// The pure event→mutation mapping (`cpe_server::index_watch::{WatchEvent, plan_from_events}`) and the
// batch-applying `IndexService::apply_mutations` live in `cpe-server` (notify-free, unit-tested
// headlessly). This is the thin OS-watch half: a `notify` watcher per resident volume that feeds a
// debounced pump, which resolves rename-cookie pairs, plans mutations, and applies them. Mirrors
// `FolderWatchState`/`AgentWatchState` — gated behind `sidecar-platform` (the plain explorer pulls no
// watcher machinery), and it's `index_build`/`index_drop`/`index_clear` that arm/disarm it, so a watcher
// only exists while its volume is actually resident (off means off).

/// The live index watchers, one per resident volume, keyed by `volume_id`. Each stored `notify` watcher
/// (plus its pump thread) stays alive only while its key is present — removing a key (or clearing the
/// map) drops the watcher AND ends its pump thread (the event channel closes). An empty map is zero
/// watchers and zero threads.
#[cfg(feature = "sidecar-platform")]
#[derive(Default)]
struct IndexWatchState {
    watches: std::sync::Mutex<std::collections::HashMap<u64, IndexWatch>>,
}

#[cfg(feature = "sidecar-platform")]
struct IndexWatch {
    _watcher: notify::RecommendedWatcher,
    #[allow(dead_code)]
    root: String,
}

#[cfg(feature = "sidecar-platform")]
impl IndexWatchState {
    /// Insert (or replace) `volume_id`'s watch. Re-arming an already-watched volume drops the old
    /// `IndexWatch` first (its pump thread exits on the closed channel) before storing the new one —
    /// the map-lifecycle half of arming, kept free of any `notify`/`AppHandle` setup so it's directly
    /// testable (mirrors `AgentWatchState::arm`).
    fn arm(&self, volume_id: u64, watch: IndexWatch) {
        self.watches.lock().unwrap().insert(volume_id, watch);
    }

    /// Disarm just one volume's watcher (its pump exits on the closed channel). Idempotent.
    fn stop(&self, volume_id: u64) {
        self.watches.lock().unwrap().remove(&volume_id);
    }

    /// Disarm every watcher at once (e.g. `index_clear`). The map is emptied → 0 threads.
    fn stop_all(&self) {
        self.watches.lock().unwrap().clear();
    }

    #[cfg(test)]
    fn armed_count(&self) -> usize {
        self.watches.lock().unwrap().len()
    }
}

/// Start (or re-arm) `volume_id`'s live watcher over `root` if the sidecar platform is enabled;
/// a compile-time no-op otherwise (the plain explorer build has no `IndexWatchState`/`notify` at all).
/// A `notify` setup failure is swallowed — the index just stays live-search-stale for that volume
/// rather than failing the build that triggered this.
#[cfg(feature = "sidecar-platform")]
fn index_watch_start(app: &tauri::AppHandle, volume_id: u64, root: &str) {
    use notify::{RecursiveMode, Watcher};
    use tauri::Manager;
    let Some(state) = app.try_state::<IndexWatchState>() else { return };
    let (tx, rx) = std::sync::mpsc::channel();
    let Ok(mut watcher) = notify::recommended_watcher(move |res| {
        let _ = tx.send(res);
    }) else {
        return;
    };
    if watcher.watch(std::path::Path::new(root), RecursiveMode::Recursive).is_err() {
        return;
    }
    let app_for_pump = app.clone();
    let root_owned = root.to_string();
    std::thread::spawn(move || index_watch_pump(app_for_pump, rx, volume_id, root_owned));
    state.arm(volume_id, IndexWatch { _watcher: watcher, root: root.to_string() });
}
#[cfg(not(feature = "sidecar-platform"))]
#[inline]
fn index_watch_start(_app: &tauri::AppHandle, _volume_id: u64, _root: &str) {}

/// Stop `volume_id`'s live watcher (`index_drop`/a superseded build); a no-op if none is armed.
#[cfg(feature = "sidecar-platform")]
fn index_watch_stop(app: &tauri::AppHandle, volume_id: u64) {
    use tauri::Manager;
    if let Some(state) = app.try_state::<IndexWatchState>() {
        state.stop(volume_id);
    }
}
#[cfg(not(feature = "sidecar-platform"))]
#[inline]
fn index_watch_stop(_app: &tauri::AppHandle, _volume_id: u64) {}

/// Stop every live index watcher (`index_clear`); a no-op if none are armed.
#[cfg(feature = "sidecar-platform")]
fn index_watch_stop_all(app: &tauri::AppHandle) {
    use tauri::Manager;
    if let Some(state) = app.try_state::<IndexWatchState>() {
        state.stop_all();
    }
}
#[cfg(not(feature = "sidecar-platform"))]
#[inline]
fn index_watch_stop_all(_app: &tauri::AppHandle) {}

/// Coalescing pump for one volume's index watcher: fold raw `notify` events over a short debounce
/// window, resolve rename-cookie pairs (mirrors `handle_rename_event`'s fidelity ceiling, CPE-1117 —
/// a pairable rename becomes one [`cpe_server::index_watch::WatchEvent::Renamed`]; anything unpairable
/// degrades to a plain create/remove at the one path known), then plan + apply the whole window as ONE
/// batch via [`cpe_server::index_service::IndexService::apply_mutations`] — one lock acquisition, at
/// most one save, no matter how many events landed in the window. Ends when the channel closes (the
/// watcher was dropped by `stop`/`stop_all`).
#[cfg(feature = "sidecar-platform")]
fn index_watch_pump(
    app: tauri::AppHandle,
    rx: std::sync::mpsc::Receiver<notify::Result<notify::Event>>,
    volume_id: u64,
    _root: String,
) {
    use cpe_server::index_watch::{plan_from_events, WatchEvent};
    use notify::event::{ModifyKind, RenameMode};
    use notify::EventKind;
    use std::collections::{HashMap, HashSet};
    use std::sync::mpsc::RecvTimeoutError;
    use std::time::{Duration, Instant};
    use tauri::Manager;

    const FLUSH: Duration = Duration::from_millis(300);
    // Paths whose exact create/remove identity we resolve at FLUSH time by re-stat'ing (below) —
    // covers plain Create/Remove events, "Any"/"Other" renames, and any rename half `notify` never
    // paired within the window. Re-checking existence at flush (rather than trusting the individual
    // event kind) also self-corrects a same-window create-then-delete or delete-then-recreate.
    let mut touched: HashSet<String> = HashSet::new();
    // Cookie-paired renames, resolved as soon as both halves arrive.
    let mut renamed_pairs: Vec<(String, String)> = Vec::new();
    // Half-seen renames awaiting their partner, keyed by notify's tracker cookie.
    let mut pending_by_cookie: HashMap<usize, String> = HashMap::new();
    let mut last_flush = Instant::now();

    let flush = |app: &tauri::AppHandle,
                 touched: &mut HashSet<String>,
                 renamed_pairs: &mut Vec<(String, String)>,
                 pending_by_cookie: &mut HashMap<usize, String>| {
        // A `From` still waiting for its `To` when the window closes: the OS never gave us the pair
        // (moved out of the watched tree, or split across a flush boundary) — fold it into the
        // existence re-check below (the ticket's "rename-as-remove+create" fallback).
        for (_cookie, from) in pending_by_cookie.drain() {
            touched.insert(from);
        }
        if touched.is_empty() && renamed_pairs.is_empty() {
            return;
        }
        let mut events: Vec<WatchEvent> = renamed_pairs
            .drain(..)
            .map(|(from, to)| WatchEvent::Renamed { from, to })
            .collect();
        // Resolve the re-stat set into Created/Removed events with ancestors ordered before descendants
        // (CPE-1138 review): `HashSet` drain order is arbitrary, and `apply_create` silently drops a child
        // whose parent dir isn't indexed yet — so a same-window `mkdir a && touch a/f` must apply `a`
        // before `a/f`. `resolve_touched` sorts to guarantee that.
        let touched_paths: Vec<String> = touched.drain().collect();
        events.extend(cpe_server::index_watch::resolve_touched(&touched_paths, |p| {
            let path = std::path::Path::new(p);
            path.exists().then(|| path.is_dir())
        }));
        let mutations = plan_from_events(&events);
        if mutations.is_empty() {
            return;
        }
        let Some(svc) = app.try_state::<cpe_server::index_service::IndexService>() else { return };
        let Ok(dir) = index_dir(app) else { return };
        let _ = svc.apply_mutations(&dir, volume_id, &mutations);
    };

    loop {
        match rx.recv_timeout(FLUSH) {
            Ok(Ok(event)) => match event.kind {
                EventKind::Create(_) | EventKind::Remove(_) => {
                    for p in event.paths {
                        touched.insert(p.to_string_lossy().into_owned());
                    }
                }
                EventKind::Modify(ModifyKind::Name(mode)) => {
                    let mut paths = event.paths.iter().map(|p| p.to_string_lossy().into_owned());
                    match mode {
                        RenameMode::Both => match (paths.next(), paths.next()) {
                            (Some(from), Some(to)) => renamed_pairs.push((from, to)),
                            (Some(only), None) => {
                                touched.insert(only);
                            }
                            _ => {}
                        },
                        RenameMode::From => match (event.attrs.tracker(), paths.next()) {
                            (Some(cookie), Some(from)) => {
                                pending_by_cookie.insert(cookie, from);
                            }
                            (None, Some(from)) => {
                                touched.insert(from);
                            }
                            _ => {}
                        },
                        RenameMode::To => match (event.attrs.tracker(), paths.next()) {
                            (Some(cookie), Some(to)) => match pending_by_cookie.remove(&cookie) {
                                Some(from) => renamed_pairs.push((from, to)),
                                None => {
                                    touched.insert(to);
                                }
                            },
                            (None, Some(to)) => {
                                touched.insert(to);
                            }
                            _ => {}
                        },
                        // `Any`/`Other`: the backend didn't tell us direction — the existence re-check
                        // at flush time decides create vs. remove for each path.
                        _ => {
                            for p in paths {
                                touched.insert(p);
                            }
                        }
                    }
                }
                _ => {} // Modify(Data)/Access/Other — the filename index doesn't track content
            },
            Ok(Err(_)) => {} // a watch error — ignore, keep pumping
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                flush(&app, &mut touched, &mut renamed_pairs, &mut pending_by_cookie);
                break;
            }
        }
        if last_flush.elapsed() >= FLUSH {
            flush(&app, &mut touched, &mut renamed_pairs, &mut pending_by_cookie);
            last_flush = Instant::now();
        }
    }
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
    // Grant only what the user consented to (CPE-296). The frontend prompts for any undecided capability
    // before calling this; whatever isn't granted is withheld, and the sidecar degrades gracefully.
    let consented = sidecar_host::consent::ConsentStore::load(&consent_dir(&app)?).granted("ai-console");
    // Retry a transient spawn/handshake hiccup a few times before surfacing an error (CPE-868 L3). A failed
    // attempt drops its `conn` (killing the process) before the next try, so no orphan is left behind.
    let mut conn = retry_with_backoff(
        3,
        std::time::Duration::from_millis(150),
        std::thread::sleep,
        |_attempt| {
            let mut conn = spawn_process_with_env(&bin, &[], &cat_env)
                .map_err(|e| state.fail(format!("spawn failed: {e}")))?;
            let token = conn.launch_token().to_string();
            handshake(&mut conn, CONTRACT_VERSION, &consented, Some(&token))
                .map_err(|e| state.fail(format!("handshake failed: {e:?}")))?;
            Ok::<_, String>(conn)
        },
    )?;
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
/// so the frontend can frame it in a window (CPE-853, epic CPE-850). The board reads `Ticketing/` under
/// `root` (passed as `CPE_BOARD_ROOT`; falls back to the sidecar's own cwd when absent). The window
/// singleton (by label) prevents duplicate launches, so this deliberately keeps the connection alive on a
/// detached servicing thread rather than a managed reuse state. Non-fatal: returns an error string the UI
/// surfaces.
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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

    let consented = sidecar_host::consent::ConsentStore::load(&consent_dir(&app)?).granted("agent-board");
    // Retry a transient spawn/handshake hiccup a few times before surfacing an error (CPE-868 L3). A failed
    // attempt drops its `conn` (killing the process) before the next try, so no orphan is left behind.
    let mut conn = retry_with_backoff(
        3,
        std::time::Duration::from_millis(150),
        std::thread::sleep,
        |_attempt| {
            let mut conn = spawn_process_with_env(&bin, &[], &env).map_err(|e| format!("spawn failed: {e}"))?;
            let token = conn.launch_token().to_string();
            handshake(&mut conn, CONTRACT_VERSION, &consented, Some(&token))
                .map_err(|e| format!("handshake failed: {e:?}"))?;
            Ok::<_, String>(conn)
        },
    )?;

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
#[cfg_attr(feature = "specta-bindings", derive(specta::Type))]
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
#[cfg_attr(feature = "specta-bindings", derive(specta::Type))]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", derive(specta::Type))]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn forge_admitted_hosts(app: tauri::AppHandle) -> Vec<String> {
    load_admitted_hosts(&app).into_iter().collect()
}

/// Admit ONE host after explicit user consent (CPE-498). Never a wildcard and never a URL: exactly the
/// normalized host is stored, so consenting to `a.example.com` never admits `b.example.com`. Idempotent.
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", derive(specta::Type))]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", derive(specta::Type))]
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
#[cfg_attr(feature = "specta-bindings", derive(specta::Type))]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", derive(specta::Type))]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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
#[cfg_attr(feature = "specta-bindings", specta::specta)]
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

/// Resolve the `--open <dir>` launch argument to a folder to open at startup (CPE-1043). Reads the CLI
/// match in **setup** (proven reliable, unlike a frontend-initiated command), keeps it only when it names
/// an existing directory (via the pure [`cpe_server::launch::resolve_open_dir`]), and returns an absolute
/// path — or `None` (normal startup) when the flag is absent, blank, or not a directory. A relative value
/// resolves against the launch CWD; we deliberately do NOT `canonicalize()` (on Windows that prepends a
/// `\\?\` verbatim prefix that would leak into every displayed path). The frontend reads this synchronously
/// via an injected `window.__CPE_OPEN_DIR__` global, so opening-at-a-folder needs no command or Tauri-
/// presence gate and can't be perturbed by startup timing.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn resolve_startup_open_dir(app: &tauri::AppHandle) -> Option<String> {
    use tauri_plugin_cli::CliExt;
    let matches = app.cli().matches().ok()?;
    let raw = matches.args.get("open").and_then(|a| a.value.as_str().map(str::to_owned));
    let dir = cpe_server::launch::resolve_open_dir(raw.as_deref(), |p| p.is_dir())?;
    let abs = if dir.is_absolute() {
        dir
    } else {
        std::env::current_dir().map(|cwd| cwd.join(&dir)).unwrap_or(dir)
    };
    Some(abs.to_string_lossy().into_owned())
}

/// Read the `--test-mode` launch flag (CPE-1046): when passed, the frontend renders an unmistakable
/// "automated test" halo + banner overlay so a human never worries about (or interferes with) an
/// automated GUI run. Same `CliExt` read as `resolve_startup_open_dir`, just a bare boolean.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn resolve_startup_test_mode(app: &tauri::AppHandle) -> bool {
    use tauri_plugin_cli::CliExt;
    let Ok(matches) = app.cli().matches() else { return false };
    matches!(
        matches.args.get("test-mode").map(|a| &a.value),
        Some(serde_json::Value::Bool(true))
    )
}

/// Build the combined startup `initialization_script` (CPE-1043 `--open` + CPE-1046 `--test-mode`):
/// both deliver a value to the frontend as a `window` global set **before** the app's own scripts run,
/// so neither needs a command or a Tauri-presence gate at startup. Returns `None` when neither flag is
/// present, so a plain launch injects nothing — identical to pre-CPE-1046 behavior.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn startup_init_script(app: &tauri::AppHandle) -> Option<String> {
    let mut script = String::new();
    if let Some(dir) = resolve_startup_open_dir(app) {
        let json = serde_json::to_string(&dir).unwrap_or_else(|_| "null".to_string());
        script.push_str(&format!("window.__CPE_OPEN_DIR__ = {json};"));
    }
    if resolve_startup_test_mode(app) {
        script.push_str("window.__CPE_TEST_MODE__ = true;");
    }
    if script.is_empty() {
        None
    } else {
        Some(script)
    }
}

/// Apply CLI window-geometry flags (CPE-600) to the main window, over whatever `tauri-plugin-window-state`
/// restored — so precedence is `CLI flag > saved state > default`. Monitors have no work-area API in
/// Tauri, so the full monitor bounds are used and the pure resolver clamps the window fully on-screen.
/// A parse/geometry error exits non-zero (never a mangled window); nothing requested → leave as restored.
/// **CPE-1047:** under `--test-mode` the on-screen clamp is skipped (`allow_offscreen`), so an automated
/// GUI-test window can be positioned truly off the visible desktop; a normal launch is unaffected.
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

    // CPE-1047: `--test-mode` opts out of the on-screen clamp so an automated GUI-test window can be
    // positioned truly off-screen (e.g. `--test-mode --x -4000`) and never appear on the user's screen.
    let allow_offscreen = resolve_startup_test_mode(app);
    match geometry::resolve(&args, &monitors, default, allow_offscreen) {
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
            )
            // Spotlight global hotkey (CPE-1215, epic CPE-704): the plugin itself is always
            // initialized (cheap — no OS registration happens here), but NO chord is registered at
            // init time. `register_spotlight_hotkey`/`unregister_spotlight_hotkey` (below) are the
            // only things that actually claim/release an OS-wide hotkey, driven by the Settings
            // toggle — off means no background cost, and there is never a launch-time permission
            // prompt ([[avoid-modal-permission-popups]]).
            .plugin(tauri_plugin_global_shortcut::Builder::new().build());
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

    // Instant-search index service (CPE-1137, epic CPE-703): the resident per-volume indices live here.
    // Always registered — the index is a core-explorer feature, not sidecar — but off means off: the map
    // is empty until `index_build` is invoked, so with the feature unused it costs nothing at startup.
    builder = builder.manage(cpe_server::index_service::IndexService::default());

    // Thumbnail pipeline's shared in-memory hot cache (CPE-1237, epic CPE-718): the `thumb_cache` LRU
    // fronting `thumbnails_stream`'s decoded bytes, persisting across the many streamed calls one scroll
    // session issues. Always registered — thumbnails are a core-explorer feature — but empty until the
    // first request, so it costs nothing when the app never shows a gallery/icon view.
    builder = builder.manage(cpe_server::thumb_pipeline::ThumbCacheService::default());

    // Hold the AI Console sidecar connection in managed state (feature-gated).
    #[cfg(feature = "sidecar-platform")]
    {
        builder = builder.manage(AiConsoleState::default());
        // Agent Watch's filesystem watcher lives here (CPE-398); empty until a folder is watched.
        builder = builder.manage(AgentWatchState::default());
        // The watched-folder rules watcher (CPE-794); empty until the user configures watched folders.
        builder = builder.manage(FolderWatchState::default());
        // The instant-index live watcher (CPE-1138); empty until `index_build` makes a volume resident.
        builder = builder.manage(IndexWatchState::default());
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

            let mut wb = WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
                .title("Cross-Platform Explorer")
                .inner_size(1000.0, 700.0)
                .min_inner_size(600.0, 400.0)
                .on_web_resource_request(|_request, response| {
                    // Local assets, so no-store costs nothing; applied on every response for consistency.
                    response.headers_mut().insert(
                        tauri::http::header::CACHE_CONTROL,
                        tauri::http::HeaderValue::from_static("no-store"),
                    );
                });
            // `--open <dir>` (CPE-1043) + `--test-mode` (CPE-1046): inject both as globals the frontend
            // reads synchronously at startup — one combined script that runs before the app's own scripts,
            // so no command/gate is needed for either. Nothing requested → nothing injected (unchanged
            // pre-CPE-1046 behavior).
            if let Some(script) = startup_init_script(app.handle()) {
                wb = wb.initialization_script(script);
            }
            // `--test-mode` (CPE-1046) also launches the window UNFOCUSED: the halo overlay is the visual
            // half of "don't touch me", but the actual fix for an automated run stealing the user's
            // keyboard/mouse focus is not grabbing OS focus at all. `.focused(false)` only changes launch
            // behavior — the automation (WebDriver etc.) still drives the DOM/window regardless of OS
            // focus, and the user's own foreground app is left alone.
            if resolve_startup_test_mode(app.handle()) {
                wb = wb.focused(false);
            }
            let win = wb.build()?;

            // `main` is in the window-state plugin's skip_initial_state list, so restore its saved
            // geometry here (deterministic) BEFORE the CLI flags override it — restore then override.
            let _ = win.restore_state(StateFlags::all());
            apply_cli_geometry(app.handle());
        }
        #[cfg(feature = "sidecar-platform")]
        reap_orphan_session_daemons_on_startup(app.handle());
        // Self-heal a missing/stale sidecar from its pristine copy (CPE-867 L2) — after the reap, so a
        // daemon that was file-locking the exe is gone before we rewrite it.
        #[cfg(feature = "sidecar-platform")]
        restore_stale_sidecars_on_startup(app.handle());

        // Background scheduled-snapshot timer (CPE-1199, epic CPE-735). Spawned, never joined, so it
        // never blocks startup. Opt-in per folder in Settings and off-means-off: until the user adds an
        // enabled rule, each 60s wake is a single cheap catalog read that early-returns (no capture, no
        // disk growth). Desktop-only — mobile has no scheduled-snapshot surface.
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        spawn_snapshot_schedule_timer(app.handle().clone());
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
            board_card_detail,
            board_directive,
            workbench_diff,
            home_dir,
            parent_dir,
            list_drives,
            list_network_shares,
            disconnect_network_share,
            disk_space,
            special_folders,
            create_dir,
            read_file_text,
            read_file_range,
            code_intel,
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
            thumbnails_stream,
            cancel_thumbnails_stream,
            read_settings,
            write_settings,
            load_tags,
            set_tags,
            tag_counts,
            rename_tag,
            delete_tag,
            import_tags,
            retag_path,
            template_capture,
            template_save,
            template_list,
            template_load,
            template_delete,
            template_stamp,
            template_export,
            template_import,
            macro_save,
            macro_list,
            macro_load,
            macro_delete,
            macro_export,
            macro_import,
            macro_plan,
            macro_run,
            macro_undo,
            native_tags_name,
            native_tags_pull,
            native_tags_push,
            rename_entry,
            delete_to_trash,
            delete_permanent,
            can_restore_from_trash,
            restore_from_trash,
            shred_paths,
            copy_entries,
            move_entries,
            run_watch_actions,
            start_transfer,
            cancel_transfer,
            move_exact,
            batch_media_plan,
            batch_media_execute_stream,
            entry_info,
            image_meta,
            metadata_read,
            metadata_writable,
            metadata_write,
            metadata_columns_available,
            metadata_column_cells,
            metadata_column_cells_collect,
            list_dir_stream,
            cancel_dir_stream,
            entries_for_paths,
            same_volume,
            find_files_by_name_stream,
            search_file_contents_stream,
            dir_size,
            dir_children_sizes,
            dir_children_sizes_stream,
            folder_stats,
            hash_file,
            apply_backup_plan,
            apply_backup_plan_stream,
            checksum_folder,
            verify_folder,
            verify_all_baselines,
            scan_tree,
            create_symlink,
            create_hard_link,
            create_junction,
            link_status,
            suggest_repair,
            install_shell_integration,
            uninstall_shell_integration,
            shell_integration_installed,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            register_spotlight_hotkey,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            unregister_spotlight_hotkey,
            drive_type,
            audit_record,
            audit_sessions,
            audit_read,
            metrics_record,
            metrics_history,
            replay_load,
            checkpoint_create,
            checkpoint_list,
            checkpoint_preview_revert,
            checkpoint_revert,
            checkpoint_revert_one,
            snapshot_prune_preview,
            snapshot_prune_apply,
            checkpoint_diff_file,
            snapshot_schedule_list,
            snapshot_schedule_set,
            snapshot_schedule_remove,
            snapshot_run_due,
            text_stats,
            inspect_file,
            spotlight_search,
            spotlight_frecent,
            search_file_contents,
            find_files_by_name,
            files_identical,
            find_duplicates,
            find_duplicates_stream,
            find_similar_images,
            find_similar_images_stream,
            find_similar_documents,
            find_similar_folders,
            organize_plan,
            organize_clutter,
            organize_apply,
            index_build,
            index_search,
            index_search_collect,
            index_status,
            index_drop,
            index_clear,
            git_remote_url,
            open_external,
            run_as_admin,
            extract_archive_entry,
            extract_archive_entry_any,
            compress_to_zip,
            extract_archive,
            compress_archive,
            compress_to_zip_encrypted,
            extract_zip_encrypted,
            start_archive_compress,
            start_archive_extract,
            open_terminal,
            run_command,
            create_file,
            create_file_with_content,
            create_empty_zip,
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
            agent_watch_stop_all,
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
        // …every rename shape maps to the coarse `renamed` kind; the from→to *pairing* (CPE-1117)
        // is the pump's job (see the rename-pairing tests below), not this coarse classifier's.
        assert_eq!(
            classify_fs_event(&EventKind::Modify(ModifyKind::Name(RenameMode::Both))),
            Some("renamed"),
        );
        assert_eq!(
            classify_fs_event(&EventKind::Modify(ModifyKind::Name(RenameMode::From))),
            Some("renamed"),
        );
        assert_eq!(
            classify_fs_event(&EventKind::Modify(ModifyKind::Name(RenameMode::To))),
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

    // --- Rename source→target pairing (CPE-1117) ----------------------------------------
    use super::{fold_orphan_renames, handle_rename_event};
    use std::collections::HashMap;
    use std::path::PathBuf;

    /// Build a `Modify(Name(mode))` event with the given paths and (optional) tracker cookie, the way
    /// notify's backends do: a `Both` carries `[from, to]`; a `From`/`To` split carries one path each
    /// plus a shared cookie.
    fn name_event(mode: RenameMode, paths: &[&str], cookie: Option<usize>) -> notify::Event {
        let mut ev = notify::Event::new(EventKind::Modify(ModifyKind::Name(mode)));
        for p in paths {
            ev = ev.add_path(PathBuf::from(p));
        }
        if let Some(c) = cookie {
            ev = ev.set_tracker(c);
        }
        ev
    }

    // The three per-window maps the pump threads through `handle_rename_event`.
    type PendingMap = HashMap<String, &'static str>;
    type RenameFromMap = HashMap<String, String>;
    type CookieMap = HashMap<usize, String>;
    fn maps() -> (PendingMap, RenameFromMap, CookieMap) {
        (HashMap::new(), HashMap::new(), HashMap::new())
    }

    #[test]
    fn rename_both_pairs_source_and_target_in_one_record() {
        // macOS / backends that emit a single `Both` with paths == [from, to].
        let (mut pending, mut rename_from, mut cookies) = maps();
        let ev = name_event(RenameMode::Both, &["/w/old.txt", "/w/new.txt"], None);
        handle_rename_event(&ev, &mut pending, &mut rename_from, &mut cookies);
        // One item, keyed by the *target*; the source rides in the side channel — never a second item.
        assert_eq!(pending.get("/w/new.txt"), Some(&"renamed"));
        assert_eq!(pending.get("/w/old.txt"), None);
        assert_eq!(rename_from.get("/w/new.txt").map(String::as_str), Some("/w/old.txt"));
        assert!(cookies.is_empty());
    }

    #[test]
    fn rename_from_then_to_pairs_by_cookie() {
        // Windows / Linux: a `From` then a `To` sharing one tracker cookie, within the flush window.
        let (mut pending, mut rename_from, mut cookies) = maps();
        handle_rename_event(
            &name_event(RenameMode::From, &["/w/old.txt"], Some(42)),
            &mut pending,
            &mut rename_from,
            &mut cookies,
        );
        // `From` alone emits nothing yet — the source is held by cookie awaiting its `To`.
        assert!(pending.is_empty());
        assert_eq!(cookies.get(&42).map(String::as_str), Some("/w/old.txt"));

        handle_rename_event(
            &name_event(RenameMode::To, &["/w/new.txt"], Some(42)),
            &mut pending,
            &mut rename_from,
            &mut cookies,
        );
        assert_eq!(pending.get("/w/new.txt"), Some(&"renamed"));
        assert_eq!(rename_from.get("/w/new.txt").map(String::as_str), Some("/w/old.txt"));
        assert!(cookies.is_empty(), "cookie consumed once paired");
    }

    #[test]
    fn orphan_from_degrades_to_single_path_on_flush() {
        // A `From` whose matching `To` never arrives (moved out of the tree). At window flush it must
        // fall back to a single-path `renamed` — no crash, no fabricated pair.
        let (mut pending, mut rename_from, mut cookies) = maps();
        handle_rename_event(
            &name_event(RenameMode::From, &["/w/gone.txt"], Some(7)),
            &mut pending,
            &mut rename_from,
            &mut cookies,
        );
        fold_orphan_renames(&mut pending, &mut cookies);
        assert_eq!(pending.get("/w/gone.txt"), Some(&"renamed"));
        assert!(rename_from.is_empty(), "no false from→to pair for an orphan");
        assert!(cookies.is_empty());
    }

    #[test]
    fn rename_to_without_a_seen_from_is_single_path() {
        // A `To` (with or without a cookie) whose `From` we never captured degrades to single-path.
        let (mut pending, mut rename_from, mut cookies) = maps();
        handle_rename_event(
            &name_event(RenameMode::To, &["/w/x.txt"], Some(99)),
            &mut pending,
            &mut rename_from,
            &mut cookies,
        );
        assert_eq!(pending.get("/w/x.txt"), Some(&"renamed"));
        assert!(rename_from.is_empty());
    }

    #[test]
    fn rename_any_and_cookieless_split_degrade_gracefully() {
        // `Any`/`Other` (some Linux inotify backends) and a cookieless `From` both fall back to
        // single-path `renamed` with no pairing — the documented fidelity ceiling.
        let (mut pending, mut rename_from, mut cookies) = maps();
        handle_rename_event(
            &name_event(RenameMode::Any, &["/w/a.txt"], None),
            &mut pending,
            &mut rename_from,
            &mut cookies,
        );
        handle_rename_event(
            &name_event(RenameMode::From, &["/w/b.txt"], None),
            &mut pending,
            &mut rename_from,
            &mut cookies,
        );
        assert_eq!(pending.get("/w/a.txt"), Some(&"renamed"));
        assert_eq!(pending.get("/w/b.txt"), Some(&"renamed"));
        assert!(rename_from.is_empty());
        assert!(cookies.is_empty());
    }

    // --- Multi-session watch: keyed map + zero-watcher invariant (CPE-1099) -------------
    use super::{AgentWatch, AgentWatchState};

    /// Build a real `notify` watcher over a fresh temp subdir so an `AgentWatch` can be inserted in a
    /// test without any Tauri machinery. The receiver is dropped immediately: we only assert on the
    /// map's lifecycle (arm/disarm/clear), not on emitted events.
    fn make_watch(tag: &str) -> AgentWatch {
        use notify::{RecursiveMode, Watcher};
        let dir = std::env::temp_dir().join(format!("cpe1099-{}-{}", std::process::id(), tag));
        std::fs::create_dir_all(&dir).unwrap();
        let (tx, _rx) = std::sync::mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |res| {
            let _ = tx.send(res);
        })
        .unwrap();
        watcher.watch(&dir, RecursiveMode::Recursive).unwrap();
        AgentWatch { _watcher: watcher, path: dir.to_string_lossy().into_owned() }
    }

    #[test]
    fn arming_two_sessions_then_stop_all_leaves_zero_watchers() {
        let state = AgentWatchState::default();
        // Off means off: nothing armed ⇒ empty map ⇒ zero watcher/pump threads.
        assert!(state.is_empty());

        // Arm two distinct sessions — both are watched concurrently (keyed by sessionId).
        state.arm("s1".into(), make_watch("s1"));
        state.arm("s2".into(), make_watch("s2"));
        assert_eq!(state.armed_count(), 2);

        // Re-arming an existing key replaces (drops) its prior watch, not add a second — still 2.
        state.arm("s1".into(), make_watch("s1b"));
        assert_eq!(state.armed_count(), 2);

        // Disarming one session removes only that key; the other keeps watching.
        state.disarm("s1");
        assert_eq!(state.armed_count(), 1);

        // stop_all clears the map → back to the zero-watcher invariant.
        state.disarm_all();
        assert!(state.is_empty());
    }

    // --- Instant index: live watcher map lifecycle (CPE-1138) -----------------------------
    use super::{IndexWatch, IndexWatchState};

    /// Build a real `notify` watcher over a fresh temp subdir so an `IndexWatch` can be inserted in a
    /// test without any Tauri machinery (mirrors `make_watch` for `AgentWatch` above). The receiver is
    /// dropped immediately: this only asserts on the map's lifecycle (arm/stop/stop_all), not on
    /// emitted events — event→mutation behaviour is covered by `cpe_server::index_watch`'s own tests.
    fn make_index_watch(tag: &str) -> IndexWatch {
        use notify::{RecursiveMode, Watcher};
        let dir = std::env::temp_dir().join(format!("cpe1138-{}-{}", std::process::id(), tag));
        std::fs::create_dir_all(&dir).unwrap();
        let (tx, _rx) = std::sync::mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |res| {
            let _ = tx.send(res);
        })
        .unwrap();
        watcher.watch(&dir, RecursiveMode::Recursive).unwrap();
        IndexWatch { _watcher: watcher, root: dir.to_string_lossy().into_owned() }
    }

    #[test]
    fn arming_two_volumes_then_stop_all_leaves_zero_watchers() {
        let state = IndexWatchState::default();
        // Off means off: nothing armed ⇒ empty map ⇒ zero watcher/pump threads.
        assert_eq!(state.armed_count(), 0);

        // Arm two distinct volumes — both watched concurrently (keyed by volume_id).
        state.arm(1, make_index_watch("v1"));
        state.arm(2, make_index_watch("v2"));
        assert_eq!(state.armed_count(), 2);

        // Re-arming an already-watched volume (a rebuild) replaces its watch, not adds a second.
        state.arm(1, make_index_watch("v1b"));
        assert_eq!(state.armed_count(), 2);

        // Stopping one volume (`index_drop`) removes only that key; the other keeps watching.
        state.stop(1);
        assert_eq!(state.armed_count(), 1);

        // A repeat stop is a no-op (idempotent).
        state.stop(1);
        assert_eq!(state.armed_count(), 1);

        // stop_all (`index_clear`) empties the map → back to the zero-watcher invariant.
        state.stop_all();
        assert_eq!(state.armed_count(), 0);
    }

    // --- Per-event actor tags: app-op ledger + actor resolution (CPE-1101) ----------------
    use super::{normalize_op_path, resolve_actor, APP_OP_TTL};
    use std::collections::VecDeque;
    use std::time::{Duration, Instant};

    #[test]
    fn actor_is_user_after_a_matching_app_op_then_session_id_otherwise() {
        let now = Instant::now();
        // The explorer just wrote this path (as `note_app_op` would, normalized). A separator/case
        // variant of the *same* path must still match — the watcher emits its own spelling.
        let mut ledger: VecDeque<(String, Instant)> = VecDeque::new();
        ledger.push_back((normalize_op_path("Z:/proj/a.txt"), now));

        // The watcher event for that path resolves to "user" AND consumes the ledger entry, so a
        // second same-path event can't keep claiming "user".
        let a = resolve_actor(&mut ledger, true, "sess-1", r"Z:\proj\A.TXT", now);
        assert_eq!(a, "user");
        assert!(ledger.is_empty(), "a user match must consume its ledger entry");

        // A plain event with no ledger match falls back to the owning session id.
        let b = resolve_actor(&mut ledger, true, "sess-1", "Z:/proj/b.txt", now);
        assert_eq!(b, "sess-1");

        // A stale ledger entry (older than the TTL) does NOT match — it can't mis-attribute a much
        // later unrelated write to the user.
        let mut stale: VecDeque<(String, Instant)> = VecDeque::new();
        stale.push_back((normalize_op_path("Z:/proj/c.txt"), now - APP_OP_TTL - Duration::from_millis(1)));
        let c = resolve_actor(&mut stale, true, "sess-1", "Z:/proj/c.txt", now);
        assert_eq!(c, "sess-1");
        assert_eq!(stale.len(), 1, "a non-matching (stale) entry is left in place, not consumed");

        // An event whose owning session's watch is already gone (drain race) is "unknown", not a dead id.
        let mut empty: VecDeque<(String, Instant)> = VecDeque::new();
        let d = resolve_actor(&mut empty, false, "sess-gone", "Z:/proj/d.txt", now);
        assert_eq!(d, "unknown");
    }

    /// CPE-1102 (fast-follow on CPE-1101): before this ticket, `delete_permanent` recorded nothing, so a
    /// Shift+Del permanent-delete's watcher `removed` event fell back to the owning session id — reading
    /// as the agent even though the explorer itself performed it — while its sibling `delete_to_trash`
    /// (which already calls `note_app_op(&app, || paths.clone())`) correctly read "user". This test
    /// exercises the exact ledger primitives `delete_permanent`'s `note_app_op` call now feeds (same
    /// `Vec<String>` shape as `delete_to_trash`), proving a permanently-deleted path resolves to "user"
    /// just like a trashed one does.
    #[test]
    fn delete_permanent_paths_resolve_to_user_via_the_app_op_ledger() {
        let now = Instant::now();
        let paths = vec!["Z:/proj/gone.txt".to_string(), r"Z:\proj\also-gone.txt".to_string()];

        // Mirror `note_app_op`'s body: normalize + record each path just before the mutation.
        let mut ledger: VecDeque<(String, Instant)> = VecDeque::new();
        for p in &paths {
            ledger.push_back((normalize_op_path(p), now));
        }

        // The `notify` watcher's "removed" event for each permanently-deleted path — in its own
        // spelling — now resolves to "user", not the owning session id.
        let a = resolve_actor(&mut ledger, true, "sess-1", "Z:/proj/gone.txt", now);
        assert_eq!(a, "user");
        let b = resolve_actor(&mut ledger, true, "sess-1", r"Z:\proj\ALSO-GONE.TXT", now);
        assert_eq!(b, "user");
        assert!(ledger.is_empty(), "both matches must consume their ledger entries");

        // An unrelated path deleted in the same batch but never recorded still falls back to the
        // session id — the ledger doesn't blanket-attribute every delete to "user".
        let c = resolve_actor(&mut ledger, true, "sess-1", "Z:/proj/unrelated.txt", now);
        assert_eq!(c, "sess-1");
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

/// CPE-812 (epic CPE-810): generate the typed TypeScript command client for the `#[specta::specta]`-
/// annotated commands into `out`, routed through the busy-cursor `invoke` wrapper (`src/lib/invoke.ts`) so
/// every typed call flips the app-wide wait cursor for free (CPE-547). Runtime dispatch still uses
/// `generate_handler!` (this only emits the client), so behaviour is unchanged. Called from the
/// `export_bindings` bin — a plain exe, which loads where a libtest binary linking tauri-specta would fail
/// (`STATUS_ENTRYPOINT_NOT_FOUND`, a WebView2 entrypoint skew), so codegen never runs under `cargo test`.
/// Requires **both** `specta-bindings` and `sidecar-platform` so `bindings.gen.ts` is the one superset
/// contract covering the sidecar commands too (CPE-957). Regenerate:
///   cargo run --bin export_bindings --features "specta-bindings sidecar-platform"
#[cfg(all(feature = "specta-bindings", feature = "sidecar-platform"))]
pub fn export_bindings(out: &std::path::Path) -> Result<(), String> {
    use tauri_specta::{collect_commands, Builder};
    // The full SUPERSET typed surface (CPE-953 + CPE-957): every `#[tauri::command]` — the non-sidecar
    // ones (incl. the `ipc::Channel` streamers, whose `Channel<T>` methods bind `TAURI_CHANNEL` from
    // `./invoke`, which re-exports `Channel`) AND the `sidecar-platform` commands (this bin requires both
    // features). Their types derive `specta::Type` across `cpe-server` + the app modules + the sidecar
    // crates (`sidecar-contract`/`repos`, behind their own OFF-by-default `specta` features).
    // Deliberately EXCLUDED: the single-OS commands `set_permissions` (`#[cfg(unix)]`) and
    // `set_file_attribute` (`#[cfg(windows)]`) — they exist on only one platform, so including them would
    // make `bindings.gen.ts` OS-dependent and break the drift guard (CPE-813), which regenerates on Linux.
    // Callers use raw `invoke` for those two.
    let builder = Builder::<tauri::Wry>::new().commands(collect_commands![
        list_dir,
        find_project_root,
        board_cards,
        board_epics,
        board_archived,
        board_move,
        board_review,
        board_note,
        board_card_detail,
        board_directive,
        workbench_diff,
        home_dir,
        parent_dir,
        list_drives,
        list_network_shares,
        disconnect_network_share,
        disk_space,
        special_folders,
        create_dir,
        read_file_text,
        read_file_range,
        code_intel,
        file_len,
        set_readonly,
        set_file_times,
        read_attributes,
        write_file_text,
        read_archive_entries,
        read_preview_info,
        data_browser_sources,
        data_browser_page,
        data_browser_query,
        read_image_data_url,
        thumbnail,
        thumbnails_stream,
        cancel_thumbnails_stream,
        read_settings,
        write_settings,
        load_tags,
        set_tags,
        tag_counts,
        rename_tag,
        delete_tag,
        import_tags,
        retag_path,
        template_capture,
        template_save,
        template_list,
        template_load,
        template_delete,
        template_stamp,
        template_export,
        template_import,
        macro_save,
        macro_list,
        macro_load,
        macro_delete,
        macro_export,
        macro_import,
        macro_plan,
        macro_run,
        macro_undo,
        native_tags_name,
        native_tags_pull,
        native_tags_push,
        rename_entry,
        delete_to_trash,
        delete_permanent,
        can_restore_from_trash,
        restore_from_trash,
        shred_paths,
        copy_entries,
        move_entries,
        run_watch_actions,
        start_transfer,
        cancel_transfer,
        move_exact,
        batch_media_plan,
        batch_media_execute_stream,
        entry_info,
        image_meta,
        metadata_read,
        metadata_writable,
        metadata_write,
        metadata_columns_available,
        metadata_column_cells,
        metadata_column_cells_collect,
        list_dir_stream,
        cancel_dir_stream,
        entries_for_paths,
        same_volume,
        find_files_by_name_stream,
        search_file_contents_stream,
        dir_size,
        dir_children_sizes,
        dir_children_sizes_stream,
        folder_stats,
        hash_file,
        apply_backup_plan,
        apply_backup_plan_stream,
        checksum_folder,
        verify_folder,
        verify_all_baselines,
        scan_tree,
        create_symlink,
        create_hard_link,
        create_junction,
        link_status,
        suggest_repair,
        install_shell_integration,
        uninstall_shell_integration,
        shell_integration_installed,
        register_spotlight_hotkey,
        unregister_spotlight_hotkey,
        drive_type,
        audit_record,
        audit_sessions,
        audit_read,
        metrics_record,
        metrics_history,
        replay_load,
        checkpoint_create,
        checkpoint_list,
        checkpoint_preview_revert,
        checkpoint_revert,
        checkpoint_revert_one,
        snapshot_prune_preview,
        snapshot_prune_apply,
        checkpoint_diff_file,
        snapshot_schedule_list,
        snapshot_schedule_set,
        snapshot_schedule_remove,
        snapshot_run_due,
        text_stats,
        inspect_file,
        spotlight_search,
        spotlight_frecent,
        search_file_contents,
        find_files_by_name,
        files_identical,
        find_duplicates,
        find_duplicates_stream,
        find_similar_images,
        find_similar_images_stream,
        find_similar_documents,
        find_similar_folders,
        organize_plan,
        organize_clutter,
        organize_apply,
        index_build,
        index_search,
        index_search_collect,
        index_status,
        index_drop,
        index_clear,
        git_remote_url,
        open_external,
        run_as_admin,
        extract_archive_entry,
        extract_archive_entry_any,
        compress_to_zip,
        extract_archive,
        compress_archive,
        compress_to_zip_encrypted,
        extract_zip_encrypted,
        start_archive_compress,
        start_archive_extract,
        open_terminal,
        run_command,
        create_file,
        create_file_with_content,
        create_empty_zip,
        sidecar_registry_ids,
        sidecar_consent_state,
        sidecar_set_consent,
        sidecar_revoke_capability,
        sidecar_details,
        sidecar_stop,
        sidecar_repair,
        sidecar_close_session,
        sidecar_set_enabled,
        sidecar_start_ai_console,
        sidecar_start_agent_board,
        sidecar_diagnostics,
        agent_watch_start,
        agent_watch_stop,
        agent_watch_stop_all,
        folder_watch_start,
        folder_watch_stop,
        forge_browse,
        forge_clone,
        forge_generic_remote,
        forge_admitted_hosts,
        forge_admit_host,
        forge_forget_host,
        forge_clone_url,
        forge_set_token,
        forge_get_token,
        forge_delete_token,
        forge_repo_status,
        forge_sync,
        forge_conflict_state,
        forge_conflict_versions,
        forge_resolve_file,
        forge_conflict_continue,
        forge_conflict_abort,
    ])
    // CPE-1110: export the replay projection types that no command *returns* but the frontend fold
    // (`src/lib/replayFold.ts`) reconstructs client-side — `ReplayEntry` (a `children_at` row) and
    // `FsNode` (a folded path's last-touch state) — so the TS fold shares one contract with Rust.
    .typ::<cpe_server::replay_view::ReplayEntry>()
    .typ::<cpe_server::replay::FsNode>();
    let tmp = std::env::temp_dir().join("cpe_bindings_export.ts");
    // u64 byte-counts (e.g. write_file_text) map to `number` — matches how the frontend already treats
    // these returns and how serde_json emits them; specta forbids BigInt types without an explicit policy.
    let ts = specta_typescript::Typescript::default()
        .bigint(specta_typescript::BigIntExportBehavior::Number);
    builder
        .export(ts, &tmp)
        .map_err(|e| format!("specta export: {e}"))?;
    let raw = std::fs::read_to_string(&tmp).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&tmp);
    // Repoint the generated `invoke` at our busy-cursor wrapper (same signature) instead of core, so every
    // typed call raises the app-wide wait cursor for free (CPE-547). `@ts-nocheck` because this is a
    // machine-generated file with tauri-specta's event scaffolding unused in a commands-only export.
    let generated = format!(
        "// @ts-nocheck\n/* eslint-disable */\n{}",
        raw.replace("@tauri-apps/api/core\"", "./invoke\"")
    );
    std::fs::write(out, generated).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CPE-812 (epic CPE-810): guard the committed typed client `src/lib/bindings.gen.ts`. This is a
    /// **pure-fs** test — it must NOT reference `tauri_specta`/`tauri`'s runtime, or linking those into the
    /// libtest binary makes it fail to LOAD on Windows (`STATUS_ENTRYPOINT_NOT_FOUND`, a WebView2 entrypoint
    /// skew). Regeneration therefore lives in the separate `export_bindings` bin (`cargo run --bin
    /// export_bindings`), which loads fine because it's a plain exe like the app. Here we just assert the
    /// committed output exists, routes through the busy-cursor wrapper, and covers the annotated commands.
    #[test]
    fn typed_bindings_are_committed_and_routed_through_busy_cursor() {
        let src = fs::read_to_string("../src/lib/bindings.gen.ts")
            .expect("src/lib/bindings.gen.ts is missing — run `cargo run --bin export_bindings`");
        assert!(
            src.contains("./invoke"),
            "generated client must import the busy-cursor invoke wrapper, not @tauri-apps/api/core"
        );
        assert!(!src.contains("@tauri-apps/api/core"), "core invoke import must be rewritten to ./invoke");
        for cmd in [
            "runCommand", "createDir", "createFile", "writeFileText", "renameEntry", "canRestoreFromTrash",
            "listDir", "entryInfo", "deleteToTrash", "deletePermanent", "copyEntries", "moveEntries", "moveExact",
            "spotlightSearch", "spotlightFrecent",
        ] {
            assert!(src.contains(cmd), "typed client should expose `{cmd}`");
        }
        // The cross-crate cpe-server types now flow into the generated client.
        assert!(src.contains("DirEntry") && src.contains("OpResult") && src.contains("EntryInfo"),
            "cpe-server types should be exported into the typed client");
    }

    // ---- Background scheduled-snapshot timer (CPE-1199, epic CPE-735) -------------------------------
    // These exercise `snapshot_schedule_tick` — the per-wake body of the timer — over a pure
    // `HeadlessCtx` + injected clock (no tauri runtime, so this stays a pure-fs test per the note above).

    use cpe_server::ctx::HeadlessCtx;
    use cpe_server::{checkpoint_store, snapshot_schedule};
    use std::collections::BTreeMap;

    fn sched_scratch(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-sched-tick-{}-{}-{}", tag, std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
    }

    /// Off means off: with no rules at all a tick captures nothing, writes nothing to disk, and leaves
    /// the last-run bookkeeping empty — the zero-idle-cost guarantee, asserted by construction.
    #[test]
    fn snapshot_schedule_tick_is_zero_cost_with_no_rules() {
        let app = sched_scratch("noop-app");
        let ctx = HeadlessCtx::new(&app);
        let probe = sched_scratch("noop-probe");
        let probe_s = probe.to_string_lossy().to_string();

        let mut last_run = BTreeMap::new();
        let captured = snapshot_schedule_tick(&ctx, 10_000, &mut last_run);

        assert!(captured.is_empty(), "no rules ⇒ nothing captured");
        assert!(last_run.is_empty(), "no rules ⇒ bookkeeping untouched");
        // No checkpoint store was created for any folder — the tick never touched the filesystem.
        assert!(checkpoint_store::checkpoint_list(&ctx, &probe_s).unwrap().is_empty());

        let _ = fs::remove_dir_all(&app);
        let _ = fs::remove_dir_all(&probe);
    }

    /// An all-disabled catalog is also a no-op (off means off applies to a disabled rule, not just an
    /// empty catalog) — no capture despite a stored rule.
    #[test]
    fn snapshot_schedule_tick_does_nothing_when_every_rule_is_disabled() {
        let app = sched_scratch("disabled-app");
        let ctx = HeadlessCtx::new(&app);
        let root = sched_scratch("disabled-root");
        let root_s = root.to_string_lossy().to_string();
        fs::write(root.join("f.txt"), b"x").unwrap();
        snapshot_schedule::set_rule(&ctx, snapshot_schedule::ScheduleRule {
            root: root_s.clone(), interval_s: 1, retention: Default::default(), enabled: false,
        }).unwrap();

        let mut last_run = BTreeMap::new();
        assert!(snapshot_schedule_tick(&ctx, 10_000, &mut last_run).is_empty());
        assert!(last_run.is_empty());
        assert!(checkpoint_store::checkpoint_list(&ctx, &root_s).unwrap().is_empty(), "disabled ⇒ no capture");

        let _ = fs::remove_dir_all(&app);
        let _ = fs::remove_dir_all(&root);
    }

    /// The enable→due→capture path: an enabled, never-run rule is due immediately, so the first tick
    /// captures it and records its run time; a second tick within the interval then captures nothing
    /// (the recorded last-run holds it off) — proving the timer doesn't re-capture every wake.
    #[test]
    fn snapshot_schedule_tick_captures_a_due_enabled_root_then_holds_off_within_interval() {
        let app = sched_scratch("capture-app");
        let ctx = HeadlessCtx::new(&app);
        let root = sched_scratch("capture-root");
        let root_s = root.to_string_lossy().to_string();
        fs::write(root.join("f.txt"), b"content").unwrap();
        snapshot_schedule::set_rule(&ctx, snapshot_schedule::ScheduleRule {
            root: root_s.clone(), interval_s: 3_600, retention: Default::default(), enabled: true,
        }).unwrap();

        let mut last_run = BTreeMap::new();
        // Tick 1: never-run ⇒ due immediately ⇒ captured, and its run time is recorded at `now`.
        let captured = snapshot_schedule_tick(&ctx, 1_000, &mut last_run);
        assert_eq!(captured, vec![root_s.clone()]);
        assert_eq!(last_run.get(&root_s), Some(&1_000));
        assert_eq!(checkpoint_store::checkpoint_list(&ctx, &root_s).unwrap().len(), 1, "one scheduled capture landed");

        // Tick 2: still inside the 3600s interval ⇒ not due ⇒ no second capture.
        assert!(snapshot_schedule_tick(&ctx, 2_000, &mut last_run).is_empty(), "within interval ⇒ no re-capture");
        assert_eq!(checkpoint_store::checkpoint_list(&ctx, &root_s).unwrap().len(), 1, "still just the one capture");

        // Tick 3: past the interval ⇒ due again ⇒ a second capture, run time bumped.
        let captured3 = snapshot_schedule_tick(&ctx, 5_000, &mut last_run);
        assert_eq!(captured3, vec![root_s.clone()]);
        assert_eq!(last_run.get(&root_s), Some(&5_000));
        assert_eq!(checkpoint_store::checkpoint_list(&ctx, &root_s).unwrap().len(), 2, "interval elapsed ⇒ second capture");

        let _ = fs::remove_dir_all(&app);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn find_ticket_file_locates_epics_sprints_and_archived() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        // The Ticketing/ container (CPE-1128): status folders under Tickets/, Epics/ & Sprints/ as siblings.
        let t = root.join("Ticketing");
        for (dir, name) in [
            ("Tickets/Backlog", "CPE-10_a.md"),
            ("Epics", "CPE-616_epic-remote.md"),
            ("Sprints", "SPR-01_x.md"),
            ("Tickets/Done/2026/Q3/July/Week-30", "CPE-1_done.md"),
        ] {
            let d = t.join(dir);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join(name), "---\nid: x\n---\n").unwrap();
        }
        let rs = root.to_string_lossy();
        assert!(find_ticket_file(&rs, "CPE-616").unwrap().ends_with("CPE-616_epic-remote.md"), "epic found");
        assert!(find_ticket_file(&rs, "CPE-1").unwrap().ends_with("CPE-1_done.md"), "archived Done ticket found");
        assert!(find_ticket_file(&rs, "SPR-01").unwrap().ends_with("SPR-01_x.md"), "sprint found");
        assert!(find_ticket_file(&rs, "CPE-999").is_none(), "missing id → None");
        // Prefix precision: `CPE-6` must not match `CPE-616`.
        assert!(find_ticket_file(&rs, "CPE-6").is_none(), "CPE-6 does not match CPE-616");
    }

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

    // ---- Action macros: save→list→load round trip + run/undo (CPE-1188, epic CPE-739) ---------
    // These test the same underlying calls the `macro_*` `#[tauri::command]` dispatchers make
    // (`cpe_server::macro_store`/`macro_run` + the `macro_apply_*` bridge functions), against a
    // `HeadlessCtx` — the command fns themselves need a live `tauri::AppHandle`, which (per the
    // `typed_bindings_are_committed...` test's comment above) can't be constructed in a plain
    // libtest binary on Windows.

    #[test]
    fn macro_save_list_load_round_trip() {
        let base = tempfile::tempdir().unwrap();
        let ctx = cpe_server::ctx::HeadlessCtx::new(base.path());
        let mac = cpe_server::action_macro::ActionMacro {
            name: "tidy".into(),
            steps: vec![cpe_server::action_macro::MacroStep::Tag { label: "done".into() }],
        };

        cpe_server::macro_store::save(&ctx, mac.clone()).unwrap();
        let names: Vec<String> = cpe_server::macro_store::list(&ctx)
            .unwrap()
            .into_iter()
            .map(|s| s.name)
            .collect();
        assert_eq!(names, vec!["tidy".to_string()]);
        assert_eq!(cpe_server::macro_store::load(&ctx, "tidy").unwrap(), Some(mac));
        assert_eq!(cpe_server::macro_store::load(&ctx, "nope").unwrap(), None);
    }

    #[test]
    fn macro_run_tag_step_then_undo_restores_the_tag_store() {
        let config = tempfile::tempdir().unwrap();
        let ctx = cpe_server::ctx::HeadlessCtx::new(config.path());
        let work = tempfile::tempdir().unwrap();
        let file = work.path().join("a.txt");
        fs::write(&file, b"hi").unwrap();
        let input = file.to_string_lossy().to_string();
        let root = work.path().to_string_lossy().to_string();

        let mac = cpe_server::action_macro::ActionMacro {
            name: "tag-done".into(),
            steps: vec![cpe_server::action_macro::MacroStep::Tag { label: "done".into() }],
        };
        let resolved = cpe_server::macro_run::resolve(&mac, std::slice::from_ref(&input), &root).unwrap();
        let applied = macro_apply_run(&ctx, resolved).unwrap();

        let store = cpe_server::tags::load(&ctx).unwrap();
        assert!(store.get(&input).unwrap().tags().contains(&"done".to_string()));

        macro_apply_inverses(&ctx, &applied).unwrap();
        let store = cpe_server::tags::load(&ctx).unwrap();
        assert!(!store.contains_key(&input), "tag entry should be pruned back to nothing after undo");
    }

    #[test]
    fn macro_run_rename_and_move_with_undo_restores_the_original_path_and_bytes() {
        let config = tempfile::tempdir().unwrap();
        let ctx = cpe_server::ctx::HeadlessCtx::new(config.path());
        let work = tempfile::tempdir().unwrap();
        let sub = work.path().join("sub");
        fs::create_dir_all(&sub).unwrap();
        let file = sub.join("a.txt");
        fs::write(&file, b"hello").unwrap();
        let dest = work.path().join("Archive");
        fs::create_dir_all(&dest).unwrap();

        let mac = cpe_server::action_macro::ActionMacro {
            name: "archive".into(),
            steps: vec![
                cpe_server::action_macro::MacroStep::Rename { template: "b.txt".into() },
                cpe_server::action_macro::MacroStep::Move { dest: dest.to_string_lossy().to_string() },
            ],
        };
        let root = work.path().to_string_lossy().to_string();
        let input = file.to_string_lossy().to_string();
        let resolved = cpe_server::macro_run::resolve(&mac, &[input], &root).unwrap();
        let applied = macro_apply_run(&ctx, resolved).unwrap();

        let moved = dest.join("b.txt");
        assert!(moved.exists(), "file should have been renamed then moved");
        assert!(!file.exists());

        macro_apply_inverses(&ctx, &applied).unwrap();
        assert!(file.exists(), "undo should restore the original path");
        assert!(!moved.exists());
        assert_eq!(fs::read_to_string(&file).unwrap(), "hello");
    }

    #[test]
    fn macro_run_rolls_back_the_first_step_when_a_later_one_fails() {
        let config = tempfile::tempdir().unwrap();
        let ctx = cpe_server::ctx::HeadlessCtx::new(config.path());
        let work = tempfile::tempdir().unwrap();
        let a = work.path().join("a.txt");
        fs::write(&a, b"hi").unwrap();
        // Never created on disk — its rename primitive will fail when actually applied, even
        // though pure resolution (which never touches disk) planned it a valid collision-free
        // target right alongside `a.txt`'s.
        let missing = work.path().join("missing.txt");

        let mac = cpe_server::action_macro::ActionMacro {
            name: "r".into(),
            steps: vec![cpe_server::action_macro::MacroStep::Rename { template: "renamed.txt".into() }],
        };
        let root = work.path().to_string_lossy().to_string();
        let inputs = vec![a.to_string_lossy().to_string(), missing.to_string_lossy().to_string()];
        let resolved = cpe_server::macro_run::resolve(&mac, &inputs, &root).unwrap();

        let err = macro_apply_run(&ctx, resolved).unwrap_err();
        assert!(err.contains("rolled back"), "got: {err}");

        // The first (successful) step's effect must have been rolled back.
        assert!(a.exists(), "rollback should restore the first step's original file");
        assert!(!work.path().join("renamed.txt").exists());
    }

    // ---- Undo fidelity (CPE-1194) ---------------------------------------------------------------

    #[test]
    fn macro_run_tag_step_preserves_a_pre_existing_tag_after_undo() {
        // CPE-1194: the bug is specifically when the macro's OWN label was already on the path
        // before the run. `untag` on undo used to strip it unconditionally (a structural
        // add/remove of exactly that label), so a label the user had explicitly set before the run
        // was wrongly stripped by undo even though the forward step was a no-op (union add).
        // `macro_apply_run` must snapshot pre-run tag state and correct that op's inverse to a
        // no-op restore when the label pre-existed, leaving a DIFFERENT, untouched tag alone either way.
        let config = tempfile::tempdir().unwrap();
        let ctx = cpe_server::ctx::HeadlessCtx::new(config.path());
        let work = tempfile::tempdir().unwrap();
        let file = work.path().join("a.txt");
        fs::write(&file, b"hi").unwrap();
        let input = file.to_string_lossy().to_string();
        let root = work.path().to_string_lossy().to_string();

        // The user already tagged this file "done" (the SAME label the macro attaches) AND "keep"
        // (an unrelated label) before the macro ever runs.
        cpe_server::tags::set(&ctx, &input, vec!["done".to_string(), "keep".to_string()], String::new())
            .unwrap();

        let mac = cpe_server::action_macro::ActionMacro {
            name: "tag-done".into(),
            steps: vec![cpe_server::action_macro::MacroStep::Tag { label: "done".into() }],
        };
        let resolved = cpe_server::macro_run::resolve(&mac, std::slice::from_ref(&input), &root).unwrap();
        let applied = macro_apply_run(&ctx, resolved).unwrap();
        // The apply layer must have corrected the tag step's inverse to a no-op restore.
        assert_eq!(applied.inverses[0].kind, "tag", "pre-existing label ⇒ inverse must not be untag");

        let store = cpe_server::tags::load(&ctx).unwrap();
        let tags = store.get(&input).unwrap().tags();
        assert!(tags.contains(&"keep".to_string()) && tags.contains(&"done".to_string()), "got: {tags:?}");

        macro_apply_inverses(&ctx, &applied).unwrap();

        let store = cpe_server::tags::load(&ctx).unwrap();
        let tags = store.get(&input).map(|e| e.tags().to_vec()).unwrap_or_default();
        assert!(tags.contains(&"keep".to_string()), "unrelated tag must survive undo: {tags:?}");
        assert!(
            tags.contains(&"done".to_string()),
            "the pre-existing 'done' label must survive undo (this is the CPE-1194 fix): {tags:?}"
        );
    }

    #[test]
    fn macro_run_tag_step_with_no_pre_existing_tag_still_untags_on_undo() {
        // The un-corrected case (CPE-1187 behavior) must still work: when the label was NOT already
        // present, undo removes exactly what the run added.
        let config = tempfile::tempdir().unwrap();
        let ctx = cpe_server::ctx::HeadlessCtx::new(config.path());
        let work = tempfile::tempdir().unwrap();
        let file = work.path().join("a.txt");
        fs::write(&file, b"hi").unwrap();
        let input = file.to_string_lossy().to_string();
        let root = work.path().to_string_lossy().to_string();

        let mac = cpe_server::action_macro::ActionMacro {
            name: "tag-done".into(),
            steps: vec![cpe_server::action_macro::MacroStep::Tag { label: "done".into() }],
        };
        let resolved = cpe_server::macro_run::resolve(&mac, std::slice::from_ref(&input), &root).unwrap();
        let applied = macro_apply_run(&ctx, resolved).unwrap();
        assert_eq!(applied.inverses[0].kind, "untag", "no pre-existing label ⇒ inverse stays untag");

        macro_apply_inverses(&ctx, &applied).unwrap();
        let store = cpe_server::tags::load(&ctx).unwrap();
        assert!(!store.contains_key(&input), "tag entry should be pruned back to nothing after undo");
    }

    /// A minimal valid PNG, built with the `image` crate (already a real dependency, used elsewhere
    /// in this test module for fixtures) rather than hand-rolled bytes.
    fn cpe_1194_test_png_bytes() -> Vec<u8> {
        let img = image::RgbImage::from_pixel(4, 4, image::Rgb([200u8, 40, 60]));
        let mut buf = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut buf, image::ImageFormat::Png)
            .unwrap();
        buf.into_inner()
    }

    // Trash restore (`trash::os_limited::{list, restore_all}`) is only implemented on Windows and
    // Linux (see `restore_from_trash_impl` / CPE-044) — macOS has no programmatic trash listing API,
    // so this scenario cannot be exercised there. Mirrors the existing platform split.
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    #[test]
    fn macro_run_convert_step_then_undo_restores_the_original_bytes_via_trash() {
        // CPE-1194: undo of a `convert` step used to re-encode back to the source extension (lossy).
        // The forward step now trashes the original instead of deleting it, so undo can restore the
        // exact original bytes from the OS trash — proven here by a byte-for-byte comparison, not
        // just "a file with the old extension exists again".
        let config = tempfile::tempdir().unwrap();
        let ctx = cpe_server::ctx::HeadlessCtx::new(config.path());
        let work = tempfile::tempdir().unwrap();
        let original = work.path().join("photo.png");
        let original_bytes = cpe_1194_test_png_bytes();
        fs::write(&original, &original_bytes).unwrap();
        let input = original.to_string_lossy().to_string();
        let root = work.path().to_string_lossy().to_string();

        let mac = cpe_server::action_macro::ActionMacro {
            name: "to-jpg".into(),
            steps: vec![cpe_server::action_macro::MacroStep::Convert { to_ext: "jpg".into() }],
        };
        let resolved = cpe_server::macro_run::resolve(&mac, std::slice::from_ref(&input), &root).unwrap();
        assert_eq!(resolved.inverses[0].kind, "restore_convert");
        let applied = macro_apply_run(&ctx, resolved).unwrap();

        let converted = work.path().join("photo.jpg");
        assert!(converted.exists(), "converted file should exist");
        assert!(!original.exists(), "original should be gone from its path (trashed, not left behind)");

        macro_apply_inverses(&ctx, &applied).unwrap();

        assert!(original.exists(), "undo should restore the original file");
        assert!(!converted.exists(), "undo should remove the converted file (trashed, not left behind)");
        let restored_bytes = fs::read(&original).unwrap();
        assert_eq!(
            restored_bytes, original_bytes,
            "restored bytes must be byte-exact — a re-encode would differ"
        );

        // Best-effort cleanup: purge the converted file's trash entry so repeated test runs don't
        // pile up in the real OS trash. Never fails the test either way.
        let _ = trash::os_limited::list().map(|items| {
            let ours: Vec<_> = items
                .into_iter()
                .filter(|i| i.name.to_string_lossy() == "photo.jpg")
                .collect();
            let _ = trash::os_limited::purge_all(ours);
        });
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
    fn list_network_shares_merges_user_locations_without_hanging() {
        // The command dispatcher must ALWAYS return promptly (the OS probe is time-bounded) and must
        // include the user-added locations regardless of what the OS enumeration finds (CPE-1163).
        let shares = list_network_shares_impl(vec![r"\\example-host\share".to_string()]);
        assert!(
            shares.iter().any(|s| s.kind == "user" && s.path == r"\\example-host\share"),
            "the user-added location must appear even when no OS mounts exist: {shares:?}",
        );
        // Invalid entries are dropped; an empty request never panics.
        assert!(
            list_network_shares_impl(vec!["not a share".to_string()])
                .iter()
                .all(|s| s.kind != "user"),
            "junk user entries are skipped, not surfaced",
        );
        let _ = list_network_shares_impl(Vec::new());
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
    fn create_file_with_content_writes_the_stub() {
        let d = scratch("create_file_content");
        let created = create_file_with_content_impl(
            d.to_string_lossy().to_string(),
            "New Rich Text Document.rtf".into(),
            "{\\rtf1\\ansi }".into(),
        )
        .unwrap();
        assert!(std::path::Path::new(&created).is_file());
        assert_eq!(fs::read_to_string(&created).unwrap(), "{\\rtf1\\ansi }");
        // Won't clobber an existing file.
        assert!(create_file_with_content_impl(
            d.to_string_lossy().to_string(),
            "New Rich Text Document.rtf".into(),
            "other".into(),
        )
        .is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn create_empty_zip_makes_a_valid_openable_archive() {
        let d = scratch("create_empty_zip");
        let created = create_empty_zip_impl(
            d.to_string_lossy().to_string(),
            "New Compressed (zipped) Folder.zip".into(),
        )
        .unwrap();
        assert!(std::path::Path::new(&created).is_file());
        // A valid empty archive is the 22-byte End-Of-Central-Directory record (signature PK\x05\x06),
        // NOT a zero-byte file. cpe-server's own test opens it with the zip reader; here we assert the
        // structure the app hands the OS. (The `zip` crate lives in cpe-server, not src-tauri.)
        let bytes = fs::read(&created).unwrap();
        assert_eq!(bytes.len(), 22, "empty zip is a single EOCD record");
        assert_eq!(&bytes[0..4], &[0x50, 0x4b, 0x05, 0x06], "EOCD signature");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn rename_refuses_to_clobber_an_existing_name() {
        let d = scratch("rename_dup");
        let ctx = HeadlessCtx::new(&d);
        fs::write(d.join("a.txt"), b"a").unwrap();
        fs::write(d.join("b.txt"), b"b").unwrap();

        let r = rename_entry_impl(
            &ctx,
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
        let ctx = HeadlessCtx::new(&d);
        fs::write(d.join("a.txt"), b"a").unwrap();
        let p = d.join("a.txt").to_string_lossy().to_string();
        for bad in ["../evil.txt", "sub/b.txt", "a\\b.txt", "..", "."] {
            assert!(rename_entry_impl(&ctx, p.clone(), bad.into()).is_err(), "must reject {bad:?}");
        }
        // The file stays put and nothing escaped the folder.
        assert!(d.join("a.txt").exists());
        assert!(!d.parent().unwrap().join("evil.txt").exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn rename_moves_the_file() {
        let d = scratch("rename_ok");
        let ctx = HeadlessCtx::new(&d);
        fs::write(d.join("a.txt"), b"a").unwrap();
        let r = rename_entry_impl(
            &ctx,
            d.join("a.txt").to_string_lossy().to_string(),
            "c.txt".into(),
        );
        assert!(r.is_ok());
        assert!(d.join("c.txt").exists());
        assert!(!d.join("a.txt").exists());
        let _ = fs::remove_dir_all(&d);
    }

    // ---- CPE-1222: rename/move must migrate the tag-store entry instead of orphaning it -----------

    #[test]
    fn rename_entry_impl_migrates_a_tagged_files_entry_to_the_new_path() {
        let d = scratch("rename_tag_migrate");
        let ctx = HeadlessCtx::new(&d);
        fs::write(d.join("a.txt"), b"a").unwrap();
        let old_path = d.join("a.txt").to_string_lossy().to_string();
        cpe_server::tags::set(&ctx, &old_path, vec!["important".into()], "red".into()).unwrap();

        let new_path = rename_entry_impl(&ctx, old_path.clone(), "b.txt".into()).unwrap();

        let store = cpe_server::tags::load(&ctx).unwrap();
        assert!(!store.contains_key(&old_path), "old path must not linger in the tag store");
        assert_eq!(store[&new_path].tags(), &["important".to_string()]);
        assert_eq!(store[&new_path].label(), "red");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn rename_entry_impl_migrates_tags_for_every_file_inside_a_renamed_directory() {
        // The subtree case: renaming a folder must carry a tagged file living inside it too, not
        // just the folder's own tag entry (if it even has one — this folder doesn't).
        let d = scratch("rename_dir_tag_migrate");
        let ctx = HeadlessCtx::new(&d);
        let sub = d.join("proj");
        fs::create_dir_all(sub.join("nested")).unwrap();
        fs::write(sub.join("a.txt"), b"a").unwrap();
        fs::write(sub.join("nested").join("b.txt"), b"b").unwrap();

        let a_old = sub.join("a.txt").to_string_lossy().to_string();
        let b_old = sub.join("nested").join("b.txt").to_string_lossy().to_string();
        cpe_server::tags::set(&ctx, &a_old, vec!["keep".into()], "".into()).unwrap();
        cpe_server::tags::set(&ctx, &b_old, vec!["nested-tag".into()], "".into()).unwrap();

        let renamed = rename_entry_impl(&ctx, sub.to_string_lossy().to_string(), "archive".into()).unwrap();
        let renamed = PathBuf::from(renamed);

        let store = cpe_server::tags::load(&ctx).unwrap();
        assert!(!store.contains_key(&a_old) && !store.contains_key(&b_old), "old paths orphaned nothing should remain");
        let a_new = renamed.join("a.txt").to_string_lossy().to_string();
        let b_new = renamed.join("nested").join("b.txt").to_string_lossy().to_string();
        assert_eq!(store[&a_new].tags(), &["keep".to_string()]);
        assert_eq!(store[&b_new].tags(), &["nested-tag".to_string()]);
        let _ = fs::remove_dir_all(&d);
    }

    // ---- CPE-1225: rename/move must migrate the snapshot-schedule catalog entry too, not just tags -

    fn schedule_rule(root: &str) -> cpe_server::snapshot_schedule::ScheduleRule {
        cpe_server::snapshot_schedule::ScheduleRule {
            root: root.to_string(),
            interval_s: 3600,
            retention: cpe_server::snapshot_retention::RetentionPolicy::default(),
            enabled: true,
        }
    }

    #[test]
    fn rename_entry_impl_migrates_a_scheduled_folders_catalog_entry_to_the_new_path() {
        let d = scratch("rename_schedule_migrate");
        let ctx = HeadlessCtx::new(&d);
        let sub = d.join("watched");
        fs::create_dir_all(&sub).unwrap();
        let old_path = sub.to_string_lossy().to_string();
        cpe_server::snapshot_schedule::set_rule(&ctx, schedule_rule(&old_path)).unwrap();

        let new_path = rename_entry_impl(&ctx, old_path.clone(), "renamed".into()).unwrap();

        assert!(
            cpe_server::snapshot_schedule::get_rule(&ctx, &old_path).unwrap().is_none(),
            "old root must not linger in the schedule catalog"
        );
        let migrated = cpe_server::snapshot_schedule::get_rule(&ctx, &new_path).unwrap();
        assert_eq!(migrated.as_ref().map(|r| r.root.as_str()), Some(new_path.as_str()));
        assert_eq!(migrated.unwrap().interval_s, 3600);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn rename_entry_impl_migrates_a_nested_scheduled_root_under_a_renamed_parent() {
        // The subtree case, mirroring the tag-store test above: renaming a PARENT folder must carry a
        // nested folder's own schedule along with it, not just an entry keyed on the parent itself.
        let d = scratch("rename_schedule_subtree");
        let ctx = HeadlessCtx::new(&d);
        let parent = d.join("proj");
        let nested = parent.join("sub");
        fs::create_dir_all(&nested).unwrap();
        let nested_old = nested.to_string_lossy().to_string();
        cpe_server::snapshot_schedule::set_rule(&ctx, schedule_rule(&nested_old)).unwrap();

        let renamed = rename_entry_impl(&ctx, parent.to_string_lossy().to_string(), "archive".into())
            .unwrap();
        let nested_new = PathBuf::from(&renamed).join("sub").to_string_lossy().to_string();

        assert!(cpe_server::snapshot_schedule::get_rule(&ctx, &nested_old).unwrap().is_none());
        let migrated = cpe_server::snapshot_schedule::get_rule(&ctx, &nested_new).unwrap();
        assert_eq!(migrated.map(|r| r.root), Some(nested_new));
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
        let ctx = HeadlessCtx::new(&d);
        let src_dir = d.join("in");
        let sorted = d.join("sorted");
        fs::create_dir_all(&src_dir).unwrap();
        fs::create_dir_all(&sorted).unwrap();

        // move: file lands in `in/`, rule moves it into `sorted/`.
        let f = src_dir.join("a.txt");
        fs::write(&f, b"hi").unwrap();
        let r = run_watch_actions_impl(&ctx, f.to_string_lossy().to_string(), vec![wa("move", &sorted.to_string_lossy())]);
        assert!(r.iter().all(|x| x.ok), "{r:?}");
        assert!(!f.exists() && sorted.join("a.txt").exists()); // moved, original gone

        // copy: original stays put, a copy appears in `sorted/`.
        let g = src_dir.join("b.txt");
        fs::write(&g, b"yo").unwrap();
        run_watch_actions_impl(&ctx, g.to_string_lossy().to_string(), vec![wa("copy", &sorted.to_string_lossy())]);
        assert!(g.exists() && sorted.join("b.txt").exists()); // both exist

        // rename: in place, new name in same dir.
        let h = src_dir.join("c.log");
        fs::write(&h, b"x").unwrap();
        run_watch_actions_impl(&ctx, h.to_string_lossy().to_string(), vec![wa("rename", "c.txt")]);
        assert!(!h.exists() && src_dir.join("c.txt").exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn watch_actions_pipeline_threads_the_updated_path() {
        // rename → move: the move must act on the *renamed* file, so the pipeline threads the new path.
        let d = scratch("watch_pipe");
        let ctx = HeadlessCtx::new(&d);
        let src_dir = d.join("in");
        let dest = d.join("out");
        fs::create_dir_all(&src_dir).unwrap();
        fs::create_dir_all(&dest).unwrap();
        let f = src_dir.join("raw.dat");
        fs::write(&f, b"data").unwrap();

        let r = run_watch_actions_impl(
            &ctx,
            f.to_string_lossy().to_string(),
            vec![wa("rename", "final.dat"), wa("move", &dest.to_string_lossy())],
        );
        assert!(r.iter().all(|x| x.ok), "{r:?}");
        assert!(dest.join("final.dat").exists()); // renamed THEN moved under the new name
        assert!(!src_dir.join("raw.dat").exists() && !src_dir.join("final.dat").exists());
        let _ = fs::remove_dir_all(&d);
    }

    /// CPE-1102: `plan_watch_action_targets` is the no-filesystem-touching planner feeding
    /// `run_watch_actions`'s `note_app_op` ledger record. It must predict exactly the paths
    /// `run_watch_actions_impl` actually produces for the same pipeline (rename → move: the move's
    /// destination is computed from the *renamed* name, not the original).
    #[test]
    fn plan_watch_action_targets_mirrors_the_pipelines_actual_destination() {
        let planned = plan_watch_action_targets(
            "Z:/in/raw.dat",
            &[wa("rename", "final.dat"), wa("move", "Z:/out")],
        );
        assert_eq!(
            planned,
            vec![
                PathBuf::from("Z:/in").join("final.dat").to_string_lossy().into_owned(),
                PathBuf::from("Z:/out").join("final.dat").to_string_lossy().into_owned(),
            ]
        );
    }

    /// A `copy` step doesn't relocate the working path (unlike `move`/`rename`), so a copy-then-move
    /// pipeline's move destination is planned from the ORIGINAL name, matching `run_watch_actions_impl`.
    #[test]
    fn plan_watch_action_targets_copy_step_does_not_relocate_the_working_path() {
        let planned = plan_watch_action_targets(
            "Z:/in/a.txt",
            &[wa("copy", "Z:/backup"), wa("move", "Z:/sorted")],
        );
        assert_eq!(
            planned,
            vec![
                PathBuf::from("Z:/backup").join("a.txt").to_string_lossy().into_owned(),
                PathBuf::from("Z:/sorted").join("a.txt").to_string_lossy().into_owned(),
            ]
        );
    }

    #[test]
    fn watch_actions_report_unknown_action_per_step_without_aborting() {
        let d = scratch("watch_unknown");
        let ctx = HeadlessCtx::new(&d);
        fs::create_dir_all(&d).unwrap();
        let f = d.join("a.txt");
        fs::write(&f, b"z").unwrap();
        let r = run_watch_actions_impl(
            &ctx,
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
        let ctx = HeadlessCtx::new(&d);
        let from = d.join("from");
        let to = d.join("to");
        fs::create_dir_all(&from).unwrap();
        fs::create_dir_all(&to).unwrap();
        fs::write(from.join("m.txt"), b"m").unwrap();

        let results = move_entries_impl(
            &ctx,
            vec![from.join("m.txt").to_string_lossy().to_string()],
            to.to_string_lossy().to_string(),
        );
        assert!(results[0].ok, "{}", results[0].error);
        assert!(to.join("m.txt").exists());
        assert!(!from.join("m.txt").exists());
        let _ = fs::remove_dir_all(&d);
    }

    // CPE-1222: `moveEntries` (drag-and-drop / cut-paste) is the everyday move path — it must
    // migrate a tagged file's entry, and every tagged file inside a moved DIRECTORY, to the new
    // location. `do_move_into` auto-renames on collision, so the migration must land at whatever
    // path was actually written, not the naively-expected one.
    #[test]
    fn move_entries_impl_migrates_tags_for_a_file_and_a_moved_directorys_subtree() {
        let d = scratch("move_tag_migrate");
        let ctx = HeadlessCtx::new(&d);
        let from = d.join("from");
        let to = d.join("to");
        fs::create_dir_all(&from).unwrap();
        fs::create_dir_all(&to).unwrap();
        fs::write(from.join("m.txt"), b"m").unwrap();
        let proj = from.join("proj");
        fs::create_dir_all(&proj).unwrap();
        fs::write(proj.join("inner.txt"), b"i").unwrap();

        let m_old = from.join("m.txt").to_string_lossy().to_string();
        let inner_old = proj.join("inner.txt").to_string_lossy().to_string();
        cpe_server::tags::set(&ctx, &m_old, vec!["solo".into()], "".into()).unwrap();
        cpe_server::tags::set(&ctx, &inner_old, vec!["deep".into()], "".into()).unwrap();

        let results = move_entries_impl(
            &ctx,
            vec![m_old.clone(), proj.to_string_lossy().to_string()],
            to.to_string_lossy().to_string(),
        );
        assert!(results.iter().all(|r| r.ok), "{results:?}");

        let store = cpe_server::tags::load(&ctx).unwrap();
        assert!(!store.contains_key(&m_old) && !store.contains_key(&inner_old), "old paths orphaned nothing left");
        assert_eq!(store[&to.join("m.txt").to_string_lossy().to_string()].tags(), &["solo".to_string()]);
        assert_eq!(
            store[&to.join("proj").join("inner.txt").to_string_lossy().to_string()].tags(),
            &["deep".to_string()]
        );
        let _ = fs::remove_dir_all(&d);
    }

    // CPE-1227: `moveEntries` (drag-and-drop / cut-paste) must migrate a scheduled folder's
    // snapshot-schedule catalog entry too, not just its tags (CPE-1222 covered tags for this route;
    // CPE-1225 wired `reschedule` into `do_move_into` but left this route's coverage gap).
    #[test]
    fn move_entries_impl_migrates_a_scheduled_folders_catalog_entry_to_the_moved_path() {
        let d = scratch("move_schedule_migrate");
        let ctx = HeadlessCtx::new(&d);
        let from = d.join("from");
        let to = d.join("to");
        fs::create_dir_all(&from).unwrap();
        fs::create_dir_all(&to).unwrap();
        let watched = from.join("watched");
        fs::create_dir_all(&watched).unwrap();

        let old_path = watched.to_string_lossy().to_string();
        cpe_server::snapshot_schedule::set_rule(&ctx, schedule_rule(&old_path)).unwrap();

        let results = move_entries_impl(
            &ctx,
            vec![watched.to_string_lossy().to_string()],
            to.to_string_lossy().to_string(),
        );
        assert!(results.iter().all(|r| r.ok), "{results:?}");

        let new_path = to.join("watched").to_string_lossy().to_string();
        assert!(
            cpe_server::snapshot_schedule::get_rule(&ctx, &old_path).unwrap().is_none(),
            "old root must not linger in the schedule catalog"
        );
        let migrated = cpe_server::snapshot_schedule::get_rule(&ctx, &new_path).unwrap();
        assert_eq!(migrated.as_ref().map(|r| r.root.as_str()), Some(new_path.as_str()));
        assert_eq!(migrated.unwrap().interval_s, 3600);
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

    // A STRUCTURED checkerboard PNG (real 2D structure → balanced dHash popcount, so it is NOT excluded by
    // the CPE-1205 featureless-guard, unlike a flat/gradient image). Built with the dev-only `image` dep,
    // mirroring `cpe_server::image_similarity`'s own structured fixtures. Two byte-identical copies hash
    // equal, so they must land in one near-duplicate group.
    fn checkerboard_png() -> Vec<u8> {
        let img = image::RgbImage::from_fn(32, 32, |x, y| {
            if ((x / 4) + (y / 4)) % 2 == 0 {
                image::Rgb([20, 20, 20])
            } else {
                image::Rgb([235, 235, 235])
            }
        });
        let mut buf = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut buf, image::ImageFormat::Png)
            .unwrap();
        buf.into_inner()
    }

    #[test]
    fn find_similar_images_collect_groups_a_fixture() {
        let d = scratch("simimg");
        fs::create_dir_all(d.join("sub")).unwrap();
        // Two byte-identical STRUCTURED PNGs (across a subdir) → identical dHash → one near-duplicate group.
        let png = checkerboard_png();
        fs::write(d.join("a.png"), &png).unwrap();
        fs::write(d.join("sub/b.png"), &png).unwrap();
        // A non-image file must be ignored by the extension filter.
        fs::write(d.join("notes.txt"), b"not an image").unwrap();

        let r = cpe_server::image_similarity::find_similar_images(&d.to_string_lossy()).unwrap();
        assert!(!r.truncated);
        assert_eq!(r.files_scanned, 2, "only the two .png candidates are hashed");
        assert_eq!(r.groups.len(), 1, "the identical pair forms one group");
        assert_eq!(r.groups[0].paths.len(), 2);

        // A non-folder root is an Err, like find_duplicates.
        assert!(cpe_server::image_similarity::find_similar_images(&d.join("a.png").to_string_lossy()).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    // dir_size / dir_children_sizes tests moved with the code to `cpe_server::disk_usage` (CPE-815).

    // folder_stats tests moved with the code to `cpe_server::folder_stats` (CPE-815).

    #[test]
    fn move_exact_restores_to_the_original_name() {
        let d = scratch("move_exact");
        let ctx = HeadlessCtx::new(&d);
        fs::write(d.join("b.txt"), b"x").unwrap();

        let results = move_exact_impl(&ctx, vec![(
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
        let ctx = HeadlessCtx::new(&d);
        fs::write(d.join("a.txt"), b"keep").unwrap();
        fs::write(d.join("b.txt"), b"other").unwrap();

        let results = move_exact_impl(&ctx, vec![(
            d.join("b.txt").to_string_lossy().to_string(),
            d.join("a.txt").to_string_lossy().to_string(),
        )]);
        assert!(!results[0].ok, "undo must not clobber an existing file");
        assert_eq!(fs::read(d.join("a.txt")).unwrap(), b"keep");
        let _ = fs::remove_dir_all(&d);
    }

    // CPE-1222: undo (`moveExact`) previously never migrated tags at all — a rename/move + undo
    // round-trip silently dropped the tag on the way back. Also covers the acceptance criterion's
    // "move" case directly at this layer (a tagged file moved to an exact destination).
    #[test]
    fn move_exact_impl_migrates_tags_to_the_restored_path() {
        let d = scratch("move_exact_tag_migrate");
        let ctx = HeadlessCtx::new(&d);
        fs::write(d.join("b.txt"), b"x").unwrap();
        let old_path = d.join("b.txt").to_string_lossy().to_string();
        let new_path = d.join("a.txt").to_string_lossy().to_string();
        cpe_server::tags::set(&ctx, &old_path, vec!["restored".into()], "".into()).unwrap();

        let results = move_exact_impl(&ctx, vec![(old_path.clone(), new_path.clone())]);
        assert!(results[0].ok, "{}", results[0].error);

        let store = cpe_server::tags::load(&ctx).unwrap();
        assert!(!store.contains_key(&old_path), "old path must not linger");
        assert_eq!(store[&new_path].tags(), &["restored".to_string()]);
        let _ = fs::remove_dir_all(&d);
    }

    // CPE-1225: a folder moved via the exact-path primitive (used by both the bulk move command and
    // undo) must migrate its scheduled-snapshot catalog entry the same way it migrates tags — otherwise
    // the schedule silently stops applying once the folder lands at its new path.
    #[test]
    fn move_exact_impl_migrates_the_scheduled_catalog_entry_to_the_moved_path() {
        let d = scratch("move_exact_schedule_migrate");
        let ctx = HeadlessCtx::new(&d);
        let old_dir = d.join("watched-old");
        fs::create_dir_all(&old_dir).unwrap();
        let old_path = old_dir.to_string_lossy().to_string();
        let new_path = d.join("watched-new").to_string_lossy().to_string();
        cpe_server::snapshot_schedule::set_rule(&ctx, schedule_rule(&old_path)).unwrap();

        let results = move_exact_impl(&ctx, vec![(old_path.clone(), new_path.clone())]);
        assert!(results[0].ok, "{}", results[0].error);

        assert!(cpe_server::snapshot_schedule::get_rule(&ctx, &old_path).unwrap().is_none());
        let migrated = cpe_server::snapshot_schedule::get_rule(&ctx, &new_path).unwrap();
        assert_eq!(migrated.map(|r| r.root), Some(new_path));
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

    // ---- Spotlight commands (CPE-1214, epic CPE-704) -------------------------------------------
    // The `_impl` fns hold the real logic and are directly testable without an async runtime (same
    // pattern as `board_cards_impl` etc. above); the `#[tauri::command]` wrapper is just `spawn_blocking`.

    use cpe_server::spotlight_frecency::Visit;
    use cpe_server::spotlight_results::ResultKind;

    #[test]
    fn spotlight_search_impl_groups_caps_and_highlights() {
        let sources = vec![
            (ResultKind::Action, vec!["Reload window".to_string(), "Rename".to_string(), "Delete".to_string()]),
            (ResultKind::File, vec!["readme.md".to_string(), "read-later.md".to_string(), "notes.txt".to_string()]),
        ];
        let secs = spotlight_search_impl("re".to_string(), sources, 1);
        // Sections ordered by kind priority (Action before File), each capped at 1.
        assert_eq!(secs.len(), 2);
        assert_eq!(secs[0].kind, ResultKind::Action);
        assert_eq!(secs[0].results.len(), 1);
        assert_eq!(secs[1].kind, ResultKind::File);
        assert_eq!(secs[1].results.len(), 1);
        // The winning file result carries matched-char positions for highlighting.
        assert!(!secs[1].results[0].positions.is_empty());
        assert!(secs[1].results[0].score > 0);
    }

    #[test]
    fn spotlight_search_impl_empty_on_no_match() {
        let sources = vec![(ResultKind::File, vec!["cargo.toml".to_string()])];
        assert!(spotlight_search_impl("zzz".to_string(), sources, 5).is_empty());
    }

    #[test]
    fn spotlight_frecent_impl_ranks_recent_and_frequent_first() {
        let now = 100 * 86_400u64;
        let visits = vec![
            Visit { path: "/stale".into(), count: 20, last_used_s: now - 2 * 2_592_000 },
            Visit { path: "/rare".into(), count: 1, last_used_s: now - 1_800 },
            Visit { path: "/hot".into(), count: 5, last_used_s: now - 1_800 },
        ];
        let out = spotlight_frecent_impl(visits, now, 10);
        assert_eq!(out, vec!["/hot", "/stale", "/rare"]);
    }

    #[test]
    fn spotlight_frecent_impl_caps_at_limit() {
        let visits: Vec<Visit> =
            (0..5).map(|i| Visit { path: format!("/v{i}"), count: i + 1, last_used_s: 0 }).collect();
        assert_eq!(spotlight_frecent_impl(visits, 0, 2).len(), 2);
    }
}
