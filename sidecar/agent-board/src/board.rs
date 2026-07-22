//! The Agent Board's Kanban model over `Tickets/` (CPE-852, epic CPE-850).
//!
//! Reimplemented **inside the sidecar** — it must not depend on `cpe-server` or the app (ADR 0001) — so
//! the board reads and moves the same real markdown files the CLI `/ticketing-*` flow uses, staying one
//! source of truth. Pure frontmatter/column helpers + the small filesystem read/move; the served UI
//! (`ui.rs`) calls these.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// The Kanban columns — exactly the workflow status folders under `Tickets/`.
pub const COLUMNS: [&str; 5] = ["Backlog", "Doing", "Blocked", "Deferred", "Done"];

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

/// Walk up from `start` to the nearest ancestor containing a `Tickets/` folder (so the board auto-finds
/// the project it's pointed at). `None` when none does.
pub fn nearest_project_root(start: &Path) -> Option<PathBuf> {
    let mut dir = Some(start);
    while let Some(d) = dir {
        if d.join("Tickets").is_dir() {
            return Some(d.to_path_buf());
        }
        dir = d.parent();
    }
    None
}

/// Read every ticket under `root/Tickets/<column>/*.md` into cards. Unreadable dirs/files and
/// id-less files are skipped (never fails the listing). Sorted by column order then id.
pub fn read_board(root: &Path) -> Vec<Card> {
    let mut cards = Vec::new();
    for column in COLUMNS {
        let dir = root.join("Tickets").join(column);
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

/// Find the file backing ticket `id` under `root/Tickets/<column>/`, returning `(path, column)`. Searches
/// **recursively** so an archived Done ticket (in a dated `Done/YYYY/…` subfolder) is still found and can
/// be moved/reopened (CPE-864).
fn find_card_file(root: &Path, id: &str) -> Option<(PathBuf, &'static str)> {
    for column in COLUMNS {
        let dir = root.join("Tickets").join(column);
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

/// Collect archived Done tickets — those in **subdirectories** of `Tickets/Done/` (the dated `YYYY/QN/…`
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
    collect_archived(&root.join("Tickets").join("Done"), true, "Done", &mut out);
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// Move card `id` to `to_column`: rewrite its `status:` to match, and move the file into that column's
/// folder (a no-op move when it's already there — the status is still rewritten). Returns the card's new
/// column name on success.
pub fn move_card(root: &Path, id: &str, to_column: &str) -> Result<String, String> {
    let to = folder_for_column(to_column).ok_or_else(|| format!("unknown column: {to_column}"))?;
    let status = status_for_column(to).ok_or_else(|| format!("no status for column: {to}"))?;
    let (src, from) = find_card_file(root, id).ok_or_else(|| format!("no such card: {id}"))?;

    let md = fs::read_to_string(&src).map_err(|e| e.to_string())?;
    let updated = set_status(&md, status);

    let file_name = src.file_name().ok_or("bad file name")?;
    let dest_dir = root.join("Tickets").join(to);
    fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;
    let dest = dest_dir.join(file_name);

    fs::write(&dest, updated).map_err(|e| e.to_string())?;
    if from != to {
        // Remove the old file only after the new one is written (never lose the ticket).
        let _ = fs::remove_file(&src);
    }
    Ok(to.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_ticket(root: &Path, column: &str, id: &str, status: &str) {
        let dir = root.join("Tickets").join(column);
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
        assert!(!root.join("Tickets/Backlog/CPE-9_x.md").exists());
        let moved = root.join("Tickets/Doing/CPE-9_x.md");
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
        fs::create_dir_all(root.join("Tickets/Backlog")).unwrap();
        let deep = root.join("a/b");
        fs::create_dir_all(&deep).unwrap();
        assert_eq!(nearest_project_root(&deep).as_deref(), Some(root));
    }

    // Write a ticket into a nested archive subfolder of Done/ (CPE-864).
    fn write_archived(root: &Path, sub: &str, id: &str) {
        let dir = root.join("Tickets/Done").join(sub);
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
        assert!(root.join("Tickets/Doing/CPE-300_x.md").exists());
        assert!(!root.join("Tickets/Done/2026/Q3/July/Week-30/CPE-300_x.md").exists());
        assert_eq!(read_board(root).iter().find(|c| c.id == "CPE-300").unwrap().column, "Doing");
    }
}
