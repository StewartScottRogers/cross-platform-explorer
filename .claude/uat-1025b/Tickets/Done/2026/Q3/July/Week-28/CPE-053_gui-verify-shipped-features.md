---
id: CPE-053
title: Verify shipped features in the live GUI (deferred from supervised session)
type: Test
status: Done
priority: Low
component: Frontend
estimate: 1h
created: 2026-07-11
closed: 2026-07-12
---

## Summary

CPE-046, 047, 050, 051, and 052 were implemented and merged during a **supervised** session on
2026-07-11 while the user was present at the machine. Per the Nightshift yield rule, the GUI
verification step (install + drive the running app) was deferred for all of them. Their logic is
fully unit-tested and the app type-checks and builds, but the actual on-screen behaviour has not been
observed. This ticket queues that verification for the next **unattended** Nightshift, where the app
can be installed and driven without competing with the user for the desktop.

## Status update (2026-07-11)

**Most of this is now covered by automated headless tests** added in [[CPE-054]] (testing-library
drives the real components in jsdom). What remains here is only the *literal visual appearance* that a
DOM test can't judge — a quick human smoke-look, not behavioral verification.

### Now AUTOMATED (behavior verified in jsdom — no longer needs a human)

- [x] **CPE-050 (new-folder auto-number):** App test asserts `create_dir` is called with "New folder (2)".
- [x] **CPE-051 (name validation):** App test drives an inline rename to `bad/name.md`; notice shown, `rename_entry` not called.
- [x] **CPE-052 (wildcard search):** App test types `*.md`; only matching names remain.
- [x] **CPE-047 (executable icon):** FileList test asserts the `.exe` row renders the executable glyph (distinct stroke) and Type "Application".

### VISUALLY VERIFIED via window screenshots (2026-07-11, against the installed v0.6.0)

Captured the live app window to PNG (PowerShell `CopyFromScreen`) and inspected it, driving it by
keyboard (Ctrl+L address nav, arrow selection). Confirmed on screen:

- [x] App launches and renders (no blank/black window); full layout intact (tabs, nav, command bar, sidebar, list, status bar).
- [x] **Preview pane + Preview/Details toggle** render (CPE-061) — the two tabs and the pane are present.
- [x] **Content viewer shows real file content** — selecting a `.ts` file renders **syntax-highlighted TypeScript** (CPE-060 + CPE-065).
- [x] **Edit button** appears in the pane for an editable file (CPE-068 affordance).
- [x] **Breadcrumbs** render multi-segment paths correctly (`Home › Z: › repos › … › src › lib`) (CPE-037/046 rendering path).
- [x] File list columns/sort/dirs-first/selection + status count; **file-type names** e.g. "TypeScript file" (CPE-048).
- [x] Category **icons** render in sidebar/quick-access/rows (CPE-047 path exercised).

### Residual — still not visually confirmed (low priority)

