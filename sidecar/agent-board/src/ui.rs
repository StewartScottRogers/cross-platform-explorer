//! The Agent Board sidecar's own served UI (CPE-851/852, epic CPE-850).
//!
//! Per ADR 0001 each sidecar serves its **own** UI; the host frames it. A minimal, dependency-free HTTP
//! server (mirroring `sidecar/repos` and the Agent Deck) on an ephemeral loopback port, upgraded here
//! into a tiny router over the [`crate::board`] model:
//!
//! - `GET /`            → the Kanban page (HTML + JS).
//! - `GET /api/cards`   → the cards under `Ticketing/Tickets/` as JSON.
//! - `GET /api/epics`   → the epics under `Ticketing/Epics/` (+ closed ones in `Tickets/Done/`) as JSON
//!   (CPE-1129, parity with the in-process board's Epics view).
//! - `GET /api/sprints` → the sprints under `Ticketing/Sprints/` (+ closed ones in `Tickets/Done/`) as
//!   JSON (CPE-1129).
//! - `POST /api/move`   → `{ id, to }` moves a card; replies with the refreshed cards.
//!
//! Loopback-only, so it isn't reachable off the machine. Reads/writes the real `Ticketing/` files at the
//! `root` the sidecar was pointed at (`main` resolves it; host-brokered context is CPE-853).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::thread::JoinHandle;

use crate::board;

/// A running UI server: the loopback port it listens on, and the serving thread.
pub struct UiServer {
    pub port: u16,
    _handle: JoinHandle<()>,
}

impl UiServer {
    /// The loopback URL the host should frame.
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

/// Serve the board UI over an ephemeral loopback port for the process's lifetime, reading/writing the
/// `Ticketing/` under `root`.
pub fn serve(root: PathBuf) -> Result<UiServer, String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let handle = std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { break };
            handle_conn(stream, &root);
        }
    });
    Ok(UiServer { port, _handle: handle })
}

fn handle_conn(mut stream: TcpStream, root: &Path) {
    let Ok(read_side) = stream.try_clone() else { return };
    let mut reader = BufReader::new(read_side);

    // Request line: METHOD PATH HTTP/1.1
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();

    // Headers — we only need Content-Length.
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            break; // end of headers
        }
        if let Some(v) = trimmed.to_ascii_lowercase().strip_prefix("content-length:") {
            content_length = v.trim().parse().unwrap_or(0);
        }
    }

    // Body (for POST).
    let mut body = vec![0u8; content_length.min(1 << 20)];
    if !body.is_empty() {
        let _ = reader.read_exact(&mut body);
    }

    let (status, ctype, out) = route(&method, &path, &body, root);
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        out.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&out);
    let _ = stream.flush();
}

fn route(method: &str, path: &str, body: &[u8], root: &Path) -> (&'static str, &'static str, Vec<u8>) {
    match (method, path) {
        ("GET", "/") => ("200 OK", "text/html; charset=utf-8", board_html().into_bytes()),
        ("GET", "/api/cards") => (
            "200 OK",
            "application/json",
            serde_json::to_vec(&board::read_board(root)).unwrap_or_default(),
        ),
        // Archived Done tickets (dated Done/** subfolders) for the "show archived" affordance (CPE-864).
        ("GET", "/api/archived") => (
            "200 OK",
            "application/json",
            serde_json::to_vec(&board::read_archived(root)).unwrap_or_default(),
        ),
        // Epics + Sprints from their sibling Ticketing/ queues (CPE-1129, parity with the in-app board).
        ("GET", "/api/epics") => (
            "200 OK",
            "application/json",
            serde_json::to_vec(&board::read_epics(root)).unwrap_or_default(),
        ),
        ("GET", "/api/sprints") => (
            "200 OK",
            "application/json",
            serde_json::to_vec(&board::read_sprints(root)).unwrap_or_default(),
        ),
        ("POST", "/api/move") => {
            #[derive(serde::Deserialize)]
            struct MoveReq {
                id: String,
                to: String,
            }
            match serde_json::from_slice::<MoveReq>(body) {
                Ok(m) => match board::move_card(root, &m.id, &m.to) {
                    Ok(_) => (
                        "200 OK",
                        "application/json",
                        serde_json::to_vec(&board::read_board(root)).unwrap_or_default(),
                    ),
                    Err(e) => ("400 Bad Request", "text/plain; charset=utf-8", e.into_bytes()),
                },
                Err(_) => ("400 Bad Request", "text/plain; charset=utf-8", b"invalid JSON".to_vec()),
            }
        }
        _ => ("404 Not Found", "text/plain; charset=utf-8", b"not found".to_vec()),
    }
}

