//! Swarm coordination tools over MCP — the **router seam** (CPE-541, epic CPE-528).
//!
//! [`CPE-528`](../../../Tickets/Epics/CPE-528_epic-wire-agent-workspace-live-sessions.md) needs the
//! in-process [`Mailbox`](crate::swarm_mailbox::Mailbox) ([[CPE-516]]) and shared
//! [`MemoryGraph`](crate::agent_memory::MemoryGraph) ([[CPE-525]]) exposed to **external agent
//! processes** so a launched swarm coordinates and shares context. That exposure is one MCP host over
//! **stdio** (no network listener, no port, no bearer token — the smallest surface; each launched agent
//! speaks to it over its own pipe). Standing up the live stdio server + wiring it into each launch is
//! the live remainder of CPE-541 (needs the running app + GUI QA).
//!
//! The part that is **pure** — and therefore lives here, unit-tested — is what an MCP host is mostly
//! made of: the **tool manifest** (`tools/list`) and the **call router** (`tools/call`) that maps a
//! tool name + JSON args onto the in-process mailbox / memory APIs. Keeping this transport-free means
//! the tool contract is provable without a live server; the server just pipes bytes into it.

use serde_json::{json, Value};

use crate::agent_memory::{memory_tool, MemoryGraph};
use crate::swarm_mailbox::{Mailbox, Recipient};
use crate::swarm_team::Role;

/// The coordination tools this host exposes, as an MCP `tools/list` payload: `{ "tools": [ { name,
/// description, inputSchema }, ... ] }`. Kept deliberately small — the two coordination primitives a
/// swarm needs: a mailbox and a shared memory graph.
pub fn tools_manifest() -> Value {
    json!({
        "tools": [
            {
                "name": "mailbox.post",
                "description": "Post a coordination message to another agent, a role, or the whole team.",
                "inputSchema": {
                    "type": "object",
                    "required": ["to", "body"],
                    "properties": {
                        "to": { "description": "'broadcast', {\"agent\":\"id\"}, or {\"role\":\"builder\"}" },
                        "kind": { "type": "string", "description": "Message kind/topic (default 'note')." },
                        "body": { "type": "string" }
                    }
                }
            },
            {
                "name": "mailbox.read",
                "description": "Peek this agent's inbox in order without clearing it.",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "mailbox.drain",
                "description": "Take and clear this agent's inbox in order.",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "memory.write",
                "description": "Write a note (with tags + [[links]]) into the shared memory graph.",
                "inputSchema": {
                    "type": "object",
                    "required": ["body"],
                    "properties": {
                        "body": { "type": "string" },
                        "tags": { "type": "array", "items": { "type": "string" } },
                        "id": { "type": "string" }
                    }
                }
            },
            {
                "name": "memory.read",
                "description": "Read one note from the shared memory graph by id.",
                "inputSchema": { "type": "object", "required": ["id"], "properties": { "id": { "type": "string" } } }
            },
            {
                "name": "memory.recall",
                "description": "Recall the most relevant notes for a query + tags.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" },
                        "tags": { "type": "array", "items": { "type": "string" } },
                        "n": { "type": "integer" }
                    }
                }
            }
        ]
    })
}

/// Parse the `to` field of a `mailbox.post` call into a [`Recipient`]. Accepts the string
/// `"broadcast"`, `{"agent": "id"}`, or `{"role": "builder"}` (role names match the lowercase
/// [`Role`] vocabulary). Returns `Err` with a caller-facing message on anything else.
fn parse_recipient(to: &Value) -> Result<Recipient, String> {
    if let Some(s) = to.as_str() {
        return match s {
            "broadcast" => Ok(Recipient::Broadcast),
            other => Err(format!("unknown recipient string '{other}' (expected 'broadcast')")),
        };
    }
    if let Some(id) = to.get("agent").and_then(|v| v.as_str()) {
        return Ok(Recipient::Agent(id.to_string()));
    }
    if let Some(role) = to.get("role") {
        return serde_json::from_value::<Role>(role.clone())
            .map(Recipient::Role)
            .map_err(|_| format!("unknown role {role} (expected coordinator/builder/scout/reviewer)"));
    }
    if to.get("broadcast").and_then(|v| v.as_bool()) == Some(true) {
        return Ok(Recipient::Broadcast);
    }
    Err("'to' must be 'broadcast', {\"agent\":id}, or {\"role\":name}".into())
}

/// Serialize an inbox message to the wire shape returned to a reading agent.
fn message_json(m: &crate::swarm_mailbox::Message) -> Value {
    json!({ "seq": m.seq, "from": m.from, "kind": m.kind, "body": m.body, "ts": m.ts })
}

