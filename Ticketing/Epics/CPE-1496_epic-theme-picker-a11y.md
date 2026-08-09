---
id: CPE-1496
title: "EPIC: Theme picker + built-in themes + high-contrast / a11y"
type: Task
status: Proposed
priority: Medium
component: Frontend
tags: [epic]
created: 2026-08-08
closed:
---

> **Filed 2026-08-08 (sprint PM, theme-engine research pass).** Dormant brief — decompose on
> `/ticketing-epic activate CPE-1496`. **Depends on CPE-1492 + CPE-1493 (CPE-1494/1495 optional). Epic #5 of 5.**

## Why
The user-facing surface of the theme engine + the **accessibility must-haves**. High-contrast / reduce-transparency
are NOT optional polish — they belong here, and this epic's a11y half has **no hard dependency on the accent /
materials epics**, so it can be pulled forward if accessibility is prioritized.

## Scope
- **Settings → Appearance** picker: named built-in themes (Light / Dark / System, + maybe one or two accent
  presets), manual override always available (never "frozen in launch mode").
- **High-contrast variant(s)** as an additional `data-theme` value (`hc-light`/`hc-dark`) or a
  `data-contrast="high"` modifier, layered the same way as light/dark, driven by the OS signal with live update:
  - Windows: `SystemParametersInfo(SPI_GETHIGHCONTRAST)` (`HCF_HIGHCONTRASTON`).
  - macOS: `NSWorkspace.accessibilityDisplayShouldIncreaseContrast` / `...ShouldReduceTransparency` +
    `accessibilityDisplayOptionsDidChangeNotification`.
  - Linux: the `org.freedesktop.appearance` portal `contrast` key.
- Keep the picker **minimal** — NO theme marketplace / import-arbitrary-theme (that would be maximalist and cut
  against fast/small/predictable). Built-in themes only.

## Verify
`npm run check`; a11y pass (contrast ratios meet WCAG in each built-in theme; keyboard-reachable picker;
high-contrast actually engages from the OS signal). The Accessibility Auditor leg reviews it.

## Notes
~90% frontend + one small Rust high-contrast/portal-contrast reader (mirrors CPE-1494's per-OS pattern).
Accessibility is not optional. Ship docs per CPE-579.
