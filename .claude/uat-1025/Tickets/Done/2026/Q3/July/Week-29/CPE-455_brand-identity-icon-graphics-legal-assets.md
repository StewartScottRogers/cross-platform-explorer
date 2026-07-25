---
id: CPE-455
title: "Brand identity — unique app icon, graphics, and legally-clean asset kit"
type: Feature
status: Done
priority: Medium
component: Packaging
tags: [ready]
estimate: 2-3h
created: 2026-07-15
closed: 2026-07-15
---

## Summary
Give the app a unique, brandable identity: an original app icon + logo/graphics, plus a `brand/`
folder holding every asset and its documentation with a clear legal-rights guarantee, then use the
mark liberally across forms (UIs) and documentation.

## Acceptance Criteria
- [x] A `brand/` folder with original vector assets (icon, logo, monochrome mark) authored from
      scratch (geometric SVG only — no third-party art/fonts), so rights are unencumbered.
- [x] Brand guide (`brand/BRANDING.md`): concept, palette (hex), usage, do/don'ts.
- [x] Legal provenance (`brand/LICENSE.md`): original-work statement guaranteeing the project's rights
      to all branding, safe to trademark.
- [x] App icons regenerated from the master (all platforms) via `tauri icon`.
- [x] Mark used on forms + docs: README header, the AI Console launcher header.

## Work Log
2026-07-15 — Picked up. Original geometric SVG (compass sparkle + aperture/eye + orbital ring +
agent dot) → rasterize via ImageMagick → tauri icon. Legal cleanliness via zero third-party assets.

## Resolution
Created `brand/` — the durable brand kit: original geometric SVG assets (`icon.svg` app tile, `logo.svg` mark+wordmark, `mark-mono.svg`), a brand guide (`BRANDING.md`: 'Compass Aperture' concept, Indigo #4F46E5 / Cyan #06B6D4 palette, typography, usage), and a legal provenance/rights doc (`LICENSE.md`) guaranteeing the project owns all branding — everything authored from primitive vector geometry with **zero** third-party art or embedded fonts, so it's trademark-safe. Regenerated **all** platform icons from the master via `tauri icon brand/icon.svg` (desktop ico/icns/png + Android/iOS). Used the mark on forms + docs: README header logo + a brand link, and a brand header bar in the AI Console launcher (inline, CSP-safe SVG). Launcher jsdom suite still 19 green. Icon-regen command is documented in BRANDING.md for future refreshes.
