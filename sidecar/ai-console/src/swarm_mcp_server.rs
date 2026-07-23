//! Live swarm MCP host (CPE-541, epic CPE-528) — the running server the pure router
//! ([`crate::swarm_mcp`]) only described. It speaks **JSON-RPC 2.0 over stdio** (newline-delimited, the
//! MCP stdio transport), so a launched coding agent that lists this process in its MCP config spawns it
//! and calls `mailbox.*` / `memory.*` for real.
//!
//! ## Why file-backed (the shared-state decision)
//! Each agent process spawns its **own** copy of this server, so the coordination state can't live in
//! one server's memory — it must be shared. The substrate is the **filesystem** (`--dir <mission>`),
//! which is exactly how shared agent memory was already designed to persist (`.agentmemory/`, CPE-525):
//! - `memory/` — markdown notes via [`save_note`]/[`load_dir`]; reloaded per call so writes from one
//!   agent are immediately visible to another.
//! - `mailbox.jsonl` — append-only posted messages; a read replays them (+ the roster) into an
//!   in-memory [`Mailbox`] and reuses the pure router.
//! - `members.json` — the team roster (written by the live driver) so `role`/`broadcast` recipients
//!   resolve across processes.
//!
//! No sidecar IPC and no network listener/port/token — the smallest surface (the epic's SSRF-ish
//! concern never arises). The calling agent's id is a launch arg (`--agent`), one server per agent, so
//! an agent can't post as another. `mailbox.drain` (destructive) is intentionally **not** offered by
//! the shared host — cross-process cursoring is out of scope for v1; agents use non-destructive
//! `mailbox.read`.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use crate::agent_memory::{load_dir, memory_tool, save_note, MemoryGraph};
use crate::swarm_mailbox::Mailbox;
use crate::swarm_mcp::{dispatch_tool, parse_recipient, tools_manifest};
use crate::swarm_team::Role;

/// The MCP protocol version this host reports (the pinned MCP stdio revision).
const PROTOCOL_VERSION: &str = "2024-11-05";

#[derive(serde::Deserialize)]
struct MemberRec {
    id: String,
    role: Role,
}

/// A filesystem-backed shared store for one mission directory. Cheap to construct; every operation
/// reads/writes the dir, so separate processes pointed at the same dir share all state.
pub struct FileStore {
    dir: PathBuf,
}

impl FileStore {
    pub fn new(dir: PathBuf) -> FileStore {
        FileStore { dir }
    }

    fn memory_dir(&self) -> PathBuf {
        self.dir.join("memory")
    }
    fn mailbox_file(&self) -> PathBuf {
        self.dir.join("mailbox.jsonl")
    }
    fn members_file(&self) -> PathBuf {
        self.dir.join("members.json")
    }

    fn load_members(&self) -> Vec<(String, Role)> {
        let Ok(s) = std::fs::read_to_string(self.members_file()) else { return vec![] };
        let Ok(recs) = serde_json::from_str::<Vec<MemberRec>>(&s) else { return vec![] };
        recs.into_iter().map(|r| (r.id, r.role)).collect()
    }

    /// The posted-message records (`{from,to,kind,body,ts}`) in order, skipping any unparseable line.
    fn mailbox_records(&self) -> Vec<Value> {
        let Ok(s) = std::fs::read_to_string(self.mailbox_file()) else { return vec![] };
        s.lines().filter_map(|l| serde_json::from_str::<Value>(l.trim()).ok()).collect()
    }

    /// Rebuild the in-memory mailbox from the roster + the replayed post log, so the pure router can
    /// answer a read against real cross-process state.
    fn hydrate_mailbox(&self) -> Mailbox {
        let mut mb = Mailbox::new();
        for (id, role) in self.load_members() {
            mb.register(&id, role);
        }
        for rec in self.mailbox_records() {
            let (Some(from), Some(to)) = (rec.get("from").and_then(|v| v.as_str()), rec.get("to")) else {
                continue;
            };
            let Ok(recip) = parse_recipient(to) else { continue };
            let kind = rec.get("kind").and_then(|v| v.as_str()).unwrap_or("note");
            let body = rec.get("body").and_then(|v| v.as_str()).unwrap_or("");
            let ts = rec.get("ts").and_then(|v| v.as_u64()).unwrap_or(0);
            mb.post(from, recip, kind, body, ts);
        }
        mb
    }

