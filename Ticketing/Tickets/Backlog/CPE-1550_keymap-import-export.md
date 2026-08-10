---
id: CPE-1550
title: "Hotkeys: import / export keymap via clipboard JSON"
type: Feature
status: Backlog
priority: Low
component: Frontend
tags: [ready]
epic: CPE-1484
created: 2026-08-10
---
## Context
The epic's Goal also calls for "import/export of a keymap" (share a customized keymap between machines,
or back one up before experimenting). Rather than a native file picker + filesystem write — new backend
surface per CLAUDE.md's "new domain logic goes in cpe-server," disproportionate for exchanging a few
hundred bytes of JSON — this follows the app's existing copy/paste-JSON pattern already used for macro
export/import: `MacrosDialog.svelte` does `await navigator.clipboard.writeText(json)` to export
(`src/lib/components/MacrosDialog.svelte:203`) and a pasted-textarea + button to import
(`importPasted`, same file ~line 210); `TemplatesDialog.svelte` and `PreviewPane.svelte`'s clipboard
paste-import follow the same `navigator.clipboard` calls directly, no Tauri plugin, no capability entry
needed. This ticket is pure client-side logic (no backend command) since, unlike macros, a keymap has no
server-side storage to round-trip through.

## Scope
- `src/lib/keymap.ts` (CPE-1547): add two exported functions —
  - `exportKeymap(keymap: Keymap): string` → `JSON.stringify({ version: 1, bindings: keymap }, null, 2)`.
  - `importKeymap(json: string, base: Keymap = defaultKeymap()): { keymap: Keymap; applied: ActionId[];
    rejected: string[] }` — parses `json`, validates each entry the same tolerant way `parseKeymap` does
    (known `ActionId` + a chord that normalizes cleanly or `""`), but additionally reports which action
    ids were actually applied vs. rejected (bad JSON shape, unknown/renamed id, un-normalizable chord) so
    the UI can show the user what happened. Chord **conflicts** are allowed through on import (not
    blocked) — they surface the next time the dialog's `findConflicts` runs, same as any other edit path.
- `src/lib/components/KeyboardBindingsDialog.svelte` (CPE-1548/1549): add an "Export / Import" disclosure
  at the bottom of the dialog — a read-only `<textarea>` pre-filled with `exportKeymap(keymap)` plus a
  "Copy to clipboard" button (`navigator.clipboard.writeText`, mirroring `MacrosDialog.svelte`'s
  `exportOne`), and a separate empty `<textarea>` + "Import" button that runs `importKeymap` on its
  contents, calls `saveKeymap` with the resulting `keymap`, and shows a one-line summary ("Applied 12,
  skipped 2 unrecognized") mirroring `MacrosDialog.svelte`'s `note`/`error` pattern.

## How
Extend `keymap.test.ts` with `exportKeymap`/`importKeymap` round-trip (export then import reproduces the
same `Keymap`) and tolerant-rejection cases (malformed JSON, non-object JSON, unknown action id,
un-normalizable chord — each reported in `rejected`, not thrown). Extend `KeyboardBindingsDialog.test.ts`
with an export case (textarea shows current `exportKeymap` output; "Copy to clipboard" calls
`navigator.clipboard.writeText` — mocked in the test harness, same as `MacrosDialog.test.ts`'s existing
`expect(navigator.clipboard.writeText).toHaveBeenCalledWith(...)` pattern) and an import case (pasting
valid JSON then clicking Import calls `saveKeymap` and shows the applied/skipped summary).

## Verify
`npx vitest run src/lib/keymap.test.ts src/lib/components/KeyboardBindingsDialog.test.ts`; `npm run
check`. Fully headless; clipboard interaction is mocked in tests (same harness `MacrosDialog.test.ts`
already uses) — never exercised against the real OS clipboard.

## Notes
**Conflict surface:** small, additive addition to `keymap.ts` (two new exported functions) plus a small
addition to `KeyboardBindingsDialog.svelte` (one new disclosure section appended to the existing markup).
No new dependencies — `navigator.clipboard` is already used throughout the app (`App.svelte`,
`MacrosDialog.svelte`, `TemplatesDialog.svelte`, `PreviewPane.svelte`, etc.) with no
`@tauri-apps/plugin-*` addition and no capability-file entry, since this exchanges text via the clipboard
rather than a filesystem path (the native-Browse-picker convention doesn't apply here — no path is being
chosen). No `src/App.svelte`, `src/lib/settings.ts`, `src/lib/sectionDocs.ts`, `src/lib/theme.ts`, or
`src/app.css` touches. **Dispatch order:** last — after CPE-1547 (for `exportKeymap`/`importKeymap`) and
CPE-1549 (edits the same dialog file CPE-1549 already extended with capture/reset/conflict UI).
