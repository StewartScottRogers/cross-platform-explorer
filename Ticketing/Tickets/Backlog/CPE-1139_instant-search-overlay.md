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
- [x] A global shortcut opens a keyboard-first overlay; typing streams ranked cross-folder matches; ↑/↓ + Enter
      reveal/open the selected file; Esc closes. Superseding keystrokes cancel stale result streams.
- [x] With no index built, the overlay shows a "Build index" affordance (streamed progress), not a blank list;
      after building, search works — and with the mode never enabled, nothing was indexed (off-means-off).
- [x] Matches the dialog/menu UI conventions (visible border, theme vars, reflowing pills if any) and uses the
      busy-cursor `invoke` wrapper.
- [x] `npm run check` green; a jsdom/component test covers the overlay's core logic (debounce + supersede +
      keyboard nav + build-affordance state), backend mocked.
- [ ] GUI-verified on the real build (build → deploy → run): typing finds files on other drives/folders
      instantly; live edits (with CPE-1138) reflect without a manual rescan. **Deferred to the Foreman +
      user's build→deploy→run pass** — not runnable headlessly in this worktree.

## Notes
- Depends on CPE-1137 (the `index_search`/`index_build`/`index_status` bindings). Best built after CPE-1137
  merges so `bindings.gen.ts` exists.
- Pairs with CPE-1138 (live watcher) for the "stays current" half; both land before the epic's GUI-verify pass.
- Closing the epic CPE-703 DoD: <100ms warm cross-volume matches; stays current without rescan; zero cost when
  off.

## Work Log
2026-07-29 — Implemented on branch `cpe-1139-search-overlay`. Shortcut: **Ctrl+K** (verified free — audited
  `App.svelte`'s `handleKeydown` + the command palette's shortcut list; not gated by `inFolder` so it opens
  from the Home screen too, unlike Ctrl+P/Ctrl+Shift+F which need a folder open). Added to the command
  palette (`tool.instantSearch`) and the `?` shortcuts cheat sheet (`src/lib/shortcuts.ts`).
2026-07-29 — New component `src/lib/components/InstantSearch.svelte` + pure-logic module
  `src/lib/instantSearch.ts` (keyboard-nav wraparound, a deterministic `volumeIdForRoot` hash since
  `index_build`'s `volume_id` is caller-chosen with no backend path→id mapping, and `resolveBuildRoot`
  which reuses the existing `driveScheduler.ts` `driveRoot` helper).
2026-07-29 — Streaming: `index_search`/`index_build` go through `rawInvoke` + a `Channel` (not the
  busy-cursor `invoke`) since the overlay renders its own progress — same opt-out reasoning as
  `FileNameSearchDialog`/`BatchMediaDialog` (BUSY-CURSOR.md). `index_status` (a quick one-shot check) uses
  the typed `commands.indexStatus()` client, which does route through the busy wrapper. Supersede is a
  generation-token counter (`searchGen`) exactly per STREAMING.md/prefer-streaming-liveness: a newer
  debounced search bumps the token and resets `hits` synchronously; a channel batch from a stale token is
  dropped in `onmessage`.
2026-07-29 — Reveal mechanism: dispatches `navigate` with the hit's file path, same contract
  `FileNameSearchDialog`/`ContentSearchDialog` use; `App.svelte` wires it to the existing
  `revealFileInApp(path)` (navigates to the parent folder + selects the file) — nothing new needed there.
2026-07-29 — Build-index scope assumption: `index_build`'s root is the **whole drive** owning the current
  folder (`driveRoot(currentPath)`), not just the open folder — an instant *cross-folder* index only makes
  sense over a volume. Falls back to `homeDir`'s drive when opened from Home with no folder open. No
  drive-picker UI is in scope for this ticket (out of scope per the epic's UI-vs-engine split); a future
  ticket can add one if multi-drive indexing needs picking.
2026-07-29 — Off-means-off: `index_status()` on mount is the sole signal for whether to show results vs. the
  "Build index" affordance, per the ticket's literal design — matches AC#2. Typing while off does not call
  `index_search` (verified in the component test).
2026-07-29 — Tests: `src/lib/instantSearch.test.ts` (10 tests, pure keyboard-nav/hash/build-root logic) +
  `src/lib/components/InstantSearch.test.ts` (9 tests: debounce-to-one-call, supersede/stale-batch-drop,
  ArrowDown+Enter reveal, Escape close, build-affordance shown when `index_status` is empty, no search while
  off, build streams progress then reveals search, build error surfaces). `npm run check` — 0 errors. Full
  suite (`npx vitest run`) — 123 files / 1335 tests, all green, nothing weakened.
2026-07-29 — i18n: added `search.instant*`/`search.buildIndex`/`search.buildingIndex` and
  `palette.instantSearch` keys to all 12 `COMPLETE_LOCALES` (en/es/de/fr/it/pt/nl/pl/ru/zh/ja/ko) — the
  CPE-481 coverage gate in `i18n.test.ts` holds every declared-complete locale to 100%, so a partial add
  would have failed CI; `i18n.test.ts` passes.
2026-07-29 — Docs: added an "Instant search (Ctrl+K)" subsection to `src/docs/12-search.md` (no
  `sectionDocs.ts` change needed — it's a subsection of the existing Explorer→search doc, not a new
  `Section`, per the ticket's own note).
2026-07-29 — GUI-verify (AC#5) intentionally left unchecked: this worktree has no path to build → deploy →
  run the installed app; the Foreman/user complete that pass per the standing convention.