    /// Append one posted message to the log. Returns its 0-based sequence (its position in the log).
    fn append_post(&self, from: &str, to: &Value, kind: &str, body: &str, ts: u64) -> u64 {
        let seq = self.mailbox_records().len() as u64;
        let rec = json!({ "from": from, "to": to, "kind": kind, "body": body, "ts": ts });
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(self.mailbox_file()) {
            let _ = writeln!(f, "{rec}");
        }
        seq
    }

    /// Route one tool call against the shared filesystem state (the file-backed counterpart of
    /// [`dispatch_tool`]). `agent` is the caller (from its launch arg).
    pub fn dispatch(&self, agent: &str, tool: &str, args: &Value, ts: u64) -> Value {
        if let Some(rest) = tool.strip_prefix("memory.") {
            let _ = rest;
            let mem_dir = self.memory_dir();
            let mut g = load_dir(&mem_dir);
            let r = memory_tool(&mut g, tool, args);
            if tool == "memory.write" && r.get("ok").and_then(|v| v.as_bool()) == Some(true) {
                if let Some(id) = r.get("id").and_then(|v| v.as_str()) {
                    if let Some(note) = g.get(id) {
                        let _ = save_note(&mem_dir, note);
                    }
                }
            }
            return r;
        }
        match tool {
            "mailbox.post" => {
                let Some(to) = args.get("to") else { return json!({ "ok": false, "error": "missing 'to'" }) };
                if let Err(e) = parse_recipient(to) {
                    return json!({ "ok": false, "error": e });
                }
                let body = args.get("body").and_then(|v| v.as_str()).unwrap_or("");
                if body.trim().is_empty() {
                    return json!({ "ok": false, "error": "empty body" });
                }
                let kind = args.get("kind").and_then(|v| v.as_str()).unwrap_or("note");
                let seq = self.append_post(agent, to, kind, body, ts);
                json!({ "ok": true, "seq": seq })
            }
            "mailbox.read" => {
                let mut mb = self.hydrate_mailbox();
                let mut mem = MemoryGraph::new(); // unused by mailbox tools
                dispatch_tool(&mut mb, &mut mem, agent, "mailbox.read", args, ts)
            }
            "mailbox.drain" => json!({ "ok": false, "error": "mailbox.drain is not offered by the shared host — use mailbox.read" }),
            other => json!({ "ok": false, "error": format!("unknown tool '{other}'") }),
        }
    }

    /// The tools this host advertises: the pure manifest minus the destructive `mailbox.drain` (see
    /// the module note).
    fn tools_list(&self) -> Value {
        let mut m = tools_manifest();
        if let Some(arr) = m.get_mut("tools").and_then(|t| t.as_array_mut()) {
            arr.retain(|t| t.get("name").and_then(|n| n.as_str()) != Some("mailbox.drain"));
        }
        m
    }

    /// Handle one JSON-RPC message from `agent`. Returns the response to write back, or `None` for a
    /// notification (no `id`). Pure over the message + the filesystem, so it's unit-testable without
    /// wiring stdio.
    pub fn handle_message(&self, agent: &str, msg: &Value) -> Option<Value> {
        let id = msg.get("id").cloned();
        let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
        match method {
            "initialize" => Some(rpc_result(
                id,
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "cpe-swarm", "version": env!("CARGO_PKG_VERSION") }
                }),
            )),
            "notifications/initialized" => None,
            "ping" => Some(rpc_result(id, json!({}))),
            "tools/list" => Some(rpc_result(id, self.tools_list())),
            "tools/call" => {
                let params = msg.get("params").cloned().unwrap_or_else(|| json!({}));
                let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
                let out = self.dispatch(agent, name, &args, now_secs());
                let is_error = out.get("ok").and_then(|v| v.as_bool()) == Some(false);
                Some(rpc_result(
                    id,
                    json!({ "content": [ { "type": "text", "text": out.to_string() } ], "isError": is_error }),
                ))
            }
            _ if id.is_some() => Some(rpc_error(id, -32601, &format!("method not found: {method}"))),
            _ => None, // an unknown notification — nothing to answer
        }
    }
}