/// Route one MCP `tools/call` onto the in-process mailbox / memory. `from` is the **calling agent's
/// id** (the live host knows which session's pipe a call arrived on — a client can't spoof another
/// agent because it never supplies its own id here), and `ts` is a host-supplied timestamp (kept out of
/// the pure core so the mailbox stays deterministic in tests). Every result is a JSON object with an
/// `ok` boolean; unknown tools return `ok:false` rather than erroring, matching `memory_tool`.
pub fn dispatch_tool(
    mailbox: &mut Mailbox,
    memory: &mut MemoryGraph,
    from: &str,
    tool: &str,
    args: &Value,
    ts: u64,
) -> Value {
    match tool {
        "mailbox.post" => {
            let to = match args.get("to") {
                Some(v) => match parse_recipient(v) {
                    Ok(r) => r,
                    Err(e) => return json!({ "ok": false, "error": e }),
                },
                None => return json!({ "ok": false, "error": "missing 'to'" }),
            };
            let body = args.get("body").and_then(|v| v.as_str()).unwrap_or("");
            if body.trim().is_empty() {
                return json!({ "ok": false, "error": "empty body" });
            }
            let kind = args.get("kind").and_then(|v| v.as_str()).unwrap_or("note");
            let seq = mailbox.post(from, to, kind, body, ts);
            json!({ "ok": true, "seq": seq })
        }
        "mailbox.read" => {
            let msgs: Vec<Value> = mailbox.read(from).iter().map(message_json).collect();
            json!({ "ok": true, "messages": msgs })
        }
        "mailbox.drain" => {
            let msgs: Vec<Value> = mailbox.drain(from).iter().map(message_json).collect();
            json!({ "ok": true, "messages": msgs })
        }
        // Memory tools already have a pure dispatcher (CPE-525) — forward straight to it.
        t if t.starts_with("memory.") => memory_tool(memory, t, args),
        other => json!({ "ok": false, "error": format!("unknown tool '{other}'") }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn team() -> Mailbox {
        let mut mb = Mailbox::new();
        mb.register("coord", Role::Coordinator);
        mb.register("b1", Role::Builder);
        mb.register("b2", Role::Builder);
        mb
    }

    #[test]
    fn the_manifest_lists_every_exposed_tool() {
        let names: Vec<String> = tools_manifest()["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        for expected in ["mailbox.post", "mailbox.read", "mailbox.drain", "memory.write", "memory.read", "memory.recall"] {
            assert!(names.contains(&expected.to_string()), "manifest missing {expected}");
        }
    }

    #[test]
    fn posting_to_a_role_lands_in_each_member_of_that_role() {
        let mut mb = team();
        let mut mem = MemoryGraph::new();
        let out = dispatch_tool(&mut mb, &mut mem, "coord", "mailbox.post", &json!({ "to": { "role": "builder" }, "kind": "assign", "body": "build it" }), 1);
        assert_eq!(out["ok"], json!(true));
        // Both builders got it; the coordinator (sender) did not.
        assert_eq!(mb.inbox_len("b1"), 1);
        assert_eq!(mb.inbox_len("b2"), 1);
        assert_eq!(mb.inbox_len("coord"), 0);
    }

    #[test]
    fn read_returns_the_calling_agents_inbox_and_drain_clears_it() {
        let mut mb = team();
        let mut mem = MemoryGraph::new();
        dispatch_tool(&mut mb, &mut mem, "coord", "mailbox.post", &json!({ "to": { "agent": "b1" }, "body": "hi" }), 7);

        let read = dispatch_tool(&mut mb, &mut mem, "b1", "mailbox.read", &json!({}), 0);
        assert_eq!(read["messages"].as_array().unwrap().len(), 1);
        assert_eq!(read["messages"][0]["body"], json!("hi"));
        assert_eq!(read["messages"][0]["from"], json!("coord"));

        let drain = dispatch_tool(&mut mb, &mut mem, "b1", "mailbox.drain", &json!({}), 0);
        assert_eq!(drain["messages"].as_array().unwrap().len(), 1);
        // Now empty.
        let again = dispatch_tool(&mut mb, &mut mem, "b1", "mailbox.read", &json!({}), 0);
        assert_eq!(again["messages"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn an_empty_body_is_rejected() {
        let mut mb = team();
        let mut mem = MemoryGraph::new();
        let out = dispatch_tool(&mut mb, &mut mem, "coord", "mailbox.post", &json!({ "to": "broadcast", "body": "   " }), 1);
        assert_eq!(out["ok"], json!(false));
    }

    #[test]
    fn a_bad_recipient_is_a_caller_facing_error_not_a_panic() {
        let mut mb = team();
        let mut mem = MemoryGraph::new();
        let out = dispatch_tool(&mut mb, &mut mem, "coord", "mailbox.post", &json!({ "to": { "role": "wizard" }, "body": "x" }), 1);
        assert_eq!(out["ok"], json!(false));
        let out = dispatch_tool(&mut mb, &mut mem, "coord", "mailbox.post", &json!({ "to": 42, "body": "x" }), 1);
        assert_eq!(out["ok"], json!(false));
    }

    #[test]
    fn memory_tools_are_forwarded_to_the_shared_graph() {
        let mut mb = team();
        let mut mem = MemoryGraph::new();
        let w = dispatch_tool(&mut mb, &mut mem, "b1", "memory.write", &json!({ "body": "auth uses OAuth", "tags": ["auth"] }), 0);
        assert_eq!(w["ok"], json!(true));
        let r = dispatch_tool(&mut mb, &mut mem, "b2", "memory.recall", &json!({ "query": "auth", "n": 3 }), 0);
        assert_eq!(r["ok"], json!(true));
        assert!(!r["notes"].as_array().unwrap().is_empty(), "the note should be recallable by another agent");
    }

    #[test]
    fn an_unknown_tool_reports_rather_than_panics() {
        let mut mb = team();
        let mut mem = MemoryGraph::new();
        let out = dispatch_tool(&mut mb, &mut mem, "coord", "nonsense.tool", &json!({}), 0);
        assert_eq!(out["ok"], json!(false));
    }
}
