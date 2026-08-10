---
id: CPE-1555
title: "Navigation Mode: mode-indicator badge + cheatsheet dialog + docs page"
type: Feature
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-1487
created: 2026-08-10
---
## Context
CPE-1487's brief calls for "a clear on-screen mode indicator" and "a discoverable cheatsheet" alongside
the modal layer itself, plus a docs page per CLAUDE.md's CPE-579 self-maintaining-docs rule (every
user-facing section ships/updates a `src/docs/*.md` page and a `Section → slug` entry in
`src/lib/sectionDocs.ts`, enforced by the guard test `src/lib/sectionDocs.test.ts`). This ticket ships
the UI-facing, standalone pieces of that: a small badge that will show the current mode once mounted, a
cheatsheet dialog listing the modal bindings, and the doc page — all as new files the caller (CPE-1556)
mounts, not wired into `App.svelte` here.

## Scope
- New file `src/lib/components/NavModeIndicator.svelte`:
  - Prop: `mode: NavMode` (import type from `./navMode`, CPE-1552).
  - A small fixed badge (e.g. bottom-right corner of the active pane) showing `"NORMAL"` / `"VISUAL"`
    text. Colors from existing semantic tokens only — `var(--accent)` background, `var(--text)` (or
    whatever contrasting token `CommandBar.svelte`/`StatusBar.svelte` already use for a filled badge —
    match that pattern) foreground. **No new CSS custom properties** — reusing existing tokens means no
    `[data-theme="light"|"dark"|"hc-light"|"hc-dark"]` block edits are needed in `src/app.css`; confirm
    this holds (contrast-check the chosen existing token pair against all four blocks before finalizing,
    per the theme-tokens convention) rather than inventing a new token pair for this one badge.
- New file `src/lib/components/NavCheatsheet.svelte`:
  - A small dialog (own file, not an edit to the existing `src/lib/components/ShortcutsDialog.svelte`,
    which is a shared file this batch avoids touching concurrently) listing the Navigation Mode bindings:
    `h j k l` move, `gg` / `G` top/bottom, `v` visual-range select, `d` / `y` / `p` cut/copy/paste
    selection, `/` filter, `:` command line, `Esc` exit visual mode. Follows the visible-border dialog
    convention (thin themed border, not just a shadow) and `docs/design/MENUS.md`-equivalent styling
    consistency (theme-variable colors only). Opens/closes via props (`open: boolean`, `on:close`) — the
    caller decides when to show it.
- New file `src/docs/37-navigation-mode.md`: user-facing doc — what Navigation Mode is, how to enable it
  (Settings → Navigation Mode, off by default), the full binding list (same content as the cheatsheet,
  written for the docs library), and how to exit back to normal mouse/keyboard use (`Esc`, or simply
  toggling the Settings switch off).
- `src/lib/sectionDocs.ts`: add `"navigation-mode"` to the `Section` union (~line 35, alongside
  `"keyboard-shortcuts"`) and `"navigation-mode": "37-navigation-mode"` to the `SECTION_DOC` map
  (~line 132).
- No `App.svelte`, `ShortcutsDialog.svelte`, or `src/app.css` edits.

## How
New `src/lib/components/NavModeIndicator.test.ts` (jsdom): renders `"NORMAL"` for `mode="normal"` and
`"VISUAL"` for `mode="visual"`, no other DOM assertions needed. New `src/lib/components/
NavCheatsheet.test.ts` (jsdom): renders all seven binding rows when `open=true`, renders nothing when
`open=false`, fires `close` on the dialog's close affordance (mirroring how other dialogs' close-button
tests are written, e.g. `src/lib/components/ShortcutsDialog.test.ts` if present). Extend
`src/lib/sectionDocs.test.ts` implicitly passes once the new `Section` entry and doc file both exist (the
guard test enumerates all `Section` values and all `DOCS` slugs — no new test code needed here beyond
running it).

## Verify
`npx vitest run src/lib/components/NavModeIndicator.test.ts src/lib/components/NavCheatsheet.test.ts
src/lib/sectionDocs.test.ts`; `npm run check`. Fully headless — jsdom component tests plus a static guard
test, no Tauri invoke, unreachable from the running app until CPE-1556 mounts these components.

## Notes
**Conflict surface:** three new files (`NavModeIndicator.svelte` + test, `NavCheatsheet.svelte` + test,
`src/docs/37-navigation-mode.md`) plus one additive entry-pair in `src/lib/sectionDocs.ts` (only this
ticket touches that file in the batch — no collision with the other CPE-579 doc work elsewhere in the
repo since slug `37` is currently free). No overlap with CPE-1553 or CPE-1554's files. **Dispatch order:**
after CPE-1552 (for the `NavMode` type import in `NavModeIndicator.svelte`); independent of, and
mergeable in parallel with, CPE-1553 and CPE-1554.