/// Write the team roster the host reads to resolve `role`/`broadcast` recipients. Called by the live
/// driver when it staffs a mission (and by tests).
pub fn write_members(dir: &Path, members: &[(String, Role)]) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let recs: Vec<Value> = members.iter().map(|(id, role)| json!({ "id": id, "role": role })).collect();
    std::fs::write(dir.join("members.json"), serde_json::to_string_pretty(&recs).unwrap_or_default())
}

/// Seed the shared mailbox with the coordinator's opening posts (CPE-955) so the live coordination panel
/// shows real activity the instant a mission starts — a `kickoff` broadcast naming the team + a `→` `assign`
/// post per task — instead of "no messages yet…" until an agent chooses to post. Agents' own `mailbox.post`
/// chatter (incl. the CPE-954 teamchat demo) then appends after these. Records match the `{from,to,kind,
/// body,ts}` shape `/api/swarm/activity` reads. Best-effort: a write failure is ignored (the mission still
/// runs); posts are appended, never clobbering an existing log.
pub fn seed_kickoff(dir: &Path, members: &[(String, Role)], tasks: &[(String, String)]) {
    let _ = std::fs::create_dir_all(dir);
    let ts = now_secs();
    let team = members.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>().join(", ");
    let mut recs: Vec<Value> = Vec::with_capacity(tasks.len() + 1);
    let n = tasks.len();
    recs.push(json!({
        "from": "coordinator", "to": "broadcast", "kind": "kickoff",
        "body": format!("Mission started — {} task{}. Team: {}", n, if n == 1 { "" } else { "s" }, team),
        "ts": ts,
    }));
    for (i, (desc, scope)) in tasks.iter().enumerate() {
        let body = if scope.is_empty() {
            format!("→ task {}: {}", i + 1, desc)
        } else {
            format!("→ task {}: {} (scope: {})", i + 1, desc, scope)
        };
        recs.push(json!({ "from": "coordinator", "to": "broadcast", "kind": "assign", "body": body, "ts": ts }));
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("mailbox.jsonl")) {
        for r in recs {
            let _ = writeln!(f, "{r}");
        }
    }
}

/// Seed the shared **memory** graph with a coordinator "mission brief" note (CPE-956) so the Shared Memory
/// panel shows a real note the instant a mission starts — instead of "no notes yet…" until an agent chooses
/// to `memory.write`. Agents' own notes build on it. Reuses the tested `memory.write` persist path, so the
/// note lands as a `<mission>/memory/*.md` file `/api/swarm/activity` parses. Best-effort.
pub fn seed_memory(dir: &Path, members: &[(String, Role)], tasks: &[(String, String)]) {
    let _ = std::fs::create_dir_all(dir);
    let team = members.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>().join(", ");
    let goals = tasks
        .iter()
        .enumerate()
        .map(|(i, (desc, _))| format!("{}. {}", i + 1, desc))
        .collect::<Vec<_>>()
        .join("  ");
    let body = format!("Mission brief — team: {team}. Goals: {goals}");
    let store = FileStore::new(dir.to_path_buf());
    let _ = store.dispatch(
        "coordinator",
        "memory.write",
        &json!({ "body": body, "tags": ["mission", "brief"] }),
        now_secs(),
    );
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn rpc_result(id: Option<Value>, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id.unwrap_or(Value::Null), "result": result })
}

fn rpc_error(id: Option<Value>, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id.unwrap_or(Value::Null), "error": { "code": code, "message": message } })
}

