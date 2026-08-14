//! The Agent Board's Kanban model over `Ticketing/Tickets/` (CPE-852, epic CPE-850), plus read-only
//! views over the sibling `Ticketing/Epics/` and `Ticketing/Sprints/` queues (CPE-1129).
//!
//! Reimplemented **inside the sidecar** — it must not depend on `cpe-server` or the app (ADR 0001) — so
//! the board reads and moves the same real markdown files the CLI `/ticketing-*` flow uses, staying one
//! source of truth. Pure frontmatter/column helpers + the small filesystem read/move; the served UI
//! (`ui.rs`) calls these.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// The Kanban columns — exactly the workflow status folders under `Ticketing/Tickets/`.
pub const COLUMNS: [&str; 5] = ["Backlog", "Doing", "Blocked", "Deferred", "Done"];

/// The status-flow queue root under a project root: `<root>/Ticketing/Tickets/` (CPE-1128 — the
/// `Ticketing/` container holds the status folders alongside the sibling `Epics/`/`Sprints/` queues).
fn tickets_dir(root: &Path) -> PathBuf {
    root.join("Ticketing").join("Tickets")
}

/// The sibling Epics queue: `<root>/Ticketing/Epics/` (CPE-1129).
fn epics_dir(root: &Path) -> PathBuf {
    root.join("Ticketing").join("Epics")
}

/// The sibling Sprints queue: `<root>/Ticketing/Sprints/` (CPE-1129).
fn sprints_dir(root: &Path) -> PathBuf {
    root.join("Ticketing").join("Sprints")
}

/// The folder for a column (the folder IS the status); case-insensitive match to the canonical name.
pub fn folder_for_column(column: &str) -> Option<&'static str> {
    COLUMNS.iter().copied().find(|c| c.eq_ignore_ascii_case(column))
}

/// The `status:` frontmatter value that mirrors a column (the wiki's Status Lifecycle).
pub fn status_for_column(column: &str) -> Option<&'static str> {
    match folder_for_column(column)? {
        "Backlog" => Some("Open"),
        "Doing" => Some("In Progress"),
        "Blocked" => Some("Blocked"),
        "Deferred" => Some("Deferred"),
        "Done" => Some("Done"),
        _ => None,
    }
}

/// A board card — a ticket flattened for the Kanban UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Card {
    pub id: String,
    pub title: String,
    #[serde(rename = "type")]
    pub ticket_type: String,
    pub priority: String,
    pub tags: Vec<String>,
    pub column: String,
}

