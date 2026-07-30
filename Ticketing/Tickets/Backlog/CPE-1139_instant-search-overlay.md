---
id: CPE-1139
title: "Instant index: keyboard-first global search overlay (streamed results)"
type: feature
component: Frontend
priority: high
status: Backlog
tags: ready
created: 2026-07-29
epic: CPE-703
blocked-by: CPE-1137
---

## Summary
The user-facing half of epic CPE-703: a keyboard-first **global search overlay** that calls the streamed
`index_search` command (CPE-1137) and shows cross-volume filename matches **as you type** (<100ms warm),
independent of the current folder — the "Everything-style instant find" the epic promises. GUI feature;
verified via build → deploy → run.

## Design
- **Invocation.** A global shortcut opens the overlay (pick a free binding — e.g. `Ctrl+Shift+P` is taken by
  the command palette; use something like `Ctrl+Shift+F` / `Ctrl+K` if free — check existing shortcuts in
  `App.svelte`/the command palette first and decide-and-log). A centered overlay with a single text input +
  a results list, keyboard-first (↑/↓ to move, Enter to reveal/open, Esc to close). Follow the app's dialog
  conventions (visible thin border per [[dialogs-need-visible-border]]; theme vars only).
- **Streamed, superseding results.** Debounce input; call the streamed `index_search` over an `ipc::Channel`;
  append batches; flip `loading` off on the first batch; **supersede** an in-flight query by generation token
  when the user types more (per `docs/design/STREAMING.md` and [[prefer-streaming-liveness]]). Use the
  busy-cursor `invoke` wrapper from `src/lib/invoke.ts` (or `rawInvoke` + allowlist if it renders its own
  progress) per [[diagnostics-mode-instrument-os-calls]]/BUSY-CURSOR.md.
- **Result rows.** Show filename (highlight the matched span), the containing path (dimmed), and an icon;
  Enter reveals the file in the explorer (navigate to its folder + select it) — reuse the existing
  reveal/navigate path. Keyboard selection follows the menu/list conventions.
- **Index-not-ready affordance.** If `index_status` shows no resident index, the overlay shows a clear
  "Instant search is off — Build index" action (calls `index_build` with streamed progress) rather than a
  blank list. This is the opt-in surface for the **off-means-off** mode; nothing indexes until the user asks.
- **Empty/error/zero-result states** are explicit (not a silent blank).

## Acceptance Criteria
- [ ] A global shortcut opens a keyboard-first overlay; typing streams ranked cross-folder matches; ↑/↓ + Enter
      reveal/open the selected file; Esc closes. Superseding keystrokes cancel stale result streams.
- [ ] With no index built, the overlay shows a "Build index" affordance (streamed progress), not a blank list;
      after building, search works — and with the mode never enabled, nothing was indexed (off-means-off).
- [ ] Matches the dialog/menu UI conventions (visible border, theme vars, reflowing pills if any) and uses the
      busy-cursor `invoke` wrapper.
- [ ] `npm run check` green; a jsdom/component test covers the overlay's core logic (debounce + supersede +
      keyboard nav + build-affordance state), backend mocked.
- [ ] GUI-verified on the real build (build → deploy → run): typing finds files on other drives/folders
      instantly; live edits (with CPE-1138) reflect without a manual rescan.

## Notes
- Depends on CPE-1137 (the `index_search`/`index_build`/`index_status` bindings). Best built after CPE-1137
  merges so `bindings.gen.ts` exists.
- Pairs with CPE-1138 (live watcher) for the "stays current" half; both land before the epic's GUI-verify pass.
- Closing the epic CPE-703 DoD: <100ms warm cross-volume matches; stays current without rescan; zero cost when
  off.
