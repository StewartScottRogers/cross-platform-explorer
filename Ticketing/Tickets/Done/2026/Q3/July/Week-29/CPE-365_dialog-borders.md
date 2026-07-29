---
id: CPE-365
title: "Dialogs need a clearly-visible border to differentiate from underlying content"
type: Bug
status: Done
closed: 2026-07-14
priority: Medium
component: Frontend
created: 2026-07-14
---

## Summary

Dialogs/overlays could blend into the form behind them — the borders were too faint (explorer
dialogs: `--border-strong` #d6d6d6 on white; AI Console launcher panels: `--line` at 26% alpha),
leaving only a drop shadow. A dialog can then look like part of what it overlays. Give every
dialog a visibly-thin border.

## Fix
- `app.css`: strengthen `--border-strong` (the emphasis/dialog-root border token) so all explorer
  dialogs/menus (all key off it) get a defined edge.
- `launcher.html`: the credential/onboarding panels use a stronger border than the faint `--line`.

## Acceptance
- Every dialog reads as a distinct surface over the content behind it. `npm run check` + build green.

2026-07-14 — Fixed on branch `CPE-365-dialog-borders`.
- `app.css`: `--border-strong` #d6d6d6 → #b3b3b3. Every explorer dialog/menu roots its border on
  this token (ConfirmDialog, Settings, About, Shortcuts, Properties, BatchRename, PatternSelect,
  ContextMenu, TabMenu, ConsentSheet, Update), so all get a clearly-visible edge at once.
- `launcher.html`: `#keys-panel` and `#onboard-panel` borders raised from the faint `var(--line)`
  to `rgba(128,128,128,.55)`.
- `npm run check` 0 errors; `npm run build` ok; sidecar rebuilds (embeds launcher). Recorded the
  standing rule in memory (`dialogs-need-visible-border`).