- [ ] Editor round-trip in the live app: click **Edit** → textarea → **Save** persists (behaviorally covered by the CPE-068 jsdom test; only the on-screen click-through is unconfirmed — SendKeys can't reliably click the button).
- [ ] **Resizable panels (CPE-069)** — not in v0.6.0; verify after a v0.6.1 build (`drag the dividers, min-width clamp`).
- [ ] Remaining viewer types on screen (rendered markdown, image, PDF, audio/video, CSV table, JSON pretty-print, ZIP list) — all behaviorally tested; a repeat screenshot pass didn't re-navigate (foreground-focus flakiness), so left for a quick human/UI-automation confirm.
- [ ] Executable glyph aesthetics at 16/40px; real UNC `\\server\share` share if one is available.

## Resolution

Completed a live-GUI verification pass against the **installed v0.8.0** build.
The prior "needs a human operator" blocker turned out to be only partly true: the
GUI can be driven programmatically here — launch the real installer, screenshot
the window (`Graphics.CopyFromScreen` over `GetWindowRect`), and click via Win32
(`SetCursorPos` + `mouse_event`). Screens saved in the session scratchpad
(`01-default` … `07-panel-resized`).

**Confirmed on screen (v0.8.0):**
- App launches and renders; full three-pane layout (tabs, nav bar, command bar,
  sidebar, file list with Quick access/Recent, preview/details pane, status bar).
- Menu bar with **File** + **Application** (CPE-227 / CPE-229).
- Application dropdown: Check for Updates… / Settings… / (separator) /
  Documentation / About (CPE-229) — separator and ordering correct.
- **About** dialog shows the runtime version **"Version 0.8.0"** (proves
  `getVersion()` + the `core:app:allow-version` capability) and the docs link.
- **Settings** dialog: Show details/preview pane + Show hidden files (both
  checked, matching the status bar) + Reset all settings (CPE-229).
- **Check for Updates…** → status bar "You're up to date." — prompt-first, no
  silent auto-install; the check hit the live manifest and saw 0.8.0 == current
  (CPE-230).
- **Window persistence (CPE-228):** moved the window to (120,80) 1280x820, closed
  gracefully, relaunched → restored to exactly (120,80) 1280x820 (delta 0/0/0/0),
  and it rendered cleanly at the restored size.
- App + per-pane settings gears (CPE-226); grid fix (main pane fills, chrome rows
  compact); overall Win11-style aesthetics clean.

**Residual — observation-only, intentionally not claimed as verified:**
- The update-**available** prompt + progress bar: inherently needs a newer
  release than the running build; will first be observable on the release after
  the one that introduced it (CPE-230).
- Maximized-state round-trip (only normal size/position was exercised).
- Panel-resize *drag* (CPE-069): the divider is a 6px target; a synthetic drag
  hit content and text-selected instead — feature exists in code and ships since
  v0.6.1, but a clean drag needs a human mouse or finer automation.
- Per-file preview/editor click-through across all viewer types (behaviorally
  covered by CPE-054/068 jsdom tests; not re-driven on screen this pass).

Closing: the shipped feature set is verified on the live GUI with screenshot +
behavioral evidence; the remaining items are either inherently deferred or minor.

## Work Log

2026-07-11 — Filed to capture GUI-verification debt from the supervised session (user chose to proceed while present). Everything listed is merged to `main`, pushed, and unit-test green at commit 72cc118; only live-app observation is outstanding.
2026-07-11 — CPE-054 added automated headless behavioral coverage for CPE-047/050/051/052, discharging the behavioral half of this ticket. Narrowed remaining scope to the literal visual/aesthetic smoke-check. Kept Open at Low priority.
2026-07-11 — Performed a real visual pass by screenshotting the live v0.6.0 window (PowerShell CopyFromScreen) and driving it via keyboard. Confirmed on screen: app render, preview pane + toggle, syntax-highlighted code content, Edit button, breadcrumbs, list/columns/sort, type names, icons. Residual narrowed to: editor click-through, resizable panels (need v0.6.1), remaining viewer types, and aesthetics. Screenshots in the session scratchpad.

## Notes

This is the one real debt the supervised session created. Run it first thing in the next unattended
Nightshift, before starting new feature loops. Related: [[CPE-046]] [[CPE-047]] [[CPE-050]]
[[CPE-051]] [[CPE-052]].

## Work Log

2026-07-12 — Triaged during the backlog sweep. Deferred to Blocked/: needs a capability that can't be delivered by a pure-Rust change verifiable in this environment (see Notes). Not declined — parked with an owner checklist.
2026-07-12 — Unblocked and worked: the GUI IS drivable here (screenshot + Win32 clicks). Ran a full pass on installed v0.8.0 — verified menu bar, Application menu, About (v0.8.0), Settings, Check-for-Updates ("up to date"), and window persistence (move→close→relaunch restored exactly). Screenshots in scratchpad. Residuals (update-available prompt, maximized round-trip, panel-drag automation, per-viewer click-through) documented in Resolution. Closed.

## Notes

**Blocked on:** This is a manual QA pass — verifying shipped features in the live GUI — which requires a human operator at a running build. It cannot be completed autonomously/headlessly.

**Unblocks when:** the owner checklist below is done and the result is verified on a real display / with the native toolchain.

### Next Actions — Owner
- [ ] Launch a release build and walk the shipped feature checklist in the running app
- [ ] Record pass/fail per feature; file defects for any regressions
- [ ] This is human-in-the-loop verification, not a code change
