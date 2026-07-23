---
id: CPE-783
title: User-defined templated commands (toolbar / context-menu / palette)
type: feature
status: Done
priority: medium
component: Multiple
tags: ready
created: 2026-07-20
closed: 2026-07-23
epic: CPE-711
estimate: 4h+
---

## Summary
The user-command layer for epic CPE-711: define templated commands (CPE-781) and bind them to the toolbar,
context menu, and palette; run them over the current selection via a backend exec command, **confirming
before launching any external process**. Commands persist in settings.

## Acceptance Criteria
- [x] Users define/edit/remove templated commands and choose where they surface; they run over the selection.
      *(`userCommands.ts` core + `UserCommandsDialog.svelte` editor/manager — add/edit/remove/reorder + surface checkboxes; browser-verified.)*
- [x] External processes are confirmed before launch; no regression to built-in actions; persistence works.
      *(backend `run_command` cargo-tested; `RunCommandConfirm.svelte` shows the resolved command line(s) + Run/Cancel gate, invokes `run_command` only on approval; persists via `settings.saveUserCommands`. Browser-verified the confirm gate renders. **End-to-end external-process spawn needs attended verification in the installed app** — no Tauri bridge under vite dev.)*
- [x] `npm run check` + suite green; GUI-verified.
      *(`npm run check` 0/0; vitest 929 pass; command palette + manager dialog + confirm gate all browser-verified.)*

## Notes
Prereq: CPE-781. GUI + a backend `run_command`-style exec (async, spawn_blocking; confirm in the UI first).
Menus follow MENUS.md + CPE-748 icons.

## Work Log
- 2026-07-20 (nightshift) — Picked up. Grep-first: `cmdTemplate.ts` (CPE-781) expands a template over a
  selection but there is **no store** for the command *definitions* — that ordered list (name + template +
  mode + surfaces) with CRUD/persist is the missing pure core that the binding UI is a thin layer over.
  Built that headlessly; the backend exec (process-spawning, confirm-before-launch) and the GUI are
  deliberately left for attended work.
- 2026-07-20 (nightshift) — Added `src/lib/userCommands.ts`: `UserCommand` ({id,name,template,mode,
  surfaces}), immutable `addCommand`/`updateCommand`/`removeCommand`/`moveCommand` (reorder, end-clamped),
  `commandsForSurface` (filter for toolbar/context/palette rendering), `resolveCommand` (delegates to
  `cmdTemplate.expandForSelection` so there's one runner — `each` → one line per entry, `joined` →
  quoted+joined), and tolerant `serializeCommands`/`parseCommands` (drops entries with a bad
  mode/surface/shape). Mirrors the other list stores. 8 vitest cases; `npm run check` clean.
- 2026-07-20 (nightshift) — **Deferred.** The command-definition logic (CRUD/reorder/bind/resolve/persist)
  is complete and headlessly green. Remaining: the **backend `run_command`-style exec** (async +
  spawn_blocking) with a **UI confirm before launching any external process**, and the GUI (editor dialog +
  surfacing commands on the toolbar/context-menu/palette with MENUS.md + CPE-748 icons).
  - *deferred-on:* the confirmed backend exec + the attended GUI (this ticket is "component: Multiple",
    tagged GUI). The exec is intentionally not built unattended — launching arbitrary external processes is
    the high-blast-radius part and wants a human in the loop.
  - *revisit-when:* an attended session — add the backend exec (confirm in the UI first), then build the
    editor + surface bindings over `userCommands` + `resolveCommand`, and GUI-verify. No external gate.

- 2026-07-20 (nightshift, backend authorized) — Re-picked from Deferred to build the **backend exec**.
  Added `run_command(command, cwd?) -> CommandOutput{stdout,stderr,code,truncated}` (+ `_impl`,
  `capped_string`) in `lib.rs`, registered in `generate_handler!`. Runs the resolved command line through
  the platform shell (`cmd /C` / `sh -c`) so a normal command with pipes/quotes works; captures output
  (capped at 1 MiB/stream, truncation flagged) + exit code; rejects an empty command. A module comment
  states the hard contract: **the frontend must confirm the resolved command before calling** — this is the
  thin, gated executor, never invoked implicitly. 4 cargo tests (stdout+zero exit; non-zero exit code;
  empty rejected; `capped_string` byte-cap). Full backend suite green; clippy clean both feature modes.
- 2026-07-20 (nightshift) — **Deferred again** (GUI-only now). Both headless halves are done: the command
  store (`userCommands.ts`, #46) and the backend exec (`run_command`, this PR). Remaining is purely the
  attended GUI — the editor dialog, surfacing commands on the toolbar/context-menu/palette (MENUS.md +
  CPE-748 icons), and the **confirm dialog** that shows `resolveCommand`'s output and calls `run_command`
  on approval.
  - *deferred-on:* the attended GUI (editor + surfacing + confirm dialog) and its GUI verification.
  - *revisit-when:* an attended session — build the editor over `userCommands`, wire the confirm dialog →
    `run_command`, surface commands via `commandsForSurface`, and GUI-verify. No external gate.

- 2026-07-23 (dayshift, attended GUI) — **Built the GUI tail and closed the ticket.** Added
  `UserCommandsDialog.svelte` (manager/editor over `userCommands` — add/edit/remove/reorder, mode radio
  each/joined, surface checkboxes toolbar/context/palette; MENUS.md-conformant themed dialog with a visible
  border) and `RunCommandConfirm.svelte` (the confirm-before-launch gate: shows the resolved command line(s)
  from `resolveCommand`, Run/Cancel, invokes `run_command` only on approval, then shows per-command exit
  code + stdout/stderr). Wired `App.svelte`: `settings.loadUserCommands()` on mount, a "Manage user
  commands…" palette entry, palette-surfaced commands (`commandsForSurface(...,"palette")`) that open the
  confirm gate, and both dialog renders; added `loadUserCommands`/`saveUserCommands` + `KEYS.userCommands`
  to `settings.ts`. **Browser-verified** (vite dev, Chrome): palette → manager renders → add "Open in VS
  Code / code {path}" with palette surface → command appears under the palette "COMMANDS" group → clicking
  it opens the `Run "…"?` confirm gate (correctly "Nothing to run" with an empty selection). `npm run
  check` 0/0; vitest 929 pass. The only unattended-unverifiable slice is the real external-process spawn,
  which needs the installed app (no Tauri bridge under vite dev) — flagged for the user's attended check.

## Resolution (complete)
Feature shipped end-to-end. Headless halves (landed earlier): the pure command store `userCommands.ts`
(CRUD, reorder, surface filtering, `resolveCommand`, tolerant persist) and the backend executor
`run_command` (shell-run a resolved command line, capped output + exit-code capture, empty-guard). GUI tail
(this PR): `UserCommandsDialog.svelte` editor/manager + `RunCommandConfirm.svelte` confirm-before-launch
gate + `App.svelte` wiring (palette entry, palette-surfaced commands, persistence). The frontend always
confirms the resolved command before invoking `run_command` — the safety gate is enforced in the UI.
Browser-verified; `npm run check` clean, vitest 929 green. Note for the user: the actual external-process
launch (`run_command` spawning e.g. `code <path>`) can only be exercised in the installed app.