fn frontmatter(md: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let body = md.trim_start();
    let Some(rest) = body.strip_prefix("---") else { return map };
    let Some(end) = rest.find("\n---") else { return map };
    for line in rest[..end].lines() {
        if let Some((k, v)) = line.split_once(':') {
            let k = k.trim();
            if !k.is_empty() {
                map.insert(k.to_string(), v.trim().to_string());
            }
        }
    }
    map
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    let b = s.as_bytes();
    if b.len() >= 2 && (b[0] == b'"' || b[0] == b'\'') && b[b.len() - 1] == b[0] {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

/// Parse a `[a, b]` frontmatter list; non-list/empty ⇒ `[]`.
pub fn parse_tags(raw: &str) -> Vec<String> {
    let raw = raw.trim();
    let Some(inner) = raw.strip_prefix('[').and_then(|r| r.strip_suffix(']')) else {
        return Vec::new();
    };
    inner
        .split(',')
        .map(|t| unquote(t.trim()))
        .filter(|t| !t.is_empty())
        .collect()
}

/// Build a [`Card`] from a ticket's markdown + the column it was found in. `None` if it has no `id`.
pub fn card_from(md: &str, column: &str) -> Option<Card> {
    let fm = frontmatter(md);
    let id = fm.get("id").map(|s| unquote(s)).filter(|s| !s.is_empty())?;
    Some(Card {
        id,
        title: fm.get("title").map(|s| unquote(s)).unwrap_or_default(),
        ticket_type: fm.get("type").map(|s| unquote(s)).unwrap_or_default(),
        priority: fm.get("priority").map(|s| unquote(s)).unwrap_or_default(),
        tags: fm.get("tags").map(|s| parse_tags(s)).unwrap_or_default(),
        column: column.to_string(),
    })
}

/// Rewrite the first `status:` line of the frontmatter to `new_status` (inserting one before the closing
/// `---` if absent). Pure; the caller writes the result.
pub fn set_status(md: &str, new_status: &str) -> String {
    let mut out = String::with_capacity(md.len() + 16);
    let mut in_fm = false;
    let mut seen_open = false;
    let mut replaced = false;
    for line in md.lines() {
        if line.trim() == "---" {
            if !seen_open {
                seen_open = true;
                in_fm = true;
            } else if in_fm {
                if !replaced {
                    out.push_str(&format!("status: {new_status}\n"));
                    replaced = true;
                }
                in_fm = false;
            }
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if in_fm && !replaced && line.trim_start().starts_with("status:") {
            out.push_str(&format!("status: {new_status}"));
            out.push('\n');
            replaced = true;
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Walk up from `start` to the nearest ancestor containing a `Ticketing/` folder (so the board auto-finds
/// the project it's pointed at). `None` when none does. Keys on the `Ticketing/` container since CPE-1128.
pub fn nearest_project_root(start: &Path) -> Option<PathBuf> {
    let mut dir = Some(start);
    while let Some(d) = dir {
        if d.join("Ticketing").is_dir() {
            return Some(d.to_path_buf());
        }
        dir = d.parent();
    }
    None
}

/// Read every ticket under `root/Ticketing/Tickets/<column>/*.md` into cards. Unreadable dirs/files and
/// id-less files are skipped (never fails the listing). Sorted by column order then id.
pub fn read_board(root: &Path) -> Vec<Card> {
    let mut cards = Vec::new();
    for column in COLUMNS {
        let dir = tickets_dir(root).join(column);
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            if let Ok(md) = fs::read_to_string(&path) {
                if let Some(card) = card_from(&md, column) {
                    cards.push(card);
                }
            }
        }
    }
    cards.sort_by(|a, b| {
        let ca = COLUMNS.iter().position(|c| c == &a.column);
        let cb = COLUMNS.iter().position(|c| c == &b.column);
        ca.cmp(&cb).then_with(|| a.id.cmp(&b.id))
    });
    cards
}

/// Find the file backing ticket `id` under `root/Ticketing/Tickets/<column>/`, returning `(path, column)`.
/// Searches **recursively** so an archived Done ticket (in a dated `Done/YYYY/…` subfolder) is still found
/// and can be moved/reopened (CPE-864).
fn find_card_file(root: &Path, id: &str) -> Option<(PathBuf, &'static str)> {
    for column in COLUMNS {
        let dir = tickets_dir(root).join(column);
        if let Some(hit) = find_in_dir(&dir, id, column) {
            return Some(hit);
        }
    }
    None
}

fn find_in_dir(dir: &Path, id: &str, column: &'static str) -> Option<(PathBuf, &'static str)> {
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(hit) = find_in_dir(&path, id, column) {
                return Some(hit);
            }
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            if let Ok(md) = fs::read_to_string(&path) {
                if card_from(&md, column).map(|c| c.id == id).unwrap_or(false) {
                    return Some((path, column));
                }
            }
        }
    }
    None
}

/// Collect archived Done tickets — those in **subdirectories** of `Ticketing/Tickets/Done/` (the dated `YYYY/QN/…`
/// folders `/ticketing-organize` produces). Top-level files are "recent" and come from [`read_board`];
/// anything nested is archived (CPE-864, mirroring the in-process board's CPE-531).
fn collect_archived(dir: &Path, top_level: bool, column: &str, out: &mut Vec<Card>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_archived(&path, false, column, out);
        } else if !top_level && path.extension().and_then(|e| e.to_str()) == Some("md") {
            if let Ok(md) = fs::read_to_string(&path) {
                if let Some(card) = card_from(&md, column) {
                    out.push(card);
                }
            }
        }
    }
}

/// The archived Done tickets (in dated `Done/**` subfolders) — the board's "show archived" affordance
/// (CPE-864). Kept separate from [`read_board`] so the default board stays fast as Done grows. Id-sorted.
pub fn read_archived(root: &Path) -> Vec<Card> {
    let mut out = Vec::new();
    collect_archived(&tickets_dir(root).join("Done"), true, "Done", &mut out);
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// An epic for the board's Epics view (CPE-1129, mirroring the in-process board's `ticket_board::Epic`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Epic {
    pub id: String,
    pub title: String,
    pub status: String,
    pub tags: Vec<String>,
}

/// The epic `status:` a folder under `Ticketing/Epics/` means (CPE-1676) — the Epics queue has the
/// same five status folders as `Tickets/`, and there too **the folder is the status**. The only
/// difference from [`status_for_column`] is that a dormant epic brief is `Proposed`, not `Open`.
/// Deliberately duplicated from `cpe_server::ticket_board` — the sidecar must not depend on the app
/// or `cpe-server` (ADR 0001), so both boards carry their own copy and change in lockstep.
fn epic_status_for_folder(folder: &str) -> Option<&'static str> {
    match folder_for_column(folder)? {
        "Backlog" => Some("Proposed"),
        "Doing" => Some("In Progress"),
        "Blocked" => Some("Blocked"),
        "Deferred" => Some("Deferred"),
        "Done" => Some("Done"),
        _ => None,
    }
}

/// Parse an epic from a ticket's markdown. `None` if it has no id **or** isn't `epic`-tagged.
/// `folder_status`, when given, overrides the frontmatter — the caller passes it for a file read out
/// of an `Epics/<Folder>/` status folder, where the folder is authoritative (CPE-1676).
fn epic_from(md: &str, folder_status: Option<&str>) -> Option<Epic> {
    let fm = frontmatter(md);
    let id = fm.get("id").map(|s| unquote(s)).filter(|s| !s.is_empty())?;
    let tags: Vec<String> = fm.get("tags").map(|s| parse_tags(s)).unwrap_or_default();
    if !tags.iter().any(|t| t == "epic") {
        return None;
    }
    Some(Epic {
        id,
        title: fm.get("title").map(|s| unquote(s)).unwrap_or_default(),
        status: match folder_status {
            Some(s) => s.to_string(),
            None => fm.get("status").map(|s| unquote(s)).unwrap_or_default(),
        },
        tags,
    })
}

/// Read every `epic`-tagged `.md` directly inside `dir` (non-recursive), taking each epic's status
/// from `folder_status` when the caller supplies one. Unreadable dirs/files, the folders' `wiki.md`
/// explainers and non-epic tickets are skipped.
fn collect_epics_in(dir: &Path, folder_status: Option<&str>, out: &mut Vec<Epic>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        if let Ok(md) = fs::read_to_string(&path) {
            if let Some(epic) = epic_from(&md, folder_status) {
                out.push(epic);
            }
        }
    }
}

/// Read the repo's epics: the epics in the five status folders of `Ticketing/Epics/` + closed epics
/// from the top level of `Ticketing/Tickets/Done/` (each `epic`-tagged) — mirrors the in-process
/// board's `board_epics_impl` (CPE-1129, CPE-1676). Inside `Epics/` the folder supplies the status,
/// so a stale `status:` line can't make the board disagree with the queue's layout; epics closed
/// before CPE-1676 still sit in `Tickets/Done/` and keep using their frontmatter. Unreadable
/// dirs/files and non-epic tickets are skipped. Id-sorted.
pub fn read_epics(root: &Path) -> Vec<Epic> {
    let mut epics = Vec::new();
    for column in COLUMNS {
        collect_epics_in(&epics_dir(root).join(column), epic_status_for_folder(column), &mut epics);
    }
    collect_epics_in(&tickets_dir(root).join("Done"), None, &mut epics);
    epics.sort_by(|a, b| a.id.cmp(&b.id));
    epics
}

/// A sprint for the board's Sprints view (CPE-1129) — a named, time-boxed batch of tickets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Sprint {
    pub id: String,
    pub title: String,
    pub status: String,
    pub start: Option<String>,
    pub end: Option<String>,
}

