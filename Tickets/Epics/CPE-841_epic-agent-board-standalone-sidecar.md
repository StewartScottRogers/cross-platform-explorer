---
id: CPE-841
title: "EPIC: Agent Board as a standalone singleton sidecar app (like the AI Console)"
type: Task
status: Proposed
priority: Medium
component: Multiple
tags: [epic, big-design]
estimate: 4h+
created: 2026-07-21
closed:
---

## Goal
Turn the **Agent Board** (today an in-app view — `BoardView.svelte` / `board.ts` over the
`ticket_board` backend, CPE-520) into a **standalone, singleton, host-managed external application**,
mirroring the **AI Console** sidecar architecture in as many ways as possible: its own process, its own
window, launched and supervised by `sidecar/host` over the versioned `sidecar/contract`, and enforced as a
single instance that coexists with — and outlives toggling of — the explorer.

## Why
The AI Console proved the pattern: a heavy, self-contained surface belongs in its **own process and
window**, not welded into the explorer's main window. The Agent Board is the same kind of surface — a
full Kanban workspace over `Tickets/` — and today it competes for space and lifecycle with the file
explorer. Making it a sidecar app gives it:

- its own resizable, independently-positioned window (multi-monitor friendly);
- a **singleton** identity, so there's exactly one board across the whole session no matter how many
  explorer windows exist, and it survives an explorer restart/reload;
- reuse of the already-hardened sidecar machinery — host supervision, the versioned contract,
  single-instance/daemon reaping, OS-keychain-backed secrets, IPC security — instead of re-inventing it;
- the delete-test property: gated behind the `sidecar-platform` feature, so the plain explorer ships
  with no Agent Board process and zero cost when off.

## Rough scope (areas, not child tickets)
- **Extract a sidecar binary/crate** for the Agent Board (analogous to `sidecar/ai-console`), owning its
  own window + UI, talking to the host over `sidecar/contract`. Reuse `cpe-server::ticket_board` for the
  card/column/frontmatter logic (it is already Tauri-free), so only the transport/host wiring is new.
- **Host supervision + launch**: `sidecar/host` spawns/attaches to the board like it does the console;
  a toolbar/menu action in the explorer launches or focuses it.
- **Singleton enforcement**: exactly one Agent Board instance (single-instance guard / host-owned
  daemon + reaping, like the AI Console session daemon), focus-existing-on-relaunch, survives explorer
  restart.
- **Bundling + identity**: its own product identity/installer overlay so it coexists with the explorer
  and the console (mirroring the `…crossplatformexplorer.sidecar` identity split), spawned with
  `CREATE_NO_WINDOW` for any helper processes (see CPE-840).
- **Migration/coexistence**: keep or retire the in-app `BoardView` — decide whether the embedded view
  becomes a thin launcher for the external app or is removed.

## Open questions (resolve at activation)
- **Same window vs. new window vs. new app?** How close to the AI Console: a bundled sidecar with its own
  webview window, or a fuller separate app? What exactly does "singleton external application in as many
  ways as possible" require — likely: separate process + separate window + single instance + host-managed
  lifecycle + own identity.
- **Reuse the AI Console host channel or a parallel one?** One host multiplexing console + board, or a
  second host instance/contract surface.
- **Data + live updates**: the board reads `Tickets/` on the local filesystem; how does the external
  process get the working directory / watch for ticket changes (reuse the CPE-398 watcher)?
- **Singleton scope**: one board per machine, or per repo/working-directory? Relaunch focuses the
  existing window.
- **What happens to the embedded `BoardView`** — launcher shim vs. removed vs. kept as a fallback in the
  plain build.
- **Security/permissions**: the board writes ticket files (moves cards between columns) — same IPC
  hardening + consent model as the console (CPE-275).

## Definition of Done
- The Agent Board runs as its **own process with its own window**, launched from the explorer, supervised
  by the host over the contract — not embedded in the explorer's main window.
- It is a **singleton**: one instance across the session; relaunch focuses the existing window; it
  survives an explorer restart/reload.
- It reuses the sidecar machinery (host supervision, contract, single-instance/reaping, secrets, IPC
  security) rather than a bespoke lifecycle, and its card logic still comes from
  `cpe-server::ticket_board`.
- Gated behind `sidecar-platform`: the plain explorer ships with **no** Agent Board process and no cost
  when off (delete-test).
- Architecture documented under `docs/design/` (how it mirrors, and where it deliberately diverges from,
  the AI Console).

## Relationship to other epics / prior context
- **Mirrors** the sidecar platform program (`sidecar/host` + `sidecar/contract` + `sidecar/ai-console`,
  CPE-259–314) — this epic applies that proven "singleton external app behind a versioned contract"
  pattern to the Agent Board.
- **Builds on** CPE-520 (the in-app Agent Board + `ticket_board` backend) and the Agent Watch watcher
  (CPE-398) for live ticket updates.
- Inherits the `CREATE_NO_WINDOW` discipline from **CPE-840** for any helper processes it spawns.

## Child tickets
_None yet — dormant brief. Decompose at activation (`/ticketing-epic activate CPE-841`)._
