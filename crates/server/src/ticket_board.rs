//! Agent Board backend (CPE-520) — read the repo's `Tickets/` folders as Kanban **cards** and move a
//! card between columns. The board is backed by the **real markdown files** (the activation decision
//! for [CPE-503]), so the board and the CLI `/ticketing-*` flow share one source of truth. This module
//! is the pure core: parse a ticket's frontmatter into a [`Card`], and map folders ↔ columns ↔ the
//! `status:` frontmatter. The Tauri commands in `lib.rs` do the file I/O on top of it.

/// The Kanban columns, which are exactly the workflow status folders under `Tickets/`.
pub const COLUMNS: [&str; 5] = ["Backlog", "Doing", "Blocked", "Deferred", "Done"];

/// The folder a column's tickets live in (identical to the column name — the folder IS the status).
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Card {
    pub id: String,
    pub title: String,
    pub ticket_type: String,
    pub priority: String,
    pub tags: Vec<String>,
    pub epic: Option<String>,
    pub sprint: Option<String>,
    /// The column this card is in (its folder).
    pub column: String,
}

/// Pull the `---` frontmatter block's `key: value` lines into a lookup. Tolerant: no frontmatter ⇒
/// empty. Values keep their raw text (quotes/brackets); helpers below clean specific fields.
fn frontmatter(md: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let body = md.trim_start();
    let Some(rest) = body.strip_prefix("---") else { return map };
    // The block ends at the next line that is exactly `---`.
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

/// Split a ticket's markdown into its **ordered** frontmatter fields (`(key, value)`, quotes stripped) and
/// its **body** (everything after the closing `---`), for the Agent Board's card-detail popup (CPE-959).
/// Tolerant: no frontmatter ⇒ empty fields + the whole input (trimmed) as the body.
pub fn detail_from(md: &str) -> (Vec<(String, String)>, String) {
    let trimmed = md.trim_start();
    let Some(rest) = trimmed.strip_prefix("---") else {
        return (Vec::new(), md.trim().to_string());
    };
    let Some(end) = rest.find("\n---") else {
        return (Vec::new(), md.trim().to_string());
    };
    let mut fields = Vec::new();
    for line in rest[..end].lines() {
        if let Some((k, v)) = line.split_once(':') {
            let k = k.trim();
            if !k.is_empty() {
                fields.push((k.to_string(), unquote(v.trim())));
            }
        }
    }
    // `rest[end..]` is "\n---…\n<body>"; the body is everything after that closing delimiter line.
    let tail = &rest[end + 1..]; // drop the leading '\n' → "---…\n<body>"
    let body = tail.split_once('\n').map(|x| x.1).unwrap_or("").trim().to_string();
    (fields, body)
}

/// Strip one layer of surrounding single/double quotes.
fn unquote(s: &str) -> String {
    let s = s.trim();
    let b = s.as_bytes();
    if b.len() >= 2 && (b[0] == b'"' || b[0] == b'\'') && b[b.len() - 1] == b[0] {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

/// Parse a `[a, b, c]` frontmatter list. Non-list / empty ⇒ `[]`.
pub fn parse_tags(raw: &str) -> Vec<String> {
    let raw = raw.trim();
    let inner = raw.strip_prefix('[').and_then(|r| r.strip_suffix(']'));
    let Some(inner) = inner else { return Vec::new() };
    inner
        .split(',')
        .map(|t| unquote(t.trim()))
        .filter(|t| !t.is_empty())
        .collect()
}

/// Build a [`Card`] from a ticket's markdown, given the column (folder) it was found in. Returns
/// `None` if it has no `id` — a malformed/partial file is skipped rather than failing the listing.
pub fn card_from(md: &str, column: &str) -> Option<Card> {
    let fm = frontmatter(md);
    let id = fm.get("id").map(|s| unquote(s)).filter(|s| !s.is_empty())?;
    Some(Card {
        id,
        title: fm.get("title").map(|s| unquote(s)).unwrap_or_default(),
        ticket_type: fm.get("type").map(|s| unquote(s)).unwrap_or_default(),
        priority: fm.get("priority").map(|s| unquote(s)).unwrap_or_default(),
        tags: fm.get("tags").map(|s| parse_tags(s)).unwrap_or_default(),
        epic: fm.get("epic").map(|s| unquote(s)).filter(|s| !s.is_empty()),
        sprint: fm.get("sprint").map(|s| unquote(s)).filter(|s| !s.is_empty()),
        column: column.to_string(),
    })
}

/// An epic for the board's epic-organized left pane (CPE-530).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Epic {
    pub id: String,
    pub title: String,
    pub status: String,
    pub tags: Vec<String>,
}

/// Parse an epic from a ticket's markdown. Returns `None` if it has no id **or** isn't an epic (its
/// `tags` must include `epic`). Used to list epics from `Tickets/Epics/` + closed epics in `Done/`.
pub fn epic_from(md: &str) -> Option<Epic> {
    let fm = frontmatter(md);
    let id = fm.get("id").map(|s| unquote(s)).filter(|s| !s.is_empty())?;
    let tags: Vec<String> = fm.get("tags").map(|s| parse_tags(s)).unwrap_or_default();
    if !tags.iter().any(|t| t == "epic") {
        return None;
    }
    Some(Epic {
        id,
        title: fm.get("title").map(|s| unquote(s)).unwrap_or_default(),
        status: fm.get("status").map(|s| unquote(s)).unwrap_or_default(),
        tags,
    })
}

/// Rewrite the `status:` line of a ticket's frontmatter to `new_status` (adding one if absent). Only
/// the first `status:` in the frontmatter block is touched. Pure — the caller writes the result.
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
                // closing the frontmatter — if we never saw a status line, insert one before `---`.
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
    // Preserve a trailing-newline-free original minimally; markdown tolerates the extra newline.
    out
}

/// Add or remove the `review` disposition tag on a ticket's `tags:` frontmatter line (CPE-523) — the
/// board's virtual Review lane is driven by this tag, so no new `Tickets/` folder is needed. Pure.
pub fn set_review(md: &str, on: bool) -> String {
    let mut out = String::with_capacity(md.len() + 16);
    let mut in_fm = false;
    let mut seen_open = false;
    let mut done = false;
    for line in md.lines() {
        if line.trim() == "---" {
            if !seen_open {
                seen_open = true;
                in_fm = true;
            } else if in_fm {
                if !done && on {
                    out.push_str("tags: [review]\n"); // no tags line existed — add one
                    done = true;
                }
                in_fm = false;
            }
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if in_fm && !done && line.trim_start().starts_with("tags:") {
            let raw = line.split_once(':').map(|(_, v)| v).unwrap_or("");
            let mut tags = parse_tags(raw);
            let has = tags.iter().any(|t| t == "review");
            if on && !has {
                tags.push("review".to_string());
            } else if !on && has {
                tags.retain(|t| t != "review");
            }
            out.push_str(&format!("tags: [{}]\n", tags.join(", ")));
            done = true;
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Append a finding bullet under a `## Findings` section (creating it at the end if absent) — the
/// affordance a dispatched agent (or the UI) uses to record progress on a card (CPE-523). Pure.
/// Append an agent **directive** to a ticket (CPE-961) — a structured, machine-readable instruction the
/// board emits for an agent (local or external) to act on. Newest-first under `## Agent Directives` (the
/// section is created if absent). Each entry names a target + timestamp + an `open` status an agent flips
/// to `done` once it has appended a reply. `when` is an ISO-8601 timestamp supplied by the caller, so this
/// stays clock-free + pure. An empty `text` is a no-op; an empty `to` defaults to `any`.
pub fn append_directive(md: &str, when: &str, to: &str, text: &str) -> String {
    let text = text.trim();
    if text.is_empty() {
        return md.to_string();
    }
    let to = {
        let t = to.trim();
        if t.is_empty() { "any" } else { t }
    };
    let entry = format!("### ▸ open · to `{to}` · {when}\n{text}\n");
    if let Some((sec_start, body)) = directives_section(md) {
        // Insert BEFORE the first existing directive (newest-first), keeping the intro note under the
        // heading. If there are no directives yet, append at the section's end.
        let insert_at = body
            .find("### ▸ ")
            .map(|i| sec_start + i)
            .unwrap_or(sec_start + body.len());
        let mut out = String::with_capacity(md.len() + entry.len() + 1);
        out.push_str(&md[..insert_at]);
        out.push_str(&entry);
        out.push('\n');
        out.push_str(&md[insert_at..]);
        out
    } else {
        let sep = if md.ends_with('\n') { "" } else { "\n" };
        format!(
            "{md}{sep}\n## Agent Directives\n\
             _Machine-readable: an agent acts on an `open` directive, appends a reply, then flips it to `done`._\n\n{entry}"
        )
    }
}

/// The ticket's `id` frontmatter value (e.g. `CPE-520`), or `None` if absent (CPE-962).
pub fn ticket_id(md: &str) -> Option<String> {
    let id = frontmatter(md).get("id").map(|s| unquote(s))?;
    if id.is_empty() { None } else { Some(id) }
}

/// One parsed directive from a ticket's `## Agent Directives` section (CPE-962).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Directive {
    pub status: String,
    pub to: String,
    pub when: String,
    pub text: String,
}

/// The slice of `md` that is the `## Agent Directives` section body (between its heading and the next
/// top-level `## ` heading or EOF), plus the byte offset where that body starts. `None` if there's no
/// such section.
fn directives_section(md: &str) -> Option<(usize, &str)> {
    let head = md.find("## Agent Directives")?;
    let after_head = md[head..].find('\n').map(|n| head + n + 1).unwrap_or(md.len());
    // The section ends at the next line that begins a top-level heading (`## `), scanning line starts.
    let mut end = md.len();
    let mut scan = after_head;
    while scan < md.len() {
        let line_end = md[scan..].find('\n').map(|n| scan + n + 1).unwrap_or(md.len());
        if md[scan..].starts_with("## ") {
            end = scan;
            break;
        }
        scan = line_end;
    }
    Some((after_head, &md[after_head..end]))
}

/// Parse the directives in a ticket, newest-first as authored (CPE-962). Each entry is a
/// `### ▸ <status> · to \`<target>\` · <when>` header + the text lines under it.
pub fn parse_directives(md: &str) -> Vec<Directive> {
    let Some((_, body)) = directives_section(md) else { return Vec::new() };
    let mut out: Vec<Directive> = Vec::new();
    let mut cur: Option<Directive> = None;
    for line in body.lines() {
        if let Some(rest) = line.strip_prefix("### ▸ ") {
            if let Some(d) = cur.take() {
                out.push(d);
            }
            // "<status> · to `<to>` · <when>"
            let parts: Vec<&str> = rest.split(" · ").collect();
            let status = parts.first().map(|s| s.trim().to_string()).unwrap_or_default();
            let to = parts
                .get(1)
                .and_then(|s| s.split('`').nth(1))
                .map(|s| s.to_string())
                .unwrap_or_else(|| "any".to_string());
            let when = parts.get(2).map(|s| s.trim().to_string()).unwrap_or_default();
            cur = Some(Directive { status, to, when, text: String::new() });
        } else if let Some(d) = cur.as_mut() {
            if !d.text.is_empty() {
                d.text.push('\n');
            }
            d.text.push_str(line);
        }
    }
    if let Some(d) = cur.take() {
        out.push(d);
    }
    for d in &mut out {
        d.text = d.text.trim().to_string();
    }
    out
}

/// Append a `reply` under the directive whose header carries `when`, and optionally flip its `open` status
/// to `done` (CPE-962). Returns the rewritten markdown, or `None` if no directive matches `when`.
pub fn reply_to_directive(md: &str, when: &str, reply: &str, mark_done: bool) -> Option<String> {
    let reply = reply.trim();
    let head = md.find("## Agent Directives")?;
    // Find the `### ▸` header line (at/after the section) whose text contains `when`.
    let mut idx = head;
    let mut header_start = None;
    while idx < md.len() {
        let line_end = md[idx..].find('\n').map(|n| idx + n + 1).unwrap_or(md.len());
        let line = &md[idx..line_end];
        if line.starts_with("### ▸ ") && line.contains(when) {
            header_start = Some(idx);
            break;
        }
        idx = line_end;
    }
    let header_start = header_start?;
    let header_end = md[header_start..].find('\n').map(|n| header_start + n + 1).unwrap_or(md.len());
    // The directive's block ends at the next `### ` header or the next top-level `## ` heading or EOF.
    let mut block_end = md.len();
    let mut scan = header_end;
    while scan < md.len() {
        let line_end = md[scan..].find('\n').map(|n| scan + n + 1).unwrap_or(md.len());
        if md[scan..].starts_with("### ") || md[scan..].starts_with("## ") {
            block_end = scan;
            break;
        }
        scan = line_end;
    }
    // Rewrite: header (optionally open→done) + existing block + the reply line, then the rest.
    let header_line = &md[header_start..header_end];
    let new_header = if mark_done { header_line.replacen("▸ open ", "▸ done ", 1) } else { header_line.to_string() };
    let block_body = &md[header_end..block_end];
    let reply_line = if reply.is_empty() { String::new() } else { format!("> reply: {reply}\n") };
    let mut out = String::with_capacity(md.len() + new_header.len() + reply_line.len() + 8);
    out.push_str(&md[..header_start]);
    out.push_str(&new_header);
    out.push_str(block_body.trim_end());
    out.push('\n');
    out.push_str(&reply_line);
    out.push_str(&md[block_end..]);
    Some(out)
}

pub fn append_finding(md: &str, note: &str) -> String {
    let note = note.trim();
    if note.is_empty() {
        return md.to_string();
    }
    let bullet = format!("- {note}");
    if let Some(pos) = md.find("## Findings") {
        // Insert the bullet right after the heading line.
        let after_heading = md[pos..].find('\n').map(|n| pos + n + 1).unwrap_or(md.len());
        let mut out = String::with_capacity(md.len() + bullet.len() + 1);
        out.push_str(&md[..after_heading]);
        out.push_str(&bullet);
        out.push('\n');
        out.push_str(&md[after_heading..]);
        out
    } else {
        let sep = if md.ends_with('\n') { "" } else { "\n" };
        format!("{md}{sep}\n## Findings\n{bullet}\n")
    }
}

/// Walk up from `start` (inclusive) and return the nearest ancestor directory that contains a readable
/// `Tickets/` folder — so the Agent Board auto-finds the project you're inside (CPE-554). `None` when no
/// ancestor has one. Read-only; a missing/denied dir simply isn't a match.
pub fn nearest_project_root(start: &std::path::Path) -> Option<std::path::PathBuf> {
    let mut dir = Some(start);
    while let Some(d) = dir {
        if d.join("Tickets").is_dir() {
            return Some(d.to_path_buf());
        }
        dir = d.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const TICKET: &str = "---\nid: CPE-520\ntitle: \"Board — read + move\"\ntype: Feature\nstatus: Open\npriority: Medium\ntags: [ready, backend]\nepic: CPE-503\nsprint: SPR-03\n---\n\n## Summary\nbody\n";

    #[test]
    fn detail_from_splits_ordered_fields_and_body() {
        let (fields, body) = detail_from(TICKET);
        // Ordered, first-to-last, quotes stripped.
        assert_eq!(fields.first(), Some(&("id".to_string(), "CPE-520".to_string())));
        assert_eq!(fields[1], ("title".to_string(), "Board — read + move".to_string()));
        assert!(fields.iter().any(|(k, v)| k == "epic" && v == "CPE-503"));
        assert!(fields.iter().any(|(k, v)| k == "sprint" && v == "SPR-03"));
        assert_eq!(body, "## Summary\nbody");
    }

    #[test]
    fn append_directive_creates_the_section_then_prepends_newest_first() {
        let base = "---\nid: CPE-1\n---\n\n## Summary\nhi\n";
        let one = append_directive(base, "2026-07-23T10:00:00Z", "any", "Summarize the risks.");
        assert!(one.contains("## Agent Directives"));
        assert!(one.contains("### ▸ open · to `any` · 2026-07-23T10:00:00Z"));
        assert!(one.contains("Summarize the risks."));
        // A second directive lands ABOVE the first (newest-first) within the same section.
        let two = append_directive(&one, "2026-07-23T11:00:00Z", "claude", "Now draft a fix.");
        assert_eq!(two.matches("## Agent Directives").count(), 1, "reuses the section");
        let i_new = two.find("Now draft a fix.").unwrap();
        let i_old = two.find("Summarize the risks.").unwrap();
        assert!(i_new < i_old, "newest directive comes first");
        assert!(two.contains("to `claude`"));
    }

    #[test]
    fn parse_directives_reads_entries_and_reply_resolves_by_when() {
        let base = "---\nid: CPE-1\n---\n\n## Summary\nhi\n";
        let a = append_directive(base, "2026-07-23T09:00:00Z", "any", "First task.");
        let b = append_directive(&a, "2026-07-23T10:00:00Z", "claude", "Second task.");
        let ds = parse_directives(&b);
        assert_eq!(ds.len(), 2);
        // Newest-first (append_directive prepends).
        assert_eq!(ds[0].when, "2026-07-23T10:00:00Z");
        assert_eq!(ds[0].to, "claude");
        assert_eq!(ds[0].status, "open");
        assert_eq!(ds[0].text, "Second task.");
        assert_eq!(ds[1].text, "First task.");

        // Reply to the first (by its timestamp) and mark it done.
        let replied = reply_to_directive(&b, "2026-07-23T09:00:00Z", "Done — see PR.", true).unwrap();
        assert!(replied.contains("> reply: Done — see PR."));
        let after = parse_directives(&replied);
        let first = after.iter().find(|d| d.when == "2026-07-23T09:00:00Z").unwrap();
        assert_eq!(first.status, "done", "resolved directive flips to done");
        // The other directive is untouched + still open.
        assert!(after.iter().any(|d| d.when == "2026-07-23T10:00:00Z" && d.status == "open"));
    }

    #[test]
    fn reply_to_directive_returns_none_when_no_match() {
        let md = append_directive("## Summary\nx\n", "t1", "any", "do it");
        assert!(reply_to_directive(&md, "nope", "hi", false).is_none());
        assert!(parse_directives("## Summary\nno directives here").is_empty());
    }

    #[test]
    fn append_directive_ignores_empty_and_defaults_target() {
        let base = "## Summary\nx\n";
        assert_eq!(append_directive(base, "t", "any", "   "), base, "empty text is a no-op");
        let out = append_directive(base, "t", "  ", "do it");
        assert!(out.contains("to `any`"), "blank target defaults to any");
    }

    #[test]
    fn detail_from_tolerates_no_frontmatter() {
        let (fields, body) = detail_from("just a note\nwith lines");
        assert!(fields.is_empty());
        assert_eq!(body, "just a note\nwith lines");
    }

    #[test]
    fn nearest_project_root_walks_up_to_the_tickets_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("Tickets").join("Backlog")).unwrap();
        let deep = root.join("src").join("lib").join("components");
        std::fs::create_dir_all(&deep).unwrap();

        // Found from deep inside the project…
        assert_eq!(nearest_project_root(&deep).as_deref(), Some(root));
        // …and at the project root itself.
        assert_eq!(nearest_project_root(root).as_deref(), Some(root));

        // A tree with no Tickets/ ancestor yields None.
        let other = tempfile::tempdir().unwrap();
        assert_eq!(nearest_project_root(other.path()), None);
    }

    #[test]
    fn parses_a_card_from_frontmatter() {
        let c = card_from(TICKET, "Backlog").unwrap();
        assert_eq!(c.id, "CPE-520");
        assert_eq!(c.title, "Board — read + move");
        assert_eq!(c.ticket_type, "Feature");
        assert_eq!(c.priority, "Medium");
        assert_eq!(c.tags, vec!["ready", "backend"]);
        assert_eq!(c.epic.as_deref(), Some("CPE-503"));
        assert_eq!(c.sprint.as_deref(), Some("SPR-03"));
        assert_eq!(c.column, "Backlog");
    }

    #[test]
    fn a_file_without_an_id_is_skipped() {
        assert!(card_from("---\ntitle: no id\n---\n", "Backlog").is_none());
        assert!(card_from("no frontmatter at all", "Backlog").is_none());
    }

    #[test]
    fn tags_parse_handles_empty_and_quoted() {
        assert_eq!(parse_tags("[a, b]"), vec!["a", "b"]);
        assert_eq!(parse_tags("[]"), Vec::<String>::new());
        assert_eq!(parse_tags("[\"big-design\", ready]"), vec!["big-design", "ready"]);
        assert_eq!(parse_tags("not a list"), Vec::<String>::new());
    }

    #[test]
    fn columns_map_to_folders_and_statuses() {
        assert_eq!(folder_for_column("doing"), Some("Doing")); // case-insensitive
        assert_eq!(folder_for_column("Nope"), None);
        assert_eq!(status_for_column("Backlog"), Some("Open"));
        assert_eq!(status_for_column("Doing"), Some("In Progress"));
        assert_eq!(status_for_column("Done"), Some("Done"));
    }

    #[test]
    fn set_status_rewrites_the_status_line() {
        let out = set_status(TICKET, "In Progress");
        assert!(out.contains("status: In Progress"));
        assert!(!out.contains("status: Open"));
        // Other frontmatter + body are preserved.
        assert!(out.contains("id: CPE-520"));
        assert!(out.contains("## Summary"));
        // Re-parsing reflects nothing about status (column drives it) but stays intact.
        assert_eq!(card_from(&out, "Doing").unwrap().id, "CPE-520");
    }

    #[test]
    fn set_status_inserts_one_when_absent() {
        let no_status = "---\nid: CPE-1\ntitle: x\n---\nbody\n";
        let out = set_status(no_status, "Done");
        assert!(out.contains("status: Done"));
        assert!(out.contains("id: CPE-1"));
    }

    #[test]
    fn set_review_adds_and_removes_the_review_tag() {
        let on = set_review(TICKET, true);
        assert_eq!(card_from(&on, "Doing").unwrap().tags, vec!["ready", "backend", "review"]);
        let off = set_review(&on, false);
        assert_eq!(card_from(&off, "Doing").unwrap().tags, vec!["ready", "backend"]);
        // Idempotent both ways.
        assert_eq!(card_from(&set_review(&on, true), "Doing").unwrap().tags, vec!["ready", "backend", "review"]);
        assert_eq!(card_from(&set_review(TICKET, false), "Doing").unwrap().tags, vec!["ready", "backend"]);
    }

    #[test]
    fn epic_from_parses_only_epic_tagged_tickets() {
        let epic = "---\nid: CPE-528\ntitle: \"EPIC: Wire live sessions\"\nstatus: Proposed\ntags: [epic, big-design]\n---\nbody";
        let e = epic_from(epic).unwrap();
        assert_eq!(e.id, "CPE-528");
        assert_eq!(e.status, "Proposed");
        assert!(e.title.contains("Wire live"));
        // A normal (non-epic) ticket is not an epic.
        assert!(epic_from("---\nid: CPE-1\ntitle: x\ntags: [ready]\n---\nb").is_none());
        assert!(epic_from("no frontmatter").is_none());
    }

    #[test]
    fn set_review_adds_a_tags_line_when_absent() {
        let no_tags = "---\nid: CPE-1\ntitle: x\n---\nbody\n";
        let out = set_review(no_tags, true);
        assert_eq!(card_from(&out, "Doing").unwrap().tags, vec!["review"]);
    }

    #[test]
    fn append_finding_creates_the_section_then_appends() {
        let a = append_finding(TICKET, "found a null deref");
        assert!(a.contains("## Findings"));
        assert!(a.contains("- found a null deref"));
        // A second finding lands under the same heading, newest first.
        let b = append_finding(&a, "and a race");
        let idx_head = b.find("## Findings").unwrap();
        let idx_race = b.find("- and a race").unwrap();
        let idx_first = b.find("- found a null deref").unwrap();
        assert!(idx_head < idx_race && idx_race < idx_first);
        // A blank note is a no-op.
        assert_eq!(append_finding(TICKET, "   "), TICKET);
    }
}
