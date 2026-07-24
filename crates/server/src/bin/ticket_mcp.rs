//! `ticket-mcp` — a stdio **MCP server** exposing a repo's Agent Board directives (CPE-962). Point an
//! MCP-speaking agent at `ticket-mcp <repo-root>` and it serves `directives.list` / `directives.reply`
//! over newline-delimited JSON-RPC; the repo's `Tickets/**.md` files (their `## Agent Directives`
//! sections) are the source of truth. Runs anywhere the repo is checked out — the board→agent bridge for
//! deployments outside your control.

use cpe_server::ticket_board;
use cpe_server::ticket_mcp::{handle_message, DirectiveStore, OpenDirective};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

struct FsStore {
    root: PathBuf,
}

impl FsStore {
    fn tickets(&self) -> PathBuf {
        self.root.join("Tickets")
    }
    fn walk_md(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                Self::walk_md(&p, out);
            } else if p.extension().and_then(|s| s.to_str()) == Some("md") {
                out.push(p);
            }
        }
    }
    fn all_md(&self) -> Vec<PathBuf> {
        let mut v = Vec::new();
        Self::walk_md(&self.tickets(), &mut v);
        v
    }
}

impl DirectiveStore for FsStore {
    fn list_open(&self) -> Vec<OpenDirective> {
        let mut out = Vec::new();
        for p in self.all_md() {
            let Ok(md) = std::fs::read_to_string(&p) else { continue };
            let Some(id) = ticket_board::ticket_id(&md) else { continue };
            for d in ticket_board::parse_directives(&md) {
                if d.status == "open" {
                    out.push(OpenDirective { ticket: id.clone(), when: d.when, to: d.to, text: d.text });
                }
            }
        }
        out
    }

    fn reply(&mut self, ticket: &str, when: &str, reply: &str, done: bool) -> Result<(), String> {
        for p in self.all_md() {
            let Ok(md) = std::fs::read_to_string(&p) else { continue };
            if ticket_board::ticket_id(&md).as_deref() != Some(ticket) {
                continue;
            }
            return match ticket_board::reply_to_directive(&md, when, reply, done) {
                Some(updated) => std::fs::write(&p, updated).map_err(|e| e.to_string()),
                None => Err(format!("no directive {when} in {ticket}")),
            };
        }
        Err(format!("ticket {ticket} not found"))
    }
}

fn main() {
    let root = std::env::args().nth(1).unwrap_or_else(|| ".".to_string());
    let mut store = FsStore { root: PathBuf::from(root) };
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(msg) = serde_json::from_str::<serde_json::Value>(trimmed) else { continue };
        if let Some(resp) = handle_message(&mut store, &msg) {
            let _ = writeln!(stdout, "{resp}");
            let _ = stdout.flush();
        }
    }
}
