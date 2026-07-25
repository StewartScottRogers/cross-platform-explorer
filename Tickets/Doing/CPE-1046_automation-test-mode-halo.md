---
id: CPE-1046
title: "Automation test-mode — halo overlay + off-screen launch so the user never interferes with a GUI test"
type: feature
component: Multiple
priority: high
status: Doing
tags: ready
created: 2026-07-25
epic: CPE-616
estimate: 3h
---

## Summary
When automated GUI tests (or GUI verification) drive the real app, its window sits on the user's shared
screen — they don't know if it's "theirs" or the machine's, so they either freeze up or risk clicking and
breaking the test. This makes automated GUI runs **impossible to interfere with**, on two layers:

1. **Halo (Layer 2, app feature):** a `--test-mode` launch flag renders an **unmistakable overlay** — a
   thick, pulsing halo border around the whole window + a fixed "🤖 AUTOMATED TEST — please don't touch"
   banner — so any *visible* test window screams "hands off."
2. **Off-screen (Layer 1, convention):** automated runs launch **off the interactive screen** via the
   existing `--x/--y` geometry flags, so normally there's nothing on the user's screen to interfere with at
   all. Documented convention; the CPE-1045 GUI-smoke harness adopts it.

Delivered via the **same init-script mechanism as `--open`** (CPE-1044): a global set before the app's
scripts run, no command/gate.

## Design (buildable)

### Backend — `src-tauri`
- `tauri.conf.json` → `plugins.cli.args`: add `{ "name": "test-mode", "takesValue": false, "description":
  "Show the automation test-mode halo overlay (CPE-1046)" }` beside `open`.
- In `run()`'s `setup()`, next to the `--open` resolution (~`lib.rs:5758`), read the flag via `CliExt`
  (`matches.args.get("test-mode")` → `Value::Bool(true)`). **Extend the existing
  `initialization_script`** so a single injected script sets both `window.__CPE_OPEN_DIR__` (when present)
  and `window.__CPE_TEST_MODE__ = true` (when the flag is set). Keep one combined script string built from
  whatever is present; inject only if non-empty.

### Frontend — `src`
- New `src/lib/components/TestModeOverlay.svelte`:
  - Full-viewport `position: fixed; inset: 0; pointer-events: none; z-index: <max>` frame.
  - A **thick pulsing halo border** (e.g. `box-shadow: inset 0 0 0 6px <accent>` + an animated glow via
    `@keyframes`), in a loud amber/red that stands out in **both** light and dark (this overlay is
    *supposed* to be jarring — do not blend into the theme).
  - A fixed **banner** (top-center): `🤖 AUTOMATED TEST IN PROGRESS — please don't touch this window` with
    a warning glyph, high contrast.
  - `pointer-events: none` on everything so it can never block the automation's clicks or the user closing
    the window. Text hardcoded **English** (a test/dev overlay, not localized product chrome — deliberately
    avoids the 12-locale i18n coverage gate).
- `App.svelte`: read `window.__CPE_TEST_MODE__` once at init into a `testMode` boolean; render
  `{#if testMode}<TestModeOverlay/>{/if}` at top level. Zero cost when off (never rendered).

### Convention — off-screen automation (Layer 1)
- Document in the CPE-1045 GUI-smoke harness README + `.claude/qa-architecture/README.md`: **automated GUI
  runs launch off-screen + in test-mode** — e.g. `--test-mode --x -4000 --open <tmpdir>`. WebDriver drives
  the DOM regardless of window position, so it works fully while never appearing in front of the user.
- No code needed here beyond the flag; it's a launch convention. (A dedicated virtual-desktop is a noted
  follow-up, not v1 — off-screen is simpler and just as effective.)

## Acceptance Criteria
- [ ] Launching a bundled build with `--test-mode` shows the halo + banner overlay; without it, no overlay
      (normal app unchanged). Verified by GUI screenshot (on-screen) — the overlay is unmissable and
      non-interactive (you can still click through / close the window).
- [ ] `--test-mode --x -4000` launches off-screen and the automation can still drive it (no visible window
      in front of the user).
- [ ] Overlay is `pointer-events:none`, above all app chrome, and loud in both light + dark themes.
- [ ] Vitest for `TestModeOverlay`: renders on the global, hidden without it, is non-interactive, contains
      the warning text. Backend flag read compiles; clippy clean both modes; `npm run check` +
      `npx vitest run` green; i18n gate untouched (hardcoded English).

## Work Log
2026-07-25 (attended) — Filed at user request (approved the two-layer plan: off-screen isolation + an
obvious halo). Rides the `--open` init-script delivery (CPE-1044) and pairs with the GUI-smoke harness
(CPE-1045). Both touch `lib.rs` setup — serialize merges.

## REVISED (user feedback, 2026-07-25) — hard requirements
The user saw an automated test window pop into the **middle** of the screen and **capture their mouse +
keyboard**. That is unacceptable. Revised design:
1. **Not a full-screen frame — a small upper-LEFT corner badge**, sized only as big as its content and no
   bigger (`position: fixed; top: 8px; left: 8px; width: max-content`). No overlay across the center.
2. **Never captures input:** overlay root + children `pointer-events: none` (click-through); no focusable
   elements / no autofocus / `tabindex="-1"` — must not steal keyboard focus.
3. **Window must not steal OS focus:** in `--test-mode`, launch the main Tauri window **non-focused /
   non-activating** (`WebviewWindowBuilder::focused(false)`) so creating it never grabs the user's mouse or
   keyboard from their active app. This — plus off-screen launch for automation — is the real "can't
   interfere" fix; the badge is only the visual cue.
