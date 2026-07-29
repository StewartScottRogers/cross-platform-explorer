---
id: CPE-961
title: Agent Board — "Send directive" writes a machine-readable directive into the ticket
type: feature
component: Multiple
priority: medium
status: Done
tags: ready
closed: 2026-07-23
created: 2026-07-23
epic: CPE-503
---

## Summary
First slice of board→agent communication (see the design discussion): let the card-detail popup emit a
**directive** — a structured, machine-readable instruction — into the ticket itself, so an agent (local or
an external one that shares the repo/folder) can read `open` directives, act, append a reply, and flip them
to `done`. The ticket file is the transport, so this works across boundaries you don't control (git/sync).

## Acceptance Criteria
- [x] Pure `ticket_board::append_directive(md, when, to, text)` — structured entry under `## Agent
      Directives` (creates section if absent; newest-first; empty text = no-op; blank target → `any`);
      clock-free; 2 tests.
- [x] `board_directive(root, id, target, text, when)` command — reuses `find_ticket_file`; registered + in
      the typed `collect_commands!` (regenerated `bindings.gen.ts` → `commands.boardDirective`).
- [x] Card-detail popup **"Send directive"** composer (instruction + target `any` default + Send/Enter);
      routes through the typed client, re-reads the ticket so the directive shows in the rendered body.
- [x] `npm run check` 0/0; vitest **930 pass**; `cargo test` append_directive (2) + app 67 + clippy clean
      (cpe-server + app default).
- [ ] GUI-verify in 0.57.26: send a directive from a card → it appears under `## Agent Directives`.
      *(attended, together)*

## Notes
Format: `### ▸ open · to \`<target>\` · <ISO-8601>` + the instruction text, so an external agent greps for
`open` directives, does the work, appends a reply, and edits `open`→`done`. Frontend supplies the ISO
timestamp (`new Date().toISOString()`). Foundation for the MCP / cpe-net directive paths later.
