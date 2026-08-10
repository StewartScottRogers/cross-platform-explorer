---
id: CPE-1556
title: "Navigation Mode: wire the modal layer into App.svelte (single opt-in-gated integration point)"
type: Feature
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-1487
created: 2026-08-10
---
## Context
CPE-1552 through CPE-1555 land the whole Navigation Mode feature as inert, independently-tested new
files: the mode-state reducer + Settings toggle (1552), the motion→selection bridge (1553), the `:`
command-line bridge (1554), and the mode indicator + cheatsheet + docs (1555). None of them touch
`src/App.svelte`. This ticket is the single integration point that makes the feature reachable — and it
is deliberately sequenced **last** and scoped to be the **only** ticket in this batch touching
`src/App.svelte`'s 7300+ lines, avoiding concurrent edits to that shared file (same reasoning CPE-1547
used to keep `handleKeydown` migration out of the hotkey-customization foundation ticket).

Activation model: Navigation Mode has no separate "enter mode" keypress. When the Settings toggle
(`navigationModeEnabled`, from CPE-1552) is on and the file list has keyboard focus (no rename in
progress, no confirm dialog, no quick-look), the modal layer is always live and starts in `"normal"` mode
— matching how the surveyed vim TUIs (ranger, lf, nnn) behave: the pane is always modal when the app has
focus, there's no separate on/off keystroke to remember. Flipping the Settings switch off is the escape
hatch back to plain-explorer keyboard behavior. This keeps the change a single boolean-gated branch with
**zero behavior change when off** (the default).

