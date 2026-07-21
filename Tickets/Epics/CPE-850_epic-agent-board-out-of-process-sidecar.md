---
id: CPE-850
title: "EPIC: Agent Board as an out-of-process sidecar (like AI Console / Repositories)"
type: Task
status: In Progress
priority: Medium
component: Multiple
tags: [epic, big-design]
estimate: 4h+
created: 2026-07-21
closed:
---

> **Activated on creation 2026-07-21** (user directive: "make the Agent Board out of process like the AI
> Console and Repositories, just like in settings"). This is the **heavy path** offered at CPE-841 and
> declined then — now chosen. It **supersedes** the CPE-841 in-process `WebviewWindow` (which stays as a
> convenience; this adds the real out-of-process sidecar).

## Goal
Ship the Agent Board as a first-class **out-of-process sidecar** on the platform ADR-0001 pattern —
alongside `ai-console` and `repos`: its own `sidecar/agent-board` crate + `sidecar.json` manifest, run and
supervised by the host, serving **its own** loopback UI (the Kanban board over `Tickets/`), and listed +
manageable in **Settings → the sidecar manager** (enable/disable, version, capabilities, diagnostics)
exactly like the other two.

## Why
The user wants parity with the AI Console / Repositories model: an isolated, independently-versioned,
consent-gated process the host manages, not a webview welded into the app. It gets the platform for free —
host supervision, the versioned contract, capability consent, OS-keychain secrets, per-sidecar diagnostics,
and the delete-test (no sidecar code in the plain build).

## Decisions
- **Out-of-process sidecar**, not the in-process window (CPE-841). The window may remain as a lightweight
  shortcut, but the canonical Agent Board becomes the sidecar.
- **Follow the `repos` template exactly:** a crate depending **only** on `sidecar-contract` (ADR 0001;
  the CI guard forbids depending on the app), a stdio protocol loop (Hello → Ready), and its own
  dependency-free loopback HTTP UI announced via a `ui:<url>` Status event.
- **Board data inside the sidecar:** the Kanban reads `Tickets/` itself (the sidecar is self-contained;
  it does **not** depend on `cpe-server`). The card/column/frontmatter logic is small and either
  reimplemented in the sidecar or lifted into a tiny shared, contract-free crate — resolved in CPE-852.
- **Foundation first:** a compliant, building, handshaking sidecar that serves a board-branded UI ships
  first (like `repos` shipped with a placeholder UI); real data + drag-to-move + host wiring + bundling
  follow.

## Child tickets
1. **CPE-851** — **Foundation:** the `sidecar/agent-board` crate (scaffolded) — protocol-conformant
   handshake, a loopback UI server announced to the host, `sidecar.json` (name "Agent Board", `ui:
   local_port`), added to CI's `sidecar` 3-OS job. Cargo-tested. *Buildable now.*
2. **CPE-852** — Board data + interactive UI in the sidecar: read `Tickets/` (via the granted `context`
   root), render the Kanban columns/cards, and move a card between columns (writes the ticket file +
   status). *(prereq: 851)*
3. **CPE-853** — Host launch + open-from-explorer: register/launch the `agent-board` sidecar and frame its
   announced UI, mirroring how the AI Console opens (a launcher entry). *(prereq: 851)*
4. **CPE-854** — Bundle + Settings: include `agent-board` in `release-sidecar.yml` (binary + manifest in
   the bundle) so the host discovers it and it appears in **SidecarManager** (enable/disable, version,
   capabilities, diagnostics) — **GUI-verified** alongside AI Console / Repositories. *(prereq: 851, 853)*

## Definition of Done
- `agent-board` is a registered sidecar: it appears in **Settings → sidecar manager** with its version and
  capabilities, and can be enabled/disabled like AI Console / Repositories, with per-sidecar diagnostics.
- It runs **out-of-process**, supervised by the host over the versioned contract, and serves its own board
  UI (Kanban over `Tickets/`, drag-to-move).
- It depends only on `sidecar-contract` (ADR 0001 / CI guard); the plain explorer ships with no
  agent-board code (delete-test).
- Architecture documented under `docs/design/`.

## Relationship
- **Supersedes** the CPE-841 in-process Agent Board *window* (kept as a convenience).
- **Mirrors** `sidecar/repos` (CPE-429/432) and `sidecar/ai-console` — the ADR-0001 sidecar platform
  (CPE-259–314). Reuses the scaffolder (CPE-303), the host registry/supervisor, `SidecarManager`
  (CPE-296/323), and the sidecar bundling.

## Work Log
- **2026-07-21** — Created + activated on the user's directive. Researched the sidecar platform (manifest
  registry, `repos`/`ai-console` template, the scaffolder, `SidecarManager`, release bundling). Scaffolded
  `sidecar/agent-board` and decomposed into CPE-851 (foundation) → 852 (data/UI) → 853 (host launch) → 854
  (bundle + settings). Starting CPE-851.