/// The Kanban page: fetches `/api/cards`, renders a column per status, and drag-and-drop moves a card via
/// `POST /api/move`. Self-contained (no external assets), theme-aware via CSS system colors.
pub fn board_html() -> String {
    r#"<!doctype html>
<html lang="en">
<head><meta charset="utf-8"><title>Agent Board</title>
<style>
  :root { color-scheme: light dark; }
  * { box-sizing: border-box; }
  /* The view switcher (Board/Epics/Sprints) toggles the `hidden` attribute on the three view
     containers, but `.cols`/`.list` below set `display:flex` — an AUTHOR rule that otherwise
     overrides the UA `[hidden]{display:none}` rule, leaving a "hidden" pane still on screen
     (CPE-1168). Restore the attribute's intent so a view swap actually hides the other panes. */
  [hidden] { display: none !important; }
  body { font: 13px system-ui, sans-serif; margin: 0; height: 100vh; display: flex; flex-direction: column;
         background: Canvas; color: CanvasText; }
  header { padding: 8px 12px; font-weight: 600; border-bottom: 1px solid GrayText; display: flex; gap: 10px; align-items: baseline; }
  header .note { font-weight: 400; color: GrayText; font-size: 12px; }
  .views { margin-left: auto; display: flex; gap: 4px; }
  .viewbtn { font: inherit; font-weight: 600; background: transparent; border: 1px solid GrayText; color: GrayText;
             border-radius: 6px; padding: 3px 10px; cursor: pointer; }
  .viewbtn.active { color: CanvasText; border-color: Highlight; background: color-mix(in srgb, Highlight 15%, Canvas); }
  .cols { flex: 1; display: flex; gap: 8px; padding: 10px; overflow: auto; min-height: 0; }
  .list { flex: 1; padding: 10px; overflow: auto; min-height: 0; display: flex; flex-direction: column; gap: 8px; }
  .row { background: Canvas; border: 1px solid GrayText; border-radius: 6px; padding: 8px 10px; }
  .row .id { font-weight: 600; }
  .row .title { color: GrayText; margin-left: 6px; }
  .row .meta { margin-top: 4px; display: flex; gap: 8px; flex-wrap: wrap; align-items: center; }
  .pill { font-size: 11px; border: 1px solid GrayText; border-radius: 999px; padding: 1px 8px; color: GrayText;
          white-space: nowrap; flex: 0 0 auto; }
  .col { flex: 1 1 0; min-width: 150px; display: flex; flex-direction: column; background: color-mix(in srgb, CanvasText 6%, Canvas);
         border-radius: 8px; padding: 6px; }
  .col h2 { font-size: 12px; margin: 2px 4px 6px; display: flex; justify-content: space-between; color: GrayText; }
  .col.over { outline: 2px solid Highlight; }
  .cards { flex: 1; overflow: auto; display: flex; flex-direction: column; gap: 6px; min-height: 20px; }
  .card { background: Canvas; border: 1px solid GrayText; border-radius: 6px; padding: 6px 8px; cursor: grab; }
  .card:active { cursor: grabbing; }
  .card .id { font-weight: 600; }
  .card .title { color: GrayText; }
  .empty { color: GrayText; text-align: center; padding: 20px 0; }
  button { white-space: nowrap; } /* button labels stay on one line (CPE-928) */
  .archtoggle { margin-top: 8px; background: transparent; border: 1px dashed GrayText; color: GrayText;
                border-radius: 6px; padding: 4px 8px; cursor: pointer; font: inherit; text-align: left; }
  .archtoggle:hover { color: CanvasText; }
  .archlist { display: flex; flex-direction: column; gap: 6px; margin-top: 6px; }
  .archlist .card { opacity: 0.8; }
</style></head>
<body>
  <header>Agent Board <span class="note">running out of process · drag a card to move it</span>
    <span class="views">
      <button class="viewbtn active" id="viewboard" data-view="board">Board</button>
      <button class="viewbtn" id="viewepics" data-view="epics">Epics</button>
      <button class="viewbtn" id="viewsprints" data-view="sprints">Sprints</button>
    </span>
  </header>
  <div class="cols" id="cols"></div>
  <div class="list" id="epicsList" hidden></div>
  <div class="list" id="sprintsList" hidden></div>
<script>
const COLUMNS = ["Backlog","Doing","Blocked","Deferred","Done"];
let dragId = null;
let archivedCards = [];   // archived Done tickets (CPE-864), fetched once, shown behind a toggle
let showArchived = false;
let activeCards = [];

// Epics/Sprints view (CPE-1129) — a lightweight parallel to the Board's Kanban, mirroring the
// in-process board's Epics view. Not draggable; a read surface over the sibling Ticketing/ queues.
function esc(s) { return (s || "").replace(/</g, "&lt;"); }
function pillsHtml(tags) { return (tags || []).map(t => `<span class="pill">${esc(t)}</span>`).join(""); }

async function loadEpics() {
  const res = await fetch("/api/epics");
  const epics = res.ok ? await res.json() : [];
  const root = document.getElementById("epicsList");
  root.innerHTML = epics.length
    ? epics.map(e => `<div class="row"><span class="id">${esc(e.id)}</span><span class="title">${esc(e.title)}</span>` +
        `<div class="meta"><span class="pill">${esc(e.status)}</span>${pillsHtml(e.tags)}</div></div>`).join("")
    : `<div class="empty">No epics.</div>`;
}

async function loadSprints() {
  const res = await fetch("/api/sprints");
  const sprints = res.ok ? await res.json() : [];
  const root = document.getElementById("sprintsList");
  root.innerHTML = sprints.length
    ? sprints.map(s => {
        const window = (s.start || s.end) ? `${esc(s.start || "?")} → ${esc(s.end || "?")}` : "";
        return `<div class="row"><span class="id">${esc(s.id)}</span><span class="title">${esc(s.title)}</span>` +
          `<div class="meta"><span class="pill">${esc(s.status)}</span>${window ? `<span class="pill">${window}</span>` : ""}</div></div>`;
      }).join("")
    : `<div class="empty">No sprints.</div>`;
}

function switchView(view) {
  document.getElementById("cols").hidden = view !== "board";
  document.getElementById("epicsList").hidden = view !== "epics";
  document.getElementById("sprintsList").hidden = view !== "sprints";
  ["board", "epics", "sprints"].forEach(v =>
    document.getElementById("view" + v).classList.toggle("active", v === view));
  if (view === "epics") loadEpics();
  if (view === "sprints") loadSprints();
}
document.querySelectorAll(".viewbtn").forEach(btn =>
  btn.addEventListener("click", () => switchView(btn.dataset.view)));

async function load() {
  const [cardsRes, archRes] = await Promise.all([fetch("/api/cards"), fetch("/api/archived")]);
  archivedCards = archRes.ok ? await archRes.json() : [];
  render(cardsRes.ok ? await cardsRes.json() : []);
}
async function move(id, to) {
  const res = await fetch("/api/move", { method: "POST", headers: {"Content-Type":"application/json"},
                                         body: JSON.stringify({ id, to }) });
  if (res.ok) { archivedCards = archivedCards.filter(c => c.id !== id); render(await res.json()); }
}
function cardHtml(c) {
  return `<div class="card" draggable="true" data-id="${c.id}"><div class="id">${c.id}</div>` +
         `<div class="title">${(c.title||"").replace(/</g,"&lt;")}</div></div>`;
}
function render(cards) {
  activeCards = cards;
  const byCol = {}; COLUMNS.forEach(c => byCol[c] = []);
  cards.forEach(c => (byCol[c.column] || (byCol[c.column] = [])).push(c));
  const root = document.getElementById("cols");
  root.innerHTML = "";
  for (const col of COLUMNS) {
    const el = document.createElement("div"); el.className = "col"; el.dataset.col = col;
    el.addEventListener("dragover", e => { e.preventDefault(); el.classList.add("over"); });
    el.addEventListener("dragleave", () => el.classList.remove("over"));
    el.addEventListener("drop", e => { e.preventDefault(); el.classList.remove("over"); if (dragId) move(dragId, col); });
    const list = byCol[col] || [];
    const cardsHtml = list.map(cardHtml).join("") || `<div class="empty">—</div>`;
    // Done keeps its recent (top-level) cards inline; archived tickets collapse behind a toggle (CPE-864).
    let archHtml = "";
    if (col === "Done" && archivedCards.length) {
      const items = showArchived ? archivedCards.map(cardHtml).join("") : "";
      archHtml = `<button class="archtoggle" id="archtoggle">${showArchived ? "▾" : "▸"} Archived (${archivedCards.length})</button>` +
                 `<div class="archlist">${items}</div>`;
    }
    el.innerHTML = `<h2>${col}<span>${list.length}</span></h2><div class="cards">${cardsHtml}${archHtml}</div>`;
    el.querySelectorAll(".card").forEach(card => {
      card.addEventListener("dragstart", () => { dragId = card.dataset.id; });
      card.addEventListener("dragend", () => { dragId = null; });
    });
    root.appendChild(el);
  }
  const t = document.getElementById("archtoggle");
  if (t) t.addEventListener("click", () => { showArchived = !showArchived; render(activeCards); });
}
load();
</script>
</body>
</html>
"#
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn seed(root: &Path, column: &str, id: &str) {
        let dir = root.join("Ticketing").join("Tickets").join(column);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(format!("{id}_x.md")),
            format!("---\nid: {id}\ntitle: \"{id}\"\ntype: feature\nstatus: Open\npriority: low\ntags: [ready]\n---\nbody\n"),
        )
        .unwrap();
    }

    fn get(port: u16, path: &str) -> String {
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        s.write_all(format!("GET {path} HTTP/1.0\r\nHost: localhost\r\n\r\n").as_bytes()).unwrap();
        let mut resp = String::new();
        s.read_to_string(&mut resp).unwrap();
        resp
    }

    fn post(port: u16, path: &str, body: &str) -> String {
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let req = format!(
            "POST {path} HTTP/1.0\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        s.write_all(req.as_bytes()).unwrap();
        let mut resp = String::new();
        s.read_to_string(&mut resp).unwrap();
        resp
    }

    #[test]
    fn serves_the_page_and_cards_and_moves() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        seed(&root, "Backlog", "CPE-1");
        let server = serve(root.clone()).unwrap();
        assert!(server.url().starts_with("http://127.0.0.1:"));

        // The page.
        let page = get(server.port, "/");
        assert!(page.contains("200 OK") && page.contains("Agent Board"));

        // Cards JSON lists the seeded card in Backlog.
        let cards = get(server.port, "/api/cards");
        assert!(cards.contains("application/json"));
        assert!(cards.contains("CPE-1") && cards.contains("Backlog"));

        // Move it to Doing — the reply reflects the new column, and the file actually moved.
        let moved = post(server.port, "/api/move", "{\"id\":\"CPE-1\",\"to\":\"Doing\"}");
        assert!(moved.contains("200 OK"), "resp: {moved}");
        assert!(moved.contains("Doing"));
        assert!(root.join("Ticketing/Tickets/Doing/CPE-1_x.md").exists());
        assert!(!root.join("Ticketing/Tickets/Backlog/CPE-1_x.md").exists());

        // A bad move is a 400.
        let bad = post(server.port, "/api/move", "{\"id\":\"CPE-1\",\"to\":\"Nope\"}");
        assert!(bad.contains("400"));
        // Unknown path 404s.
        assert!(get(server.port, "/nope").contains("404"));
    }

    #[test]
    fn board_html_is_valid() {
        let html = board_html();
        assert!(html.contains("<!doctype html>"));
        assert!(html.contains("Agent Board"));
        assert!(html.contains("/api/move"));
        // The Epics/Sprints view switcher (CPE-1129) is wired into the served page.
        assert!(html.contains("/api/epics"));
        assert!(html.contains("/api/sprints"));
        // CPE-1168: the view switcher toggles `hidden` on the three panes, but `.cols`/`.list` set
        // `display:flex`, which (as an author rule) would override the UA `[hidden]{display:none}`
        // and leave a "hidden" pane on screen. This `!important` rule restores the attribute's
        // intent so a view swap actually hides the others; a headless click-through (clickthrough.mjs)
        // caught the missing swap. Keep the rule so the fix can't silently regress.
        assert!(html.contains("[hidden] { display: none !important; }"));
    }

    fn seed_epic(root: &Path, dir: &str, id: &str, status: &str) {
        let d = root.join("Ticketing").join(dir);
        fs::create_dir_all(&d).unwrap();
        fs::write(
            d.join(format!("{id}_x.md")),
            format!("---\nid: {id}\ntitle: \"{id}\"\ntype: Epic\nstatus: {status}\npriority: high\ntags: [epic]\n---\nbody\n"),
        )
        .unwrap();
    }

    fn seed_sprint(root: &Path, dir: &str, id: &str, status: &str) {
        let d = root.join("Ticketing").join(dir);
        fs::create_dir_all(&d).unwrap();
        fs::write(
            d.join(format!("{id}_x.md")),
            format!("---\nid: {id}\ntitle: \"{id}\"\nstatus: {status}\nstart: 2026-07-20\nend: 2026-08-03\n---\nbody\n"),
        )
        .unwrap();
    }

    #[test]
    fn serves_epics_and_sprints_json_from_the_sibling_ticketing_queues() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        seed_epic(&root, "Epics", "CPE-500", "In Progress");
        seed_sprint(&root, "Sprints", "SPR-02", "Active");
        let server = serve(root.clone()).unwrap();

        let epics = get(server.port, "/api/epics");
        assert!(epics.contains("application/json"));
        assert!(epics.contains("CPE-500") && epics.contains("In Progress"));

        let sprints = get(server.port, "/api/sprints");
        assert!(sprints.contains("application/json"));
        assert!(sprints.contains("SPR-02") && sprints.contains("Active") && sprints.contains("2026-07-20"));
    }
}
