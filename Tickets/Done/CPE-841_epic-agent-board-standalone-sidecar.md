---
id: CPE-841
title: "EPIC: Agent Board as a standalone singleton sidecar app (like the AI Console)"
type: Task
status: Done
priority: Medium
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-21
closed: 2026-07-21
---

## Resolution (closed 2026-07-21)
Delivered by **CPE-843 + CPE-844 + CPE-845** (all Done). The Agent Board now opens in its **own singleton
`WebviewWindow`**, launched from the explorer (a ⧉ pop-out button in the board title bar + a command-
palette entry), focusing the existing window on relaunch; the embedded in-app view is kept ("both").
`bootMode`'s `?board` marker mounts a chrome-less `BoardView windowed`; the window's label is in
`capabilities/default.json` so it can `invoke` `ticket_board` (the trusted-vs-isolated distinction from the
AI Console window is documented in `docs/design/STANDALONE-WINDOWS.md`). Window size/position persist via
`tauri-plugin-window-state`.

**DoD, as reframed by the activation decision (light window path, not a separate OS process):**
- ✅ Own window, launched from the explorer, app-wide singleton (CPE-843/844).
- ✅ Embedded view kept; both surfaces work (CPE-844).
- ✅ Architecture documented under `docs/design/` (CPE-845).
- **N/A — sidecar-platform gating / host-supervised process:** moot under the chosen light path. The board
  is a lightweight window in the base app (not a heavy sidecar process), so there is nothing to gate — it
  costs nothing until the user opens it. The heavier "separate OS process, host-supervised over the
  contract" framing in the original brief was explicitly **declined** at activation.
- **GUI-verify** of runtime behaviour (invoke works, card move, focus-on-relaunch, persistence) is
  implemented against the proven AI Console pattern and headless-verified (`npm run check` + full suite);
  on-screen confirmation rides the next install of a build carrying CPE-844.

> **Activated 2026-07-21** (`/ticketing-epic activate CPE-841`). Researched the AI Console architecture and
> resolved the open questions with the user (below). The `big-design` tag was dropped: the user chose the
> **lightweight** path — a singleton `WebviewWindow`, not a separate OS process — so this is a mostly-
> frontend epic, not a sidecar-platform-scale build. Decomposed into CPE-843/844/845.

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

## Decisions (activated 2026-07-21 — resolved with the user)
Research finding: the AI Console is **two** things — a singleton **window** (a Tauri `WebviewWindow`,
`App.svelte openAiConsole`: `getByLabel` → focus-or-create with a fixed label) **and** a separate
`ai-console.exe` process for its heavy backend (pty / sessions / swarm). The Agent Board only reads
`Tickets/` via the existing `ticket_board` backend, so it does not need the heavy process.

- **Process model → separate *window*, not a separate process.** A singleton `WebviewWindow` rendering the
  board (own resizable window, focus-on-relaunch). This mirrors the AI Console's *window* exactly and is
  lightweight — no new binary/host machinery. (The heavier separate-OS-process option was offered and
  declined.)
- **Embedded view → keep both.** The in-app `BoardView` stays; the standalone window is an addition (a
  pop-out), not a replacement.
- **Singleton scope → one, app-wide.** A single board window across the app; a second launch focuses the
  existing one (exactly like the AI Console).
- **Trust difference (important):** unlike the AI Console window (which loads an *untrusted* sidecar URL
  and is deliberately in no capability), the board window renders our **own trusted BoardView** and needs
  Tauri `invoke` — so its window label must be added to `capabilities/default.json` (CPE-844).

These supersede the heavier "extract a sidecar binary" framing in the Rough scope above.

## Child tickets
1. **CPE-843** — Standalone Agent Board page: render `BoardView` chrome-less when the app is loaded with an
   `agent-board` marker; reuse `board.ts` + `ticket_board`. Unit-tested. *Foundation — buildable now.*
2. **CPE-844** — Singleton Agent Board window + launcher: `openAgentBoard()` (`getByLabel` focus-or-create
   `WebviewWindow`, app-wide singleton) + a pop-out/menu launcher (keep the embedded view) + the
   `default.json` capability entry so the window can invoke + window-state persistence. **GUI-verified.**
   *(prereq: 843)*
3. **CPE-845** — Docs: the in-app Agent Board page + a design note on the standalone-window pattern (mirror
   of, and trust difference from, the AI Console window). *(prereq: 844)*

## Work Log
- **2026-07-21** — Activated. Researched the AI Console window/process split (`WebviewWindow` + `getByLabel`
  singleton vs the `ai-console.exe` sidecar) and the current embedded `BoardView`. Resolved the three open
  questions with the user (separate window / keep both / one app-wide singleton), dropped `big-design`
  (light path chosen), and decomposed into CPE-843 (foundation), CPE-844 (window+launcher, GUI), CPE-845
  (docs). Children are ordinary Backlog work now; the epic itself is not worked directly.
