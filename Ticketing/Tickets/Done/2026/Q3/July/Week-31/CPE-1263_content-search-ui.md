---
id: CPE-1263
title: "File-content search UI (query box + ranked results with snippets, wired to content_search)"
type: feature
component: frontend
priority: medium
status: Done
tags: ready
created: 2026-08-02
closed: 2026-08-02
epic: CPE-976
---

## Summary
Frontend for CPE-976 file-content search. Depends on CPE-1262 (the `content_index_build` + `content_search` commands).
Give the user a way to search files by what's INSIDE them and jump to a hit. Local embedder → works offline, no key.

## Build
- A content-search surface (reuse the existing search/panel conventions — prefer an inline instant control over a modal,
  per house style). Elements: a query input; a "build/refresh index for this folder" action with streamed progress
  (subscribe to `content_index_build`'s channel — show files-indexed count / a progress bar, per STREAMING.md); a ranked
  results list showing filename, relative path, a score indicator, and the snippet; click a result → navigate to its
  folder + select the file (reuse existing navigate/select).
- Empty/needs-build state: if no index exists for the folder, prompt to build it (don't show a raw error).
- Route all backend calls through `src/lib/invoke.ts` (busy-cursor wrapper) except the streamed build (use the streaming
  pattern like other bulk producers). Debounce the query; supersede in-flight searches by generation token.
- Follow UI conventions: pills/chips reflow (tick-tacks), light-theme palette vars only, menu/tab standards if any are added.

## Acceptance criteria
- Typing a query returns ranked content hits with snippets; clicking navigates+selects the file.
- Build-index shows live streamed progress and doesn't block the UI (busy cursor / progress).
- Component unit tests (jsdom, backend mocked) cover: query→results render, empty/needs-build state, navigate-on-click,
  debounce/generation-token supersede. `npm run check` clean.
- A `gui-smoke` render pin (seed a mocked/seeded result set via a test-mode hook if needed) OR at minimum a jsdom test
  pinning the render — match how sibling panels are pinned; note the MVD/burndown row if any human-visual residual remains.
- Docs (CPE-579): add/extend a content-search doc page in `src/docs/*.md` + its `sectionDocs.ts` entry; guard test green.

## Notes
Honest framing in UI copy: "search file contents" (embedder-pluggable), not overpromising "AI semantic". Visual Critic +
(if user-facing interaction feel) an attended pass may be needed — screenshot-judge first, minimize user involvement.

## Work Log
- 2026-08-02 — Worker (sonnet, worktree) built `src/lib/components/ContentIndexSearchDialog.svelte`, a
  new overlay modelled closely on `InstantSearch.svelte` (the closest existing sibling: off-means-off
  build affordance, debounce, generation-token supersede, streamed build progress) rather than the
  older `ContentSearchDialog.svelte` (grep-based "Search in files", CPE-417 — a name already taken by a
  different, line-level engine). Query input debounces 250ms; a shared `gen` counter supersedes both the
  opening index-existence probe and any in-flight search. **Index-existence detection:** rather than a
  separate status command (none exists), the dialog probes with a cheap `content_search(root, "", 0)`
  on mount — confirmed safe/cheap by CPE-1262's own
  `empty_query_or_zero_k_yields_no_hits_but_index_exists_is_still_true` test — and reads the
  `index_exists` flag off the `ContentSearchOutcome`. **Wiring split per BUSY-CURSOR.md/STREAMING.md:**
  `content_search` goes through the typed `commands.contentSearch` client (already routed to
  `src/lib/invoke.ts`'s busy-cursor `invoke` via `bindings.gen.ts`'s `TAURI_INVOKE` import — confirmed by
  reading the generated file's tail, not assumed); `content_index_build` uses `rawInvoke` +
  `createChannel` directly (self-progress, opts out of the busy cursor, mirroring `InstantSearch`'s
  `index_build` call). Results show filename (`baseName`), a folder-relative path (new pure helper
  `relativeToRoot` in `src/lib/contentSearch.ts`, cross-platform + case-insensitive prefix strip), a
  score bar + percentage (new pure helper `scorePercent`, clamped 0–100 since `SemanticIndex::search`
  only returns positive cosine scores), and a highlighted snippet (reused `highlightSegments`). A
  "Rebuild index" button in the header is always available once an index exists (files can change without
  the index knowing — it isn't kept live). Wired into `App.svelte`: state `contentIndexSearchOpen`,
  command-palette entry `tool.contentIndexSearch` ("Search file contents…", `enabled: inFolder`, no free
  global keyboard shortcut was available — all sensible Ctrl combos are already bound), `on:navigate`
  reuses the existing `revealFileInApp` (same contract as the other 3 search dialogs), `on:help` opens
  `12-search` (extended with a new "Search file contents" section documenting the honest
  local/offline/embedder-pluggable framing, the build-first flow, and what the score/snippet mean).
  **i18n:** added 12 new `en` keys (`search.byContent*`, `search.*ContentIndex`,
  `palette.contentIndexSearch`) — since ALL 12 catalogs (`es de fr it pt nl pl ru zh ja ko` +`en`) are
  listed in `COMPLETE_LOCALES` and the CPE-481 coverage gate holds every one of them to 100%, translated
  all 12 keys into all 11 non-English locales by hand (not machine-guessed placeholders) so
  `i18n.test.ts`'s coverage gate stays green — verified (34/34 pass).
  Tests: `src/lib/contentSearch.test.ts` gained `relativeToRoot`/`scorePercent` cases (21/21 pass, up from
  15). New `src/lib/components/ContentIndexSearchDialog.test.ts` (11/11 pass, mocking
  `@tauri-apps/api/core` per the `InstantSearch.test.ts` precedent): needs-build prompt (not a raw
  error) when `index_exists:false`; no `content_search` fired while typing in that state; streamed build
  progress renders live + unlocks the query once done; build error surfaced; ranked hits render
  name+relative-path+score%+snippet (snippet assertion reads `.snippet` container text directly rather
  than `getByText` on the highlighted fragment, since a `<mark>`-split text node can't be matched by a
  plain string/regex matcher — same documented caveat as `InstantSearch.test.ts`); clean no-matches
  state; click-to-navigate dispatches the file path + closes (asserted via the row's `title`, not
  `getByText`, since a root-level file's name and relative-path spans can coincide); Escape closes;
  debounce coalesces rapid keystrokes into one search; a stale search's late-arriving result is dropped
  once a newer one has superseded it; clearing the query cancels the pending search.
  **No `gui-smoke` spec** — this dialog only opens via the command palette (no free global shortcut), so
  a spec would need to drive `Ctrl+Shift+P` → type → Enter rather than a direct key combo like
  `instant-search.smoke.ts`; logged as a residual row (CPE-1263) in
  `.claude/qa-architecture/MANUAL-TEST-BURNDOWN.md` — interaction logic is jsdom-pinned, but real render/
  pixel/score-bar-fill "feel" on the installed build still wants a human glance before that flips to
  render-automated.
  Verify: `npm install` (node_modules was stale — 2 pre-existing `TerminalPanel.svelte` xterm errors
  cleared) then `npm run check` → 0 errors, 0 warnings. `npx vitest run` (full suite) → **164 files /
  1861 tests pass**, no regressions. `npx vitest run src/lib/sectionDocs.test.ts` → 2/2 pass (no new
  `Section` added — content search is a cross-cutting palette dialog like `ContentSearchDialog`/
  `FileNameSearchDialog`, not a sidebar view, so it reuses the existing `12-search` doc slug rather than
  earning a new `sectionDocs.ts` entry). `npx vitest run src/lib/i18n.test.ts` → 34/34 pass (coverage
  gate green across all 12 complete locales). No backend/Cargo change; `bindings.gen.ts` untouched.
  Assumptions: (1) treated the command palette as an acceptable "inline instant control" entry point,
  matching house style — the dialog itself IS the instant/live-search surface (debounced type-ahead, no
  Apply/OK step), same shape as the 3 existing search overlays, rather than a literal dropdown; (2) chose
  a new component name (`ContentIndexSearchDialog`) instead of reusing `ContentSearchDialog` because that
  name is already the line-grep "Search in files" feature (CPE-417) — a different backend engine — and
  overloading it would have been confusing; (3) K (result cap) = 25, debounce = 250ms — no ticket-specified
  values, chosen close to `InstantSearch`'s precedent (limit 200 for a cross-volume index vs. a
  single-folder index here; debounce 150ms there vs. 250ms here since embedding+scoring is heavier work
  than a name-prefix scan).
