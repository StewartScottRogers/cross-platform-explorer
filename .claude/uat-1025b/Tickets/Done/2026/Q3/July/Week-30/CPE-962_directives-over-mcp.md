---
id: CPE-962
title: Expose ticket directives over MCP — any MCP agent can list + reply
type: feature
component: Backend
priority: medium
status: Doing
tags: ready
created: 2026-07-23
epic: CPE-503
---

## Summary
Second slice of board→agent comms (after CPE-961's directive-in-ticket): expose the `## Agent Directives`
over the **Model Context Protocol** so ANY MCP-speaking agent (Claude, others — deployed outside your
control) can `directives.list` open directives across the repo's `Tickets/` and `directives.reply` (append a
reply + flip `open`→`done`). A runnable stdio MCP server the agent configures; the ticket files stay the
source of truth.

## Acceptance Criteria
- [x] `ticket_board`: `ticket_id`, `parse_directives(md) -> Vec<Directive{status,to,when,text}>`,
      `reply_to_directive(md, when, reply, mark_done) -> Option<String>`; 4 tests.
- [x] `cpe_server::ticket_mcp`: MCP `tools_manifest()` + `handle_message` JSON-RPC dispatch
      (`initialize` / `tools/list` / `tools/call` → `directives.list` + `directives.reply`) over a
      `DirectiveStore` seam; 4 tests with a fake store.
- [x] `ticket-mcp` bin: `FsStore` over the real `Tickets/**` (walk + `ticket_id`/`parse_directives`/reply) +
      a newline-delimited stdio JSON-RPC loop.
- [x] `cargo test` **321 pass** + clippy clean (incl. the bin); no GUI change; app still builds.
- [x] **Smoke-tested live**: piped `initialize`/`tools/list`/`directives.list` into `ticket-mcp <repo>` — it
      returned the real directive the board wrote on CPE-959 ("Summarize the risks in this ticket").

## Resolution
Ticket directives are now exposed over MCP. Any MCP-speaking agent launches `ticket-mcp <repo-root>` and
gets `directives.list` (open directives across `Tickets/**`) + `directives.reply` (append + resolve). The
ticket files remain the source of truth, so this works for agents deployed outside your control (they just
need the repo). Verified end-to-end against the live repo. Next steps toward `cpe-net` remain optional.

## Notes
Directive format (CPE-961): `### ▸ <status> · to \`<target>\` · <ISO>` + text under `## Agent Directives`.
Mirror the swarm MCP JSON-RPC shape (`swarm_mcp_server`), but cpe-server is standalone so this lives here
(ai-console doesn't depend on cpe-server). serde_json already a dep. Foundation extends toward the cpe-net path.