## Scope
- `src/App.svelte`:
  - Import `initialNavState`, `reduceNavKey`, `NavState` from `./lib/navMode`; `applyNavIntent` from
    `./lib/navMotion`; `NavModeIndicator` from `./lib/components/NavModeIndicator.svelte`;
    `NavCommandLine` from `./lib/components/NavCommandLine.svelte`; `NavCheatsheet` from
    `./lib/components/NavCheatsheet.svelte`; `settings.loadNavigationModeEnabled`.
  - Reactive/local state: `let navigationModeEnabled = settings.loadNavigationModeEnabled();` (re-read
    when `SettingsDialog` closes, matching how other toggles already refresh); `let navState: NavState =
    initialNavState();` reset to `initialNavState()` on tab switch / active-pane switch so mode/pending
    buffers never leak across panes; `let navCommandLineOpen = false;` `let navCheatsheetOpen = false;`.
  - New small helper `function dispatchNavIntent(intent: NavIntent, inPaneB: boolean)` (co-located near
    `doCopy`/`doCut`/`doPaste`/`doDelete`, ~line 3170-4180) mapping intents to the **existing** functions
    only — no new file-op logic:
    - `motion` → `selectionForPane = applyNavIntent(intent, selectionForPane, itemCount, navState.mode,
      layout)` (reuses CPE-1553's bridge against whichever `Selection` the active pane already tracks).
    - `op: "yank"` → `doCopy(inPaneB)`; `op: "cut"`... — wait, CPE-1487's brief maps `d` to cut (matching
      vim's delete-into-register semantics, i.e. cut) and `y` to copy: `op: "delete"` → `doCut(inPaneB)`;
      `op: "yank"` → `doCopy(inPaneB)`; `op: "paste"` → `doPaste(inPaneB)` (async — `await` it, matching
      the existing `Ctrl+V` branch's handling).
    - `startFilter` → focus the pane's existing filter/search entry point (reuse whatever `Ctrl+F`
      already triggers — do not build a second filter UI).
    - `startCommand` → `navCommandLineOpen = true`.
    - `enterVisual`/`exitVisual`/`none` → no side effect beyond the state update already applied by
      `reduceNavKey` in the caller.
  - In `handleKeydown` (`src/App.svelte:4955`), add **one** new early guard placed immediately after the
    existing quick-look guards (after line ~4985, before the `ctrl`/`pane`/`inPaneB` computation at
    ~4987-4992): `if (navigationModeEnabled && !renamingPath && !renamingPathB && !confirm && !quickLook &&
    !mediaQuickLook && !navCommandLineOpen) { const { state, intent } = reduceNavKey(navState, event.key);
    navState = state; if (intent.kind !== "none") { event.preventDefault(); dispatchNavIntent(intent,
    dualPane && activePane === 1); } return; }` — roughly 10-15 lines, calling only the new
    `dispatchNavIntent` helper and CPE-1552's pure `reduceNavKey`. When `navigationModeEnabled` is
    `false` (the default), this `if` short-circuits on its first condition and every line below it in
    `handleKeydown` executes exactly as it does today — **zero behavior change**.
  - Mount, conditionally on `navigationModeEnabled`, near the existing conditional-mount cluster (where
    `quickLook`/`mediaQuickLook` overlays are mounted): `<NavModeIndicator mode={navState.mode} />` when
    enabled; `<NavCommandLine commands={paletteCommands} on:run={...} on:cancel={() =>
    navCommandLineOpen = false} />` when `navCommandLineOpen`; `<NavCheatsheet open={navCheatsheetOpen}
    on:close={() => navCheatsheetOpen = false} />` bound to a discoverability affordance (e.g. a `?` key
    inside Navigation Mode, or a link from the new Settings row added in CPE-1552 — pick whichever is a
    one-line addition; do not add a new toolbar button, per the fast/small/predictable tiebreaker).
- No `keymap.ts` edit — Navigation Mode's V1 grammar stays the hardcoded table from CPE-1552 (see that
  ticket's Context note on remapping being an explicit fast-follow).

## How
Since `App.svelte` itself has no existing dedicated `handleKeydown` unit-test file (verify this is still
true before starting — grep for one first), this ticket's headless verification is: (1) `npm run check`
passes with the new imports/branch; (2) the four upstream unit suites (`navMode.test.ts`,
`navMotion.test.ts`, `navCommandLine.test.ts` + `NavCommandLine.test.ts`,
`NavModeIndicator.test.ts`/`NavCheatsheet.test.ts`, `settings.test.ts`) all still pass unmodified,
confirming the integration didn't need to change any of their contracts; (3) manually trace the new guard
against the "off by default" requirement — `navigationModeEnabled` defaults to `false` via
`loadNavigationModeEnabled`, so the new `if` is unreachable in a fresh install, and every existing branch
below it in `handleKeydown` is reached exactly as before (no lines moved, only inserted). If any headless
DOM-level integration test scaffolding already exists for `App.svelte`'s keyboard handling, add one case
there confirming a `j` keypress with the setting off falls through to today's existing arrow-key-adjacent
behavior unchanged, and confirming a `j` keypress with the setting on (and a mock `navigationModeEnabled
= true`) invokes `dispatchNavIntent` instead.

## Verify
`npx vitest run` (full suite, to catch any regression the new branch introduces elsewhere in
`App.svelte`'s tested surface); `npm run check`. Fully headless — no OS keyboard interaction, no Tauri
invoke beyond what `doCopy`/`doCut`/`doPaste`/`doDelete` already perform (unchanged call sites, just a
new caller). Visual sign-off (does the badge/cheatsheet look right on screen) can be queued async per the
sprint's lights-out constraint; it is not required to close this ticket.

## Notes
**Conflict surface:** the only ticket in this batch touching `src/App.svelte` — sequenced deliberately
last to avoid concurrent edits to that shared 7300+-line file from CPE-1553/1554/1555 landing in
parallel. Imports from all four prior tickets' new files; no edits to `src/lib/navMode.ts`,
`src/lib/navMotion.ts`, `src/lib/navCommandLine.ts`, `src/lib/commandPalette.ts`, `src/lib/selection.ts`,
or `src/lib/keymap.ts`. **Dispatch order:** last — requires CPE-1552, CPE-1553, CPE-1554, and CPE-1555 all
merged first.
