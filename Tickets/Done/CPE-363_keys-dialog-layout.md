---
id: CPE-363
title: "Keys… dialog renders too small / cramped after the label field (CPE-348)"
type: Bug
status: Done
closed: 2026-07-14
priority: Medium
component: Frontend
created: 2026-07-14
---

## Summary

Adding the credential **label** field (CPE-348) turned the Keys form into a 5-column grid
inside a fixed 460px panel; the key input is squished and the layout overflows (grid items
don't shrink without `min-width:0`). Restructure the form into robust rows and widen the panel.

## Fix
- `launcher.html`: two-row form (provider + label; then key + Check + Save), flex with
  `min-width:0` so nothing overflows; widen `#keys-panel`.

## Acceptance
- The Keys dialog lays out cleanly at its default size; provider/label/key and buttons all fit.

2026-07-14 — Fixed on branch `CPE-363-keys-dialog-layout`.
- `launcher.html`: the Keys form is now a two-row flex layout (provider + label; then key +
  Check + Save) with `min-width:0` on the controls so nothing overflows; `#keys-panel` widened
  to 560px. `cargo build` (embeds launcher) clean.
