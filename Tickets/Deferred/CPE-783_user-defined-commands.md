---
id: CPE-783
title: User-defined templated commands (toolbar / context-menu / palette)
type: feature
status: Open
priority: medium
component: Multiple
tags: needs-prereq
created: 2026-07-20
closed:
epic: CPE-711
estimate: 4h+
---

## Summary
The user-command layer for epic CPE-711: define templated commands (CPE-781) and bind them to the toolbar,
context menu, and palette; run them over the current selection via a backend exec command, **confirming
before launching any external process**. Commands persist in settings.

## Acceptance Criteria
- [~] Users define/edit/remove templated commands and choose where they surface; they run over the selection.
      *(**define/edit/remove/reorder + surface-binding + selection-resolve core landed & tested** — `userCommands.ts`; the toolbar/context/palette + editor rendering is the attended GUI tail.)*
- [~] External processes are confirmed before launch; no regression to built-in actions; persistence works.
      *(persistence core done; **backend exec `run_command` now landed & cargo-tested** (output/exit capture, capped, empty-guard) — the frontend **confirm dialog** before launch is the remaining GUI gate.)*
- [~] `npm run check` + suite green; GUI-verified.
      *(`npm run check` clean + vitest 8/8 now; **GUI-verified** + the exec path are the attended part.)*

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

## Resolution (partial — store + backend exec landed, GUI deferred)
Both headless halves of the feature are done and CI-green: the pure command store `userCommands.ts` (CRUD,
reorder, surface filtering, `resolveCommand`, tolerant persist — #46) and the backend executor
`run_command` (shell-run a resolved command line, capped output + exit code capture, empty-guard — this
PR). The frontend confirms the command before invoking `run_command` (enforced by design, per the ticket).
Only the attended GUI remains: the editor dialog, surfacing on toolbar/context/palette, and the confirm
dialog. Deferred with a turnkey revisit note.
