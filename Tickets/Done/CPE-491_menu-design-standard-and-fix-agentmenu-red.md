---
id: CPE-491
title: "Menu design standard + fix the red AgentMenu (consistent menus everywhere)"
type: Task
status: Done
priority: Medium
component: Frontend
tags: [ready]
estimate: 1h
created: 2026-07-16
closed: 2026-07-16
---

## Summary
The new per-session close menu (CPE-489) rendered its items in **red**, whereas the app's menus are
normally **black** — the file right-click `ContextMenu` renders every item (even Delete) in
`var(--text)`. Root cause: `AgentMenu` was the only menu using a hard-coded `#d05656` for "danger"
text, and there was **no shared rule** keeping menus consistent. Fix the red and write the standard so
it can't recur — cross-platform, theme-aware.

## Acceptance Criteria
- [x] `AgentMenu` items use the theme's `var(--text)` (no red, no hard-coded colour); hover matches the
      canonical menu (the global `button:hover → var(--hover)`).
- [x] A written **menu standard** documents the rules for all popup menus (container, items, hover,
      separators, hints, destructive-action policy, behaviour, cross-platform/theme).
- [x] The standard is referenced from `CLAUDE.md` so new menus follow it.

## Resolution
- **Fix:** `AgentMenu.svelte` — dropped the `danger` class + the `#d05656` rule and the bespoke
  `.row:hover`; items now inherit `var(--text)` and get hover from the global `button:hover`, so the
  menu matches `ContextMenu`/`TabMenu`. Destructive intent is conveyed by wording ("Close …"), not
  colour (matching the app, where Delete isn't red either).
- **Standard:** new `docs/design/MENUS.md` — the single source of truth for every popup menu
  (`ContextMenu`, `AgentMenu`, `TabMenu`, `MenuBar`/`CommandBar` dropdowns, and the console's
  `launcher.html` menus). Core rules: custom-rendered (identical on Windows/macOS/Linux); colour only
  from theme variables (never hard-coded hex → adapts light/dark + per-OS); item text always
  `var(--text)`; no red menu text for destructive actions (red belongs only on a `ConfirmDialog`
  primary button); Escape/click-outside close; open-at-cursor + clamp-to-viewport; i18n labels. The
  console webview follows the same rules via CSS **system colors** since `app.css` isn't available there.
- `CLAUDE.md` gains a "UI conventions" note pointing at the standard.

`npm run check` clean; AgentMenu tests pass. Files: `src/lib/components/AgentMenu.svelte`,
`docs/design/MENUS.md` (new), `CLAUDE.md`.

## Notes
Follow-up (optional, not required for consistency now that the red is gone): extract the duplicated
`.ctx`/`.row`/`.sep` CSS into shared global classes so the menu components share one implementation
rather than parallel copies. Filed as an idea, not scheduled.
