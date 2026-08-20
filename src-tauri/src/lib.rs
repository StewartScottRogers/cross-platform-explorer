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

/// Embedded terminal PTY backend (CPE-1242, epic CPE-714): the `PtySession`/`PtyRegistry` behind the
/// terminal dock's `open_pty`/`write_pty`/`resize_pty`/`close_pty` commands. Always compiled — the
/// docked terminal is a core-explorer feature, not sidecar (see the module docs for why it lives here
/// rather than in `cpe-server`).
mod pty;

/// HEIC/HEIF preview via per-OS platform APIs (CPE-1351, epic CPE-097): decode `.heic`/`.heif`/`.hif`
/// to a PNG `data:` URL through the Windows Imaging Component / macOS ImageIO — the FFI the
/// domain-crate `image_preview::encode_rgba_to_png_data_url` sink can't own (it stays Tauri- and
/// platform-FFI-free). Dispatched by plain `cfg`; the `read_heic_preview_data_url` command below is a
/// thin `spawn_blocking` into it.
mod heic_preview;

/// System-tray icon + quick-access menu + show/hide + close-to-tray (CPE-1272, epic CPE-713). Desktop-only
/// (mobile has no system tray). Renders `cpe_server::tray_quick::QuickAccess::items()` and wires the tray's
/// events; see the module docs. Gated the same way as the tray plugin bits below.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod tray;

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
    // CPE-1705: was `if dest.exists()`, whose `false` covers "not there" AND "could not tell", ahead of
    // an `fs::rename` that replaces its destination silently. A board move that landed on an unreadable
    // slot destroyed the ticket file already sitting there. CPE-1710: paired with the dangling-link check
    // via `rename_slot_refusal`, so this site cannot carry one half of the guard without the other.
    if let Some(e) = cpe_server::fsutil::rename_slot_refusal(
        &dest,
        &format!("a ticket file already exists at {}", dest.display()),
    ) {
        return Err(e);
    }
    let md = std::fs::read_to_string(&src).map_err(|e| e.to_string())?;
    std::fs::write(&src, ticket_board::set_status(&md, status)).map_err(|e| e.to_string())?;
    // The refusal above runs BEFORE the status rewrite, so a refused move never leaves a ticket whose
    // frontmatter says one column while the file sits in another. This second call re-checks immediately
    // before the rename (CPE-1710 round 3) — guard and destructive call in one function.
    cpe_server::fsutil::rename_into_slot(
        &src,
        &dest,
        &format!("a ticket file already exists at {}", dest.display()),
    )?;
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

/// Read every `CPE-*.md` directly inside `dir`, parsing each with `parse` and collecting the hits.
/// Non-CPE files (the folders' `wiki.md` explainers) and unreadable entries are skipped.
fn collect_epics_in(dir: &std::path::Path, parse: impl Fn(&str) -> Option<ticket_board::Epic>, out: &mut Vec<ticket_board::Epic>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if !name.starts_with("CPE-") || !name.ends_with(".md") {
            continue;
        }
        if let Ok(md) = std::fs::read_to_string(&p) {
            if let Some(epic) = parse(&md) {
                out.push(epic);
            }
        }
    }
}

/// List the repo's epics for the board's epic-organized view (CPE-530): the epics in the five status
/// folders of `Ticketing/Epics/` + closed epics from `Ticketing/Tickets/Done/` (top level), each
/// `epic`-tagged. Read-only.
///
/// Since CPE-1676 the Epics queue has the same five status folders as `Tickets/` and **the folder is
/// the status**, so an epic read out of `Epics/<Folder>/` takes its status from that folder, not from
/// its `status:` line. Epics closed before that migration still sit in `Tickets/Done/` (and its dated
/// subfolders, which reach the board via `board_archived`) — those keep using their frontmatter.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn board_epics(root: String) -> Vec<ticket_board::Epic> {
    tauri::async_runtime::spawn_blocking(move || board_epics_impl(root))
        .await.unwrap()
}

fn board_epics_impl(root: String) -> Vec<ticket_board::Epic> {
    let base = std::path::Path::new(&root).join("Ticketing");
    let epics_dir = base.join("Epics");
    let mut epics = Vec::new();
    for column in ticket_board::EPIC_COLUMNS {
        collect_epics_in(&epics_dir.join(column), |md| ticket_board::epic_from_in(md, column), &mut epics);
    }
    // Epics closed before CPE-1676 live in the status-flow's Done (CPE-1128); status from frontmatter.
    collect_epics_in(&base.join("Tickets").join("Done"), ticket_board::epic_from, &mut epics);
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
use cpe_server::model::{extension_of, is_hidden, DirEntry, EntryInfo, ListDirResult, OpResult, Place};
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

/// True if `a` and `b` refer to the SAME on-disk path — canonicalised so `.`/`..`/separator/symlink
/// differences don't fool it, with a literal-compare fallback when a path can't be canonicalised (e.g. it
/// doesn't exist yet). Used to refuse a copy/paste whose target resolves to its own source (CPE-1375).
fn same_path(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => a == b,
    }
}

/// Whether a candidate copy target is **provably** free (CPE-1696) — the pure classifier behind every
/// collision probe in [`unique_target`] and [`resolve_conflict`], split out (mirroring
/// `cpe_server::dispatch::classify_path_error` and `dest_parent_stat_error` below) so the
/// `NotFound`-vs-everything-else taxonomy is unit-testable without a real filesystem: permission bits are
/// platform- and privilege-dependent — inert as root, and on Windows `Path::exists()` is not refused by a
/// deny ACE at all — so an ACL-based test alone would leave this taxonomy unverified on some machines.
///
/// `stat` is the outcome of [`Path::try_exists`], which returns `io::Result<bool>` instead of folding
/// every failure into `false` the way [`Path::exists`] does. `Ok(false)` (or an explicit `NotFound`) is
/// the **only** answer that means free.
///
/// **An unknown counts as occupied, deliberately — it does not abort the operation (PR #889 review).**
/// The first cut of this fix returned `Err` on an unknown, which was a behaviour regression on Windows:
/// the probe moved from `Path::exists()` (which no deny ACE on the target *alone* can refuse — a parent
/// `(RD)` deny is needed too, see `cpe_server::fsutil::deny_stat_of`) to `try_exists()` (which one
/// can), so a destination holding an unreadable `f.txt` went from "auto-rename to `f - Copy.txt` and
/// succeed" to "fail the whole copy". Refusing is the right instinct at a site whose only options are
/// *refuse* or *overwrite blind* — but this site has a third option, because [`unique_target`]'s entire
/// job is to find some free name and it can simply try the next candidate. Treating the unknown as
/// occupied is exactly as safe (nothing is ever written to a path we could not prove empty) with no
/// denial of service.
///
/// **CPE-1705 — this enum and its classifier no longer live here.** They were the first copy of what
/// turned out to be an eighteen-site decision, so they moved to `cpe_server::fsutil` alongside
/// `clobber_refusal`, and this module now uses the shared one. There is exactly one implementation of
/// "is this slot free?" in the codebase; the AC that asked for it put the reason plainly — *twelve copies
/// of the same check is how the thirteenth gets missed*, and five rounds of this bug had already proved
/// it empirically.
use cpe_server::fsutil::{classify_target_slot, TargetSlot};

/// Classify one candidate slot from its [`Path::try_exists`] outcome. Thin local alias kept so this
/// module's call sites and their CPE-1696 doc references read unchanged.
fn classify_copy_target(stat: std::io::Result<bool>) -> TargetSlot {
    classify_target_slot(&stat)
}

/// Whether a candidate is *provably* free. Thin wrapper over [`classify_copy_target`] for the single-shot
/// probe in [`resolve_conflict`], which has no candidate sequence to advance through.
fn copy_target_is_free(stat: std::io::Result<bool>) -> bool {
    classify_copy_target(stat) == TargetSlot::Free
}

/// **CPE-1715.** Probe a name-picking candidate, folding the dangling-link hazard into the same
/// `try_exists`-shaped result [`classify_copy_target`]/[`copy_target_is_free`] already read — a change to
/// their *input*, not a second guard bolted in front of the eventual `fs::rename`/`fs::copy`.
///
/// Thin one-line alias over [`cpe_server::fsutil::name_pick_slot_probe`] (see its doc comment for the full
/// rationale, including why the fallback check is broader than "is it a link"). The real implementation
/// moved there under review — it is character-for-character the same shape as
/// `cpe_server::fsutil::symlink_slot_refusal`'s stat expression, and CPE-1705's own doc comment says why a
/// fourteenth copy of "is this slot free?" logic must not live in the app adapter: *"twelve copies of the
/// same check is how the thirteenth gets missed."* This alias exists only so this module's two call sites
/// and its CPE-1715 tests read unchanged.
fn probe_name_pick_slot(candidate: &Path) -> std::io::Result<bool> {
    cpe_server::fsutil::name_pick_slot_probe(candidate)
}

/// How many candidate slots in a row may come back [`TargetSlot::Unknown`] before [`unique_target`]
/// stops probing (CPE-1696).
///
/// Treating an unknown as occupied is what keeps a single unreadable file from aborting a copy — but if
/// the *directory* is what cannot be read (a dead network mount, a revoked share), then **every** one of
/// the 10,000 candidates comes back `Unknown` and the naive loop performs 10,000 stats before falling
/// through. On the very mount where that happens each stat can block for seconds, so the fix for a hard
/// failure would have become a hang. A run this long cannot be a real name collision — it means the
/// directory itself is unreadable — so we stop and hand back the pathological fallback, which the
/// caller's own `fs::copy`/`fs::rename` will then fail on quickly and honestly.
const MAX_CONSECUTIVE_UNKNOWN_SLOTS: usize = 8;

/// Pick a non-colliding name in `dir`, Explorer-style:
/// "report.txt" -> "report - Copy.txt" -> "report - Copy (2).txt".
/// We never overwrite an existing file — silent overwrite is data loss.
///
/// **CPE-1696.** All three collision probes here were `!candidate.exists()`, and `Path::exists()`
/// collapses every `stat` failure into `false` — so a candidate whose stat was refused (permission denied
/// along the resolved path, a dead network mount) was returned as a *free* name and the caller's
/// `fs::copy` / `copy_dir_all` / `fs::rename` wrote over it. That contradicted this function's own
/// contract two lines up. A candidate is now only free when [`copy_target_is_free`] can prove it; an
/// unknown is skipped like any other occupied name, so the caller still gets a usable target and the
/// unreadable path is left untouched.
fn unique_target(dir: &Path, file_name: &str) -> PathBuf {
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

    // n = 0 is the bare name, n = 1 is " - Copy", n >= 2 is " - Copy (n)" — the original sequence.
    let mut unknown_run = 0usize;
    for n in 0..10_000u32 {
        let candidate = match n {
            0 => dir.join(file_name),
            1 => build(" - Copy"),
            _ => build(&format!(" - Copy ({n})")),
        };
        match classify_copy_target(probe_name_pick_slot(&candidate)) {
            TargetSlot::Free => return candidate,
            TargetSlot::Occupied => unknown_run = 0,
            TargetSlot::Unknown => {
                unknown_run += 1;
                if unknown_run >= MAX_CONSECUTIVE_UNKNOWN_SLOTS {
                    break; // the directory itself is unreadable — stop stat'ing it
                }
            }
        }
    }
    // Pathological fallback; effectively unreachable.
    dir.join(format!("{file_name}.{}", std::process::id()))
}

/// Recursively copy a directory tree.
///
/// **CPE-1765.** Was `create_dir_all` + `fs::copy` per file — both of which *follow* a symlink at the
/// final component, so a link planted at a name between [`unique_target`]'s pick and this write sent the
/// tree outside the folder the user chose. It is now a one-line delegation to
/// [`cpe_server::fsutil::copy_tree_into_claimed_slot`], which claims every directory and file name it
/// creates with `create_dir`/`create_new`; see that function for the measurement and the trade-offs.
/// Kept as a named wrapper only so this module's call sites and its `copy_dir_all_copies_the_whole_tree`
/// test read unchanged.
fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), String> {
    cpe_server::fsutil::copy_tree_into_claimed_slot(src, dst)
}

/// List the immediate children of `path`.
// Async so a listing on a slow drive runs off the main thread (CPE-760).
/// List a directory's entries. Model + the shared walker live in `cpe_server::listing` (CPE-815); this
/// is a thin `spawn_blocking` dispatcher.
///
/// Returns [`ListDirResult`], not a bare `Vec<DirEntry>` (CPE-1708): `filtered` carries how many
/// provider-supplied entries were left out of `entries` because their name could not be shown safely —
/// always `0` for a local listing. This is the field CPE-1704 deliberately left at the Tauri boundary
/// (an `eprintln!` only); see [`ListDirResult`]'s doc for why the count travels as typed data, never a
/// synthetic row.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn list_dir(path: String) -> Result<ListDirResult, String> {
    // Local fast-path (CPE-1511): a local URI takes the EXACT same code path as before — the same
    // classification `require_local` did, then the same `listing::list_dir` on a blocking thread — so the
    // plain explorer's hot path is byte-for-byte unchanged (PURPOSE.md). Only a recognised remote scheme
    // diverts to the provider router.
    match cpe_server::fs_route::route(&path) {
        cpe_server::fs_route::Route::Local => {
            tauri::async_runtime::spawn_blocking(move || local_list_dir_result(&path))
                .await
                .map_err(|e| e.to_string())?
        }
        cpe_server::fs_route::Route::Remote(_) => {
            tauri::async_runtime::spawn_blocking(move || remote_list_dir_impl(path).map(listing_to_result))
                .await
                .map_err(|e| e.to_string())?
        }
    }
}

/// `list_dir`'s LOCAL arm (CPE-1708): a free function so the "a local listing always reports
/// `filtered: 0`" AC line ("Confirm every other provider (SFTP, WebDAV, FTP, local) reports zero and is
/// unaffected") is independently unit testable against the SAME code the real command calls, not a
/// hand-duplicated copy of it that could silently drift from what `list_dir` actually runs.
fn local_list_dir_result(path: &str) -> Result<ListDirResult, String> {
    cpe_server::listing::list_dir(path).map(|entries| ListDirResult { entries, filtered: 0 })
}

/// The `RemoteListing` → `ListDirResult` link of the CPE-1708 chain: a free function (not a `From` impl —
/// neither type is local to this crate, so the orphan rule blocks that) so it's independently unit
/// testable without needing a live provider pool. `entries`/`filtered` both carry straight across —
/// nothing here can silently drop or zero the count.
fn listing_to_result(listing: cpe_vfs::connect::RemoteListing) -> ListDirResult {
    ListDirResult { entries: listing.entries, filtered: listing.filtered }
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

/// `list_dir_stream`'s terminal result (CPE-1708). `total` is the entry count streamed, unchanged since
/// CPE-663/665; `filtered` carries the SAME count `list_dir`'s [`ListDirResult::filtered`] carries — how
/// many provider-supplied entries were left out because their name could not be shown safely — but via
/// the streaming twin, which is the pane's actual first-paint path (`ExplorerPane.loadListing`; `list_dir`
/// itself is only the collect-to-vec convenience path, STREAMING.md). Always `0` for a local walk.
#[derive(serde::Serialize)]
#[cfg_attr(feature = "specta-bindings", derive(specta::Type))]
struct StreamDirResult {
    total: usize,
    filtered: usize,
}

/// Streaming variant of `list_dir` (CPE-663, epic CPE-662): pushes `DirEntry` batches over an IPC channel
/// as the directory is read, so the frontend paints the first rows immediately instead of waiting for the
/// whole listing. `stream_id` (frontend-supplied, monotonic) registers a cancel flag polled each batch, so
/// a superseded walk stops promptly instead of reading a huge folder to completion (CPE-665). Returns
/// [`StreamDirResult`] once the walk completes (or is cancelled) — see its doc for `filtered` (CPE-1708).
// Async so a listing on a slow/network drive streams from a blocking thread and never freezes the main
// thread (CPE-760). The `Channel` batches still arrive live; only the walk moves off the UI thread.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn list_dir_stream(
    path: String,
    stream_id: u64,
    on_entry: tauri::ipc::Channel<Vec<DirEntry>>,
) -> Result<StreamDirResult, String> {
    // Local fast-path (CPE-1511): unchanged from before for a local URI (same classification + same
    // streaming walker on a blocking thread). A remote scheme streams the resolved provider's listing over
    // the SAME `Channel` + cancel registry, so the pane paints identically and `cancel_dir_stream` works
    // for remote too (STREAMING.md).
    match cpe_server::fs_route::route(&path) {
        cpe_server::fs_route::Route::Local => {
            tauri::async_runtime::spawn_blocking(move || {
                list_dir_stream_impl(path, stream_id, on_entry)
                    .map(|total| StreamDirResult { total, filtered: 0 })
            })
            .await
            .map_err(|e| e.to_string())?
        }
        cpe_server::fs_route::Route::Remote(_) => {
            tauri::async_runtime::spawn_blocking(move || {
                remote_list_dir_stream_impl(path, stream_id, on_entry)
            })
            .await
            .map_err(|e| e.to_string())?
        }
    }
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

/// # What a dangling link at a save destination means — the decision, recorded here (CPE-1725)
///
/// **It is refused, and the refusal names the link. Both whole-file save paths give that answer, because
/// they now call the same function to produce it.**
///
/// Before CPE-1725 the app had two commands that write a whole file back over a path the user opened and
/// they answered the question oppositely — measured by PR #899's UAT round 2, not reasoned about:
///
/// | path | dangling link at the destination | result |
/// |---|---|---|
/// | `metadata_write` (Metadata Studio) | refused, naming the link | link survives, target NOT created |
/// | `write_file_text` (this one, then `fs::write`) | `Ok(8)` | link survives, target **created** |
///
/// Re-measured on Windows while fixing it, by putting the pre-fix `fs::write` back under
/// `cpe_1725_both_save_paths_refuse_a_dangling_link_and_neither_creates_its_target`:
///
/// ```text
/// write_file_text conjured a file at the far end of a broken link
/// (…\cpe_test_cpe1725_parity_47344\notes.txt-target-that-does-not-exist) — pre-fix `fs::write`
/// followed the link and created it while reporting success (result was Ok(7))
/// ```
///
/// `fs::write` is `O_CREAT|O_TRUNC` and *follows* the final component, so through a link pointing at
/// something that is no longer there it silently **creates** the missing file. Nothing was destroyed and
/// the link was not harmed — which is why this was filed separately from CPE-1716's data-loss bug rather
/// than folded into it — but the user got a file conjured at the far end of a broken link with no
/// indication that is what happened, and the same app told them the opposite thing one dialog over.
///
/// **Refuse wins** for three reasons, in order of weight:
///
/// 1. A save is meant to land in the file the user is looking at. Through a broken link it lands somewhere
///    they cannot see, under a name they did not type, and "saved" is then a true statement about the
///    wrong file. An error naming the link is strictly more information than a silent success.
/// 2. It is the answer `metadata_write` already gives, so choosing it changes one command rather than two,
///    and leaves the Metadata Studio's shipped user documentation true.
/// 3. Creating the target is only *arguably* right (`fs::write` semantics, "save should create a file that
///    isn't there") for the case where the path is a **plain missing name** — and that case is unaffected:
///    a save to a name where nothing exists still creates the file, here as before. The refusal is scoped
///    to a link that exists and does not resolve, which is a broken thing, not an absent one.
///
/// The mechanism is the point as much as the verdict: both paths get the answer from
/// [`cpe_server::fsutil::resolve_write_target`] — literally the same call, so they cannot drift apart
/// again by one of them being edited, and the user reads the same sentence from either dialog.
///
/// ## What this deliberately does NOT do: change how the bytes are written
///
/// **The obvious implementation was to route this through [`cpe_server::fsutil::replace_file_contents`]**,
/// which bundles the dangling-link decision *and* CPE-1716's atomic temp-sibling + rename write. PR #904
/// shipped that, and the independent UAT measured what it cost on the **ordinary** path — files with no
/// link anywhere near them, which is essentially all of this command's traffic. Same file, same run,
/// pre-fix `fs::write` versus the rename-based save:
///
/// ```text
/// 0600 private:    pre -> 0o600 | post -> 0o644     (a private file becomes world-readable)
/// 0755 executable: pre -> 0o755 | post -> 0o644     (a script silently stops being executable)
/// pre:  attrs=0x22 (HIDDEN) ADS=Ok("ZoneId=3\r\n")
/// post: attrs=0x20 (HIDDEN lost) ADS=Err(NotFound)  (Mark of the Web destroyed)
/// SHARE_READ|WRITE open: pre -> Ok(()) | post -> Err("Access is denied. (os error 5)")
/// ```
///
/// All four have one cause: a rename **replaces the file object**, so everything attached to the object
/// rather than to the bytes — mode, ownership, Windows attributes, alternate data streams, and the
/// identity that open handles refer to — is left behind on the file that got unlinked. `fs::write`
/// truncates the existing object and keeps all of it.
///
/// So the trade was: make the **rare** case (an interrupted save) safer, by making the **common** case
/// worse in four ways, two of them security-relevant and one of them a functional regression — a save
/// that used to succeed while another program held the file open now fails. That is a bad trade on its
/// face, and it is also **not what this ticket asked for**: CPE-1725's question is what a dangling link
/// *means*, and its own acceptance criterion treats the atomicity move as conditional ("**if**
/// `write_file_text` moves to `replace_file_contents`…"). Widening a decision about broken symlinks into
/// a permissions change for every text save is the exact blast-radius mistake CPE-1725 was itself filed
/// to avoid — the ticket exists because CPE-1716 declined to fold this command into *its* fix.
///
/// The narrowing keeps the whole point and drops the whole cost: **share the classifier, not the write
/// strategy.** [`cpe_server::fsutil::resolve_write_target`] is the decision; `fs::write` stays the write.
/// Parity is if anything stronger, since both commands now call that one entry point directly rather than
/// one of them reaching it through another function.
///
/// **Item 4 is why "just copy the attributes across" was not the answer either.** Mode is easy and
/// Windows attributes are tractable, but a rename onto a file another process holds open fails regardless
/// of what has been copied — nothing about the staged file changes the target's sharing mode. So that
/// route closes three of four at best. **That is exactly how CPE-1739 landed**: `metadata_write`'s save
/// now carries the mode and extended attributes across on Unix and switches to `ReplaceFileW` on Windows
/// (which preserves the attribute word, the ACL and named streams including `Zone.Identifier`), closing
/// items 1–3 — while **item 4 stayed open**, measured, because the obstacle is the target's sharing mode
/// and no amount of care on the replacement file reaches it. So this command stays on `fs::write`: the
/// acceptance criterion for re-routing it was *all four*, and it is still three.
///
/// ## What is therefore true of this command's writes, all of it unchanged from before CPE-1725
///
/// - **Not atomic.** A save killed part-way leaves the file part-written. That is a real defect, and it
///   is still open after CPE-1739: closing it means moving to the rename-based writer, whose remaining
///   cost (item 4) has not gone away. It is not a *regression*, and buying it at that price is not this
///   ticket's call to make.
/// - **Mode, ownership, Windows attributes, alternate data streams and hard links are all preserved**,
///   because the file object is never replaced. Pinned by
///   `cpe_1725_an_ordinary_save_keeps_the_same_file_object_and_its_mode`, which reds if anyone re-routes
///   this to a rename-based writer.
/// - **A live link is still followed** to its target, which the link keeps pointing at.
///
/// ## The Save-As callers, and the one question this does NOT answer for them
///
/// Two of this command's five call sites are "edit the file I have open" — `savePreviewText`, the preview
/// pane's in-place text editor, which exists in both `App.svelte` and `lib/preview/loaders.ts`. The other
/// three — the audit-log export, the file-list CSV/TXT export and the tag-store JSON export, all in
/// `App.svelte` — go through a native **Save As** dialog, so they *claim a name* rather than edit a file.
/// (Enumerated by `rg 'writeFileText' src/`, which is the whole of the frontend; the backend has no other
/// caller.) By
/// [`cpe_server::fsutil::replace_file_contents`]'s own distinguishing question ("am I claiming this name,
/// or editing this file?") a claiming site arguably wants the vault's non-following policy instead:
/// refuse or replace a **live** link at the chosen name rather than write through it.
///
/// **That is deliberately not decided here, and this is the record of the absence** (Evidence Rule 5). It
/// is not this ticket's question — CPE-1725 is scoped to the *dangling* case, on which all five call sites
/// now agree — and the live-link behaviour of the three is **unchanged** by this commit, since `fs::write`
/// followed a live link to its target exactly as `replace_file_contents` does. Changing it would be a new
/// refusal for a case nobody has reported, on a command whose name says "write text", so it needs its own
/// ticket and its own test rather than a ride on this one.
///
/// ## The three `fs::write` siblings CPE-1725 was asked to inventory
///
/// The search that produced this list, so the negative is not stated wider than it: `rg 'fs::write'` over
/// `src-tauri/src/lib.rs` (the four commands the ticket names) plus reading each write's own guard chain.
/// It is **not** a claim about every write in the repo.
///
/// - **`macro_convert_in_place`** (`fs::write(to, ..)`) — **different question, and it has no guard.** `to`
///   is a *new* name derived by swapping the extension (`from != to` is enforced), so this is a create
///   site, not an edit site: the primitive it wants is CPE-1718's
///   [`cpe_server::fsutil::create_slot_refusal`] + [`cpe_server::fsutil::create_exclusive`], not this one —
///   resolving a link would be actively wrong for a name being claimed. Today it has neither, so a link at
///   `to` is written through and the original is then trashed. Not fixed here on purpose: it is a different
///   guard, on a command with rollback semantics, and it needs image fixtures this test module does not
///   have. Filed as **CPE-1734**; see the note at that function.
/// - **`batch_execute`'s in-place overwrite** — **the ticket's premise is wrong about this one, and it is
///   worth being wrong in the safe direction.** It does not use `fs::write` at all: it goes through
///   `batch_media::open_output_verified`, which opens with `O_NOFOLLOW` / `FILE_FLAG_OPEN_REPARSE_POINT`,
///   then re-checks `symlink_metadata` and the handle's reparse bit, and refuses **any** link — live or
///   dangling — before a byte is written. So it is already stricter than either save path and needs no
///   change.
/// - **`forge_resolve_file`** (`fs::write(<repo>/<file>, ..)`, `sidecar-platform` only) — **must stay
///   different, and the reason is structural.** Its whole contract is that a merge-conflict resolution
///   cannot escape the repo (`is_safe_repo_relative` refuses `..`, absolute and drive-prefixed paths).
///   *Resolving* a link would defeat exactly that: a symlink inside the working tree can point anywhere,
///   so following it is how a repo-confined write stops being repo-confined. A create-style refusal is the
///   right shape there, not this function; recorded at that site too.
fn write_file_text_impl(path: String, contents: String) -> Result<u64, String> {
    cpe_server::fs_route::require_local(&path)?;
    // The dangling-link decision, from the same call `metadata_write_impl` makes — which is what makes the
    // two answers provably identical rather than coincidentally so. A live link resolves to its target; a
    // dangling one is refused here, before anything is written.
    let target = cpe_server::fsutil::resolve_write_target(std::path::Path::new(&path))?;
    // Still `fs::write`, deliberately — see "What this deliberately does NOT do" above. Truncating the
    // existing file keeps its mode, ownership, Windows attributes and alternate data streams; a
    // temp-sibling + rename would silently drop all of them.
    //
    // The error names the path, which the bare `e.to_string()` this replaced did not. It matters for the
    // one construction where the two save paths' messages differ (PR #904 UAT): a link pointing at a
    // *directory* resolves fine and then fails on the write, and "Access is denied" with no path is not a
    // diagnosis. `display_path` strips Windows' `\\?\` verbatim prefix, which `canonicalize` puts on every
    // resolved path and which the user has never seen before.
    fs::write(&target, contents.as_bytes())
        .map_err(|e| format!("{}: {e}", cpe_server::fsutil::display_path(&target)))?;
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
        // Single-file compression formats (CPE-1439): no decoder is wired in (unlike gzip via flate2),
        // so there's no entry list to browse — a friendly "compressed file" summary beats a raw hex dump.
        "xz" => cpe_server::binary_preview::compressed_file_info(&path, "XZ"),
        "bz2" => cpe_server::binary_preview::compressed_file_info(&path, "BZip2"),
        "zst" => cpe_server::binary_preview::compressed_file_info(&path, "Zstandard"),
        "lz" => cpe_server::binary_preview::compressed_file_info(&path, "Lzip"),
        "lzma" => cpe_server::binary_preview::compressed_file_info(&path, "LZMA"),
        // generic binary (.bin/.dat) and anything else routed here: hex dump
        _ => cpe_server::binary_preview::hex_dump(&path, 64 * 1024),
    }
}

/// Structured PE/ELF/Mach-O summary (format, arch, sections/imports/exports/symbols, x86/x64 disasm)
/// for the Binary Inspector (CPE-1572 DTO + CPE-1581 disasm, epic CPE-1562 "Binary Inspector"). Model
/// lives in `cpe_server::binary_preview::binary_info`; this is a thin `spawn_blocking` dispatcher.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn binary_info(path: String) -> Result<cpe_server::model::BinaryInfo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        ensure_previewable_size(&path, PREVIEW_INFO_MAX_BYTES)?;
        cpe_server::binary_preview::binary_info(&path)
    })
    .await.map_err(|e| e.to_string())?
}

/// x86/x64 disassembly of a PE/ELF/Mach-O binary's code section (CPE-1581, epic CPE-1562 "Binary
/// Inspector" slice 2) — the same list embedded in [`binary_info`]'s `disasm` field, exposed on its
/// own so the Binary Inspector's disassembly tab can fetch it without also paying for the
/// sections/imports/exports/symbols tables. `cpe_server::binary_preview` has no separate path-based
/// disasm entry point (its `disassemble` fn decodes already-located code bytes, not a path), so this
/// thin `spawn_blocking` dispatcher reuses `binary_info`'s parse and returns just its `disasm` list.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn binary_disasm(path: String) -> Result<Vec<cpe_server::model::BinaryInstruction>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        ensure_previewable_size(&path, PREVIEW_INFO_MAX_BYTES)?;
        cpe_server::binary_preview::binary_info(&path).map(|info| info.disasm)
    })
    .await.map_err(|e| e.to_string())?
}

/// Structured CLR metadata (assembly identity, `AssemblyRef`s, capped `TypeDef`/`MethodDef` name
/// listings) for a managed .NET PE (CPE-1596, epic CPE-1562 "Binary Inspector" slice 3) —
/// `Ok(None)` for a native (non-managed) PE, per [`cpe_server::model::BinaryInfo::is_managed`]'s
/// contract. Parser lives in `cpe_server::dotnet_metadata::read`; this is a thin `spawn_blocking`
/// dispatcher, same size guard as [`binary_info`]/[`binary_disasm`].
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn dotnet_metadata(path: String) -> Result<Option<cpe_server::model::DotnetMetadata>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        ensure_previewable_size(&path, PREVIEW_INFO_MAX_BYTES)?;
        cpe_server::dotnet_metadata::read(&path)
    })
    .await.map_err(|e| e.to_string())?
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

/// JWT preview decoder (CPE-1418, epic CPE-1417): read-only header/payload/claims viewer, never a
/// signature verifier. Thin async dispatcher into `cpe_server::jwt_preview`; reads the file as text
/// (capped by the same preview size guard the other whole-file info readers use) and hands it straight to
/// the pure decoder, which never panics on malformed input.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn jwt_preview(path: String) -> Result<cpe_server::jwt_preview::JwtPreview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        ensure_previewable_size(&path, PREVIEW_INFO_MAX_BYTES)?;
        let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        Ok(cpe_server::jwt_preview::jwt_preview(&text))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// `.eml` email preview (CPE-1434, epic CPE-1433): read-only RFC 822/MIME viewer — headers + MIME parts
/// + attachments + a sanitized plain-text body. Thin async dispatcher into `cpe_server::email_preview`;
/// reads the file's raw bytes (capped by the same preview size guard the other whole-file info readers
/// use) and hands them to the pure decoder, which never renders HTML, never loads remote resources, and
/// never panics on malformed input.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn email_preview(path: String) -> Result<cpe_server::email_preview::EmailPreview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        ensure_previewable_size(&path, PREVIEW_INFO_MAX_BYTES)?;
        let bytes = fs::read(&path).map_err(|e| e.to_string())?;
        Ok(cpe_server::email_preview::email_preview(&bytes))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// `.ics` iCalendar preview (CPE-1435, epic CPE-1433): read-only RFC 5545 viewer — VEVENT/VTODO/VJOURNAL
/// components decoded into summary/when/where/who + a readable recurrence summary. Thin async dispatcher
/// into `cpe_server::ical_preview`; reads the file's raw bytes (capped by the same preview size guard the
/// other whole-file info readers use) and hands them to the pure decoder, which never panics on malformed
/// input.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn ical_preview(path: String) -> Result<cpe_server::ical_preview::IcalPreview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        ensure_previewable_size(&path, PREVIEW_INFO_MAX_BYTES)?;
        let bytes = fs::read(&path).map_err(|e| e.to_string())?;
        Ok(cpe_server::ical_preview::ical_preview(&bytes))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// `.vcf` vCard preview (CPE-1436, epic CPE-1433): read-only contact-card viewer — FN/N/ORG/TITLE/TEL/
/// EMAIL/ADR/URL/BDAY decoded, with PHOTO reported presence-only (its bytes are never returned over IPC).
/// Thin async dispatcher into `cpe_server::vcard_preview`; reads the file's raw bytes (capped by the same
/// preview size guard the other whole-file info readers use) and hands them to the pure decoder, which
/// never panics on malformed input.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn vcard_preview(path: String) -> Result<cpe_server::vcard_preview::VcardPreview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        ensure_previewable_size(&path, PREVIEW_INFO_MAX_BYTES)?;
        let bytes = fs::read(&path).map_err(|e| e.to_string())?;
        Ok(cpe_server::vcard_preview::vcard_preview(&bytes))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Certificate/CSR/public-key decoder (CPE-1419, epic CPE-1417): read-only X.509 viewer, never a
/// verifier. Thin async dispatcher into `cpe_server::cert_decode`; reads the file's raw bytes (capped by
/// the same preview size guard the other whole-file info readers use) and hands them to the pure decoder,
/// which never panics on malformed input.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn cert_decode(path: String) -> Result<cpe_server::cert_decode::CertPreview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        ensure_previewable_size(&path, PREVIEW_INFO_MAX_BYTES)?;
        let bytes = fs::read(&path).map_err(|e| e.to_string())?;
        Ok(cpe_server::cert_decode::cert_decode(&bytes))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Certificate creation (CPE-1420, epic CPE-1417): generate a keypair + self-signed X.509 certificate
/// and write both PEM files to disk. Thin async dispatcher into `cpe_server::cert_create`; the pure
/// generation logic lives there, this only writes the two resulting PEMs to the caller-chosen paths. The
/// private key is written with restrictive permissions where the OS supports it (POSIX chmod 0600 on
/// Unix — see [`write_key_pem_restrictive`]) and is NEVER logged or echoed back over IPC: on success this
/// returns only `()`, never the key material, so it can't leak into a frontend console or Diagnostics-mode
/// invoke log.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn cert_create(
    params: cpe_server::cert_create::CertCreateParams,
    cert_path: String,
    key_path: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let result = cpe_server::cert_create::cert_create(&params)?;
        fs::write(&cert_path, &result.cert_pem).map_err(|e| e.to_string())?;
        write_key_pem_restrictive(&key_path, &result.key_pem)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Certificate signing / issue-from-CSR (CPE-1421, epic CPE-1417): parse a PKCS#10 CSR and issue a leaf
/// X.509 certificate for it, signed by an existing CA's certificate + private key. Thin async dispatcher
/// into `cpe_server::cert_sign`; the pure issuance logic lives there, this only reads the CSR/CA-cert/
/// CA-key PEM files from disk and writes the issued certificate PEM to `out_cert_path`. The CA private
/// key is read only to sign — it is NEVER returned over IPC, logged, or echoed back: on success this
/// returns only `()`, never key material, same as [`cert_create`].
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn cert_issue_from_csr(
    csr_path: String,
    ca_cert_path: String,
    ca_key_path: String,
    validity_days: u32,
    out_cert_path: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        ensure_previewable_size(&csr_path, PREVIEW_INFO_MAX_BYTES)?;
        ensure_previewable_size(&ca_cert_path, PREVIEW_INFO_MAX_BYTES)?;
        ensure_previewable_size(&ca_key_path, PREVIEW_INFO_MAX_BYTES)?;
        let csr_pem = fs::read_to_string(&csr_path).map_err(|e| e.to_string())?;
        let ca_cert_pem = fs::read_to_string(&ca_cert_path).map_err(|e| e.to_string())?;
        let ca_key_pem = fs::read_to_string(&ca_key_path).map_err(|e| e.to_string())?;
        let issued_pem =
            cpe_server::cert_sign::cert_issue_from_csr(&csr_pem, &ca_cert_pem, &ca_key_pem, validity_days)?;
        fs::write(&out_cert_path, &issued_pem).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Write a private-key PEM to `path` and tighten its permissions where the OS supports it: POSIX chmod
/// 0600 (owner read/write only) on Unix, right after writing. Windows inherits the parent directory's
/// ACL — narrowing that needs a Windows-specific ACL call this dispatcher doesn't attempt, the same
/// Unix-only scope `set_permissions` above already has.
fn write_key_pem_restrictive(path: &str, key_pem: &str) -> Result<(), String> {
    fs::write(path, key_pem).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 3D-model geometry/stats reader (CPE-1333, epic CPE-118): reads a binary/ASCII STL or Wavefront OBJ's
/// triangle/vertex counts + bounding box for the metadata-pane fallback the (blocked) interactive-viewer
/// epic's acceptance criteria call for. Thin async dispatcher into `cpe_server::model_3d`; capped by the
/// same preview size guard the other whole-file info readers use.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn read_model_info(path: String) -> Result<Option<cpe_server::model_3d::ModelInfo>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        ensure_previewable_size(&path, PREVIEW_INFO_MAX_BYTES)?;
        let bytes = fs::read(&path).map_err(|e| e.to_string())?;
        Ok(cpe_server::model_3d::read_model_info(&bytes))
    })
    .await
    .map_err(|e| e.to_string())?
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

/// Extract the embedded JPEG preview from a camera-RAW file (`.cr2`/`.nef`/`.arw`) as a
/// `data:image/jpeg;base64,...` URL the `<img>` tag can show (CPE-1349, epic CPE-102). Mirrors
/// `read_image_data_url` above: a thin `spawn_blocking` dispatcher into `cpe_server::camera_raw`,
/// capped by the same preview size guard. `Err` when the file can't be parsed as a TIFF-based raw
/// container or carries no embedded JPEG preview — the frontend falls back to the metadata view.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn read_raw_preview_data_url(path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        ensure_previewable_size(&path, PREVIEW_INFO_MAX_BYTES)?;
        cpe_server::camera_raw::read_raw_preview_data_url(&path)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Decode a DICOM (`.dcm`) file's pixel data to a PNG `data:` URL the `<img>` tag can show
/// (CPE-1350, epic CPE-102). Mirrors `read_raw_preview_data_url` above: a thin `spawn_blocking`
/// dispatcher into `cpe_server::dicom`, capped by the same preview size guard. `Err` on a corrupt
/// file or a transfer syntax needing a native codec this build doesn't carry (JPEG2000/JPEG-LS/
/// vendor) — the frontend falls back to the metadata view (tags, if readable, still show).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn read_dicom_image_data_url(path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        ensure_previewable_size(&path, PREVIEW_INFO_MAX_BYTES)?;
        cpe_server::dicom::read_dicom_image_data_url(&path)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Read a curated set of DICOM tags (patient/study identity + basic imaging attributes) for the
/// preview pane (CPE-1350). A thin `spawn_blocking` dispatcher into `cpe_server::dicom` — no size
/// cap needed, `dicom-object` reads the header/data-set structurally rather than slurping the whole
/// file. `Err` only when the file can't be opened as DICOM at all.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn read_dicom_tags(path: String) -> Result<Vec<(String, String)>, String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::dicom::read_dicom_tags(&path))
        .await
        .map_err(|e| e.to_string())?
}

/// Decode a HEIC/HEIF (`.heic`/`.heif`/`.hif`) file to a PNG `data:` URL the `<img>` tag can show
/// (CPE-1351, epic CPE-097), via the platform image stack (Windows Imaging Component / macOS
/// ImageIO). Mirrors `read_raw_preview_data_url` above: a thin `spawn_blocking` dispatcher into the
/// app-adapter `heic_preview` module (the FFI can't live in `cpe-server`), capped by the same preview
/// size guard. `Err` on a corrupt file, an unsupported platform, or — commonly on Windows — a missing
/// OS HEIF codec (the Store "HEIF Image Extensions"); the frontend falls back to the metadata view.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn read_heic_preview_data_url(path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        ensure_previewable_size(&path, PREVIEW_INFO_MAX_BYTES)?;
        heic_preview::decode_heic_preview(&path)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Structural-validity check for a `.pdf` file, called BEFORE the preview pane hands it to WebView2's
/// embedded PDF viewer for the raw `<iframe>` render (CPE-1357): a malformed or empty PDF (no resolvable
/// cross-reference table, or a `/Pages` tree declaring zero pages) can crash the WebView2 PDF renderer
/// and take the whole app down, so an `Err` here routes the pane to the metadata fallback instead of
/// ever reaching the iframe. Thin `spawn_blocking` dispatcher into `cpe_server::media_meta_read::
/// pdf_validity` (a pure byte-scan, not pdfium — no native dependency needed for this check), capped by
/// the same preview size guard as the other automatic-on-selection preview readers above. `Ok(Some(n))`
/// is a known page count, `Ok(None)` means the scanner couldn't resolve `/Pages` (e.g. compressed
/// cross-reference streams) but the header/xref checks passed — still treated as previewable.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn read_pdf_validity(path: String) -> Result<Option<u32>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        ensure_previewable_size(&path, PREVIEW_INFO_MAX_BYTES)?;
        let bytes = fs::read(&path).map_err(|e| e.to_string())?;
        cpe_server::media_meta_read::pdf_validity(&bytes)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Downsampled audio-waveform peak array — exactly `buckets` `(min, max)` sample pairs in ascending time
/// order regardless of the source file's length (CPE-1478, epic CPE-720): the first concrete backend
/// deliverable of the audio/video player pane's waveform strip (CPE-1431), landed backend-first ahead of
/// its GUI consumer per the established pattern for this epic. Thin `spawn_blocking` dispatcher into
/// `cpe_server::media_waveform::extract_waveform_peaks`, which shells out to the same bundled `ffmpeg`
/// subprocess `thumbnail`'s video-frame path uses (never linked in-process) and bounds the PCM read to a
/// fixed byte cap so a long or crafted audio file can't OOM the process — see that module's doc for the
/// full design. `Err` on a missing ffmpeg binary, a non-zero ffmpeg exit, or a nonexistent/empty/
/// undecodable input.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn audio_waveform_peaks(path: String, buckets: usize) -> Result<Vec<(f32, f32)>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        cpe_server::media_waveform::extract_waveform_peaks(Path::new(&path), buckets)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// A PNG thumbnail of an image file as a `data:` URL the `<img>` tag can show (CPE-642), served from
/// an mtime-keyed on-disk cache (CPE-644). Also covers `.svg` (rasterized) and `.ttf`/`.otf`/`.woff`
/// glyph-sheet specimens (CPE-1236) — the format dispatch lives entirely in
/// `cpe_server::thumb_source`, so this stays a thin one-line-per-branch delegate. The raster/PSD/SVG/
/// font source-file size cap lives INSIDE `cpe_server::thumb_source::decode_thumb_image` (CPE-1447/
/// CPE-1449), not here — it used to be an `ensure_previewable_size` call at this call site, but that
/// gated video extensions too, which `decode_thumb_image` dispatches early to ffmpeg (streams the file,
/// never reads it whole); the cap now sits AFTER that video dispatch, in the one place that actually
/// does the unbounded `fs::read`, so video thumbnails are never wrongly refused for being "too large".
/// Errors (rather than hangs) on an unsupported or malformed source, so the frontend falls back to an
/// icon.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn thumbnail(app: tauri::AppHandle, path: String, max_edge: u32) -> Result<String, String> {
    use base64::Engine;
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
/// `thumbnail_cached`/`make_thumbnail_png` decoder as `compute`, and forward each streamed `ThumbResult`
/// over `on_thumb` as it lands (STREAMING.md). The 128 MiB source-file size cap that keeps a huge raster
/// image from being `fs::read` in full lives inside `cpe_server::thumb_source::decode_thumb_image`
/// itself (CPE-1447/CPE-1449) — this command doesn't gate anything extra, so it can't drift from the
/// single `thumbnail` command's behavior the way the old lib.rs-side gate did. An oversized source
/// returns `Err` from `decode_thumb_image`, which `run_thumb_batch` already turns into `data_url: None`
/// — the existing decode-failure fallback the frontend renders as the type icon. `stream_id` registers a
/// cancel flag `cancel_thumbnails_stream` can trip when the frontend's visible window moves on before
/// this batch finishes draining. Async + `spawn_blocking` — the decode work is real file + CPU work and
/// must never run on the UI thread.
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

/// Bounded windowed read for the log preview (CPE-1637, epic CPE-1568 slice 8): reads one page of a text
/// file — the **tail** by default (`end: None`), the same `max_bytes` cap other preview reads use — so a
/// multi-megabyte real-world log (`CBS.log`, `dism.log`, …) is viewable instead of outright refused like
/// `read_file_text` above it. Pass a previous [`cpe_server::log_window::LogWindow::window_start`] back in
/// as `end` to page further back. Thin dispatcher into `cpe_server::log_window::read_window`, which does
/// the bounded seek+read (never the whole file, regardless of its size) and the line/UTF-8-boundary
/// alignment — see that module's docs.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn read_log_window(
    path: String,
    max_bytes: u64,
    end: Option<u64>,
) -> Result<cpe_server::log_window::LogWindow, String> {
    tauri::async_runtime::spawn_blocking(move || {
        cpe_server::fs_route::require_local(&path)?;
        cpe_server::log_window::read_window(Path::new(&path), max_bytes, end)
    })
    .await
    .map_err(|e| e.to_string())?
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
    // CPE-1705 — the highest-traffic instance of the stat-collapse class. This was
    // `if target.exists() { Err("already exists") }`, and `Path::exists()` is `metadata().is_ok()`: a
    // stat that FAILS for any reason other than absence also answers `false`, so the guard passed and
    // `fs::rename` below ran — and `fs::rename` replaces its destination silently on both Windows and
    // Unix. The user's existing file was destroyed with no warning and no error. Two independent guards,
    // because two independent things can be sitting at `target`:
    // a real entry, and a DANGLING SYMLINK — which the occupancy check genuinely cannot see: `try_exists`
    // follows links, so it correctly answers `Ok(false)` for one, while `fs::rename` does not follow the
    // final component and destroys the link. A different bug (CPE-1461 family) at the same line.
    //
    // CPE-1710 made the pairing a single call. This site had both halves open-coded and was one of only
    // two that did; four sibling sites had just the first, so the convention was not holding. Order and
    // wording are unchanged — `rename_slot_refusal` runs occupancy first, exactly as these two lines did.
    cpe_server::fsutil::rename_into_slot(src, &target, &format!("\"{new_name}\" already exists"))?;
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
    use trash::os_limited::restore_all;

    // CPE-1791: routed through the panic-catching wrapper, not a raw `list()` call (review blocker 1 —
    // this call site was missed the first time round). `.or_fail()` because a caught panic must be
    // reported as a failure, never silently treated as "the trash is empty" (which would misreport
    // every one of these paths as already emptied).
    let all = match list_trash_catching_dependency_panics().or_fail() {
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
        //
        // CPE-1705: the comment said "never clobber" and `Path::exists()` did not deliver it — every
        // stat failure answered `false`, so an unreadable original location was handed to `restore_all`
        // as free. What the OS trash then does to whatever is really there is out of our hands, which is
        // exactly why this check has to be the one that is sure.
        if let Some(e) = cpe_server::fsutil::clobber_refusal(
            target,
            "Something already exists at the original location",
        ) {
            results.push(OpResult::err(target, e));
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

// ---- Trash listing / restore / empty (CPE-1558, epic CPE-1486 slice 1) --------------------------
// A browsable in-app Trash, built on the same `trash::os_limited` API `restore_from_trash` already
// uses (Windows + Linux only — macOS has no listing/restore API, see `can_restore_from_trash_impl`).
// Per `docs/design/SERVER-ARCHITECTURE.md`, these stay in the Tauri adapter (only the `TrashEntry` DTO
// lives in `cpe_server::model`) since the `trash` crate itself must never become a `cpe-server` dep.
// Typed bindings + specta export wired in CPE-1559 — these commands are reachable from the frontend
// via bindings.gen.ts (listTrash/listTrashStream/restoreTrashItems/emptyTrash).

/// Batch size for `list_trash_stream`'s channel flushes (STREAMING.md): small enough that even a tiny
/// Trash reveals rows in one flush, big enough that a full one rarely needs more than a couple.
#[cfg(any(target_os = "windows", target_os = "linux"))]
const TRASH_LIST_BATCH: usize = 256;

/// Maps one `trash::TrashItem` to the cross-platform-safe `cpe_server::model::TrashEntry`, skipping
/// (never failing) an item whose id/name/path can't round-trip through UTF-8 — mirrors `list_dir`'s
/// "skip unreadable entries rather than fail the whole listing" rule. A per-item metadata-lookup
/// failure, by contrast, does NOT skip the entry: `TrashEntry.size` is `Option<u64>` specifically so a
/// failed lookup can degrade to `size: None` while the id/name/original_path/time_deleted (still valid
/// and enough to restore or purge the item) are kept (CPE-1559 review fix on CPE-1558).
///
/// CPE-1804: the skip itself is unchanged — an id we can't round-trip is genuinely unusable for restore
/// or purge, so there is nothing honest to show for it — but it is no longer *silent*.
/// [`stream_trash_entries`] counts every `None` this returns and both listing commands report that count
/// to the frontend, because "we dropped some of your trash" is exactly the fact a user needs and the one
/// a bare skip withheld: an all-undecodable trash rendered as "Trash is empty", and a mixed one silently
/// under-counted.
#[cfg(any(target_os = "windows", target_os = "linux"))]
fn trash_item_to_entry(item: &trash::TrashItem) -> Option<cpe_server::model::TrashEntry> {
    trash_item_to_entry_with_size(item, trash_item_size(item))
}

/// The one part of the mapping that touches the OS: this item's size, or `None` when the lookup fails.
/// Split out (CPE-1804 review, #962 Linux red) so the skip decision below can be exercised without it.
#[cfg(any(target_os = "windows", target_os = "linux"))]
fn trash_item_size(item: &trash::TrashItem) -> Option<u64> {
    trash::os_limited::metadata(item).ok().and_then(|meta| meta.size.size())
}

/// The entire skip-or-keep decision, with `size` supplied rather than looked up — so it depends only on
/// the item's three `OsStr` fields and reaches no OS call at all.
///
/// This split exists because routing *fabricated* `TrashItem`s through [`trash_item_size`] panicked on
/// Linux CI: `trash::os_limited::metadata` derives the in-trash file path from `item.id` via
/// `Path::new(id).parent().unwrap().parent().unwrap()` (`trash-5.2.6/src/freedesktop.rs:350`), and a
/// bare, non-path-shaped id has fewer than two ancestors. On Windows the same call is a COM lookup that
/// merely returns `Err`, which is why a Windows-green run did not generalise.
///
/// That panic is **not** a production bug and deliberately did NOT get wrapped in CPE-1791's
/// `catch_unwind` boundary: every real `id` comes from `list()`, which sets it to the full
/// `<trash>/info/<name>.trashinfo` path (`freedesktop.rs:121`), so both `.parent()` calls always
/// succeed. Extending the catch to `metadata` would have been a test accommodation dressed as
/// hardening — it would only ever fire for input production cannot produce.
///
/// `size` cannot affect the outcome (it is passed straight through to `trash_entry_from_fields`'s
/// last argument, which never gates the `?` chain), so nothing about the skip is stubbed by supplying
/// it — which is what makes leaving the OS call out of the walker's unit tests honest rather than
/// convenient. That contract is itself pinned by
/// `trash_entry_from_fields_degrades_to_none_size_when_metadata_lookup_failed`.
#[cfg(any(target_os = "windows", target_os = "linux"))]
fn trash_item_to_entry_with_size(
    item: &trash::TrashItem,
    size: Option<u64>,
) -> Option<cpe_server::model::TrashEntry> {
    trash_entry_from_fields(
        item.id.to_str(),
        item.name.to_str(),
        item.original_parent.to_str(),
        item.time_deleted,
        size,
    )
}

/// Pure id/name/path-validation half of [`trash_item_to_entry`], split out so the skip-vs-degrade
/// behaviour is unit-testable without touching the real OS trash (constructing a `trash::TrashItem`
/// whose metadata lookup reliably fails would depend on OS-specific plumbing).
#[cfg(any(target_os = "windows", target_os = "linux"))]
fn trash_entry_from_fields(
    id: Option<&str>,
    name: Option<&str>,
    original_parent: Option<&str>,
    time_deleted: i64,
    size: Option<u64>,
) -> Option<cpe_server::model::TrashEntry> {
    Some(cpe_server::model::trash_entry(
        id?.to_string(),
        name?.to_string(),
        original_parent?.to_string(),
        time_deleted,
        size,
    ))
}

/// One shared "walker" over the already-materialized `list()` result: maps + skips-on-error, then
/// chunks into `batch`-sized `Vec`s via `flush` — the STREAMING.md shape, adapted for a payload the OS
/// API hands back all at once rather than one this code walks lazily itself. Backs both
/// `list_trash`/`list_trash_stream` so they can never diverge.
///
/// Returns **how many items were skipped** (CPE-1804). Counting here rather than at each call site is
/// what keeps the two commands honest in lockstep: the count comes out of the same loop that does the
/// skipping, so a skip that is not counted is not reachable.
/// `map` is always [`trash_item_to_entry`] in production — passed explicitly rather than hardcoded so
/// the OS size lookup inside it can be left out of a unit test (CPE-1804 review, #962 Linux red). See
/// [`trash_item_to_entry_with_size`] for why fabricated items must not reach that lookup, and why
/// hardening it instead would have been a test accommodation rather than a fix. Tests pass the *real*
/// skip decision with only `size` supplied, so the genuine `OsStr::to_str()` behaviour on the genuine
/// fields is still what is under test — nothing about skipping is stubbed.
#[cfg(any(target_os = "windows", target_os = "linux"))]
#[must_use = "the skipped count is how the frontend learns this listing is incomplete (CPE-1804)"]
fn stream_trash_entries(
    items: Vec<trash::TrashItem>,
    batch: usize,
    map: impl Fn(&trash::TrashItem) -> Option<cpe_server::model::TrashEntry>,
    mut flush: impl FnMut(Vec<cpe_server::model::TrashEntry>),
) -> usize {
    let mut buf = Vec::with_capacity(batch.min(items.len()));
    let mut skipped = 0usize;
    for item in &items {
        if let Some(entry) = map(item) {
            buf.push(entry);
            if buf.len() >= batch {
                flush(std::mem::take(&mut buf));
            }
        } else {
            skipped += 1;
        }
    }
    if !buf.is_empty() {
        flush(buf);
    }
    skipped
}

/// The single place that decides whether a listing pass is reported as incomplete (CPE-1804). Both
/// listing commands call it so the two routes into "incomplete" — a caught `list()` panic that wiped the
/// whole pass (CPE-1791) and per-item non-UTF-8 skips that thinned it — can never disagree about the
/// flag, and so a third route added later has one obvious place to join.
///
/// Takes the two facts directly and deliberately never looks at `entries`: an empty `entries` is the
/// shape of a *healthy* empty Trash just as much as a wiped one, so inferring incompleteness from the
/// list is the exact bug CPE-1803 fixed.
#[cfg(any(target_os = "windows", target_os = "linux"))]
fn listing_is_degraded(panic_degraded: bool, skipped: usize) -> bool {
    panic_degraded || skipped > 0
}

// ---- CPE-1791: contain a dependency panic at the trash-listing boundary --------------------------
// `trash::os_limited::list()` panics on Linux (`trash-5.2.6/src/freedesktop.rs:139-140`, a
// `split.next().unwrap()`) when a `.trashinfo` file's body has a line with no `=`. The realistic
// trigger is a foreign tool or a hand-edited file — a genuinely non-conforming `.trashinfo` is a
// *static* on-disk condition, not (mainly) a race: `list()` does `.lines().skip(1)`, so the moment right
// after `move_to_trash`'s own first `writeln!("[Trash Info]")` and before its second `writeln!("Path=…")`
// produces a file with exactly one line, which `list()` never even reaches the loop body for (it just
// warns and skips it, same as a file missing `Path=` entirely) — not the panicking shape. Only a
// mid-write torn `write()` splitting the literal `"Path="` string itself would reach it via that route,
// which is vanishingly rare.
//
// Before this fix the panic surfaced through `tauri::async_runtime::spawn_blocking` as an opaque
// `JoinError`, taking out the whole Trash view for one bad entry — the exact thing `CLAUDE.md`'s
// "filesystem commands skip entries they can't read" rule forbids for `list_dir`, and the Trash view is
// a filesystem listing too.
//
// `catch_unwind`, applied to every production call site that invokes `list()`
// (`list_trash_impl`/`list_trash_stream`/`restore_from_trash_impl`/`restore_trash_items_impl`/
// `empty_trash_impl` — all five, via [`list_trash_catching_dependency_panics`]): a panic inside
// `list()` is caught at the boundary rather than left to propagate as an opaque `JoinError`. What
// happens next depends on the caller: a *listing* pass (`list_trash_impl`/`list_trash_stream`) degrades
// to "no entries from this pass" — a listing is allowed to come back thin; it self-heals on the next
// refresh once the malformed file is fixed, removed, or (for the rare genuine race) once the concurrent
// write finishes. A *restore/empty* caller must NOT do that: treating a caught panic as "the trash is
// empty" would misreport every restore target as already emptied, or report a purge of nothing as
// success — those callers propagate the failure as an `Err` instead. See [`TrashListOutcome`]. A
// custom, thread-local-gated panic hook (installed once, process-wide) suppresses the default noisy
// backtrace-style dump for JUST this known, handled panic, without touching how any unrelated panic on
// any other thread is reported.
//
// A SECOND layer was attempted and abandoned — worth recording, not just deleting, per this ticket's
// own review: a Linux-only "quarantine" that pre-scanned `.trashinfo` files for the exact malformed
// shape and moved ONLY those aside before `list()` ran, restoring them right after, so `list()` never
// saw the bad file and every OTHER entry in the same folder kept listing normally — a true per-entry
// skip, matching the ticket's original "show every other entry, only the bad one skipped" ask more
// closely than the coarser fallback below. It went through three review rounds and was rejected because
// every round surfaced a NEW correctness bug in the same mechanism, each one moving real files inside
// the user's actual trash directory:
//   - round 2: not crash-durable — `Drop` (which restored a quarantined file) doesn't run on SIGKILL,
//     OOM-kill, power loss, or an abort, so a kill mid-listing could leave a file permanently outside
//     `info/`, invisible to every trash tool and surviving "Empty Trash" too.
//   - round 2: the quarantine destination was keyed by the original filename, so two overlapping
//     listings (independent blocking-pool threads) could clobber each other's held copy via `rename`'s
//     silent-overwrite semantics.
//   - round 3, the one that actually killed it: the quarantine guard's lifetime was scoped to the
//     `list()` call, so a quarantined file was restored to `info/` *before* `empty_trash_gated`'s
//     `targets` (computed from that same `list()` call) had a chance to include it — meaning `Empty
//     Trash` would report success while the malformed entry, and its real payload in `files/`, silently
//     survived on disk. That is precisely the "one malformed file breaks everything" failure this ticket
//     exists to fix, just inverted into a false-success instead of a crash — and for the realistic,
//     *static* trigger this module's second paragraph describes, it would have been the ORDINARY
//     outcome of Empty Trash, not a rare edge case. Every fix attempted for round 3 added more
//     complexity to a mechanism that had already produced two prior bugs, which was the deciding factor
//     against it (CPE-1791 review, 2026-08-20): each additional guard was another chance to be wrong
//     again, on a wide-blast-radius path (deleting a user's trash contents) rather than a narrow one.
//
// `catch_unwind` alone keeps the smaller, honestly-kept promise instead: a malformed file makes a
// listing pass come back thin (or restore/empty fail loudly) rather than crash — worse browsing UX than
// true per-entry skip, but no risk of moving, clobbering, or losing real user data, and a fraction of
// the code. If per-entry skip is wanted again later, the quarantine approach's three failure modes above
// are the bar any replacement design has to clear.

#[cfg(any(target_os = "windows", target_os = "linux"))]
thread_local! {
    /// Set for the duration of the `trash::os_limited::list()` call inside
    /// [`list_trash_catching_dependency_panics`] on THIS thread, and only this thread. Each
    /// `spawn_blocking` closure runs to completion on its own blocking-pool thread before that thread
    /// picks up another task, so per-thread state here is race-free without needing to save/restore
    /// the process-global panic hook on every call.
    static SUPPRESS_TRASH_LIST_PANIC_SPEW: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Installed once. While [`SUPPRESS_TRASH_LIST_PANIC_SPEW`] is set on the panicking thread — i.e. the
/// panic came from inside the `catch_unwind`ed `list()` call below — print one clean diagnostic line
/// instead of letting the default hook dump its full backtrace-style spew, since the caller is about
/// to catch and handle this anyway. A panic on any other thread (the flag unset there) falls through
/// to the previous hook completely unchanged.
#[cfg(any(target_os = "windows", target_os = "linux"))]
fn install_trash_list_panic_hook() {
    static INSTALLED: std::sync::Once = std::sync::Once::new();
    INSTALLED.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if SUPPRESS_TRASH_LIST_PANIC_SPEW.with(std::cell::Cell::get) {
                eprintln!(
                    "cpe: trash::os_limited::list() panicked inside the `trash` crate ({info}) — caught \
                     at the boundary; this listing pass returns what it has instead of crashing (CPE-1791)"
                );
            } else {
                previous(info);
            }
        }));
    });
}

/// Outcome of [`list_trash_catching_dependency_panics`] — kept distinct from a plain `Result` so a
/// *genuine* `trash::Error` (e.g. no trash folder exists yet) and a *caught panic* are never conflated:
/// callers decide independently what each means for them. See the module comment above this section.
#[cfg(any(target_os = "windows", target_os = "linux"))]
enum TrashListOutcome {
    Ok(Vec<trash::TrashItem>),
    /// A genuine `trash::Error` surfaced normally — not a panic.
    Error(String),
    /// `list()` panicked and was caught at the boundary.
    PanicCaught,
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
impl TrashListOutcome {
    /// For a *listing* pass (`list_trash_impl`/`list_trash_stream`): a caught panic degrades to an
    /// empty `Vec` for this one pass rather than failing the whole Trash view. A genuine error still
    /// propagates — unaffected by CPE-1791, this is the pre-existing behaviour.
    ///
    /// The returned `bool` is `true` only when THIS pass degraded from a caught panic — never inferred
    /// from the `Vec` being empty, since a genuinely empty Trash also produces an empty `Vec` and the two
    /// must stay distinguishable (CPE-1803). Both listing call sites thread it straight through to the
    /// frontend instead of collapsing it back into "empty".
    fn degrade_panic_to_empty(self) -> Result<(Vec<trash::TrashItem>, bool), String> {
        match self {
            Self::Ok(items) => Ok((items, false)),
            Self::Error(e) => Err(e),
            Self::PanicCaught => Ok((Vec::new(), true)),
        }
    }

    /// For a *restore/empty* caller: a caught panic is treated as a genuine failure, exactly like a
    /// `trash::Error` — CPE-1791 review, blocker 1. Silently treating it as "the trash is empty" would
    /// misreport every restore target as already emptied, or report a purge of nothing as success.
    fn or_fail(self) -> Result<Vec<trash::TrashItem>, String> {
        match self {
            Self::Ok(items) => Ok(items),
            Self::Error(e) => Err(e),
            Self::PanicCaught => Err(
                "the OS trash dependency panicked while listing the Recycle Bin and was caught at the \
                 boundary — treat this as a failed lookup, not an empty trash (CPE-1791)"
                    .to_string(),
            ),
        }
    }
}

/// Boundary containment for `trash::os_limited::list()` (CPE-1791) — see the module comment above this
/// section for the design (and for why a second, per-entry-skip layer was attempted and abandoned).
/// Every production call site that calls `trash::os_limited::list()` goes through this instead of
/// calling it directly.
#[cfg(any(target_os = "windows", target_os = "linux"))]
fn list_trash_catching_dependency_panics() -> TrashListOutcome {
    install_trash_list_panic_hook();
    SUPPRESS_TRASH_LIST_PANIC_SPEW.with(|f| f.set(true));
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(trash::os_limited::list));
    SUPPRESS_TRASH_LIST_PANIC_SPEW.with(|f| f.set(false));
    match outcome {
        Ok(Ok(items)) => TrashListOutcome::Ok(items),
        Ok(Err(e)) => TrashListOutcome::Error(e.to_string()),
        Err(_) => {
            eprintln!(
                "cpe: trash::os_limited::list() panicked and was caught at the boundary (CPE-1791) — \
                 the caller decides what that means for it: a listing pass returns an empty page for \
                 this one call, a restore/empty call fails outright rather than acting on an empty list"
            );
            TrashListOutcome::PanicCaught
        }
    }
}

/// Collect-to-vec Trash listing, for tests and any caller that wants the whole list at once.
#[cfg(any(target_os = "windows", target_os = "linux"))]
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn list_trash() -> Result<cpe_server::model::TrashListing, String> {
    tauri::async_runtime::spawn_blocking(list_trash_impl).await.map_err(|e| e.to_string())?
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn list_trash_impl() -> Result<cpe_server::model::TrashListing, String> {
    list_trash_from(list_trash_catching_dependency_panics(), trash_item_to_entry)
}

/// The whole body of [`list_trash_impl`], with its one uncontrollable input — the outcome of the OS
/// `list()` call — taken as a parameter instead of read from the machine's real Recycle Bin. Mirrors
/// [`empty_trash_gated`]'s injection seam, and exists for the same reason: the interesting behaviour is
/// what this function *does* with that outcome, and a test can only pin it if it can choose the outcome.
///
/// CPE-1804 review (#962, blocking 1): without this split, substituting `degraded: panic_degraded` below
/// left the entire suite green — the walker tests never build a listing, the [`listing_is_degraded`]
/// truth-table test never checks that anyone *calls* it, and the real-trash round-trip tests only ever
/// exercise the clean pass, where the two expressions agree. "Both commands fold both routes" was
/// guaranteed by reading the code, which is exactly the certified-by-omission shape this ticket
/// criticised in the old pin. It is now guaranteed by a test that constructs its own input.
///
/// CPE-1791: a caught dependency panic degrades THIS ONE listing pass to empty rather than failing the
/// whole Trash view — see `TrashListOutcome::degrade_panic_to_empty`. Restore/empty call sites do NOT do
/// this; they use `.or_fail()` instead.
///
/// CPE-1803: `degraded` rides along on the response instead of being swallowed here, so the frontend can
/// render "the Trash couldn't be read" instead of misreporting a degraded pass as "Trash is empty".
///
/// CPE-1804: the SECOND route into an incomplete listing — items skipped for a non-UTF-8
/// id/name/original_parent — rides along too, as a count. Unlike the panic route this one can leave
/// `entries` non-empty, which is why `degraded` must not be read as "and therefore empty" (CPE-1805).
#[cfg(any(target_os = "windows", target_os = "linux"))]
fn list_trash_from(
    outcome: TrashListOutcome,
    map: impl Fn(&trash::TrashItem) -> Option<cpe_server::model::TrashEntry>,
) -> Result<cpe_server::model::TrashListing, String> {
    let (items, panic_degraded) = outcome.degrade_panic_to_empty()?;
    let mut entries = Vec::new();
    let skipped = stream_trash_entries(items, TRASH_LIST_BATCH, map, |batch| entries.extend(batch));
    Ok(cpe_server::model::TrashListing {
        entries,
        degraded: listing_is_degraded(panic_degraded, skipped),
        skipped,
    })
}

/// Streamed twin of [`list_trash_from`] — same injected outcome, plus the batch sink injected too, so
/// the streamed command's fold is pinned by a test rather than by inspection (CPE-1804 review, blocking
/// 1). Keeping the two bodies adjacent and identically shaped is what makes a divergence between them
/// visible; keeping them both testable is what makes one *fail*.
#[cfg(any(target_os = "windows", target_os = "linux"))]
fn trash_stream_summary_from(
    outcome: TrashListOutcome,
    map: impl Fn(&trash::TrashItem) -> Option<cpe_server::model::TrashEntry>,
    mut send: impl FnMut(Vec<cpe_server::model::TrashEntry>),
) -> Result<cpe_server::model::TrashStreamSummary, String> {
    let (items, panic_degraded) = outcome.degrade_panic_to_empty()?;
    let mut count = 0usize;
    let skipped = stream_trash_entries(items, TRASH_LIST_BATCH, map, |batch| {
        count += batch.len();
        send(batch);
    });
    Ok(cpe_server::model::TrashStreamSummary {
        count,
        degraded: listing_is_degraded(panic_degraded, skipped),
        skipped,
    })
}

/// Streamed Trash listing (STREAMING.md): flushes batches over `on_entry` as they're mapped so a large
/// Trash paints immediately instead of blocking on one big `Vec`. `os_limited::list()` already hands
/// back everything at once (there's no lazy walk to interrupt), so — like `metadata_column_cells`'s
/// visible-window batches — this has no cancel registry; a listing is bounded by what's literally
/// sitting in the Recycle Bin.
#[cfg(any(target_os = "windows", target_os = "linux"))]
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn list_trash_stream(
    on_entry: tauri::ipc::Channel<Vec<cpe_server::model::TrashEntry>>,
) -> Result<cpe_server::model::TrashStreamSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        // CPE-1791/CPE-1803/CPE-1804: the body lives in `trash_stream_summary_from` so it can be tested
        // against a chosen `TrashListOutcome` — see that function's doc comment. All this adapter adds is
        // the two things a test genuinely can't supply: the real OS listing and the Tauri channel.
        trash_stream_summary_from(list_trash_catching_dependency_panics(), trash_item_to_entry, |batch| {
            let _ = on_entry.send(batch);
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Turns a `trash::Error` from a *single-item* restore into a short, distinguishable message instead of
/// the crate's `{:?}`-based `Display`, which for `RestoreCollision`/`RestoreTwins` would dump the whole
/// `Vec<TrashItem>` of "remaining items" into the error string.
#[cfg(any(target_os = "windows", target_os = "linux"))]
fn describe_restore_error(e: &trash::Error) -> String {
    match e {
        trash::Error::RestoreCollision { path, .. } => {
            format!("something already exists at {} — restore skipped", path.display())
        }
        trash::Error::RestoreTwins { path, .. } => {
            format!("another trashed item shares the original path {} — restore skipped", path.display())
        }
        other => other.to_string(),
    }
}

/// Restore specific Trash items by id (as returned by `list_trash`/`list_trash_stream`). Restores
/// **one item at a time** — rather than a single `restore_all(matched)` batch call — so a
/// `RestoreCollision`/`RestoreTwins` on one item is reported against just that item and doesn't abort
/// (or falsely blame) the rest of the selection, mirroring `restore_from_trash_impl`'s per-item
/// target-exists check.
#[cfg(any(target_os = "windows", target_os = "linux"))]
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn restore_trash_items(ids: Vec<String>) -> Vec<OpResult> {
    tauri::async_runtime::spawn_blocking(move || restore_trash_items_impl(ids))
        .await.unwrap()
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn restore_trash_items_impl(ids: Vec<String>) -> Vec<OpResult> {
    use trash::os_limited::restore_all;

    // CPE-1791: see `restore_from_trash_impl` — routed through the panic-catching wrapper with
    // `.or_fail()`, not a raw `list()` call.
    let all = match list_trash_catching_dependency_panics().or_fail() {
        Ok(v) => v,
        Err(e) => return ids.iter().map(|id| OpResult::err(Path::new(id), &e)).collect(),
    };

    ids.iter()
        .map(|id| {
            let item = match all.iter().find(|item| item.id.to_string_lossy() == id.as_str()) {
                Some(item) => item,
                None => {
                    return OpResult::err(
                        Path::new(id),
                        "Not found in the Recycle Bin — it may have been emptied",
                    )
                }
            };

            let target = item.original_path();

            // Never clobber: if something now occupies the original path, refuse rather than
            // overwrite it — same rule as `restore_from_trash_impl`, and the same CPE-1705 collapse.
            if let Some(e) = cpe_server::fsutil::clobber_refusal(
                &target,
                "Something already exists at the original location",
            ) {
                return OpResult::err(&target, e);
            }

            match restore_all([item.clone()]) {
                Ok(()) => OpResult::ok(&target),
                Err(e) => OpResult::err(&target, describe_restore_error(&e)),
            }
        })
        .collect()
}

/// Empty the Trash: purge everything (`ids: None`) or just the given items (`ids: Some`). Purging is
/// permanent — a purged item is gone from the Recycle Bin with nothing left to restore.
///
/// **Refuses up front** ([`Err`], nothing purged, the OS trash never even listed) when `confirmed` is
/// `false` (CPE-1651's sibling audit, same shape as `delete_permanent`/`shred_paths`/`vault_create`).
/// The gate covers BOTH scopes deliberately: purging a named subset (`ids: Some`) destroys those items
/// exactly as irrecoverably as purging the lot, so "the UI must confirm before calling this with `None`"
/// — what this doc comment used to say, and delegate to the frontend — was both an ungated promise and
/// too narrow a one. `TrashView.svelte`'s Empty-confirm dialog is the only place allowed to set it.
#[cfg(any(target_os = "windows", target_os = "linux"))]
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn empty_trash(ids: Option<Vec<String>>, confirmed: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || empty_trash_impl(ids, confirmed))
        .await.map_err(|e| e.to_string())?
}

/// Pure selection logic for `empty_trash`: `ids: None` means "everything", `Some(ids)` means "just the
/// items whose id is named". Factored out from `empty_trash_impl` so it's unit-testable against
/// hand-built `TrashItem`s without ever calling the real `purge_all` — a test exercising the `None` ("all")
/// branch against the live OS trash would permanently delete whatever the developer/CI runner actually has
/// in their Recycle Bin, which this split avoids entirely.
#[cfg(any(target_os = "windows", target_os = "linux"))]
fn select_trash_targets(all: Vec<trash::TrashItem>, ids: Option<&[String]>) -> Vec<trash::TrashItem> {
    match ids {
        None => all,
        Some(ids) => all
            .into_iter()
            .filter(|item| ids.iter().any(|id| item.id.to_string_lossy() == id.as_str()))
            .collect(),
    }
}

/// The consent gate + purge sequencing for [`empty_trash`], with the two OS calls injected (CPE-1651).
///
/// Injected for the same reason `select_trash_targets` was split out in the first place: a test that
/// exercised the real `list`/`purge_all` would permanently destroy whatever the developer or CI runner
/// actually has in their Recycle Bin. With them as parameters, a test can prove the refusal reaches
/// **neither** — not just that it returned `Err` — which is the whole claim being made. Generic over the
/// item type so that test never has to hand-build a `trash::TrashItem`.
///
/// `list` runs *after* the gate on purpose: an unconfirmed call must be inert, not "inert but it went
/// and enumerated your Recycle Bin first".
#[cfg(any(target_os = "windows", target_os = "linux"))]
fn empty_trash_gated<T>(
    confirmed: bool,
    list: impl FnOnce() -> Result<Vec<T>, String>,
    select: impl FnOnce(Vec<T>) -> Vec<T>,
    purge: impl FnOnce(Vec<T>) -> Result<(), String>,
) -> Result<(), String> {
    // CONFIRM GATE (CPE-1651), mirroring `delete_permanent_impl`'s.
    if !confirmed {
        return Err(
            "refusing to purge: `confirmed` was not set on this empty_trash call — purging is \
             permanent and leaves nothing to restore, so it must be re-invoked with an explicit \
             confirmation (only TrashView's Empty-confirm dialog should ever set it)"
                .to_string(),
        );
    }

    let targets = select(list()?);
    if targets.is_empty() {
        return Ok(());
    }
    purge(targets)
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn empty_trash_impl(ids: Option<Vec<String>>, confirmed: bool) -> Result<(), String> {
    use trash::os_limited::purge_all;

    empty_trash_gated(
        confirmed,
        // CPE-1791: see `restore_from_trash_impl` — routed through the panic-catching wrapper with
        // `.or_fail()`, not a raw `list()` call. A caught panic here must fail the whole purge, never
        // silently look like "nothing to purge" (`empty_trash_gated` treats an empty `targets` as a
        // no-op success — a caught panic must never be allowed to reach that path).
        || list_trash_catching_dependency_panics().or_fail(),
        |all| select_trash_targets(all, ids.as_deref()),
        |targets| purge_all(targets).map_err(|e| e.to_string()),
    )
}

/// Permanently delete entries. Irreversible — there is no Recycle Bin copy and no undo.
///
/// **Refuses the whole batch up front** ([`Err`], nothing touched) when `confirmed` is `false`
/// (CPE-1651, the same shape CPE-1611 gave `secure_shred::shred_paths` and CPE-1630 gave
/// `vault_manager::create_vault`). Every call to this command is unconditionally destructive — unlike
/// `delete_to_trash`, which leaves a recoverable copy — so there is no "safe" plan to distinguish and
/// the flag itself is the entire gate.
///
/// Before this ticket the doc comment here said "the UI must confirm", i.e. the backend delegated the
/// safety decision to the frontend. This flag moves that discipline into Rust, where a call site cannot
/// simply forget it. Three call sites, and only these three, are allowed to set `confirmed: true`:
/// `App.svelte`'s "Delete permanently?" confirm, `RepairLinkDialog.svelte`'s replace-confirm, and
/// `folderWatch.ts`'s `undoFire` (the user pressed Undo, and `plan.deletes` only ever holds copies that
/// fire itself created at freshly-uniqued paths).
///
/// **Be precise about what this defends — it is UI discipline enforced in Rust, NOT an authorization
/// boundary.** The flag rides on the same IPC message as `paths`, so any caller that can forge the call
/// can also set the flag. What it genuinely stops: a frontend call site that forgets the dialog; a
/// replayed pre-CPE-1651 payload (serde gives `confirmed` no default, so the old shape now fails to
/// deserialize outright); and a mechanical enumerator working from `bindings.gen.ts` that doesn't know
/// the field exists. What it does **not** do is stop a deliberate attacker who has already reached the
/// IPC surface. The control that actually breaks the PR #838 exploit chain is CPE-1647's containment
/// re-check inside `vault_lock`, immediately before the wipe — **do not relax that on the strength of
/// this flag.** A real boundary would have to be something the caller cannot mint (backend-issued
/// one-shot consent, or a deny-list on destructive commands); the boolean is kept because it matches
/// `shred_paths` (CPE-1611) and `create_vault` (CPE-1630), and consistency is worth more than a token
/// the caller still supplies.
///
/// **Siblings from the PR #844 review and audit, now gated the same way:** `start_transfer`'s
/// `ConflictPolicy::Overwrite` (CPE-1662), `apply_backup_plan` + its streaming twin (CPE-1664, which also
/// fixed `safe_join` to reject a plan entry naming the root — the correctness half, which holds whatever
/// the caller sets), and `run_command`, whose "the frontend MUST confirm" comment this ticket was filed
/// against (CPE-1665). **Still do not read this as "destruction now requires consent" app-wide:** the
/// auditor also flagged `checkpoint_revert` (`crates/server/src/revert_engine.rs`), which removes every
/// file under a caller-chosen root that is absent from the named manifest — bounded by requiring a
/// pre-existing checkpoint for that exact root, and semantically a restore, so it was rated lower and is
/// **not** gated. And every one of these gates is the same UI discipline described above, not an
/// authorization boundary.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn delete_permanent(
    app: tauri::AppHandle,
    paths: Vec<String>,
    confirmed: bool,
) -> Result<Vec<OpResult>, String> {
    // Same as its sibling `delete_to_trash` — a permanent delete is the user's doing too, not the
    // owning agent session (CPE-1102, fast-follow on CPE-1101). Only worth noting once we're actually
    // about to delete, mirroring `shred_paths` (CPE-1611): a refused call mutates nothing, so it must
    // not leave a phantom entry in the app-op ledger that mis-attributes an unrelated watcher event.
    if confirmed {
        note_app_op(&app, || paths.clone());
    }
    tauri::async_runtime::spawn_blocking(move || delete_permanent_impl(paths, confirmed))
        .await
        .map_err(|e| e.to_string())?
}

/// See [`delete_permanent`]. Split out (and taking `confirmed` itself, rather than being called only
/// once the command has checked it) so the CPE-1651 consent gate is exercised by the same entry point a
/// forged IPC call reaches — a test that checked the flag in the command and the deletion here could
/// pass while the gate sat outside the code under test.
fn delete_permanent_impl(paths: Vec<String>, confirmed: bool) -> Result<Vec<OpResult>, String> {
    // CONFIRM GATE (CPE-1651): checked first, before a single path is even inspected. Refuses cleanly —
    // never a panic, never a partial delete, never a silently-skipped one.
    if !confirmed {
        return Err(
            "refusing to delete: `confirmed` was not set on this delete_permanent call — this is a \
             permanent operation with no Recycle Bin copy and no undo, so it must be re-invoked with \
             an explicit confirmation (only the \"Delete permanently?\" confirm dialog, \
             RepairLinkDialog's replace-confirm, or folderWatch's Undo should ever set it)"
                .to_string(),
        );
    }

    Ok(paths
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
        .collect())
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
///
/// **CPE-1611:** `cpe_server::secure_shred::shred_paths` itself now refuses (returns `Err`, shreds
/// nothing) unless `confirmed` is `true` — this thin dispatcher does nothing special to enforce that;
/// it's the engine's own gate, same treatment CPE-1599 gave `batch_media_execute_stream`. The error
/// propagates straight out to the frontend `Result`. `ShredConfirmDialog.svelte`'s "Shred permanently"
/// button is the one and only call site allowed to pass `confirmed: true`.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn shred_paths(
    app: tauri::AppHandle,
    paths: Vec<String>,
    scheme: ShredScheme,
    confirmed: bool,
) -> Result<Vec<ShredResult>, String> {
    // Same as its siblings `delete_to_trash`/`delete_permanent` — a shred is the user's doing too, not
    // the owning agent session (CPE-1102 pattern). Only worth noting once we're actually about to shred.
    if confirmed {
        note_app_op(&app, || paths.clone());
    }
    tauri::async_runtime::spawn_blocking(move || cpe_server::secure_shred::shred_paths(&paths, scheme, confirmed))
        .await
        .map_err(|e| e.to_string())?
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
    write_copy_into_picked_slot(src, target)
}

/// The write half of [`do_copy_into`], split at the seam [`unique_target`] hands over at (CPE-1765).
///
/// **Extracted so a test can call the real one.** The defect this closes lives *between* the pick and the
/// write, and there is no way to plant something at a picked name mid-call from outside the process
/// without a race — so the reproduction stages the gap by calling this with a `target` that is already a
/// link (or already occupied), which is byte-for-byte the state the production write meets when it loses
/// the race. A test that rebuilt this branch itself would pin `std`'s semantics rather than this app's
/// use of them, which is the same mistake `cpe_server::fsutil::create_exclusive`'s doc records.
///
/// Both arms claim the name atomically (`create_new` / `create_dir`) rather than writing to a name a
/// probe pronounced free; see [`cpe_server::fsutil::copy_file_into_claimed_slot`].
fn write_copy_into_picked_slot(src: &Path, target: PathBuf) -> Result<PathBuf, String> {
    let result = if src.is_dir() {
        copy_dir_all(src, &target)
    } else {
        cpe_server::fsutil::copy_file_into_claimed_slot(src, &target).map(|_| ())
    };
    result.map(|()| target)
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
    let target = write_move_into_picked_slot(src, target)?;
    let _ = cpe_server::tags::retag(ctx, &src.to_string_lossy(), &target.to_string_lossy());
    let _ = cpe_server::snapshot_schedule::reschedule(
        ctx,
        &src.to_string_lossy(),
        &target.to_string_lossy(),
    );
    Ok(target)
}

/// The write half of [`do_move_into`], split at the same seam and for the same reason as
/// [`write_copy_into_picked_slot`] (CPE-1765). The tag/snapshot re-keying stays with the caller so this
/// is purely "get the bytes to the picked name, or say why not".
///
/// **CPE-1710 said this site must NOT use `rename_into_slot`**, because that guard's occupancy half would
/// refuse the very name `unique_target` just chose. That reasoning still holds and this is not a reversal
/// of it: [`cpe_server::fsutil::rename_into_claimed_slot`] does not *ask* whether the name is free, it
/// **takes** it with the same atomic primitive the copy path uses, and only then renames — onto this
/// process's own placeholder. The pre-CPE-1765 code renamed straight onto the picked name, which
/// `fs::rename` replaces silently: the auditor measured it destroying a link that appeared in the gap and
/// reporting success. Now a name taken in the gap is a **refusal that names the path**, and — this is the
/// part a re-probe could never give — the name is genuinely occupied for every other picker while the
/// move runs.
///
/// A `SlotTaken` verdict deliberately does **not** fall through to the copy-then-delete path: the name is
/// not ours, so copying to it is exactly the mistake being fixed. Only a `RenameFailed` (the ordinary
/// cross-volume `EXDEV`) falls through, and by then the placeholder has already been dropped, so the
/// fallback finds the name as it left it and re-claims it atomically.
fn write_move_into_picked_slot(src: &Path, target: PathBuf) -> Result<PathBuf, String> {
    use cpe_server::fsutil::RenameIntoSlot;
    match cpe_server::fsutil::rename_into_claimed_slot(src, &target) {
        RenameIntoSlot::Renamed => return Ok(target),
        RenameIntoSlot::SlotTaken(msg) => return Err(msg),
        RenameIntoSlot::RenameFailed(_) => {}
    }
    // Cross-volume move: copy, then remove the original only if the copy fully succeeded.
    let target = write_copy_into_picked_slot(src, target)?;
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
    /// operations panel already understands (CPE-1184).
    ///
    /// **`skipped` used to be hard-coded to 0, and that was CPE-1775's bug, not a simplification.** The
    /// comment here said archive ops "don't have a per-item conflict policy like copy/move" — true, and
    /// beside the point: they have per-entry *guards*, and a guard's refusal is a skip by any reading.
    /// Zeroing it meant the frontend, which surfaces `errors` only when `failed > 0`, showed a plain
    /// "N items extracted" success toast for an archive whose hostile entry had just been refused, with
    /// N quietly lower than the archive's contents. The engine now counts refusals
    /// (`ArchiveReport::skipped`) and this carries the count through unchanged.
    fn from_archive(id: u64, op: TransferOp, r: cpe_server::archive::ArchiveReport) -> Self {
        TransferReport {
            id,
            op,
            transferred: r.done,
            skipped: r.skipped,
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

/// The CPE-1662 consent gate for a transfer, checked before anything is measured, inspected or touched.
///
/// **Only `ConflictPolicy::Overwrite` is gated**, deliberately. That is the one policy whose collision
/// handling calls `fs::remove_dir_all` / `remove_file` on `dest/<name>` — a caller-named path — inside
/// `resolve_conflict`; `Skip` no-ops on a collision and `Keepboth` writes beside it, so neither destroys
/// anything and neither has anything to ask about. Making every copy/move prompt would train the user to
/// click through the prompt, which is worse than no gate at all.
///
/// `confirmed` is a **separate argument from `policy`** (the CPE-1646 lesson): the policy is what the
/// user chose to do, the flag is that they were actually asked. Deriving one from the other would make
/// the gate self-satisfying and worth nothing. `App.svelte`'s `resolveCopyConflict` /
/// `resolveDropStackCopyConflict` — the two handlers for `TransferConflictDialog`'s buttons — are the
/// only call sites that pass `true`.
///
/// **Be precise about what this defends — it is UI discipline enforced in Rust, NOT an authorization
/// boundary.** The flag arrives on the same IPC message as `sources`/`dest`/`policy`, so a caller able
/// to forge the call can set it too. What it genuinely stops: a call site that reaches an overwriting
/// transfer without the conflict dialog; a replayed pre-CPE-1662 payload (serde gives `confirmed` no
/// default, so the old argument shape now fails to deserialize outright); and a mechanical enumerator
/// working from `bindings.gen.ts` that doesn't know the field exists.
fn require_overwrite_consent(policy: ConflictPolicy, confirmed: bool) -> Result<(), String> {
    if policy == ConflictPolicy::Overwrite && !confirmed {
        return Err(
            "refusing to transfer: `confirmed` was not set on this start_transfer call with the \
             overwrite policy — overwriting removes whatever already sits at each destination path \
             (recursively, for a folder) with no Recycle Bin copy and no undo, so it must be re-invoked \
             with an explicit confirmation (only the copy-conflict dialog's Overwrite choice should \
             ever set it). Skip and keep-both need no confirmation and are unaffected."
                .to_string(),
        );
    }
    Ok(())
}

/// Resolve a collision at `base_target` per `policy`. `Ok(Some(path))` is where to write; `Ok(None)`
/// means skip this source (policy `Skip` with an existing target); `Err` means the collision could not
/// be resolved safely and the item must be reported as failed rather than transferred.
///
/// **`Overwrite` removes the existing entry, so it carries the containment assertion** (PR #855 security
/// audit): `base_target` must not resolve to `dest_dir` itself. `base_target` is `dest_dir.join(name)`
/// with `name` taken straight from `src.file_name()`, and on Windows a name of `" "`, `"..."` or `". "`
/// normalises away entirely — making `base_target` **the destination folder**, so this arm called
/// `remove_dir_all(dest_dir)` and deleted the user's whole target folder. The source did not even have
/// to exist: the destruction happens here, before the copy is attempted, so the returned report carried
/// only "file not found" while the destination was already gone.
///
/// Neither pre-existing guard fired: `is_self_or_descendant` requires `src.is_dir()` (false for a
/// missing source) and `same_path(&base_target, src)` compares two genuinely different paths. Nor did
/// CPE-1662's consent gate narrow it — the *gated* path is the one that reaches it, since dragging an
/// entry named `" "` off a NAS share and clicking **Replace** in the collision dialog is exactly the
/// handler authorised to send `confirmed: true`. Consent to replace *an item* is not consent to lose
/// *the folder*, so this is a correctness check, not a consent one, and it is asserted on the resolved
/// path rather than by pattern-matching names.
fn resolve_conflict(
    base_target: &Path,
    dest_dir: &Path,
    policy: ConflictPolicy,
) -> Result<Option<PathBuf>, String> {
    // CPE-1696: this was `if !base_target.exists()`, the same collapse `unique_target` carried — a stat
    // failure returned the target as free and every policy arm below was skipped, so the caller wrote
    // straight onto a path it could not prove was empty. An unknown now counts as occupied, which routes
    // it into the policy arms rather than past them: `Skip` skips it, `Keepboth` picks a different name
    // via `unique_target`, and `Overwrite` is the one case the user explicitly asked for.
    //
    // CPE-1715: `base_target.try_exists()` alone still followed a dangling link straight through to
    // `Ok(false)` ("free"), so `Skip`/`Keepboth` were bypassed entirely and the caller's rename/copy
    // landed on the link. `probe_name_pick_slot` folds the link check into this same probe, so a link slot
    // — dangling or live — now reads as occupied here too, and only `Overwrite` (which the user chose
    // explicitly) ever touches it.
    if copy_target_is_free(probe_name_pick_slot(base_target)) {
        return Ok(Some(base_target.to_path_buf()));
    }
    match policy {
        ConflictPolicy::Skip => Ok(None),
        ConflictPolicy::Keepboth => {
            let dir = base_target.parent().unwrap_or_else(|| Path::new("."));
            let name = base_target.file_name().and_then(|n| n.to_str()).unwrap_or("file");
            Ok(Some(unique_target(dir, name)))
        }
        ConflictPolicy::Overwrite => {
            // CONTAINMENT (PR #855 audit): overwriting the destination directory itself is never a
            // legitimate transfer outcome. Asserted on the canonicalised paths, so it holds for any
            // spelling — not just the five Win32 ones the audit enumerated.
            //
            // Shares `cpe_server::fsutil::contained_under` with the backup mirror-delete loop so both
            // destructive sites have ONE implementation and ONE failure policy. The first version here
            // was a local `if let (Ok(a), Ok(b)) = (canonicalize(..), canonicalize(..))`, which SKIPPED
            // the assertion whenever either canonicalisation failed and fell straight through to
            // `remove_dir_all` — the destructive call as the default on IO failure, the opposite of the
            // sibling's deliberate fail-closed, and it omitted the `starts_with` half entirely.
            //
            // The precondition holds here: `contained_under` returns `Ok` for a path that doesn't
            // resolve, which is sound only for a target about to be REMOVED — and this arm is now ALSO
            // reached for a slot that does not resolve at all: an Unknown (CPE-1696) or a dangling link
            // (CPE-1715), for which `contained_under` is vacuously `Ok`. That is sound here and only here:
            // a path that resolves to nothing cannot resolve onto `dest_dir` either, and the arm's only
            // action is removal.
            cpe_server::fsutil::contained_under(base_target, dest_dir)
                .map_err(|e| format!("refusing to overwrite: {e}"))?;
            if base_target.is_dir() {
                let _ = fs::remove_dir_all(base_target);
            } else if fs::remove_file(base_target).is_err() {
                // CPE-1715: a dangling *directory* link — the NTFS junction `make_dangling_link` falls
                // back to when `SeCreateSymbolicLinkPrivilege` is absent, which is what an unprivileged
                // Windows CI runner stages — reports `is_dir() == false` (that check follows the link, and
                // nothing resolves), so the branch above is skipped; but `remove_file` then refuses it with
                // `PermissionDenied` (measured directly: os error 5), because a junction is a directory
                // reparse point and Windows will not `DeleteFile` one. `remove_dir` removes the reparse
                // point itself without following it, which is exactly the slot the user authorised
                // replacing, and it is the call that actually succeeds here.
                let _ = fs::remove_dir(base_target);
            }
            Ok(Some(base_target.to_path_buf()))
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
    // CPE-1765: `create_exclusive` (`create_new`), not `File::create`. `File::create` opens
    // create-or-truncate and FOLLOWS a symlink at the final component, so a link planted at this name
    // after `resolve_conflict` picked it sent the user's bytes straight out of the destination folder —
    // measured on PR #924. `create_new` makes the create its own existence check and refuses a link
    // rather than following it. The error is re-worded through `slot_taken_message` so it names the
    // DESTINATION path: `copy_tree_streamed`'s per-item error line names the source, and "the file
    // exists" about a name the app itself invented is unreadable without the path it is about.
    let mut w = cpe_server::fsutil::create_exclusive(dst).map_err(|e| {
        std::io::Error::new(e.kind(), cpe_server::fsutil::slot_taken_message(dst, &e))
    })?;
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
        // CPE-1765: `claim_dir_slot` (`create_dir`), not `create_dir_all`. `create_dir_all` returns `Ok`
        // whenever the path already RESOLVES to a directory — so a directory link (an NTFS junction, a
        // POSIX symlink) planted at this name was accepted and the whole tree was then written through
        // it, outside the folder the user chose. That is reachable in the shipped Overwrite path today:
        // on Unix `resolve_conflict`'s `remove_dir_all` fails on a symlink-to-directory and leaves it
        // standing. `create_dir` never follows the final component and refuses, naming the path.
        if let Err(e) = cpe_server::fsutil::claim_dir_slot(dst) {
            report.failed += 1;
            report.errors.push(e);
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
///
/// `confirmed` is the CPE-1662 consent gate — see [`require_overwrite_consent`]. The engine carries it
/// as well as the command so the gate is exercised by the code that actually reaches `remove_dir_all`,
/// rather than sitting outside the function under test (the reasoning `delete_permanent_impl` records);
/// a refused batch measures nothing, copies nothing and deletes nothing.
// One over clippy's threshold since CPE-1662 added `confirmed`; same treatment as `copy_tree_streamed`
// below. Bundling the arguments into a struct would only move the same list somewhere less readable.
#[allow(clippy::too_many_arguments)]
fn run_transfer(
    id: u64,
    sources: &[PathBuf],
    dest_dir: &Path,
    kind: TransferKind,
    policy: ConflictPolicy,
    confirmed: bool,
    cancel: &std::sync::atomic::AtomicBool,
    mut emit: impl FnMut(&TransferProgress),
) -> TransferReport {
    use std::sync::atomic::Ordering;
    // CONFIRM GATE (CPE-1662): first, before the sources are even measured — an unconsented overwriting
    // transfer must be wholly inert, not "inert but it walked and stat'd your destination first".
    if let Err(e) = require_overwrite_consent(policy, confirmed) {
        return TransferReport {
            id,
            op: TransferOp::from(kind),
            failed: 1,
            errors: vec![e],
            ..Default::default()
        };
    }
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
        // PR #855 audit: a name Windows normalises away (`" "`, `"..."`, `". "`) makes `dest_dir.join(name)`
        // resolve to `dest_dir` ITSELF, which under Overwrite deleted the user's whole destination folder.
        // Refused here, before the target is computed, for every policy and every kind: on Windows such a
        // name can never address the item the user picked, so transferring it is wrong even where it isn't
        // fatal. `resolve_conflict`'s containment assertion is the backstop; this is the cheap filter.
        //
        // **`cfg!(windows)`-scoped, and that scoping is a bug fix** (round-3 review + security audit).
        // `notes.` and `My Report ` are legal, everyday filenames on Linux and macOS, where
        // `dest/notes.` is a real distinct path and nothing is aliased. Unscoped, this filter ran for
        // every kind and every policy, so a plain additive keep-both copy of such a file reported
        // `transferred=0 failed=1` with a message about Windows path normalisation that is simply false
        // on POSIX — a macOS user moving `My Documents ` had the move fail outright. Nothing is lost by
        // scoping it: the audit's mutation test neutralised this filter and every off-disk survival
        // assertion still passed, because `resolve_conflict`'s containment check holds the guarantee on
        // its own.
        if cfg!(windows) && cpe_server::fsutil::win32_name_is_unstable(name) {
            report.failed += 1;
            report.errors.push(format!(
                "{name:?}: name has trailing dots/spaces, which Windows strips — it would resolve to the \
                 destination folder itself rather than to an item inside it"
            ));
            continue;
        }
        if src.is_dir() && is_self_or_descendant(src, dest_dir) {
            report.failed += 1;
            report.errors.push(format!("{name}: can't transfer a folder into itself"));
            continue;
        }
        let base_target = dest_dir.join(name);
        // CPE-1375 — CRITICAL data-loss guard, Overwrite ONLY: pasting a copy into the source's OWN parent
        // makes the computed target the source path itself. Under `Overwrite`, `resolve_conflict` would
        // `remove_file`/`remove_dir_all` the source BEFORE copying from it — permanently destroying the
        // original (a whole folder tree for a directory), and the copy from the now-missing source fails.
        // Overwriting an item with itself is a no-op, so skip it. Scoped to Overwrite deliberately: `Skip`
        // already no-ops via `resolve_conflict` returning `None`, and `Keepboth` MUST fall through here so
        // it reaches `unique_target` and produces the in-place duplicate ("a - Copy.txt") — the whole point
        // of copy→paste-in-the-same-folder. (Move+Overwrite onto self is likewise covered.)
        if policy == ConflictPolicy::Overwrite && same_path(&base_target, src) {
            report.skipped += 1;
            prog.done_bytes += sb;
            prog.done_items += sf;
            emit(&prog);
            continue;
        }
        let target = match resolve_conflict(&base_target, dest_dir, policy) {
            Ok(Some(t)) => t,
            Ok(None) => {
                report.skipped += 1;
                prog.done_bytes += sb;
                prog.done_items += sf;
                emit(&prog);
                continue;
            }
            Err(e) => {
                report.failed += 1;
                report.errors.push(format!("{name}: {e}"));
                continue;
            }
        };
        // Same-volume move: an atomic rename, no byte streaming needed.
        // CPE-1710: `resolve_conflict` above already applied the user's chosen policy to this name, so the
        // paired guard would refuse the target it just authorised. Same fix as `do_move_into`: closed at
        // the source in `resolve_conflict` itself (via `probe_name_pick_slot`), which now treats a link
        // slot — dangling or live — as occupied rather than handing it back as a free target for this
        // rename to land on (CPE-1715).
        //
        // CPE-1765: the rename now goes through `rename_into_claimed_slot`, which CLAIMS the resolved
        // name before renaming onto it, so a name taken between `resolve_conflict` and here is a loud
        // per-item failure instead of a silent replace. `SlotTaken` must not fall through to the copy
        // below — the name is not ours — while `RenameFailed` (cross-volume `EXDEV`) still does, exactly
        // as before, with the placeholder already cleaned up.
        if kind == TransferKind::Move {
            match cpe_server::fsutil::rename_into_claimed_slot(src, &target) {
                cpe_server::fsutil::RenameIntoSlot::Renamed => {
                    report.transferred += 1;
                    prog.done_bytes += sb;
                    prog.done_items += sf;
                    last_emit = prog.done_bytes;
                    emit(&prog);
                    continue;
                }
                cpe_server::fsutil::RenameIntoSlot::SlotTaken(msg) => {
                    report.failed += 1;
                    report.errors.push(format!("{name}: {msg}"));
                    continue;
                }
                cpe_server::fsutil::RenameIntoSlot::RenameFailed(_) => {}
            }
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

/// Check consent, then — only if it passes — issue the transfer id and register its cancel flag
/// (CPE-1662).
///
/// Split out of [`start_transfer`] so the ordering is **testable**, not merely asserted in prose: the
/// PR #855 audit mutation-tested the gate by deleting `require_overwrite_consent(policy, confirmed)?`
/// from the command and the whole Rust suite stayed green, because nothing could reach the command
/// without an `AppHandle`. With the guard and the two side effects in one small function, a test can
/// prove that a refused transfer advances no id and leaves no registry entry. Everything after this in
/// `start_transfer` — the app-op ledger note, the spawned thread — is sequenced by its `?`, so a
/// refusal reaches none of it and no phantom entry appears in the operations panel.
fn begin_transfer(
    policy: ConflictPolicy,
    confirmed: bool,
) -> Result<(u64, std::sync::Arc<std::sync::atomic::AtomicBool>), String> {
    use std::sync::atomic::Ordering;
    // Before the id and the registry entry — a refused transfer must leave nothing behind at all
    // (mirroring `delete_permanent`'s "note only once we're actually about to").
    require_overwrite_consent(policy, confirmed)?;
    let id = TRANSFER_SEQ.fetch_add(1, Ordering::Relaxed);
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    transfer_registry().lock().unwrap().insert(id, cancel.clone());
    Ok((id, cancel))
}

/// Start a copy/move on a background thread, returning its id immediately. Progress is emitted as
/// `transfer://progress` events and the final `TransferReport` as `transfer://done` (CPE-620).
///
/// **CPE-1662:** an `Overwrite` transfer is refused up front — `Err`, no id issued, no registry entry,
/// no ledger note, no thread spawned, so there is no partial state and no phantom entry in the
/// operations panel — unless `confirmed` is `true`. See [`require_overwrite_consent`] for why only that
/// policy is gated, why the flag is a separate argument from `policy`, and exactly what it does and does
/// not defend; [`begin_transfer`] carries the check so that claim is pinned by a test. The check is
/// repeated inside [`run_transfer`] so the engine can't be driven past it by a future caller.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn start_transfer(
    app: tauri::AppHandle,
    sources: Vec<String>,
    dest: String,
    kind: TransferKind,
    policy: ConflictPolicy,
    confirmed: bool,
) -> Result<u64, String> {
    let (id, cancel) = begin_transfer(policy, confirmed)?;
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
        let report = run_transfer(id, &srcs, &dest_dir, kind, policy, confirmed, &cancel, |p| {
            let _ = ctx.emit_json("transfer://progress", serde_json::to_value(p).unwrap_or_default());
        });
        let _ = ctx.emit_json("transfer://done", serde_json::to_value(&report).unwrap_or_default());
        transfer_registry().lock().unwrap().remove(&id);
    });
    Ok(id)
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
///
/// **CPE-1651 audit — deliberately NO `confirmed` flag, unlike its `delete_permanent`/`shred_paths`/
/// `vault_create` siblings.** The PR #838 reviewer noted this command "works equally well" as the
/// exploit chain's step 2, so it was audited alongside `delete_permanent`. The conclusion is that a
/// consent flag is the wrong instrument here, for three reasons:
///
/// 1. **It destroys nothing.** A move is a rename, fully reversible, and the `dst.exists()` refusal
///    below means it can never clobber whatever occupies the destination — pinned by
///    `move_exact_refuses_to_overwrite`, which reads the victim file's bytes back. The three gated
///    commands all annihilate bytes with no recovery path; this one relocates them.
/// 2. **Every caller is a non-destructive, already-reversible flow** — undo/redo, batch-rename apply,
///    `macro_run`'s move step, `FileHealthDialog`'s rename-to-correct-extension fix-it, and
///    `folderWatch`'s undo move-back. None shows (or should show) an irreversible-action confirm, so
///    threading a flag through them could only ever be a hard-coded `true`: a compiler-satisfying
///    constant, which is exactly the anti-pattern the ticket forbids and which would make the flag
///    read as consent while carrying none.
/// 3. **Consent cannot fix what the reviewer actually demonstrated.** Step 2's primitive is *vacating a
///    path* so a link can be planted at it — and a fully consented, entirely legitimate move vacates
///    its source just as effectively. The defence has to live at the point of destruction, not at the
///    move, which is what CPE-1647 landed: `vault_lock` re-resolves containment immediately before the
///    wipe, so a session dir that was moved/deleted and replaced by a junction is refused there.
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

/// Turn a `try_exists()` outcome for the destination's parent folder into the `move_exact` error, or
/// `None` meaning "proceed" (CPE-1692). Split out, mirroring `cpe_server::disk_usage::dir_size_stat_error`,
/// so the `NotFound`-vs-everything-else split is unit-testable without touching a real filesystem
/// (permission bits are platform- and privilege-dependent — inert as root, or defeated by Windows's
/// default "bypass traverse checking" privilege — so an ACL-based test alone would leave this taxonomy
/// unverified on some machines).
///
/// Only a genuine `NotFound` means the folder is actually gone; any other stat failure (permission
/// denied along the resolved path, a dead network mount, …) names the real cause instead of claiming
/// absence — `Path::exists()` used to swallow every `stat` failure into the same `false`.
fn dest_parent_stat_error(stat: std::io::Result<bool>) -> Option<String> {
    match stat {
        Ok(true) => None,
        Ok(false) => Some("The original folder no longer exists".to_string()),
        Err(e) => Some(format!("Could not confirm the destination folder still exists: {e}")),
    }
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
            // **The parent check comes FIRST, and the order is load-bearing (CPE-1705).** When the
            // destination *folder* is what cannot be stat'ed, so is everything named inside it — on Unix
            // a directory without `+x` refuses a stat of every child — so both guards fire and whichever
            // runs first decides the message. Naming the folder is strictly more useful than naming a
            // file inside a folder we cannot see, and CPE-1692's classifier is the one that names the
            // folder. Putting the destination-slot guard first silently demoted it: CI's Unix legs reded
            // with `the classifier's own wrapper text must be present …`, which is that test doing
            // exactly its job. The Windows leg stayed green, because there a deny on the parent does not
            // refuse a child's `try_exists` at all — a reminder that guard ORDER is only observable on
            // one of the three legs.
            if let Some(parent) = dst.parent() {
                if let Some(e) = dest_parent_stat_error(parent.try_exists()) {
                    return OpResult::err(src, e);
                }
            }
            // CPE-1705: was `if dst.exists()`. CPE-1692 hardened the destination *parent* check above but
            // left the destination's OWN check collapsed, so a `dst` whose stat was refused read as a
            // free name and the `fs::rename` below replaced it silently. Same two guards as
            // `rename_entry_impl`, for the same two reasons — a real entry, and a dangling link.
            // CPE-1710: the two calls are now one — `rename_slot_refusal`, same order, same messages.
            match cpe_server::fsutil::rename_into_slot(
                src,
                dst,
                &format!("\"{}\" already exists", dst.file_name().unwrap_or_default().to_string_lossy()),
            ) {
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
/// one-line summary — this IS the dialog's live preview data. Thin dispatcher into `cpe_server::batch_media`.
///
/// **Not pure/in-memory as of CPE-1613/CPE-1623:** `plan()` canonicalizes paths (same-file detection) and
/// stats candidate outputs (real-filesystem collision + containment guards), so — unlike when this
/// dispatcher was first written — it now does genuine blocking I/O. `spawn_blocking` (CPE-760/761's
/// async-command convention) keeps that off the async executor thread, same as `entry_info` below.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn batch_media_plan(
    job: cpe_server::batch_media::BatchJob,
    inputs: Vec<String>,
) -> Result<Vec<cpe_server::batch_media::PlannedItem>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        cpe_server::batch_media::validate(&job)?;
        cpe_server::batch_media::plan(&job, &inputs)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Streamed batch-media execute (CPE-1092): runs a previously computed plan on a blocking thread and
/// sends each file's `OpResult` over `on_result` in batches of 16 as it completes, mirroring
/// `apply_backup_plan_stream` above. `on_result` is a raw transport channel — the Batch-Media dialog
/// renders its own progress, so this is not routed through the busy-cursor wrapper. Returns the aggregate
/// `BatchReport` once every item has run. No cancellation in v1.
///
/// **CPE-1599:** `execute_plan_walk` itself now refuses (returns `Err`, writes nothing, `on_result` never
/// fires) a plan containing an in-place overwrite unless `job.confirmed_overwrite` is set — this thin
/// dispatcher does nothing special to enforce that; it's the engine's own gate. The error propagates
/// straight out to the frontend `Result`, same as any other command failure.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn batch_media_execute_stream(
    items: Vec<cpe_server::batch_media::PlannedItem>,
    job: cpe_server::batch_media::BatchJob,
    on_result: tauri::ipc::Channel<Vec<OpResult>>,
) -> Result<cpe_server::batch_execute::BatchReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut batch: Vec<OpResult> = Vec::new();
        let result = cpe_server::batch_execute::execute_plan_walk(&items, &job, |r| {
            batch.push(r);
            if batch.len() >= 16 {
                let _ = on_result.send(std::mem::take(&mut batch));
            }
        });
        if !batch.is_empty() {
            let _ = on_result.send(batch);
        }
        result
    })
    .await
    .map_err(|e| e.to_string())?
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
///
/// **CPE-1716:** the save goes through [`cpe_server::fsutil::replace_file_contents`], which resolves a
/// symlink at `path` and rewrites the file the link points at. `path` comes straight off IPC and is the
/// user's own media file, so the open-coded `fs::rename` this replaced destroyed a **live** symlink
/// standing there, wrote the edit to the link's former slot instead of the real file, and still returned
/// `Ok` with the edited fields echoed back. Both halves are fixed here: the link survives and the edit
/// reaches the real file, so the fields returned below describe bytes that provably landed — the rename
/// is the last thing that can fail, and its `Err` propagates instead of being reported as a save.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn metadata_write(
    path: String,
    edits: Vec<cpe_server::media_meta_edit::MetaEdit>,
) -> Result<Vec<cpe_server::media_meta_edit::MetaField>, String> {
    tauri::async_runtime::spawn_blocking(move || metadata_write_impl(&path, &edits))
        .await.map_err(|e| e.to_string())?
}

/// The blocking body of [`metadata_write`], split out so the save is reachable from a test without a Tauri
/// runtime (CPE-1716 — the bug it fixes returned `Ok` throughout, so it can only be caught by a test that
/// asserts on the *file*, and that test needs to be able to call this).
///
/// **The link is resolved BEFORE the read, and the resolved path is used for both** (PR #899 UAT round 2).
/// `std::fs::read` follows a symlink too, so reading first meant a dangling link failed here with the OS's
/// bare `The system cannot find the file specified. (os error 2)` — no path, no mention of a link — and
/// `replace_file_contents`'s refusal, which says all of that, could never fire from its only caller.
/// Resolving first also means the bytes read and the bytes written provably concern the same file.
///
/// `ext` stays keyed on the path the **user** opened, matching `metadata_read` / `metadata_writable`: the
/// studio decided which codec to offer from that name, and the save must not silently pick a different one.
fn metadata_write_impl(
    path: &str,
    edits: &[cpe_server::media_meta_edit::MetaEdit],
) -> Result<Vec<cpe_server::media_meta_edit::MetaField>, String> {
    let target = cpe_server::fsutil::resolve_write_target(std::path::Path::new(path))?;
    let bytes = std::fs::read(&target).map_err(|e| e.to_string())?;
    let ext = std::path::Path::new(path)
        .extension().and_then(|e| e.to_str()).unwrap_or("").to_string();
    let out = cpe_server::media_meta::write_back(&ext, &bytes, edits)?;
    cpe_server::fsutil::replace_file_contents(&target, &out)?;
    Ok(cpe_server::media_meta::read_all(&ext, &out))
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
///
/// `confirmed` is the CPE-1664 consent gate; the engine
/// (`cpe_server::backup::apply_backup_plan_walk`) owns it, so this dispatcher does nothing special to
/// enforce it — same treatment CPE-1611 gave `shred_paths`. An unconfirmed call is `Err` with nothing
/// deleted and **no** batch ever pushed down the channel.
// Over clippy's threshold since CPE-1664 added `confirmed`; the argument list is the IPC payload shape
// the frontend sends, so it can't be collapsed without changing the command's public signature.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn apply_backup_plan_stream(
    source_root: String,
    dest_root: String,
    copy: Vec<String>,
    update: Vec<String>,
    delete_paths: Vec<String>,
    verify: bool,
    confirmed: bool,
    on_result: tauri::ipc::Channel<Vec<OpResult>>,
) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut batch: Vec<OpResult> = Vec::new();
        let mut total = 0usize;
        cpe_server::backup::apply_backup_plan_walk(
            &source_root,
            &dest_root,
            &copy,
            &update,
            &delete_paths,
            verify,
            confirmed,
            |r| {
                total += 1;
                batch.push(r);
                if batch.len() >= 16 {
                    let _ = on_result.send(std::mem::take(&mut batch));
                }
            },
        )?;
        if !batch.is_empty() {
            let _ = on_result.send(batch);
        }
        Ok(total)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Execute a backup plan (CPE-797). Model lives in `cpe_server::backup` (CPE-821); thin dispatcher.
///
/// **CPE-1664:** a mirror plan's `delete_paths` are removed outright under `dest_root` — no Recycle Bin
/// copy, no undo — from a root the caller chooses freely, so the engine refuses the whole plan unless
/// `confirmed` is `true`. `BackupDashboard.svelte`'s Run/Restore buttons (and `App.svelte`'s
/// drive-connect scheduler, for a job the user ticked auto-run for) are the only call sites allowed to
/// set it. See `cpe_server::backup::apply_backup_plan_walk` for exactly what the flag does and does not
/// defend — it is UI discipline enforced in Rust, not an authorization boundary.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn apply_backup_plan(
    source_root: String,
    dest_root: String,
    copy: Vec<String>,
    update: Vec<String>,
    delete_paths: Vec<String>,
    verify: bool,
    confirmed: bool,
) -> Result<Vec<OpResult>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        cpe_server::backup::apply_backup_plan(
            &source_root,
            &dest_root,
            &copy,
            &update,
            &delete_paths,
            verify,
            confirmed,
        )
    })
    .await
    .map_err(|e| e.to_string())?
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

// ---- File split/join (CPE-1491) --------------------------------------------------------------------
// The classic orthodox-commander utility: chunk a large file into fixed-size numbered parts plus a small
// manifest, and rejoin them back into the original, verified by SHA-256. Model lives in
// `cpe_server::split_join` (bounded/streamed both ways, reuses the existing streaming SHA-256 approach);
// these are thin `spawn_blocking` dispatchers. GUI dialog + context-menu entries are the follow-up
// CPE-1509 — these two commands are the whole backend surface for now.

/// Split `path` into fixed-`part_size` parts under `out_dir`, plus a manifest recording the original
/// name, size, part count, and whole-file SHA-256 (CPE-1491). Refuses `part_size == 0` and refuses to
/// overwrite a pre-existing manifest/part in `out_dir`.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn split_file(
    path: String,
    part_size: u64,
    out_dir: String,
) -> Result<cpe_server::split_join::SplitManifest, String> {
    tauri::async_runtime::spawn_blocking(move || {
        cpe_server::split_join::split_file(std::path::Path::new(&path), part_size, std::path::Path::new(&out_dir))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Rejoin the parts referenced by `first_part_or_manifest` (the manifest itself, or any one numbered
/// part) into `out_path`, verifying the reconstructed SHA-256 against the manifest (CPE-1491). Refuses to
/// overwrite a pre-existing `out_path`; a missing/short/corrupt part is a clear `Err`, never a panic.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn join_files(first_part_or_manifest: String, out_path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        cpe_server::split_join::join_files(
            std::path::Path::new(&first_part_or_manifest),
            std::path::Path::new(&out_path),
        )
    })
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

/// Register CPE as a Windows "Default apps" *candidate* (CPE-1277, epic CPE-712), then open the Windows
/// Default-apps settings page so the user can confirm the choice. This is HONEST + reversible: modern
/// Windows never lets a program silently force itself as the default, so we only publish the registration
/// (an app Capabilities entry + a folder ProgID under HKCU) and direct the user to Settings → Default apps.
/// Windows-only today; other OSes return an error. Logic in `cpe_server::shell_menu`.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn set_default_file_manager() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?.to_string_lossy().into_owned();
    tauri::async_runtime::spawn_blocking(move || {
        cpe_server::shell_menu::install_default_apps_registration(&exe, "Cross-Platform Explorer")?;
        // Registration only makes CPE selectable — the user confirms the default in Windows Settings.
        open_external_impl("ms-settings:defaultapps".to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Remove the Windows Default-apps registration (CPE-1277) — a complete, idempotent reversal of everything
/// `set_default_file_manager` wrote. Does NOT change any default the user may have chosen (Windows owns
/// that); it only withdraws CPE as a candidate. Logic in `cpe_server::shell_menu`.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn unset_default_file_manager() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(|| {
        cpe_server::shell_menu::uninstall_default_apps_registration("Cross-Platform Explorer")
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Whether CPE is currently registered as a Windows Default-apps candidate (CPE-1277) — best-effort, so the
/// Settings control can reflect true state. Note: this reports *registration*, not whether the user has
/// actually chosen CPE as the default (Windows doesn't expose that reliably). Logic in `cpe_server::shell_menu`.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn default_file_manager_status() -> bool {
    tauri::async_runtime::spawn_blocking(cpe_server::shell_menu::default_apps_registered)
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

// ---- Linux drive-type classification (CPE-1355, epic CPE-716) ------------------------------------
// `classify_drive_type_from_proc` is the pure SAFETY-RELEVANT core: it takes an already-read
// `/proc/mounts` string and a `read_removable` seam (real I/O on Linux, a synthetic map in tests) and
// returns "removable"/"fixed". No real I/O inside it, so it compiles + runs (and is unit-tested) on
// every OS, including this Windows dev box — mirrors how `eject_guard`/`eject_drive_seam` above split
// hardware-dependent logic into a pure, hardware-free core plus a thin OS-specific wrapper.

/// Filesystem types that are pseudo/virtual (never a real removable/fixed block device) — a match on
/// `/proc/mounts`' 3rd field skips these mounts outright so they never get classified as "removable".
/// `cfg`-gated to Linux (the only real caller) + test (so the pure-fn unit tests run on every OS,
/// including this Windows dev box) — otherwise a non-Linux release build sees it as dead code.
#[cfg(any(target_os = "linux", test))]
fn is_virtual_fstype(fstype: &str) -> bool {
    matches!(
        fstype,
        "tmpfs"
            | "proc"
            | "sysfs"
            | "devtmpfs"
            | "devpts"
            | "overlay"
            | "cgroup"
            | "cgroup2"
            | "squashfs"
            | "ramfs"
            | "debugfs"
            | "tracefs"
            | "securityfs"
            | "pstore"
            | "mqueue"
            | "hugetlbfs"
            | "autofs"
            | "configfs"
            | "fusectl"
            | "binfmt_misc"
            | "nsfs"
    )
}

/// Reduce a partition device name (already stripped of the `/dev/` prefix, e.g. `sdb1`, `nvme0n1p1`,
/// `mmcblk0p1`) to its parent block device (`sdb`, `nvme0n1`, `mmcblk0`) for the `/sys/block/<dev>`
/// lookup. `nvme`/`mmcblk` devices name their partitions with an explicit `pN` suffix (`nvme0n1p1`,
/// `mmcblk0p1`), so ONLY that suffix is stripped for them — an unpartitioned whole-disk name like
/// `nvme0n1` or `mmcblk0` has no `pN` and must be returned unchanged; a naive trailing-digit trim would
/// wrongly cut the trailing digit off the base name itself (`nvme0n1` → `nvme0n`, `mmcblk0` → `mmcblk`),
/// pointing the `/sys/block/<dev>/removable` lookup at a device that doesn't exist (CPE-1355 review fix).
/// `sd`/`hd`/`vd`-style devices have no such ambiguity — their partition number IS a plain trailing-digit
/// suffix (`sdb1` → `sdb`), so those keep the trim.
#[cfg(any(target_os = "linux", test))]
fn parent_block_device(dev: &str) -> String {
    if dev.starts_with("nvme") || dev.starts_with("mmcblk") {
        // Partition form is "...p<N>"; strip ONLY that. A whole disk (no "pN") is returned unchanged.
        if let Some(pos) = dev.rfind('p') {
            let tail = &dev[pos + 1..];
            if !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()) {
                return dev[..pos].to_string();
            }
        }
        return dev.to_string();
    }
    // sd/hd/vd/etc: the partition number is a plain trailing-digit suffix.
    dev.trim_end_matches(|c: char| c.is_ascii_digit()).to_string()
}

/// The pure classifier core (CPE-1355). Given `path`, the contents of `/proc/mounts`, and a
/// `read_removable` seam that returns the `removable` sysfs flag for a base block device name (or
/// `None` if it can't be read), decide `"removable"` vs `"fixed"`:
/// - Find the mount entry whose mount point is the LONGEST prefix of `path` (the containing mount),
///   skipping pseudo/virtual filesystems and any non-`/dev/*` device.
/// - Reduce that mount's partition device to its parent block device and consult `read_removable`.
/// - `"1"` → `"removable"`; anything else, or any failure along the way (no matching mount, unreadable
///   flag, malformed input) → `"fixed"`. Never panics.
#[cfg(any(target_os = "linux", test))]
fn classify_drive_type_from_proc(
    path: &str,
    mounts: &str,
    read_removable: impl Fn(&str) -> Option<String>,
) -> String {
    let mut best: Option<(&str, &str)> = None; // (mount_point, device)
    for line in mounts.lines() {
        let mut fields = line.split_whitespace();
        let device = match fields.next() {
            Some(d) => d,
            None => continue,
        };
        let mount_point = match fields.next() {
            Some(m) => m,
            None => continue,
        };
        let fstype = fields.next().unwrap_or("");
        if !device.starts_with("/dev/") || is_virtual_fstype(fstype) {
            continue; // pseudo/virtual fs (tmpfs, proc, sysfs, overlay, snap squashfs, ...) never removable
        }
        let is_prefix_mount = mount_point == "/"
            || path == mount_point
            || path.starts_with(&format!("{mount_point}/"));
        if !is_prefix_mount {
            continue;
        }
        if best.map_or(true, |(bm, _)| mount_point.len() > bm.len()) {
            best = Some((mount_point, device));
        }
    }

    let Some((_, device)) = best else {
        return "fixed".to_string();
    };
    let dev_name = device.trim_start_matches("/dev/");
    if dev_name.is_empty() {
        return "fixed".to_string();
    }
    let base = parent_block_device(dev_name);
    match read_removable(&base) {
        Some(flag) if flag.trim() == "1" => "removable".to_string(),
        _ => "fixed".to_string(),
    }
}

/// Real Linux wrapper (CPE-1355): reads `/proc/mounts` and, for the matched device, `/sys/block/<dev>/removable`,
/// then delegates all decision logic to the pure `classify_drive_type_from_proc`. Any I/O failure surfaces
/// as `None`/missing input to the pure fn, which already falls back to `"fixed"` — never panics. This
/// wrapper does real filesystem I/O so it can only be *compiled* on Linux (CI's ubuntu leg) and cannot be
/// exercised on this Windows dev box; the decision logic it delegates to is fully unit-tested above.
#[cfg(target_os = "linux")]
fn drive_type_impl(path: &str) -> String {
    let mounts = match std::fs::read_to_string("/proc/mounts") {
        Ok(s) => s,
        Err(_) => return "fixed".to_string(),
    };
    classify_drive_type_from_proc(path, &mounts, |dev| {
        std::fs::read_to_string(format!("/sys/block/{dev}/removable"))
            .ok()
            .map(|s| s.trim().to_string())
    })
}

#[cfg(all(unix, not(target_os = "linux")))]
fn drive_type_impl(_path: &str) -> String {
    // Best-effort until a macOS/BSD classifier lands (a follow-up); Linux gets real classification above.
    "fixed".to_string()
}

// ---- Safe drive eject / remove (CPE-1278, epic CPE-716) ------------------------------------------
// SAFETY-CRITICAL. Never eject a fixed/system, network, optical, or RAM drive — ONLY a removable
// volume. The guard (`eject_guard`) is a pure function, unit-tested exhaustively; `eject_drive_seam`
// injects both the classifier and the eject syscall so a test can PROVE the syscall is never reached
// for a non-removable drive, without any real hardware. `eject_drive_impl` wires the real
// `drive_type_impl` classifier + the platform `perform_eject` into that seam.

/// Whether a path *looks* ejectable to the UI (CPE-1278) — true only for a `removable` drive, so the
/// sidebar shows an eject affordance on those rows alone. This never itself ejects anything.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn drive_ejectable(path: String) -> bool {
    tauri::async_runtime::spawn_blocking(move || eject_guard(&path, &drive_type_impl(&path)).is_ok())
        .await
        .unwrap_or(false)
}

/// Safely eject / remove the removable drive that owns `path` (CPE-1278, epic CPE-716). SAFETY-CRITICAL:
/// refuses anything that is not a *removable* volume — a fixed/system, network, CD-ROM, RAM, or unknown
/// drive is rejected with a clear error and NO eject syscall is issued. Only after the guard passes does
/// Windows run the safe-remove sequence on the volume handle (`\\.\X:`): `FSCTL_LOCK_VOLUME` →
/// `FSCTL_DISMOUNT_VOLUME` → `IOCTL_STORAGE_EJECT_MEDIA`, unlocking on any failure so an in-use volume
/// ("files open") is left mounted and usable. Non-Windows is not supported yet and returns an honest error.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn eject_drive(path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || eject_drive_impl(&path))
        .await
        .map_err(|e| e.to_string())?
}

/// Normalise any path to its drive-letter root ("X:\\") when it has one; otherwise pass it through.
fn drive_root_of(path: &str) -> String {
    if path.len() >= 2 && path.as_bytes()[1] == b':' {
        format!("{}:\\", &path[..1])
    } else {
        path.to_string()
    }
}

/// The SAFETY CORE (CPE-1278): given a drive's classification (from `drive_type_impl`), decide whether a
/// safe-eject may proceed. ONLY `removable` is ejectable; every other kind — `fixed` (the system disk),
/// `network`, `cdrom`, `ram`, `unknown`, or anything unexpected — is refused with a user-facing message.
/// Pure + hardware-free, so it is exhaustively unit-tested.
fn eject_guard(root: &str, kind: &str) -> Result<(), String> {
    match kind {
        "removable" => Ok(()),
        "fixed" => Err(format!(
            "{root} is a fixed/system drive and can never be ejected — only removable drives can be safely removed."
        )),
        "network" => Err(format!("{root} is a network drive; disconnect it rather than eject it.")),
        "cdrom" => Err(format!("{root} is an optical drive, not a removable USB volume.")),
        other => Err(format!("{root} ({other}) is not a removable drive and cannot be ejected.")),
    }
}

/// Guard-then-perform with the classifier and the eject syscall injected as SEAMS. The guard runs BEFORE
/// `do_eject` can, so a non-removable drive can never reach the eject path — a fact the unit tests assert
/// directly without touching hardware. `eject_drive_impl` supplies the real classifier + platform eject.
fn eject_drive_seam(
    path: &str,
    classify: impl Fn(&str) -> String,
    do_eject: impl FnOnce(&str) -> Result<(), String>,
) -> Result<(), String> {
    let root = drive_root_of(path);
    let kind = classify(&root);
    eject_guard(&root, &kind)?; // refuses non-removable BEFORE any eject syscall can run
    do_eject(&root)
}

fn eject_drive_impl(path: &str) -> Result<(), String> {
    eject_drive_seam(path, drive_type_impl, perform_eject)
}

/// The real Windows safe-remove sequence on the volume handle. Called ONLY after `eject_guard` has
/// confirmed a removable drive. Never panics; unlocks + closes the handle on every exit path.
#[cfg(windows)]
fn perform_eject(root: &str) -> Result<(), String> {
    use windows::core::HSTRING;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows::Win32::System::Ioctl::{
        FSCTL_DISMOUNT_VOLUME, FSCTL_LOCK_VOLUME, FSCTL_UNLOCK_VOLUME, IOCTL_STORAGE_EJECT_MEDIA,
    };
    use windows::Win32::System::IO::DeviceIoControl;

    // GENERIC_READ | GENERIC_WRITE — lock/dismount/eject all need write access to the volume.
    const ACCESS: u32 = 0x8000_0000 | 0x4000_0000;

    let letter = root.chars().next().ok_or_else(|| "invalid drive root".to_string())?;
    let device = format!(r"\\.\{letter}:");
    let wide = HSTRING::from(device);

    // SAFETY: `wide` is a valid NUL-terminated wide string; a standard volume-handle open for
    // lock/dismount/eject. CreateFileW returns Err on the invalid-handle sentinel, so success ⇒ a real
    // handle, which we CloseHandle on every path below.
    let handle: HANDLE = unsafe {
        CreateFileW(
            &wide,
            ACCESS,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            None,
        )
    }
    .map_err(|e| format!("Couldn't open the drive to eject it: {e}"))?;

    let mut bytes: u32 = 0;
    // SAFETY: `handle` is a valid open volume handle; all buffers are None (these control codes take no
    // data) and `bytes` outlives every call.
    let mut ioctl = |code: u32| -> windows::core::Result<()> {
        unsafe { DeviceIoControl(handle, code, None, 0, None, 0, Some(&mut bytes as *mut u32), None) }
    };

    // 1) Lock — fails if files/handles are open on the volume ("drive in use").
    if let Err(e) = ioctl(FSCTL_LOCK_VOLUME) {
        unsafe {
            let _ = CloseHandle(handle);
        }
        return Err(format!(
            "The drive is in use — close any open files or windows on it, then try again. ({e})"
        ));
    }
    // 2) Dismount so the filesystem is flushed and detached.
    if let Err(e) = ioctl(FSCTL_DISMOUNT_VOLUME) {
        let _ = ioctl(FSCTL_UNLOCK_VOLUME);
        unsafe {
            let _ = CloseHandle(handle);
        }
        return Err(format!("Couldn't dismount the drive: {e}"));
    }
    // 3) Eject the media / spin the device down.
    let ejected = ioctl(IOCTL_STORAGE_EJECT_MEDIA);
    let _ = ioctl(FSCTL_UNLOCK_VOLUME);
    unsafe {
        let _ = CloseHandle(handle);
    }
    ejected.map_err(|e| format!("The drive was dismounted but couldn't be ejected: {e}"))?;
    Ok(())
}

#[cfg(not(windows))]
fn perform_eject(_root: &str) -> Result<(), String> {
    Err("Safe drive eject isn't supported on this platform yet.".to_string())
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

/// Record that a best-effort pre-write checkpoint of `root` was attempted and failed (CPE-1600), so the
/// Checkpoints panel can show it as a durable record alongside checkpoints that did succeed. Called by
/// every "checkpoint before an irreversible batch" caller (Batch Media, Metadata Studio, Declutter,
/// Similar Images) from its existing failure `catch`, in addition to the `console.error` it already logs
/// — never in place of it, and never blocking the write that's already proceeding.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn checkpoint_record_failure(
    app: tauri::AppHandle,
    root: String,
    operation: String,
    reason: String,
) -> Result<(), String> {
    let ctx = server_ctx::TauriCtx::new(&app);
    tauri::async_runtime::spawn_blocking(move || {
        cpe_server::checkpoint_store::record_checkpoint_failure(&ctx, &root, &operation, &reason)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The failed checkpoint attempts recorded for `root`, newest-first (CPE-1600). Kept as a separate list
/// from `checkpoint_list` — the frontend renders the two shapes distinctly rather than merging them here.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn checkpoint_failures_list(
    app: tauri::AppHandle,
    root: String,
) -> Result<Vec<cpe_server::checkpoint_store::CheckpointFailure>, String> {
    let ctx = server_ctx::TauriCtx::new(&app);
    tauri::async_runtime::spawn_blocking(move || {
        cpe_server::checkpoint_store::checkpoint_failures_list(&ctx, &root)
    })
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

/// Per-pixel diff of two images — the headless engine for the compare studio's deferred image-compare
/// mode (CPE-1490, epic CPE-722; GUI pane is the separate follow-up CPE-1508). Decodes both through the
/// thumbnail pipeline's bomb-guarded decoder and returns a grayscale diff-mask PNG plus summary stats
/// (`percentDifferent`, changed-pixel count, bounding box). Differing dimensions and non-image/oversized
/// sources are reported as documented in `cpe_server::image_diff`, never a panic.
/// Model lives in `cpe_server::image_diff` (bounded, no new dependency); this is a thin `spawn_blocking`
/// dispatcher.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn diff_images(a: String, b: String) -> Result<cpe_server::image_diff::ImageDiff, String> {
    tauri::async_runtime::spawn_blocking(move || {
        cpe_server::image_diff::diff_images(Path::new(&a), Path::new(&b))
    })
    .await
    .map_err(|e| e.to_string())?
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

// ---- File-content search / content index (CPE-1262, epic CPE-976) ---------------------------------
// Thin dispatchers over `cpe_server::content_index`: walk a folder's text-like files into a local,
// dependency-free `FakeEmbedder`-backed `SemanticIndex` (no network, no API key), persist it under
// app-data keyed by a stable hash of the indexed root, and search it. Honestly file-CONTENT search
// (embedder-pluggable), not oversold "semantic" — see `cpe_server::content_index`'s module docs. No
// managed state: each command loads/saves the persisted index directly on a blocking thread — unlike
// `index_service`'s whole-machine instant index, a per-folder content index is small enough that keeping
// it resident between calls buys nothing.

/// Resolve the content-index directory under app-data (`<app_data>/content-index`), sibling of `index`.
fn content_index_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = server_ctx::TauriCtx::new(app).app_data_dir()?.join("content-index");
    Ok(dir)
}

// ---- Configurable real embedder for content search (CPE-1273, epic CPE-976) -----------------------
// Content search (CPE-1262/1263) defaults to the local, dependency-free FakeEmbedder (no key, no
// network). This lets the user opt into a REAL, OpenAI-compatible embeddings endpoint (a local LM Studio /
// Ollama server, or OpenAI/others with a key) via the "AI content search" Settings section. The
// enabled/base-URL/model config is persisted by the frontend (settings.ts) and passed to
// `content_index_build`/`content_search`; the API KEY is stored ONLY in the OS keychain (never in
// settings.json), fetched here through the same `SecretAccess` seam the vault work uses (KeyringBackend).
// The embedder SELECTION + the HttpEmbedder live in `cpe_server` (Tauri-free); these commands are thin.

/// Keychain service/account the content-search embedder API key is stored under. A single global key
/// (there's one configured endpoint), unlike the per-vault passphrase accounts.
const EMBEDDER_KEY_SERVICE: &str = "cpe.content-embedder";
const EMBEDDER_KEY_ACCOUNT: &str = "api-key";

/// Fetch the saved content-embedder API key from the OS keychain, or `None`. Best-effort: a keychain
/// error degrades to `None` (treated as "no key" — a local server needs none) rather than failing the
/// whole build/search. The value is never logged.
#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
fn content_embedder_key() -> Option<String> {
    use cpe_server::vault_manager::SecretAccess;
    KeyringBackend
        .get(EMBEDDER_KEY_SERVICE, EMBEDDER_KEY_ACCOUNT)
        .ok()
        .flatten()
        .filter(|k| !k.is_empty())
}
#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
fn content_embedder_key() -> Option<String> {
    None
}

/// Registry of in-flight `content_index_build` cancel flags, keyed by [`cpe_server::content_index::root_key`],
/// so a re-issued build for the same root cancels the prior one (mirrors `DIR_STREAM_CANCELS`/
/// `INDEX_BUILD_CANCELS`).
static CONTENT_INDEX_BUILD_CANCELS: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<u64, std::sync::Arc<std::sync::atomic::AtomicBool>>>,
> = std::sync::OnceLock::new();

fn content_index_build_registry(
) -> &'static std::sync::Mutex<std::collections::HashMap<u64, std::sync::Arc<std::sync::atomic::AtomicBool>>>
{
    CONTENT_INDEX_BUILD_CANCELS.get_or_init(Default::default)
}

/// Walk `root`'s text-like files into a fresh content index, streaming `ContentIndexProgress` over
/// `on_progress` as the walk advances (per `docs/design/STREAMING.md`), then persist it under app-data
/// keyed by `root`. Re-issuing a build for the same `root` cancels any prior build for it. Returns the
/// final `ContentIndexBuildStats`. Async + `spawn_blocking` — the walk/chunk/embed is CPU+IO work and must
/// never run on the UI thread (CPE-760).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn content_index_build(
    app: tauri::AppHandle,
    root: String,
    on_progress: tauri::ipc::Channel<cpe_server::content_index::ContentIndexProgress>,
    embedder: Option<cpe_server::content_index::ContentEmbedderConfig>,
) -> Result<cpe_server::content_index::ContentIndexBuildStats, String> {
    use std::sync::atomic::Ordering;
    let dir = content_index_dir(&app)?;
    let key = cpe_server::content_index::root_key(&root);

    // Cancel any build already running for this root, then register our own flag.
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let mut reg = content_index_build_registry().lock().unwrap();
        if let Some(prev) = reg.get(&key) {
            prev.store(true, Ordering::Relaxed);
        }
        reg.insert(key, cancel.clone());
    }
    let cancel_id = cancel.clone();

    // Fetch the API key from the keychain (only used when an enabled HTTP embedder is configured; a
    // local server needs none). `build_and_save` picks the embedder from `embedder` + this key, builds,
    // and persists to the identity-keyed path — or the local untagged path when unconfigured (CPE-1273).
    let api_key = content_embedder_key();
    let result = tauri::async_runtime::spawn_blocking(move || {
        cpe_server::content_index::build_and_save(
            &root,
            &dir,
            &cancel,
            |p| {
                let _ = on_progress.send(p);
            },
            embedder.as_ref(),
            api_key,
        )
    })
    .await
    .map_err(|e| e.to_string())?;

    // Deregister — but only if a newer build hasn't already replaced our flag.
    {
        let mut reg = content_index_build_registry().lock().unwrap();
        if reg.get(&key).is_some_and(|f| std::sync::Arc::ptr_eq(f, &cancel_id)) {
            reg.remove(&key);
        }
    }
    result
}

/// Search `root`'s persisted content index for `query`, returning up to `k` ranked hits (path, score,
/// snippet). No index built yet for `root` → an empty, `index_exists: false` outcome — a clean "needs
/// build" signal, not an error/crash. Async + `spawn_blocking` (loading + scoring the index is blocking
/// IO/CPU work).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn content_search(
    app: tauri::AppHandle,
    root: String,
    query: String,
    k: usize,
    embedder: Option<cpe_server::content_index::ContentEmbedderConfig>,
) -> Result<cpe_server::content_index::ContentSearchOutcome, String> {
    let dir = content_index_dir(&app)?;
    let api_key = content_embedder_key();
    tauri::async_runtime::spawn_blocking(move || {
        cpe_server::content_index::search_configured(&dir, &root, &query, k, embedder.as_ref(), api_key)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Save (or clear) the content-search embedder API key in the OS keychain — the ONLY place it persists,
/// never settings.json, never a log (CPE-1273). An empty/blank `key` deletes the stored key (so a local
/// server that needs none leaves nothing behind). Async + `spawn_blocking`: the credential store is a
/// blocking OS call (CPE-760/761). The key value is never echoed back.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn content_embedder_set_key(key: String) -> Result<(), String> {
    #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
    {
        tauri::async_runtime::spawn_blocking(move || {
            use cpe_server::vault_manager::SecretAccess;
            if key.trim().is_empty() {
                KeyringBackend.delete(EMBEDDER_KEY_SERVICE, EMBEDDER_KEY_ACCOUNT)
            } else {
                KeyringBackend.set(EMBEDDER_KEY_SERVICE, EMBEDDER_KEY_ACCOUNT, &key)
            }
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        let _ = key;
        Err("no OS keychain available on this platform".to_string())
    }
}

/// Whether a content-search embedder API key is saved in the OS keychain — lets Settings show "key
/// saved" without ever materialising (or transmitting) the value. Async + `spawn_blocking` (blocking OS
/// credential-store call).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn content_embedder_has_key() -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(content_embedder_key)
        .await
        .map(|k| k.is_some())
        .map_err(|e| e.to_string())
}

/// Test an embeddings endpoint (the Settings "Test connection" button): embed a short probe string and
/// report the detected vector dimensionality, or a clear error (unreachable, bad key, bad response) —
/// never a panic (CPE-1273). Uses the key currently saved in the keychain, so the user can Save then Test.
/// Ignores the `enabled` flag (you test before turning it on). Async + `spawn_blocking` (network I/O).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn content_embedder_test(
    config: cpe_server::content_index::ContentEmbedderConfig,
) -> Result<usize, String> {
    let api_key = content_embedder_key();
    tauri::async_runtime::spawn_blocking(move || {
        cpe_server::content_index::probe_embedder(&config.base_url, &config.model, api_key)
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---- AI file copilot: LLM planner → plan/execute + undo (CPE-1275, epic CPE-977) ------------------
// SAFETY-CRITICAL: an LLM proposes file operations. Everything the copilot does is whitelisted by
// construction (`cpe_server::op_plan::FileOp` is a closed set — no shell/free-form op exists) and passes
// the same safety chain the domain layer owns: validate (scope root + op cap) → human confirm (the UI's
// job) → RE-VALIDATE at execute → checkpoint FIRST (one-click undo) → apply → deletes to the OS TRASH
// (recoverable). The planner + plan/execute glue live in `cpe_server::copilot{,_planner}` (Tauri-free);
// these commands are thin `spawn_blocking` dispatchers. The API key is stored ONLY in the OS keychain
// (service `cpe.copilot`, distinct from the content-embedder key), fetched through the same `SecretAccess`
// seam the vault + embedder work use — never in settings.json, never logged, never returned.

/// Keychain service/account the AI-copilot model API key is stored under. Distinct from
/// [`EMBEDDER_KEY_SERVICE`] so the two endpoints keep independent keys.
const COPILOT_KEY_SERVICE: &str = "cpe.copilot";
const COPILOT_KEY_ACCOUNT: &str = "api-key";

/// Fetch the saved copilot API key from the OS keychain, or `None`. Best-effort: a keychain error degrades
/// to `None` (treated as "no key" — a local server needs none). The value is never logged.
#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
fn copilot_key() -> Option<String> {
    use cpe_server::vault_manager::SecretAccess;
    KeyringBackend
        .get(COPILOT_KEY_SERVICE, COPILOT_KEY_ACCOUNT)
        .ok()
        .flatten()
        .filter(|k| !k.is_empty())
}
#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
fn copilot_key() -> Option<String> {
    None
}

/// The app's [`cpe_server::copilot::TrashBin`] impl — routes a copilot `Delete` op to the OS recycle bin /
/// trash via the same `trash` crate `delete_to_trash` uses, so a copilot delete is recoverable (never a
/// hard delete). Keeps `cpe-server` free of the `trash` crate, exactly as `KeyringBackend` keeps it free of
/// `keyring`.
struct TauriTrash;
impl cpe_server::copilot::TrashBin for TauriTrash {
    fn trash(&self, path: &str) -> Result<(), String> {
        trash::delete(path).map_err(|e| format!("could not move {path} to the trash: {e}"))
    }
}

/// Build a SAFE, validated file-operation plan for `instruction` over `root` (CPE-1275). Lists the folder,
/// asks the configured OpenAI-compatible model for a whitelisted [`cpe_server::op_plan::FileOpPlan`], then
/// validates it against the scope+cap envelope — returning the plan, a dry-run summary, and ALL violations
/// (non-empty ⇒ the UI must NOT offer execute). NO filesystem change. Model unreachable / bad output → a
/// clear `Err`; an unsafe-but-produced plan → a non-empty `violations`. The API key comes from the keychain
/// (a local server needs none). Async + `spawn_blocking` (network + a directory listing).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn copilot_plan(
    root: String,
    instruction: String,
    config: cpe_server::copilot::CopilotConfig,
) -> Result<cpe_server::copilot::CopilotPlanResult, String> {
    cpe_server::fs_route::require_local(&root)?;
    let api_key = copilot_key();
    tauri::async_runtime::spawn_blocking(move || {
        let planner = cpe_server::copilot::resolve_planner(&config, api_key)?;
        cpe_server::copilot::plan_with(planner.as_ref(), &root, &instruction)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Execute a human-confirmed copilot `plan` against `root` (CPE-1275). RE-VALIDATES the plan against the
/// same envelope (never trusts a stale/tampered plan from the frontend) — if it no longer validates,
/// nothing runs and no checkpoint is taken. Otherwise a checkpoint is captured FIRST (one-click undo for
/// the whole plan), then the whitelisted ops are applied (skip-on-error), with deletes routed to the OS
/// trash (recoverable). Returns per-op results + the checkpoint/undo handle. Async + `spawn_blocking`
/// (filesystem I/O + snapshot capture).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn copilot_execute(
    app: tauri::AppHandle,
    root: String,
    plan: cpe_server::op_plan::FileOpPlan,
) -> Result<cpe_server::copilot::CopilotExecuteResult, String> {
    cpe_server::fs_route::require_local(&root)?;
    let ctx = server_ctx::TauriCtx::new(&app);
    tauri::async_runtime::spawn_blocking(move || {
        cpe_server::copilot::execute_with(&ctx, &TauriTrash, &root, &plan)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Save (or clear) the AI-copilot model API key in the OS keychain — the ONLY place it persists, never
/// settings.json, never a log (CPE-1275, mirrors `content_embedder_set_key`). An empty/blank `key` deletes
/// the stored key (a local server needs none). Async + `spawn_blocking` (blocking credential-store call).
/// The key value is never echoed back.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn copilot_set_key(key: String) -> Result<(), String> {
    #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
    {
        tauri::async_runtime::spawn_blocking(move || {
            use cpe_server::vault_manager::SecretAccess;
            if key.trim().is_empty() {
                KeyringBackend.delete(COPILOT_KEY_SERVICE, COPILOT_KEY_ACCOUNT)
            } else {
                KeyringBackend.set(COPILOT_KEY_SERVICE, COPILOT_KEY_ACCOUNT, &key)
            }
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        let _ = key;
        Err("no OS keychain available on this platform".to_string())
    }
}

/// Whether an AI-copilot API key is saved in the OS keychain — lets Settings show "key saved" without ever
/// materialising (or transmitting) the value. Async + `spawn_blocking` (blocking credential-store call).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn copilot_has_key() -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(copilot_key)
        .await
        .map(|k| k.is_some())
        .map_err(|e| e.to_string())
}

/// Test a model endpoint (the Settings "Test connection" button): send a trivial planning request and
/// confirm a parseable plan comes back, or a clear error (unreachable, bad key, bad response) — never a
/// panic (CPE-1275). Uses the key currently saved in the keychain, so the user can Save then Test. Ignores
/// the `enabled` flag (you test before turning it on). Async + `spawn_blocking` (network I/O).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn copilot_test(config: cpe_server::copilot::CopilotConfig) -> Result<(), String> {
    let api_key = copilot_key();
    tauri::async_runtime::spawn_blocking(move || {
        cpe_server::copilot::probe_planner(&config.base_url, &config.model, api_key)
    })
    .await
    .map_err(|e| e.to_string())?
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

// ---- Safety-scan command integration (CPE-1287, epic CPE-1002) ------------------------------------
// Thin `spawn_blocking` dispatchers into the Shift-1 `cpe-server` scan adapters (CPE-1281–1285). Each
// adapter module owns its own walk/caps/skip-unreadable discipline; these commands add nothing but the
// String->Path conversion and the async boundary, mirroring `find_similar_folders` above.

/// Score a ZIP archive at `path` for zip-bomb-like expansion ratio (CPE-1281, epic CPE-1002). Thin
/// `spawn_blocking` dispatcher into [`cpe_server::archive_safety_scan::analyze_archive_safety`], which uses
/// [`cpe_server::archive_safety::RatioLimits::default`] and never errors on its own — a corrupt, missing, or
/// non-ZIP `path` yields a graceful zero-entry, non-dangerous report rather than an `Err`.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn analyze_archive_safety(
    path: String,
) -> Result<cpe_server::archive_safety_scan::ArchiveSafetyReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        cpe_server::archive_safety_scan::analyze_archive_safety(std::path::Path::new(&path))
    })
    .await
    .map_err(|e| e.to_string())
}

/// Find the topmost cascade-empty directories under `root` (CPE-1282, epic CPE-1002) — a directory that is
/// empty or contains only nested empty directories. Thin `spawn_blocking` dispatcher into
/// [`cpe_server::empty_dirs_scan::find_empty_dirs`], which never errors — an unreadable/non-existent `root`
/// yields an empty, non-truncated report. `excludes` are glob patterns pruning matching sub-directories
/// (CPE-1302); an empty list scans the whole tree.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn find_empty_dirs(
    root: String,
    excludes: Vec<String>,
) -> Result<cpe_server::empty_dirs_scan::EmptyDirsReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        cpe_server::empty_dirs_scan::find_empty_dirs(std::path::Path::new(&root), &excludes)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Find orphaned sidecar files (e.g. a `.srt`/`.xmp` with no matching primary) under `root` (CPE-1283, epic
/// CPE-1002), using [`cpe_server::orphan_sidecars_scan`]'s default rules. `recursive` controls whether
/// subdirectories are walked (each directory's sidecars are only ever paired against primaries in that same
/// directory). Thin `spawn_blocking` dispatcher; never errors. `excludes` are glob patterns pruning
/// matching sub-directories when `recursive` (CPE-1302); an empty list scans the whole tree.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn find_orphan_sidecars(
    root: String,
    recursive: bool,
    excludes: Vec<String>,
) -> Result<cpe_server::orphan_sidecars_scan::OrphanSidecarResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        cpe_server::orphan_sidecars_scan::find_orphan_sidecars(std::path::Path::new(&root), recursive, &excludes)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Registry of in-flight `find_orphan_sidecars_stream` walks' cancel flags, keyed by the frontend-supplied
/// `stream_id`, mirroring `DIR_STREAM_CANCELS` (CPE-1299, STREAMING.md).
static ORPHAN_SIDECARS_STREAM_CANCELS: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<u64, std::sync::Arc<std::sync::atomic::AtomicBool>>>,
> = std::sync::OnceLock::new();

fn orphan_sidecars_stream_registry(
) -> &'static std::sync::Mutex<std::collections::HashMap<u64, std::sync::Arc<std::sync::atomic::AtomicBool>>>
{
    ORPHAN_SIDECARS_STREAM_CANCELS.get_or_init(Default::default)
}

/// Streaming variant of `find_orphan_sidecars` (CPE-1299, streaming-liveness convention): pushes each
/// directory's orphan batch over an IPC channel as [`cpe_server::orphan_sidecars_scan::walk_orphan_sidecars`]
/// finds it, so a slow/large tree paints progressively instead of blocking on the whole walk. Async +
/// `spawn_blocking` so the walk never freezes the UI thread (CPE-760). `stream_id` (frontend-supplied,
/// monotonic) registers a cancel flag polled each batch, so a superseded walk stops promptly — mirrors
/// `list_dir_stream`/`cancel_dir_stream` (CPE-665). The returned result carries the final `scanned` +
/// `truncated` with an empty `orphans` (those streamed, in walk order).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn find_orphan_sidecars_stream(
    root: String,
    recursive: bool,
    excludes: Vec<String>,
    stream_id: u64,
    on_orphan: tauri::ipc::Channel<Vec<String>>,
) -> Result<cpe_server::orphan_sidecars_scan::OrphanSidecarResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use std::sync::atomic::Ordering;
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        orphan_sidecars_stream_registry().lock().unwrap().insert(stream_id, cancel.clone());
        let tail = cpe_server::orphan_sidecars_scan::walk_orphan_sidecars(
            std::path::Path::new(&root),
            recursive,
            &excludes,
            |batch| {
                let _ = on_orphan.send(batch);
                if cancel.load(Ordering::Relaxed) {
                    std::ops::ControlFlow::Break(())
                } else {
                    std::ops::ControlFlow::Continue(())
                }
            },
        );
        orphan_sidecars_stream_registry().lock().unwrap().remove(&stream_id);
        Ok(cpe_server::orphan_sidecars_scan::OrphanSidecarResult {
            orphans: Vec::new(),
            scanned: tail.scanned,
            truncated: tail.truncated,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Signal an in-flight `find_orphan_sidecars_stream` to stop at the next batch boundary (CPE-1299). A
/// no-op if the stream already finished (its id is gone from the registry).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn cancel_orphan_sidecars_stream(stream_id: u64) {
    use std::sync::atomic::Ordering;
    if let Some(flag) = orphan_sidecars_stream_registry().lock().unwrap().get(&stream_id) {
        flag.store(true, Ordering::Relaxed);
    }
}

/// Find dangling (target-missing) and cyclic (self/loop) symlinks under `root` (CPE-1284, epic CPE-1002).
/// Thin `spawn_blocking` dispatcher into [`cpe_server::dangling_links_scan::find_dangling_links`], which
/// errors when `root` isn't a directory. `excludes` are glob patterns pruning matching sub-directories
/// (CPE-1302); an empty list walks the whole tree.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn find_dangling_links(
    root: String,
    excludes: Vec<String>,
) -> Result<cpe_server::dangling_links_scan::DanglingReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        cpe_server::dangling_links_scan::find_dangling_links(std::path::Path::new(&root), &excludes)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Registry of in-flight `find_dangling_links_stream` walks' cancel flags, keyed by the frontend-supplied
/// `stream_id`, mirroring `DIR_STREAM_CANCELS` (CPE-1299, STREAMING.md).
static DANGLING_LINKS_STREAM_CANCELS: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<u64, std::sync::Arc<std::sync::atomic::AtomicBool>>>,
> = std::sync::OnceLock::new();

fn dangling_links_stream_registry(
) -> &'static std::sync::Mutex<std::collections::HashMap<u64, std::sync::Arc<std::sync::atomic::AtomicBool>>>
{
    DANGLING_LINKS_STREAM_CANCELS.get_or_init(Default::default)
}

/// Streaming variant of `find_dangling_links` (CPE-1299, streaming-liveness convention): pushes classified
/// batches over an IPC channel as [`cpe_server::dangling_links_scan::walk_dangling_links`] delivers them —
/// note the walk itself must finish (classification needs the whole link set) before the first flush, but
/// `flush` still lets a superseded stream stop consuming further batches early. Async + `spawn_blocking` so
/// the walk never freezes the UI thread (CPE-760). `stream_id` (frontend-supplied, monotonic) registers a
/// cancel flag polled each batch, mirroring `list_dir_stream`/`cancel_dir_stream` (CPE-665). The returned
/// result carries the final `scanned` + `truncated` with an empty `links` (those streamed).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn find_dangling_links_stream(
    root: String,
    excludes: Vec<String>,
    stream_id: u64,
    on_link: tauri::ipc::Channel<Vec<cpe_server::dangling_links::DanglingLink>>,
) -> Result<cpe_server::dangling_links_scan::DanglingReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use std::sync::atomic::Ordering;
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        dangling_links_stream_registry().lock().unwrap().insert(stream_id, cancel.clone());
        let tail = cpe_server::dangling_links_scan::walk_dangling_links(std::path::Path::new(&root), &excludes, |batch| {
            let _ = on_link.send(batch);
            if cancel.load(Ordering::Relaxed) {
                std::ops::ControlFlow::Break(())
            } else {
                std::ops::ControlFlow::Continue(())
            }
        });
        dangling_links_stream_registry().lock().unwrap().remove(&stream_id);
        let tail = tail?;
        Ok(cpe_server::dangling_links_scan::DanglingReport {
            links: Vec::new(),
            scanned: tail.scanned,
            truncated: tail.truncated,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Signal an in-flight `find_dangling_links_stream` to stop at the next batch boundary (CPE-1299). A
/// no-op if the stream already finished (its id is gone from the registry).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn cancel_dangling_links_stream(stream_id: u64) {
    use std::sync::atomic::Ordering;
    if let Some(flag) = dangling_links_stream_registry().lock().unwrap().get(&stream_id) {
        flag.store(true, Ordering::Relaxed);
    }
}

/// Sweep `root` for files whose sniffed content disagrees with their claimed extension (CPE-1285, epic
/// CPE-1002) — e.g. a `.jpg` that's really a Windows PE. Thin `spawn_blocking` dispatcher into
/// [`cpe_server::type_mismatch_scan::find_type_mismatches`]; never errors. `excludes` are glob patterns
/// pruning matching sub-directories (CPE-1302); an empty list sweeps the whole tree.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn find_type_mismatches(
    root: String,
    excludes: Vec<String>,
) -> Result<cpe_server::type_mismatch_scan::MismatchReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        cpe_server::type_mismatch_scan::find_type_mismatches(std::path::Path::new(&root), &excludes)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Registry of in-flight `find_type_mismatches_stream` walks' cancel flags, keyed by the frontend-supplied
/// `stream_id`, mirroring `DIR_STREAM_CANCELS` (CPE-1299, STREAMING.md).
static TYPE_MISMATCH_STREAM_CANCELS: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<u64, std::sync::Arc<std::sync::atomic::AtomicBool>>>,
> = std::sync::OnceLock::new();

fn type_mismatch_stream_registry(
) -> &'static std::sync::Mutex<std::collections::HashMap<u64, std::sync::Arc<std::sync::atomic::AtomicBool>>>
{
    TYPE_MISMATCH_STREAM_CANCELS.get_or_init(Default::default)
}

/// Streaming variant of `find_type_mismatches` (CPE-1299, streaming-liveness convention): pushes each
/// flagged-file batch over an IPC channel as [`cpe_server::type_mismatch_scan::walk_type_mismatches`] finds
/// it, so a slow/large tree paints progressively instead of blocking on the whole sweep. Async +
/// `spawn_blocking` so the walk never freezes the UI thread (CPE-760). `stream_id` (frontend-supplied,
/// monotonic) registers a cancel flag polled each batch, mirroring `list_dir_stream`/`cancel_dir_stream`
/// (CPE-665). The returned result carries the final `scanned` + `truncated` with an empty `hits` (those
/// streamed, in walk order).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn find_type_mismatches_stream(
    root: String,
    excludes: Vec<String>,
    stream_id: u64,
    on_hit: tauri::ipc::Channel<Vec<cpe_server::type_mismatch_scan::MismatchHit>>,
) -> Result<cpe_server::type_mismatch_scan::MismatchReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use std::sync::atomic::Ordering;
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        type_mismatch_stream_registry().lock().unwrap().insert(stream_id, cancel.clone());
        let tail = cpe_server::type_mismatch_scan::walk_type_mismatches(std::path::Path::new(&root), &excludes, |batch| {
            let _ = on_hit.send(batch);
            if cancel.load(Ordering::Relaxed) {
                std::ops::ControlFlow::Break(())
            } else {
                std::ops::ControlFlow::Continue(())
            }
        });
        type_mismatch_stream_registry().lock().unwrap().remove(&stream_id);
        Ok(cpe_server::type_mismatch_scan::MismatchReport {
            hits: Vec::new(),
            scanned: tail.scanned,
            truncated: tail.truncated,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Signal an in-flight `find_type_mismatches_stream` to stop at the next batch boundary (CPE-1299). A
/// no-op if the stream already finished (its id is gone from the registry).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn cancel_type_mismatches_stream(stream_id: u64) {
    use std::sync::atomic::Ordering;
    if let Some(flag) = type_mismatch_stream_registry().lock().unwrap().get(&stream_id) {
        flag.store(true, Ordering::Relaxed);
    }
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

/// Record a folder the user just opened into the system-tray quick-access list (CPE-1272, epic CPE-713):
/// it moves to the front of "recents", is persisted, and the tray menu is refreshed. The frontend fires
/// this on every folder navigation (`recordRecentFolder`). Not annotated for typed bindings — it takes
/// only strings, so callers use the busy-cursor `invoke` wrapper. A no-op on mobile (no system tray).
#[tauri::command]
// The `return` reads as needless on desktop (where it's the last statement once the mobile arm below is
// cfg'd out), but it's required so the two mutually-exclusive cfg arms both type-check.
#[allow(clippy::needless_return)]
fn tray_note_folder(app: tauri::AppHandle, path: String, label: String) -> Result<(), String> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        return tray::note_folder(&app, &path, &label);
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = (&app, &path, &label);
        Ok(())
    }
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
///
/// **The `fs::write` below has no slot guard, and that is a known open bug, not a considered choice
/// (CPE-1734).** CPE-1725 inventoried this site while settling the dangling-link question for the two
/// whole-file *save* paths and deliberately left it alone: `to` is a name being **claimed** (an extension
/// swap; `from != to` is enforced), so the guard it wants is CPE-1718's `create_slot_refusal` +
/// `create_exclusive`, **not** CPE-1716's `replace_file_contents` — resolving a link is the right answer
/// when editing a file the user opened and the wrong one when claiming a name. Today a link at `to` is
/// followed (`fs::write` follows the final component, and a dangling one reads as a free name), the bytes
/// land at the link's target, and `trash::delete` then removes the original anyway. Recorded here rather
/// than fixed under a ticket about the other primitive; see CPE-1734 for the decision and its tests.
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

/// Whether the OS's high-contrast / increased-contrast accessibility mode is currently ON (CPE-1546, epic
/// CPE-1496). A one-shot read at boot (no live subscription — deferred) letting the frontend's
/// `ContrastSetting "system"` follow the real OS state; fail-open to `false`. The synchronous Win32/Cocoa/
/// D-Bus read runs on a blocking thread so it never stalls the main thread ([[async-all-blocking-commands]]).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn is_high_contrast_active() -> bool {
    tauri::async_runtime::spawn_blocking(cpe_server::high_contrast::is_high_contrast_active)
        .await
        .unwrap_or(false)
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

/// Whether a Windows drive letter's root should be listed (CPE-1696) — the pure classifier behind
/// [`list_drives_impl`]'s per-letter probe, split out so the taxonomy is unit-testable on every OS and
/// account (a real locked BitLocker volume or an empty card reader can't be conjured in a test).
///
/// `stat` is the outcome of [`Path::try_exists`] on `"X:\\"`. The probe used to be `Path::exists()`, which
/// folds every `stat` failure into `false` — so a drive that is genuinely **present but not currently
/// readable** (a disconnected mapped network drive, a card reader with no card, a locked BitLocker volume)
/// vanished from the sidebar entirely, which is the one thing the user can least explain. Windows Explorer
/// itself lists those drives; so do we now. Only `Ok(false)` — an unassigned letter, which is where
/// `fs::exists` folds a genuine `ERROR_FILE_NOT_FOUND`/`ERROR_PATH_NOT_FOUND` — hides the row.
#[cfg(target_os = "windows")]
fn drive_letter_is_present(stat: std::io::Result<bool>) -> bool {
    match stat {
        Ok(present) => present,
        // A genuine absence, if the OS ever reports one as an error rather than `Ok(false)`.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        // Present but unreadable right now: list it rather than claim it isn't there.
        Err(_) => true,
    }
}

fn list_drives_impl() -> Vec<Place> {
    let mut drives = Vec::new();

    #[cfg(target_os = "windows")]
    {
        for letter in b'A'..=b'Z' {
            let root = format!("{}:\\", letter as char);
            if drive_letter_is_present(Path::new(&root).try_exists()) {
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

// ---- Windows-native network discovery (CPE-1519, epic CPE-1517) -----------------------------------
//
// Explorer's "Network" folder is populated by the WNet provider chain (WS-Discovery + mDNS, per the
// ticket's research) — this walks the SAME `RESOURCE_GLOBALNET` tree Explorer does, via
// `WNetOpenEnumW`/`WNetEnumResourceW`, recursing into container `NETRESOURCE`s (workgroup/domain →
// server → share). A server container's own `WNetEnumResource` call yields its disk shares directly, so
// there's no separate `NetShareEnum` step. The pure mapping/flatten logic lives in
// `cpe_server::net_share` (`DiscoveredResource`/`map_discovered_share`/`flatten_discovered`); this is
// only the FFI walk + the timeout wrapper. Frontend "Discovered on your network" tier is a follow-up
// slice (this ticket's backend half only) — see CPE-1519's Work Log.

/// Windows-native network discovery: the OS's own view of the network neighborhood (CPE-1519),
/// returned as [`NetShare`](cpe_server::net_share::NetShare) rows (`kind: "discovered"`) uniform with
/// `list_network_shares`'s mapped-drive rows. **Windows only** — WS-Discovery/mDNS device discovery has
/// no WNet equivalent on other platforms; those get it via the cross-platform mDNS/Avahi backend
/// (CPE-1517) instead. Async + `spawn_blocking` + time-bounded: WNet enumeration is live network I/O
/// per hop (workgroup/domain → server → share) and can hang on an unresponsive/wedged provider, and
/// unlike a subprocess an in-flight WNet call can't be killed to force it to give up. So — mirroring
/// `run_bounded_capture`'s spawn+`recv_timeout` shape — the walk runs on a detached thread; if it
/// hasn't finished within [`DISCOVERY_TIMEOUT`] this gives up waiting and returns nothing (same as a
/// dead `net use` host today for `list_network_shares`). The abandoned thread simply keeps running to
/// completion in the background rather than being force-killed.
#[cfg(windows)]
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn discover_network_windows() -> Vec<cpe_server::net_share::NetShare> {
    tauri::async_runtime::spawn_blocking(discover_network_windows_impl)
        .await
        .unwrap_or_default()
}

#[cfg(windows)]
const DISCOVERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(6);

#[cfg(windows)]
fn discover_network_windows_impl() -> Vec<cpe_server::net_share::NetShare> {
    use std::sync::mpsc;

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(wnet_enum_level(None, 0));
    });

    match rx.recv_timeout(DISCOVERY_TIMEOUT) {
        Ok(resources) => cpe_server::net_share::flatten_discovered(&resources),
        Err(_) => Vec::new(),
    }
}

/// How many `NETRESOURCE` containers deep the walk will recurse (real network neighborhoods are 2-3
/// levels: workgroup/domain → server → share). Guards against a pathological or hostile provider
/// reporting a container that (directly or via a cycle) contains itself.
#[cfg(windows)]
const WNET_MAX_DEPTH: u32 = 6;

/// Starting `WNetEnumResource` buffer size, in `NETRESOURCEW` array elements. Generous enough that a
/// typical home/office network neighborhood never needs to grow it, while staying small (a few KB) for
/// the common case.
#[cfg(windows)]
const WNET_INITIAL_BUFFER_ENTRIES: usize = 32;

/// Cap on `ERROR_MORE_DATA` buffer-growth retries for a single `WNetEnumResource` call, so a provider
/// that keeps reporting "need more" can't spin this thread forever.
#[cfg(windows)]
const WNET_MAX_BUFFER_GROWS: u32 = 6;

/// Cap on the outer pagination loop in [`wnet_enum_level`]: the cumulative entries emitted across
/// all `WNetEnumResource` batches at one level, and (belt-and-suspenders) the raw count of outer
/// iterations. Far above any real network neighborhood's share count — guards against a
/// pathological/hostile provider that keeps returning `ERROR_SUCCESS` with `count > 0` without
/// advancing its enumeration cursor, which would otherwise spin the (detached, post-timeout)
/// background thread and grow the output `Vec` unboundedly.
#[cfg(windows)]
const WNET_MAX_TOTAL_ENTRIES: usize = 4096;

/// Open a `WNetEnumResource` handle at `RESOURCE_GLOBALNET`, either the top level (`container: None`)
/// or recursed into one specific container `NETRESOURCE` returned by a previous call at this scope.
/// `RESOURCETYPE_DISK` filters to disk shares (and the containers that may hold them) — printer shares
/// and other resource types are out of scope for a file explorer. Returns `None` on any failure (access
/// denied, an unreachable/unresponsive provider, a container that vanished between enumeration and this
/// open) so the caller can skip that branch rather than fail the whole walk.
#[cfg(windows)]
fn wnet_open(
    container: Option<&windows::Win32::NetworkManagement::WNet::NETRESOURCEW>,
) -> Option<windows::Win32::Foundation::HANDLE> {
    use windows::Win32::Foundation::{ERROR_SUCCESS, HANDLE};
    use windows::Win32::NetworkManagement::WNet::{
        WNetOpenEnumW, RESOURCETYPE_DISK, RESOURCEUSAGE_NONE, RESOURCE_GLOBALNET,
    };

    let mut henum = HANDLE::default();
    let lpnetresource = container.map(|c| c as *const _);
    // SAFETY: `container`, when `Some`, is a live `&NETRESOURCEW` borrowed from the caller's
    // still-in-scope `Vec<NETRESOURCEW>` buffer (see `wnet_enum_level`) — `WNetOpenEnumW` only reads
    // through the pointer for the duration of this synchronous call, which is within that borrow's
    // lifetime. `henum` is a valid `&mut` to a stack local, matching the documented out-parameter.
    let status = unsafe {
        WNetOpenEnumW(RESOURCE_GLOBALNET, RESOURCETYPE_DISK, RESOURCEUSAGE_NONE, lpnetresource, &mut henum)
    };
    (status == ERROR_SUCCESS).then_some(henum)
}

/// Enumerate one level of the WNet tree (the top level when `container` is `None`, or one container's
/// children when recursing) into [`DiscoveredResource`](cpe_server::net_share::DiscoveredResource)
/// nodes, recursing into any child container up to [`WNET_MAX_DEPTH`]. Skips (rather than fails) a
/// level this can't open, a call that errors mid-enumeration, a container whose buffer keeps
/// demanding more space past [`WNET_MAX_BUFFER_GROWS`], or an outer pagination loop that keeps
/// yielding entries (or bare iterations) past [`WNET_MAX_TOTAL_ENTRIES`] — the caller
/// (`flatten_discovered`) treats a partial/empty result as "nothing found here", never a hard
/// error, mirroring `list_dir`'s skip-on-error guarantee.
#[cfg(windows)]
fn wnet_enum_level(
    container: Option<&windows::Win32::NetworkManagement::WNet::NETRESOURCEW>,
    depth: u32,
) -> Vec<cpe_server::net_share::DiscoveredResource> {
    use windows::Win32::Foundation::{ERROR_MORE_DATA, ERROR_SUCCESS};
    use windows::Win32::NetworkManagement::WNet::{WNetCloseEnum, WNetEnumResourceW, NETRESOURCEW};

    if depth > WNET_MAX_DEPTH {
        return Vec::new();
    }
    let Some(henum) = wnet_open(container) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    let mut cap: usize = WNET_INITIAL_BUFFER_ENTRIES;
    let mut outer_iters: usize = 0;

    loop {
        outer_iters += 1;
        if outer_iters > WNET_MAX_TOTAL_ENTRIES {
            // Belt-and-suspenders: even if every batch reported zero new entries (so the
            // `out.len()` check below never trips), a provider that just keeps returning
            // `ERROR_SUCCESS` can't spin this loop past a bounded number of passes.
            break;
        }
        let mut buf: Vec<NETRESOURCEW> = vec![NETRESOURCEW::default(); cap];
        let mut count: u32 = u32::MAX; // "as many entries as fit in the buffer"
        let mut cb: u32 = (cap * std::mem::size_of::<NETRESOURCEW>()) as u32;

        let mut grows = 0u32;
        let status = loop {
            // SAFETY: `henum` is the handle opened above and closed on every exit path below (still
            // open here). `buf` is a real `Vec<NETRESOURCEW>` (not a raw byte buffer reinterpreted),
            // so its allocation is correctly aligned for an array of `NETRESOURCEW` — required, since
            // the struct contains pointer-sized fields. `cb` is always `buf.len() *
            // size_of::<NETRESOURCEW>()` in bytes on input, matching the capacity we actually pass;
            // `count`/`cb` are valid `&mut` out-parameters the API is documented to write through.
            let s = unsafe {
                WNetEnumResourceW(
                    henum,
                    &mut count,
                    buf.as_mut_ptr() as *mut core::ffi::c_void,
                    &mut cb,
                )
            };
            if s == ERROR_MORE_DATA && grows < WNET_MAX_BUFFER_GROWS {
                // The buffer was too small for even one entry; `cb` now holds the byte size the API
                // needs. A `Vec<NETRESOURCEW>` sized to fit that many bytes' worth of *structs* is
                // always large enough — the entries' variable-length string data packs into the same
                // flat buffer a raw byte allocation would need, so rounding up to whole `NETRESOURCEW`
                // elements only ever over-allocates relative to that byte count, never under-allocates.
                // Nothing was consumed by this failed call, so retrying at the SAME enumeration
                // position (not calling again after this loop) is correct.
                let need = (cb as usize).div_ceil(std::mem::size_of::<NETRESOURCEW>()).max(1);
                cap = need.max(cap * 2);
                buf = vec![NETRESOURCEW::default(); cap];
                count = u32::MAX;
                cb = (cap * std::mem::size_of::<NETRESOURCEW>()) as u32;
                grows += 1;
                continue;
            }
            break s;
        };

        if status != ERROR_SUCCESS {
            // ERROR_NO_MORE_ITEMS (normal end of this level) or any other error (skip the rest of this
            // level rather than fail the whole walk) both stop here.
            break;
        }
        // SAFETY: the API just reported `count` valid, initialized `NETRESOURCEW` entries at the front
        // of `buf`; `count <= buf.len()` by construction (we always told it exactly `buf.len()`
        // elements of space was available), but `.min()` guards that invariant defensively rather than
        // trusting it blindly for an unsafe slice index.
        let n = (count as usize).min(buf.len());
        if n == 0 {
            // Defensive: a provider reporting success with zero entries would otherwise spin forever.
            break;
        }
        for entry in &buf[..n] {
            out.push(discovered_resource_from(entry, depth));
        }
        if out.len() >= WNET_MAX_TOTAL_ENTRIES {
            // Cumulative-entry cap: a provider that keeps reporting `ERROR_SUCCESS` with
            // `count > 0` without advancing its cursor would otherwise grow `out` forever. This
            // is a hard bound far above any real network neighborhood's share count, so it never
            // fires in practice; when it does, return the partial results gathered so far rather
            // than error the whole walk (same skip-on-error semantics as the depth and
            // buffer-grows caps above).
            break;
        }
    }

    // SAFETY: `henum` was returned by the matching `WNetOpenEnumW` in `wnet_open` above and hasn't been
    // closed on any earlier path in this function.
    unsafe {
        let _ = WNetCloseEnum(henum);
    }
    out
}

/// Map one FFI `NETRESOURCEW` entry (still backed by the live enumeration buffer) into a plain-data
/// [`DiscoveredResource`](cpe_server::net_share::DiscoveredResource), recursing into it first if it's a
/// container so `children` arrives pre-populated for the pure `flatten_discovered` step.
#[cfg(windows)]
fn discovered_resource_from(
    entry: &windows::Win32::NetworkManagement::WNet::NETRESOURCEW,
    depth: u32,
) -> cpe_server::net_share::DiscoveredResource {
    use windows::Win32::NetworkManagement::WNet::RESOURCEUSAGE_CONTAINER;

    let remote_name = unsafe { wnet_wide_field(entry.lpRemoteName) };
    let display_name = unsafe { wnet_wide_field(entry.lpComment) };
    let is_container = entry.dwUsage & RESOURCEUSAGE_CONTAINER.0 != 0;
    let children = if is_container { wnet_enum_level(Some(entry), depth + 1) } else { Vec::new() };

    cpe_server::net_share::DiscoveredResource { remote_name, display_name, is_container, children }
}

/// Read a `NETRESOURCEW` wide-string field (`lpRemoteName`/`lpComment`) into an owned `String`. Returns
/// `None` for a null pointer (fields the provider left unset) or a run that isn't valid UTF-16 — never
/// panics either way.
///
/// # Safety
/// `field` must be either null, or a pointer taken directly from a `NETRESOURCEW` entry that a
/// `WNetEnumResourceW` call just populated, read before the buffer backing it is dropped or reused.
/// `WNetEnumResourceW` packs each entry's variable-length string data into the SAME flat buffer as the
/// struct array, NUL-terminated, so `PWSTR::to_string`'s `wcslen`-then-copy is sound for as long as
/// that buffer is alive — true at every call site here (`buf` in `wnet_enum_level` outlives the
/// `discovered_resource_from` call that reads its entries).
#[cfg(windows)]
unsafe fn wnet_wide_field(field: windows::core::PWSTR) -> Option<String> {
    if field.is_null() {
        return None;
    }
    field.to_string().ok()
}

#[cfg(not(windows))]
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn discover_network_windows() -> Vec<cpe_server::net_share::NetShare> {
    // WNet/Explorer-parity network discovery is a Windows-only concept (CPE-1519); other platforms get
    // discovery via the cross-platform mDNS/Avahi backend (CPE-1517) instead. Never called from a
    // non-Windows frontend, but kept as a real stub (not `#[cfg(windows)]`-absent) so the command exists
    // uniformly in `generate_handler!` and the build stays clean on every OS.
    Vec::new()
}

// ---- Cross-platform mDNS/DNS-SD network discovery (CPE-1523, epic CPE-1517) -----------------------
//
// The cross-platform complement to `discover_network_windows` above: mDNS/DNS-SD is the ONLY discovery
// path on macOS/Linux, and on Windows it's a superset — it surfaces sftp/webdav/ftp/nfs hosts WNet's
// SMB-only neighborhood never sees. All the browse/resolve/map/dedup logic lives in the Tauri-free
// `cpe_mdns` crate (mirroring `cpe_server::net_share`'s pure/impure split); this command is the thin
// one-line dispatcher CLAUDE.md's server-architecture convention calls for.

/// How long [`discover_network_mdns`] waits for mDNS/DNS-SD responders to answer before returning
/// whatever it has collected so far — matches [`DISCOVERY_TIMEOUT`], the WNet tier's own bound.
const MDNS_DISCOVERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(6);

/// Cross-platform mDNS/DNS-SD LAN discovery (CPE-1523): browses for smb/sftp/webdav/webdavs/ftp/nfs
/// service advertisements and returns them as [`NetShare`](cpe_server::net_share::NetShare) rows
/// (`kind: "discovered"`), uniform with `discover_network_windows`'s WNet rows. Unlike that command,
/// this one is identical on every OS — **not** `#[cfg(windows)]`-gated — so it's a normal
/// `generate_handler!` entry AND included in the typed specta bindings (see `export_bindings` below).
/// Async + `spawn_blocking`: `cpe_mdns::discover` blocks the calling thread for up to
/// [`MDNS_DISCOVERY_TIMEOUT`] polling the daemon's receivers, so it must run off the async executor's
/// thread exactly like `discover_network_windows_impl` does for its WNet walk.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn discover_network_mdns() -> Vec<cpe_server::net_share::NetShare> {
    tauri::async_runtime::spawn_blocking(|| cpe_mdns::discover(MDNS_DISCOVERY_TIMEOUT))
        .await
        .unwrap_or_default()
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

// ---- Embedded terminal dock: tab model + PTY backend (CPE-1242/947, epic CPE-714) -------------------
// The dock's tab bookkeeping (`TerminalDock`) lives in `cpe_server::terminal_tabs` (Tauri-free, unit
// tested there); the PTY session type + registry (`pty::PtySession`/`pty::PtyRegistry`) live in this
// app's own `pty` module since a session spawns OS processes + streams over `ipc::Channel` (adapter
// territory, per CLAUDE.md). Both are managed state; these commands are thin dispatchers into them.

/// Open a tab in the terminal dock at `cwd` (auto-titled from `cwd`'s basename when `title` is blank)
/// and return its id. Pure bookkeeping — does not spawn a shell; pair with `open_pty` to actually run
/// one (the frontend correlates the two by passing this id back as `open_pty`'s `session_id`, CPE-1243).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn terminal_dock_open(
    state: tauri::State<cpe_server::terminal_tabs::TerminalDockState>,
    cwd: String,
    title: String,
) -> u64 {
    state.open(&cwd, &title)
}

/// Close a dock tab (active-tab fixup happens inside `TerminalDock::close`). Does **not** kill any PTY —
/// callers close the tab's `open_pty` session (via `close_pty`) themselves, so the two stay decoupled
/// (a tab can exist with no live shell, e.g. mid-open).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn terminal_dock_close(state: tauri::State<cpe_server::terminal_tabs::TerminalDockState>, id: u64) {
    state.close(id);
}

/// Make a dock tab active (no-op if `id` is unknown).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn terminal_dock_activate(state: tauri::State<cpe_server::terminal_tabs::TerminalDockState>, id: u64) {
    state.activate(id);
}

/// Update a dock tab's working directory (e.g. after the panel follows navigation or the shell `cd`s).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn terminal_dock_set_cwd(state: tauri::State<cpe_server::terminal_tabs::TerminalDockState>, id: u64, cwd: String) {
    state.set_cwd(id, &cwd);
}

/// All open dock tabs, in order.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn terminal_dock_tabs(
    state: tauri::State<cpe_server::terminal_tabs::TerminalDockState>,
) -> Vec<cpe_server::terminal_tabs::TermTab> {
    state.tabs()
}

/// The dock's currently active tab, if any are open.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn terminal_dock_active(
    state: tauri::State<cpe_server::terminal_tabs::TerminalDockState>,
) -> Option<cpe_server::terminal_tabs::TermTab> {
    state.active_tab()
}

/// Open a PTY at `cwd` (empty/`None` = the process's own cwd) running `shell` (a program name/path, e.g.
/// `"powershell.exe"`/`"/bin/zsh"` — the frontend's shell picker, CPE-1243) or, when `shell` is
/// `None`/blank, the OS's own default shell (`pty::default_shell()`, via `pty::resolve_shell`). Starts
/// streaming its output over `on_output` as base64-encoded chunks (exact bytes — ANSI escapes and any
/// split multibyte UTF-8 survive the trip — mirroring the sidecar's own PTY wire format,
/// `session_server.rs`). Returns the new session's id, which `write_pty`/`resize_pty`/`close_pty` key on.
///
/// Async + `spawn_blocking` for the OS spawn itself (CPE-760/761 — launching a subprocess must never run
/// on the UI thread). The **read loop** that follows is deliberately a raw `std::thread::spawn`, not
/// another `spawn_blocking`: it runs for the session's whole lifetime, and parking a `spawn_blocking`
/// task per open terminal would slowly exhaust the async runtime's bounded blocking-thread pool. This
/// mirrors the sidecar's own long-lived reader pumps (`session_daemon.rs::spawn_reader`,
/// `session_engine.rs::LocalEngine::launch`).
///
/// Self-cleaning: if the child exits on its own (the user types `exit`) the reader hits EOF and removes
/// the session from the registry there too — a closed (or self-terminated) panel is never left as a
/// "live" entry even if the frontend never calls `close_pty` (CPE-1242 DoD).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn open_pty(
    state: tauri::State<'_, pty::PtyRegistry>,
    cwd: Option<String>,
    shell: Option<String>,
    rows: u16,
    cols: u16,
    on_output: tauri::ipc::Channel<String>,
) -> Result<u64, String> {
    let (program, args) = pty::resolve_shell(shell);
    let launch = pty::PtyLaunch {
        program,
        args,
        cwd,
        env: std::collections::BTreeMap::new(),
        rows: rows.max(1),
        cols: cols.max(1),
    };
    let (session, mut reader, writer) = tauri::async_runtime::spawn_blocking(move || {
        let session = pty::PtySession::spawn(&launch)?;
        let reader = session.reader()?;
        let writer = session.writer()?;
        Ok::<_, String>((session, reader, writer))
    })
    .await
    .map_err(|e| e.to_string())??;

    let registry = state.inner().clone();
    let id = registry.insert(session, writer);

    let cleanup_registry = registry.clone();
    std::thread::spawn(move || {
        use base64::Engine as _;
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break, // EOF or read error: the child is gone.
                Ok(n) => {
                    let chunk = base64::engine::general_purpose::STANDARD.encode(&buf[..n]);
                    if on_output.send(chunk).is_err() {
                        break; // frontend/channel gone — stop pumping.
                    }
                }
            }
        }
        // The child exited on its own (or the channel died) without a `close_pty` call — still leave no
        // live entry behind: `close` removes the registry entry and kills the (likely already-dead)
        // child in one idempotent call, same as an explicit `close_pty`.
        let _ = cleanup_registry.close(id);
    });

    Ok(id)
}

/// Write `data` (base64-encoded, matching `open_pty`'s output encoding) to a session's shell input.
/// Async + `spawn_blocking` — writing to a PTY's input pipe is subprocess I/O (CPE-760/761).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn write_pty(state: tauri::State<'_, pty::PtyRegistry>, session_id: u64, data: String) -> Result<(), String> {
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&data)
        .map_err(|e| format!("bad base64 input: {e}"))?;
    let registry = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || registry.write(session_id, &bytes))
        .await
        .map_err(|e| e.to_string())?
}

/// Resize a session's terminal (rows/cols). A lightweight ioctl/WinAPI call on an already-open handle —
/// unlike open/write/close it touches no subprocess I/O, so it stays a plain sync command (mirrors
/// `agent_watch_stop`'s quick in-memory ops).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn resize_pty(state: tauri::State<pty::PtyRegistry>, session_id: u64, rows: u16, cols: u16) -> Result<(), String> {
    state.resize(session_id, rows, cols)
}

/// Close a PTY session: remove it from the registry (so it stops counting as live) and kill the child +
/// free the PTY. Idempotent — closing an unknown/already-closed id is a no-op success, so a panel that
/// races its own cleanup (or double-closes) never errors. Async + `spawn_blocking` — killing a child
/// process is subprocess control (CPE-760/761).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn close_pty(state: tauri::State<'_, pty::PtyRegistry>, session_id: u64) -> Result<(), String> {
    let registry = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || registry.close(session_id))
        .await
        .map_err(|e| e.to_string())?
}

// ---- Encrypted-vault lifecycle + keychain seam (CPE-1248, epic CPE-738) ----------------------------
// Thin async dispatchers into `cpe_server::vault_manager` (the Tauri-free lifecycle over the crypto core
// `vault_crypto`, CPE-1247). Every command that touches crypto (scrypt ~1s) or the filesystem runs on a
// `spawn_blocking` thread so the UI thread never stalls (CPE-760/761). Passphrases arrive as a `String`
// over IPC and are wrapped into an `age::secrecy::SecretString` at this boundary — never logged, never
// written to a plaintext file; the ONLY persistence is the OS keychain via `KeyringBackend` below.
// Unlocked-session state lives in the managed `VaultRegistry` (like `TerminalDockState`/`PtyRegistry`).

/// The real OS keychain backend for vault passphrases, via the cross-platform `keyring` crate — Windows
/// Credential Manager, macOS Keychain, Linux Secret Service (D-Bus). Mirrors the sidecar's proven
/// `KeyringBackend` (CPE-268/322): the code is identical across targets; only the per-OS `keyring`
/// feature (in `Cargo.toml`) differs. This adapter is the app's concrete
/// `cpe_server::vault_manager::SecretAccess`, keeping `cpe-server` itself keyring- and Tauri-free.
#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
struct KeyringBackend;

#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
impl cpe_server::vault_manager::SecretAccess for KeyringBackend {
    fn set(&self, service: &str, account: &str, secret: &str) -> Result<(), String> {
        keyring::Entry::new(service, account)
            .and_then(|e| e.set_password(secret))
            .map_err(|e| e.to_string())
    }

    fn get(&self, service: &str, account: &str) -> Result<Option<String>, String> {
        match keyring::Entry::new(service, account).and_then(|e| e.get_password()) {
            Ok(pw) => Ok(Some(pw)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    fn delete(&self, service: &str, account: &str) -> Result<(), String> {
        match keyring::Entry::new(service, account).and_then(|e| e.delete_credential()) {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

/// The app's own encrypted-vault session root: `appCacheDir()/vault-sessions`. THE one resolver for that
/// path — the unlock containment guard (CPE-1647), the startup orphan sweep (CPE-1252) and the frontend's
/// `defaultAllocSessionDir` (`src/lib/vaultStore.ts`) must all name the same directory, or a legitimate
/// unlock would be refused (or a hostile one waved through). Pure path arithmetic on Tauri's resolver — no
/// filesystem I/O — so it is safe to call on the async command thread before `spawn_blocking`.
fn vault_sessions_root(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    server_ctx::TauriCtx::new(app)
        .app_cache_dir()
        .map(|c| c.join("vault-sessions"))
}

/// Is `path` a CPE vault? Detected by reading its `CPEVLT1` magic header, not its extension. A quick
/// bounded read, but still `spawn_blocking` since it opens a (possibly remote/slow) file (CPE-760/761).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn vault_is(path: String) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || cpe_server::vault_manager::is_vault(Path::new(&path)))
        .await
        .map_err(|e| e.to_string())
}

/// Seal `folder` into a `.cpevault` blob at `dest`. `shred_original` (default from the caller) destroys the
/// plaintext original — but ONLY after the encrypted copy is proven recoverable by a full decrypt
/// round-trip (the verify-before-shred safety invariant lives in `vault_manager::create_vault`). Async +
/// `spawn_blocking`: scrypt KDF (~1s) + a full tree walk/encrypt/write (CPE-760/761).
///
/// `confirmed` (CPE-1630) is a distinct flag from `shred_original`, required whenever `shred_original` is
/// true — mirroring `shred_paths`' `confirmed` gate (CPE-1611). `VaultCreateDialog.svelte` is the one
/// caller allowed to set it; a devtools/automation call with `shred_original: true, confirmed: false` is
/// refused by the engine before anything is written or destroyed.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn vault_create(
    folder: String,
    dest: String,
    passphrase: String,
    shred_original: bool,
    confirmed: bool,
) -> Result<(), String> {
    let pass = cpe_server::vault_manager::PassphraseSecret::from(passphrase);
    tauri::async_runtime::spawn_blocking(move || {
        let opts = cpe_server::vault_manager::CreateOpts { shred_original, ..Default::default() };
        cpe_server::vault_manager::create_vault(Path::new(&folder), Path::new(&dest), &pass, &opts, confirmed)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Unlock the vault at `blob_path` with `passphrase`, decrypting its tree into `session_dir` and recording
/// the unlocked state in the managed registry. Async + `spawn_blocking`: scrypt (~1s) + a full
/// decrypt/extract to disk (CPE-760/761).
///
/// `session_dir` is untrusted IPC input, so it is NOT taken at face value (CPE-1647): the engine refuses
/// any path that does not resolve strictly inside this app's own `vault-sessions` root (resolved here via
/// `vault_sessions_root`, the same dir the startup sweep and the frontend's `defaultAllocSessionDir` use).
/// Without that check a devtools/automation caller holding a vault + its passphrase could point the unlock
/// at any directory on the machine and then have `vault_lock` securely shred it. The root is recorded with
/// the session and the same check is re-run inside `vault_lock`, immediately before the wipe — validating
/// only here would contain the path *string*, not the directory that actually gets shredded.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn vault_unlock(
    app: tauri::AppHandle,
    state: tauri::State<'_, cpe_server::vault_manager::VaultRegistry>,
    blob_path: String,
    passphrase: String,
    session_dir: String,
) -> Result<(), String> {
    let registry = state.inner().clone();
    let sessions_root = vault_sessions_root(&app)?;
    let pass = cpe_server::vault_manager::PassphraseSecret::from(passphrase);
    tauri::async_runtime::spawn_blocking(move || {
        registry
            .unlock(
                cpe_server::vault_manager::SessionsRoot::new(&sessions_root),
                Path::new(&blob_path),
                &pass,
                Path::new(&session_dir),
            )
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Lock the vault at `blob_path`: **re-seal its session directory back into the blob**, securely wipe
/// (shred + remove) that directory so no plaintext lingers, and drop its unlocked state. Async +
/// `spawn_blocking`: a full tree walk/encrypt/write + a verifying decrypt + shreds (CPE-760/761) — locking
/// now costs roughly what creating the vault did.
///
/// Locking re-seals (CPE-1645): everything the user edited while the vault was unlocked is encrypted back
/// into the blob, and the working copy is wiped ONLY after the new blob has been written and proven to
/// decrypt from disk. A failed re-seal or wipe leaves the vault unlocked and everything intact, so the
/// error surfaced here is retryable; `classifyLockError` (`src/lib/vaultStore.ts`) sorts that from the
/// tamper refusal below, which is not.
///
/// The engine re-checks containment here, immediately before re-sealing and shredding (CPE-1647): the
/// session dir was validated at unlock, but other registered commands (`deletePermanent`/`moveExact` +
/// `createJunction`) can replace it with a link afterwards, so the *path* being contained at unlock is not
/// the same claim as the *directory* being re-sealed and wiped. A failed re-check re-seals nothing, wipes
/// nothing, forgets the session, and surfaces the refusal here rather than reporting a successful lock.
///
/// The error is a **structured** `LockError { code, message }`, not a string (SEC-847 finding 3): the
/// frontend's recovery differs completely between the three failure shapes, and the messages interpolate
/// file paths — so a file *inside* the vault could otherwise choose its name to impersonate a tamper
/// refusal and make the UI report a vault sealed while its whole decrypted tree was still on disk.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn vault_lock(
    state: tauri::State<'_, cpe_server::vault_manager::VaultRegistry>,
    blob_path: String,
) -> Result<(), cpe_server::vault_manager::LockError> {
    let registry = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || registry.lock(Path::new(&blob_path)))
        .await
        .map_err(|e| cpe_server::vault_manager::LockError {
            // A join failure means the blocking task itself died; nothing was re-sealed or wiped, so the
            // vault is still unlocked and a retry is the right advice.
            code: cpe_server::vault_manager::LockFailureCode::ResealFailed,
            message: e.to_string(),
        })?
}

/// The lifecycle status of `blob_path`: is-vault (by magic), unlocked (from the registry), and whether a
/// passphrase is saved in the keychain. Async + `spawn_blocking`: reads the file header + queries the OS
/// credential store (CPE-760/761). Carries no secret value.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn vault_status(
    state: tauri::State<'_, cpe_server::vault_manager::VaultRegistry>,
    blob_path: String,
) -> Result<cpe_server::vault_manager::VaultStatus, String> {
    let registry = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let path = Path::new(&blob_path);
        let unlocked = registry.is_unlocked(path);
        Ok(cpe_server::vault_manager::compute_status(path, unlocked, &KeyringBackend))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Save `passphrase` for the vault at `blob_path` in the OS keychain (the only place it persists). Async +
/// `spawn_blocking`: the credential store is a blocking OS call (CPE-760/761).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn vault_remember_passphrase(blob_path: String, passphrase: String) -> Result<(), String> {
    let pass = cpe_server::vault_manager::PassphraseSecret::from(passphrase);
    tauri::async_runtime::spawn_blocking(move || {
        cpe_server::vault_manager::remember_passphrase(&KeyringBackend, Path::new(&blob_path), &pass)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Forget any saved passphrase for the vault at `blob_path`. Async + `spawn_blocking`: a blocking OS
/// credential-store call (CPE-760/761).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn vault_forget_passphrase(blob_path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        cpe_server::vault_manager::forget_passphrase(&KeyringBackend, Path::new(&blob_path))
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---- Connection-secret keychain store (CPE-1510, epic CPE-1497 "mount anything" F1 slice) ----------
// A saved remote-connection profile (`cpe_server::connections::Connection`) records only non-secret
// metadata; the password/passphrase lives in the OS keychain, keyed by the connection's `name`, via
// `cpe_server::secret_store` — the same `SecretAccess`/`KeyringBackend` seam the vault work above uses,
// just under its own service (`cpe_server::secret_store::CONNECTION_SECRET_SERVICE`) so a connection
// secret never collides with a vault passphrase or any other stored secret. These three commands are the
// thin dispatchers; wiring the resolved secret into `vfs::open` at connect time is CPE-1499's job.

/// Save `secret` (a password or key passphrase) for the saved connection named `name`, overwriting any
/// value already stored for that name. The ONLY place this secret persists. Async + `spawn_blocking`: a
/// blocking OS credential-store call (CPE-760/761). The secret value is never logged or echoed back.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn connection_secret_set(name: String, secret: String) -> Result<(), String> {
    #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
    {
        tauri::async_runtime::spawn_blocking(move || {
            cpe_server::secret_store::set_secret(&KeyringBackend, &name, &secret)
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        let _ = (name, secret);
        Err("no OS keychain available on this platform".to_string())
    }
}

/// Fetch the stored secret for the saved connection named `name`, or `None` if none is saved. Async +
/// `spawn_blocking` (blocking OS credential-store call).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn connection_secret_get(name: String) -> Result<Option<String>, String> {
    #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
    {
        tauri::async_runtime::spawn_blocking(move || {
            cpe_server::secret_store::get_secret(&KeyringBackend, &name)
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        let _ = name;
        Ok(None)
    }
}

/// Delete the stored secret for the saved connection named `name` (e.g. the sidebar's "forget password").
/// Deleting a missing entry is `Ok`. Async + `spawn_blocking` (blocking OS credential-store call).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn connection_secret_delete(name: String) -> Result<(), String> {
    #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
    {
        tauri::async_runtime::spawn_blocking(move || {
            cpe_server::secret_store::delete_secret(&KeyringBackend, &name)
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        let _ = name;
        Ok(())
    }
}

// ---- Saved-connection profile CRUD (CPE-1513, epic CPE-1498 — the Network sidebar's entry point) ----
// Thin async wrappers over `cpe_server::connections::{load_connections, upsert, remove, save_connections}`
// — the pure logic + on-disk JSON persistence already live in `cpe-server` (CPE-683); these three commands
// are the dispatcher the sidebar's "＋ Add a connection" / Edit / Forget controls call. Each mutator
// re-loads, applies the pure reducer, saves, and returns the fresh whole list so the sidebar can just
// replace its local copy (same "return the updated store" shape as `set_tags`/`rename_tag` above). No
// secrets ever pass through here — those are `connection_secret_set/get/delete` above.

/// Every saved connection profile (no secrets — see module docs on `cpe_server::connections`).
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn connections_list() -> Result<Vec<cpe_server::connections::Connection>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let path = cpe_server::connections::default_connections_path()
            .ok_or_else(|| "no config directory available on this platform".to_string())?;
        Ok(cpe_server::connections::load_connections(&path))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Insert or replace a connection by name (editing is just an upsert with the same name). Returns the
/// updated whole list.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn connections_upsert(
    conn: cpe_server::connections::Connection,
) -> Result<Vec<cpe_server::connections::Connection>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = cpe_server::connections::default_connections_path()
            .ok_or_else(|| "no config directory available on this platform".to_string())?;
        let list = cpe_server::connections::upsert(cpe_server::connections::load_connections(&path), conn);
        cpe_server::connections::save_connections(&path, &list)?;
        Ok(list)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Remove the connection named `name` ("Forget" in the sidebar menu — the caller is also responsible
/// for `connection_secret_delete`, so no orphaned keychain entry survives). Returns the updated list.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn connections_remove(name: String) -> Result<Vec<cpe_server::connections::Connection>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = cpe_server::connections::default_connections_path()
            .ok_or_else(|| "no config directory available on this platform".to_string())?;
        let list = cpe_server::connections::remove(cpe_server::connections::load_connections(&path), &name);
        cpe_server::connections::save_connections(&path, &list)?;
        Ok(list)
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---- Remote-location routing (CPE-1511, epic CPE-1499 "mount anything" F3 — the crux) --------------
// A remote URI (sftp://…, webdav://…) resolves to a live [`cpe_server::provider::FileSystemProvider`] via
// `cpe_vfs::connect`, so `list_dir`/`list_dir_stream` browse it exactly like a folder. LOCAL PATHS NEVER
// REACH HERE: each command's fast-path classifies with `fs_route::route` first and only calls in for a
// recognised remote scheme, so the plain explorer's hot path is byte-for-byte unchanged (PURPOSE.md).
//
// The provider pool keeps one live session per saved connection (open once, reuse across ops). Secrets
// come from the OS keychain via the same `KeyringBackend`/`SecretAccess` seam the vault + connection-secret
// commands use; host keys use TOFU — a CHANGED/REVOKED key is refused loudly by the SFTP provider, and that
// distinct error propagates out here (never a silent connect). Provider-supplied names are re-filtered
// through the connected provider's OWN `is_safe_leaf_name` rule inside `remote_dir_entries` (CPE-1704 —
// defaults to `is_safe_name`, inheriting the CPE-1461/1462 traversal defense, but a backend with a
// different keyspace, e.g. S3, can state a different rule instead of the guard being hardcoded for every
// backend alike).

/// The process-wide pool of live remote providers, keyed by connection name — open once, reuse across ops.
#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
static REMOTE_POOL: std::sync::OnceLock<cpe_vfs::connect::ProviderPool> = std::sync::OnceLock::new();

/// Resolve a remote `uri` to a pooled provider: load the saved connections + the merged `known_hosts`
/// (CPE-1512: the app-managed store, which records first-contact SFTP keys, merged with the user's real
/// `~/.ssh/known_hosts` — whose pins always win on a conflict, see
/// [`cpe_server::known_hosts::load_merged_known_hosts`]), fetch the connection's secret through the
/// keychain, then open (or reuse) a provider via `cpe_vfs`. A changed host key refuses loudly (TOFU); a
/// missing connection or missing password-secret is a clear `Err`. A first-contact (`Unknown`) SFTP host
/// key is persisted to the app-managed store (never the user's own `~/.ssh/known_hosts`) so the next
/// connect to that host resolves `Trusted` instead of `Unknown` forever.
#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
fn remote_provider_for(uri: &str) -> Result<cpe_vfs::connect::SharedProvider, String> {
    let pool = REMOTE_POOL.get_or_init(cpe_vfs::connect::ProviderPool::new);
    let conns = cpe_server::connections::default_connections_path()
        .map(|p| cpe_server::connections::load_connections(&p))
        .unwrap_or_default();
    let app_known_hosts_path = cpe_server::known_hosts::default_app_known_hosts_path();
    let ssh_known_hosts_path = cpe_server::known_hosts::default_known_hosts_path();
    let known = match (&app_known_hosts_path, &ssh_known_hosts_path) {
        (Some(app), Some(ssh)) => cpe_server::known_hosts::load_merged_known_hosts(app, ssh),
        (Some(app), None) => cpe_server::known_hosts::load_known_hosts(app),
        (None, Some(ssh)) => cpe_server::known_hosts::load_known_hosts(ssh),
        (None, None) => Vec::new(),
    };
    cpe_vfs::connect::connected_provider(
        pool,
        &cpe_vfs::connect::VfsOpener,
        &KeyringBackend,
        &conns,
        &known,
        cpe_vfs::HostKeyPolicy::Tofu,
        uri,
        app_known_hosts_path.as_deref(),
    )
}

/// Platforms without an OS keychain can't resolve a connection secret, so remote locations are unavailable
/// there (the app only ships on desktop; this keeps the non-desktop build honest).
#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
fn remote_provider_for(_uri: &str) -> Result<cpe_vfs::connect::SharedProvider, String> {
    Err("remote locations require an OS keychain, unavailable on this platform".to_string())
}

/// Drop any pooled session for `uri`'s connection so the next op reconnects (used when a session looks
/// dead — a poisoned lock). A no-op on a platform without remote support.
fn invalidate_remote(uri: &str) {
    #[cfg(any(windows, target_os = "macos", target_os = "linux"))]
    {
        let conns = cpe_server::connections::default_connections_path()
            .map(|p| cpe_server::connections::load_connections(&p))
            .unwrap_or_default();
        if let Some(conn) = cpe_vfs::connect::find_connection(uri, &conns) {
            if let Some(pool) = REMOTE_POOL.get() {
                pool.invalidate(&conn.name);
            }
        }
    }
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        let _ = uri;
    }
}

/// Collect-to-vec remote directory listing — the remote arm of `list_dir` (and the entry point
/// `remote_list_dir_stream_impl` below reuses for the streaming arm). Returns the full
/// [`cpe_vfs::connect::RemoteListing`] — entries AND the filtered count together — so both callers can
/// decide what to do with `filtered` themselves rather than this function silently choosing for them.
///
/// CPE-1704 added the count; CPE-1708 carries it the rest of the way. It used to stop here as an
/// `eprintln!` because threading it into the `specta`-typed command response (+ the TS side that
/// consumes it) was judged out of scope for a fix already spanning three crates — the right call at the
/// time, not a rushed one. This function itself no longer logs anything: both `list_dir` (via
/// `ListDirResult`) and `list_dir_stream` (via `StreamDirResult`) now carry `filtered` to the frontend as
/// typed data instead.
fn remote_list_dir_impl(uri: String) -> Result<cpe_vfs::connect::RemoteListing, String> {
    let provider = remote_provider_for(&uri)?;
    let guard = match provider.lock() {
        Ok(g) => g,
        Err(_) => {
            invalidate_remote(&uri);
            return Err("remote provider is unavailable (session dropped); retry".to_string());
        }
    };
    cpe_vfs::connect::remote_dir_entries(&**guard, &uri)
}

/// Streaming remote directory listing — the remote arm of `list_dir_stream`. Reuses the SAME cancel
/// registry as the local walk (so `cancel_dir_stream` stops a superseded remote walk), and pushes the
/// mapped rows over the IPC `Channel` in `LIST_DIR_BATCH`-sized batches so the pane paints immediately.
fn remote_list_dir_stream_impl(
    uri: String,
    stream_id: u64,
    on_entry: tauri::ipc::Channel<Vec<DirEntry>>,
) -> Result<StreamDirResult, String> {
    use std::sync::atomic::Ordering;
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    dir_stream_registry().lock().unwrap().insert(stream_id, cancel.clone());
    let result = (|| {
        let listing = remote_list_dir_impl(uri)?;
        let result = stream_result_for(&listing);
        // Drain into owned batches (DirEntry isn't `Clone`, so move rather than copy) — one flush for a
        // small listing, several for a big one, mirroring the local batch size.
        let mut it = listing.entries.into_iter();
        loop {
            let batch: Vec<DirEntry> = it.by_ref().take(cpe_server::listing::LIST_DIR_BATCH).collect();
            if batch.is_empty() {
                break;
            }
            let _ = on_entry.send(batch);
            if cancel.load(Ordering::Relaxed) {
                break;
            }
        }
        Ok(result)
    })();
    dir_stream_registry().lock().unwrap().remove(&stream_id);
    result
}

/// The `RemoteListing` → `StreamDirResult` link of the CPE-1708 chain (the streaming twin of
/// `listing_to_result`): pure and independently unit testable — computed BEFORE `listing.entries` is
/// drained into channel batches, so `total`/`filtered` can never end up reflecting a partially-cancelled
/// walk.
fn stream_result_for(listing: &cpe_vfs::connect::RemoteListing) -> StreamDirResult {
    StreamDirResult { total: listing.entries.len(), filtered: listing.filtered }
}

// ---- User-defined command exec (CPE-783, epic CPE-711) --------------------------------------------
// Runs a user's resolved command line (built by userCommands.resolveCommand / cmdTemplate, CPE-781) and
// returns its captured output + exit code. Executed through the platform shell (`cmd /C` on Windows,
// `sh -c` elsewhere) so a normal command string with pipes/quotes works as the user expects. Output is
// capped per stream so a chatty command can't balloon memory.
//
// CPE-1665: this comment used to say "the frontend MUST confirm the resolved command with the user
// BEFORE calling" — the backend delegating a safety decision to the UI, which is precisely the pattern
// CPE-1651 was filed against, on the most powerful primitive in the process (arbitrary code execution,
// strictly more than deletion). The requirement now lives in Rust as `run_command`'s `confirmed` flag;
// see that command's doc comment for what the flag genuinely defends and what it does not.

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

/// Run a resolved user command line through the platform shell and capture its output (CPE-783). Async
/// per the commands rule.
///
/// **Refuses up front** ([`Err`], no process spawned, the shell never reached) when `confirmed` is
/// `false` — CPE-1665, the same shape CPE-1611 gave `shred_paths`, CPE-1630 gave `create_vault`, and
/// CPE-1651 gave `delete_permanent`/`empty_trash`. `RunCommandConfirm.svelte`'s "Run" button — which
/// shows the exact resolved command line(s) first — is the one and only call site allowed to set it.
///
/// **Be precise about what this defends — it is UI discipline enforced in Rust, NOT an authorization
/// boundary.** The flag rides on the same IPC message as `command`, so any caller that can forge the
/// call can also set the flag; nothing here makes shell execution safe to expose. What it genuinely
/// stops: a frontend call site that reaches this command without going through the confirm dialog; a
/// replayed pre-CPE-1665 payload (serde gives `confirmed` no default, so the old two-argument shape now
/// fails to deserialize outright); and a mechanical enumerator working from `bindings.gen.ts` that
/// doesn't know the field exists. A real boundary would have to be something the caller cannot mint — a
/// backend-issued one-shot consent token, or dropping the command from the IPC surface entirely.
///
/// **This gate is consistency, not coverage — do not read it as "shell execution now requires
/// consent".** The PR #855 security audit found several ungated siblings that reach a process launch
/// without passing through any dialog: `open_pty` takes a caller-supplied `shell` verbatim and
/// `write_pty` pushes arbitrary bytes into its stdin, so those two ungated calls give everything this
/// command gives and more; `run_as_admin` and `open_external` each launch a caller-named executable.
/// And `run_as_admin`'s "the UAC prompt is the consent" defence is **Windows-only** — on other
/// platforms it falls through to `open_external_impl` with no prompt at all. Those are filed separately
/// and deliberately not fixed here; this comment names them so a reader of *this* command doesn't
/// conclude the surface is closed.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn run_command(command: String, cwd: Option<String>, confirmed: bool) -> Result<CommandOutput, String> {
    tauri::async_runtime::spawn_blocking(move || run_command_impl(command, cwd, confirmed))
        .await
        .map_err(|e| e.to_string())?
}

/// See [`run_command`]. Takes `confirmed` itself, rather than being called only once the command has
/// checked it, so the CPE-1665 gate sits inside the code under test — the same entry point a forged IPC
/// call reaches (the reasoning `delete_permanent_impl` records).
fn run_command_impl(command: String, cwd: Option<String>, confirmed: bool) -> Result<CommandOutput, String> {
    // CONFIRM GATE (CPE-1665): checked first, before the command string is even inspected for
    // emptiness, and long before a shell is built. Refuses cleanly — no process is created.
    if !confirmed {
        return Err(
            "refusing to run: `confirmed` was not set on this run_command call — this launches an \
             external process through the platform shell, so it must be re-invoked with an explicit \
             confirmation (only RunCommandConfirm's \"Run\" button, which shows the resolved command \
             line first, should ever set it)"
                .to_string(),
        );
    }
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

/// Turn one archive-extraction command's result into the paths `note_app_op` should record (CPE-1745,
/// fixing drift from CPE-1195/1102): the REAL written path, taken from the `Ok` value, on success — and
/// nothing on failure.
///
/// This used to be a `note_app_op` call made *before* the extraction, mirroring
/// `cpe_server::archive::temp_extract_target`'s `dir.join(base)` by hand under a comment claiming it was
/// "the exact temp-file target" the server would write. It was not: CPE-1733 hardened that target with an
/// exclusive `fs::create_dir` that retries the next `<pid>-<seq>` when a name is taken, so the
/// subdirectory actually used is not predictable from outside the call, even in principle — a mirror
/// cannot be correct any more. Every extraction command already returns the written path as its `Ok`
/// value, so recording *after* the call, from that value, is both simpler and correct.
///
/// Recording only on success is a deliberate choice, not an oversight: a failed extraction wrote
/// nothing, so recording nothing is the accurate ledger entry, not a loss. It matches how
/// `organize_apply` and `template_stamp` elsewhere in this file already record only the paths a
/// spawn_blocking call actually produced, once its result is known.
fn archive_extract_op_paths(result: &Result<String, String>) -> Vec<String> {
    match result {
        Ok(path) => vec![path.clone()],
        Err(_) => Vec::new(),
    }
}

/// Extract a single entry from a ZIP to a temp file and return its path, so it
/// can be opened with its default app while browsing inside the archive
/// (CPE-242). Read-only: the temp copy is what opens, not the archived bytes.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn extract_archive_entry(app: tauri::AppHandle, zip: String, inner: String) -> Result<String, String> {
    let result = tauri::async_runtime::spawn_blocking(move || cpe_server::archive::extract_archive_entry(&zip, &inner))
        .await.map_err(|e| e.to_string())?;
    // See `archive_extract_op_paths` (CPE-1745): records the real returned path, success only.
    note_app_op(&app, || archive_extract_op_paths(&result));
    result
}

/// Extract a single entry from any supported non-zip archive (tar/tar.gz/tgz/7z; zip delegates to the
/// same underlying extractor as [`extract_archive_entry`]) to a temp file and return its path, so a leaf
/// inside a tar/7z archive can be opened the same way a zip leaf already can (CPE-1180, unblocks
/// CPE-1181). Read-only: the temp copy is what opens, not the archived bytes.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn extract_archive_entry_any(app: tauri::AppHandle, path: String, inner: String) -> Result<String, String> {
    let result = tauri::async_runtime::spawn_blocking(move || cpe_server::archive::extract_archive_entry_any(&path, &inner))
        .await.map_err(|e| e.to_string())?;
    // See `archive_extract_op_paths` (CPE-1745): records the real returned path, success only.
    note_app_op(&app, || archive_extract_op_paths(&result));
    result
}

/// Extract a single **STORED** entry from a `.rar` to a temp file and return its path (CPE-1360). RAR's
/// compression is proprietary with no free decoder, so only uncompressed (STORE) entries can be served;
/// a compressed entry returns a clear error. Used by the archive preview's extract-then-preview path and
/// by external-open of a stored rar leaf. Read-only: the temp copy is what previews/opens.
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
async fn extract_rar_entry(app: tauri::AppHandle, path: String, inner: String) -> Result<String, String> {
    let result = tauri::async_runtime::spawn_blocking(move || cpe_server::archive::extract_rar_entry(&path, &inner))
        .await.map_err(|e| e.to_string())?;
    // See `archive_extract_op_paths` (CPE-1745): records the real returned path, success only.
    note_app_op(&app, || archive_extract_op_paths(&result));
    result
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

/// Injects the resolved bundle-resource directory into the (Tauri-free) `cpe_server` crate's PDF +
/// video thumbnail extractors (CPE-1258 fix). `pdf-thumb`/`video-thumb` are unconditionally on for
/// `cpe-server` from this app (see the dependency line in `src-tauri/Cargo.toml`), so this call — like
/// the extractors themselves — is not gated behind `sidecar-platform`.
///
/// **Why this exists:** both extractors originally looked ONLY next to the running executable
/// (`current_exe().parent()`) for their bundled native binary. That's where Tauri's NSIS bundler
/// stages `bundle.resources` on Windows, so it worked there — but on macOS the exe lives in
/// `AppName.app/Contents/MacOS/` while resources land in `AppName.app/Contents/Resources/`, and on
/// Linux the exe is `usr/bin/<exe>` while resources land in `usr/lib/<exe>/`. On those two platforms
/// the old code silently never found the bundled pdfium/ffmpeg, and every PDF/video thumbnail
/// degraded to the generic type icon. `cpe_server` is Tauri-free and can't call
/// `app.path().resource_dir()` itself, so this adapter resolves it and pushes it in — mirroring how
/// `resolve_sidecar_bin` (below) resolves the SAME `resource_dir()` for the sidecar binaries, and
/// exactly matching where the `tauri.sidecar.*.conf.json` overlays' flat `bundle.resources` keys
/// (`"ffmpeg.exe"`, `"pdfium.dll"`, `"libpdfium.so"`, `"libpdfium.dylib"`, all with no subfolder
/// prefix) actually land.
///
/// Must run before the first thumbnail request reaches `cpe_server::thumb_pdf` (its pdfium binding is
/// resolved lazily and cached on first use — a later call would be too late); called from `setup()`,
/// so that's guaranteed. The shared `ffmpeg_util` resolver (CPE-1478 extraction; both `thumb_video` and
/// `media_waveform` use it) re-resolves on every call, so it has no such ordering requirement, but is
/// wired the same way for symmetry — one injection covers ffmpeg for both callers. Best-effort: if
/// `resource_dir()` can't be resolved (shouldn't happen on a real bundled build — only a theoretical
/// dev-environment gap), both extractors simply keep falling back to their `current_exe().parent()` /
/// PATH guesses, unchanged from before this fix.
fn init_thumbnail_native_dep_dir(app: &tauri::AppHandle) {
    use tauri::Manager;
    if let Ok(resource) = app.path().resource_dir() {
        cpe_server::thumb_pdf::set_native_dep_dir(resource.clone());
        cpe_server::ffmpeg_util::set_native_dep_dir(resource);
    }
}

/// Sweep orphaned encrypted-vault session dirs at startup (CPE-1252, VAULT-SECURITY.md §5). The
/// managed `VaultRegistry` is always empty at process start (v1 has no persisted unlock state across
/// a restart), so any `vault-sessions/*` dir found here can only be a leftover from the app being
/// killed/crashing while a vault was unlocked — by definition an orphan holding decrypted plaintext.
/// Model lives in `cpe_server::vault_manager::sweep_orphan_sessions`; this is the thin adapter that
/// resolves the SAME base dir the frontend allocates (`appCacheDir()` + `"vault-sessions"`, see
/// `src/lib/vaultStore.ts`'s `defaultAllocSessionDir`) via the shared `vault_sessions_root` helper — the
/// same one `vault_unlock`'s containment guard uses (CPE-1647) — so the sweep looks in exactly the
/// directory unlock/lock actually use.
///
/// Runs off the main/setup thread (`spawn`) and does the fs work in `spawn_blocking` (CPE-760/761) so
/// a large/slow sweep never delays the window coming up; best-effort and never fatal — a failed sweep
/// (missing cache dir, a permission error) only logs, it must never block or fail app launch.
fn sweep_orphan_vault_sessions_on_startup(app: &tauri::AppHandle) {
    let Ok(sessions_root) = vault_sessions_root(app) else {
        eprintln!("cpe: vault-session sweep skipped: could not resolve the app cache dir");
        return;
    };
    tauri::async_runtime::spawn(async move {
        let outcome = tauri::async_runtime::spawn_blocking(move || {
            cpe_server::vault_manager::sweep_orphan_sessions(&sessions_root)
        })
        .await;
        match outcome {
            Ok(Ok(0)) => {}
            Ok(Ok(n)) => eprintln!("cpe: swept {n} orphaned vault-session dir(s) at startup (CPE-1252)"),
            Ok(Err(e)) => eprintln!("cpe: vault-session sweep failed (non-fatal): {e}"),
            Err(e) => eprintln!("cpe: vault-session sweep task panicked (non-fatal): {e}"),
        }
    });
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

/// The outcome of looking for a sidecar's launchable binary (CPE-1696) — three states, not two. `Missing`
/// and `Unreadable` are different diagnoses and the difference is the whole point: given
/// [[install-kill-all-processes-first]], a **locked or half-installed** sidecar is one of the most common
/// real causes here, and "it isn't there — reinstall" is precisely the wrong advice for it.
///
/// Deliberately **not** behind `#[cfg(feature = "sidecar-platform")]`, unlike its one producer
/// [`resolve_sidecar_bin`]: CI runs `cargo test` for `src-tauri` with default features only and reaches
/// the sidecar feature via `cargo clippy --all-targets --features sidecar-platform`, which *compiles*
/// tests but never *runs* them — so a `sidecar-platform`-gated unit test would silently never execute
/// anywhere (an Evidence Rules trap: verify through the channel that will carry the message). This type
/// and the two pure classifiers below carry no sidecar dependency, so they compile and their tests run in
/// the default build.
#[cfg_attr(not(feature = "sidecar-platform"), allow(dead_code))]
#[derive(Debug, Clone)]
enum SidecarBinLookup {
    /// A binary resolved at this path.
    Found(PathBuf),
    /// Every candidate path was genuinely absent.
    Missing,
    /// No candidate resolved, and at least one could not be stat'd at all, so we cannot say it is absent.
    Unreadable { path: PathBuf, cause: String },
}

impl SidecarBinLookup {
    /// The resolved path, if one was found. Preserves the pre-CPE-1696 `Option<PathBuf>` reading for the
    /// launch/health question — an unreadable candidate is still not launchable.
    #[cfg_attr(not(feature = "sidecar-platform"), allow(dead_code))]
    fn found(&self) -> Option<&Path> {
        match self {
            SidecarBinLookup::Found(p) => Some(p.as_path()),
            _ => None,
        }
    }
}

/// What one candidate path's [`Path::try_exists`] outcome means to `resolve_sidecar_bin`'s search.
#[cfg_attr(not(feature = "sidecar-platform"), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq)]
enum SidecarCandidate {
    /// A binary is at this candidate path.
    Present,
    /// Genuinely nothing here; try the next candidate.
    Absent,
    /// The stat failed for a reason other than absence, so we cannot call this candidate empty.
    Unreadable(String),
}

/// The pure classifier behind `resolve_sidecar_bin`'s per-candidate probe, split out so the
/// `NotFound`-vs-everything-else taxonomy is unit-testable without a `tauri::AppHandle`, a real bundle
/// layout, or a real permission denial (all three are environment-dependent — same rationale as
/// `cpe_server::dispatch::classify_path_error`'s own unit tests).
#[cfg_attr(not(feature = "sidecar-platform"), allow(dead_code))]
fn classify_sidecar_candidate(stat: std::io::Result<bool>) -> SidecarCandidate {
    match stat {
        Ok(true) => SidecarCandidate::Present,
        Ok(false) => SidecarCandidate::Absent,
        // `try_exists` already folds a genuine `NotFound` into `Ok(false)`; be explicit anyway.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => SidecarCandidate::Absent,
        Err(e) => SidecarCandidate::Unreadable(e.to_string()),
    }
}

/// The `actions` line `sidecar_repair` reports for a binary lookup, or `None` when the binary resolved.
/// Pure so the "missing" / "unreadable" wording split is testable without an `AppHandle` (CPE-1696).
#[cfg_attr(not(feature = "sidecar-platform"), allow(dead_code))]
fn sidecar_binary_action(lookup: &SidecarBinLookup) -> Option<String> {
    match lookup {
        SidecarBinLookup::Found(_) => None,
        SidecarBinLookup::Missing => {
            Some("binary is missing — reinstall required (auto-restore is coming in L2)".into())
        }
        SidecarBinLookup::Unreadable { path, cause } => Some(format!(
            "binary could not be checked at {} ({cause}) — it may be locked by a still-running process or \
             half-installed, rather than missing; close every cpe / ai-console process and retry before \
             reinstalling",
            path.display()
        )),
    }
}

/// Whether a sidecar `id`'s launchable binary actually resolves — the "missing binary" health signal
/// (CPE-863). Generic over id (the exe name equals the sidecar id): checks the bundled resource copy,
/// then the dev source-tree targets, mirroring the per-sidecar resolvers.
///
/// **CPE-1696:** every candidate probe was `p.exists()`, which folds every `stat` failure into `false`, so
/// the function returned the same `None` for "no such file" and "that path exists but I was refused". It
/// now reports [`SidecarBinLookup::Unreadable`] for the latter, remembering the FIRST such candidate — a
/// genuine absence is only ever `Ok(false)` (or an explicit `NotFound`) from [`Path::try_exists`].
#[cfg(feature = "sidecar-platform")]
fn resolve_sidecar_bin(app: &tauri::AppHandle, id: &str) -> SidecarBinLookup {
    use tauri::Manager;
    let exe = if cfg!(windows) { format!("{id}.exe") } else { id.to_string() };
    // The first candidate we could not stat, kept so the caller can name the real cause instead of
    // reporting an absence we never established.
    let mut unreadable: Option<(PathBuf, String)> = None;
    let mut probe = |p: PathBuf| -> Option<PathBuf> {
        match classify_sidecar_candidate(p.try_exists()) {
            SidecarCandidate::Present => Some(p),
            SidecarCandidate::Absent => None,
            SidecarCandidate::Unreadable(cause) => {
                if unreadable.is_none() {
                    unreadable = Some((p, cause));
                }
                None
            }
        }
    };

    if let Ok(resource) = app.path().resource_dir() {
        if let Some(p) = probe(resource.join("sidecars").join(&exe)) {
            return SidecarBinLookup::Found(p);
        }
    }
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    for profile in ["debug", "release"] {
        for base in [
            manifest.join(format!("../sidecar/{id}/target")),
            PathBuf::from(format!("sidecar/{id}/target")),
        ] {
            if let Some(p) = probe(base.join(profile).join(&exe)) {
                return SidecarBinLookup::Found(p);
            }
        }
    }
    match unreadable {
        Some((path, cause)) => SidecarBinLookup::Unreadable { path, cause },
        None => SidecarBinLookup::Missing,
    }
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
                binary_ok: resolve_sidecar_bin(&app, &m.id).found().is_some(),
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
    // CPE-1696: an unreadable candidate used to be reported with the same "binary is missing — reinstall
    // required" line as a genuinely absent one. A locked/half-installed exe is the far more common cause
    // (CPE-483, [[install-kill-all-processes-first]]) and a reinstall is exactly the wrong next step for
    // it, so the two now read differently. `binary_ok` keeps its meaning (not launchable either way).
    let lookup = resolve_sidecar_bin(&app, &id);
    let binary_ok = lookup.found().is_some();
    if let Some(action) = sidecar_binary_action(&lookup) {
        actions.push(action);
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

/// What `sidecar_close_all_sessions` actually did (CPE-1621 F1). The caller (`App.svelte`'s
/// `closeAllConsoles`) uses this to decide whether it may honestly clear the Agents leaves — a bare
/// `Ok(())` used to mean both "genuinely closed" and "console wasn't running, nothing happened" alike,
/// so the UI cleared its leaves on both, even though the second case reaches every daemon-backed
/// session that survives a UI-sidecar restart (CPE-464/CPE-309 S4) and leaves it running untouched.
#[cfg(feature = "sidecar-platform")]
#[derive(Clone, Copy, PartialEq, serde::Serialize)]
#[cfg_attr(feature = "specta-bindings", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
enum CloseAllOutcome {
    /// The console was running (`state.url` was `Some`) and `POST /api/close-all` returned success —
    /// every session's `SessionIo` was genuinely asked to end, including any daemon-backed one.
    Closed,
    /// No console was running (`state.url` was `None`), so there was no loopback server to reach at
    /// all. This is NOT proof there was nothing to close — the Agents leaves are a client-side store
    /// that persists independently of the live connection (CPE-461 reattach design) — so the caller
    /// must only treat this as "genuinely nothing to close" when it independently knows the leaf list
    /// is already empty; otherwise it's "couldn't reach anything to close" wearing an `Ok`.
    Nothing,
}

/// Close **every** running AI Console session at once (CPE-1621) — the main-window/sidebar "Close all
/// consoles" path's real fan-out teardown. Routes to the console's own `POST /api/close-all` over its
/// loopback UI server, the SAME endpoint the in-console "Close all" button already uses
/// (`sidecar/ai-console/src/launcher.html`) — `ConsoleState::close_all` there kills each session's
/// `SessionIo` regardless of whether it's a `LocalIo` or a `DaemonIo` (the production case once a
/// session daemon is running), so this genuinely reaches the host-owned session-daemon process's own
/// PTYs, not just the console UI's local bookkeeping. Must be called BEFORE `sidecar_stop` drops the
/// connection/URL (CPE-464) — once that happens there is nothing left to reach. Deliberately does NOT
/// touch `AiConsoleState.daemon`: the session daemon process itself is left running (empty), matching
/// its documented "outlives a UI-sidecar restart" design (see `AiConsoleState::daemon`'s doc comment)
/// — this only ends every session inside it.
///
/// Returns `Ok(CloseAllOutcome::Nothing)` rather than a bare `Ok(())` when the console isn't running
/// (F1 fix): before this, that no-op was indistinguishable from a genuine close on the wire, and
/// `App.svelte` cleared every Agents leaf either way — silently lying about a kill it never performed
/// whenever `state.url` had gone `None` (e.g. after `sidecar_repair`, or a crashed-but-not-yet-stopped
/// console) while leaves for daemon-backed sessions were still showing. A POST failure/timeout still
/// surfaces as `Err` unchanged, for the same reason.
#[cfg(feature = "sidecar-platform")]
#[tauri::command]
#[cfg_attr(feature = "specta-bindings", specta::specta)]
fn sidecar_close_all_sessions(state: tauri::State<AiConsoleState>) -> Result<CloseAllOutcome, String> {
    let url = { state.url.lock().map_err(|_| "state lock poisoned")?.clone() };
    let Some(base) = url else { return Ok(CloseAllOutcome::Nothing) }; // console not running
    let target = format!("{}/api/close-all", base.trim_end_matches('/'));
    ureq::post(&target)
        .timeout(std::time::Duration::from_secs(5))
        .call()
        .map_err(|e| format!("close all sessions failed: {e}"))?;
    state.log(sidecar_host::observability::LogLevel::Info, "closed all sessions by user");
    Ok(CloseAllOutcome::Closed)
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
///
/// CPE-1658: on Windows, killing `child` here also reclaims the one hidden `conhost.exe` Windows
/// allocated for this process's own (`CREATE_NO_WINDOW`) console — see the doc comment on
/// [`AiConsoleState::ensure_session_daemon`] for the process-level evidence that this conhost belongs
/// to the daemon, not to any one agent session, and so cannot (and should not) be reclaimed by a
/// session-level `close_all` while the daemon is still meant to be running.
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
        // CPE-1696: the closure used to be `path.exists().then(|| path.is_dir())`, which folds every
        // `stat` failure into `None` — and `None` means `Removed`, i.e. a transient permission/mount/IO
        // blip during a debounce window TOMBSTONED a file that still existed, silently dropping it from
        // search results. `stat_touched` returns a three-state `TouchedState`; `resolve_touched` emits no
        // event at all for `Unknown`, leaving the index entry intact.
        events.extend(cpe_server::index_watch::resolve_touched(
            &touched_paths,
            cpe_server::index_watch::stat_touched,
        ));
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
    ///
    /// CPE-1658: on Windows, `CREATE_NO_WINDOW` suppresses the window but Windows still allocates the
    /// daemon process a real (just hidden) console + **one** `conhost.exe` to host it — this is what
    /// lets the daemon's own ConPTY calls work at all. That `conhost.exe` is a direct child of the
    /// daemon's own pid, created the instant the daemon starts (before any session is launched), and
    /// is *not* part of any individual session's process tree — confirmed with a real process-tree
    /// capture (`Get-CimInstance Win32_Process`, walking `ParentProcessId`) that spawned this daemon
    /// exactly as below, then launched + `close_all`-ed a session: the per-session
    /// `conhost.exe --headless` (that session's own ConPTY host) and its whole `cmd.exe`/agent tree die
    /// within ~1s of `close_all` as expected, while the daemon's own `conhost.exe` — present since
    /// *before* the session even existed — is untouched, because it belongs to the daemon, not the
    /// session. It does **not** accumulate per session or per close-all (there is exactly one, for the
    /// daemon's whole lifetime); it is reclaimed the moment the daemon process itself dies, either via
    /// `Drop for HostSessionDaemon` below (app exit) or CPE-483's orphan sweep on next startup — also
    /// confirmed in the same repro by killing the spawned daemon pid and observing its conhost die with
    /// it. Freeing it early while the daemon keeps running would tear down the very console ConPTY
    /// needs for *future* sessions launched through this same daemon, which CPE-1621 established is
    /// meant to survive "Close all consoles" — so this is Windows-owned, tied to the daemon's own
    /// lifetime by design, not a per-console leak. Won't-fix, per CPE-1658's own documented-reason
    /// carve-out.
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
            // "ai-console" is the id the host spawned — the Hello must echo it back (CPE-1472).
            handshake(&mut conn, CONTRACT_VERSION, &consented, "ai-console", Some(&token))
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
            // "agent-board" is the id the host spawned — the Hello must echo it back (CPE-1472).
            handshake(&mut conn, CONTRACT_VERSION, &consented, "agent-board", Some(&token))
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

/// **This one keeps `fs::write` on purpose, and does NOT get CPE-1716's `replace_file_contents`
/// (CPE-1725).** The two whole-file save paths (`write_file_text`, `metadata_write`) *resolve* a symlink
/// and edit the file it points at, because there "the file the user opened" is at the far end of the link.
/// Here the contract is the opposite: `is_safe_repo_relative` exists so a conflict resolution can never
/// write outside the repo, and a symlink inside a working tree can point anywhere — following one is
/// precisely how a repo-confined write stops being repo-confined. So resolution is not merely unnecessary
/// here, it would defeat the guard on the line above the write.
///
/// What that leaves unguarded is stated rather than implied: a link at `<repo>/<file>` is still *followed*
/// by `fs::write` today, so a resolution staged onto one lands at the link's target (and a dangling one has
/// its target created). The right shape for a name being claimed under a containment guarantee is CPE-1718's
/// `create_slot_refusal`, not resolution. It is not added here because this command is behind
/// `sidecar-platform`, its input is a git-reported conflicted path rather than a path the user typed, and
/// CPE-1725's scope was the two save paths; recorded so the absence is a decision on the record and not an
/// oversight.
///
/// (Kept on the impl rather than the `#[tauri::command]` above deliberately: specta copies a command's doc
/// comment verbatim into `src/lib/bindings.gen.ts`, so an internal design note there ships to the frontend
/// bindings and shows up as generated-file drift.)
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
            .plugin(tauri_plugin_global_shortcut::Builder::new().build())
            // Native OS drag-out plumbing (CPE-1264, epic CPE-661 follow-on for CPE-672/674): registers
            // `plugin:drag|start_drag` so `src/lib/dragOut.ts` can start a real OS-level file drag.
            // Plumbing only in this slice — no row calls it yet, so this is a no-op until a later
            // ticket wires it into FileList/Sidebar dragstart. Desktop-only, same as the block above.
            .plugin(tauri_plugin_drag::init());
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

    // Terminal dock (CPE-1242/947, epic CPE-714): the tab model + the live PTY session registry. Always
    // registered — the docked terminal is a core-explorer feature, not sidecar — but off means off: an
    // empty dock is zero tabs, and an empty PTY registry is zero shells/threads, until a panel is opened.
    builder = builder.manage(cpe_server::terminal_tabs::TerminalDockState::default());
    builder = builder.manage(pty::PtyRegistry::default());

    // Encrypted-vault unlocked-session state (CPE-1248, epic CPE-738): the blob→session-dir map behind
    // vault unlock/lock. Always registered — vaults are a core-explorer feature — but empty until a vault
    // is unlocked, so it costs nothing (no plaintext, no session dir) when no vault is open.
    builder = builder.manage(cpe_server::vault_manager::VaultRegistry::default());

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

            // Close-to-tray (CPE-1272, epic CPE-713): when the Settings toggle `cpe.closeToTray` is on,
            // a window close HIDES the window (leaving the app tray-resident) instead of quitting; Quit
            // stays available from the tray menu. Default off — the flag is read fresh on each close, so
            // a plain close still quits unless the user opted in. Desktop-only (this whole block is).
            //
            // Critically, hide-to-tray is ALSO gated on a tray icon actually existing: `tray::setup`
            // fails non-fatally (e.g. a Linux DE with no notification-area host), and hiding without a
            // tray would strand the window with no way to restore it and no tray Quit. If there's no
            // tray, we fall through to a normal close/quit so the user is never trapped.
            {
                let handle = app.handle().clone();
                let win_for_close = win.clone();
                win.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        let tray_present = handle.tray_by_id(tray::TRAY_ID).is_some();
                        if tray::should_hide_to_tray(tray_present, tray::close_to_tray_enabled(&handle)) {
                            api.prevent_close();
                            let _ = win_for_close.hide();
                        }
                    }
                });
            }

            // `main` is in the window-state plugin's skip_initial_state list, so restore its saved
            // geometry here (deterministic) BEFORE the CLI flags override it — restore then override.
            let _ = win.restore_state(StateFlags::all());
            apply_cli_geometry(app.handle());

            // System-tray icon + quick-access menu + show/hide (CPE-1272, epic CPE-713). Built after the
            // window exists so its click/menu handlers can toggle/show it. Reuses the bundled app icon, so
            // no new bundle resource (CPE-1271 guard stays green). A failure here is logged, not fatal —
            // the explorer still works without a tray.
            if let Err(e) = tray::setup(app) {
                eprintln!("tray: could not create system-tray icon: {e}");
            }
        }
        #[cfg(feature = "sidecar-platform")]
        reap_orphan_session_daemons_on_startup(app.handle());
        // Self-heal a missing/stale sidecar from its pristine copy (CPE-867 L2) — after the reap, so a
        // daemon that was file-locking the exe is gone before we rewrite it.
        #[cfg(feature = "sidecar-platform")]
        restore_stale_sidecars_on_startup(app.handle());

        // Securely wipe any orphaned encrypted-vault session dir left behind by a crash/kill while a
        // vault was unlocked (CPE-1252, VAULT-SECURITY.md §5). Core-explorer feature, not sidecar-gated.
        sweep_orphan_vault_sessions_on_startup(app.handle());

        // Tell the PDF/video thumbnail extractors where their bundled native binaries actually landed
        // (CPE-1258 fix) — BEFORE anything can request a thumbnail. Core-explorer feature, not
        // sidecar-gated (pdf-thumb/video-thumb are unconditionally on for cpe-server).
        init_thumbnail_native_dep_dir(app.handle());

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
            is_high_contrast_active,
            parent_dir,
            list_drives,
            list_network_shares,
            disconnect_network_share,
            discover_network_windows,
            discover_network_mdns,
            disk_space,
            special_folders,
            create_dir,
            read_file_text,
            read_log_window,
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
            binary_info,
            binary_disasm,
            dotnet_metadata,
            data_browser_sources,
            data_browser_page,
            data_browser_query,
            email_preview,
            ical_preview,
            vcard_preview,
            jwt_preview,
            cert_decode,
            cert_create,
            cert_issue_from_csr,
            read_model_info,
            read_image_data_url,
            read_raw_preview_data_url,
            read_dicom_image_data_url,
            read_dicom_tags,
            read_heic_preview_data_url,
            read_pdf_validity,
            audio_waveform_peaks,
            thumbnail,
            thumbnails_stream,
            cancel_thumbnails_stream,
            read_settings,
            write_settings,
            tray_note_folder,
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
            #[cfg(any(target_os = "windows", target_os = "linux"))]
            list_trash,
            #[cfg(any(target_os = "windows", target_os = "linux"))]
            list_trash_stream,
            #[cfg(any(target_os = "windows", target_os = "linux"))]
            restore_trash_items,
            #[cfg(any(target_os = "windows", target_os = "linux"))]
            empty_trash,
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
            split_file,
            join_files,
            scan_tree,
            create_symlink,
            create_hard_link,
            create_junction,
            link_status,
            suggest_repair,
            install_shell_integration,
            uninstall_shell_integration,
            shell_integration_installed,
            set_default_file_manager,
            unset_default_file_manager,
            default_file_manager_status,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            register_spotlight_hotkey,
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            unregister_spotlight_hotkey,
            drive_type,
            drive_ejectable,
            eject_drive,
            audit_record,
            audit_sessions,
            audit_read,
            metrics_record,
            metrics_history,
            replay_load,
            checkpoint_create,
            checkpoint_list,
            checkpoint_record_failure,
            checkpoint_failures_list,
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
            diff_images,
            find_duplicates,
            find_duplicates_stream,
            find_similar_images,
            find_similar_images_stream,
            find_similar_documents,
            find_similar_folders,
            analyze_archive_safety,
            find_empty_dirs,
            find_orphan_sidecars,
            find_orphan_sidecars_stream,
            cancel_orphan_sidecars_stream,
            find_dangling_links,
            find_dangling_links_stream,
            cancel_dangling_links_stream,
            find_type_mismatches,
            find_type_mismatches_stream,
            cancel_type_mismatches_stream,
            organize_plan,
            organize_clutter,
            organize_apply,
            index_build,
            index_search,
            index_search_collect,
            index_status,
            index_drop,
            index_clear,
            content_index_build,
            content_search,
            content_embedder_set_key,
            content_embedder_has_key,
            content_embedder_test,
            copilot_plan,
            copilot_execute,
            copilot_set_key,
            copilot_has_key,
            copilot_test,
            git_remote_url,
            open_external,
            run_as_admin,
            extract_archive_entry,
            extract_archive_entry_any,
            extract_rar_entry,
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
            sidecar_close_all_sessions,
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
            forge_conflict_abort,
            terminal_dock_open,
            terminal_dock_close,
            terminal_dock_activate,
            terminal_dock_set_cwd,
            terminal_dock_tabs,
            terminal_dock_active,
            open_pty,
            write_pty,
            resize_pty,
            close_pty,
            vault_is,
            vault_create,
            vault_unlock,
            vault_lock,
            vault_status,
            vault_remember_passphrase,
            vault_forget_passphrase,
            connection_secret_set,
            connection_secret_get,
            connection_secret_delete,
            connections_list,
            connections_upsert,
            connections_remove
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(move |app_handle, event| {
        // Owning `keep_awake` here keeps the assertion alive for the entire run
        // loop; it is dropped (and the screen lock re-enabled) when the loop
        // ends. The reference just anchors the capture — see the comment above.
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        let _ = &keep_awake;

        // App-quit PTY sweep (CPE-1244): kill every still-open terminal dock shell so quitting with
        // tabs open never orphans them. `RunEvent::Exit` fires once, after every window has closed and
        // the run loop is about to end -- the right (and only) point to sweep state that outlives any
        // one window. `PtyRegistry` is always managed (see above), so `state()` never panics here.
        if let tauri::RunEvent::Exit = event {
            use tauri::Manager;
            app_handle.state::<pty::PtyRegistry>().close_all();
        }
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
    ///
    /// **CPE-1693 (PR #934 re-review, the 69th helper).** Returns the owning [`ScratchDir`] alongside the
    /// watch. This helper was invisible to the first census because that filtered on *return type* and
    /// this returns a domain struct, not a path — and it was worse than the 68th: `sched_scratch` at least
    /// had a trailing `remove_dir_all` at its call sites, so it leaked only on a panicking assertion,
    /// whereas nothing anywhere removed these. Measured: two green tests left 6 directories behind.
    /// The old `cpe1099-` name also fell outside the purge script's `cpe-*` filter, so the backlog it
    /// built was unpurgeable as well as uncounted; the new prefix brings it back in scope.
    ///
    /// The caller must keep the guard alive for at least as long as the watch — see the drop-order note
    /// at each call site.
    fn make_watch(tag: &str) -> (cpe_server::fsutil::ScratchDir, AgentWatch) {
        use notify::{RecursiveMode, Watcher};
        let dir = cpe_server::fsutil::scratch_dir(&format!("cpe-agent-watch-{tag}"));
        let (tx, _rx) = std::sync::mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |res| {
            let _ = tx.send(res);
        })
        .unwrap();
        watcher.watch(&dir, RecursiveMode::Recursive).unwrap();
        let watch = AgentWatch { _watcher: watcher, path: dir.to_string_lossy().into_owned() };
        (dir, watch)
    }

    #[test]
    fn arming_two_sessions_then_stop_all_leaves_zero_watchers() {
        // CPE-1693: declared FIRST so it drops LAST — locals drop in reverse declaration order, so every
        // scratch directory outlives `state` and therefore every `notify` watcher holding a handle on it.
        // Removing a directory out from under a live watcher is what would make this flaky on Windows.
        let mut _dirs = Vec::new();
        let state = AgentWatchState::default();
        // Off means off: nothing armed ⇒ empty map ⇒ zero watcher/pump threads.
        assert!(state.is_empty());

        // Arm two distinct sessions — both are watched concurrently (keyed by sessionId).
        let (d, w) = make_watch("s1");
        _dirs.push(d);
        state.arm("s1".into(), w);
        let (d, w) = make_watch("s2");
        _dirs.push(d);
        state.arm("s2".into(), w);
        assert_eq!(state.armed_count(), 2);

        // Re-arming an existing key replaces (drops) its prior watch, not add a second — still 2.
        let (d, w) = make_watch("s1b");
        _dirs.push(d);
        state.arm("s1".into(), w);
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
    ///
    /// **CPE-1693 (PR #934 re-review, the 70th helper)** — same fix, same reason as `make_watch` above:
    /// it returned a domain struct so the return-type census could not see it, nothing removed its
    /// `cpe1138-` directories on any code path, and that prefix was outside the purge script's `cpe-*`
    /// filter.
    fn make_index_watch(tag: &str) -> (cpe_server::fsutil::ScratchDir, IndexWatch) {
        use notify::{RecursiveMode, Watcher};
        let dir = cpe_server::fsutil::scratch_dir(&format!("cpe-index-watch-{tag}"));
        let (tx, _rx) = std::sync::mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |res| {
            let _ = tx.send(res);
        })
        .unwrap();
        watcher.watch(&dir, RecursiveMode::Recursive).unwrap();
        let watch = IndexWatch { _watcher: watcher, root: dir.to_string_lossy().into_owned() };
        (dir, watch)
    }

    #[test]
    fn arming_two_volumes_then_stop_all_leaves_zero_watchers() {
        // CPE-1693: declared FIRST so it drops LAST, after `state` and therefore after every watcher —
        // see the same note in `arming_two_sessions_then_stop_all_leaves_zero_watchers`.
        let mut _dirs = Vec::new();
        let state = IndexWatchState::default();
        // Off means off: nothing armed ⇒ empty map ⇒ zero watcher/pump threads.
        assert_eq!(state.armed_count(), 0);

        // Arm two distinct volumes — both watched concurrently (keyed by volume_id).
        let (d, w) = make_index_watch("v1");
        _dirs.push(d);
        state.arm(1, w);
        let (d, w) = make_index_watch("v2");
        _dirs.push(d);
        state.arm(2, w);
        assert_eq!(state.armed_count(), 2);

        // Re-arming an already-watched volume (a rebuild) replaces its watch, not adds a second.
        let (d, w) = make_index_watch("v1b");
        _dirs.push(d);
        state.arm(1, w);
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
    // Deliberately EXCLUDED: the single-OS-behavior commands `set_permissions` (`#[cfg(unix)]`),
    // `set_file_attribute` (real impl `#[cfg(windows)]` / error stub `#[cfg(not(windows))]`), and
    // `discover_network_windows` (real impl `#[cfg(windows)]` / empty-Vec stub `#[cfg(not(windows))]`,
    // CPE-1519) — even where a same-shaped stub exists on the rest, the ACTUAL behavior is one
    // platform's, so including them would make `bindings.gen.ts` semantics OS-dependent and risk the
    // drift guard (CPE-813), which regenerates on Linux. Callers use raw `invoke` for these.
    // `discover_network_mdns` (CPE-1523) is, by contrast, INCLUDED — mDNS/DNS-SD behaves identically on
    // every OS (not `#[cfg(windows)]`-gated), so its binding is safe to generate uniformly.
    // `collect_commands!` (below) is a plain `macro_rules!`, unlike `tauri::generate_handler!` (a real
    // proc macro) — it can't take a per-entry `#[cfg(...)]` the way the `generate_handler!` call above
    // does for these same four commands (CPE-1559). Splice the OS-gated trash commands in via a local
    // wrapper macro instead of duplicating this ~280-command list across two `#[cfg]` branches.
    macro_rules! specta_commands {
        ($($trash:ident),* $(,)?) => {
            collect_commands![
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
        is_high_contrast_active,
        parent_dir,
        list_drives,
        list_network_shares,
        disconnect_network_share,
        discover_network_mdns,
        disk_space,
        special_folders,
        create_dir,
        read_file_text,
        read_log_window,
        read_file_range,
        code_intel,
        file_len,
        set_readonly,
        set_file_times,
        read_attributes,
        write_file_text,
        read_archive_entries,
        read_preview_info,
        binary_info,
        binary_disasm,
        dotnet_metadata,
        data_browser_sources,
        data_browser_page,
        data_browser_query,
        email_preview,
        ical_preview,
        vcard_preview,
        jwt_preview,
        cert_decode,
        cert_create,
        cert_issue_from_csr,
        read_model_info,
        read_image_data_url,
        read_raw_preview_data_url,
        read_dicom_image_data_url,
        read_dicom_tags,
        read_heic_preview_data_url,
        read_pdf_validity,
        audio_waveform_peaks,
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
        // `list_trash`/`list_trash_stream`/`restore_trash_items`/`empty_trash` only exist under
        // `#[cfg(any(windows, linux))]` (CPE-1558, no macOS equivalent). Spliced in by the
        // `specta_commands!` caller below so this list still compiles wherever `export_bindings` is
        // built, instead of hard-coding them here and breaking a macOS build.
        $($trash,)*
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
        split_file,
        join_files,
        scan_tree,
        create_symlink,
        create_hard_link,
        create_junction,
        link_status,
        suggest_repair,
        install_shell_integration,
        uninstall_shell_integration,
        shell_integration_installed,
        set_default_file_manager,
        unset_default_file_manager,
        default_file_manager_status,
        register_spotlight_hotkey,
        unregister_spotlight_hotkey,
        drive_type,
        drive_ejectable,
        eject_drive,
        audit_record,
        audit_sessions,
        audit_read,
        metrics_record,
        metrics_history,
        replay_load,
        checkpoint_create,
        checkpoint_list,
        checkpoint_record_failure,
        checkpoint_failures_list,
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
        diff_images,
        find_duplicates,
        find_duplicates_stream,
        find_similar_images,
        find_similar_images_stream,
        find_similar_documents,
        find_similar_folders,
        analyze_archive_safety,
        find_empty_dirs,
        find_orphan_sidecars,
        find_orphan_sidecars_stream,
        cancel_orphan_sidecars_stream,
        find_dangling_links,
        find_dangling_links_stream,
        cancel_dangling_links_stream,
        find_type_mismatches,
        find_type_mismatches_stream,
        cancel_type_mismatches_stream,
        organize_plan,
        organize_clutter,
        organize_apply,
        index_build,
        index_search,
        index_search_collect,
        index_status,
        index_drop,
        index_clear,
        content_index_build,
        content_search,
        content_embedder_set_key,
        content_embedder_has_key,
        content_embedder_test,
        copilot_plan,
        copilot_execute,
        copilot_set_key,
        copilot_has_key,
        copilot_test,
        git_remote_url,
        open_external,
        run_as_admin,
        extract_archive_entry,
        extract_archive_entry_any,
        extract_rar_entry,
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
        sidecar_close_all_sessions,
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
        terminal_dock_open,
        terminal_dock_close,
        terminal_dock_activate,
        terminal_dock_set_cwd,
        terminal_dock_tabs,
        terminal_dock_active,
        open_pty,
        write_pty,
        resize_pty,
        close_pty,
        vault_is,
        vault_create,
        vault_unlock,
        vault_lock,
        vault_status,
        vault_remember_passphrase,
        vault_forget_passphrase,
        connection_secret_set,
        connection_secret_get,
        connection_secret_delete,
        connections_list,
        connections_upsert,
        connections_remove,
            ]
        };
    }
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    let trash_commands = specta_commands!(list_trash, list_trash_stream, restore_trash_items, empty_trash);
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    let trash_commands = specta_commands!();
    let builder = Builder::<tauri::Wry>::new().commands(trash_commands)
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
            // CPE-1559 (epic CPE-1486 slice 2): the trash browse/restore/empty commands (CPE-1558) must be
            // reachable through the typed client too, not just `generate_handler!`.
            "listTrash", "listTrashStream", "restoreTrashItems", "emptyTrash",
        ] {
            assert!(src.contains(cmd), "typed client should expose `{cmd}`");
        }
        // The cross-crate cpe-server types now flow into the generated client.
        assert!(src.contains("DirEntry") && src.contains("OpResult") && src.contains("EntryInfo"),
            "cpe-server types should be exported into the typed client");
        assert!(src.contains("TrashEntry"), "TrashEntry DTO should be exported into the typed client");
    }

    // ---- `filtered` count survives provider → RemoteListing → command → frontend (CPE-1708) ----------
    // CPE-1704 built the count (a `RemoteListing::filtered` `usize`, computed in-process from what the
    // provider's own listing pass + the shared name guard actually dropped — never reconstructable from
    // wire data) but deliberately left it at the Tauri boundary as an `eprintln!`. This is that count's
    // missing last mile: `list_dir`/`list_dir_stream` now carry it to the frontend as a typed field
    // (`ListDirResult`/`StreamDirResult`), never as a synthetic row mixed into `entries` (CPE-1704 round
    // 2's rejected approach — see `ListDirResult`'s doc in `crates/server/src/model.rs`).
    //
    // Each link below is a DISTINCT test, per the Evidence Rules in `Ticketing/wiki.md`: breaking any ONE
    // of them must turn exactly one of these red, not silently pass through the others.

    /// A minimal `FileSystemProvider` double (mirrors `crates/vfs/src/connect.rs`'s `Hostile` test
    /// fixture) that hands back two names the shared guard genuinely must refuse (a literal `..` and an
    /// embedded-traversal `../escape`) alongside one safe leaf — so `cpe_vfs::connect::remote_dir_entries`
    /// (the REAL production function, not a stand-in) has something real to filter.
    struct HostileListing;
    impl cpe_server::provider::FileSystemProvider for HostileListing {
        fn list(&self, _p: &str) -> Result<Vec<cpe_server::provider::ProviderEntry>, String> {
            Ok(vec![
                cpe_server::provider::ProviderEntry { name: "..".into(), is_dir: true, size: 0 },
                cpe_server::provider::ProviderEntry { name: "../escape".into(), is_dir: false, size: 1 },
                cpe_server::provider::ProviderEntry { name: "ok.txt".into(), is_dir: false, size: 7 },
            ])
        }
        fn stat(&self, _p: &str) -> Result<cpe_server::provider::ProviderEntry, String> { unreachable!() }
        fn read(&self, _p: &str) -> Result<Vec<u8>, String> { unreachable!() }
        fn write(&mut self, _p: &str, _d: &[u8]) -> Result<(), String> { unreachable!() }
        fn mkdir(&mut self, _p: &str) -> Result<(), String> { unreachable!() }
        fn delete(&mut self, _p: &str) -> Result<(), String> { unreachable!() }
        fn rename(&mut self, _f: &str, _t: &str) -> Result<(), String> { unreachable!() }
    }

    /// Link 1→2 (provider → `RemoteListing`) THROUGH link 2→3 (`RemoteListing` → `ListDirResult`, the
    /// `list_dir` command's own mapping): calls the real `remote_dir_entries` against `HostileListing`,
    /// then feeds the real `listing_to_result` — the exact function `list_dir`'s remote arm calls — and
    /// asserts the count comes out the other side unchanged. Breaking EITHER `remote_dir_entries`'s
    /// filtering or `listing_to_result`'s field mapping turns this red.
    #[test]
    fn filtered_count_survives_remote_dir_entries_into_list_dir_result() {
        let listing = cpe_vfs::connect::remote_dir_entries(&HostileListing, "sftp://h/x").unwrap();
        assert_eq!(listing.filtered, 2, "sanity: the provider fixture must actually produce 2 unsafe names");
        let result = listing_to_result(listing);
        assert_eq!(result.entries.len(), 1, "only the one safe leaf reaches the entries the UI renders");
        assert_eq!(result.entries[0].name, "ok.txt");
        assert_eq!(result.filtered, 2, "list_dir's ListDirResult must carry the real count through, not 0");
    }

    /// The streaming twin of the test above: `stream_result_for` is the exact function
    /// `remote_list_dir_stream_impl` calls (computed BEFORE `entries` is drained into channel batches).
    /// This is a DISTINCT test from the one above — breaking `stream_result_for` alone (leaving
    /// `listing_to_result` correct) must turn only this one red, proving `list_dir_stream` isn't silently
    /// riding on `list_dir`'s own correctness.
    #[test]
    fn filtered_count_survives_remote_dir_entries_into_stream_dir_result() {
        let listing = cpe_vfs::connect::remote_dir_entries(&HostileListing, "sftp://h/x").unwrap();
        let result = stream_result_for(&listing);
        assert_eq!(result.total, 1, "one safe leaf streamed");
        assert_eq!(result.filtered, 2, "list_dir_stream's StreamDirResult must carry the real count through");
    }

    /// Link 3→4 (command → frontend): what actually crosses the Tauri IPC wire is `serde_json`, keyed by
    /// field name — this is what `bindings.gen.ts`'s generated TS interface (and the frontend code that
    /// reads `.filtered`) actually depends on. Proves the wire shape directly rather than trusting the
    /// struct definition alone; breaking a field rename here (without updating the frontend) is exactly
    /// the class of drift CI's Typed-bindings guard also independently catches.
    #[test]
    fn list_dir_result_and_stream_dir_result_serialize_the_filtered_field_by_name() {
        let ldr = ListDirResult { entries: vec![], filtered: 3 };
        let v = serde_json::to_value(&ldr).unwrap();
        assert_eq!(v["filtered"], 3, "ListDirResult must serialize `filtered` under that exact key");
        assert_eq!(v["entries"], serde_json::json!([]));

        let sdr = StreamDirResult { total: 5, filtered: 2 };
        let v = serde_json::to_value(&sdr).unwrap();
        assert_eq!(v["filtered"], 2, "StreamDirResult must serialize `filtered` under that exact key");
        assert_eq!(v["total"], 5);
    }

    /// AC: "Confirm every other provider (SFTP, WebDAV, FTP, local) reports zero and is unaffected."
    /// `list_with_filtered_count`'s DEFAULT (delegates to `list`, reports 0 — every provider except S3)
    /// is already covered by `crates/server/src/provider.rs`'s
    /// `list_with_filtered_count_defaults_to_delegating_to_list_and_reporting_zero` (CPE-1704); this test
    /// covers the ONE hop that's new here — `list_dir`'s LOCAL arm hardcodes `filtered: 0` rather than
    /// asking any provider at all, since a local listing was never routed through `RemoteListing`.
    #[test]
    fn list_dir_local_arm_always_reports_zero_filtered() {
        // CPE-1693: `scratch()` now returns the guard itself, armed before any assertion below can
        // panic and skip it — so CPE-1708's hand-rolled `Cleanup(PathBuf)` wrapper is redundant and has
        // been dropped in the rebase. This is exactly the per-test boilerplate the helper-level fix
        // exists to delete.
        let dir = scratch("cpe1708-local-filtered");
        fs::write(dir.join("a.txt"), b"hi").unwrap();

        let result = local_list_dir_result(dir.to_str().unwrap()).expect("local listing succeeds");
        assert_eq!(result.filtered, 0, "a local listing has no name-guard filtering to report");
        assert_eq!(result.entries.len(), 1);
    }

    // ---- Safety-scan command smoke tests (CPE-1287, epic CPE-1002) ----------------------------------
    // Confirms each thin command actually dispatches into its `cpe-server` adapter and comes back with a
    // sane `Ok` result — the adapters themselves are exhaustively tested in their own crate; this is just
    // the wiring (String->Path conversion, spawn_blocking, error mapping, command registration).

    #[test]
    fn safety_scan_commands_dispatch_into_their_adapters() {
        let d = std::env::temp_dir().join(format!("cpe1287_safety_scan_{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(d.join("empty_sub")).expect("scratch dir");
        fs::write(d.join("keep.txt"), b"hello world").unwrap();
        let root = d.to_string_lossy().into_owned();

        let archive = tauri::async_runtime::block_on(analyze_archive_safety(d.join("keep.txt").to_string_lossy().into_owned()))
            .expect("analyze_archive_safety dispatches");
        assert_eq!(archive.entries_scanned, 0, "keep.txt isn't a zip, so nothing to score");
        assert!(!archive.report.dangerous);
        assert!(archive.unreadable, "keep.txt isn't a valid zip, so it should be flagged unreadable (CPE-1320)");

        let empty = tauri::async_runtime::block_on(find_empty_dirs(root.clone(), Vec::new())).expect("find_empty_dirs dispatches");
        assert!(empty.dirs.iter().any(|p| Path::new(p) == d.join("empty_sub")), "empty_sub should be reported: {:?}", empty.dirs);

        let orphans =
            tauri::async_runtime::block_on(find_orphan_sidecars(root.clone(), false, Vec::new())).expect("find_orphan_sidecars dispatches");
        assert!(orphans.orphans.is_empty(), "keep.txt is not a sidecar type");
        assert_eq!(orphans.scanned, 1);

        let dangling = tauri::async_runtime::block_on(find_dangling_links(root.clone(), Vec::new())).expect("find_dangling_links dispatches");
        assert!(dangling.links.is_empty(), "no symlinks in the scratch tree");

        let mismatches =
            tauri::async_runtime::block_on(find_type_mismatches(root.clone(), Vec::new())).expect("find_type_mismatches dispatches");
        assert!(mismatches.hits.is_empty(), "plain text keep.txt has no content/extension mismatch");
        assert_eq!(mismatches.scanned, 1);

        let _ = fs::remove_dir_all(&d);
    }

    // ---- Streaming safety-scan commands (CPE-1299, epic CPE-1002) -----------------------------------
    // Confirms each `*_stream` command actually drives its `cpe-server` walker over a real
    // `tauri::ipc::Channel` — constructible standalone via `Channel::new` (no running app/webview
    // needed) — and delivers at least one batch, mirroring `list_dir_stream`'s Channel + stream_id +
    // cancel-registry shape. The tail `Result` always comes back with an empty collection (CPE-420's
    // `find_duplicates_stream` convention): the batches are what streamed.

    /// Collect every batch a `Channel<TSend>` receives into `out`, decoding the JSON body as a generic
    /// [`serde_json::Value`] array — the same wire shape a real frontend `Channel` gets from
    /// `on_message`. Decoding as `Value` (rather than the batch item's own Rust type) sidesteps
    /// `MismatchHit`/`DanglingLink` deriving `Serialize` only (no app-side need for `Deserialize`);
    /// `TSend` is only ever a phantom type parameter here, inferred from the command signature the
    /// `Channel` is passed into.
    fn collecting_channel<TSend>(
        out: std::sync::Arc<std::sync::Mutex<Vec<serde_json::Value>>>,
    ) -> tauri::ipc::Channel<TSend> {
        tauri::ipc::Channel::new(move |body| {
            if let tauri::ipc::InvokeResponseBody::Json(json) = body {
                if let Ok(serde_json::Value::Array(items)) = serde_json::from_str::<serde_json::Value>(&json) {
                    out.lock().unwrap().extend(items);
                }
            }
            Ok(())
        })
    }

    #[test]
    fn find_type_mismatches_stream_drives_the_walker_and_emits_a_batch() {
        let d = std::env::temp_dir().join(format!("cpe1299_mismatch_stream_{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).expect("scratch dir");
        // Real PE/MZ header disguised as a .jpg (same fixture as `type_mismatch_scan`'s own tests).
        fs::write(d.join("disguised.jpg"), [0x4D, 0x5A, 0x90, 0x00, 0x03, 0x00]).unwrap();
        let root = d.to_string_lossy().into_owned();

        let hits: std::sync::Arc<std::sync::Mutex<Vec<serde_json::Value>>> = Default::default();
        let on_hit = collecting_channel(hits.clone());
        let result = tauri::async_runtime::block_on(find_type_mismatches_stream(root, Vec::new(), 1, on_hit))
            .expect("find_type_mismatches_stream dispatches");

        assert_eq!(result.scanned, 1);
        assert!(result.hits.is_empty(), "hits stream over the channel, not in the tail result");
        let hits = hits.lock().unwrap();
        assert_eq!(hits.len(), 1, "the disguised PE must stream as one batch: {hits:?}");
        assert!(hits[0]["path"].as_str().unwrap().ends_with("disguised.jpg"));

        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn find_orphan_sidecars_stream_drives_the_walker_and_emits_a_batch() {
        let d = std::env::temp_dir().join(format!("cpe1299_orphan_stream_{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).expect("scratch dir");
        // A .srt with no matching video primary in the same folder is an orphaned sidecar.
        fs::write(d.join("movie.srt"), b"1\n00:00:00,000 --> 00:00:01,000\nhi\n").unwrap();
        let root = d.to_string_lossy().into_owned();

        let orphans: std::sync::Arc<std::sync::Mutex<Vec<serde_json::Value>>> = Default::default();
        let on_orphan = collecting_channel(orphans.clone());
        let result =
            tauri::async_runtime::block_on(find_orphan_sidecars_stream(root, false, Vec::new(), 1, on_orphan))
                .expect("find_orphan_sidecars_stream dispatches");

        assert_eq!(result.scanned, 1);
        assert!(result.orphans.is_empty(), "orphans stream over the channel, not in the tail result");
        let orphans = orphans.lock().unwrap();
        assert_eq!(orphans.len(), 1, "movie.srt has no primary, so it must stream as an orphan: {orphans:?}");
        assert!(orphans[0].as_str().unwrap().ends_with("movie.srt"));

        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn find_dangling_links_stream_drives_the_walker_and_emits_a_batch() {
        let d = std::env::temp_dir().join(format!("cpe1299_dangling_stream_{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).expect("scratch dir");
        let target = d.join("gone.txt");
        fs::write(&target, "will be deleted").unwrap();
        let link = d.join("broken_link");
        // Skip where symlink creation is unprivileged (Windows without Developer Mode / admin) — same
        // guard `recursive_walks_skip_symlinked_dirs_and_do_not_cycle` uses above.
        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_file(&target, &link).is_ok();
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(&target, &link).is_ok();
        if !made {
            let _ = fs::remove_dir_all(&d);
            return;
        }
        fs::remove_file(&target).unwrap();
        let root = d.to_string_lossy().into_owned();

        // `DanglingLink` derives `Serialize` only — decode the wire JSON as `Value` (same reasoning as
        // the type-mismatch test above).
        let links: std::sync::Arc<std::sync::Mutex<Vec<serde_json::Value>>> = Default::default();
        let on_link = collecting_channel(links.clone());
        let result = tauri::async_runtime::block_on(find_dangling_links_stream(root, Vec::new(), 1, on_link))
            .expect("find_dangling_links_stream dispatches");

        assert!(result.links.is_empty(), "links stream over the channel, not in the tail result");
        let links = links.lock().unwrap();
        assert_eq!(links.len(), 1, "the broken link must stream as one batch: {links:?}");
        assert!(links[0]["path"].as_str().unwrap().ends_with("broken_link"));

        let _ = fs::remove_dir_all(&d);
    }

    // ---- Background scheduled-snapshot timer (CPE-1199, epic CPE-735) -------------------------------
    // These exercise `snapshot_schedule_tick` — the per-wake body of the timer — over a pure
    // `HeadlessCtx` + injected clock (no tauri runtime, so this stays a pure-fs test per the note above).

    use cpe_server::ctx::HeadlessCtx;
    use cpe_server::{checkpoint_store, snapshot_schedule};
    use std::collections::BTreeMap;

    /// CPE-1693 (PR #934 review finding 1): the 68th scratch helper, missed by the sweep because it is
    /// spelled `sched_scratch`, not `scratch`. Now delegates to the shared `scratch_dir` and hands back
    /// the owning guard like every other one — same `cpe-sched-tick-<tag>-<pid>-<seq>` directory naming
    /// as before, since `scratch_dir` appends the `-<pid>-<seq>` half itself.
    fn sched_scratch(tag: &str) -> cpe_server::fsutil::ScratchDir {
        cpe_server::fsutil::scratch_dir(&format!("cpe-sched-tick-{tag}"))
    }

    /// **CPE-1693 PR #934 review finding 1.** The 68th scratch helper — the one the mechanical sweep
    /// missed, because it is spelled `sched_scratch` rather than `scratch` and so didn't match the
    /// transform's pattern. It sits in a crate the sweep *did* convert, three call sites deep, and every
    /// one of those tests leaks its directory on a failing assertion exactly the way this whole ticket
    /// is about.
    ///
    /// Same proof shape as `cpe_server::fsutil`'s: arm the helper, drop it on a panicking unwind, and
    /// check the directory is gone on the far side. This compiles either way — with a bare `PathBuf`
    /// (`to_path_buf` is `PathBuf`'s own method, `let _armed = dir;` just moves it) it fails on the last
    /// assertion, which is precisely the "returns a bare path with no `Drop` guard" bug.
    #[test]
    fn sched_scratch_removes_its_directory_even_when_the_caller_panics_mid_assertion() {
        let dir = sched_scratch("guard-proof");
        let path = dir.to_path_buf();
        assert!(path.is_dir(), "sanity: the helper must actually create a real directory");

        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _armed = dir;
            panic!("CPE-1693 proof: deliberate panic — sched_scratch's guard must already be armed");
        }))
        .is_err();

        assert!(panicked, "the proof only proves anything if the inner closure actually panicked");
        assert!(
            !path.is_dir(),
            "CPE-1693 REGRESSION: {} still exists after sched_scratch's value went out of scope on a \
             panicking unwind — this helper still hands back a bare path with no Drop guard",
            path.display()
        );
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
        // The Ticketing/ container (CPE-1128): status folders under Tickets/, Epics/ & Sprints/ as
        // siblings — and since CPE-1676 the Epics queue has its own status folders too, so the epic
        // sits one level deeper than it used to. The recursive search must still reach it.
        let t = root.join("Ticketing");
        for (dir, name) in [
            ("Tickets/Backlog", "CPE-10_a.md"),
            ("Epics/Backlog", "CPE-616_epic-remote.md"),
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

    /// CPE-1676: the Epics queue is five status folders and the **folder** is the status. The board
    /// must read epics at the new depth, take the status from the folder even when the file's own
    /// `status:` disagrees, ignore the folders' `wiki.md` explainers, and still surface the epics
    /// closed into `Tickets/Done/` before the migration.
    #[test]
    fn board_epics_reads_the_status_folders_and_trusts_the_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let epics = root.join("Ticketing").join("Epics");
        let write = |dir: &std::path::Path, name: &str, body: &str| {
            std::fs::create_dir_all(dir).unwrap();
            std::fs::write(dir.join(name), body).unwrap();
        };
        let epic_md = |id: &str, status: &str| {
            format!("---\nid: {id}\ntitle: \"EPIC: {id}\"\nstatus: {status}\ntags: [epic]\n---\nbody\n")
        };
        write(&epics.join("Backlog"), "CPE-1_epic-a.md", &epic_md("CPE-1", "Proposed"));
        // Stale frontmatter: the folder wins, so this reports as In Progress, not Proposed.
        write(&epics.join("Doing"), "CPE-2_epic-b.md", &epic_md("CPE-2", "Proposed"));
        write(&epics.join("Blocked"), "CPE-3_epic-c.md", &epic_md("CPE-3", "Blocked"));
        write(&epics.join("Deferred"), "CPE-4_epic-d.md", &epic_md("CPE-4", "Deferred"));
        write(&epics.join("Done"), "CPE-5_epic-e.md", &epic_md("CPE-5", "Done"));
        // Each folder's explainer is not an epic and must not appear.
        write(&epics.join("Backlog"), "wiki.md", "# Backlog\n");
        // A pre-CPE-1676 closed epic still parked in the status-flow's Done.
        write(&root.join("Ticketing/Tickets/Done"), "CPE-6_epic-old.md", &epic_md("CPE-6", "Done"));
        // A plain (non-epic) ticket in Done is not an epic.
        write(&root.join("Ticketing/Tickets/Done"), "CPE-7_plain.md", "---\nid: CPE-7\ntags: [ready]\n---\n");

        let mut got: Vec<(String, String)> = board_epics_impl(root.to_string_lossy().into_owned())
            .into_iter()
            .map(|e| (e.id, e.status))
            .collect();
        got.sort();
        assert_eq!(
            got,
            vec![
                ("CPE-1".to_string(), "Proposed".to_string()),
                ("CPE-2".to_string(), "In Progress".to_string()),
                ("CPE-3".to_string(), "Blocked".to_string()),
                ("CPE-4".to_string(), "Deferred".to_string()),
                ("CPE-5".to_string(), "Done".to_string()),
                ("CPE-6".to_string(), "Done".to_string()),
            ]
        );
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

    /// A minimal valid PNG, built with the `image` crate (already a real dependency) rather than
    /// hand-rolled bytes. Its only caller is the trash-restore test below, which is gated to Windows +
    /// Linux (macOS has no programmatic trash listing), so this helper carries the same `cfg`; without
    /// it the fn is dead code on macOS and `-D warnings` fails the build there (CPE-1268).
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    fn cpe_1194_test_png_bytes() -> Vec<u8> {
        let img = image::RgbImage::from_pixel(4, 4, image::Rgb([200u8, 40, 60]));
        let mut buf = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut buf, image::ImageFormat::Png)
            .unwrap();
        buf.into_inner()
    }

    /// Held for the duration of every test below that touches the **real** OS trash (CPE-1785). The OS
    /// trash is process-global — one shared XDG trash directory on Linux, one shared Recycle Bin on
    /// Windows — and every trash-touching test in this binary runs concurrently under the default
    /// multi-threaded harness otherwise. That produced exactly the failure this ticket is about:
    /// `empty_trash_purges_only_the_selected_probe_item` panicked *inside* the `trash` crate at
    /// `freedesktop.rs:140` (a `.trashinfo` line-parse `unwrap`), which is what happens when one test
    /// enumerates an entry that a concurrently-running sibling test has just removed or rewritten — a
    /// list-then-read race on shared state, not a bug in the assertion.
    ///
    /// On Linux this guard does more than serialise: it also redirects `XDG_DATA_HOME` (which the
    /// `trash` crate's `home_trash()` honours, see `freedesktop.rs:688-701`) to a private
    /// [`cpe_server::fsutil::scratch_dir`] for the guarded section's duration, so the tests get their
    /// OWN trash directory instead of merely taking turns with the shared one. That removes the sharing
    /// entirely rather than just scheduling around it, and — the same objection the round-trip probe's
    /// own comment raises about the Windows Recycle Bin — it also stops this suite from ever touching a
    /// real Linux developer's actual trash (and, as a side effect, fixes a hole that predates this
    /// ticket: on any machine where `/tmp` is its own mount — e.g. a tmpfs `/tmp` — these tests were
    /// already landing in the shared per-mount `.Trash-$uid` rather than the home trash, PR #940 review).
    /// `trash::os_limited` has no equivalent redirection knob on Windows (the Recycle Bin is addressed by
    /// drive, not by an env var), so there the mutex alone is the fix; Windows CI has not been observed
    /// to hit this race, but the shared resource is the same shape, so the tests are serialised there
    /// too rather than left exposed.
    ///
    /// **The mutex is not redundant with the redirect and may not be dropped as such.** It is the only
    /// available fix on Windows, and on Linux it is what makes the save/mutate/restore of the
    /// process-global `XDG_DATA_HOME` atomic against the ~183 other tests in this binary that read it
    /// indirectly via `std::env::temp_dir()` (`tempfile::tempdir()`, `cpe_server::fsutil::scratch_dir`
    /// itself) — without it, `setenv`'s possible `environ` reallocation on glibc races every concurrent
    /// `getenv` in the same process (PR #940 review).
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    fn lock_real_trash() -> TrashTestGuard<'static> {
        // Named to match `crates/server/src/shell_menu.rs`'s `HOME_ENV_LOCK` — same pattern (a named
        // poison-tolerant mutex guarding a process-global env var mutation), greppable as one family.
        static TRASH_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let mutex = TRASH_ENV_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        #[cfg(target_os = "linux")]
        {
            let scratch = cpe_server::fsutil::scratch_dir("cpe-1785-trash-xdg");
            let previous_xdg = std::env::var_os("XDG_DATA_HOME");
            // SAFETY: test-only, guarded by TRASH_ENV_LOCK above so no other test observes a torn value;
            // restored in `TrashTestGuard::drop` (still under the same lock) before any other test can
            // observe the overridden value. Matches `shell_menu.rs`'s `HOME_ENV_LOCK` pattern.
            unsafe { std::env::set_var("XDG_DATA_HOME", &scratch) };
            TrashTestGuard { _scratch: scratch, previous_xdg, _mutex: mutex }
        }
        #[cfg(not(target_os = "linux"))]
        {
            TrashTestGuard { _mutex: mutex }
        }
    }

    /// RAII guard returned by [`lock_real_trash`]. On Linux, dropping it restores `XDG_DATA_HOME` to
    /// whatever it was before (or removes it, if it was unset) — the restore runs before the mutex guard
    /// field drops, so the environment is back to normal before the next queued test can enter the
    /// critical section. Fields are declared `_scratch` (and `previous_xdg`) BEFORE `_mutex` so struct
    /// fields, which drop in declaration order, finish removing the redirected trash directory before
    /// the mutex releases — the next queued test doesn't start mutating a fresh scratch dir while this
    /// one's `remove_dir_all_with_retries` might still be retrying (PR #940 review nit).
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    struct TrashTestGuard<'a> {
        #[cfg(target_os = "linux")]
        _scratch: cpe_server::fsutil::ScratchDir,
        #[cfg(target_os = "linux")]
        previous_xdg: Option<std::ffi::OsString>,
        _mutex: std::sync::MutexGuard<'a, ()>,
    }

    #[cfg(any(target_os = "windows", target_os = "linux"))]
    impl TrashTestGuard<'_> {
        /// The private trash directory `XDG_DATA_HOME` was redirected to for this guard's lifetime.
        /// `None` on Windows, where no such redirection exists (see [`lock_real_trash`]'s doc comment).
        /// Used by [`trash_roundtrip_available`] to assert the redirect actually took effect (CPE-1785,
        /// PR #940 review, blocker 2) rather than trusting it on faith.
        fn scratch_path(&self) -> Option<&std::path::Path> {
            #[cfg(target_os = "linux")]
            {
                Some(self._scratch.path())
            }
            #[cfg(not(target_os = "linux"))]
            {
                None
            }
        }
    }

    #[cfg(target_os = "linux")]
    impl Drop for TrashTestGuard<'_> {
        fn drop(&mut self) {
            match self.previous_xdg.take() {
                // SAFETY: see the SAFETY comment in `lock_real_trash` — same lock, same invariant.
                Some(v) => unsafe { std::env::set_var("XDG_DATA_HOME", v) },
                None => unsafe { std::env::remove_var("XDG_DATA_HOME") },
            }
        }
    }

    /// Probes whether THIS environment can perform a full trash *round-trip* (delete → list →
    /// restore), not just whether the platform's API exists. A headless CI runner — e.g. GitHub's
    /// Windows Server image, which runs the job in a non-interactive session with no working Recycle
    /// Bin — can move a file to the trash but then cannot list/restore it, so the round-trip fails
    /// there through no fault of the product (CPE-1268). Tests that assert an undo-via-trash round-trip
    /// skip (not fail) when this returns false. This is purely a test-environment probe: real desktops
    /// round-trip fine, and the product guarantee is unchanged.
    ///
    /// **Measured per-platform verdict (CPE-1806), not assumed** — the PR #961 review grepped whole raw
    /// CI job logs (`gh api .../actions/jobs/<id>/logs`, not a region of one) for the `CPE-1268`
    /// skip-notice text below, rather than trusting that an absence in a snippet meant an absence in the
    /// run. On the `Backend (ubuntu-latest)` job — run `32361564571` (PR #954, CPE-1791) job
    /// `96402134715`, and run `32374146099` (PR #957, CPE-1803) job `96441615073` — **zero** `CPE-1268`
    /// notices across either complete log; unrelated `CPE-1696`/`CPE-1705` skip notices from other
    /// mechanisms ARE present in both, which proves the emitter's output really does reach the log and
    /// an absence here means "did not fire", not "invisible" (`writeln!(stderr)` bypasses libtest's
    /// capture — see `fsutil::require_staged`'s doc comment). The same run's `Backend (windows-latest)`
    /// job (`96441615090`) carries **five** `CPE-1268` notices — one per `cfg(any(target_os = "windows",
    /// target_os = "linux"))` round-trip test below, every run. So: **Linux stages this for real on
    /// every measured run; Windows legitimately may not.** macOS has no `trash::os_limited` API at all
    /// (see the `#[cfg(...)]` on every test that calls this function), so the question does not arise
    /// there. This backs `supported_here = true` on the two Linux-only malformed-`.trashinfo`
    /// panic-boundary tests (CPE-1791/CPE-1803) and `supported_here = cfg!(target_os = "linux")` on the
    /// five shared round-trip tests, both routed through [`cpe_server::fsutil::require_staged`]; the CI
    /// `skip-visibility guard (CPE-1717 / CPE-1806)` step re-measures this on every run instead of
    /// resting on this paragraph going stale.
    ///
    /// Takes the [`TrashTestGuard`] from [`lock_real_trash`] by reference — not just as a convention
    /// documented in prose, but so the guard must already have been constructed (and therefore the real
    /// OS trash locked, and on Linux redirected) before this function's own real delete→list→restore
    /// probe can run against the same shared OS trash the guarded tests use (CPE-1785).
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    fn trash_roundtrip_available(guard: &TrashTestGuard) -> bool {
        let dir = match tempfile::tempdir() {
            Ok(d) => d,
            Err(_) => return false,
        };
        let probe = dir.path().join("cpe-trash-roundtrip-probe.tmp");
        if fs::write(&probe, b"probe").is_err() {
            return false;
        }
        if trash::delete(&probe).is_err() {
            return false;
        }
        // It must be listable back out of the trash by its original path...
        let item = match trash::os_limited::list() {
            Ok(items) => items
                .into_iter()
                .find(|i| i.original_parent.join(&i.name) == probe),
            Err(_) => None,
        };
        let item = match item {
            Some(i) => i,
            None => return false,
        };
        // CPE-1785 (PR #940 review, blocker 2): prove the Linux redirect actually took effect, rather
        // than trusting `home_trash()`'s uncached `var_os` read on faith. The redirect's correctness
        // rests on an unstated invariant — `scratch_dir`'s root and `tempfile::tempdir()`'s root share a
        // mount, so `trash`'s `delete_all_canonicalized` takes the `home_trash` branch
        // (`freedesktop.rs:36-58`) rather than the shared per-mount `.Trash-$uid` branch. If that ever
        // stops holding (a CI image with `/tmp` on its own mount, a container with a different
        // `TMPDIR`, `scratch_dir` moving off `std::env::temp_dir()`), this guard silently degrades back
        // to the shared trash and every existing assertion here would still pass (they only check that
        // the probe is listed, and it would be, just from the wrong directory). Assert containment
        // explicitly so that degrades LOUDLY instead. `item.id` is an absolute path to the `.trashinfo`
        // file on Linux (`trash` crate doc comment on `TrashItem::id`), so it must sit inside the
        // redirected scratch directory once the fix is doing its job.
        if let Some(scratch) = guard.scratch_path() {
            let trashinfo_path = std::path::Path::new(&item.id);
            assert!(
                trashinfo_path.starts_with(scratch),
                "the probe's trashinfo file ({}) is not inside the redirected scratch trash ({}) — \
                 XDG_DATA_HOME redirection silently did not take effect, so this test just touched the \
                 REAL shared OS trash (CPE-1785, PR #940 review, blocker 2)",
                trashinfo_path.display(),
                scratch.display()
            );
        }
        // ...and restorable to that path.
        if trash::os_limited::restore_all([item]).is_err() {
            return false;
        }
        let restored = probe.exists();
        let _ = fs::remove_file(&probe);
        restored
    }

    // Trash restore (`trash::os_limited::{list, restore_all}`) is only implemented on Windows and
    // Linux (see `restore_from_trash_impl` / CPE-044) — macOS has no programmatic trash listing API,
    // so this scenario cannot be exercised there. Mirrors the existing platform split.
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    #[test]
    fn macro_run_convert_step_then_undo_restores_the_original_bytes_via_trash() {
        // CPE-1785: serialise against every other test touching the real OS trash (and, on Linux,
        // redirect it to a private scratch directory) before doing anything else in this test.
        let trash_guard = lock_real_trash();
        // CPE-1268: skip (don't fail) on a runner with no working trash round-trip — see
        // `trash_roundtrip_available`. The product guarantee is proven on any real desktop / a CI
        // runner whose Recycle Bin works (Linux does; a headless Windows Server session may not).
        // CPE-1806: routed through `require_staged` — `supported_here = cfg!(target_os = "linux")`
        // matches the measured verdict right above (Linux's round-trip works; a headless Windows
        // Server CI session may not), so a Linux runner that stops staging goes RED instead of
        // silently voiding this test, while Windows keeps the legitimate loud skip.
        if !cpe_server::fsutil::require_staged(
            "trash_roundtrip",
            cfg!(target_os = "linux"),
            trash_roundtrip_available(&trash_guard),
        ) {
            cpe_server::skip_notice!(
                "skipping trash round-trip test: this environment cannot delete→list→restore via the \
                 OS trash (e.g. a headless CI Windows Server session with no working Recycle Bin) — \
                 CPE-1268"
            );
            return;
        }

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

    // ---- CPE-1651: backend consent gate on the permanent-destruction primitives --------------------
    // `delete_permanent` used to delete whatever it was handed and say, in its doc comment, that "the UI
    // must confirm" — delegating the safety decision to a frontend the IPC surface can bypass entirely.
    // The PR #838 review used it as step 2 of a working exploit chain. These tests pin the gate the same
    // way CPE-1611 pinned `shred_paths` and CPE-1630 pinned `vault_create`: refusal proven by reading the
    // files' BYTES back off disk, not by trusting the `Err`.

    /// The core defence-in-depth guarantee: `confirmed: false` refuses the WHOLE batch — `Err`, with a
    /// specific reason, and **nothing deleted**, verified by reading each file's contents back.
    #[test]
    fn delete_permanent_refuses_the_whole_batch_when_not_confirmed() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.txt");
        let b = dir.path().join("b.txt");
        let sub = dir.path().join("sub");
        let nested = sub.join("nested.txt");
        fs::write(&a, b"alpha").unwrap();
        fs::write(&b, b"bravo").unwrap();
        fs::create_dir(&sub).unwrap();
        fs::write(&nested, b"nested").unwrap();

        let paths = vec![
            a.to_string_lossy().to_string(),
            b.to_string_lossy().to_string(),
            sub.to_string_lossy().to_string(),
        ];
        let err = delete_permanent_impl(paths, false)
            .expect_err("an unconfirmed delete_permanent call must be refused, not executed");

        assert!(!err.is_empty(), "the refusal must carry a specific reason");
        assert!(err.to_lowercase().contains("confirm"), "refusal reason: {err}");

        // Nothing was deleted — bytes, not merely `exists()`. The directory arm (`remove_dir_all`, the
        // arm the exploit chain used) is covered too.
        assert_eq!(fs::read(&a).unwrap(), b"alpha", "a.txt must be byte-for-byte untouched");
        assert_eq!(fs::read(&b).unwrap(), b"bravo", "b.txt must be byte-for-byte untouched");
        assert_eq!(fs::read(&nested).unwrap(), b"nested", "the directory arm must not have recursed");
    }

    /// The flip side: the identical call proceeds and actually deletes once `confirmed` is `true`,
    /// proving the flag is load-bearing rather than decorative — and that the legitimate UI path
    /// (`App.svelte`'s "Delete permanently?" confirm) still works end to end.
    #[test]
    fn delete_permanent_proceeds_once_confirmed_is_true() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.txt");
        let sub = dir.path().join("sub");
        fs::write(&a, b"alpha").unwrap();
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("nested.txt"), b"nested").unwrap();

        let results = delete_permanent_impl(
            vec![a.to_string_lossy().to_string(), sub.to_string_lossy().to_string()],
            true,
        )
        .expect("a confirmed delete_permanent call must be allowed to run");

        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.ok), "both deletes should report ok: {results:?}");
        assert!(!a.exists(), "a confirmed delete must actually remove the file");
        assert!(!sub.exists(), "a confirmed delete must actually remove the directory tree");
    }

    /// Per-path failures are still reported per-path (skip-and-report), NOT promoted to the batch-level
    /// `Err` the refusal now uses — so the frontend can keep telling "the whole call was refused" apart
    /// from "one of these paths failed", and one bad path never loses the others' results.
    #[test]
    fn a_confirmed_delete_permanent_still_reports_per_path_failures_without_failing_the_batch() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real.txt");
        fs::write(&real, b"real").unwrap();
        let missing = dir.path().join("not-there.txt");

        let results = delete_permanent_impl(
            vec![missing.to_string_lossy().to_string(), real.to_string_lossy().to_string()],
            true,
        )
        .expect("a per-path failure must not fail the whole batch");

        assert_eq!(results.len(), 2);
        assert!(!results[0].ok, "the missing path should report a per-path error");
        assert!(results[1].ok, "the healthy path must still be deleted despite its neighbour failing");
        assert!(!real.exists());
    }

    // ---- Trash listing / restore / empty (CPE-1558, epic CPE-1486 slice 1) --------------------------
    // `trash::os_limited::{list, restore_all, purge_all}` only round-trips on a real desktop session —
    // a headless CI runner can lack a working Recycle Bin (CPE-1268) — so every test below that touches
    // the real OS trash guards on `trash_roundtrip_available()`, routed through `require_staged`
    // (CPE-1806): a legitimate loud skip on Windows (a headless CI session may genuinely lack a working
    // Recycle Bin), but a hard failure on Linux, where the round-trip is measured to work, so a runner
    // that stops staging there is a bug, not an environment gap, and must go red rather than silently
    // pass with none of these tests' assertions ever having run.
    // `select_trash_targets` is pure and never calls `purge_all`, so it's tested unconditionally and never
    // risks the developer's or CI runner's actual trash contents.

    #[cfg(any(target_os = "windows", target_os = "linux"))]
    #[test]
    fn select_trash_targets_filters_by_id_or_selects_everything() {
        // Hand-built `TrashItem`s — this never touches the real OS trash, so it needs no roundtrip guard
        // and is always safe to run, including on a machine with real items sitting in its Recycle Bin.
        fn probe_item(id: &str) -> trash::TrashItem {
            trash::TrashItem {
                id: id.into(),
                name: format!("{id}.tmp").into(),
                original_parent: std::env::temp_dir(),
                time_deleted: 0,
            }
        }
        let all = vec![probe_item("a"), probe_item("b"), probe_item("c")];

        let everything = select_trash_targets(all.clone(), None);
        assert_eq!(everything.len(), 3, "ids=None must select every item");

        let just_b = select_trash_targets(all, Some(&["b".to_string()]));
        assert_eq!(just_b.len(), 1);
        assert_eq!(just_b[0].id.to_string_lossy(), "b");
    }

    /// CPE-1803: `TrashListOutcome::degrade_panic_to_empty` is the one place that decides whether a
    /// listing pass is reported as "degraded" — get this wrong and every downstream consumer
    /// (`list_trash_impl`/`list_trash_stream`/the frontend) inherits the mistake. Pure and OS-trash-free
    /// (constructs the enum by hand), so — unlike the Linux-only malformed-`.trashinfo` tests elsewhere in
    /// this module — it runs on every OS, including this Windows dev machine.
    ///
    /// Both halves are asserted in ONE test on purpose: a test that only checked the panic case could
    /// pass even if `degraded` were hardcoded to `true`, which is exactly the "half a test" the ticket
    /// warns against — a genuinely empty `Vec` (`TrashListOutcome::Ok(vec![])`, the real shape a healthy
    /// but empty Trash produces) must come back `degraded: false`, or a healthy empty Trash would
    /// permanently render as "couldn't be read".
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    #[test]
    fn degrade_panic_to_empty_distinguishes_a_genuinely_empty_list_from_a_caught_panic() {
        let (items, degraded) = TrashListOutcome::Ok(Vec::new()).degrade_panic_to_empty().unwrap();
        assert!(items.is_empty());
        assert!(!degraded, "a genuinely empty listing must not be reported as degraded");

        let (items, degraded) = TrashListOutcome::PanicCaught.degrade_panic_to_empty().unwrap();
        assert!(items.is_empty(), "a caught panic still degrades this pass to no entries");
        assert!(
            degraded,
            "a caught panic must be reported as degraded — indistinguishable-from-empty is the CPE-1803 bug"
        );
    }

    // CPE-1651 (sibling audit of `delete_permanent`): `empty_trash` carried the same ungated promise —
    // "the UI must confirm before calling this with `None`". These exercise `empty_trash_gated` with the
    // two OS calls injected, so the refusal is proven to reach NEITHER `list` nor `purge_all` without
    // ever putting the developer's or CI runner's real Recycle Bin at risk.

    #[cfg(any(target_os = "windows", target_os = "linux"))]
    #[test]
    fn empty_trash_refuses_without_reaching_the_os_when_not_confirmed() {
        for ids in [None, Some(vec!["b".to_string()])] {
            let mut listed = false;
            let mut purged: Option<Vec<String>> = None;
            let err = empty_trash_gated(
                false,
                || {
                    listed = true;
                    Ok(vec!["a".to_string(), "b".to_string()])
                },
                |all| match &ids {
                    None => all,
                    Some(want) => all.into_iter().filter(|i| want.contains(i)).collect(),
                },
                |targets| {
                    purged = Some(targets);
                    Ok(())
                },
            )
            .expect_err("an unconfirmed empty_trash call must be refused, not executed");

            assert!(err.to_lowercase().contains("confirm"), "refusal reason: {err}");
            assert!(purged.is_none(), "a refused purge must never reach purge_all (ids: {ids:?})");
            assert!(!listed, "a refused purge must not even enumerate the trash (ids: {ids:?})");
        }
    }

    #[cfg(any(target_os = "windows", target_os = "linux"))]
    #[test]
    fn empty_trash_purges_the_selected_targets_once_confirmed_is_true() {
        let mut purged: Option<Vec<String>> = None;
        empty_trash_gated(
            true,
            || Ok(vec!["a".to_string(), "b".to_string()]),
            |all| all.into_iter().filter(|i| i == "b").collect(),
            |targets| {
                purged = Some(targets);
                Ok(())
            },
        )
        .expect("a confirmed empty_trash call must be allowed to run");
        assert_eq!(purged, Some(vec!["b".to_string()]), "the confirmed purge must reach purge_all");
    }

    // CPE-1559 review fix on CPE-1558: `trash_item_to_entry` must DEGRADE (keep the entry with
    // `size: None`) rather than DROP the entry when the per-item metadata lookup fails — only a
    // genuinely unusable id/name/original_parent (non-UTF-8) should skip the entry. These two tests
    // exercise the pure `trash_entry_from_fields` half directly, so they never touch the real OS trash.

    #[cfg(any(target_os = "windows", target_os = "linux"))]
    #[test]
    fn trash_entry_from_fields_degrades_to_none_size_when_metadata_lookup_failed() {
        let entry = trash_entry_from_fields(
            Some("id-1"),
            Some("file.txt"),
            Some("/home/user"),
            42,
            None, // stands in for a failed `trash::os_limited::metadata` lookup
        )
        .expect("valid utf-8 id/name/parent must still produce an entry even without a size");
        assert_eq!(entry.id, "id-1");
        assert_eq!(entry.size, None);
    }

    /// An `OsString` that is well-formed for the platform's own filesystem API but has no valid UTF-8
    /// encoding, so `OsStr::to_str()` returns `None` — the real trigger the tests below need, rather than
    /// the `Option::None` stand-in `trash_entry_from_fields` takes.
    ///
    /// Constructible on BOTH supported platforms with plain std, which is why the CPE-1804 tests are not
    /// Linux-only the way the malformed-`.trashinfo` panic tests are: on Linux a path is arbitrary bytes,
    /// and on Windows it is arbitrary UTF-16 *including unpaired surrogates*, which is exactly the case
    /// that made this route "not Linux-only" in the ticket's framing.
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    fn undecodable_os_string() -> std::ffi::OsString {
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::ffi::OsStringExt;
            // A lone high surrogate: legal in a Windows filename, no UTF-8 encoding.
            std::ffi::OsString::from_wide(&[0xD800])
        }
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::ffi::OsStringExt;
            // 0xFF is not a legal UTF-8 lead byte, but is a perfectly legal filename byte.
            std::ffi::OsString::from_vec(vec![0xFF])
        }
    }

    /// A `trash::TrashItem` whose three UTF-8-validated fields are individually switchable between a
    /// decodable and an undecodable value, so a walker test can build a mixed listing.
    ///
    /// The `id` is deliberately **shaped like a real one** — `<trash>/info/<name>.trashinfo`, which is
    /// exactly what `list()` produces (`trash-5.2.6/src/freedesktop.rs:121`) — and the undecodable bytes
    /// go in the *filename component* rather than replacing the whole path, which is also what a real
    /// undecodable trash entry looks like: an ordinary trash directory holding a file whose name is not
    /// valid UTF-8.
    ///
    /// This is not cosmetic. The first version used a bare `"id-ok"`, and on Linux CI that panicked
    /// inside the dependency (`freedesktop.rs:350` does `.parent().unwrap().parent().unwrap()` on the
    /// id, and a bare name has fewer than two ancestors) — the very panic family CPE-1791 exists for,
    /// triggered by unrealistic test input rather than by anything production can produce. Windows never
    /// showed it because its `metadata` is a COM lookup that simply returns `Err`.
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    fn item_with_undecodable(field: Option<&str>) -> trash::TrashItem {
        let bad = undecodable_os_string();
        // A path that does not exist, so any real metadata lookup takes its ordinary `Err` route — the
        // same one a genuinely concurrent purge produces between `list()` and `metadata()`.
        let info_dir = std::env::temp_dir().join("cpe-1804-no-such-trash").join("info");
        let mut file_name = if field == Some("id") { bad.clone() } else { "ok".into() };
        file_name.push(".trashinfo");
        trash::TrashItem {
            id: info_dir.join(file_name).into_os_string(),
            name: if field == Some("name") { bad.clone() } else { "ok.txt".into() },
            original_parent: if field == Some("parent") {
                std::env::temp_dir().join(bad)
            } else {
                std::env::temp_dir()
            },
            time_deleted: 0,
        }
    }

    /// The production skip decision with the OS size lookup left out — what the CPE-1804 tests map with.
    /// See [`trash_item_to_entry_with_size`]: `size` cannot influence the skip, so this stubs nothing
    /// that matters, and it keeps these tests free of a dependency call that has no bearing on what they
    /// assert. The *real* mapper, metadata lookup included, stays covered by the real-trash round-trip
    /// tests further down, which list an actually-trashed probe.
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    fn map_without_metadata(item: &trash::TrashItem) -> Option<cpe_server::model::TrashEntry> {
        trash_item_to_entry_with_size(item, None)
    }

    /// Tripwire for the #962 Linux red, runnable on the platform that could not see it.
    ///
    /// No CPE-1804 test routes a fabricated item through `trash::os_limited::metadata` any more, so this
    /// is belt-and-braces — but the fixture is one careless edit away from being reused somewhere that
    /// does, and the failure mode is a dependency panic on a platform this machine cannot execute. So
    /// the precondition that dependency imposes is asserted here directly: `freedesktop.rs:350-351`
    /// unwraps `Path::new(id).parent().parent()` and `file_stem()`, which is precisely what a bare
    /// `"id-ok"` could not satisfy.
    ///
    /// Windows `Path` semantics stand in for Linux ones, which is sound for exactly this claim: the id
    /// is built by `join`ing an absolute `temp_dir()` with two more components, so its ancestor count is
    /// the same on both platforms. This asserts the fixture is *shaped* correctly; it does not — and
    /// cannot, from Windows — prove the dependency's behaviour on Linux.
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    #[test]
    fn fabricated_trash_item_ids_satisfy_the_dependencys_path_preconditions() {
        for field in [None, Some("id"), Some("name"), Some("parent")] {
            let item = item_with_undecodable(field);
            let id = std::path::Path::new(&item.id);
            assert!(
                id.parent().and_then(std::path::Path::parent).is_some(),
                "a fabricated id needs two ancestors or `trash` panics unwrapping them on Linux \
                 (freedesktop.rs:350) — field {field:?}, id {id:?}"
            );
            assert!(
                id.file_stem().is_some(),
                "a fabricated id needs a file stem for the same reason (freedesktop.rs:351) — \
                 field {field:?}, id {id:?}"
            );
        }
    }

    /// CPE-1804 — this is the CPE-1558 pin, **updated rather than removed**. The skip itself is still
    /// correct and still pinned: an id/name/original_parent that can't round-trip through UTF-8 is
    /// unusable for restore or purge, so the entry is dropped, exactly as before. What changed is that
    /// the drop is no longer *silent* — [`stream_trash_entries`] now returns how many it dropped, and
    /// both listing commands report that count. The old pin certified silence by omission (it asserted
    /// `is_none()` and stopped there, which a caller that threw the skip away passed just as happily);
    /// this one asserts the skip AND its count, so the silence can't come back without failing.
    ///
    /// The counted half is asserted against real undecodable `OsString`s through the actual walker, not
    /// against the `Option::None` stand-in, because the stand-in is exactly the thing that can't tell you
    /// whether the count is wired to anything.
    ///
    /// Runs on every OS and cannot skip itself (the CPE-1806 trap): it builds its own input and reaches
    /// no OS call at all — `map_without_metadata` supplies `size` instead of looking it up, which stubs
    /// nothing that can affect a skip. That last part is not a convenience: the first version DID route
    /// these fabricated items through `trash::os_limited::metadata`, which is harmless on Windows and
    /// panics on Linux, so a green local run said nothing about CI (#962).
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    #[test]
    fn a_non_utf8_field_skips_the_entry_and_the_skip_is_counted_not_silent() {
        // --- unchanged half: which fields skip. A `None` field stands in for `OsStr::to_str()` failing
        // on non-UTF-8 content — the one case that should genuinely skip the entry, unlike a
        // metadata-lookup failure (`size: None` above).
        assert!(trash_entry_from_fields(None, Some("file.txt"), Some("/home/user"), 42, Some(10)).is_none());
        assert!(trash_entry_from_fields(Some("id-1"), None, Some("/home/user"), 42, Some(10)).is_none());
        assert!(trash_entry_from_fields(Some("id-1"), Some("file.txt"), None, 42, Some(10)).is_none());
        // Sanity check: all-valid fields with a real size still produces an entry (not accidentally
        // caught by the `?` chain above).
        assert!(
            trash_entry_from_fields(Some("id-1"), Some("file.txt"), Some("/home/user"), 42, Some(10)).is_some()
        );

        // --- new half: the skip is counted, per field, through the real walker.
        for field in ["id", "name", "parent"] {
            let items = vec![item_with_undecodable(None), item_with_undecodable(Some(field))];
            let mut entries = Vec::new();
            let skipped =
                stream_trash_entries(items, TRASH_LIST_BATCH, map_without_metadata, |b| entries.extend(b));
            assert_eq!(entries.len(), 1, "the decodable sibling must still list ({field})");
            assert_eq!(
                skipped, 1,
                "an undecodable `{field}` must be COUNTED, not silently dropped — an uncounted skip is \
                 how a full trash reads as empty and a mixed one under-counts (CPE-1804)"
            );
        }

        // --- the asymmetric half: an all-decodable listing must report ZERO skips, or "skipped" would
        // be a constant and the warning it drives would fire on every healthy Trash.
        let clean = vec![item_with_undecodable(None), item_with_undecodable(None)];
        let mut entries = Vec::new();
        let skipped =
            stream_trash_entries(clean, TRASH_LIST_BATCH, map_without_metadata, |b| entries.extend(b));
        assert_eq!(entries.len(), 2);
        assert_eq!(skipped, 0, "a fully decodable listing must report no skips");
    }

    /// CPE-1804: the collect-to-vec and streamed commands must never disagree about how much they
    /// dropped. They share one walker, and this proves the sharing is real by driving the SAME mixed
    /// listing through a collect-shaped consumer and a stream-shaped one (batch size 1, so the flushes
    /// actually chunk) and comparing both the entries and the skip count.
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    #[test]
    fn both_listing_paths_report_the_same_entries_and_the_same_skip_count() {
        let mixed = || {
            vec![
                item_with_undecodable(None),
                item_with_undecodable(Some("name")),
                item_with_undecodable(None),
                item_with_undecodable(Some("id")),
            ]
        };

        let mut collected = Vec::new();
        let collect_skipped = stream_trash_entries(mixed(), TRASH_LIST_BATCH, map_without_metadata, |b| {
            collected.extend(b)
        });

        let mut streamed = Vec::new();
        let mut batches = 0usize;
        let stream_skipped = stream_trash_entries(mixed(), 1, map_without_metadata, |b| {
            batches += 1;
            streamed.extend(b);
        });

        assert_eq!(batches, 2, "batch size 1 over 2 listable items must flush twice");
        assert_eq!(collected, streamed, "the two paths must surface identical entries");
        assert_eq!(collect_skipped, stream_skipped, "the two paths must report identical skip counts");
        assert_eq!(collect_skipped, 2, "two of the four items are undecodable");
    }

    /// CPE-1804/CPE-1805: `listing_is_degraded` is the one place that folds the two incompleteness
    /// routes into the flag the frontend renders from. All four rows are asserted together on purpose —
    /// a test that only checked the true rows would pass with the function hardcoded to `true`, which
    /// would put the "couldn't be fully read" notice on every healthy Trash forever.
    ///
    /// This pins the fold's *arithmetic* only. That both commands actually PERFORM it is a separate
    /// claim, pinned by `both_commands_fold_a_per_item_skip_into_degraded` below — see its comment for
    /// why one test cannot stand in for the other.
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    #[test]
    fn listing_is_degraded_folds_both_incompleteness_routes_and_stays_false_for_a_clean_pass() {
        assert!(!listing_is_degraded(false, 0), "a clean pass must NOT be reported as degraded");
        assert!(listing_is_degraded(true, 0), "a caught panic degrades the pass (CPE-1791/CPE-1803)");
        assert!(listing_is_degraded(false, 1), "a skipped undecodable item degrades the pass (CPE-1804)");
        assert!(listing_is_degraded(true, 3), "both at once is still degraded");
    }

    /// CPE-1804 review (#962, blocking 1): the fold existing and being correct is not the same claim as
    /// both commands USING it. Before this test, substituting `degraded: panic_degraded` at either call
    /// site left the whole suite green — the walker tests never build a listing, the truth-table test
    /// above never checks that anyone calls the function, and the real-trash round-trip tests only cover
    /// the clean pass, where `panic_degraded` and `listing_is_degraded(..)` happen to agree. That is the
    /// certified-by-omission shape this ticket set out to remove from the old pin; leaving it one layer
    /// up would have been the same mistake with a better view.
    ///
    /// Drives BOTH command bodies through their injected-outcome seams with input the test constructs
    /// itself, so it is deterministic on every machine and never consults the real Recycle Bin.
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    #[test]
    fn both_commands_fold_a_per_item_skip_into_degraded() {
        let mixed = || {
            TrashListOutcome::Ok(vec![item_with_undecodable(None), item_with_undecodable(Some("name"))])
        };

        // --- collect-to-vec. `panic_degraded` is FALSE here (no panic), so `degraded` can only be true
        // if this command folded the skip in. `degraded: panic_degraded` fails exactly here.
        let listed =
            list_trash_from(mixed(), map_without_metadata).expect("a listable outcome must not error");
        assert_eq!(listed.entries.len(), 1, "the decodable item must still be listed");
        assert_eq!(listed.skipped, 1, "list_trash must report the skip it made");
        assert!(listed.degraded, "list_trash must fold a per-item skip into `degraded` (CPE-1804)");

        // --- streamed. Same claim, and additionally that the entries really went out over the sink:
        // this route can flush real batches and still be incomplete, which is the case CPE-1805 covers.
        let mut sent = Vec::new();
        let summary = trash_stream_summary_from(mixed(), map_without_metadata, |batch| sent.extend(batch))
            .expect("a listable outcome must not error");
        assert_eq!(summary.count, 1);
        assert_eq!(sent.len(), 1, "the decodable item must have been streamed, not just counted");
        assert_eq!(summary.skipped, 1, "list_trash_stream must report the skip it made");
        assert!(summary.degraded, "list_trash_stream must fold a per-item skip into `degraded`");

        // --- the two commands must agree, entry for entry, on the same input.
        assert_eq!(listed.entries, sent, "the two commands must surface identical entries");

        // --- asymmetric half: an all-decodable outcome must come back CLEAN from both, or `degraded`
        // would be a constant and every healthy Trash would carry the notice forever.
        let clean = || TrashListOutcome::Ok(vec![item_with_undecodable(None)]);
        let listed = list_trash_from(clean(), map_without_metadata).unwrap();
        assert!(!listed.degraded, "a clean pass must not be reported as degraded by list_trash");
        assert_eq!(listed.skipped, 0);
        let summary = trash_stream_summary_from(clean(), map_without_metadata, |_| {}).unwrap();
        assert!(!summary.degraded, "a clean pass must not be reported as degraded by list_trash_stream");
        assert_eq!(summary.skipped, 0);

        // --- and the OTHER route still folds, through the same seam: a caught panic degrades both
        // commands with no per-item count to show for it (CPE-1791/CPE-1803 behaviour, unchanged).
        let listed = list_trash_from(TrashListOutcome::PanicCaught, map_without_metadata).unwrap();
        assert!(listed.entries.is_empty());
        assert!(listed.degraded, "a caught panic must still degrade list_trash");
        assert_eq!(listed.skipped, 0, "the panic route wipes the pass; it has no per-item count");
        let summary =
            trash_stream_summary_from(TrashListOutcome::PanicCaught, map_without_metadata, |_| {}).unwrap();
        assert!(summary.degraded, "a caught panic must still degrade list_trash_stream");
        assert_eq!(summary.skipped, 0);

        // --- a genuine `trash::Error` is still an error, not a degraded-but-successful listing.
        assert!(list_trash_from(TrashListOutcome::Error("boom".into()), map_without_metadata).is_err());
        assert!(
            trash_stream_summary_from(TrashListOutcome::Error("boom".into()), map_without_metadata, |_| {})
                .is_err()
        );
    }

    #[cfg(any(target_os = "windows", target_os = "linux"))]
    #[test]
    fn list_trash_then_restore_trash_items_round_trips_a_probe_file() {
        let trash_guard = lock_real_trash(); // CPE-1785: see the doc comment on `lock_real_trash`
        // CPE-1806: `supported_here = cfg!(target_os = "linux")` — see `trash_roundtrip_available`'s
        // doc comment for the measured per-platform verdict this mirrors. Red-proofed by temporarily
        // forcing this call to (true, false) and confirming a CI=true run panics with the CPE-1717
        // message naming "trash_roundtrip", then reverting; the CI `skip-visibility guard (CPE-1717 /
        // CPE-1806)` step below (`.github/workflows/ci.yml`) re-proves the same panic on every run.
        if !cpe_server::fsutil::require_staged(
            "trash_roundtrip",
            cfg!(target_os = "linux"),
            trash_roundtrip_available(&trash_guard),
        ) {
            cpe_server::skip_notice!(
                "skipping list_trash/restore_trash_items round-trip test: this environment cannot \
                 delete→list→restore via the OS trash (e.g. a headless CI Windows Server session with no \
                 working Recycle Bin) — CPE-1268"
            );
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let probe = dir.path().join("cpe-1558-list-restore-probe.tmp");
        fs::write(&probe, b"probe").unwrap();
        trash::delete(&probe).unwrap();

        let listed = list_trash_impl().expect("list_trash_impl should succeed");
        // CPE-1804 review (#962, blocking 2): `degraded`/`skipped` are properties of the WHOLE trash this
        // pass read, not of the probe this test created — and on Windows that is the developer's or CI
        // runner's real Recycle Bin, whose contents this test does not control. Asserting `!degraded`
        // here would fail on any machine whose trash happens to hold one non-UTF-8 name, i.e. under
        // precisely the condition CPE-1804 exists to handle: a flake with a plausible-looking cause,
        // which is the kind someone "fixes" by changing the code. (Linux is safe — `lock_real_trash`
        // redirects `XDG_DATA_HOME` to a scratch dir — but the assertion has to hold on both.)
        //
        // Both flags are pinned deterministically instead, on constructed input, by
        // `both_commands_fold_a_per_item_skip_into_degraded`, which covers strictly more than this line
        // did: clean AND panic AND per-item-skip AND error, on BOTH commands. So this is a relocation of
        // CPE-1803's claim to somewhere it can actually be proven, not a dropped guard — what remains
        // here is what only a real round-trip can show, that a genuinely trashed file lists and restores.
        let probe_path = probe.to_string_lossy().to_string();
        let entry = listed
            .entries
            .iter()
            .find(|e| e.original_path == probe_path)
            .expect("the probe file should appear in the trash listing")
            .clone();
        assert_eq!(entry.name, "cpe-1558-list-restore-probe.tmp");

        let results = restore_trash_items_impl(vec![entry.id.clone()]);
        assert_eq!(results.len(), 1);
        assert!(results[0].ok, "restore should succeed: {:?}", results[0]);
        assert!(probe.exists(), "the probe file should be restored to its original path");

        let _ = fs::remove_file(&probe);
    }

    #[cfg(any(target_os = "windows", target_os = "linux"))]
    #[test]
    fn restore_trash_items_reports_a_collision_as_a_distinguishable_per_item_error_without_aborting_the_batch() {
        let trash_guard = lock_real_trash(); // CPE-1785: see the doc comment on `lock_real_trash`
        // CPE-1806: `supported_here = cfg!(target_os = "linux")` — see `trash_roundtrip_available`'s
        // doc comment for the measured per-platform verdict this mirrors.
        if !cpe_server::fsutil::require_staged(
            "trash_roundtrip",
            cfg!(target_os = "linux"),
            trash_roundtrip_available(&trash_guard),
        ) {
            cpe_server::skip_notice!(
                "skipping restore_trash_items collision test: this environment cannot delete→list→restore \
                 via the OS trash — CPE-1268"
            );
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let colliding = dir.path().join("cpe-1558-collision-probe.tmp");
        let clean = dir.path().join("cpe-1558-clean-probe.tmp");
        fs::write(&colliding, b"one").unwrap();
        fs::write(&clean, b"two").unwrap();
        trash::delete(&colliding).unwrap();
        trash::delete(&clean).unwrap();

        // Something now occupies the colliding item's original path again — restoring it must be
        // refused, but that must NOT stop the other item in the same call from restoring successfully.
        fs::write(&colliding, b"occupied").unwrap();

        let listed = list_trash_impl().unwrap();
        let colliding_path = colliding.to_string_lossy().to_string();
        let clean_path = clean.to_string_lossy().to_string();
        let collision_id = listed
            .entries.iter().find(|e| e.original_path == colliding_path)
            .expect("colliding probe should be listed").id.clone();
        let clean_id = listed
            .entries.iter().find(|e| e.original_path == clean_path)
            .expect("clean probe should be listed").id.clone();

        let results = restore_trash_items_impl(vec![collision_id.clone(), clean_id.clone()]);
        assert_eq!(results.len(), 2);

        let collision_result = results.iter().find(|r| r.path == colliding_path)
            .expect("a per-item result must exist for the colliding path");
        assert!(!collision_result.ok, "a colliding restore must fail, never silently overwrite");
        assert!(
            collision_result.error.to_lowercase().contains("already exists"),
            "the error must be a clear, distinguishable message, not a raw `{{:?}}` dump: {}",
            collision_result.error
        );

        let clean_result = results.iter().find(|r| r.path == clean_path)
            .expect("a per-item result must exist for the clean path");
        assert!(
            clean_result.ok,
            "the OTHER item in the same batch must still restore despite the first one colliding: {clean_result:?}"
        );
        assert!(clean.exists(), "the non-colliding item should be restored to disk");

        // Best-effort cleanup: purge the still-trashed colliding item so repeat runs don't pile up in the
        // real OS trash (mirrors the cleanup in `macro_run_convert_step_then_undo_restores_...`).
        let _ = trash::os_limited::list().map(|items| {
            let ours: Vec<_> = items.into_iter().filter(|i| i.id.to_string_lossy() == collision_id).collect();
            let _ = trash::os_limited::purge_all(ours);
        });
        let _ = fs::remove_file(&colliding);
        let _ = fs::remove_file(&clean);
    }

    #[cfg(any(target_os = "windows", target_os = "linux"))]
    #[test]
    fn empty_trash_purges_only_the_selected_probe_item() {
        let trash_guard = lock_real_trash(); // CPE-1785: see the doc comment on `lock_real_trash`
        // CPE-1806: `supported_here = cfg!(target_os = "linux")` — see `trash_roundtrip_available`'s
        // doc comment for the measured per-platform verdict this mirrors.
        if !cpe_server::fsutil::require_staged(
            "trash_roundtrip",
            cfg!(target_os = "linux"),
            trash_roundtrip_available(&trash_guard),
        ) {
            cpe_server::skip_notice!(
                "skipping empty_trash selective-purge test: this environment cannot delete→list→restore \
                 via the OS trash — CPE-1268"
            );
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let keep = dir.path().join("cpe-1558-empty-keep.tmp");
        let purge = dir.path().join("cpe-1558-empty-purge.tmp");
        fs::write(&keep, b"keep").unwrap();
        fs::write(&purge, b"purge").unwrap();
        trash::delete(&keep).unwrap();
        trash::delete(&purge).unwrap();

        let listed = list_trash_impl().unwrap();
        let keep_path = keep.to_string_lossy().to_string();
        let purge_path = purge.to_string_lossy().to_string();
        let keep_id = listed.entries.iter().find(|e| e.original_path == keep_path)
            .expect("keep probe should be listed").id.clone();
        let purge_id = listed.entries.iter().find(|e| e.original_path == purge_path)
            .expect("purge probe should be listed").id.clone();

        // Deliberately never call `empty_trash_impl(None)` here — on this real machine that would purge
        // EVERYTHING sitting in the actual Recycle Bin, not just this test's probes. The "purge everything"
        // branch is covered instead by the OS-call-free `select_trash_targets_filters_by_id_or_selects_everything`.
        empty_trash_impl(Some(vec![purge_id.clone()]), true).expect("selective empty_trash should succeed");

        let after = list_trash_impl().unwrap();
        assert!(after.entries.iter().any(|e| e.id == keep_id), "an item NOT named in `ids` must survive a selective purge");
        assert!(!after.entries.iter().any(|e| e.id == purge_id), "the targeted item must be gone after purging");

        // Clean up: restore `keep`'s probe entry rather than leaving it in the real trash.
        let _ = restore_trash_items_impl(vec![keep_id]);
        let _ = fs::remove_file(&keep);
    }

    #[cfg(any(target_os = "windows", target_os = "linux"))]
    #[test]
    fn list_trash_stream_flushes_batches_over_the_channel_and_matches_the_collect_variant() {
        let trash_guard = lock_real_trash(); // CPE-1785: see the doc comment on `lock_real_trash`
        // CPE-1806: `supported_here = cfg!(target_os = "linux")` — see `trash_roundtrip_available`'s
        // doc comment for the measured per-platform verdict this mirrors.
        if !cpe_server::fsutil::require_staged(
            "trash_roundtrip",
            cfg!(target_os = "linux"),
            trash_roundtrip_available(&trash_guard),
        ) {
            cpe_server::skip_notice!(
                "skipping list_trash_stream test: this environment cannot delete→list→restore via the OS \
                 trash — CPE-1268"
            );
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let probe = dir.path().join("cpe-1558-stream-probe.tmp");
        fs::write(&probe, b"probe").unwrap();
        trash::delete(&probe).unwrap();

        let collected: std::sync::Arc<std::sync::Mutex<Vec<serde_json::Value>>> = Default::default();
        let on_entry = collecting_channel(collected.clone());
        let summary = tauri::async_runtime::block_on(list_trash_stream(on_entry))
            .expect("list_trash_stream dispatches");
        // CPE-1804 review (#962, blocking 2): no `degraded`/`skipped` assertion here either — same
        // reason as in the round-trip test above (both describe the ambient trash, not this probe), and
        // both are pinned deterministically by `both_commands_fold_a_per_item_skip_into_degraded`. What
        // this test uniquely proves — that the streamed batches and the summary count agree against a
        // real OS listing — is below and unaffected.

        let batches = collected.lock().unwrap();
        assert_eq!(batches.len(), summary.count, "every streamed entry must have gone out over the channel");
        let probe_path = probe.to_string_lossy().to_string();
        assert!(
            batches.iter().any(|v| v.get("original_path").and_then(|p| p.as_str()) == Some(probe_path.as_str())),
            "the probe file should appear among the streamed batches: {batches:?}"
        );

        // Clean up: restore + remove so the probe doesn't linger in the real trash.
        let listed = list_trash_impl().unwrap();
        if let Some(entry) = listed.entries.iter().find(|e| e.original_path == probe_path) {
            let _ = restore_trash_items_impl(vec![entry.id.clone()]);
        }
        let _ = fs::remove_file(&probe);
    }

    // CPE-1791: one malformed `.trashinfo` file must not take out the whole listing, and — the part
    // three review rounds took to get right — Restore/Empty must never report success while data
    // silently survives (or is silently skipped) on disk. Linux-only: `trash::os_limited::list()` only
    // actually panics on this shape on Linux (`trash-5.2.6/src/freedesktop.rs:139-140` — confirmed by
    // reading the dependency source; Windows's Recycle Bin listing doesn't parse text `.trashinfo` files
    // at all). This crew runs on Windows, so these tests can only be compiled and executed by CI's Linux
    // leg of the 3-OS matrix — not locally, and there is no local Linux cross-toolchain available either
    // (`ring`/`libdbus-sys`'s build scripts need a real Linux C toolchain), so not even a cross
    // `cargo check` was possible.
    //
    // A prior version of this fix also had a Linux-only "quarantine" layer that hid a malformed file
    // from `list()` just long enough to skip only that one entry, keeping every other entry listed. It
    // was abandoned after three review rounds each found a new correctness bug in it — the last one
    // being the exact silent-false-success failure mode the second test below now pins — see the module
    // comment above `list_trash_catching_dependency_panics` for the full account. What ships instead is
    // the coarser, honestly-kept promise these tests prove: a malformed file degrades a *listing* pass to
    // empty rather than crashing it, and makes a *restore/empty* call fail loudly rather than lie.

    #[cfg(target_os = "linux")]
    #[test]
    fn trash_listing_degrades_to_empty_instead_of_crashing_on_a_malformed_trashinfo_file() {
        let trash_guard = lock_real_trash(); // CPE-1785: see the doc comment on `lock_real_trash`
        // CPE-1806: routed through `require_staged` (`supported_here = true` — this block is
        // `#[cfg(target_os = "linux")]`, and the round-trip is measured to work there; see
        // `trash_roundtrip_available`'s doc comment) so a runner that stopped staging goes RED under
        // CI instead of silently voiding the CPE-1803 `degraded` assertions below, which this
        // Linux-only panic-boundary test is the *only* execution any of that Linux behaviour ever gets.
        if !cpe_server::fsutil::require_staged(
            "trash_roundtrip",
            true,
            trash_roundtrip_available(&trash_guard),
        ) {
            cpe_server::skip_notice!(
                "skipping malformed-trashinfo resilience test: this environment cannot delete→list→restore \
                 via the OS trash — CPE-1268"
            );
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let good = dir.path().join("cpe-1791-good.tmp");
        fs::write(&good, b"good").unwrap();
        trash::delete(&good).unwrap();

        // Plant the malformed file directly into the redirected trash's info/ folder (the real delete
        // above guarantees that folder now exists, and the redirect guarantees this is a private scratch
        // trash, never a real developer's). Its body's only line after the mandatory `[Trash Info]`
        // header has no `=` — the exact shape that panics `trash-5.2.6/src/freedesktop.rs:139-140` via
        // `split.next().unwrap()`.
        let scratch = trash_guard.scratch_path().expect("the Linux XDG_DATA_HOME redirect must be active");
        let info_dir = scratch.join("Trash").join("info");
        assert!(info_dir.is_dir(), "the real delete above must have created the info/ folder");
        let malformed = info_dir.join("cpe-1791-malformed.trashinfo");
        fs::write(&malformed, "[Trash Info]\nPath\n").unwrap();

        // RED: prove the dependency itself really does panic on this on-disk shape — not a rewritten
        // test double, the actual `trash::os_limited::list()` this codebase calls in production.
        let raw_outcome = std::panic::catch_unwind(trash::os_limited::list);
        assert!(
            raw_outcome.is_err(),
            "expected trash::os_limited::list() to panic on a malformed .trashinfo body line with no \
             '=' (trash-5.2.6/src/freedesktop.rs:139-140) — if this now passes, the dependency was fixed \
             upstream"
        );

        // GREEN, with the honest, coarser guarantee this fix actually ships: the listing pass degrades
        // to `Ok` — never a crash, never an opaque `JoinError` — but `list()`'s single unwind-discarded
        // accumulator loses every entry from this pass, including the otherwise-good one planted above.
        // That is the documented trade-off of dropping the per-entry-skip quarantine layer (see the
        // module comment): a malformed file makes a listing come back thin, not wrong, not crashed.
        let listed = list_trash_impl().expect("list_trash_impl must return Ok, never propagate the panic");
        assert!(
            listed.entries.is_empty(),
            "a caught dependency panic must degrade this pass to empty, not partially succeed: {listed:?}"
        );
        // CPE-1803: this empty page must be flagged `degraded`, not indistinguishable from a genuinely
        // empty Trash — the frontend renders a different message for each. Pinning both in the same test
        // (rather than only asserting `degraded`) is what actually proves the two states are
        // distinguishable instead of `degraded` just being permanently `true`.
        assert!(
            listed.degraded,
            "a listing thinned by a caught dependency panic must be reported as degraded: {listed:?}"
        );

        // Recovery: once the malformed file is gone, listing works normally again — nothing was
        // permanently lost, the good entry's own `.trashinfo` and payload were never touched.
        let _ = fs::remove_file(&malformed);
        let listed_after = list_trash_impl().expect("list_trash_impl should succeed once the bad file is gone");
        assert!(
            listed_after.entries.iter().any(|e| e.original_path == good.to_string_lossy()),
            "the good probe should reappear once the malformed file is out of the way: {listed_after:?}"
        );
        assert!(
            !listed_after.degraded,
            "a fully-recovered listing must NOT still be reported as degraded, or a genuinely healthy \
             Trash would be told it's unreadable forever: {listed_after:?}"
        );
    }

    /// CPE-1791 review, round 3: pins the actual blocker that killed the quarantine layer so it cannot
    /// come back by a different route. A prior version of this fix let a quarantined file be restored to
    /// `info/` *before* `empty_trash_gated`'s purge targets (computed from an earlier `list()` call) had
    /// a chance to include it — so `empty_trash_impl` returned `Ok(())` while the malformed entry, and
    /// its real payload, silently survived on disk. Restore/Empty must instead fail loudly, and never
    /// silently purge only the entries the dependency happened to be able to see.
    #[cfg(target_os = "linux")]
    #[test]
    fn restore_and_empty_trash_fail_loudly_instead_of_reporting_false_success_when_the_dependency_panics()
    {
        let trash_guard = lock_real_trash(); // CPE-1785: see the doc comment on `lock_real_trash`
        // CPE-1806: routed through `require_staged` (`supported_here = true` — this block is
        // `#[cfg(target_os = "linux")]`, and the round-trip is measured to work there; see
        // `trash_roundtrip_available`'s doc comment) so a runner that stopped staging goes RED under
        // CI rather than silently voiding this Linux-only panic-boundary coverage.
        if !cpe_server::fsutil::require_staged(
            "trash_roundtrip",
            true,
            trash_roundtrip_available(&trash_guard),
        ) {
            cpe_server::skip_notice!(
                "skipping restore/empty panic-honesty test: this environment cannot delete→list→restore \
                 via the OS trash — CPE-1268"
            );
            return;
        }

        let dir = tempfile::tempdir().unwrap();
        let good = dir.path().join("cpe-1791-honesty-good.tmp");
        fs::write(&good, b"good").unwrap();
        trash::delete(&good).unwrap();

        let scratch = trash_guard.scratch_path().expect("the Linux XDG_DATA_HOME redirect must be active");
        let info_dir = scratch.join("Trash").join("info");
        let malformed = info_dir.join("cpe-1791-honesty-malformed.trashinfo");
        fs::write(&malformed, "[Trash Info]\nPath\n").unwrap();

        // restore_from_trash_impl: every requested path must come back as an explicit failure, and the
        // message must be the panic-boundary one, NOT the ordinary "Not found in the Recycle Bin — it
        // may have been emptied" — that message would be a LIE here (the item may well still be in the
        // trash; we simply could not ask).
        let good_path = good.to_string_lossy().to_string();
        let restore_results = restore_from_trash_impl(vec![good_path.clone()]);
        assert_eq!(restore_results.len(), 1);
        assert!(!restore_results[0].ok, "a caught dependency panic must never report a restore as ok");
        assert!(
            !restore_results[0].error.to_lowercase().contains("may have been emptied"),
            "must not reuse the not-found message for a panic — that would misreport an unknown state \
             as a known one: {}",
            restore_results[0].error
        );

        // restore_trash_items_impl: same honesty requirement, by id this time.
        let items_results = restore_trash_items_impl(vec!["whatever-id".to_string()]);
        assert_eq!(items_results.len(), 1);
        assert!(!items_results[0].ok, "a caught dependency panic must never report a restore as ok");
        assert!(
            !items_results[0].error.to_lowercase().contains("may have been emptied"),
            "must not reuse the not-found message for a panic: {}",
            items_results[0].error
        );

        // empty_trash_impl: the actual CPE-1791 round-3 blocker — purging must fail outright, never
        // silently report success while a malformed entry (and whatever real payload it guards) survives
        // on disk untouched.
        let empty_result = empty_trash_impl(None, true);
        assert!(
            empty_result.is_err(),
            "a caught dependency panic must fail the whole purge, never report Ok(()) while the trash \
             was never actually emptied"
        );

        // Prove nothing was purged: once the malformed file is removed by hand, the good entry must
        // still be sitting in the trash, exactly as it was before the failed Empty Trash call — not
        // silently purged despite the overall call reporting `Err`.
        let _ = fs::remove_file(&malformed);
        let listed = list_trash_impl().expect("list_trash_impl should succeed once the bad file is gone");
        assert!(
            listed.entries.iter().any(|e| e.original_path == good_path),
            "the good entry must have survived the failed Empty Trash call untouched: {listed:?}"
        );
    }

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
    fn scratch(tag: &str) -> cpe_server::fsutil::ScratchDir {
        cpe_server::fsutil::scratch_dir(&format!("cpe_test_{tag}"))
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

    // macOS/BSD still use the hardcoded fallback (CPE-1355 only lands real classification on Linux);
    // gated to exclude Linux so it doesn't assert on the real `/proc`+`/sys` classifier's environment-
    // dependent outcome on CI's ubuntu leg (that's covered by the pure-fn tests below instead).
    #[cfg(all(unix, not(target_os = "linux")))]
    #[test]
    fn drive_type_unix_fallback() {
        assert_eq!(drive_type_impl("/"), "fixed");
    }

    // ---- Linux drive-type classifier — pure fn, runs + is asserted on EVERY OS (CPE-1355) ----------

    #[test]
    fn classify_drive_type_usb_removable() {
        let mounts = "/dev/sda2 / ext4 rw,relatime 0 0\n/dev/sdb1 /media/usb vfat rw,relatime 0 0\n";
        let removable = |dev: &str| match dev {
            "sdb" => Some("1".to_string()),
            "sda" => Some("0".to_string()),
            _ => None,
        };
        assert_eq!(
            classify_drive_type_from_proc("/media/usb", mounts, removable),
            "removable"
        );
        assert_eq!(
            classify_drive_type_from_proc("/media/usb/photos", mounts, removable),
            "removable",
            "a path nested under the mount point still resolves to the containing mount"
        );
    }

    #[test]
    fn classify_drive_type_internal_disk_is_fixed() {
        let mounts = "/dev/sda2 / ext4 rw,relatime 0 0\n";
        let removable = |dev: &str| if dev == "sda" { Some("0".to_string()) } else { None };
        assert_eq!(classify_drive_type_from_proc("/", mounts, removable), "fixed");
        assert_eq!(
            classify_drive_type_from_proc("/home/user/file.txt", mounts, removable),
            "fixed"
        );
    }

    #[test]
    fn classify_drive_type_nvme_partition_reduces_to_parent_device() {
        let mounts = "/dev/nvme0n1p3 /mnt/data ext4 rw,relatime 0 0\n";
        let removable = |dev: &str| if dev == "nvme0n1" { Some("1".to_string()) } else { None };
        assert_eq!(classify_drive_type_from_proc("/mnt/data", mounts, removable), "removable");
    }

    #[test]
    fn classify_drive_type_mmcblk_partition_reduces_to_parent_device() {
        let mounts = "/dev/mmcblk0p1 /media/sdcard vfat rw,relatime 0 0\n";
        let removable = |dev: &str| if dev == "mmcblk0" { Some("1".to_string()) } else { None };
        assert_eq!(
            classify_drive_type_from_proc("/media/sdcard", mounts, removable),
            "removable"
        );
    }

    #[test]
    fn classify_drive_type_unpartitioned_nvme_whole_disk_is_removable() {
        // A directly-mounted, unpartitioned removable NVMe/SD device (no "pN" suffix) — the regression
        // caught by review: a naive trailing-digit trim previously mangled "nvme0n1" into "nvme0n" and
        // "mmcblk0" into "mmcblk", so the sysfs lookup missed and this always fell back to "fixed".
        let mounts = "/dev/nvme0n1 /mnt/nvme ext4 rw,relatime 0 0\n";
        let removable = |dev: &str| if dev == "nvme0n1" { Some("1".to_string()) } else { None };
        assert_eq!(classify_drive_type_from_proc("/mnt/nvme", mounts, removable), "removable");
    }

    #[test]
    fn classify_drive_type_unpartitioned_mmcblk_whole_disk_is_removable() {
        let mounts = "/dev/mmcblk0 /mnt/sd ext4 rw,relatime 0 0\n";
        let removable = |dev: &str| if dev == "mmcblk0" { Some("1".to_string()) } else { None };
        assert_eq!(classify_drive_type_from_proc("/mnt/sd", mounts, removable), "removable");
    }

    #[test]
    fn classify_drive_type_picks_longest_prefix_mount() {
        // A path under a nested mount must resolve to the NESTED mount's device, not the outer "/".
        let mounts = "/dev/sda2 / ext4 rw,relatime 0 0\n/dev/sdc1 /mnt/nested ext4 rw,relatime 0 0\n";
        let removable = |dev: &str| match dev {
            "sda" => Some("0".to_string()),
            "sdc" => Some("1".to_string()),
            _ => None,
        };
        assert_eq!(
            classify_drive_type_from_proc("/mnt/nested/deep/file", mounts, removable),
            "removable",
            "the nested mount is the containing mount, not the root fs"
        );
    }

    #[test]
    fn classify_drive_type_unresolvable_path_is_fixed() {
        let mounts = "/dev/sda2 /home ext4 rw,relatime 0 0\n";
        // No mount entry contains "/nowhere" as a prefix — no fallback root "/" mount either.
        assert_eq!(
            classify_drive_type_from_proc("/nowhere", mounts, |_| Some("1".to_string())),
            "fixed"
        );
    }

    #[test]
    fn classify_drive_type_missing_device_or_unreadable_removable_is_fixed() {
        let mounts = "/dev/sda2 / ext4 rw,relatime 0 0\n";
        // read_removable seam returns None (simulating a missing/unreadable /sys/block/*/removable).
        assert_eq!(classify_drive_type_from_proc("/", mounts, |_| None), "fixed");
    }

    #[test]
    fn classify_drive_type_pseudo_filesystems_are_never_removable() {
        let mounts = "\
tmpfs /tmp tmpfs rw,relatime 0 0
proc /proc proc rw,relatime 0 0
overlay / overlay rw,relatime 0 0
";
        // Even if the (bogus) removable seam would say "1", pseudo/virtual mounts must be skipped, so
        // there's no `/dev/*` device left to resolve and the classifier falls back to "fixed".
        assert_eq!(
            classify_drive_type_from_proc("/", mounts, |_| Some("1".to_string())),
            "fixed"
        );
    }

    #[test]
    fn classify_drive_type_malformed_mounts_never_panics() {
        let mounts = "garbage line with no fields really\n\n/dev/sdb1\nonlytwo fields\n";
        assert_eq!(classify_drive_type_from_proc("/", mounts, |_| Some("1".to_string())), "fixed");
    }

    #[test]
    fn parent_block_device_reductions() {
        // sd/hd/vd-style: the partition number is a plain trailing-digit suffix.
        assert_eq!(parent_block_device("sda1"), "sda");
        assert_eq!(parent_block_device("sda15"), "sda");
        assert_eq!(parent_block_device("sdb1"), "sdb");
        assert_eq!(parent_block_device("sda2"), "sda");
        assert_eq!(parent_block_device("sdb"), "sdb", "a whole-disk mount has no partition suffix to strip");
        // nvme/mmcblk: ONLY strip an explicit "pN" partition suffix — an unpartitioned whole-disk name
        // (no "pN") must come back unchanged, never trimmed on its own trailing digit (review regression:
        // "nvme0n1" -> "nvme0n" / "mmcblk0" -> "mmcblk" would point the /sys/block lookup at nothing).
        assert_eq!(parent_block_device("nvme0n1p3"), "nvme0n1");
        assert_eq!(parent_block_device("nvme0n1"), "nvme0n1", "unpartitioned whole-disk nvme");
        assert_eq!(parent_block_device("nvme1n1"), "nvme1n1", "unpartitioned whole-disk nvme, other index");
        assert_eq!(parent_block_device("mmcblk0p1"), "mmcblk0");
        assert_eq!(parent_block_device("mmcblk0"), "mmcblk0", "unpartitioned whole-disk mmcblk");
    }

    // ---- Safe drive eject guard (CPE-1278) — hardware-free, cross-platform ------------------------

    #[test]
    fn eject_guard_permits_only_removable() {
        assert!(eject_guard("E:\\", "removable").is_ok(), "a removable drive is ejectable");
        for kind in ["fixed", "network", "cdrom", "ram", "unknown", "", "garbage"] {
            assert!(
                eject_guard("C:\\", kind).is_err(),
                "a {kind:?} drive must NEVER be ejectable"
            );
        }
    }

    #[test]
    fn eject_refuses_fixed_drive_and_never_calls_syscall() {
        use std::cell::Cell;
        let called = Cell::new(false);
        let r = eject_drive_seam(
            "C:\\Windows\\System32",
            |_| "fixed".to_string(),
            |_| {
                called.set(true);
                Ok(())
            },
        );
        assert!(r.is_err(), "a fixed/system drive must be refused");
        assert!(!called.get(), "the eject syscall seam must NEVER run for a non-removable drive");
    }

    #[test]
    fn eject_runs_syscall_only_for_removable_drive() {
        use std::cell::Cell;
        let called = Cell::new(false);
        let r = eject_drive_seam(
            "E:\\",
            |_| "removable".to_string(),
            |_| {
                called.set(true);
                Ok(())
            },
        );
        assert!(r.is_ok(), "a removable drive should reach the eject seam");
        assert!(called.get(), "the eject seam must run for a removable drive");
    }

    #[test]
    fn eject_drive_impl_refuses_the_system_drive() {
        // The real classifier over the current working dir's drive is "fixed" (Windows GetDriveTypeW,
        // and the unix fallback), so the real impl must refuse — no removable hardware required.
        let cwd = std::env::current_dir().unwrap().to_string_lossy().into_owned();
        assert!(
            eject_drive_impl(&cwd).is_err(),
            "the system/working drive must never be ejectable"
        );
    }

    #[test]
    fn drive_root_of_normalises_to_letter_root() {
        assert_eq!(drive_root_of("E:\\some\\path"), "E:\\");
        assert_eq!(drive_root_of("C:\\"), "C:\\");
        assert_eq!(drive_root_of("/mnt/usb"), "/mnt/usb");
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
        // (An earlier round of CPE-1725 added a "no `.cpe-tmp` left behind" assertion here, copied from
        // `metadata_write`'s test. It was removed when this command was narrowed back to `fs::write`: with
        // no staging file ever created, that assertion could not fail, and an assertion that cannot fail
        // is the thing this ticket's evidence rules exist to keep out of the suite.)
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

    // ---- binary_info / binary_disasm command smoke test (CPE-1585, epic CPE-1562) --------------------
    // Confirms both thin dispatchers actually reach `cpe_server::binary_preview::binary_info` (String->Path
    // conversion, spawn_blocking, error mapping, command registration) — the parser itself (CPE-1572's DTO,
    // CPE-1581's iced-x86 disasm) is exhaustively tested in `cpe-server`.

    #[cfg(windows)]
    #[test]
    fn binary_info_and_binary_disasm_commands_dispatch_into_their_adapter() {
        // The test executable itself is a real PE on Windows.
        let exe = std::env::current_exe().unwrap().to_string_lossy().into_owned();

        let info = tauri::async_runtime::block_on(binary_info(exe.clone())).expect("binary_info dispatches");
        assert_eq!(info.format, cpe_server::model::BinaryFormat::Pe, "identifies the PE image");
        assert!(!info.sections.is_empty(), "a real PE has sections");

        let disasm = tauri::async_runtime::block_on(binary_disasm(exe)).expect("binary_disasm dispatches");
        assert_eq!(disasm, info.disasm, "binary_disasm must return the same list embedded in binary_info");
    }

    // ---- dotnet_metadata command smoke test (CPE-1596, epic CPE-1562) --------------------------------
    // Confirms the thin dispatcher reaches `cpe_server::dotnet_metadata::read` — the parser itself
    // (metadata root / `#~` table walk / heap resolution) is exhaustively tested in `cpe-server`.

    #[cfg(windows)]
    #[test]
    fn dotnet_metadata_command_dispatches_and_reports_a_native_exe_as_unmanaged() {
        // The test executable is a native Rust binary, not a managed .NET assembly, so this must
        // dispatch cleanly to `Ok(None)` rather than erroring — proving both the String->Path plumbing
        // and `is_managed`'s "native PE -> None" contract through the real command.
        let exe = std::env::current_exe().unwrap().to_string_lossy().into_owned();
        let result = tauri::async_runtime::block_on(dotnet_metadata(exe)).expect("dotnet_metadata dispatches");
        assert!(result.is_none(), "a native Rust test binary must not be reported as managed: {result:?}");

        // `binary_info` on the same file must agree it's not managed (the two entry points read the
        // same underlying signal).
        let info = cpe_server::binary_preview::binary_info(&std::env::current_exe().unwrap().to_string_lossy())
            .expect("binary_info parses the test exe");
        assert!(!info.is_managed, "binary_info must also report the native test exe as unmanaged");
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

    #[test]
    fn read_preview_info_dispatches_single_file_compression_formats_to_compressed_info() {
        // CPE-1439: xz/bz2/zst/lz/lzma have no decoder wired in, so they must land on the "compressed
        // file" info summary (format + size), never the generic hex-dump arm.
        let d = scratch("dispatch_compressed");
        for (ext, label) in [
            ("xz", "XZ"),
            ("bz2", "BZip2"),
            ("zst", "Zstandard"),
            ("lz", "Lzip"),
            ("lzma", "LZMA"),
        ] {
            let f = d.join(format!("thing.{ext}"));
            fs::write(&f, [0u8; 8]).unwrap();
            let out = read_preview_info_impl(f.to_string_lossy().to_string()).unwrap();
            assert!(out.contains(&format!("{label}-compressed file")), "{ext}: {out}");
            assert!(out.contains("8 bytes"), "{ext}: {out}");
            // Must NOT be a hex dump (no offset column).
            assert!(!out.contains("00000000"), "{ext} should not hex-dump: {out}");
        }
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

    // ---- CPE-1745: `note_app_op` must record the REAL archive-extraction temp path, not a guess -----
    //
    // Before this ticket, `extract_archive_entry`/`extract_archive_entry_any`/`extract_rar_entry`
    // recorded `temp_dir().join("cpe-archive").join(base)` *before* extracting, under a comment
    // claiming it was "the exact temp-file target" the server would write. It was not: the server
    // actually writes into a private `<pid>-<seq>` subdirectory (CPE-1195), and CPE-1733 made that
    // sequence number unpredictable from outside the call (exclusive `fs::create_dir`, retried on
    // collision). So the ledger recorded a path that was never written.
    //
    // These tests exercise `archive_extract_op_paths` — the extracted helper all three commands now
    // call *after* the real extraction — against the REAL return value of `cpe_server::archive`'s
    // extraction functions (a genuine zip, genuinely extracted), and assert EQUALITY with that real
    // value, not merely that the recorded string has the right shape (that weaker assertion is exactly
    // what let the drift go unnoticed for months).
    //
    // Cleanup: extraction writes into `%TEMP%/cpe-archive/<pid>-<seq>/`, which sits OUTSIDE `scratch()`'s
    // per-test directory, so a second `Drop` guard is armed for it — armed immediately after the
    // extraction call returns, before any assertion, per this file's leaked-temp-dir convention (mirrors
    // `Cpe1715Scratch` above and `split_join`/`dispatch`'s `Restore` guards).
    struct Cpe1745ExtractGuard(Option<PathBuf>);
    impl Drop for Cpe1745ExtractGuard {
        fn drop(&mut self) {
            // `.0` is the file `temp_extract_target` wrote; its parent is the private `<pid>-<seq>` dir
            // that owns it, which is what actually needs removing.
            if let Some(parent) = self.0.as_deref().and_then(Path::parent) {
                let _ = fs::remove_dir_all(parent);
            }
        }
    }

    /// The success case: `archive_extract_op_paths` must record EXACTLY the path
    /// `cpe_server::archive::extract_archive_entry` actually returned — real `<pid>-<seq>` subdirectory
    /// included — not a `temp_dir().join("cpe-archive").join(base)` guess. Proves the fix by comparing
    /// against the real function's real output, and also proves the recorded path is the one genuinely
    /// on disk (not just string-equal to something plausible).
    #[test]
    fn archive_extract_op_paths_records_the_real_written_path_on_success() {
        let d = scratch("cpe1745_extract_ok");
        // Build a REAL zip (via the same public `cpe_server::archive::compress_to_zip` the app's own
        // `compress_to_zip` command dispatches to) so the extraction below exercises the real
        // `temp_extract_target` machinery end to end, not a hand-built fixture.
        let src = d.join("note.txt");
        fs::write(&src, b"hello from CPE-1745").unwrap();
        let zip_path = d.join("archive.zip");
        cpe_server::archive::compress_to_zip(
            &[src.to_string_lossy().into_owned()],
            &zip_path.to_string_lossy(),
        )
        .unwrap();

        let result = cpe_server::archive::extract_archive_entry(&zip_path.to_string_lossy(), "note.txt");
        // Arm cleanup for the real out-of-scratch temp dir BEFORE any assertion below can panic.
        let _clean_extract = Cpe1745ExtractGuard(result.as_ref().ok().map(PathBuf::from));

        let real_path = result.clone().expect("extraction must succeed against a real zip");
        assert!(
            Path::new(&real_path).is_file(),
            "the returned path must be the file genuinely written to disk"
        );
        assert_eq!(
            fs::read(&real_path).unwrap(),
            b"hello from CPE-1745",
            "the returned path must point at the actual extracted bytes"
        );

        let recorded = archive_extract_op_paths(&result);
        // The load-bearing assertion (CPE-1745's whole point): EQUALITY with the real value, not a
        // shape/prefix check. `real_path` was never re-derived — it came straight out of the same `Ok`
        // the command returns to the frontend.
        assert_eq!(
            recorded,
            vec![real_path.clone()],
            "note_app_op must be fed the exact path the extraction actually returned"
        );
        // And, since this is precisely the bug that shipped: the recorded path must NOT be the old
        // flat guess, which never existed on disk once CPE-1195 introduced the `<pid>-<seq>` subdir.
        let old_guess = std::env::temp_dir()
            .join("cpe-archive")
            .join("note.txt")
            .to_string_lossy()
            .into_owned();
        assert_ne!(
            recorded[0], old_guess,
            "must not regress to the pre-CPE-1745 flat-guess path"
        );

        let _ = fs::remove_dir_all(&d);
    }

    /// The failure case, recorded deliberately (ticket's ordering decision): a failed extraction wrote
    /// nothing, so `archive_extract_op_paths` must record nothing — not a phantom path for a file that
    /// was never created. Exercised against a REAL failure (entry not present in a real zip), not a
    /// synthetic `Err` string.
    #[test]
    fn archive_extract_op_paths_records_nothing_on_a_real_failure() {
        let d = scratch("cpe1745_extract_err");
        let zip_path = d.join("empty.zip");
        cpe_server::archive::create_empty_zip(&zip_path.to_string_lossy()).unwrap();

        let result = cpe_server::archive::extract_archive_entry(&zip_path.to_string_lossy(), "missing.txt");
        assert!(result.is_err(), "extracting an absent entry must fail");

        let recorded = archive_extract_op_paths(&result);
        assert_eq!(
            recorded,
            Vec::<String>::new(),
            "a failed extraction wrote nothing, so nothing should be recorded"
        );

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

    // ---- CPE-1696: unique_target / resolve_conflict must not read a refused stat as a free name -----
    //
    // All three of `unique_target`'s collision probes, plus `resolve_conflict`'s own, were
    // `!candidate.exists()`. `Path::exists()` folds EVERY stat failure into `false`, i.e. into "this name
    // is free" — so a candidate the process could not stat was handed back as a copy target and the
    // caller's `fs::copy` / `copy_dir_all` / `fs::rename` wrote over it. That directly contradicted the
    // function's own doc comment: "We never overwrite an existing file — silent overwrite is data loss."

    /// The deterministic half (runs on every OS and account, no privilege needed) — same role as
    /// `cpe_server::dispatch::classify_path_error`'s own unit tests.
    #[test]
    fn cpe_1696_a_copy_target_is_only_free_when_the_stat_says_so() {
        assert!(copy_target_is_free(Ok(false)), "an absent name is free");
        assert!(!copy_target_is_free(Ok(true)), "an occupied name is not free");
        assert!(
            copy_target_is_free(Err(std::io::Error::from(std::io::ErrorKind::NotFound))),
            "an explicit NotFound is a genuine absence"
        );
        for kind in [
            std::io::ErrorKind::PermissionDenied,
            std::io::ErrorKind::Other,
            std::io::ErrorKind::TimedOut,
        ] {
            assert!(
                !copy_target_is_free(Err(std::io::Error::new(kind, "Access is denied."))),
                "{kind:?} must never be reported as a free name — that is the overwrite"
            );
        }

        // The three-state classifier underneath. `Unknown` must stay DISTINCT from `Occupied`: both are
        // "not free", but only a run of `Unknown`s means the directory itself is unreadable, which is
        // what lets `unique_target` stop after `MAX_CONSECUTIVE_UNKNOWN_SLOTS` instead of performing
        // 10,000 blocking stats against a dead mount. Collapsing them would restore that stall.
        assert_eq!(classify_copy_target(Ok(false)), TargetSlot::Free);
        assert_eq!(classify_copy_target(Ok(true)), TargetSlot::Occupied);
        assert_eq!(
            classify_copy_target(Err(std::io::Error::from(std::io::ErrorKind::NotFound))),
            TargetSlot::Free
        );
        assert_eq!(
            classify_copy_target(Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))),
            TargetSlot::Unknown,
            "a refused stat is Unknown, never Occupied — the two drive different loop behaviour"
        );
    }

    /// **The strongest test in this ticket: it stages REAL BYTE LOSS, not a wrong error message.**
    ///
    /// Drives the real `do_move_into` entry point at a destination holding a file whose `try_exists()`
    /// the OS refuses. Pre-CPE-1696, `unique_target` handed that refused candidate back as a *free* name
    /// and `do_move_into`'s `fs::rename` replaced its bytes. Post-fix it is treated as occupied, so the
    /// move lands on `victim - Copy.txt` and the original survives byte-for-byte.
    ///
    /// # This claim was wrongly "corrected" once — CPE-1705, and the record is worth keeping
    ///
    /// CPE-1705's author rewrote this comment to say the byte-loss claim was **false**, having reverted
    /// `unique_target`'s probe to `!candidate.exists()` and watched this test pass green. The measurement
    /// was real; the setup was incomplete. It denied only the **target**, and on Windows `fs::metadata`
    /// falls back from a refused `CreateFileW` open to `FindFirstFileW`, which reads the entry out of the
    /// **parent** — so `exists()` still answered `true` and the unfixed guard still refused.
    ///
    /// `deny_stat_of` now also denies `(RD)` on the parent, which kills that fallback while leaving the
    /// rename's `FILE_DELETE_CHILD` route intact. Under it, the original claim holds exactly as written:
    /// the pre-fix probe reads the victim as free and `fs::rename` destroys it. **The claim was right the
    /// first time.** Four rounds of this chain have now concluded "not stageable" from a correct
    /// measurement of an incomplete setup — see `cpe_server::fsutil::deny_stat_of`.
    ///
    /// **Why `do_move_into` (rename) and not `do_copy_into` (copy).** Measured for the PR #889 review
    /// and written up on `cpe_server::fsutil::deny_stat_of`: on Windows a deny ACE that refuses
    /// `try_exists` also refuses `fs::write`/`fs::copy` (both request SYNCHRONIZE), so a *copy* test can
    /// only ever assert on a message — the ACL protects the victim, not the guard. `fs::rename` is the
    /// exception: replacing a file needs `DELETE` on the target **or** `FILE_DELETE_CHILD` on its parent,
    /// and a normal scratch parent grants the latter, so the rename destroys the bytes right through the
    /// deny. That makes a byte-level assertion possible here and nowhere else in this ticket.
    #[test]
    fn cpe_1696_a_move_never_renames_over_a_target_it_cannot_stat() {
        use std::io::Write;

        // **Windows-only, and the whole body is `#[cfg]`'d rather than early-returned** (a
        // `#[cfg(not(windows))] { ..; return; }` block makes every following statement an `unreachable
        // statement` error under CI's `-D warnings` on Linux and macOS — invisible from a Windows dev box).
        //
        // The reason it cannot run on Unix is the *point* of the test, so it is written down rather than
        // waved at: `deny_stat_of`'s Windows mechanism denies the **target file**, plus **list-directory
        // (`RD`) on its parent** — and `RD` is not `DC`, so the parent stays *writable*. That asymmetry is
        // the whole trick: the parent's `RD` deny kills `fs::metadata`'s `FindFirstFileW` fallback so
        // `try_exists` on the victim fails, while the parent's intact `FILE_DELETE_CHILD` still lets a
        // buggy `fs::rename` replace the victim's bytes. Unix has no equivalent. Its only lever is `chmod` on the *parent*, and
        // `unique_target`'s candidates live in that same parent — so the very bits that make `stat` fail
        // with `EACCES` also deny `rename(2)`, which needs write+execute on that directory. The two calls
        // are governed by the same permission there, so no `chmod` can stage a stat failure that a rename
        // then survives. (Confirmed the hard way: the first CI run of this test reded the Linux leg with
        // `the move must still succeed onto a non-colliding name: "Permission denied (os error 13)"` — the
        // legitimate control move, not the guard.)
        //
        // The honest case does not vanish from Unix with it:
        // `cpe_1696_a_move_into_a_readable_folder_auto_renames_instead_of_overwriting` below drives the
        // same `do_move_into` entry point ungated, and
        // `cpe_1696_a_copy_target_is_only_free_when_the_stat_says_so` pins the taxonomy everywhere.
        #[cfg(not(windows))]
        {
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1696] SKIPPED the do_move_into byte-loss leg on this platform: no `chmod` can \
                 stage it, because `stat` and `rename(2)` on a candidate are governed by the SAME \
                 permission bits on its parent directory — the deny that makes the stat fail also makes \
                 the rename fail, so a buggy guard cannot be caught destroying anything here. NOTHING in \
                 this test covered CPE-1696's overwrite route on this run; the Windows leg carries that \
                 evidence, and the ungated move/copy/taxonomy tests carry the honest cases."
            );
        }
        #[cfg(windows)]
        {
            let d = scratch("cpe1696_move_denied");
            let src_dir = d.join("src");
            let dest_dir = d.join("dest");
            fs::create_dir_all(&src_dir).unwrap();
            fs::create_dir_all(&dest_dir).unwrap();
            let src = src_dir.join("victim.txt");
            fs::write(&src, b"MOVED SOURCE").unwrap();
            // The colliding candidate `unique_target` probes first, holding bytes that must survive.
            let victim = dest_dir.join("victim.txt");
            fs::write(&victim, b"VICTIM ORIGINAL").unwrap();

            // Armed before the deny so cleanup runs on every exit path, panic or not (Evidence Rules: a
            // red run must never leave debris). Mirrors cpe_server's `Restore` pattern.
            struct Restore<'a>(&'a Path, &'a Path, &'a Path);
            impl Drop for Restore<'_> {
                fn drop(&mut self) {
                    if let Ok(user) = std::env::var("USERNAME") {
                        // **PARENT FIRST, target last, and the order is not cosmetic.** `icacls <file>`
                        // cannot rewrite a file's ACL while the containing directory still denies
                        // list-directory: it fails silently, the target keeps its `(R)` deny, and the
                        // `fs::read(&victim)` below then dies with `PermissionDenied` — which reads
                        // exactly like this test's own byte assertion failing, sending the next person
                        // after a guard that was never broken. See `cpe_server::fsutil::undo_deny_stat_of`.
                        for p in [self.2, self.0] {
                            let _ = std::process::Command::new("icacls")
                                .arg(p)
                                .arg("/remove:d")
                                .arg(&user)
                                .output();
                        }
                    }
                    let _ = fs::remove_dir_all(self.1);
                }
            }
            let _restore = Restore(&victim, &d, &dest_dir);

            // **Two denies, and BOTH are required (CPE-1705 correction 4).**
            //
            // `(R)` on the victim refuses `try_exists`, while deliberately leaving the parent's
            // FILE_DELETE_CHILD intact so a buggy rename really can replace it — denying the parent's
            // `(DC)` would make the rename fail for the wrong reason and prove nothing.
            //
            // `(RD)` — list-directory — on the PARENT is what makes this catch the *original*
            // `!candidate.exists()` bug rather than only a regression within the fixed design. Without
            // it, `fs::metadata` falls back from its refused `CreateFileW` open to `FindFirstFileW`,
            // reads the entry out of the parent, and answers `Ok` — so `Path::exists()` returns `true`,
            // the unfixed guard refuses anyway, and every assertion below passes for the wrong reason.
            // That is exactly the vacuous pass CPE-1705 first mistook for a refutation of this test.
            // `RD` is not `DC`, so the rename still lands.
            if let Ok(user) = std::env::var("USERNAME") {
                if !user.is_empty() {
                    let _ = std::process::Command::new("icacls")
                        .arg(&victim)
                        .arg("/deny")
                        .arg(format!("{user}:(R)"))
                        .output();
                    let _ = std::process::Command::new("icacls")
                        .arg(&dest_dir)
                        .arg("/deny")
                        .arg(format!("{user}:(RD)"))
                        .output();
                }
            }

            // The premise that makes this test stronger than a message assertion: the pre-fix probe
            // (`Path::exists()`) must now answer FALSE on a file that is really sitting there. If this
            // ever regresses to `true`, the parent deny stopped working and the test is back to being
            // vacuous — so it is asserted, not assumed.
            //
            // CPE-1717: routed through `require_staged` (`supported_here = true` — this block is
            // `#[cfg(windows)]`, and the deny is measured to work there) so that under CI a runner
            // that stopped staging goes RED. Un-staged is not "nothing to see"; it is zero coverage.
            if !cpe_server::fsutil::require_staged(
                "do_move_into parent (RD) deny",
                true,
                !victim.exists(),
            ) {
                let _ = writeln!(
                    std::io::stderr(),
                    "[CPE-1696] SKIPPED the do_move_into byte-loss leg: the parent `(RD)` deny did not \
                     take effect on this machine, so `Path::exists()` still answers true and the pre-fix \
                     guard would refuse anyway. NOTHING in this test covered the overwrite route on this \
                     run."
                );
                return;
            }
            if !cpe_server::fsutil::require_staged(
                "do_move_into target deny",
                true, // CPE-1717 — `#[cfg(windows)]` block; the target deny is supposed to work here
                victim.try_exists().is_err(),
            ) {
                let _ = writeln!(
                    std::io::stderr(),
                    "[CPE-1696] SKIPPED the do_move_into denied-candidate leg: could not deny stat of {} \
                     on this machine (running elevated, or a filesystem that ignores ACLs). NOTHING in \
                     this test covered CPE-1696's overwrite route on this run; see \
                     cpe_1696_a_copy_target_is_only_free_when_the_stat_says_so for the taxonomy, which \
                     does run here.",
                    victim.display()
                );
                return;
            }

            let ctx = cpe_server::ctx::HeadlessCtx::new(&d);
            let landed = do_move_into(&ctx, &src, &dest_dir);

            // Restore access on BOTH the victim and its parent so the bytes can be inspected — the
            // assertion below is the point of the test and must not be defeated by the very ACLs that
            // staged it.
            if let Ok(user) = std::env::var("USERNAME") {
                // Parent first — see the `Restore` impl above for why the order matters.
                for p in [&dest_dir, &victim] {
                    let _ = std::process::Command::new("icacls")
                        .arg(p)
                        .arg("/remove:d")
                        .arg(&user)
                        .output();
                }
            }

            // **The byte-level assertion.** Pre-fix this read back "MOVED SOURCE".
            assert_eq!(
                fs::read(&victim).unwrap(),
                b"VICTIM ORIGINAL".to_vec(),
                "a destination file whose stat we were refused must NEVER be renamed over — the \
                 pre-CPE-1696 `!candidate.exists()` probe read the refusal as \"this name is free\" and \
                 `fs::rename` replaced its bytes, which is exactly the silent overwrite \
                 `unique_target`'s own doc comment forbids"
            );
            // And the move must still have gone somewhere sensible rather than failing outright (the PR
            // #889 review's non-blocking item 1: an unknown is occupied, not fatal).
            let landed = landed.expect("the move must still succeed onto a non-colliding name");
            assert_eq!(
                landed,
                dest_dir.join("victim - Copy.txt"),
                "it must auto-rename past the unprovable candidate, not abort the operation"
            );
            assert_eq!(fs::read(&landed).unwrap(), b"MOVED SOURCE".to_vec());
        }
    }

    /// The ungated sibling for the `#[cfg(windows)]` test above — the honest `do_move_into` case, on
    /// **every** OS. Without it, a `unique_target` that answered "occupied" to everything (or a
    /// `do_move_into` that stopped moving at all) would be caught only on Windows, which is one of CI's
    /// three legs. This repo has been bitten twice by a gated assertion silently vanishing elsewhere.
    #[test]
    fn cpe_1696_a_move_into_a_readable_folder_auto_renames_instead_of_overwriting() {
        let d = scratch("cpe1696_move_ok");
        let src_dir = d.join("src");
        let dest_dir = d.join("dest");
        fs::create_dir_all(&src_dir).unwrap();
        fs::create_dir_all(&dest_dir).unwrap();
        let occupied = dest_dir.join("m.txt");
        fs::write(&occupied, b"KEEP ME").unwrap();

        let ctx = cpe_server::ctx::HeadlessCtx::new(&d);

        // A free name is used as-is.
        let free_src = src_dir.join("fresh.txt");
        fs::write(&free_src, b"FRESH").unwrap();
        assert_eq!(do_move_into(&ctx, &free_src, &dest_dir).unwrap(), dest_dir.join("fresh.txt"));

        // A colliding name auto-renames and leaves the occupant byte-for-byte.
        let colliding = src_dir.join("m.txt");
        fs::write(&colliding, b"MOVED").unwrap();
        assert_eq!(
            do_move_into(&ctx, &colliding, &dest_dir).unwrap(),
            dest_dir.join("m - Copy.txt"),
            "a colliding move must auto-rename, never overwrite"
        );
        assert_eq!(fs::read(&occupied).unwrap(), b"KEEP ME".to_vec());
        assert_eq!(fs::read(dest_dir.join("m - Copy.txt")).unwrap(), b"MOVED".to_vec());

        let _ = fs::remove_dir_all(&d);
    }

    // ---- CPE-1710: the three app-adapter rename sites, each proving a DANGLING LINK survives ----------
    //
    // These three shipped in CPE-1710's first round with **no test each**, covered only by the source
    // scan in `fsutil` — which the PR #895 UAT then showed is bypassable three ways. A lint is not a test.
    //
    // They need no ACL staging: a dangling link is an ordinary object, `try_exists` answers `Ok(false)`
    // for one on every platform, and `fs::rename` does not follow the final component. So all three run on
    // all three CI legs; only *creating* the link can be refused, and
    // `cpe_server::fsutil::make_dangling_link` falls back to a privilege-free NTFS junction before giving
    // up — at which point it is a loud skip, never a silent pass.
    //
    // Each asserts on the **slot** (`symlink_metadata(..).is_symlink()`), never on the returned `Result`:
    // the reviewer's original reproduction returned success while destroying the link.

    /// The main rename command. `rename_entry_impl` had the pairing before CPE-1710 (open-coded) and has
    /// it via `rename_slot_refusal` now — but nothing asserted it end to end.
    #[test]
    fn cpe_1710_rename_entry_never_renames_over_a_dangling_link_at_the_new_name() {
        use std::io::Write;
        let d = scratch("cpe1710_rename_link");
        let ctx = HeadlessCtx::new(&d);
        fs::write(d.join("a.txt"), b"NEW CONTENT").unwrap();
        let link = d.join("b.txt");
        if !cpe_server::fsutil::make_dangling_link(&link) {
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1710] SKIPPED the rename_entry dangling-link leg: this machine could not create a \
                 link at {} (no symlink privilege and no junction). NOTHING in this test covered the \
                 link-destruction route on this run.",
                link.display()
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }

        let r = rename_entry_impl(&ctx, d.join("a.txt").to_string_lossy().to_string(), "b.txt".into());

        // The slot first, deliberately: an `expect_err` here would red on the RESULT, and the whole point
        // of this bug is that the result looked fine while the link was gone.
        assert!(
            fs::symlink_metadata(&link).is_ok_and(|m| m.file_type().is_symlink()),
            "the user's link was DESTROYED by the rename (result was {r:?})"
        );
        let e = r.expect_err("and the rename must be refused, not silently performed");
        assert!(e.contains("is a link"), "and the refusal must say what is in the way: {e}");
        assert_eq!(fs::read(d.join("a.txt")).unwrap(), b"NEW CONTENT".to_vec(), "source must not move");
        let _ = fs::remove_dir_all(&d);
    }

    /// The exact-destination primitive behind bulk move and undo.
    #[test]
    fn cpe_1710_move_exact_never_renames_over_a_dangling_link_at_the_destination() {
        use std::io::Write;
        let d = scratch("cpe1710_move_exact_link");
        let ctx = HeadlessCtx::new(&d);
        fs::write(d.join("a.txt"), b"NEW CONTENT").unwrap();
        let link = d.join("dest.txt");
        if !cpe_server::fsutil::make_dangling_link(&link) {
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1710] SKIPPED the move_exact dangling-link leg: this machine could not create a \
                 link at {}. NOTHING in this test covered the link-destruction route on this run.",
                link.display()
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }

        let results = move_exact_impl(
            &ctx,
            vec![(
                d.join("a.txt").to_string_lossy().to_string(),
                link.to_string_lossy().to_string(),
            )],
        );

        assert!(
            fs::symlink_metadata(&link).is_ok_and(|m| m.file_type().is_symlink()),
            "the user's link was DESTROYED by the move"
        );
        assert!(!results[0].ok, "the move must be reported failed, not silently succeeded: {results:?}");
        assert!(
            results[0].error.contains("is a link"),
            "and it must say what is in the way: {}",
            results[0].error
        );
        let _ = fs::remove_dir_all(&d);
    }

    /// The Agent Board's ticket move — the site CPE-1710's enumeration turned up unreported, and the one
    /// whose destination is a **ticket file** (this repo's own tickets are the app's dogfood data).
    #[test]
    fn cpe_1710_board_move_never_renames_over_a_dangling_link_at_the_destination() {
        use std::io::Write;
        let d = scratch("cpe1710_board_link");
        let tickets = d.join("Ticketing").join("Tickets");
        fs::create_dir_all(tickets.join("Backlog")).unwrap();
        fs::create_dir_all(tickets.join("Doing")).unwrap();
        let name = "CPE-9999_a-ticket.md";
        fs::write(
            tickets.join("Backlog").join(name),
            "---\nid: CPE-9999\nstatus: Open\n---\n\nbody\n",
        )
        .unwrap();
        let link = tickets.join("Doing").join(name);
        if !cpe_server::fsutil::make_dangling_link(&link) {
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1710] SKIPPED the board_move dangling-link leg: this machine could not create a \
                 link at {}. NOTHING in this test covered the link-destruction route on this run.",
                link.display()
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }

        let r = board_move_impl(d.to_string_lossy().to_string(), "CPE-9999".into(), "Doing".into());

        // The slot first — see the note in the rename test above.
        assert!(
            fs::symlink_metadata(&link).is_ok_and(|m| m.file_type().is_symlink()),
            "the link in the destination column was DESTROYED by the board move (result was {r:?})"
        );
        let e = r.expect_err("and the move must be refused, not silently performed");
        assert!(e.contains("is a link"), "and the refusal must say what is in the way: {e}");
        assert!(
            tickets.join("Backlog").join(name).is_file(),
            "and the ticket must still be in its original column — a refused move that moved is not a \
             refusal"
        );
        let _ = fs::remove_dir_all(&d);
    }

    // ---- CPE-1715: unique_target / resolve_conflict must treat a link slot -- including a DANGLING one --
    // as OCCUPIED, never as free (CPE-1710's sibling). Those sites REFUSE at an occupied slot; these two
    // instead ADVANCE to the next candidate, so a refusal is the wrong verdict here -- the fix is a change
    // to the classifier's *input*, not a guard.
    //
    // `Path::try_exists` follows symlinks, so a dangling link answers `Ok(false)` -- correctly, to the
    // question it was asked -- and every one of these sites read that as "this name is free". A
    // `fs::rename`/`fs::copy` onto that name does not follow the final component, so it destroys the link.
    // Each disk-backed test below asserts on the SLOT (`symlink_metadata(..).is_symlink()`) or the picked
    // NAME first, never on the returned `Result` alone -- the CPE-1710 lesson: a "successful" result is
    // exactly what this bug produces.
    //
    // (The pure classifier test -- `classify_link_presence`'s taxonomy, no disk touched -- moved to
    // `cpe_server::fsutil`'s own test module under review, alongside the function itself: PR #924.)
    //
    // Every test below relies on its scratch dir's own `ScratchDir` guard (armed the moment `scratch()`
    // returns, i.e. before any assertion) rather than a manual per-test `Drop` wrapper -- CPE-1693 moved
    // this guarantee to the `scratch()` helper itself, closing the whole class at once instead of test by
    // test. Same requirement, same reason, as the `Restore` guard in `cpe_server::dispatch`'s and
    // `split_join`'s tests before this ticket.

    /// `probe_name_pick_slot` itself, on a real dangling link: `try_exists` alone answers "nothing here",
    /// but the combined probe must answer "occupied".
    #[test]
    fn cpe_1715_probe_name_pick_slot_reports_a_dangling_link_as_occupied() {
        use std::io::Write;
        let d = scratch("cpe1715_probe");
        let link = d.join("dangling.txt");
        if !cpe_server::fsutil::make_dangling_link(&link) {
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1715] SKIPPED the probe_name_pick_slot dangling-link leg: this machine could not \
                 create a link at {}. NOTHING in this test covered the classification route on this run.",
                link.display()
            );
            return;
        }

        // The pre-fix probe, side by side: `try_exists` alone genuinely answers "nothing resolves here" --
        // that is not the bug, it is the fact the bug exploits.
        assert!(
            !link.try_exists().unwrap(),
            "try_exists alone follows the link and correctly sees nothing at the far end"
        );

        let occupied =
            probe_name_pick_slot(&link).expect("a dangling link is a readable slot, not a stat failure");
        assert!(occupied, "the dangling link must read as OCCUPIED, not free");
    }

    /// [`unique_target`] itself: a dangling link at the bare candidate name must not be handed back as
    /// free.
    #[test]
    fn cpe_1715_unique_target_skips_a_dangling_link_and_picks_the_next_candidate() {
        use std::io::Write;
        let d = scratch("cpe1715_unique_target");
        let link = d.join("report.txt");
        if !cpe_server::fsutil::make_dangling_link(&link) {
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1715] SKIPPED the unique_target dangling-link leg: this machine could not create a \
                 link at {}. NOTHING in this test covered the name-picking route on this run.",
                link.display()
            );
            return;
        }

        let picked = unique_target(&d, "report.txt");
        assert_eq!(
            picked,
            d.join("report - Copy.txt"),
            "a dangling link's slot must be treated as occupied, so the picker must advance past it rather \
             than hand it back as free"
        );
        // unique_target only probes candidates, it never writes -- the slot must be exactly as it was.
        assert!(
            fs::symlink_metadata(&link).is_ok_and(|m| m.file_type().is_symlink()),
            "unique_target must not have touched the link's slot"
        );
    }

    /// [`resolve_conflict`]'s `Skip` arm: a dangling link must route into the policy match as occupied,
    /// not be handed back as a free target that skips the policy entirely.
    #[test]
    fn cpe_1715_resolve_conflict_skip_skips_a_dangling_link_instead_of_treating_it_as_free() {
        use std::io::Write;
        let d = scratch("cpe1715_resolve_skip");
        let link = d.join("skip-me.txt");
        if !cpe_server::fsutil::make_dangling_link(&link) {
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1715] SKIPPED the resolve_conflict Skip dangling-link leg: this machine could not \
                 create a link at {}. NOTHING in this test covered the Skip route on this run.",
                link.display()
            );
            return;
        }

        let r = resolve_conflict(&link, &d, ConflictPolicy::Skip).expect("Skip never errors");
        assert!(
            r.is_none(),
            "a dangling link must be treated as occupied, so Skip must actually skip it (got {r:?})"
        );
        assert!(
            fs::symlink_metadata(&link).is_ok_and(|m| m.file_type().is_symlink()),
            "and the link itself must be untouched"
        );
    }

    /// [`resolve_conflict`]'s `Keepboth` arm: a dangling link must be routed into `unique_target` for a
    /// different name, not handed straight back as the (occupied) name itself.
    #[test]
    fn cpe_1715_resolve_conflict_keepboth_renames_past_a_dangling_link_instead_of_returning_it_as_free() {
        use std::io::Write;
        let d = scratch("cpe1715_resolve_keepboth");
        let link = d.join("keep-me.txt");
        if !cpe_server::fsutil::make_dangling_link(&link) {
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1715] SKIPPED the resolve_conflict Keepboth dangling-link leg: this machine could \
                 not create a link at {}. NOTHING in this test covered the Keepboth route on this run.",
                link.display()
            );
            return;
        }

        let r = resolve_conflict(&link, &d, ConflictPolicy::Keepboth)
            .expect("Keepboth never errors")
            .expect("Keepboth always finds a target");
        assert_eq!(
            r,
            d.join("keep-me - Copy.txt"),
            "a dangling link must be treated as occupied, so Keepboth must pick a different name instead \
             of handing the link's own name back as free"
        );
        assert!(
            fs::symlink_metadata(&link).is_ok_and(|m| m.file_type().is_symlink()),
            "and the link itself must be untouched"
        );
    }

    /// [`resolve_conflict`]'s `Overwrite` arm is the ONE policy allowed to touch the slot -- and it must
    /// still work, now that the link routes through the same occupied branch as any other collision.
    /// Exercises the CPE-1715 review's Overwrite fix: on an unprivileged Windows runner
    /// `make_dangling_link` stages an NTFS junction, whose `is_dir()` follows the (unresolvable) link to
    /// `false` while `remove_file` refuses it with `PermissionDenied` -- so this must fall back to
    /// `remove_dir`, or the assertion below reds on CI even though it passes on a box with symlink
    /// privilege (which only ever exercises the real-symlink leg).
    #[test]
    fn cpe_1715_resolve_conflict_overwrite_is_the_only_arm_that_touches_a_dangling_link() {
        use std::io::Write;
        let d = scratch("cpe1715_resolve_overwrite");
        let link = d.join("replace-me.txt");
        if !cpe_server::fsutil::make_dangling_link(&link) {
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1715] SKIPPED the resolve_conflict Overwrite dangling-link leg: this machine could \
                 not create a link at {}. NOTHING in this test covered the Overwrite route on this run.",
                link.display()
            );
            return;
        }

        let r = resolve_conflict(&link, &d, ConflictPolicy::Overwrite)
            .expect("Overwrite never errors on a slot the user explicitly consented to replace")
            .expect("Overwrite always resolves to a target");
        assert_eq!(r, link, "Overwrite is the one policy the user explicitly asked to replace the slot with");
        assert!(
            fs::symlink_metadata(&link).is_err(),
            "and, having been explicitly authorised, the link itself must actually be gone"
        );
    }

    /// **End to end, through the real `fs::rename`.** [`do_move_into`] is the function CPE-1715's own
    /// filing names as reached by "the bulk move command and the watch executor". This proves the dangling
    /// link at the destination SURVIVES a bulk move: asserted on the slot first, exactly as the CPE-1710
    /// tests above do, because a `Result` that reports success is precisely what this bug produces.
    #[test]
    fn cpe_1715_do_move_into_never_renames_onto_a_dangling_link_the_link_survives_a_bulk_move() {
        use std::io::Write;
        let d = scratch("cpe1715_move_survives");
        let src_dir = d.join("src");
        let dest_dir = d.join("dest");
        fs::create_dir_all(&src_dir).unwrap();
        fs::create_dir_all(&dest_dir).unwrap();
        fs::write(src_dir.join("report.txt"), b"MOVED CONTENT").unwrap();
        let link = dest_dir.join("report.txt");
        if !cpe_server::fsutil::make_dangling_link(&link) {
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1715] SKIPPED the do_move_into dangling-link leg: this machine could not create a \
                 link at {}. NOTHING in this test covered the survives-a-bulk-move route on this run.",
                link.display()
            );
            return;
        }

        let ctx = cpe_server::ctx::HeadlessCtx::new(&d);
        let r = do_move_into(&ctx, &src_dir.join("report.txt"), &dest_dir);

        // The slot first, deliberately: an `unwrap()` here would red on the RESULT, and the whole point of
        // this bug is that the result looks fine while the link is gone.
        assert!(
            fs::symlink_metadata(&link).is_ok_and(|m| m.file_type().is_symlink()),
            "the dangling link at the destination was DESTROYED by the bulk move (result was {r:?})"
        );
        let landed = r.expect("the move must still succeed onto a non-colliding name");
        assert_eq!(
            landed,
            dest_dir.join("report - Copy.txt"),
            "unique_target must have advanced past the link's slot rather than handing it back as free"
        );
        assert_eq!(fs::read(&landed).unwrap(), b"MOVED CONTENT".to_vec());
        assert!(
            !src_dir.join("report.txt").exists(),
            "the source must actually have moved, to the auto-renamed target"
        );
    }

    // ---- CPE-1716: the metadata save must edit the file the user opened, not replace their link -------
    //
    // The site CPE-1710 classified out of class as "a file we own". It is the user's own media path off
    // IPC, and it had **no** guard at all, so a *live* symlink was destroyed — not merely a dangling one.
    // Three failures at once, all silent: the link went, the real file kept its old metadata, and the
    // command returned `Ok` with the edited field echoed back.
    //
    // So these assert on the **file the user opens** and on the **slot still being a link** before they
    // look at the `Result`. `ok: true` was the pre-fix outcome; a test that trusted it would have passed.

    /// A minimal well-formed WAV (`RIFF`/`fmt `/`data`) — enough for the wav codec to read and rewrite.
    fn tiny_wav(audio: &[u8]) -> Vec<u8> {
        let fmt: [u8; 16] = [1, 0, 2, 0, 0x44, 0xAC, 0, 0, 0, 0, 0, 0, 4, 0, 16, 0];
        let mut body = Vec::new();
        body.extend_from_slice(b"WAVE");
        body.extend_from_slice(b"fmt ");
        body.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
        body.extend_from_slice(&fmt);
        body.extend_from_slice(b"data");
        body.extend_from_slice(&(audio.len() as u32).to_le_bytes());
        body.extend_from_slice(audio);
        if audio.len() % 2 != 0 {
            body.push(0);
        }
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&body);
        out
    }

    fn set_title(v: &str) -> cpe_server::media_meta_edit::MetaEdit {
        cpe_server::media_meta_edit::MetaEdit::Set {
            group: "wav".into(),
            key: "Title".into(),
            value: v.into(),
        }
    }

    fn title_on_disk(p: &std::path::Path) -> Option<String> {
        let bytes = fs::read(p).ok()?;
        cpe_server::media_meta::read_all("wav", &bytes)
            .into_iter()
            .find(|f| f.key == "Title")
            .map(|f| f.value)
    }

    /// **The reproduction, inverted.** A live symlink standing in a symlink-organised library: the edit
    /// must land on the file the link points at, and the link must still be a link afterwards.
    ///
    /// Runs for real on Unix always, and on Windows with Developer Mode or elevation. A file symlink is
    /// the one construction that cannot be faked without privilege — a junction reports
    /// `is_symlink = true` but fails `canonicalize` with `NotADirectory`, so it is refused as *dangling*
    /// and cannot stand in for a live file link, and a hard link is `is_symlink = false`. Where this
    /// cannot run, the decision it drives is still covered on that runner by `fsutil`'s
    /// `classify_write_target` arms and by `replace_file_contents`'s dangling-link leg (which the junction
    /// fallback stages everywhere).
    ///
    /// **That fallback coverage is for an unprivileged contributor machine, not for CI** — a claim an
    /// earlier version of this comment got backwards. The skip below is emitted with
    /// `writeln!(std::io::stderr())`, which a plain `cargo test` does **not** swallow for a passing test
    /// (unlike `println!`/`eprintln!`), so a skipped leg announces itself in the CI log; PR #899's UAT and
    /// Reviewer measured this independently, the second by finding `links.rs`'s identically-emitted notice
    /// in a real Windows CI log. No `[CPE-1716] SKIPPED` line appears in either Windows job of run
    /// `31772062682` while both live-link tests are recorded running, so this leg ran for real on all
    /// three CI legs.
    #[test]
    fn cpe_1716_metadata_write_edits_the_file_a_live_link_points_at_and_keeps_the_link() {
        use std::io::Write;
        let d = scratch("cpe1716_meta_live_link");
        let real = d.join("real-track.wav");
        fs::write(&real, tiny_wav(b"AUDIOAUDIO")).unwrap();
        let link = d.join("01 - My Track.wav");

        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_file(&real, &link).is_ok();
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(&real, &link).is_ok();
        // CPE-1717, retrofitted in PR #904's review round. This leg predates the mechanism and kept the
        // pre-CPE-1717 shape — a bare `if !made` whose only consequence was the notice below. Its twin in
        // `cpe_1725_write_file_text_writes_through_a_live_link_and_keeps_the_link` was found green-with-
        // zero-coverage under `CPE_STAGING_STRICT=1`; this one had the identical gap, and it guards the
        // route CPE-1716's actual data loss travelled through. See that twin for why `supported_here` is
        // `true` and why a contributor machine still gets the skip rather than a red build.
        if !cpe_server::fsutil::require_staged("live_file_symlink", true, made) {
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1716] SKIPPED the metadata_write LIVE-link leg: this machine cannot create a file \
                 symlink at {} (Windows without Developer Mode / admin). The resolution it drives is \
                 still covered on this runner by fsutil::classify_write_target.",
                link.display()
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }

        let r = metadata_write_impl(&link.to_string_lossy(), &[set_title("EDITED")]);

        // The FILE first, then the slot, and only then the result — in that order on purpose. Pre-fix the
        // result was `Ok` carrying `Title = "EDITED"` while both of these were wrong.
        assert_eq!(
            title_on_disk(&real).as_deref(),
            Some("EDITED"),
            "the REAL media file the link points at must carry the edit (result was {r:?})"
        );
        assert!(
            fs::symlink_metadata(&link).is_ok_and(|m| m.file_type().is_symlink()),
            "and the user's link must still be a LINK — pre-fix the save replaced it with a regular file \
             holding the edit, forking the library (result was {r:?})"
        );
        let fields = r.expect("and the save itself must succeed");
        assert_eq!(
            fields.iter().find(|f| f.key == "Title").map(|f| f.value.as_str()),
            Some("EDITED"),
            "and the fields reported back must be the ones now on disk"
        );
        let _ = fs::remove_dir_all(&d);
    }

    /// **The refusal has to be reachable from the command, not just present in the helper** (PR #899 UAT
    /// round 2, finding F3). `std::fs::read` follows a link too, so reading before resolving meant a
    /// dangling link failed with the OS's bare `The system cannot find the file specified. (os error 2)` —
    /// no path, no mention of a link — while the message that says all of that could never fire from its
    /// only caller. The shipped user docs described the message the user could not get.
    ///
    /// Runs on **every runner**: `make_dangling_link` falls back to a privilege-free junction.
    #[test]
    fn cpe_1716_metadata_write_refuses_a_dangling_link_with_a_message_that_names_it() {
        use std::io::Write;
        let d = scratch("cpe1716_meta_dangling");
        let link = d.join("gone.wav");
        if !cpe_server::fsutil::make_dangling_link(&link) {
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1716] SKIPPED the metadata_write dangling-link leg: this machine could not create a \
                 link at {}. NOTHING in this test covered the reachable-refusal route on this run.",
                link.display()
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }

        let e = metadata_write_impl(&link.to_string_lossy(), &[set_title("EDITED")])
            .expect_err("a dangling link has no file to edit — the save must be refused");

        assert!(
            fs::symlink_metadata(&link).is_ok_and(|m| m.file_type().is_symlink()),
            "the link must survive being refused"
        );
        assert!(e.contains("gone.wav"), "the refusal must name the link the user opened: {e}");
        assert!(e.contains("is a link"), "and say that it IS a link: {e}");
        assert!(e.contains("nothing was written"), "and that the edit did not happen: {e}");
        assert!(
            // The helper's own derivation, not a hand-typed literal (CPE-1725, PR #904 review): this is a
            // negative assertion, so a name that drifted from `make_dangling_link`'s would satisfy it
            // vacuously and this leg would cover nothing.
            !cpe_server::fsutil::dangling_link_target(&link).exists(),
            "and refusing must not invent the missing target"
        );
        let _ = fs::remove_dir_all(&d);
    }

    /// The success report must describe bytes that actually landed. Runs on every runner: no link, no
    /// privilege, just the ordinary save — the returned fields are compared against a **fresh read of the
    /// file**, which is the check that would have caught the echo-back lie regardless of the symlink.
    #[test]
    fn cpe_1716_metadata_write_reports_only_what_reached_the_file() {
        let d = scratch("cpe1716_meta_plain");
        let f = d.join("track.wav");
        fs::write(&f, tiny_wav(b"AUDIOAUDIO")).unwrap();

        let fields = metadata_write_impl(&f.to_string_lossy(), &[set_title("EDITED")])
            .expect("an ordinary save must succeed");

        assert_eq!(
            title_on_disk(&f).as_deref(),
            Some("EDITED"),
            "the file must actually hold the edit — a report is not a save"
        );
        assert_eq!(
            fields.iter().find(|f| f.key == "Title").map(|f| f.value.as_str()),
            Some("EDITED"),
            "and the reported fields must match it"
        );
        assert!(
            !fs::read_dir(&d).unwrap().flatten().any(|e| {
                e.file_name().to_string_lossy().contains(".cpe-tmp")
            }),
            "and the staging temp must not be left sitting in the user's music folder"
        );
        let _ = fs::remove_dir_all(&d);
    }

    // ---- CPE-1725: the TWO whole-file save paths must answer the dangling-link question the SAME way ---
    //
    // Before this ticket they answered it oppositely, measured by PR #899's UAT round 2 rather than
    // reasoned about: `metadata_write` refused and named the link, while `write_file_text` returned
    // `Ok(8)` and **created** the missing target through it — `fs::write` is `O_CREAT|O_TRUNC` and follows
    // the final component. Nothing was destroyed, which is why it was filed apart from CPE-1716's data
    // loss, but the same app told the user opposite things about the same broken link one dialog apart.
    //
    // The decision is **refuse**, recorded in full at `write_file_text_impl`. The test below asserts it by
    // driving **both commands** against one construction in one test — a per-command test could go green
    // on two different answers, which is the whole defect — and by asserting on the **far end of the
    // link**: does the phantom target exist? The pre-fix bug returned `Ok`, so a `Result` assertion proves
    // nothing here.
    //
    // Runs on every runner: `make_dangling_link` falls back to a privilege-free NTFS junction, and
    // `require_staged` (CPE-1717) makes a runner that *should* be able to stage but cannot go red rather
    // than announce into a green log.

    #[test]
    fn cpe_1725_both_save_paths_refuse_a_dangling_link_and_neither_creates_its_target() {
        use std::io::Write;
        let d = scratch("cpe1725_parity");
        let text_link = d.join("notes.txt");
        let meta_link = d.join("track.wav");
        if !cpe_server::fsutil::make_dangling_link(&text_link)
            || !cpe_server::fsutil::make_dangling_link(&meta_link)
        {
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1725] SKIPPED the save-parity dangling-link leg: this machine could not create a \
                 link at {} / {}. NOTHING in this test covered the two commands' agreement on this run.",
                text_link.display(),
                meta_link.display()
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }
        // The names the two assertions below turn on, taken from the ONE derivation rather than re-derived
        // here (PR #904 review). Both of those assertions are negatives, so a locally-copied literal that
        // drifted from the helper's would make them pass **vacuously** — measured by the reviewer, who
        // drifted the copy this replaced and watched the filesystem check stop asserting while the test
        // still went red, but on the `Result`, which this test exists to not rely on.
        let text_target = cpe_server::fsutil::dangling_link_target(&text_link);
        let meta_target = cpe_server::fsutil::dangling_link_target(&meta_link);

        let text_r =
            write_file_text_impl(text_link.to_string_lossy().to_string(), "edited!".to_string());
        let meta_r = metadata_write_impl(&meta_link.to_string_lossy(), &[set_title("EDITED")]);

        // The FILESYSTEM first, for BOTH paths, before either `Result` is so much as looked at.
        assert!(
            !text_target.exists(),
            "write_file_text conjured a file at the far end of a broken link ({}) — pre-fix `fs::write` \
             followed the link and created it while reporting success (result was {text_r:?})",
            text_target.display()
        );
        assert!(
            !meta_target.exists(),
            "metadata_write conjured a file at the far end of a broken link ({}) (result was {meta_r:?})",
            meta_target.display()
        );
        assert!(
            fs::symlink_metadata(&text_link).is_ok_and(|m| m.file_type().is_symlink()),
            "and the user's link must survive the refused text save (result was {text_r:?})"
        );
        assert!(
            fs::symlink_metadata(&meta_link).is_ok_and(|m| m.file_type().is_symlink()),
            "and the user's link must survive the refused metadata save (result was {meta_r:?})"
        );

        let text_e = text_r.expect_err("a dangling link has no file to edit — the text save must refuse");
        let meta_e =
            meta_r.expect_err("a dangling link has no file to edit — the metadata save must refuse");
        assert!(text_e.contains("notes.txt"), "the text refusal must name the link: {text_e}");
        assert!(meta_e.contains("track.wav"), "the metadata refusal must name the link: {meta_e}");

        // The acceptance criterion is not "both refuse somehow" but "both give the SAME answer". Both
        // messages come from one function, so once each link's own name is substituted out they must be
        // the *whole* same string — including the platform's trailing OS error, because both links are
        // built by one helper in one directory on one run and therefore fail resolution identically.
        //
        // An earlier version normalised that trailing segment away (`rsplit_once(": ")`), which the PR
        // #904 reviewer flagged: as a guard it would silently swallow a future divergence confined to the
        // final segment, which is exactly the "a denylist where a property was meant" shape this sprint
        // keeps finding. Comparing the entire message normalises nothing, so nothing can hide in it. If a
        // runner ever does produce two different OS errors here the assertion prints both in full, which
        // is a diagnosable red rather than a silent pass.
        let text_norm = text_e.replace(&text_link.display().to_string(), "<LINK>");
        let meta_norm = meta_e.replace(&meta_link.display().to_string(), "<LINK>");
        assert_eq!(
            text_norm, meta_norm,
            "the two save paths must give the SAME answer about a dangling link, not merely both fail"
        );
        assert!(
            text_norm.contains("is a link") && text_norm.contains("nothing was written"),
            "and that shared answer must say it is a link and that nothing was written: {text_norm}"
        );
        let _ = fs::remove_dir_all(&d);
    }

    /// The **working** case must not move. `fs::write` followed a live link to its target and so does
    /// `replace_file_contents`; this pins that the routing change did not turn a normal edit-through-a-link
    /// into a refusal.
    ///
    /// A live *file* symlink is the one construction that cannot be staged without privilege on Windows (a
    /// junction is directory-only and classifies as *dangling*; a hard link is `is_symlink == false`), so
    /// this is a loud `writeln!(std::io::stderr(), ..)` skip there — which a plain `cargo test` does not
    /// swallow. Where it cannot run, `fsutil`'s `classify_write_target_resolves_a_link_and_refuses_a_
    /// dangling_one` covers the same decision as a pure function on every runner.
    #[test]
    fn cpe_1725_write_file_text_writes_through_a_live_link_and_keeps_the_link() {
        use std::io::Write;
        let d = scratch("cpe1725_live_link");
        let real = d.join("real-notes.txt");
        fs::write(&real, b"OLD").unwrap();
        let link = d.join("shortcut.txt");

        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_file(&real, &link).is_ok();
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(&real, &link).is_ok();
        // CPE-1717, added in PR #904's review round: routed through `require_staged` rather than a bare
        // `if !made`. The notice below was the whole consequence, and a notice inside a green run is not
        // one — measured by the reviewer under CI's own condition (`CPE_STAGING_STRICT=1`, staging forced
        // to fail): the leg printed its SKIPPED line and still reported `ok`, i.e. green over zero
        // coverage. `supported_here = true` matches `fsutil`'s `live_file_symlink` precedent and is the
        // honest value: on Unix `symlink(2)` never needs privilege, and CI's `windows-latest` demonstrably
        // has it (the CPE-1725 legs all recorded `... ok` there with no SKIPPED line), so a failure means
        // the runner changed under us. `staging_is_strict` is CI-only, so an unprivileged contributor
        // machine still gets the loud skip below rather than a red build.
        if !cpe_server::fsutil::require_staged("live_file_symlink", true, made) {
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1725] SKIPPED the write_file_text LIVE-link leg: this machine cannot create a file \
                 symlink at {} (Windows without Developer Mode / admin). The resolution it drives is still \
                 covered on this runner by fsutil::classify_write_target.",
                link.display()
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }

        let r = write_file_text_impl(link.to_string_lossy().to_string(), "NEW TEXT".to_string());

        assert_eq!(
            fs::read_to_string(&real).unwrap(),
            "NEW TEXT",
            "the real file the link points at must carry the edit (result was {r:?})"
        );
        assert!(
            fs::symlink_metadata(&link).is_ok_and(|m| m.file_type().is_symlink()),
            "and the user's link must still be a LINK (result was {r:?})"
        );
        assert_eq!(r.expect("and the save itself must succeed"), 8);
        let _ = fs::remove_dir_all(&d);
    }

    /// The **Save As** callers (audit export, file-list export, tag export) hand this command a path where
    /// nothing exists. Refusing a *broken link* must not have turned into refusing an *absent name* — those
    /// are different things, and only the first is broken. Runs everywhere: no link, no privilege.
    #[test]
    fn cpe_1725_write_file_text_still_creates_a_file_at_a_name_where_nothing_exists() {
        let d = scratch("cpe1725_save_as");
        let f = d.join("file-list.csv");

        let n = write_file_text_impl(f.to_string_lossy().to_string(), "a,b\n1,2\n".to_string())
            .expect("a save to a free name must still create the file");

        assert_eq!(fs::read_to_string(&f).unwrap(), "a,b\n1,2\n", "and it must hold the exported bytes");
        assert_eq!(n, 8);
        let _ = fs::remove_dir_all(&d);
    }

    /// **The narrowing, made load-bearing** (PR #904 UAT). CPE-1725's first round routed this command
    /// through `replace_file_contents`, whose temp-sibling + rename **replaces the file object**. The UAT
    /// measured what that cost on ordinary files, which are essentially all of this command's traffic: a
    /// `0600` file became `0644`, a `0755` script became `0644`, Windows `HIDDEN` and the
    /// `Zone.Identifier` alternate data stream (Mark of the Web) were destroyed, and a save that used to
    /// succeed while another program held the file open began failing with `Access is denied`.
    ///
    /// Every one of those is the same fact wearing four hats — *the file object was replaced* — so this
    /// asserts the **property** rather than enumerating the four symptoms, which is what makes it a guard
    /// and not a denylist.
    ///
    /// The property is expressed through an **open handle**, which is both the sharpest probe and the
    /// user-visible consequence in the UAT's own item 4. A handle refers to the file *object*, not to the
    /// name: if the save truncates in place, a reader holding the file open sees the new bytes; if the
    /// save renames a new file over the name, that reader is left holding the old, now-unlinked object and
    /// still sees the old bytes. Re-route this command to any rename-based writer and this reds on every
    /// runner. (`MetadataExt::file_index` would say the same thing more directly and is still unstable —
    /// `windows_by_handle`, rust#63010 — so it is not available to a test that must compile on stable.)
    ///
    /// The Unix mode assertion is kept **as well**, because a property is only as good as its link to the
    /// consequence: `0600` staying `0600` is the security-relevant symptom in its own words, on the two
    /// runners that can express it.
    #[test]
    fn cpe_1725_an_ordinary_save_keeps_the_same_file_object_and_its_mode() {
        use std::io::{Read, Seek};
        let d = scratch("cpe1725_identity");
        let f = d.join("secrets.env");
        fs::write(&f, b"TOKEN=old").unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&f, fs::Permissions::from_mode(0o600)).unwrap();
        }
        // Opened BEFORE the save and held across it — this stands in for the other program that has the
        // user's file open, and it is what makes the assertion about the object rather than the name.
        let mut held = fs::File::open(&f).expect("holding the file open must be possible");

        write_file_text_impl(f.to_string_lossy().to_string(), "TOKEN=new".to_string())
            .expect("an ordinary save must succeed");

        assert_eq!(fs::read_to_string(&f).unwrap(), "TOKEN=new", "the edit must land at the path");
        let mut through_handle = String::new();
        held.seek(std::io::SeekFrom::Start(0)).unwrap();
        held.read_to_string(&mut through_handle).unwrap();
        assert_eq!(
            through_handle, "TOKEN=new",
            "an ordinary save must TRUNCATE the user's file, not replace it with a different object — a \
             reader holding it open still sees the OLD bytes after a rename-based save, and everything \
             else attached to the object goes with it (measured: 0600 -> 0644, 0755 -> 0644, HIDDEN lost, \
             Zone.Identifier destroyed)"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&f).unwrap().permissions().mode() & 0o777,
                0o600,
                "and a private file must not become world-readable by being saved"
            );
        }
        let _ = fs::remove_dir_all(&d);
    }

    // ---- CPE-1705: the MAIN RENAME COMMAND, and `move_exact`, must not read a refused stat as absent ---
    //
    // `rename_entry_impl` is the most-used mutating operation in a file explorer, and its guard was
    // `if target.exists() { Err("already exists") }` immediately above an `fs::rename`. `Path::exists()`
    // is `metadata().is_ok()`, so a stat that failed for ANY reason other than absence also answered
    // `false`, the guard passed, and `fs::rename` — which replaces its destination silently on both
    // Windows and Unix — destroyed whatever was really at that name. No warning, no error, no undo.

    /// **The strongest test in this ticket: it stages REAL BYTE LOSS at the main rename command, not a
    /// wrong error message.**
    ///
    /// Drives `rename_entry_impl`, the exact function behind the `rename_entry` Tauri command, at a
    /// destination holding a file the OS will not let it stat. Pre-fix, `target.exists()` answers `false`
    /// on a file that is really sitting there, the guard passes, and `fs::rename` destroys it (measured:
    /// the victim reads back `RENAMED SOURCE`). Post-fix the victim reads `VICTIM ORIGINAL` and the
    /// command returns *"could not check what is at … nothing was written"*.
    ///
    /// # The two denies, and why one alone proves nothing
    ///
    /// `(R)` on the target refuses `try_exists`. **`(RD)` — list-directory — on the PARENT is what makes
    /// `Path::exists()` fail**, and without it this test is vacuous: `fs::metadata` falls back from a
    /// refused `CreateFileW` open to `FindFirstFileW`, which reads the entry out of the parent directory,
    /// so `exists()` answers `true`, the *unfixed* guard refuses anyway, and every assertion passes for
    /// the wrong reason. CPE-1705's first draft denied the target only, watched exactly that happen, and
    /// concluded the byte-loss construction was impossible — the fourth time this chain drew that
    /// conclusion from an incomplete setup. `RD` is **not** `DC`, so the rename's `FILE_DELETE_CHILD`
    /// route survives and the destructive operation still lands. Never deny `(DC)` here.
    ///
    /// **Windows-only, and the whole body is `#[cfg]`'d rather than early-returned** — a
    /// `#[cfg(not(windows))] { ..; return; }` makes every following statement an `unreachable_statement`
    /// error under CI's `-D warnings` on Linux and macOS, invisible from a Windows dev box.
    ///
    /// The reason it cannot run on Unix is the point of the test, so it is written down rather than waved
    /// at, and it is *sharper* here than at the CPE-1696 sites: `target = parent.join(new_name)` and the
    /// source lives in that **same** parent, so Unix's only lever — `chmod` on the parent — denies
    /// `rename(2)` (which needs write+execute on that directory) with the very bits that make `stat` fail.
    /// The two calls are governed by one permission there, so no `chmod` can stage a stat failure that a
    /// rename then survives. A byte-loss test written without this platform gate fails the Unix legs on
    /// its own *control* assertion, not on the guard — which is exactly what happened to PR #889.
    #[test]
    fn cpe_1705_rename_entry_never_renames_over_a_target_it_cannot_stat() {
        use std::io::Write;
        #[cfg(not(windows))]
        {
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1705] SKIPPED the rename_entry byte-loss leg on this platform: no `chmod` can stage \
                 it, because a rename target and its source share one parent directory, so the bits that \
                 make `stat` fail with EACCES also deny `rename(2)`. NOTHING in this test covered \
                 CPE-1705's overwrite route on this run; the Windows leg carries that evidence, and \
                 `cpe_1705_rename_entry_honest_cases_still_behave` plus the `fsutil` taxonomy tests carry \
                 the honest cases on every OS."
            );
        }
        #[cfg(windows)]
        {
            let d = scratch("cpe1705_rename_denied");
            let src = d.join("draft.txt");
            fs::write(&src, b"RENAMED SOURCE").unwrap();
            let victim = d.join("final.txt");
            fs::write(&victim, b"VICTIM ORIGINAL").unwrap();

            // Armed before the deny so cleanup runs on every exit path, panic or not (Evidence Rules: a
            // red run must never leave debris).
            struct Restore<'a>(&'a Path, &'a Path);
            impl Drop for Restore<'_> {
                fn drop(&mut self) {
                    if let Ok(user) = std::env::var("USERNAME") {
                        // Both denies come off — the target's `(R)` and the parent's `(RD)`. Leaving the
                        // parent unlistable would break `remove_dir_all` and the next test in that tree.
                        //
                        // **PARENT FIRST, target last, and the order is not cosmetic.** `icacls <file>`
                        // cannot rewrite a file's ACL while its directory still denies list-directory:
                        // it fails silently, the target keeps its `(R)` deny, and the `fs::read(&victim)`
                        // below then dies with `PermissionDenied` — which reads exactly like this test's
                        // own byte assertion failing, sending the next person after a guard that was
                        // never broken. See `cpe_server::fsutil::undo_deny_stat_of`.
                        for p in [self.1, self.0] {
                            let _ = std::process::Command::new("icacls")
                                .arg(p)
                                .arg("/remove:d")
                                .arg(&user)
                                .output();
                        }
                    }
                    let _ = fs::remove_dir_all(self.1);
                }
            }
            let _restore = Restore(&victim, &d);

            // Deny READ on the victim — `(R)`, deliberately NOT `(F)`. Both refuse `try_exists`, but only
            // `(F)` denies the target's own `DELETE`, which lets a parent that withholds
            // FILE_DELETE_CHILD block the rename and make this assertion pass for the wrong reason.
            //
            // …AND deny `(RD)` on the parent, which is what makes `Path::exists()` fail and so what makes
            // this a byte-loss test against the ORIGINAL bug rather than a message test. `(RD)` is not
            // `(DC)`: the rename's FILE_DELETE_CHILD route on the parent stays intact, so the stat fails
            // and the destructive rename still lands. Never deny `(DC)` here.
            if let Ok(user) = std::env::var("USERNAME") {
                if !user.is_empty() {
                    let _ = std::process::Command::new("icacls")
                        .arg(&victim)
                        .arg("/deny")
                        .arg(format!("{user}:(R)"))
                        .output();
                    let _ = std::process::Command::new("icacls")
                        .arg(&d)
                        .arg("/deny")
                        .arg(format!("{user}:(RD)"))
                        .output();
                }
            }

            // **Both probes are asserted, and each one guards against a different vacuous pass.**
            //
            // `try_exists()` must fail, because that is the call the FIXED code makes — if it succeeded,
            // the new guard would never fire and the test would prove nothing about the fix.
            //
            // `exists()` must ALSO now answer `false`, because that is the call the BROKEN code makes. If
            // the parent `(RD)` deny fails to take effect, `fs::metadata`'s `FindFirstFileW` fallback
            // answers `Ok`, the pre-fix guard refuses on its own, and the byte assertion below passes
            // against the bug. That is precisely the vacuous pass CPE-1705 first mistook for proof that
            // byte loss could not be staged at all — so it is checked, loudly, rather than assumed.
            //
            // CPE-1717: both probes together are the staging premise, so they are handed to
            // `require_staged` as one fact — a Windows runner that can no longer stage either half
            // covers nothing, and under CI that is red rather than a notice in a green log.
            if !cpe_server::fsutil::require_staged(
                "rename_entry target + parent deny",
                true,
                victim.try_exists().is_err() && !victim.exists(),
            ) {
                let _ = writeln!(
                    std::io::stderr(),
                    "[CPE-1705] SKIPPED the rename_entry byte-loss leg: could not stage the denied stat of \
                     {} on this machine (running elevated, an ACL-less filesystem, or the parent `(RD)` \
                     deny not taking effect — try_exists_err={}, exists={}). NOTHING in this test covered \
                     CPE-1705's overwrite route on this run.",
                    victim.display(),
                    victim.try_exists().is_err(),
                    victim.exists()
                );
                return;
            }

            let ctx = cpe_server::ctx::HeadlessCtx::new(&d);
            let outcome =
                rename_entry_impl(&ctx, src.to_string_lossy().into_owned(), "final.txt".to_string());

            // Restore access on BOTH the victim and its parent so the bytes can be inspected — the
            // assertion below is the point of the test and must not be defeated by the ACLs that staged it.
            if let Ok(user) = std::env::var("USERNAME") {
                // Parent first — see the `Restore` impl above for why the order matters.
                for p in [d.path(), victim.as_path()] {
                    let _ = std::process::Command::new("icacls")
                        .arg(p)
                        .arg("/remove:d")
                        .arg(&user)
                        .output();
                }
            }

            // **The byte-level assertion.** Pre-fix this read back "RENAMED SOURCE".
            assert_eq!(
                fs::read(&victim).unwrap(),
                b"VICTIM ORIGINAL".to_vec(),
                "a rename target whose stat we were refused must NEVER be renamed over — the pre-CPE-1705 \
                 `target.exists()` guard read the refusal as \"nothing is there\" and `fs::rename` \
                 destroyed the user's file with no warning and no error"
            );
            // …and the source must still be sitting where it was: a refused rename changes nothing.
            assert_eq!(fs::read(&src).unwrap(), b"RENAMED SOURCE".to_vec(), "the source must be untouched");

            // Not a bare `is_err()`: a neutralised guard would ALSO error here (from the rename), so a
            // bare `expect_err` passes vacuously. Assert it is OUR refusal, naming the cause and the fact
            // that nothing happened — never a claim that the file "already exists", which is the specific
            // lie CPE-1687 traced to users hunting for a file that was never gone.
            let msg = outcome.expect_err("a rename onto a target we cannot stat must refuse");
            assert!(
                msg.contains("could not check what is at") && msg.contains("nothing was written"),
                "the refusal must name the uncertainty, not guess: {msg}"
            );
            assert!(
                !msg.contains("already exists"),
                "must NOT claim the target exists — we do not know that: {msg}"
            );
        }
    }

    /// The ungated sibling — the honest cases at the same real entry point, on **every** OS. Without it a
    /// guard that refused everything would look identical to a correct one on the two CI legs where the
    /// byte-loss test above cannot run, and "a fix that refuses everything is as broken as one that
    /// overwrites" is this ticket's own acceptance criterion.
    #[test]
    fn cpe_1705_rename_entry_honest_cases_still_behave() {
        let d = scratch("cpe1705_rename_ok");
        let ctx = cpe_server::ctx::HeadlessCtx::new(&d);

        // 1. A genuinely absent destination renames, and really moves the bytes.
        let a = d.join("a.txt");
        fs::write(&a, b"CONTENT").unwrap();
        let landed = rename_entry_impl(&ctx, a.to_string_lossy().into_owned(), "b.txt".to_string())
            .expect("renaming onto a genuinely absent name must still succeed");
        assert_eq!(landed, d.join("b.txt").to_string_lossy());
        assert_eq!(fs::read(d.join("b.txt")).unwrap(), b"CONTENT".to_vec());
        assert!(!a.exists(), "the source name must be gone after a successful rename");

        // 2. An ordinary, readable collision still refuses — with the ORIGINAL wording, which is part of
        //    the command's contract and what the UI shows on an everyday name clash.
        let c = d.join("c.txt");
        fs::write(&c, b"OTHER").unwrap();
        let err = rename_entry_impl(&ctx, c.to_string_lossy().into_owned(), "b.txt".to_string())
            .expect_err("an occupied destination must refuse");
        assert_eq!(err, "\"b.txt\" already exists", "the everyday collision message must not change");
        assert_eq!(fs::read(d.join("b.txt")).unwrap(), b"CONTENT".to_vec(), "and nothing may be clobbered");
        assert_eq!(fs::read(&c).unwrap(), b"OTHER".to_vec(), "…nor the source moved");

        let _ = fs::remove_dir_all(&d);
    }

    /// The **other** bug on the same line, kept a separate test because it is a separate bug: a rename
    /// onto a **dangling symlink** used to destroy the link.
    ///
    /// `exists()` follows symlinks and `fs::rename` does not, so on a dangling link `exists()` answers
    /// `false` — and so does `try_exists()`, *correctly*: nothing resolves there. **This ticket's stat
    /// collapse remedy therefore does not fix this route at all**, which is why the fix is a second,
    /// separately-named guard (`fsutil::symlink_slot_refusal`) and this is a second, separately-named
    /// test. A `try_exists` swap must never be reported as having closed it. CPE-1461 family.
    ///
    /// Runs on every OS; skips loudly only where creating a symlink is unprivileged (Windows without
    /// Developer Mode), which is a property of the machine, not of the platform.
    #[test]
    fn cpe_1705_rename_entry_refuses_onto_a_dangling_symlink() {
        use std::io::Write;
        let d = scratch("cpe1705_rename_dangling");
        let src = d.join("real.txt");
        fs::write(&src, b"SOURCE").unwrap();
        let link = d.join("link.txt");
        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_file(d.join("no-such-target"), &link).is_ok();
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(d.join("no-such-target"), &link).is_ok();
        if !made {
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1705] SKIPPED the dangling-symlink rename leg: this machine does not permit creating \
                 symlinks (Windows without Developer Mode / admin). NOTHING in this test covered the \
                 link-destroying route on this run."
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }

        // The premise, asserted rather than assumed — if `try_exists` ever started answering `Ok(true)`
        // here, the test would be proving something else entirely.
        assert_eq!(
            link.try_exists().ok(),
            Some(false),
            "premise: a dangling link reads as ABSENT to try_exists — which is why the stat-collapse fix \
             cannot see it"
        );

        let ctx = cpe_server::ctx::HeadlessCtx::new(&d);
        let err = rename_entry_impl(&ctx, src.to_string_lossy().into_owned(), "link.txt".to_string())
            .expect_err("renaming onto a dangling link must refuse — the rename would destroy the link");
        assert!(err.contains("is a link"), "the refusal must say what is actually in the way: {err}");
        assert!(
            fs::symlink_metadata(&link).is_ok_and(|m| m.file_type().is_symlink()),
            "the link must still be there — pre-CPE-1705 the rename replaced it and it was gone"
        );
        assert_eq!(fs::read(&src).unwrap(), b"SOURCE".to_vec(), "and the source must not have moved");

        let _ = fs::remove_dir_all(&d);
    }

    /// The same byte-loss construction at `move_exact_impl`, the second rename-destructive command.
    /// Distinct from the `rename_entry` test above so that neutralising **one** guard reds exactly one
    /// test: these are two separate call sites with two separate `clobber_refusal` calls.
    ///
    /// This site is worth its own test for a second reason: CPE-1692 already hardened the destination's
    /// **parent** check three statements below and left the destination's OWN check collapsed. A fix that
    /// stops one line short of the dangerous one is this chain's signature failure.
    #[test]
    fn cpe_1705_move_exact_never_renames_over_a_target_it_cannot_stat() {
        use std::io::Write;
        #[cfg(not(windows))]
        {
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1705] SKIPPED the move_exact byte-loss leg on this platform: Unix's only lever is \
                 `chmod` on the parent, which denies `rename(2)` along with `stat`, so a buggy guard \
                 cannot be caught destroying anything here. NOTHING in this test covered CPE-1705's \
                 overwrite route on this run; `cpe_1705_move_exact_honest_cases_still_behave` carries the \
                 honest case on every OS."
            );
        }
        #[cfg(windows)]
        {
            let d = scratch("cpe1705_move_exact_denied");
            let src = d.join("from.txt");
            fs::write(&src, b"MOVED SOURCE").unwrap();
            let victim = d.join("to.txt");
            fs::write(&victim, b"VICTIM ORIGINAL").unwrap();

            struct Restore<'a>(&'a Path, &'a Path);
            impl Drop for Restore<'_> {
                fn drop(&mut self) {
                    if let Ok(user) = std::env::var("USERNAME") {
                        let _ = std::process::Command::new("icacls")
                            .arg(self.0)
                            .arg("/remove:d")
                            .arg(&user)
                            .output();
                    }
                    let _ = fs::remove_dir_all(self.1);
                }
            }
            let _restore = Restore(&victim, &d);

            if let Ok(user) = std::env::var("USERNAME") {
                if !user.is_empty() {
                    // `(R)` on the target only; the parent's `(DC)` stays intact — see the rename test.
                    let _ = std::process::Command::new("icacls")
                        .arg(&victim)
                        .arg("/deny")
                        .arg(format!("{user}:(R)"))
                        .output();
                }
            }
            if !cpe_server::fsutil::require_staged(
                "move_exact target deny",
                true, // CPE-1717 — `#[cfg(windows)]` block; the target deny is supposed to work here
                victim.try_exists().is_err(),
            ) {
                let _ = writeln!(
                    std::io::stderr(),
                    "[CPE-1705] SKIPPED the move_exact denied-target leg: could not deny stat of {} on \
                     this machine. NOTHING in this test covered CPE-1705's overwrite route on this run.",
                    victim.display()
                );
                return;
            }

            let ctx = cpe_server::ctx::HeadlessCtx::new(&d);
            let results = move_exact_impl(
                &ctx,
                vec![(src.to_string_lossy().into_owned(), victim.to_string_lossy().into_owned())],
            );

            if let Ok(user) = std::env::var("USERNAME") {
                let _ = std::process::Command::new("icacls")
                    .arg(&victim)
                    .arg("/remove:d")
                    .arg(&user)
                    .output();
            }

            assert_eq!(
                fs::read(&victim).unwrap(),
                b"VICTIM ORIGINAL".to_vec(),
                "move_exact must never rename over a destination whose stat it was refused"
            );
            assert_eq!(fs::read(&src).unwrap(), b"MOVED SOURCE".to_vec(), "the source must be untouched");
            let r = &results[0];
            assert!(!r.ok, "the move must be reported as failed, not silently succeeded");
            let msg = r.error.clone();
            assert!(
                msg.contains("could not check what is at") && !msg.contains("already exists"),
                "must name the uncertainty rather than claim the destination exists: {msg}"
            );
        }
    }

    /// The ungated sibling for `move_exact` — both directions, every OS.
    #[test]
    fn cpe_1705_move_exact_honest_cases_still_behave() {
        let d = scratch("cpe1705_move_exact_ok");
        let ctx = cpe_server::ctx::HeadlessCtx::new(&d);

        // A genuinely absent destination still moves.
        let a = d.join("a.txt");
        fs::write(&a, b"A").unwrap();
        let to = d.join("moved.txt");
        let r = move_exact_impl(
            &ctx,
            vec![(a.to_string_lossy().into_owned(), to.to_string_lossy().into_owned())],
        );
        assert!(r[0].ok, "an absent destination must still accept the move: {:?}", r[0].error);
        assert_eq!(fs::read(&to).unwrap(), b"A".to_vec());

        // A readable collision still refuses, with the original wording, and destroys nothing.
        let b = d.join("b.txt");
        fs::write(&b, b"B").unwrap();
        let r = move_exact_impl(
            &ctx,
            vec![(b.to_string_lossy().into_owned(), to.to_string_lossy().into_owned())],
        );
        assert!(!r[0].ok);
        assert_eq!(r[0].error, "\"moved.txt\" already exists", "the everyday collision message stands");
        assert_eq!(fs::read(&to).unwrap(), b"A".to_vec(), "the occupant must survive");
        assert_eq!(fs::read(&b).unwrap(), b"B".to_vec(), "the source must survive");

        let _ = fs::remove_dir_all(&d);
    }

    /// The honest cases at the same real entry point, on every OS: a free name copies, and an occupied one
    /// still auto-renames rather than refusing (Evidence Rules: a guard that refuses everything is not a
    /// guard).
    #[test]
    fn cpe_1696_a_copy_into_a_readable_folder_still_copies_and_still_auto_renames() {
        let d = scratch("cpe1696_copy_ok");
        let src_dir = d.join("src");
        let dest_dir = d.join("dest");
        fs::create_dir_all(&src_dir).unwrap();
        fs::create_dir_all(&dest_dir).unwrap();
        let src = src_dir.join("f.txt");
        fs::write(&src, b"SOURCE").unwrap();

        assert_eq!(do_copy_into(&src, &dest_dir).unwrap(), dest_dir.join("f.txt"));
        assert_eq!(
            do_copy_into(&src, &dest_dir).unwrap(),
            dest_dir.join("f - Copy.txt"),
            "a second copy must auto-rename, never overwrite"
        );
        assert_eq!(fs::read(dest_dir.join("f.txt")).unwrap(), b"SOURCE");
        let _ = fs::remove_dir_all(&d);
    }

    /// CPE-1696, the sidecar-binary diagnosis split: an unreadable candidate must NOT be reported with the
    /// "binary is missing — reinstall required" line a genuinely absent one gets. Given
    /// [[install-kill-all-processes-first]], a locked / half-installed exe is the far more common cause and
    /// a reinstall is the wrong next step for it. Pure, so it runs in the default `cargo test` — the
    /// `sidecar-platform` feature is only ever *clippy*'d in CI, never test-run (see `SidecarBinLookup`).
    #[test]
    fn cpe_1696_an_unreadable_sidecar_binary_is_not_diagnosed_as_missing() {
        assert_eq!(classify_sidecar_candidate(Ok(true)), SidecarCandidate::Present);
        assert_eq!(classify_sidecar_candidate(Ok(false)), SidecarCandidate::Absent);
        assert_eq!(
            classify_sidecar_candidate(Err(std::io::Error::from(std::io::ErrorKind::NotFound))),
            SidecarCandidate::Absent,
            "an explicit NotFound is a genuine absence"
        );
        for kind in [
            std::io::ErrorKind::PermissionDenied,
            std::io::ErrorKind::Other,
            std::io::ErrorKind::TimedOut,
        ] {
            assert!(
                matches!(
                    classify_sidecar_candidate(Err(std::io::Error::new(kind, "boom"))),
                    SidecarCandidate::Unreadable(ref c) if c.contains("boom")
                ),
                "{kind:?} must be remembered as unreadable, not folded into absent"
            );
        }

        assert!(
            sidecar_binary_action(&SidecarBinLookup::Found(PathBuf::from("/x/ai-console"))).is_none(),
            "a resolved binary reports no problem"
        );
        let missing = sidecar_binary_action(&SidecarBinLookup::Missing).unwrap();
        assert!(missing.contains("missing") && missing.contains("reinstall"), "{missing}");
        let unreadable = sidecar_binary_action(&SidecarBinLookup::Unreadable {
            path: PathBuf::from("/x/ai-console"),
            cause: "Access is denied. (os error 5)".into(),
        })
        .unwrap();
        assert!(
            !unreadable.contains("binary is missing"),
            "an unreadable binary must not be reported as missing: {unreadable}"
        );
        assert!(
            unreadable.contains("could not be checked")
                && unreadable.contains("locked")
                && unreadable.contains("Access is denied."),
            "it must name the real cause and the right next step: {unreadable}"
        );
    }

    /// The honest half of the drive-listing site, on **every** OS: the real enumeration still returns
    /// this machine's drives. Deliberately ungated, so the Windows-only taxonomy sibling below cannot be
    /// the only coverage — a `drive_letter_is_present` that answered `false` to everything would empty
    /// the sidebar, and on Linux/macOS this is the only leg that would notice `list_drives_impl`
    /// regressing at all (CI's three-OS matrix; a Windows-gated assertion is invisible on two of them).
    #[test]
    fn cpe_1696_the_real_drive_enumeration_still_returns_this_machines_drives() {
        let drives = list_drives_impl();
        // `Place` is a `specta::Type` binding struct with no `Debug` (deriving one would force a
        // `bindings.gen.ts` regeneration for a test message), so report the paths by hand.
        let seen = drives.iter().map(|p| p.path.as_str()).collect::<Vec<_>>().join(", ");
        assert!(
            drives.iter().any(|p| p.kind == "drive"),
            "the real drive enumeration must still return at least one drive; got: [{seen}]"
        );
        #[cfg(not(target_os = "windows"))]
        assert!(
            drives.iter().any(|p| p.path == "/"),
            "and on Unix that is the single filesystem root; got: [{seen}]"
        );
    }

    /// CPE-1696, the drive-listing site: a drive letter that is present but momentarily unreadable (a
    /// disconnected mapped drive, an empty card reader, a locked BitLocker volume) must still be listed —
    /// `Path::exists()` folded those into `false` and the row vanished from the sidebar. Only a genuine
    /// absence hides it. Windows-only because the probe it classifies is Windows-only (`list_drives_impl`
    /// returns a single `/` root elsewhere — pinned by the ungated sibling above); CI's Windows leg runs
    /// this one.
    #[cfg(target_os = "windows")]
    #[test]
    fn cpe_1696_a_present_but_unreadable_drive_letter_is_still_listed() {
        assert!(drive_letter_is_present(Ok(true)), "a readable drive is listed");
        assert!(!drive_letter_is_present(Ok(false)), "an unassigned letter is not");
        assert!(
            !drive_letter_is_present(Err(std::io::Error::from(std::io::ErrorKind::NotFound))),
            "an explicit NotFound is a genuine absence"
        );
        for kind in [
            std::io::ErrorKind::PermissionDenied,
            std::io::ErrorKind::Other,
            std::io::ErrorKind::TimedOut,
        ] {
            assert!(
                drive_letter_is_present(Err(std::io::Error::new(kind, "The device is not ready."))),
                "{kind:?} means present-but-unreadable; hiding the drive is the one thing the user can \
                 least explain"
            );
        }
        // And the real enumeration still finds this machine's system drive.
        let drives = list_drives_impl();
        let seen = drives.iter().map(|p| p.path.as_str()).collect::<Vec<_>>().join(", ");
        assert!(
            drives.iter().any(|p| p.kind == "drive"),
            "the real drive enumeration must still return this machine's drives; got: [{seen}]"
        );
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

    // ---- CPE-1765: the gap between `unique_target`/`resolve_conflict` picking a name and this module
    // writing to it. `cpe_server::fsutil`'s tests pin the primitives on all three OSes; these pin the
    // APP's four write sites — the two `do_*_into` halves and the two transfer-engine walkers — because a
    // primitive nothing calls fixes nothing.
    //
    // **How the race is staged without a race.** The defect lives *between* the pick and the write, and
    // nothing can plant a file there mid-call from outside the process except by luck. So each test calls
    // the production **write half** directly with a name that is already taken — which is byte-for-byte
    // the state the write meets when it loses the race. That is why `write_copy_into_picked_slot` and
    // `write_move_into_picked_slot` are extracted functions rather than inline code: a test that rebuilt
    // the branch itself would pin `std`'s semantics rather than this module's use of them.
    //
    // Every assertion about **harm** comes before the `Result` is looked at. This family fails by
    // succeeding.

    /// The auditor's copy-shaped escape, at `do_copy_into`'s own write. A hard link needs no symlink
    /// privilege, so this runs on Windows, Linux and macOS alike.
    #[test]
    fn cpe_1765_write_copy_into_picked_slot_cannot_land_outside_the_destination_folder() {
        use std::io::Write;
        let d = scratch("cpe1765_copy_gap");
        let evil_dir = d.join("evil_dir");
        fs::create_dir(&evil_dir).unwrap();
        let outside = evil_dir.join("outside.txt");
        fs::write(&outside, b"PRE-EXISTING OUTSIDE FILE").unwrap();
        let dest = d.join("dest");
        fs::create_dir(&dest).unwrap();
        let src = d.join("src.txt");
        fs::write(&src, b"USER CONTENT").unwrap();

        // THE GAP: `unique_target` proved `dest/victim.txt` free, then this appeared at it.
        let picked = dest.join("victim.txt");
        if !cpe_server::fsutil::require_staged(
            "hard_link_in_the_gap",
            true,
            fs::hard_link(&outside, &picked).is_ok(),
        ) {
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1765] SKIPPED the do_copy_into gap leg: no hard link on this filesystem"
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }

        let r = write_copy_into_picked_slot(&src, picked.clone());

        assert_eq!(
            fs::read_to_string(&outside).unwrap(),
            "PRE-EXISTING OUTSIDE FILE",
            "the copy wrote through the link and landed OUTSIDE the folder the user chose (result {r:?})"
        );
        let e = r.expect_err("a name taken in the gap must not report success");
        assert!(e.contains(&picked.display().to_string()), "the refusal must name the path: {e}");
        let _ = fs::remove_dir_all(&d);
    }

    /// The rename-shaped half, at `do_move_into`'s own write: the occupant must survive and the move must
    /// say so, instead of `fs::rename`'s silent replace.
    #[test]
    fn cpe_1765_write_move_into_picked_slot_refuses_a_name_taken_in_the_gap() {
        let d = scratch("cpe1765_move_gap");
        let dest = d.join("dest");
        fs::create_dir(&dest).unwrap();
        let src = d.join("src.txt");
        fs::write(&src, b"USER CONTENT").unwrap();
        let picked = dest.join("victim.txt");
        fs::write(&picked, b"SOMEONE ELSE'S FILE").unwrap();

        let r = write_move_into_picked_slot(&src, picked.clone());

        assert_eq!(
            fs::read_to_string(&picked).unwrap(),
            "SOMEONE ELSE'S FILE",
            "the move replaced a file that appeared in the gap, silently (result {r:?})"
        );
        assert!(src.exists(), "a refused move must leave the source where it was");
        let e = r.expect_err("a name taken in the gap must not report success");
        assert!(e.contains(&picked.display().to_string()), "the refusal must name the path: {e}");
        let _ = fs::remove_dir_all(&d);
    }

    /// The transfer engine's per-file writer — the one every copy the progress dialog shows goes through.
    /// It used `File::create`, which follows a link at the final component.
    #[test]
    fn cpe_1765_stream_copy_file_refuses_a_link_at_the_destination_name() {
        use std::io::Write;
        let d = scratch("cpe1765_stream_gap");
        let evil_dir = d.join("evil_dir");
        fs::create_dir(&evil_dir).unwrap();
        let outside = evil_dir.join("outside.txt");
        fs::write(&outside, b"PRE-EXISTING OUTSIDE FILE").unwrap();
        let dest = d.join("dest");
        fs::create_dir(&dest).unwrap();
        let src = d.join("src.txt");
        fs::write(&src, b"USER CONTENT").unwrap();
        let picked = dest.join("victim.txt");
        if !cpe_server::fsutil::require_staged(
            "hard_link_in_the_gap",
            true,
            fs::hard_link(&outside, &picked).is_ok(),
        ) {
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1765] SKIPPED the stream_copy_file gap leg: no hard link on this filesystem"
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }

        let cancel = std::sync::atomic::AtomicBool::new(false);
        let mut prog = new_test_progress();
        let mut emit: Box<dyn FnMut(&TransferProgress)> = Box::new(|_| {});
        let mut last_emit = 0u64;
        let r = stream_copy_file(&src, &picked, &cancel, &mut prog, &mut *emit, &mut last_emit);

        assert_eq!(
            fs::read_to_string(&outside).unwrap(),
            "PRE-EXISTING OUTSIDE FILE",
            "the transfer engine wrote through the link, outside the destination folder (result {r:?})"
        );
        let e = r.expect_err("a name taken in the gap must not report success");
        assert!(
            e.to_string().contains(&picked.display().to_string()),
            "the refusal must name the destination path, not just the source: {e}"
        );
        let _ = fs::remove_dir_all(&d);
    }

    /// The transfer engine's directory walker. `create_dir_all` returns `Ok` for a name that already
    /// *resolves* to a directory, so a directory link at the resolved name swallowed the whole tree.
    #[test]
    fn cpe_1765_copy_tree_streamed_refuses_a_directory_link_at_the_destination_name() {
        use std::io::Write;
        let d = scratch("cpe1765_tree_gap");
        let evil_dir = d.join("evil_dir");
        fs::create_dir(&evil_dir).unwrap();
        let dest = d.join("dest");
        fs::create_dir(&dest).unwrap();
        let src = d.join("src_tree");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("a.txt"), b"USER CONTENT").unwrap();

        let picked = dest.join("src_tree");
        if !cpe_server::fsutil::make_dir_link(&evil_dir, &picked) {
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1765] SKIPPED the copy_tree_streamed gap leg: no directory link on this machine"
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }

        let cancel = std::sync::atomic::AtomicBool::new(false);
        let mut prog = new_test_progress();
        let mut emit: Box<dyn FnMut(&TransferProgress)> = Box::new(|_| {});
        let mut last_emit = 0u64;
        let mut report = TransferReport { id: 1, op: TransferOp::from(TransferKind::Copy), ..Default::default() };
        let cancelled =
            !copy_tree_streamed(&src, &picked, &cancel, &mut prog, &mut *emit, &mut last_emit, &mut report);

        let escaped: Vec<_> = fs::read_dir(&evil_dir)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.file_name()))
            .collect();
        assert!(
            escaped.is_empty(),
            "the tree was written THROUGH the link, outside the destination folder: {escaped:?}"
        );
        assert!(!cancelled, "a refused item is a per-item failure, not a cancelled batch");
        assert_eq!(report.failed, 1, "the refusal must be reported: {:?}", report.errors);
        assert!(
            report.errors.iter().any(|e| e.contains(&picked.display().to_string())),
            "the refusal must name the path: {:?}",
            report.errors
        );
        let _ = fs::remove_dir_all(&d);
    }

    /// Regression: the ordinary path — a free name and an auto-renamed collision — still lands inside the
    /// destination folder with the right bytes. A fix that refuses everything would pass every test above.
    #[test]
    fn cpe_1765_do_copy_into_still_lands_inside_the_folder_and_still_auto_renames() {
        let d = scratch("cpe1765_copy_ok");
        let dest = d.join("dest");
        fs::create_dir(&dest).unwrap();
        let src = d.join("report.txt");
        fs::write(&src, b"USER CONTENT").unwrap();

        let first = do_copy_into(&src, &dest).expect("a free name must copy");
        assert_eq!(first, dest.join("report.txt"));
        assert_eq!(fs::read_to_string(&first).unwrap(), "USER CONTENT");

        let second = do_copy_into(&src, &dest).expect("a collision must auto-rename, not fail");
        assert_eq!(second, dest.join("report - Copy.txt"));
        assert_eq!(fs::read_to_string(&second).unwrap(), "USER CONTENT");

        // …and the same for a directory tree, which takes the `copy_dir_all` arm.
        let tree = d.join("tree");
        fs::create_dir_all(tree.join("sub")).unwrap();
        fs::write(tree.join("sub/leaf.txt"), b"leaf").unwrap();
        let t1 = do_copy_into(&tree, &dest).expect("a free name must copy the tree");
        assert_eq!(fs::read_to_string(t1.join("sub/leaf.txt")).unwrap(), "leaf");
        let t2 = do_copy_into(&tree, &dest).expect("a colliding tree must auto-rename");
        assert_eq!(t2, dest.join("tree - Copy"));
        assert_eq!(fs::read_to_string(t2.join("sub/leaf.txt")).unwrap(), "leaf");
        let _ = fs::remove_dir_all(&d);
    }

    /// A `TransferProgress` for the two engine tests above — the struct has no `Default` and its `id`/`op`
    /// are irrelevant to what they assert.
    fn new_test_progress() -> TransferProgress {
        TransferProgress {
            id: 1,
            op: TransferOp::from(TransferKind::Copy),
            total_bytes: 0,
            done_bytes: 0,
            total_items: 0,
            done_items: 0,
            current: String::new(),
        }
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
        let out = run_command_impl("echo hello".to_string(), None, true).unwrap();
        assert!(out.stdout.contains("hello"), "stdout was {:?}", out.stdout);
        assert_eq!(out.code, Some(0));
        assert!(!out.truncated);
    }

    #[test]
    fn run_command_reports_a_nonzero_exit_code() {
        // `exit 3` sets the shell's exit status on both platforms.
        let out = run_command_impl("exit 3".to_string(), None, true).unwrap();
        assert_eq!(out.code, Some(3));
    }

    #[test]
    fn run_command_rejects_an_empty_command() {
        assert!(run_command_impl("   ".to_string(), None, true).is_err());
    }

    /// CPE-1665: the gate must stop the *process*, not just the return value — asserting `Err` came back
    /// is exactly what let this class of bug survive three tickets. So the command line under test is one
    /// whose only observable effect is a file: if any child process ran, `pwned.txt` exists. Redirection
    /// (`>`) behaves the same under `cmd /C` and `sh -c`, so this holds on all three CI legs.
    ///
    /// The second half runs the identical line **with** consent and asserts the file DOES appear —
    /// without it, the first half could pass because the command was harmless all along.
    #[test]
    fn run_command_spawns_no_process_at_all_when_not_confirmed() {
        let d = scratch("run_cmd_gate");
        let dir = d.to_string_lossy().to_string();
        let sentinel = d.join("pwned.txt");
        let line = "echo pwned > pwned.txt".to_string();

        let outcome = run_command_impl(line.clone(), Some(dir.clone()), false);
        // Disk first, deliberately: "no process was created" is the claim, so the sentinel check is the
        // assertion that must trip on an ungated build — not a return-value check tripping ahead of it.
        assert!(
            !sentinel.exists(),
            "no child process may be created for an unconfirmed run_command — the shell wrote {}",
            sentinel.display()
        );
        // `CommandOutput` isn't `Debug`, so unwrap the refusal by hand rather than via `expect_err`.
        let err = match outcome {
            Err(e) => e,
            Ok(_) => panic!("an unconfirmed run_command must be refused, not executed"),
        };
        assert!(err.contains("`confirmed` was not set"), "the refusal must name the flag: {err}");

        // Same line, consented: it really does spawn a shell, so the assertion above has teeth.
        run_command_impl(line, Some(dir), true).expect("a confirmed run_command must be allowed to run");
        assert!(sentinel.exists(), "the consented command must actually run");
        let _ = fs::remove_dir_all(&d);
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
            false, // keep-both destroys nothing, so it needs no CPE-1662 consent
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
        let r = run_transfer(1, &src(), &d.join("dst"), TransferKind::Copy, ConflictPolicy::Skip, false, &cancel, |_| {});
        assert_eq!(r.skipped, 1);
        assert_eq!(fs::read(d.join("dst/a.txt")).unwrap(), b"OLD");

        // Keep both: a non-colliding copy is created; the original stays.
        let r = run_transfer(2, &src(), &d.join("dst"), TransferKind::Copy, ConflictPolicy::Keepboth, false, &cancel, |_| {});
        assert_eq!(r.transferred, 1);
        assert_eq!(fs::read(d.join("dst/a.txt")).unwrap(), b"OLD");
        assert!(d.join("dst/a - Copy.txt").exists(), "keep-both should auto-number");

        // Overwrite: the existing file is replaced.
        let r = run_transfer(3, &src(), &d.join("dst"), TransferKind::Copy, ConflictPolicy::Overwrite, true, &cancel, |_| {});
        assert_eq!(r.transferred, 1);
        assert_eq!(fs::read(d.join("dst/a.txt")).unwrap(), b"NEW");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn run_transfer_skips_a_copy_onto_itself_instead_of_destroying_it() {
        // CPE-1375 (CRITICAL): pasting a copy into the source's OWN folder makes the target path equal the
        // source. With policy Overwrite, resolve_conflict used to remove_file/remove_dir_all the source
        // BEFORE copying from it — permanent, unrecoverable data loss (a whole tree for a directory). It
        // must now be a no-op skip that leaves the original intact.
        let d = scratch("xfer_self");
        fs::write(d.join("a.txt"), b"KEEP").unwrap();
        fs::create_dir_all(d.join("sub")).unwrap();
        fs::write(d.join("sub/inner.txt"), b"KEEPDIR").unwrap();
        let cancel = std::sync::atomic::AtomicBool::new(false);

        // File pasted into its own folder with the destructive Overwrite policy — must skip, not delete.
        let r = run_transfer(1, &[d.join("a.txt")], &d, TransferKind::Copy, ConflictPolicy::Overwrite, true, &cancel, |_| {});
        assert_eq!(r.skipped, 1, "copy onto itself must be skipped");
        assert_eq!(r.transferred, 0);
        assert_eq!(r.failed, 0);
        assert!(d.join("a.txt").exists(), "the source file must survive a copy-onto-itself");
        assert_eq!(fs::read(d.join("a.txt")).unwrap(), b"KEEP");

        // Folder pasted into its own parent with Overwrite — the whole tree must survive (remove_dir_all
        // would otherwise have nuked it).
        let r = run_transfer(2, &[d.join("sub")], &d, TransferKind::Copy, ConflictPolicy::Overwrite, true, &cancel, |_| {});
        assert_eq!(r.skipped, 1);
        assert!(d.join("sub/inner.txt").exists(), "the source folder tree must survive");
        assert_eq!(fs::read(d.join("sub/inner.txt")).unwrap(), b"KEEPDIR");

        // The guard is scoped to Overwrite: copy→paste-in-the-same-folder with KEEPBOTH must still produce
        // the in-place duplicate (it falls through to unique_target), not be swallowed by the self-guard.
        let r = run_transfer(3, &[d.join("a.txt")], &d, TransferKind::Copy, ConflictPolicy::Keepboth, false, &cancel, |_| {});
        assert_eq!(r.transferred, 1, "keep-both in-place duplicate must still copy");
        assert_eq!(r.skipped, 0);
        assert!(d.join("a - Copy.txt").exists(), "in-place duplicate should be created");
        assert_eq!(fs::read(d.join("a.txt")).unwrap(), b"KEEP", "the original must remain untouched");
        let _ = fs::remove_dir_all(&d);
    }

    /// CPE-1662: `ConflictPolicy::Overwrite` is the one transfer policy that reaches
    /// `fs::remove_dir_all(dest/<name>)` on a caller-named path. Without consent the whole batch must be
    /// refused — and the proof is the **victim tree read back off disk**, nested file included, because
    /// asserting the returned report is what let this class of bug survive twice.
    #[test]
    fn run_transfer_refuses_an_unconfirmed_overwrite_without_deleting_the_victim() {
        let d = scratch("xfer_gate");
        // The source: a folder named `Documents`, the shape of the filed exploit.
        fs::create_dir_all(d.join("src/Documents")).unwrap();
        fs::write(d.join("src/Documents/new.txt"), b"incoming").unwrap();
        // The victim: an existing `dst/Documents` tree with a nested file inside it.
        fs::create_dir_all(d.join("dst/Documents/nested")).unwrap();
        fs::write(d.join("dst/Documents/taxes.docx"), b"irreplaceable").unwrap();
        fs::write(d.join("dst/Documents/nested/deep.txt"), b"also irreplaceable").unwrap();
        let cancel = std::sync::atomic::AtomicBool::new(false);

        let mut emitted = 0usize;
        let r = run_transfer(
            1,
            &[d.join("src/Documents")],
            &d.join("dst"),
            TransferKind::Copy,
            ConflictPolicy::Overwrite,
            false, // confirmed
            &cancel,
            |_| emitted += 1,
        );
        // OFF-DISK verification FIRST, deliberately: this is the assertion that must carry the claim, so
        // it is the one that trips on an ungated build rather than a return-value check tripping earlier.
        // List the victim directory and read the nested file back.
        let victim = d.join("dst/Documents");
        let names: Vec<String> = fs::read_dir(&victim)
            .expect("the victim directory must still be listable after a refused overwrite")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.iter().any(|n| n == "taxes.docx"), "taxes.docx must survive: {names:?}");
        assert!(names.iter().any(|n| n == "nested"), "nested/ must survive: {names:?}");
        assert_eq!(
            fs::read(victim.join("nested/deep.txt")).unwrap(),
            b"also irreplaceable",
            "the nested file must survive — this is the remove_dir_all arm the gate covers"
        );
        assert!(!victim.join("new.txt").exists(), "nothing may be copied in either");

        // …and only then the reported shape of the refusal.
        assert_eq!(r.failed, 1, "the refusal must be reported, not silently swallowed");
        assert_eq!(r.transferred, 0);
        assert!(
            r.errors.iter().any(|e| e.contains("`confirmed` was not set")),
            "the refusal must name what was missing: {:?}",
            r.errors
        );
        assert_eq!(emitted, 0, "a refused batch must not even emit a progress snapshot");

        // Consented, the identical call really does overwrite — so the assertions above have teeth.
        let r = run_transfer(
            2,
            &[d.join("src/Documents")],
            &d.join("dst"),
            TransferKind::Copy,
            ConflictPolicy::Overwrite,
            true, // confirmed
            &cancel,
            |_| {},
        );
        assert_eq!(r.transferred, 1, "a consented overwrite must run: {:?}", r.errors);
        assert!(victim.join("new.txt").exists(), "the consented copy must land");
        assert!(!victim.join("taxes.docx").exists(), "and it really is destructive");
        let _ = fs::remove_dir_all(&d);
    }

    /// The gate is scoped to the destructive policy: an unconfirmed `Skip`/`Keepboth` transfer is
    /// completely unaffected, so routine copies never learn to click through a prompt (CPE-1662 scope 2).
    #[test]
    fn run_transfer_does_not_gate_the_non_destructive_policies() {
        let d = scratch("xfer_gate_scope");
        fs::write(d.join("a.txt"), b"NEW").unwrap();
        fs::create_dir_all(d.join("dst")).unwrap();
        fs::write(d.join("dst/a.txt"), b"OLD").unwrap();
        let cancel = std::sync::atomic::AtomicBool::new(false);

        let r = run_transfer(1, &[d.join("a.txt")], &d.join("dst"), TransferKind::Copy, ConflictPolicy::Skip, false, &cancel, |_| {});
        assert_eq!(r.skipped, 1);
        assert_eq!(r.failed, 0, "an unconfirmed skip must not be refused: {:?}", r.errors);

        let r = run_transfer(2, &[d.join("a.txt")], &d.join("dst"), TransferKind::Copy, ConflictPolicy::Keepboth, false, &cancel, |_| {});
        assert_eq!(r.transferred, 1);
        assert_eq!(r.failed, 0, "an unconfirmed keep-both must not be refused: {:?}", r.errors);
        assert_eq!(fs::read(d.join("dst/a.txt")).unwrap(), b"OLD", "and it still destroys nothing");
        assert!(d.join("dst/a - Copy.txt").exists());
        let _ = fs::remove_dir_all(&d);
    }

    /// The command-level half of the CPE-1662 gate: `start_transfer` refuses before issuing an id or
    /// spawning its thread, so a refused call leaves no phantom entry in the operations panel. Exercised
    /// through the shared helper both layers call — `start_transfer` itself needs an `AppHandle`.
    #[test]
    fn overwrite_consent_is_required_of_the_command_and_only_for_overwrite() {
        assert!(require_overwrite_consent(ConflictPolicy::Overwrite, false).is_err());
        assert!(require_overwrite_consent(ConflictPolicy::Overwrite, true).is_ok());
        for (p, name) in [(ConflictPolicy::Skip, "skip"), (ConflictPolicy::Keepboth, "keep-both")] {
            assert!(require_overwrite_consent(p, false).is_ok(), "{name} must never require consent");
        }
    }

    /// Pins the doc claim "no id issued, no registry entry" (PR #855 audit): deleting the consent check
    /// from `start_transfer` used to leave the whole Rust suite green, because the command needs an
    /// `AppHandle` no test can build. `begin_transfer` carries the guard *and* both side effects, so the
    /// ordering is now observable — a refused transfer must not advance the id sequence or register a
    /// cancel flag, which is exactly what would otherwise strand a phantom row in the operations panel.
    #[test]
    fn a_refused_transfer_issues_no_id_and_registers_nothing() {
        use std::sync::atomic::Ordering;
        let seq_before = TRANSFER_SEQ.load(Ordering::Relaxed);
        let registered_before = transfer_registry().lock().unwrap().len();

        let err = begin_transfer(ConflictPolicy::Overwrite, false)
            .expect_err("an unconfirmed overwrite must be refused before anything is allocated");
        assert!(err.contains("`confirmed` was not set"), "the refusal must name the flag: {err}");
        assert_eq!(
            TRANSFER_SEQ.load(Ordering::Relaxed),
            seq_before,
            "a refused transfer must not consume an id — the operations panel would show a phantom row"
        );
        assert_eq!(
            transfer_registry().lock().unwrap().len(),
            registered_before,
            "a refused transfer must not register a cancel flag"
        );

        // Consented, the identical call really does allocate — so the assertions above have teeth.
        let (id, _cancel) = begin_transfer(ConflictPolicy::Overwrite, true)
            .expect("a consented overwrite must be allowed to start");
        assert!(TRANSFER_SEQ.load(Ordering::Relaxed) > seq_before, "a consented transfer takes an id");
        assert!(transfer_registry().lock().unwrap().contains_key(&id), "…and registers its cancel flag");
        transfer_registry().lock().unwrap().remove(&id); // don't leak into the shared registry
    }

    /// PR #855 security audit, HIGH: a source whose last component is `" "` / `"..."` / `". "` made
    /// `dest_dir.join(name)` normalise to **`dest_dir` itself** on Windows, so `resolve_conflict`'s
    /// Overwrite arm called `remove_dir_all` on the user's whole destination folder. **The source never
    /// had to exist** — measurement tolerates a missing path and the destruction happened before the copy
    /// was attempted, so the report came back saying only "file not found" while the folder was gone.
    ///
    /// `confirmed: true` throughout: this is *inside* CPE-1662's gated path, not something the gate
    /// narrows. The real route is dragging an entry named `" "` off a NAS share and clicking **Replace**
    /// — the one handler authorised to send consent. Consenting to replace an item is not consenting to
    /// lose the folder, so this is a correctness fix.
    ///
    /// WINDOWS-ONLY: these names only alias on Windows, so both the wipe and the refusal exist only
    /// there — on POSIX `dest/ ` is a real distinct path and the copy is an ordinary (missing-source)
    /// failure. `a_posix_legal_trailing_dot_name_still_transfers` covers the other leg, so neither is
    /// silently uncovered.
    #[cfg(windows)]
    #[test]
    fn run_transfer_refuses_a_source_whose_name_windows_normalises_away() {
        for name in [" ", "  ", "...", ". ", " ."] {
            let d = scratch("xfer_blank_name");
            // The victim: the user's chosen destination folder, with real content in it.
            let dest = d.join("Documents");
            fs::create_dir_all(dest.join("nested")).unwrap();
            fs::write(dest.join("taxes.docx"), b"irreplaceable").unwrap();
            fs::write(dest.join("nested/deep.txt"), b"also irreplaceable").unwrap();
            let cancel = std::sync::atomic::AtomicBool::new(false);

            let r = run_transfer(
                1,
                &[d.join("elsewhere").join(name)], // the source need not exist
                &dest,
                TransferKind::Copy,
                ConflictPolicy::Overwrite,
                true, // consented — the CPE-1662 gate is satisfied; this is the path handling
                &cancel,
                |_| {},
            );

            // OFF-DISK first: the destination folder and everything in it must still be there.
            assert!(dest.is_dir(), "the destination folder must survive a source named {name:?}");
            let names: Vec<String> = fs::read_dir(&dest)
                .unwrap_or_else(|e| panic!("the destination must still be listable for {name:?}: {e}"))
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect();
            assert!(names.iter().any(|n| n == "taxes.docx"), "taxes.docx must survive {name:?}: {names:?}");
            assert_eq!(
                fs::read(dest.join("nested/deep.txt")).unwrap_or_default(),
                b"also irreplaceable",
                "the nested file must survive {name:?} — this is the remove_dir_all arm"
            );

            // …and the refusal is reported, naming why, rather than a bare "file not found".
            assert_eq!(r.failed, 1, "the refusal must be reported for {name:?}: {:?}", r.errors);
            assert_eq!(r.transferred, 0);
            assert!(
                r.errors.iter().any(|e| e.contains("trailing dots/spaces")),
                "the error must explain the name, not just say not-found, for {name:?}: {:?}",
                r.errors
            );
            let _ = fs::remove_dir_all(&d);
        }
    }

    /// **The POSIX half of the same finding** (round-3 review, BLOCKING 1): `notes.` and `My Report `
    /// are legal, everyday names on Linux and macOS. The first version of this fix ran the Win32 name
    /// filter for every kind and every policy on every platform, so an ordinary additive **keep-both
    /// copy** of such a file failed outright — the audit measured `transferred=0 failed=1` — with an
    /// error message about Windows path normalisation that is factually wrong there.
    ///
    /// So: on POSIX the transfer must simply work. On Windows the name can't even be created, and the
    /// refusal is covered by `run_transfer_refuses_a_source_whose_name_windows_normalises_away`.
    #[cfg(not(windows))]
    #[test]
    fn a_posix_legal_trailing_dot_name_still_transfers() {
        let d = scratch("xfer_posix_names");
        let src_dir = d.join("src");
        let dest = d.join("dest");
        fs::create_dir_all(&src_dir).unwrap();
        fs::create_dir_all(&dest).unwrap();
        fs::write(src_dir.join("notes."), b"real contents").unwrap();
        fs::create_dir_all(src_dir.join("My Documents ")).unwrap();
        fs::write(src_dir.join("My Documents ").join("inner.txt"), b"nested real").unwrap();
        let cancel = std::sync::atomic::AtomicBool::new(false);

        let r = run_transfer(
            1,
            &[src_dir.join("notes."), src_dir.join("My Documents ")],
            &dest,
            TransferKind::Copy,
            ConflictPolicy::Keepboth, // the plain additive case the audit measured failing
            false,                    // keep-both needs no consent
            &cancel,
            |_| {},
        );

        assert_eq!(r.failed, 0, "a POSIX-legal name must not be refused: {:?}", r.errors);
        assert_eq!(r.transferred, 2);
        assert_eq!(fs::read(dest.join("notes.")).unwrap(), b"real contents");
        assert_eq!(
            fs::read(dest.join("My Documents ").join("inner.txt")).unwrap(),
            b"nested real",
            "the folder and its contents must arrive"
        );
        let _ = fs::remove_dir_all(&d);
    }

    /// The backstop for the same finding, exercised directly: even if a name slipped past the filter
    /// above, `resolve_conflict` refuses to overwrite a `base_target` that resolves to `dest_dir`.
    /// Asserted on canonicalised paths, so it holds for spellings nobody enumerated.
    #[test]
    fn resolve_conflict_refuses_to_overwrite_the_destination_directory_itself() {
        let d = scratch("xfer_resolve_root");
        let dest = d.join("Documents");
        fs::create_dir_all(dest.join("nested")).unwrap();
        fs::write(dest.join("taxes.docx"), b"irreplaceable").unwrap();

        // `dest` reached by a path that is textually different but resolves to the same directory.
        let via_traversal = dest.join("nested").join("..");
        let e = resolve_conflict(&via_traversal, &dest, ConflictPolicy::Overwrite)
            .expect_err("overwriting the destination directory itself must be refused");
        assert!(e.contains("refusing to overwrite"), "the refusal must name the operation: {e}");
        assert!(e.contains("containing directory itself"), "…and the reason: {e}");
        assert!(dest.join("taxes.docx").exists(), "and nothing may be deleted");

        // A real colliding child is still overwritten — the check must not break ordinary Replace.
        let child = dest.join("taxes.docx");
        let t = resolve_conflict(&child, &dest, ConflictPolicy::Overwrite)
            .expect("a real collision resolves")
            .expect("…to a target path");
        assert_eq!(t, child);
        assert!(!child.exists(), "the consented overwrite really does remove the old file");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn run_transfer_move_removes_the_source() {
        let d = scratch("xfer_move");
        fs::write(d.join("m.txt"), b"data").unwrap();
        fs::create_dir_all(d.join("dst")).unwrap();
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let r = run_transfer(1, &[d.join("m.txt")], &d.join("dst"), TransferKind::Move, ConflictPolicy::Keepboth, false, &cancel, |_| {});
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
        let r = run_transfer(1, &[d.join("a.txt")], &d.join("dst"), TransferKind::Copy, ConflictPolicy::Keepboth, false, &cancel, |_| {});
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

    /// End-to-end through the real streaming pipeline (`run_thumb_batch`) using the EXACT `compute`
    /// closure shape production `thumbnails_stream` uses (`thumbnail_cached`/`make_thumbnail_png`
    /// directly — no lib.rs-side gate anymore, CPE-1447 -> CPE-1449): an oversize raster file in the
    /// same batch as a normal one must be skipped — `data_url: None`, the existing "decode failed"
    /// fallback the frontend already renders as the type icon — while the normal file still comes back
    /// with a thumbnail. The size gate itself now lives inside
    /// `cpe_server::thumb_source::decode_thumb_image` (see that module's tests for the video-bypass
    /// case); this test only proves it's actually reached through the real streaming pipeline. The
    /// oversize fixture is a sparse file (`File::set_len`, no bytes actually written) so the test stays
    /// fast despite exceeding the real 128 MiB cap.
    #[test]
    fn thumbnails_stream_pipeline_skips_oversize_and_thumbnails_normal_in_the_same_batch() {
        use cpe_server::thumb_pipeline::{run_thumb_batch, ThumbCacheStore, ThumbRequest};
        use cpe_server::thumb_queue::Priority;
        use std::sync::Mutex as StdMutex;

        let d = scratch("thumbgate_batch");
        let big = d.join("huge.png");
        {
            let f = fs::File::create(&big).unwrap();
            f.set_len(129 * 1024 * 1024).unwrap(); // > 128 MiB cap; sparse, no real bytes written
        }
        let small = d.join("small.png");
        image::RgbImage::from_pixel(4, 4, image::Rgb([1u8, 2, 3])).save(&small).unwrap();

        let store = StdMutex::new(ThumbCacheStore::new(16, 1_000_000));
        let requests = vec![
            ThumbRequest { path: big.to_string_lossy().into(), target_px: 16, priority: Priority::Visible },
            ThumbRequest { path: small.to_string_lossy().into(), target_px: 16, priority: Priority::Visible },
        ];
        let mut results = Vec::new();
        run_thumb_batch(
            &requests,
            &store,
            cpe_server::thumbnail::make_thumbnail_png,
            || false,
            |r| results.push(r),
        );

        let big_result = results.iter().find(|r| r.path == big.to_string_lossy()).unwrap();
        assert!(big_result.data_url.is_none(), "oversize file must be skipped -> icon fallback, not read");
        let small_result = results.iter().find(|r| r.path == small.to_string_lossy()).unwrap();
        assert!(small_result.data_url.is_some(), "normal-size file in the same batch still thumbnails");
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

    /// The deterministic half of the CPE-1692 guard (runs on every OS/account, no privilege needed) —
    /// same role as `cpe_server::dispatch::classify_path_error`'s own unit tests. Proves the taxonomy
    /// the wiring below depends on.
    #[test]
    fn dest_parent_stat_error_says_gone_only_for_a_genuine_absence() {
        assert!(dest_parent_stat_error(Ok(false)).unwrap().contains("no longer exists"));
        assert!(dest_parent_stat_error(Ok(true)).is_none(), "an existing folder proceeds, no error");
        for kind in [std::io::ErrorKind::PermissionDenied, std::io::ErrorKind::Other, std::io::ErrorKind::TimedOut] {
            let e = std::io::Error::new(kind, "Access is denied.");
            let msg = dest_parent_stat_error(Err(e)).unwrap();
            assert!(!msg.contains("no longer exists"), "{kind:?} must not be reported as absence: {msg}");
            assert!(msg.contains("Access is denied."), "{kind:?} must name the OS's own cause: {msg}");
        }
    }

    /// The end-to-end half, driving the real `move_exact_impl` entry point rather than the pure
    /// classifier above. The check under test calls `dst.parent().try_exists()` — its target IS the
    /// directory itself (not a child inside it) — so this needs the `deny_stat_of` mechanism the PR #874
    /// review established: `try_exists` is a different syscall on Windows than `fs::metadata`, and a
    /// deny ACE placed directly ON the target (not on a parent/ancestor) DOES refuse it there, even
    /// though the earlier `deny_dir_traversal`-based version of this test (denying `gp`, the target's
    /// *parent*) measurably did not. `src-tauri` doesn't depend on `cpe_server::fsutil`'s test-only
    /// helpers (`pub(crate)`, crate-private), so the mechanism is inlined here rather than shared —
    /// mirrors `fsutil::deny_stat_of`'s doc comment for the platform-asymmetric reasoning. Runs for REAL
    /// on both platforms now.
    #[test]
    fn move_exact_reports_the_real_cause_when_the_destination_folder_cannot_be_confirmed() {
        let d = scratch("move_exact_denied");
        let ctx = HeadlessCtx::new(&d);
        let gp = d.join("gp");
        let real_parent = gp.join("real_parent");
        fs::create_dir_all(&real_parent).unwrap();
        fs::write(d.join("source.txt"), b"x").unwrap();

        // Armed before the deny so cleanup runs on every exit path — mirrors split_join.rs's `Restore`
        // pattern (Evidence Rules: a red run must never leave debris).
        struct Restore<'a>(&'a Path, &'a Path, &'a Path);
        impl Drop for Restore<'_> {
            fn drop(&mut self) {
                #[cfg(windows)]
                {
                    let _ = self.1; // Windows denies `real_parent` (self.0) itself; `gp` is untouched there.
                    if let Ok(user) = std::env::var("USERNAME") {
                        let _ = std::process::Command::new("icacls")
                            .arg(self.0)
                            .arg("/remove:d")
                            .arg(&user)
                            .output();
                    }
                }
                #[cfg(unix)]
                {
                    // Unix denies `gp` (self.1), `real_parent`'s parent; `real_parent` itself is untouched.
                    use std::os::unix::fs::PermissionsExt;
                    // Mirror of the `let _ = self.1` above: `real_parent` is read only on the Windows
                    // path, and `-D warnings` makes an unread field a hard error, so each platform has
                    // to acknowledge the field the other one uses.
                    let _ = self.0;
                    let _ = fs::set_permissions(self.1, fs::Permissions::from_mode(0o700));
                }
                let _ = fs::remove_dir_all(self.2);
            }
        }
        let _restore = Restore(&real_parent, &gp, &d);

        #[cfg(windows)]
        {
            // Windows: deny Full Control directly on `real_parent` itself — measured (PR #874 review) to
            // be refused by `try_exists()` even though `fs::metadata` on the identical target still
            // succeeds. Denying `gp` (the parent) here, as the old `deny_dir_traversal`-based version of
            // this test did, measurably has NO effect on `try_exists(real_parent)`.
            if let Ok(user) = std::env::var("USERNAME") {
                if !user.is_empty() {
                    let _ = std::process::Command::new("icacls")
                        .arg(&real_parent)
                        .arg("/deny")
                        .arg(format!("{user}:(F)"))
                        .output();
                }
            }
        }
        #[cfg(unix)]
        {
            // Unix: `try_exists` needs no different treatment than `fs::metadata` — deny stays on the
            // parent, exactly as `deny_dir_traversal` does.
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&gp, fs::Permissions::from_mode(0o000));
        }

        // CPE-1717: `supported_here = true` — the deny is on the target on Windows and on the parent on
        // Unix, and both are measured to make `try_exists()` fail, so this stages on every CI leg. A
        // failure means the runner changed under us: red, not a notice inside a passing run.
        let denied = cpe_server::fsutil::require_staged(
            "move_exact permission-denied staging",
            true,
            real_parent.try_exists().is_err(),
        );
        if !denied {
            use std::io::Write;
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1692] SKIPPED move_exact permission-denied leg: could not deny stat of {} on this \
                 machine (elevated/root, or a filesystem ignoring ACLs/mode bits). The remaining \
                 assertions do NOT cover CPE-1692 for move_exact_impl.",
                real_parent.display()
            );
            return;
        }

        let results = move_exact_impl(&ctx, vec![(
            d.join("source.txt").to_string_lossy().to_string(),
            real_parent.join("target.txt").to_string_lossy().to_string(),
        )]);
        assert!(!results[0].ok, "the destination folder could not be confirmed, so this must not succeed");
        assert!(
            !results[0].error.contains("no longer exists"),
            "a permission-denied folder must not be reported as gone — it's right there: {}",
            results[0].error
        );
        // On Windows specifically, `fs::rename`'s OWN real failure (once the stat guard is bypassed) can
        // ALSO produce an error that happens not to contain "no longer exists" — a target-level deny ACE
        // never makes `Path::exists()` fail on Windows (F1, PR #874 review), so a wiring regression back
        // to `!parent.exists()` doesn't trip the buggy branch at all here; it silently falls through to
        // `fs::rename`, which fails for its own unrelated reason and would pass the assertion above
        // vacuously (measured: reverting the wiring here produced `ok=false,
        // error="Access is denied. (os error 5)"`, which the negative assertion above does not catch).
        // This positively pins the actual code path taken: only `dest_parent_stat_error`'s `Err(e)` arm
        // produces this wrapper text, so a wiring regression that skips the classifier entirely (falling
        // through to rename's raw OS error) is caught here even where the assertion above is not.
        assert!(
            results[0].error.contains("Could not confirm the destination folder still exists"),
            "the classifier's own wrapper text must be present — its absence means the code fell through \
             to fs::rename's raw error instead of being caught by the stat-existence guard: {}",
            results[0].error
        );
        // `_restore` cleans up on the way out, panic or not.
    }

    /// F7 (PR #874 review): the honest case, pinned at the real `move_exact_impl` entry point, not just
    /// the pure classifier (`dest_parent_stat_error_says_gone_only_for_a_genuine_absence` above) — the
    /// UAT independently hit this gap and had to add its own temporary probe to confirm it; making it
    /// permanent here per its recommendation.
    #[test]
    fn move_exact_when_the_destination_folder_is_genuinely_gone_still_says_so_at_the_real_entry_point() {
        let d = scratch("move_exact_honest");
        let ctx = HeadlessCtx::new(&d);
        fs::write(d.join("source.txt"), b"x").unwrap();

        let results = move_exact_impl(&ctx, vec![(
            d.join("source.txt").to_string_lossy().to_string(),
            d.join("truly-gone").join("target.txt").to_string_lossy().to_string(),
        )]);
        assert!(!results[0].ok);
        assert!(
            results[0].error.contains("no longer exists"),
            "a real absence must still say so: {}",
            results[0].error
        );
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