/// Parse a sprint from a ticket's markdown. `None` if it has no id **or** the id isn't a `SPR-`
/// sprint id (sprints use a separate id sequence from `CPE-NNN` — see `Ticketing/wiki.md` → "Sprints").
fn sprint_from(md: &str) -> Option<Sprint> {
    let fm = frontmatter(md);
    let id = fm.get("id").map(|s| unquote(s)).filter(|s| !s.is_empty())?;
    if !id.starts_with("SPR-") {
        return None;
    }
    Some(Sprint {
        id,
        title: fm.get("title").map(|s| unquote(s)).unwrap_or_default(),
        status: fm.get("status").map(|s| unquote(s)).unwrap_or_default(),
        start: fm.get("start").map(|s| unquote(s)).filter(|s| !s.is_empty()),
        end: fm.get("end").map(|s| unquote(s)).filter(|s| !s.is_empty()),
    })
}

/// Read the repo's sprints: planned/active sprints from `Ticketing/Sprints/` + closed sprints from the
/// top level of `Ticketing/Tickets/Done/` (matched by their `SPR-` id, since a closed sprint carries no
/// distinguishing tag the way a closed epic does) — CPE-1129. Unreadable dirs/files and non-sprint
/// tickets are skipped. Id-sorted.
pub fn read_sprints(root: &Path) -> Vec<Sprint> {
    let mut sprints = Vec::new();
    for dir in [sprints_dir(root), tickets_dir(root).join("Done")] {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            if let Ok(md) = fs::read_to_string(&path) {
                if let Some(sprint) = sprint_from(&md) {
                    sprints.push(sprint);
                }
            }
        }
    }
    sprints.sort_by(|a, b| a.id.cmp(&b.id));
    sprints
}

/// **The guard for a slot that is about to be [`fs::write`]n** (CPE-1719) — the sidecar twin of
/// `cpe_server::fsutil::rename_slot_refusal`, which it deliberately does **not** call: a sidecar depends
/// only on `sidecar-contract`, never on the app or `cpe-server` (ADR 0001).
///
/// ## Why a rename guard would not have caught this
///
/// CPE-1710 gave every `fs::rename`-destructive site a paired clobber + symlink check, and backed it with
/// a `clippy.toml` `disallowed-methods` entry for `std::fs::rename`. Neither reaches here, because
/// [`move_card`]'s destructive primitive is [`fs::write`] and the two primitives fail in *opposite*
/// directions at the same slot:
///
/// - `fs::rename` does **not** follow the final component, so a link at the destination is **destroyed**
///   and its target is left orphaned. Annoying, recoverable, and loud once guarded.
/// - `fs::write` **does** follow it, so a link at the destination is **written through**: the file at the
///   far end — a file the user never named and the board has no business touching — is truncated and
///   replaced with ticket frontmatter. The link survives, so the board looks healthy, the source card is
///   deleted, and the call returns `Ok`. Measured end to end by the PR #895 UAT.
///
/// ## One stat, not two
///
/// The rename guards need two probes ([`Path::try_exists`] for occupancy, `symlink_metadata` for a
/// dangling link) because `try_exists` follows links and so answers `Ok(false)` — genuinely, correctly —
/// for a dangling one. A *write* slot needs only [`fs::symlink_metadata`], which never follows the final
/// component and therefore answers the whole three-state question **about that final component** on its
/// own: `Ok` means the name is taken (by a link or by a real entry), `Err(NotFound)` means it is provably
/// free, and any other `Err` means we could not tell and must not guess. `try_exists` is not used here at
/// all — it cannot see the case that motivated the ticket.
///
/// **"Final component" is load-bearing, and the CPE-1719 UAT measured the difference.** Replace the
/// *column directory* with a junction into the user's own folder and `move_card` returns `Ok`, having
/// written a ticket inside that folder: `symlink_metadata` said `NotFound` because it only ever looked at
/// the leaf. That is arguably what a user who redirects a column is asking for — but it is the same
/// outcome ("a file at a path the user never named") that the dangling-link arm below refuses, reached one
/// component earlier, and this guard does not address it.
///
/// **There is also a TOCTOU window**, and it cannot be closed here: `std` has no `O_NOFOLLOW` write, so
/// between this stat and the `fs::write` a link can be planted and will be followed. Measured by the UAT
/// — `write_slot_refusal` returned `None`, a symlink was planted, and the victim was overwritten. Stated
/// rather than left for "one stat" to be read as complete.
pub fn write_slot_refusal(target: &Path) -> Option<String> {
    classify_write_slot(&fs::symlink_metadata(target).map(|m| m.file_type().is_symlink()), target)
}

