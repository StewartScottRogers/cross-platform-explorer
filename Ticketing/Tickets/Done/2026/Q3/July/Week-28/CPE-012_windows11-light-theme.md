---
id: CPE-012
title: Windows 11 light theme, typography, and design tokens
type: Feature
status: Done
priority: High
component: Frontend
estimate: 1-2h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

The app is dark with a generic system font. The reference is Windows 11 Explorer: light surfaces,
Segoe UI, subtle borders, rounded corners, blue accent, thin scrollbars.

## Acceptance Criteria

- [x] Light theme tokens replace the dark palette (surface, subtle, border, accent, text, text-dim)
- [x] Segoe UI Variable / Segoe UI font stack
- [x] Rounded corners and hairline borders matching Win11
- [x] Thin custom scrollbars
- [x] Selection/hover states use the Win11 blue accent

## Resolution

Replaced the dark palette with Windows 11 light tokens in `src/app.css`: `--bg #f3f3f3` (mica-ish
window), `--surface #ffffff`, hairline `--border #e5e5e5`, `--accent #0067c0`, and a `--selection
#e5f0fb` row fill matching Explorer's selected-row blue. Font stack is
`"Segoe UI Variable Text", "Segoe UI", system-ui` at 13px, with a 30px row height to match Explorer's
density. Added thin Win11-style scrollbars (10px, rounded, transparent track) via both
`scrollbar-width` and `::-webkit-scrollbar`. Global button reset gives consistent hover/active/
focus-visible states.

## Work Log

2026-07-11 — Picked up. Rewrote app.css around Win11 light tokens.
2026-07-11 — Matched Explorer's density (30px rows, 13px Segoe UI) and selection blue.
2026-07-11 — svelte-check 0/0. Closing as Done.

## Notes
