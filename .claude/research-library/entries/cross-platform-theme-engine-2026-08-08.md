---
slug: cross-platform-theme-engine-2026-08-08
title: Cross-platform theme engine (native-conventions-respecting) — architecture + epic breakdown
tags: [product, epics, theming, dark-mode, accent, tauri, native, pm-reference]
status: current
created: 2026-08-08
---
## User directive
A theme engine that works cross-platform yet **respects each platform's native conventions** (native
light/dark, OS accent color, native window materials). Greenlit as future work; filed as 5 Proposed epics.
Resolves the theming question surfaced in [[superfile-pm-reference]] / the competitive pass. Updates the
`app-is-light-theme-only` memory (which said "update if a dark theme lands").

## Current state (verified)
`src/app.css` = ONE hardcoded `:root { color-scheme: light; ~40 semantic vars }`. **114/120 components already
consume colors only via `var(--...)`** (MENUS.md/TABS.md discipline) → theming is token-LAYERING, not a rewrite.
Zero backend theme wiring (clean slate). `SettingsDialog.svelte` has no Appearance section. `launcher.html`
uses CSS system colors (`Canvas`/`AccentColor`) that track the OS for free.

## Architecture (lean: CSS custom props + tiny runtime, NO theming framework)
- **Layer 1 palette** (raw ramps, per theme) → **Layer 2 semantic tokens** (existing names, unchanged;
  components only ever see these) → **Layer 3** runtime accent/material overrides from OS signals.
- Migrate: split single `:root` into `:root[data-theme="light"|"dark"|"hc-*"]`, keep bare `:root`=light
  fallback. Small `theme.ts`: persisted choice (default `system`) → `getCurrentWindow().theme()` +
  `onThemeChanged` → `documentElement.dataset.theme`. Mirrors VS Code's workbench→semantic token model without
  its runtime.

## "Respects native" = window chrome + OS color SIGNALS, NOT widget mimicry
Honor OS light/dark + accent + contrast + Mica/vibrancy + titlebar. Do NOT reproduce each OS's widget kit —
CPE menus are deliberately custom-rendered/identical cross-platform (MENUS.md), so this stays coherent. Default
= follow OS; always allow manual override.

## Per-OS cheat-sheet (APIs/crates)
- Light/dark: Tauri `window.theme()`/`onThemeChanged`/`set_theme` (free; macOS app-wide; Linux best-effort per
  Tauri #9427 — keep manual override; `dark-light` crate as fallback).
- Accent (the one real custom-Rust piece, no Tauri API — #8590): Win `UISettings::GetColorValue(Accent)`; mac
  `NSColor.controlAccentColor` via objc2; Linux `org.freedesktop.appearance` portal `accent-color`.
- Materials: `window-vibrancy` (Mica/Acrylic Win, vibrancy mac; needs transparent window). Linux = compositor,
  out of scope. Immersive dark titlebar: `DWMWA_USE_IMMERSIVE_DARK_MODE`.
- High-contrast: Win `SPI_GETHIGHCONTRAST`; mac `accessibilityDisplayShouldIncreaseContrast`; Linux portal
  `contrast` key.

## Epic breakdown (filed 2026-08-08, all Proposed; strict order 1→2, then 3/4/5 parallel)
- **CPE-1492 Theme-token foundation** — LOAD-BEARING prereq; layer `:root`→tokens + `theme.ts` + Settings stub.
  ~95% frontend. Clean fit.
- **CPE-1493 OS light/dark + real dark palette** — wire Tauri theme events; author the dark ramp (the bulk =
  visual QA across ~120 components). Depends 1492.
- **CPE-1494 Native accent adoption** — the 1 custom per-OS Rust command (Win/mac/Linux); highest native-feel
  payoff. Depends 1492+1493.
- **CPE-1495 Native window materials + titlebar** — Mica/vibrancy; **PURPOSE.md tension → opt-in, DEFAULT OFF,
  first to cut**. Low priority. Depends 1492+1493.
- **CPE-1496 Theme picker + built-in themes + high-contrast/a11y** — user surface + a11y musts (a11y half has
  no dep on 1494/1495, pull forward if prioritized). Keep minimal (no theme marketplace).

Purpose-fit: 1/2/3/5 clean; 4 is the scrutiny epic (opt-in/off). No heavy deps. Sources: Tauri v2 window/app
APIs, window-vibrancy, dark-light, tauri #8590/#9427, Apple NSColor, XDG appearance portal, MS Learn dark-title/
high-contrast, VS Code color-theme guide.
