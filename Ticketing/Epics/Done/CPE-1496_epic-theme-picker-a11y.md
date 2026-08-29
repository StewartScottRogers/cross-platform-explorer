---
id: CPE-1496
title: "EPIC: Theme picker + built-in themes + high-contrast / a11y"
type: Task
status: Done
priority: Medium
component: Frontend
tags: [epic]
created: 2026-08-08
closed: 2026-08-29
---

> **Filed 2026-08-08 (sprint PM, theme-engine research pass).** Activated 2026-08-09 (sprint PM, bench
> refill) — decomposed into child tickets below. **Depends on CPE-1492 + CPE-1493 (both shipped
> 2026-08-09 — System/Light/Dark theme picker already live in Settings → Appearance). CPE-1494/1495
> remain dormant/not required. Epic #5 of 5.**
>
> **Scope note:** the "named built-in themes (Light/Dark/System) picker" half of this epic's brief
> already shipped under CPE-1493 (CPE-1536/CPE-1541: the `Theme` select in Settings → Appearance). This
> activation covers the epic's **remaining, still-unshipped half**: the high-contrast a11y variant the
> epic brief calls out as having "no hard dependency on the accent/materials epics" — new `hc-light`/
> `hc-dark` palettes, the `ContrastSetting` plumbing, a Settings control for it, and a headless one-shot
> OS high-contrast signal read. No new "marketplace" theme picker is being built (the brief explicitly
> rules that out as maximalist) — the existing Theme select is the picker.

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

## Child tickets (activated 2026-08-09, sprint PM bench refill)
1. **CPE-1543** — Author `hc-light`/`hc-dark` Layer-1/Layer-2 palettes in `app.css` (a stricter,
   WCAG-AAA-inspired contrast guard than the normal light/dark palettes) + the guard test. Inert alone
   (nothing sets `data-theme="hc-*"` yet). *(independent; parallel with 1544)*
2. **CPE-1544** — `types.ts`/`settings.ts`/`theme.ts`: add `ContrastSetting` ("system"/"off"/"high"),
   `resolveContrast`, and widen `applyTheme` to compose `hc-${base}` theme values. Backward-compatible
   default args — every existing call site keeps working unchanged. *(independent; parallel with 1543)*
3. **CPE-1545** — Settings → Appearance: add a `Contrast` control (Off/System/High) next to the
   existing `Theme` select + refresh `35-appearance.md`. *(prereq: 1544)*
4. **CPE-1546** — One-shot OS high-contrast signal read (Windows `SPI_GETHIGHCONTRAST` / macOS
   `NSWorkspace` / Linux `org.freedesktop.appearance` portal) feeding `ContrastSetting`'s `"system"`
   value at boot. Deliberately scoped to a one-shot query, not a live subscription — live OS-toggle
   tracking is a follow-on, kept out to stay honestly headless. *(prereq: 1544)*

Dispatch order: {1543 ∥ 1544} → {1545 ∥ 1546}. Same "aesthetic pass is an attended, async, non-blocking
sign-off; WCAG contrast math is the binding headless gate" pattern CPE-1539 established for the dark
palette applies here too — CPE-1543 lands on contrast-test-green and queues the visual pass. CPE-1546's
Linux branch is the one deliberate exception to the "no new deps" convention (a pure-Rust one-shot D-Bus
portal read), justified the same way CPE-1494's own brief accepts it for the identical accent-color
portal read; CPE-1494/1495 remain dormant, unpicked this sprint (native accent needs a live per-OS
subscription like the one just deferred here; window materials/vibrancy is inherently attended/visual,
declined for lights-out work per the sprint brief).

## Closed 2026-08-29

Closed 2026-08-29 (closeout audit) WITH TWO RESIDUALS. All 4 children Done.

Verified: high-contrast palettes are authored for both `hc-light` and `hc-dark` with their own contrast guards. Contrast is a **genuinely orthogonal axis** (`ContrastSetting = system|off|high`, composed as `hc-${base}`), not a fourth theme. The OS signal is real on all three platforms - Windows `SPI_GETHIGHCONTRAST`, macOS `accessibilityDisplayShouldIncreaseContrast`, Linux a timeout-bounded zbus read of `org.freedesktop.appearance/contrast` that fails to false. "Picker stays minimal, no marketplace" holds: two selects are the whole surface.

RESIDUAL 1 - the OS high-contrast read is **one-shot at boot**. Flipping Windows high contrast while the app runs does nothing until relaunch. Deliberately descoped in CPE-1546 and **honestly stated in `35-appearance.md`**, so it is disclosed rather than silently broken.
RESIDUAL 2 - macOS `accessibilityDisplayShouldReduceTransparency` and its change notification are named in Scope and are read nowhere.
