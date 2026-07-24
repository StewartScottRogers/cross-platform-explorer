//! Ticket-directives **MCP** surface (CPE-962, epic CPE-503): exposes the Agent Board's `## Agent
//! Directives` (CPE-961) over the Model Context Protocol so ANY MCP-speaking agent — Claude or another,
//! deployed outside your control — can `directives.list` the open directives across a repo's `Tickets/`
//! and `directives.reply` (append a reply + optionally flip `open`→`done`). The ticket files stay the
//! source of truth.
//!
//! Pure JSON-RPC dispatch over a [`DirectiveStore`] seam (mirrors the swarm MCP host shape), so it's
//! unit-testable without I/O; the runnable stdio server (`ticket-mcp` bin) implements the store over the
//! real filesystem.

use serde_json::{json, Value};

/// MCP protocol version this server speaks.
pub const PROTOCOL_VERSION: &str = "2024-11-05";

/// An `open` directive plus the ticket id it belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenDirective {
    pub ticket: String,
    pub when: String,
    pub to: String,
    pub text: String,
}

/// The store the MCP tools act on. A real impl walks a repo's `Tickets/`; tests use an in-memory fake.
pub trait DirectiveStore {
    /// Every `open` directive across the repo, each tagged with its ticket id.
    fn list_open(&self) -> Vec<OpenDirective>;
    /// Append `reply` to the directive in `ticket` identified by `when`, marking it `done` if `done`.
    /// `Ok(())` on success; `Err(msg)` if the ticket or directive wasn't found.
    fn reply(&mut self, ticket: &str, when: &str, reply: &str, done: bool) -> Result<(), String>;
}

/// The two tools this server advertises.
pub fn tools_manifest() -> Value {
    json!({
        "tools": [
            {
                "name": "directives.list",
                "description": "List every OPEN agent directive across the project's tickets. Each has the ticket id, an ISO `when` timestamp (its identity), the intended `to` agent, and the instruction `text`.",
                "inputSchema": { "type": "object", "properties": {} }
            },
            {
                "name": "directives.reply",
                "description": "Reply to a directive and (optionally) resolve it. Appends your `reply` under the directive and flips it open→done when `done` is true.",
                "inputSchema": {
                    "type": "object",
                    "required": ["ticket", "when", "reply"],
                    "properties": {
                        "ticket": { "type": "string", "description": "The ticket id, e.g. CPE-520." },
                        "when": { "type": "string", "description": "The directive's ISO `when` timestamp (from directives.list)." },
                        "reply": { "type": "string", "description": "Your reply / result." },
                        "done": { "type": "boolean", "description": "Mark the directive done (default false)." }
                    }
                }
            }
        ]
    })
}

/// Route one `tools/call` onto the store. Returns the tool's JSON result (an `{ok, …}` object).
fn call_tool(store: &mut dyn DirectiveStore, name: &str, args: &Value) -> Value {
    match name {
        "directives.list" => {
            let items: Vec<Value> = store
                .list_open()
                .into_iter()
                .map(|d| json!({ "ticket": d.ticket, "when": d.when, "to": d.to, "text": d.text }))
                .collect();
            json!({ "ok": true, "directives": items })
        }
        "directives.reply" => {
            let ticket = args.get("ticket").and_then(|v| v.as_str()).unwrap_or("");
            let when = args.get("when").and_then(|v| v.as_str()).unwrap_or("");
            let reply = args.get("reply").and_then(|v| v.as_str()).unwrap_or("");
            let done = args.get("done").and_then(|v| v.as_bool()).unwrap_or(false);
            if ticket.is_empty() || when.is_empty() {
                return json!({ "ok": false, "error": "`ticket` and `when` are required" });
            }
            match store.reply(ticket, when, reply, done) {
                Ok(()) => json!({ "ok": true }),
                Err(e) => json!({ "ok": false, "error": e }),
            }
        }
        other => json!({ "ok": false, "error": format!("unknown tool '{other}'") }),
    }
}

