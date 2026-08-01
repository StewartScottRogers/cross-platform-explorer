---
id: CPE-1216
title: "Spotlight overlay component (sectioned, highlighted results) + item feed + frecency"
type: feature
component: Frontend
priority: medium
status: Done
tags: ready
created: 2026-08-01
epic: CPE-704
---

## Summary
Part of CPE-704 — the frontend spotlight (folds in CPE-1216 overlay + CPE-1217 item feed + CPE-1218 frecency
into ONE worker since all touch App.svelte). Backed by the CPE-1214 commands.

## Build
- **Overlay** `src/lib/components/Spotlight.svelte` modeled on `CommandPalette.svelte` (theme vars, ↑/↓/Enter/Esc,
  visible border): renders **sectioned** results (Action→Folder→File→Recent) with **matched-position
  highlighting** (from `SpotResult.positions`). Opened by the `spotlight:open` event AND an in-app trigger (so
  it's verifiable without the OS hotkey). First slice renders in the main window.
- **Item feed** `src/lib/spotlightSources.ts` (pure, jsdom-testable): recents (`history.recentPaths`), drives
  (`listDrives`), favorites, action labels (`paletteCommands`), file/folder hits (`find_files_by_name`,
  streamed per [[prefer-streaming-liveness]]) → `sources: [ResultKind, string[]][]` → `spotlight_search`.
- **Frecency** `src/lib/spotlightFrecency.ts` store (`{path,count,last_used_s}`, settings.ts pattern):
  increment on open/reveal; empty query → `spotlight_frecent` default view; activation ordering.
- Activate: open/reveal file, run action. `invoke`/commands via the busy-tracked path.

## Acceptance Criteria
- [x] jsdom tests for spotlightSources (kind tagging, caps) + frecency store (increment/decay). gui-smoke
      `spotlight.smoke.ts`: open, type, assert ranked+highlighted sectioned rows, Enter activates; default view
      shows most-frecent first. `npm run check` + `npm test` green.

## Work Log
- 2026-08-01 — Filed by Foreman (workshift, epic CPE-704). Consolidates 1216/1217/1218 (shared App.svelte).
  Depends on CPE-1214.
- 2026-08-01 — Done. Built all three consolidated pieces against CPE-1214's `spotlightSearch`/`spotlightFrecent`
  bindings:

  - **Overlay** `src/lib/components/Spotlight.svelte` — modeled on `CommandPalette.svelte`'s chrome (theme
    vars, visible `--border-strong` border, ↑/↓/Enter/Esc) but sectioned like `InstantSearch.svelte`: renders
    one `.sp-section` per non-empty `SpotSection` (Action→Folder→File→Recent, the order `aggregate` already
    returns), each row highlighting `SpotResult.positions` via the new `highlightByPositions` helper
    (`<mark class="sp-hl">` per matched-character run — handles non-contiguous subsequence matches, not just
    substrings). Debounced (150ms) search; a first `spotlight_search` call resolves the action/folder/recent
    sections immediately, then `streamFileHits` streams the file section in over
    `find_files_by_name_stream` so a big folder never blocks first paint
    ([[prefer-streaming-liveness]]). Empty query shows the frecency default view. Enter on an "action" row
    runs the `Command.run()` directly (looked up by label from `paletteCommands`); Enter on
    folder/file/recent dispatches an `activate` event `{path, kind}` for the host to reveal/navigate, and
    records a frecency visit.
  - **Item feed** `src/lib/spotlightSources.ts` (pure, jsdom-tested) — `actionSource`/`folderSource`/
    `recentSource`/`fileSource` build the kind-tagged, capped (`SOURCE_CAPS`) candidate lists; `buildSources`
    assembles the full `[ResultKind, string[]][]` for `spotlight_search`, dropping empty sources.
    `streamFileHits(root, query, onBatch, cap)` mirrors `FileNameSearchDialog.svelte`'s
    `find_files_by_name_stream` usage (`rawInvoke` + `createChannel`, not the busy-cursor path — a
    self-progress operation). `highlightByPositions` is the char-position → highlight-segment splitter the
    overlay renders.
  - **Frecency** `src/lib/spotlightFrecency.ts` — `recordVisit`/`parseFrecent`/`serializeFrecent` mirror
    `colorRulesStore.ts`'s pure-store + tolerant-parse pattern; `defaultView(visits, nowS, limit)` calls the
    real `spotlight_frecent` command and wraps the ranked paths as one "recent" `SpotSection` so the overlay
    renders it through the exact same row component. The store overflow-prunes the stalest entries past
    `MAX_FRECENT_ENTRIES` (300) — a lightweight decay. Persisted via a new `settings.ts` KEY
    (`cpe.spotlightFrecency`) + `loadSpotlightFrecency`/`saveSpotlightFrecency`, following the existing
    per-domain-module + settings.ts-owns-the-key convention.
  - **App.svelte wiring** (kept minimal — CPE-1215's worker owns `src-tauri/*` + Settings for the OS hotkey):
    a `spotlight:open` Tauri-event listener (mirrors the existing `open-docs`/`transfer://done` listeners,
    torn down in `onDestroy`) plus a new **in-app trigger** — a "Spotlight (search everywhere)…" Command
    Palette entry (`tool.spotlight`) — so the overlay is reachable, and gui-smoke-testable, without the OS
    hotkey. `onSpotlightActivate` routes a "file" activation through the existing `revealFileInApp` and
    everything else through `navigateToTyped`.
  - i18n: added `spotlight.*`/`palette.spotlight` keys to all locales `COMPLETE_LOCALES` requires
    (en/es/de/fr/it/pt/nl/pl/ru/zh/ja/ko) — the CPE-539 100%-coverage gate in `i18n.test.ts` catches a
    locale falling short, and did until all twelve were filled in.

  Tests added (all synchronous, no backgrounded verification):
  - `src/lib/spotlightSources.test.ts` — 14 tests: action-source enabled-filter + declaration order, folder
    de-dup + directory-only favorites, recent delegation to `history.recentPaths`, file-source mapping/caps,
    `buildSources` dropping empty sources, `streamFileHits`'s no-op guards + capped-batch accumulation +
    best-effort-on-failure, and `highlightByPositions` (no-positions passthrough, non-contiguous split
    matching `spotlight.rs`'s own doc example, fully-matched-as-one-run).
  - `src/lib/spotlightFrecency.test.ts` — 9 tests: `recordVisit` create/increment/purity, the
    `MAX_FRECENT_ENTRIES` stalest-first eviction ("decay"), `parseFrecent`/`serializeFrecent` round-trip +
    tolerance, and `defaultView`'s short-circuit/backend-ranking/empty-result paths.
  - `src/lib/components/Spotlight.test.ts` — 8 tests (mocking `@tauri-apps/api/core` the same way
    `InstantSearch.test.ts` does): empty-query type-hint vs. frecency default view, debounced
    `spotlight_search` calls with the exact built sources, sectioned+highlighted rendering, the file-stream
    only starting after the first slice resolves, Enter activating folder/file/recent rows (records
    frecency + dispatches `activate`) vs. running an action row directly (no event, no frecency),
    ArrowDown navigation, and Escape closing without activating.
  - `gui-smoke/specs/spotlight.smoke.ts` — drives the real built app: opens the overlay via the Command
    Palette's "Spotlight (search everywhere)…" entry, types "marker" (matching the harness's seeded
    `CPE-1045-marker.txt` fixture), asserts a `.sp-row` renders under a "Files" `.sp-section-label` with a
    `<mark class="sp-hl">` highlighting part of the query — exercising the REAL streamed
    `find_files_by_name_stream` walk and the REAL `spotlight_search` command, not a stub — then Enter closes
    the overlay. `snap("spotlight")` / `snapFailure` per CPE-1149.

  Verification (all synchronous):
  - `npm run check`: 0 errors, 0 warnings.
  - `npm test` (root vitest): 146 files / 1618 tests passed, 0 failed (includes the 31 new Spotlight tests
    and the i18n locale-coverage gate, which required the es/de/fr/it/pt/nl/pl/ru/zh/ja/ko translations
    above).
  - `cd gui-smoke && npm install && npm run typecheck`: clean (0 errors) — `gui-smoke/node_modules` wasn't
    installed in this worktree; installed it to typecheck the new spec (gitignored, not committed). The live
    WebdriverIO run is CI's job, per the ticket.

  No backend/Settings changes — CPE-1215 (the OS-level global hotkey + `spotlight:open` emit) is a separate,
  parallel ticket; this overlay is fully wired to consume that event the moment it lands, and is independently
  usable today via the in-app Command Palette trigger.