/// The pure decision behind [`write_slot_refusal`]. `stat` is the [`fs::symlink_metadata`] outcome
/// reduced to *"is it a link?"*, so **every** arm — including the stat-failure one, which no ordinary
/// fixture reaches — is unit-testable on every OS and every CI account. Split out for the same reason
/// `cpe_server::fsutil::classify_symlink_slot` is: that helper's unreachable-looking `Err` arm quietly
/// accumulated a garbled user-facing message precisely because doing the stat inline made it untestable.
///
/// The link verdict comes **first**. `cpe_server::fsutil::rename_slot_refusal` orders the occupancy check
/// first to preserve the byte-for-byte wording its call sites already shipped; this site has no such
/// history, and a live link answers "the name is taken" to an occupancy check too — so link-first is what
/// makes a live link report *that it is a link* rather than a generic collision.
pub fn classify_write_slot(stat: &std::io::Result<bool>, target: &Path) -> Option<String> {
    match stat {
        Ok(true) => Some(format!(
            "\"{}\" is a link, and writing a ticket there would write straight THROUGH it — the file at \
             the far end, which is not a ticket, would be overwritten. Nothing was changed; remove the \
             link first if that is what you meant",
            target.display()
        )),
        // The stat succeeded and it is not a link, so a real entry holds the name. Overwriting it would
        // silently destroy whatever ticket file is already there.
        // Says "something", not "a file": a *directory* also lands here, and CPE-1687's lesson is that a
        // refusal naming the wrong kind of thing sends the user looking for something that is not there.
        Ok(false) => Some(format!(
            "something already exists at \"{}\" — nothing was changed rather than overwrite it",
            target.display()
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        // Not provably free ⇒ not free. Deliberately avoids the substring "already exists": this is a
        // refusal to *guess*, not a claim about what is there, and the wrong reading sends the user
        // looking for a file that was never gone (CPE-1687).
        Err(e) => Some(format!(
            "could not check what is at \"{}\", so nothing was written — refusing to guess rather than \
             risk overwriting it: {e}",
            target.display()
        )),
    }
}

/// Move card `id` to `to_column`: rewrite its `status:` to match, and move the file into that column's
/// folder (a no-op move when it's already there — the status is still rewritten). Returns the card's new
/// column name on success.
pub fn move_card(root: &Path, id: &str, to_column: &str) -> Result<String, String> {
    let to = folder_for_column(to_column).ok_or_else(|| format!("unknown column: {to_column}"))?;
    let status = status_for_column(to).ok_or_else(|| format!("no status for column: {to}"))?;
    let (src, _from) = find_card_file(root, id).ok_or_else(|| format!("no such card: {id}"))?;

    let md = fs::read_to_string(&src).map_err(|e| e.to_string())?;
    let updated = set_status(&md, status);

    let file_name = src.file_name().ok_or("bad file name")?;
    let dest_dir = tickets_dir(root).join(to);
    fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;
    let dest = dest_dir.join(file_name);

    // CPE-1719. Guard the destination BEFORE writing, and only when it is a different path from the
    // source. `src == dest` is the legitimate no-op move: the card is already in this column and all that
    // happens is its own `status:` being rewritten in place. That case is exempt on purpose — including
    // when the ticket file is itself a link, because then the far end IS the ticket, the very bytes
    // `read_to_string` above just returned, and refusing would make a symlinked ticket unmovable rather
    // than safer. Every other case is a name the board did not put there.
    if src != dest {
        if let Some(e) = write_slot_refusal(&dest) {
            return Err(e);
        }
    }
    fs::write(&dest, updated).map_err(|e| e.to_string())?;
    // Remove the old file whenever we wrote to a DIFFERENT path — only after the new one is written, so
    // the ticket is never lost. This covers a cross-column move AND an archived Done ticket (nested
    // `Done/YYYY/…`) moved to top-level Done: there `from == to == "Done"` but the paths differ, so
    // gating on `from != to` would leave the nested original in place and duplicate the ticket.
    //
    // `remove_file` on a link removes the LINK, not its target — so where **the source card itself** is a
    // link the user made, their file survives untouched with stale content while the ticket lives on at
    // `dest`. Data left orphaned, not destroyed. Recorded rather than refused: a move must remove its
    // source. This is the only remaining destructive primitive in this crate (CPE-1719's enumeration:
    // `fs::write` above, `fs::remove_file` here, nothing else outside `#[cfg(test)]`).
    //
    // **That reassurance is scoped to the leaf, and the CPE-1719 UAT measured where it stops.** Reach a
    // card through a junctioned *directory* — `Done/2025` pointing into the user's archive — and this
    // deletes their **real file**, not a link. It is still a move rather than data loss (the content has
    // already landed at `dest` above, and the write is ordered before the remove precisely so that holds
    // even if this fails), but do not read "orphaned, not destroyed" as covering the directory case.
    //
    // `let _ =` swallows the error deliberately — the ticket has already arrived at `dest`, so failing the
    // whole move here would be worse. The cost is that an offline or read-only far end silently leaves a
    // duplicate. Pre-existing, and worth its own ticket rather than a silent change of contract.
    if src != dest {
        let _ = fs::remove_file(&src);
    }
    Ok(to.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    // For the CPE-1717 skip notice below: `writeln!` into a `Stderr` handle needs the trait in scope.
    // Deliberately the direct-handle form, not `eprintln!` — the macro is what libtest captures.
    #[allow(unused_imports)]
    use std::io::Write as _;

    fn write_ticket(root: &Path, column: &str, id: &str, status: &str) {
        let dir = tickets_dir(root).join(column);
        fs::create_dir_all(&dir).unwrap();
        let md = format!(
            "---\nid: {id}\ntitle: \"{id} title\"\ntype: feature\nstatus: {status}\npriority: low\ntags: [ready]\n---\n\n## Summary\nbody\n"
        );
        fs::write(dir.join(format!("{id}_x.md")), md).unwrap();
    }

    #[test]
    fn columns_map_to_statuses() {
        assert_eq!(folder_for_column("doing"), Some("Doing"));
        assert_eq!(status_for_column("Backlog"), Some("Open"));
        assert_eq!(status_for_column("Doing"), Some("In Progress"));
        assert_eq!(folder_for_column("nope"), None);
    }

    #[test]
    fn card_from_parses_frontmatter() {
        let md = "---\nid: CPE-1\ntitle: \"Hi\"\ntype: bug\nstatus: Open\npriority: high\ntags: [a, b]\n---\nbody";
        let c = card_from(md, "Backlog").unwrap();
        assert_eq!(c.id, "CPE-1");
        assert_eq!(c.title, "Hi");
        assert_eq!(c.ticket_type, "bug");
        assert_eq!(c.tags, vec!["a", "b"]);
        assert_eq!(c.column, "Backlog");
        assert!(card_from("no fm", "Backlog").is_none());
    }

    #[test]
    fn read_board_collects_cards_across_columns() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_ticket(root, "Backlog", "CPE-2", "Open");
        write_ticket(root, "Backlog", "CPE-1", "Open");
        write_ticket(root, "Doing", "CPE-3", "In Progress");
        let cards = read_board(root);
        let ids: Vec<&str> = cards.iter().map(|c| c.id.as_str()).collect();
        // Column order (Backlog before Doing), id-sorted within.
        assert_eq!(ids, vec!["CPE-1", "CPE-2", "CPE-3"]);
        assert_eq!(cards[2].column, "Doing");
    }

    #[test]
    fn move_card_moves_the_file_and_rewrites_status() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_ticket(root, "Backlog", "CPE-9", "Open");

        let new_col = move_card(root, "CPE-9", "Doing").unwrap();
        assert_eq!(new_col, "Doing");
        // File moved out of Backlog into Doing…
        assert!(!root.join("Ticketing/Tickets/Backlog/CPE-9_x.md").exists());
        let moved = root.join("Ticketing/Tickets/Doing/CPE-9_x.md");
        assert!(moved.exists());
        // …with its status rewritten.
        let md = fs::read_to_string(&moved).unwrap();
        assert!(md.contains("status: In Progress"));
        assert!(!md.contains("status: Open"));
        // read_board now reports it in Doing.
        assert_eq!(read_board(root).iter().find(|c| c.id == "CPE-9").unwrap().column, "Doing");
    }

    #[test]
    fn move_card_errors_on_unknown_card_or_column() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_ticket(root, "Backlog", "CPE-1", "Open");
        assert!(move_card(root, "CPE-404", "Doing").is_err());
        assert!(move_card(root, "CPE-1", "Nope").is_err());
    }

    #[test]
    fn nearest_project_root_finds_tickets() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("Ticketing/Tickets/Backlog")).unwrap();
        let deep = root.join("a/b");
        fs::create_dir_all(&deep).unwrap();
        assert_eq!(nearest_project_root(&deep).as_deref(), Some(root));
    }

    // Write a ticket into a nested archive subfolder of Done/ (CPE-864).
    fn write_archived(root: &Path, sub: &str, id: &str) {
        let dir = tickets_dir(root).join("Done").join(sub);
        fs::create_dir_all(&dir).unwrap();
        let md = format!(
            "---\nid: {id}\ntitle: \"{id} title\"\ntype: feature\nstatus: Done\npriority: low\ntags: [ready]\nclosed: 2026-07-21\n---\n\nbody\n"
        );
        fs::write(dir.join(format!("{id}_x.md")), md).unwrap();
    }

    #[test]
    fn archived_tickets_are_aware_but_not_in_the_active_board() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_ticket(root, "Done", "CPE-100", "Done"); // top-level (recent) Done
        write_archived(root, "2026/Q3/July/Week-30", "CPE-200"); // archived

        // The active board shows the recent one, NOT the archived one.
        let active = read_board(root);
        assert!(active.iter().any(|c| c.id == "CPE-100"));
        assert!(!active.iter().any(|c| c.id == "CPE-200"), "archived must not clutter the active board");

        // The archived accessor surfaces the nested one (the app is aware of it).
        let archived = read_archived(root);
        assert_eq!(archived.len(), 1);
        assert_eq!(archived[0].id, "CPE-200");
    }

    #[test]
    fn an_archived_ticket_can_still_be_found_and_moved() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_archived(root, "2026/Q3/July/Week-30", "CPE-300");
        // Reopen the archived ticket: move recursively finds it and relocates it to the target column root.
        let col = move_card(root, "CPE-300", "Doing").unwrap();
        assert_eq!(col, "Doing");
        assert!(root.join("Ticketing/Tickets/Doing/CPE-300_x.md").exists());
        assert!(!root.join("Ticketing/Tickets/Done/2026/Q3/July/Week-30/CPE-300_x.md").exists());
        assert_eq!(read_board(root).iter().find(|c| c.id == "CPE-300").unwrap().column, "Doing");
    }

    #[test]
    fn moving_an_archived_ticket_to_done_does_not_duplicate_it() {
        // Regression: an archived Done ticket (nested Done/YYYY/…) moved to the Done column has
        // from == to == "Done" but src != dest. Gating removal on `from != to` left the nested original
        // behind, so the ticket existed twice (active board + archived). It must exist exactly once.
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_archived(root, "2026/Q3/July/Week-30", "CPE-400");

        let col = move_card(root, "CPE-400", "Done").unwrap();
        assert_eq!(col, "Done");
        // The nested archived original is gone…
        assert!(!root.join("Ticketing/Tickets/Done/2026/Q3/July/Week-30/CPE-400_x.md").exists(), "nested original removed");
        // …and the ticket now backs exactly one file across the active board + the archive.
        let active = read_board(root).into_iter().filter(|c| c.id == "CPE-400").count();
        let archived = read_archived(root).into_iter().filter(|c| c.id == "CPE-400").count();
        assert_eq!(active + archived, 1, "ticket must exist exactly once, not duplicated");
    }

    fn write_epic(root: &Path, dir: &str, id: &str, status: &str) {
        let d = root.join("Ticketing").join(dir);
        fs::create_dir_all(&d).unwrap();
        let md = format!(
            "---\nid: {id}\ntitle: \"{id} title\"\ntype: Epic\nstatus: {status}\npriority: high\ntags: [epic]\n---\n\nbody\n"
        );
        fs::write(d.join(format!("{id}_x.md")), md).unwrap();
    }

    fn write_sprint(root: &Path, dir: &str, id: &str, status: &str, start: &str, end: &str) {
        let d = root.join("Ticketing").join(dir);
        fs::create_dir_all(&d).unwrap();
        let md = format!(
            "---\nid: {id}\ntitle: \"{id} title\"\nstatus: {status}\nstart: {start}\nend: {end}\n---\n\nbody\n"
        );
        fs::write(d.join(format!("{id}_x.md")), md).unwrap();
    }

    #[test]
    fn read_epics_resolves_from_the_sibling_epics_dir_and_closed_done() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_epic(root, "Epics/Doing", "CPE-500", "In Progress");
        write_epic(root, "Tickets/Done", "CPE-501", "Done");
        // A non-epic ticket in Done must NOT be picked up as an epic.
        write_ticket(root, "Done", "CPE-9", "Done");

        let epics = read_epics(root);
        let ids: Vec<&str> = epics.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, vec!["CPE-500", "CPE-501"], "epics from the sibling Epics/ and closed Tickets/Done/");
        assert_eq!(epics[0].status, "In Progress");
        assert_eq!(epics[1].status, "Done");
        assert!(epics.iter().all(|e| e.tags.contains(&"epic".to_string())));
    }

    /// CPE-1676: the Epics queue is five status folders and **the folder is the status** — the
    /// standalone board must read epics at the new depth, prefer the folder over a stale `status:`
    /// line, and ignore each folder's `wiki.md` explainer. Mirrors the in-process board's test.
    #[test]
    fn read_epics_takes_the_status_from_the_epics_status_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_epic(root, "Epics/Backlog", "CPE-1", "Proposed");
        // Stale frontmatter: the folder wins.
        write_epic(root, "Epics/Doing", "CPE-2", "Proposed");
        write_epic(root, "Epics/Blocked", "CPE-3", "Blocked");
        write_epic(root, "Epics/Deferred", "CPE-4", "Deferred");
        write_epic(root, "Epics/Done", "CPE-5", "Done");
        fs::write(root.join("Ticketing/Epics/Backlog/wiki.md"), "# Backlog\n").unwrap();

        let epics = read_epics(root);
        let got: Vec<(&str, &str)> = epics.iter().map(|e| (e.id.as_str(), e.status.as_str())).collect();
        assert_eq!(
            got,
            vec![
                ("CPE-1", "Proposed"),
                ("CPE-2", "In Progress"),
                ("CPE-3", "Blocked"),
                ("CPE-4", "Deferred"),
                ("CPE-5", "Done"),
            ]
        );
    }

    #[test]
    fn read_sprints_resolves_from_the_sibling_sprints_dir_and_closed_done() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_sprint(root, "Sprints", "SPR-02", "Active", "2026-07-20", "2026-08-03");
        write_sprint(root, "Tickets/Done", "SPR-01", "Closed", "2026-07-06", "2026-07-20");
        // A regular ticket in Done must NOT be picked up as a sprint.
        write_ticket(root, "Done", "CPE-9", "Done");

        let sprints = read_sprints(root);
        let ids: Vec<&str> = sprints.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["SPR-01", "SPR-02"], "id-sorted, from the sibling Sprints/ and closed Tickets/Done/");
        assert_eq!(sprints[0].status, "Closed");
        assert_eq!(sprints[0].start.as_deref(), Some("2026-07-06"));
        assert_eq!(sprints[0].end.as_deref(), Some("2026-07-20"));
        assert_eq!(sprints[1].status, "Active");
    }

    #[test]
    fn read_epics_and_sprints_are_empty_when_the_dirs_are_absent() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("Ticketing").join("Tickets").join("Backlog")).unwrap();
        assert!(read_epics(root).is_empty());
        assert!(read_sprints(root).is_empty());
    }

    // ---- CPE-1719: the destination slot of a move is never written through ------------------------
    //
    // Every test below asserts on the **bytes of the file that must not change** and on the **slot still
    // being a link**, never on `move_card`'s `Result`. The bug returned `Ok` throughout: the user's file
    // was destroyed, the link survived, the source card was deleted, and nothing anywhere said so. A test
    // that watches the `Result` would have passed against the broken code.

    /// Stage a symlink at `link` pointing at `target`. `false` when the OS refuses — which on Windows
    /// means no `SeCreateSymbolicLinkPrivilege` (no Developer Mode, not elevated). Unix always succeeds.
    /// Callers must have a non-skipping fallback; see [`alias_at`].
    fn make_link_to(target: &Path, link: &Path) -> bool {
        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_file(target, link).is_ok();
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(target, link).is_ok();
        // The premise, asserted rather than assumed.
        made && fs::symlink_metadata(link).is_ok_and(|m| m.file_type().is_symlink())
    }

    /// Which construction [`alias_at`] managed — reported in the assertion messages so a failure says
    /// what was actually staged instead of leaving the reader to guess at the runner's privileges.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Alias {
        Symlink,
        HardLink,
    }

    /// Make `slot` a second name for `victim`, by whatever construction this OS and account allow.
    ///
    /// **This never skips**, which is the point. A symlink is the reported hazard, but an unprivileged
    /// Windows runner cannot create one, and a test that quietly degrades into asserting nothing on the
    /// exact platform the bug was measured on is worse than no test — it reports green forever. A hard
    /// link needs no privilege on NTFS and stages the *same user-visible hazard*: `slot` is a name for a
    /// file the user never mentioned, and `fs::write` through it truncates that file's contents. The two
    /// constructions are caught by different arms of [`classify_write_slot`] (the link arm and the
    /// occupied arm), so on every runner one real arm is exercised and the victim's bytes are checked.
    fn alias_at(victim: &Path, slot: &Path) -> Alias {
        if make_link_to(victim, slot) {
            return Alias::Symlink;
        }
        fs::hard_link(victim, slot).expect("staging the hazard needs a symlink or a hard link; got neither");
        Alias::HardLink
    }

    /// Stage a **dangling** link at `link` — a link whose target does not exist — and return that missing
    /// target's path so the caller can assert it was not conjured into being.
    ///
    /// Windows takes two attempts, in order: `symlink_file` (needs the privilege above), then an NTFS
    /// **junction**, which needs none. `junction::create` canonicalises its target, so the target must
    /// exist at creation time and is deleted afterwards to leave the reparse point pointing at nothing.
    /// Rust reports a junction's `file_type().is_symlink()` as `true` — the property the guard reads — so
    /// the junction stages the identical slot. This is the pattern of `cpe_server::fsutil`'s helper of the
    /// same name, reimplemented rather than imported: a sidecar may not depend on `cpe-server` (ADR 0001).
    ///
    /// Because the junction leg needs no privilege, **this succeeds on every runner**, so the tests that
    /// use it are unconditional and none of them needs a skip notice at all.
    ///
    /// **Correction (CPE-1717).** An earlier version of this sentence justified that by saying skip
    /// notices "are invisible under CI anyway, since libtest captures stderr for passing tests", citing
    /// CPE-1717. **That is the wrong mechanism and the claim is false.** libtest's capture is installed
    /// inside the `print!`/`eprint!` macros, so `eprintln!` is swallowed but a direct
    /// `writeln!(std::io::stderr(), ..)` is **not** — measured on a real Windows runner, where such a
    /// notice appears in a plain `cargo test` log on a passing test. The reason this function needs no
    /// notice is the junction fallback above, nothing to do with capture. If a leg here ever does need
    /// one, write it with `writeln!(std::io::stderr(), ..)`, and prefer making the leg red outright.
    fn make_dangling_link(link: &Path) -> PathBuf {
        let missing = link.with_file_name(format!(
            "{}-target-that-does-not-exist",
            link.file_name().unwrap_or_default().to_string_lossy()
        ));
        #[cfg(windows)]
        {
            if std::os::windows::fs::symlink_file(&missing, link).is_err() {
                fs::create_dir_all(&missing).expect("staging the junction's temporary target");
                junction::create(&missing, link).expect("creating a junction needs no privilege");
                fs::remove_dir_all(&missing).expect("removing it leaves the junction dangling");
            }
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&missing, link).expect("symlink(2) never resolves its target");
        }
        assert!(
            fs::symlink_metadata(link).is_ok_and(|m| m.file_type().is_symlink()),
            "the slot must hold a link for this fixture to mean anything"
        );
        assert!(!matches!(link.try_exists(), Ok(true)), "and it must dangle");
        missing
    }

    /// The reported bug, reproduced: a live link in the destination column pointing at an unrelated user
    /// file. Before the guard this returned `Ok("Doing")`, the link survived, the source card was deleted,
    /// and the victim's bytes became ticket frontmatter.
    #[test]
    fn a_live_alias_at_the_destination_never_overwrites_the_users_file() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_ticket(root, "Backlog", "CPE-9999", "Open");
        let src = root.join("Ticketing/Tickets/Backlog/CPE-9999_x.md");
        let src_before = fs::read_to_string(&src).unwrap();

        // The user's unrelated file, and the destination slot aliased onto it.
        let victim = root.join("MY-NOTES.txt");
        fs::write(&victim, "MY NOTES").unwrap();
        let doing = root.join("Ticketing/Tickets/Doing");
        fs::create_dir_all(&doing).unwrap();
        let slot = doing.join("CPE-9999_x.md");
        let how = alias_at(&victim, &slot);

        let result = move_card(root, "CPE-9999", "Doing");

        // THE assertion. Not the `Result` — this was `Ok("Doing")` while the file was being destroyed.
        assert_eq!(
            fs::read_to_string(&victim).unwrap(),
            "MY NOTES",
            "the user's unrelated file was overwritten through a {how:?} at the destination slot"
        );
        // The board must not have quietly eaten the card on the way, either.
        assert_eq!(fs::read_to_string(&src).unwrap(), src_before, "the source card must survive a refusal");
        if how == Alias::Symlink {
            assert!(
                fs::symlink_metadata(&slot).unwrap().file_type().is_symlink(),
                "the slot must still be a link — the bug left it a link too, which is why the board looked fine"
            );
        }
        // And the refusal must be the guard's, naming the hazard, not an incidental OS error.
        let err = result.expect_err("a slot the board did not create must not be written");
        assert!(
            err.contains(if how == Alias::Symlink { "is a link" } else { "already exists" }),
            "the refusal must name what it found; got: {err}"
        );
    }

    /// The dangling case, which the ticket asks to be decided explicitly. `fs::write` through a dangling
    /// link **creates** its target — so the board would materialise a file at a path the user never named,
    /// anywhere on disk the link points, while the card appeared to be sitting in the column. Decision:
    /// **refused, same as a live link.** Nothing about "the far end is empty" makes the far end ours.
    ///
    /// Unconditional on every OS and account: `make_dangling_link`'s junction leg needs no privilege.
    #[test]
    fn a_dangling_link_at_the_destination_is_refused_and_its_target_is_not_created() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_ticket(root, "Backlog", "CPE-9998", "Open");
        let src = root.join("Ticketing/Tickets/Backlog/CPE-9998_x.md");
        let src_before = fs::read_to_string(&src).unwrap();

        let doing = root.join("Ticketing/Tickets/Doing");
        fs::create_dir_all(&doing).unwrap();
        let slot = doing.join("CPE-9998_x.md");
        let would_be_target = make_dangling_link(&slot);

        let result = move_card(root, "CPE-9998", "Doing");

        assert!(
            !matches!(would_be_target.try_exists(), Ok(true)),
            "writing through a dangling link CREATES its target — a file at {} the user never named",
            would_be_target.display()
        );
        assert!(
            fs::symlink_metadata(&slot).unwrap().file_type().is_symlink(),
            "the slot must still be a link"
        );
        assert_eq!(fs::read_to_string(&src).unwrap(), src_before, "the source card must survive a refusal");
        // The error must come from the guard, not from the OS failing to follow the link — otherwise this
        // test would pass against unguarded code on any platform where the write happens to error.
        let err = result.expect_err("a dangling link at the destination must be refused");
        assert!(err.contains("is a link"), "the refusal must name the link; got: {err}");
    }

    /// A plain, ordinary file already occupying the destination name — the same clobber class CPE-1705
    /// fixed for renames, present here too because `fs::write` truncates without asking. Distinct from the
    /// link tests: this one goes red only if the *occupied* arm is broken.
    #[test]
    fn an_ordinary_file_at_the_destination_is_not_overwritten() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_ticket(root, "Backlog", "CPE-9997", "Open");
        let doing = root.join("Ticketing/Tickets/Doing");
        fs::create_dir_all(&doing).unwrap();
        let slot = doing.join("CPE-9997_x.md");
        fs::write(&slot, "SOMEONE ELSE'S TICKET").unwrap();

        let _ = move_card(root, "CPE-9997", "Doing");

        assert_eq!(
            fs::read_to_string(&slot).unwrap(),
            "SOMEONE ELSE'S TICKET",
            "a file already at the destination name must not be truncated by a move"
        );
    }

    /// Every arm of the decision, including the stat-failure one no ordinary fixture reaches. Pure, so it
    /// runs identically on every OS and every CI account — the reason the classification is split out of
    /// [`write_slot_refusal`] at all.
    #[test]
    fn write_slot_classification_covers_every_arm() {
        use std::io::{Error, ErrorKind};
        let p = Path::new("/tmp/slot.md");

        // A link — live or dangling, `symlink_metadata` reports both the same way.
        let link = classify_write_slot(&Ok(true), p).expect("a link must be refused");
        assert!(link.contains("is a link"), "got: {link}");
        assert!(link.contains("THROUGH"), "the message must say what fs::write does to it; got: {link}");

        // A real entry.
        let occupied = classify_write_slot(&Ok(false), p).expect("an occupied name must be refused");
        assert!(occupied.contains("already exists"), "got: {occupied}");

        // Provably nothing there: the only answer that means free.
        assert_eq!(classify_write_slot(&Err(Error::from(ErrorKind::NotFound)), p), None);

        // Could not tell ⇒ not free. And it must NOT claim the file is there.
        let unknown = classify_write_slot(&Err(Error::from(ErrorKind::PermissionDenied)), p)
            .expect("an unreadable slot must be refused, not guessed at");
        assert!(unknown.contains("refusing to guess"), "got: {unknown}");
        assert!(
            !unknown.contains("already exists"),
            "the unknown verdict must not read as a claim that something is there; got: {unknown}"
        );
    }

    /// Leg 2 of [`make_dangling_link`], exercised on its own. On a machine with Developer Mode the
    /// `symlink_file` leg always wins and the junction fallback — the leg unprivileged CI actually takes —
    /// would never run here. Build one directly and assert the property the guard depends on.
    #[cfg(windows)]
    #[test]
    fn the_junction_fallback_stages_the_same_hazard_as_a_symlink() {
        let tmp = tempfile::tempdir().unwrap();
        let link = tmp.path().join("junction-slot");
        let target = tmp.path().join("junction-target");
        fs::create_dir_all(&target).unwrap();
        junction::create(&target, &link).expect("creating a junction needs no privilege");
        fs::remove_dir_all(&target).unwrap();

        assert!(
            fs::symlink_metadata(&link).unwrap().file_type().is_symlink(),
            "Rust must report a junction as a link — that is the property the guard reads"
        );
        assert!(matches!(link.try_exists(), Ok(false)), "and it must dangle, so try_exists cannot see it");
        assert!(write_slot_refusal(&link).is_some_and(|e| e.contains("is a link")), "so the guard refuses it");
    }

    /// The exemption, held to its terms: `src == dest` is a no-op move whose only effect is the in-place
    /// `status:` rewrite, and it must keep working. Without this, "guard the destination" would silently
    /// mean "a card can never be re-dropped on its own column".
    #[test]
    fn a_no_op_move_still_rewrites_the_status_in_place() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_ticket(root, "Doing", "CPE-9996", "Open"); // stale status for its folder

        assert_eq!(move_card(root, "CPE-9996", "Doing").unwrap(), "Doing");
        let md = fs::read_to_string(root.join("Ticketing/Tickets/Doing/CPE-9996_x.md")).unwrap();
        assert!(md.contains("status: In Progress"), "the in-place rewrite must still happen");
    }

    /// **The half of the exemption that shipped undocumented-by-test (CPE-1719 round 2, Foreman).**
    ///
    /// `a_no_op_move_still_rewrites_the_status_in_place` above uses an *ordinary file*, so it pins that
    /// the exemption exists — but not the claim the exemption's comment actually makes, which is about a
    /// **symlinked** ticket. The round-2 reviewer measured the gap: reversing the exemption's documented
    /// symlink behaviour left the whole suite green at 27/27. An unread claim sitting on an untested
    /// path is the exact shape this ticket family keeps filing tickets about, so it is pinned here.
    ///
    /// What it holds: a card the user has symlinked in from elsewhere must still be re-droppable on its
    /// own column. Refusing here would be *worse*, not safer — it would make a symlinked ticket
    /// permanently unmovable, and the write goes to the file the content was just read from, so nothing
    /// unrelated is reachable. Note the consequence, deliberately accepted: the far end may be anywhere
    /// on disk, and the `status:` line there is rewritten. That is the user having filed that file on
    /// the board.
    #[cfg(windows)]
    #[test]
    fn a_symlinked_ticket_can_still_be_re_dropped_on_its_own_column() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // The real ticket lives outside Ticketing/ entirely; the board sees only a link to it.
        let far = tmp.path().join("elsewhere");
        fs::create_dir_all(&far).unwrap();
        let real = far.join("CPE-9995_x.md");
        fs::write(&real, "---\nid: CPE-9995\nstatus: Open\n---\n\nbody\n").unwrap();

        let slot = root.join("Ticketing/Tickets/Doing/CPE-9995_x.md");
        fs::create_dir_all(slot.parent().unwrap()).unwrap();
        // CPE-1717, found by this ticket's own UAT. The version of this block written for CPE-1719
        // returned **silently**, justified by "a skip notice would not be visible under CI anyway
        // (CPE-1717)". That justification was false — the mechanism is the macro, not the stream, so a
        // `writeln!(stderr)` notice *is* visible — and it cited this ticket as its authority for the
        // claim this ticket disproved. Worse, a silent `return` is the exact shape this ticket exists
        // to eliminate: a leg that reports green having asserted nothing.
        //
        // There is genuinely no privilege-free way to stage a **live file symlink** on Windows — a
        // junction is directory-only and a hard link is `is_symlink() == false`, both measured on
        // CPE-1716. So this leg cannot simply be made unconditional. It announces instead, loudly, and
        // CI's `windows-latest` does hold the privilege (proved on CPE-1716 by the *absence* of this
        // notice in a green job while the test is recorded running), so on CI it never fires.
        if std::os::windows::fs::symlink_file(&real, &slot).is_err() {
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1719] SKIPPED the symlinked-ticket exemption leg: this machine cannot create a \
                 file symlink at {} (Windows without Developer Mode / admin; a junction is \
                 directory-only and a hard link is not a symlink, so neither can stand in). NOTHING \
                 in this test covered the exemption's symlink claim on this run.",
                slot.display()
            );
            return;
        }
        assert!(fs::symlink_metadata(&slot).unwrap().file_type().is_symlink(), "staging must produce a link");

        assert_eq!(
            move_card(root, "CPE-9995", "Doing").unwrap(),
            "Doing",
            "a symlinked ticket must stay re-droppable on its own column — refusing would make it unmovable"
        );
        assert!(
            fs::symlink_metadata(&slot).unwrap().file_type().is_symlink(),
            "and the link itself must survive the rewrite"
        );
        assert!(
            fs::read_to_string(&real).unwrap().contains("status: In Progress"),
            "the rewrite lands on the far end — accepted, and the reason the exemption exists"
        );
    }
}