/// Run the host: read newline-delimited JSON-RPC from stdin, act, write responses to stdout. Blocks
/// until stdin closes (the agent exits). This is the `--swarm-mcp` process entry.
pub fn run(dir: PathBuf, agent: String) {
    let store = FileStore::new(dir);
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(msg) = serde_json::from_str::<Value>(line) else { continue };
        if let Some(resp) = store.handle_message(&agent, &msg) {
            let _ = writeln!(stdout, "{resp}");
            let _ = stdout.flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, FileStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = FileStore::new(dir.path().to_path_buf());
        (dir, store)
    }

    fn call(store: &FileStore, agent: &str, name: &str, args: Value) -> Value {
        let msg = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": args } });
        let resp = store.handle_message(agent, &msg).unwrap();
        // Unwrap the MCP content envelope back to the tool's JSON.
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn seed_kickoff_writes_a_kickoff_plus_one_assign_per_task() {
        let dir = tempfile::tempdir().unwrap();
        let members = [("claude#builder1".to_string(), Role::Builder), ("claude#builder2".to_string(), Role::Builder)];
        let tasks = [
            ("Write README".to_string(), "docs/**".to_string()),
            ("Add tests".to_string(), String::new()),
        ];
        seed_kickoff(dir.path(), &members, &tasks);

        // The activity endpoint reads mailbox.jsonl line-by-line as {from,to,kind,body,ts}.
        let store = FileStore::new(dir.path().to_path_buf());
        let recs = store.mailbox_records();
        assert_eq!(recs.len(), 3, "one kickoff + one assign per task");
        assert_eq!(recs[0]["kind"], json!("kickoff"));
        assert_eq!(recs[0]["from"], json!("coordinator"));
        assert_eq!(recs[0]["to"], json!("broadcast"));
        assert!(recs[0]["body"].as_str().unwrap().contains("2 tasks"), "names the task count");
        assert!(recs[0]["body"].as_str().unwrap().contains("builder1"), "names the team");
        assert_eq!(recs[1]["kind"], json!("assign"));
        assert!(recs[1]["body"].as_str().unwrap().contains("Write README"));
        assert!(recs[1]["body"].as_str().unwrap().contains("docs/**"), "includes the scope when present");
        assert!(recs[2]["body"].as_str().unwrap().contains("Add tests"));
        assert!(!recs[2]["body"].as_str().unwrap().contains("scope:"), "no scope note when globs are empty");
    }

    #[test]
    fn seed_kickoff_appends_and_agent_posts_follow() {
        // The seed must not clobber later agent posts — they append after it.
        let dir = tempfile::tempdir().unwrap();
        seed_kickoff(dir.path(), &[("claude#b1".to_string(), Role::Builder)], &[("do it".to_string(), String::new())]);
        write_members(dir.path(), &[("claude#b1".to_string(), Role::Builder)]).unwrap();
        let store = FileStore::new(dir.path().to_path_buf());
        call(&store, "claude#b1", "mailbox.post", json!({ "to": "broadcast", "kind": "hello", "body": "b1 here" }));
        let recs = store.mailbox_records();
        assert_eq!(recs.len(), 3, "kickoff + assign + the agent's post");
        assert_eq!(recs[2]["kind"], json!("hello"));
    }

    #[test]
    fn seed_memory_writes_a_mission_brief_note_agents_can_recall() {
        let dir = tempfile::tempdir().unwrap();
        let members = [("claude#builder1".to_string(), Role::Builder)];
        let tasks = [("Catalog the files".to_string(), String::new())];
        seed_memory(dir.path(), &members, &tasks);

        // The activity endpoint parses <mission>/memory/*.md notes; a builder can recall the brief.
        let store = FileStore::new(dir.path().to_path_buf());
        let recalled = call(&store, "claude#builder1", "memory.recall", json!({ "query": "mission brief", "n": 5 }));
        let notes = recalled["notes"].as_array().cloned().unwrap_or_default();
        assert!(!notes.is_empty(), "the seeded brief is recallable from shared memory");
        assert!(
            notes.iter().any(|n| n["body"].as_str().unwrap_or("").contains("Catalog the files")),
            "the brief names the mission goals"
        );
        // And it lands as a real note file the panel reads.
        assert!(dir.path().join("memory").read_dir().unwrap().next().is_some(), "a note file exists on disk");
    }

    #[test]
    fn initialize_reports_server_info_and_tools_capability() {
        let (_d, s) = store();
        let msg = json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize" });
        let r = s.handle_message("a", &msg).unwrap();
        assert_eq!(r["result"]["serverInfo"]["name"], json!("cpe-swarm"));
        assert!(r["result"]["capabilities"]["tools"].is_object());
        assert_eq!(r["result"]["protocolVersion"], json!(PROTOCOL_VERSION));
    }

    #[test]
    fn tools_list_offers_the_tools_and_omits_the_destructive_drain() {
        let (_d, s) = store();
        let msg = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list" });
        let r = s.handle_message("a", &msg).unwrap();
        let names: Vec<String> = r["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        assert!(names.contains(&"mailbox.post".to_string()));
        assert!(names.contains(&"mailbox.read".to_string()));
        assert!(names.contains(&"memory.write".to_string()));
        assert!(!names.contains(&"mailbox.drain".to_string()), "shared host must not advertise drain");
    }

    #[test]
    fn a_notification_gets_no_response() {
        let (_d, s) = store();
        let msg = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
        assert!(s.handle_message("a", &msg).is_none());
    }

    #[test]
    fn an_unknown_method_is_a_jsonrpc_error() {
        let (_d, s) = store();
        let msg = json!({ "jsonrpc": "2.0", "id": 9, "method": "does/notExist" });
        let r = s.handle_message("a", &msg).unwrap();
        assert_eq!(r["error"]["code"], json!(-32601));
    }

    #[test]
    fn a_memory_write_is_visible_to_a_separate_process_over_the_shared_dir() {
        let dir = tempfile::tempdir().unwrap();
        // Two independent stores on the same dir = two agent processes.
        let a = FileStore::new(dir.path().to_path_buf());
        let b = FileStore::new(dir.path().to_path_buf());

        let w = call(&a, "claude#builder1", "memory.write", json!({ "body": "auth uses OAuth", "tags": ["auth"] }));
        assert_eq!(w["ok"], json!(true));

        let r = call(&b, "aider#reviewer1", "memory.recall", json!({ "query": "auth", "n": 5 }));
        assert_eq!(r["ok"], json!(true));
        let notes = r["notes"].as_array().unwrap();
        assert!(notes.iter().any(|n| n["body"].as_str().unwrap_or("").contains("OAuth")), "second process should recall the note");
    }

    #[test]
    fn a_mailbox_post_reaches_the_addressed_agent_in_a_separate_process() {
        let dir = tempfile::tempdir().unwrap();
        let a = FileStore::new(dir.path().to_path_buf());
        let b = FileStore::new(dir.path().to_path_buf());

        let p = call(&a, "coord", "mailbox.post", json!({ "to": { "agent": "b1" }, "kind": "assign", "body": "build the parser" }));
        assert_eq!(p["ok"], json!(true));

        let inbox = call(&b, "b1", "mailbox.read", json!({}));
        let msgs = inbox["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["body"], json!("build the parser"));
        assert_eq!(msgs[0]["from"], json!("coord"));
    }

    #[test]
    fn role_and_broadcast_delivery_uses_the_written_roster() {
        let dir = tempfile::tempdir().unwrap();
        write_members(
            dir.path(),
            &[
                ("coord".into(), Role::Coordinator),
                ("b1".into(), Role::Builder),
                ("b2".into(), Role::Builder),
            ],
        )
        .unwrap();
        let s = FileStore::new(dir.path().to_path_buf());

        // Post to the builder role — both builders receive it, the coordinator (sender) does not.
        let p = call(&s, "coord", "mailbox.post", json!({ "to": { "role": "builder" }, "body": "sync up" }));
        assert_eq!(p["ok"], json!(true));
        assert_eq!(call(&s, "b1", "mailbox.read", json!({}))["messages"].as_array().unwrap().len(), 1);
        assert_eq!(call(&s, "b2", "mailbox.read", json!({}))["messages"].as_array().unwrap().len(), 1);
        assert_eq!(call(&s, "coord", "mailbox.read", json!({}))["messages"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn drain_is_rejected_by_the_shared_host() {
        let (_d, s) = store();
        let out = call(&s, "a", "mailbox.drain", json!({}));
        assert_eq!(out["ok"], json!(false));
    }

    #[test]
    fn an_empty_body_post_is_rejected() {
        let (_d, s) = store();
        assert_eq!(call(&s, "a", "mailbox.post", json!({ "to": "broadcast", "body": "  " }))["ok"], json!(false));
    }
}