/// Handle one JSON-RPC message (MCP). Returns the response to write back, or `None` for a notification.
/// Pure over the store, so it's fully unit-testable without stdio.
pub fn handle_message(store: &mut dyn DirectiveStore, msg: &Value) -> Option<Value> {
    let id = msg.get("id").cloned();
    let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
    match method {
        "initialize" => Some(rpc_result(
            id,
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "serverInfo": { "name": "cpe-tickets", "version": env!("CARGO_PKG_VERSION") },
                "capabilities": { "tools": {} }
            }),
        )),
        "notifications/initialized" => None,
        "tools/list" => Some(rpc_result(id, tools_manifest())),
        "tools/call" => {
            let params = msg.get("params").cloned().unwrap_or_else(|| json!({}));
            let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            let result = call_tool(store, name, &args);
            // MCP wraps a tool result in a content envelope.
            Some(rpc_result(id, json!({ "content": [ { "type": "text", "text": result.to_string() } ] })))
        }
        _ if id.is_some() => Some(rpc_error(id, -32601, &format!("method not found: {method}"))),
        _ => None,
    }
}

fn rpc_result(id: Option<Value>, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id.unwrap_or(Value::Null), "result": result })
}
fn rpc_error(id: Option<Value>, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id.unwrap_or(Value::Null), "error": { "code": code, "message": message } })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FakeStore {
        open: Vec<OpenDirective>,
        replied: Vec<(String, String, String, bool)>,
    }
    impl DirectiveStore for FakeStore {
        fn list_open(&self) -> Vec<OpenDirective> {
            self.open.clone()
        }
        fn reply(&mut self, ticket: &str, when: &str, reply: &str, done: bool) -> Result<(), String> {
            if self.open.iter().any(|d| d.ticket == ticket && d.when == when) {
                self.replied.push((ticket.into(), when.into(), reply.into(), done));
                Ok(())
            } else {
                Err(format!("no open directive {when} in {ticket}"))
            }
        }
    }

    fn call(store: &mut dyn DirectiveStore, name: &str, args: Value) -> Value {
        let msg = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": args } });
        let resp = handle_message(store, &msg).unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn advertises_the_two_directive_tools() {
        let names: Vec<String> = tools_manifest()["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        assert!(names.contains(&"directives.list".to_string()));
        assert!(names.contains(&"directives.reply".to_string()));
    }

    #[test]
    fn initialize_reports_the_server() {
        let mut s = FakeStore::default();
        let r = handle_message(&mut s, &json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize" })).unwrap();
        assert_eq!(r["result"]["serverInfo"]["name"], json!("cpe-tickets"));
        assert_eq!(r["result"]["protocolVersion"], json!(PROTOCOL_VERSION));
    }

    #[test]
    fn list_returns_open_directives_and_reply_routes_to_the_store() {
        let mut s = FakeStore::default();
        s.open.push(OpenDirective { ticket: "CPE-1".into(), when: "t1".into(), to: "any".into(), text: "do it".into() });
        let listed = call(&mut s, "directives.list", json!({}));
        assert_eq!(listed["ok"], json!(true));
        assert_eq!(listed["directives"][0]["ticket"], json!("CPE-1"));
        assert_eq!(listed["directives"][0]["text"], json!("do it"));

        let ok = call(&mut s, "directives.reply", json!({ "ticket": "CPE-1", "when": "t1", "reply": "done", "done": true }));
        assert_eq!(ok["ok"], json!(true));
        assert_eq!(s.replied, vec![("CPE-1".into(), "t1".into(), "done".into(), true)]);

        // Missing required args + unknown ticket are clean errors, not panics.
        assert_eq!(call(&mut s, "directives.reply", json!({ "reply": "x" }))["ok"], json!(false));
        assert_eq!(call(&mut s, "directives.reply", json!({ "ticket": "CPE-9", "when": "z", "reply": "x" }))["ok"], json!(false));
    }

    #[test]
    fn unknown_method_with_id_is_a_jsonrpc_error() {
        let mut s = FakeStore::default();
        let r = handle_message(&mut s, &json!({ "jsonrpc": "2.0", "id": 5, "method": "nope" })).unwrap();
        assert_eq!(r["error"]["code"], json!(-32601));
        // A notification (no id) yields no response.
        assert!(handle_message(&mut s, &json!({ "method": "notifications/initialized" })).is_none());
    }
}
