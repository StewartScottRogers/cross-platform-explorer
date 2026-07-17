//! CPE-541: the `--swarm-mcp` entry runs as its **own process**, speaks JSON-RPC 2.0 over stdio, and
//! shares state through the mission dir — so two agent processes genuinely coordinate. The routing is
//! unit-tested in-process (`swarm_mcp_server`); this proves the separate-process + stdio wiring is real.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

/// Spawn one live MCP host process for `agent` against the shared mission `dir`.
fn spawn(dir: &std::path::Path, agent: &str) -> (Child, ChildStdin, BufReader<ChildStdout>) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ai-console"))
        .args(["--swarm-mcp", "--dir", dir.to_str().unwrap(), "--agent", agent])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn --swarm-mcp process");
    let stdin = child.stdin.take().unwrap();
    let stdout = BufReader::new(child.stdout.take().unwrap());
    (child, stdin, stdout)
}

/// Send one JSON-RPC line and read one response line back.
fn rpc(stdin: &mut ChildStdin, stdout: &mut BufReader<ChildStdout>, msg: &serde_json::Value) -> serde_json::Value {
    writeln!(stdin, "{msg}").unwrap();
    stdin.flush().unwrap();
    let mut line = String::new();
    stdout.read_line(&mut line).expect("read response line");
    serde_json::from_str(line.trim()).expect("json-rpc response")
}

/// Unwrap an MCP `tools/call` result envelope back to the tool's JSON payload.
fn tool_payload(resp: &serde_json::Value) -> serde_json::Value {
    let text = resp["result"]["content"][0]["text"].as_str().expect("content text");
    serde_json::from_str(text).expect("tool json")
}

#[test]
fn two_swarm_mcp_processes_share_memory_and_mailbox_over_stdio() {
    let dir = tempfile::tempdir().unwrap();

    // Agent A's process: initialize handshake, then write a memory note + post a message to b1.
    let (mut a, mut a_in, mut a_out) = spawn(dir.path(), "coord");
    let init = rpc(
        &mut a_in,
        &mut a_out,
        &serde_json::json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize" }),
    );
    assert_eq!(init["result"]["serverInfo"]["name"], serde_json::json!("cpe-swarm"));

    let wrote = tool_payload(&rpc(
        &mut a_in,
        &mut a_out,
        &serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": { "name": "memory.write", "arguments": { "body": "parser owns src/parse.rs", "tags": ["plan"] } }
        }),
    ));
    assert_eq!(wrote["ok"], serde_json::json!(true));

    let posted = tool_payload(&rpc(
        &mut a_in,
        &mut a_out,
        &serde_json::json!({
            "jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": { "name": "mailbox.post", "arguments": { "to": { "agent": "b1" }, "kind": "assign", "body": "build the parser" } }
        }),
    ));
    assert_eq!(posted["ok"], serde_json::json!(true));

    // Agent B's process (separate process, same mission dir): recall the note + read the inbox.
    let (mut b, mut b_in, mut b_out) = spawn(dir.path(), "b1");
    let recalled = tool_payload(&rpc(
        &mut b_in,
        &mut b_out,
        &serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": { "name": "memory.recall", "arguments": { "query": "parser", "n": 5 } }
        }),
    ));
    assert!(
        recalled["notes"].as_array().unwrap().iter().any(|n| n["body"].as_str().unwrap_or("").contains("src/parse.rs")),
        "b1's process should recall coord's note over the shared dir"
    );

    let inbox = tool_payload(&rpc(
        &mut b_in,
        &mut b_out,
        &serde_json::json!({
            "jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": { "name": "mailbox.read", "arguments": {} }
        }),
    ));
    let msgs = inbox["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 1, "b1 should see coord's message");
    assert_eq!(msgs[0]["body"], serde_json::json!("build the parser"));

    // Closing stdin ends each process's read loop cleanly.
    drop(a_in);
    drop(b_in);
    let _ = a.wait();
    let _ = b.wait();
}
