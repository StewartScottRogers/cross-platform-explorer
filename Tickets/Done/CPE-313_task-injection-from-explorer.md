---
id: CPE-313
title: Task/prompt injection from explorer context
type: Feature
status: Done
closed: 2026-07-13
priority: Medium
component: Multiple
estimate: 2-3h
created: 2026-07-13
---

## Summary

The reason the console lives inside a file explorer: act on what you're looking at.
Right-click a folder/selection → "Work on this with <agent>", which opens a console
session in that repo seeded with a task referencing the selection. Uses the context
capability ([[CPE-267]]) so the explorer stays decoupled.

## Acceptance Criteria

- [ ] Explorer context action hands the current folder/selection to the console via
      the context capability — no direct coupling to console internals.
- [ ] Opens/starts a session scoped to that repo, pre-filled with the task/selection.
- [ ] Works from folder and multi-selection; degrades cleanly if no agent installed.
- [ ] The explorer feature compiles/ships even with the console absent (delete-test).

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-267]], [[CPE-289]]. **Phase:** C2 (basic) → C5 (rich).
**Epic:** [[CPE-261]].

## Work Log
2026-07-13 — Filed during epic-plan hardening. This is the explorer↔console payoff.

## Work Log
2026-07-13 (Dayshift) — Implemented on branch `CPE-313-task-injection`.

The explorer↔console hand-off is **decoupled via the console's loopback URL** (the explorer
never touches console internals — it just opens a URL the console reads client-side):
- **`src/lib/sidecar.ts`** — pure `consoleUrlWith(base, cwd?, task?)` appends `cwd`/`task`
  query params (unit-tested: 4 cases).
- **`App.svelte`** — `openAiConsole({cwd,task})` / `launchAiConsole` thread the context into
  the window URL; `openSelectionInConsole()` maps the selection to it (single folder → scope
  to it; files → current folder + a task naming them; nothing → current folder). Commands
  `open-in-console` (selection) and `open-folder-in-console` (empty-area → current folder).
  Reusing an already-open console focuses it with a notice (can't re-scope a live window
  without disrupting running sessions).
- **`ContextMenu.svelte`** — "Work on this in AI Console" on a row selection; "Work on this
  folder in AI Console" on the empty-area menu (both gated to real folders, not Home/archive).
- **`launcher.html`** — on load, reads `?cwd`/`?task`: pre-fills the working-folder input and
  surfaces the task as a suggested prompt in the status bar.

Verification: `npm run check` 0 errors; frontend suite 296 pass (incl. the new
`consoleUrlWith` tests); `cargo build` (embeds launcher.html) clean.

Assumptions (Dayshift): (a) decoupling via the loopback URL query rather than the CPE-267
context-capability broker — same no-coupling guarantee, far simpler, and the console window
deliberately has no Tauri API so a URL is the natural channel; (b) the task is **pre-filled /
surfaced** for the user to launch & run, not auto-typed into the agent's PTY — auto-injection
is fragile across full-screen TUI agents (timing/alt-screen) and is left as a rich follow-up;
(c) "degrades cleanly when console absent" is handled by the existing "AI Console isn't
available in this build" notice. **Visual confirmation** (right-click → console opens scoped
to the folder) is the one thing the headless harness can't check — recommend an eyeball.
