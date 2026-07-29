---
id: CPE-954
title: Agent Deck — a demo that exercises the swarm Mailbox heavily (so messaging is visible)
type: feature
component: Frontend
priority: medium
status: Done
tags: ready
created: 2026-07-23
closed: 2026-07-23
epic: CPE-711
---

## Summary
On the Agent Deck the "Swarm coordination" panel has a **Mailbox** feed (`#sw-mailbox`, CPE-592) that shows
inter-agent messages. It stayed empty ("no messages yet…") for the how-to demos because every demo (CPE-924
onward) only told its builders to **narrate to their own terminals** — none told them to use the shared
**`mailbox.post`** MCP tool, so no messages were ever posted. Nothing is broken; the demos just never
exercised the messaging substrate.

Fix: add a **messaging-first demo** whose agents `mailbox.post` (broadcast) constantly — announce start,
post after every step, read the mailbox and acknowledge teammates, and broadcast a final summary — so the
Mailbox fills up and the messaging is impossible to miss. Also fold a light broadcast into the existing
multi-agent demos so coordination shows there too.

## Acceptance Criteria
- [x] A new demo (messaging-first, 3 agents — `teamchat`) instructs each builder to use
      `mailbox.post`/`mailbox.read` heavily (hello + per-step status + ack teammates + done broadcast); the
      Mailbox feed fills when it runs. New reusable `MSG` instruction constant drives it.
- [x] The demo appears in the demo dropdown (own "Messaging · watch the Mailbox" optgroup) and loads its
      task text; the note tells the user to watch the "Swarm coordination → Mailbox" panel.
- [x] Existing complex demos (`inventory`/`tour`/`testplan`) also broadcast a start + done via `MSG_LITE`.
- [x] Browser-verified (standalone launcher on :8899): dropdown shows it, selecting fills the task field
      with the `mailbox.post`/broadcast instructions, status line names the Mailbox. (Live mailbox flow —
      agents actually posting — is GUI-QA in the installed app with a real agent + key.)

## Notes
Tool contract (from `swarm_mcp_server.rs`): `mailbox.post { to: {agent:"id"}|{role:"builder"}|"broadcast",
kind, body }` and `mailbox.read { drain? }`. `/api/swarm/activity` tails `mailbox.jsonl` into `#sw-mailbox`.
File is `sidecar/ai-console/src/launcher.html` (`DEMOS`, `loadSelectedDemo`). Sidecar frontend change — the
next sidecar release bundles it (host rebuild).
