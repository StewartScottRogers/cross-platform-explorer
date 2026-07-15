# Cross-Platform Explorer — Brand Kit

This folder is the single source of truth for the app's identity. Everything here is **original
vector art**, authored from scratch for this project (see [`LICENSE.md`](LICENSE.md) for the legal
guarantee). Use it liberally across UIs, installers, docs, and the website.

## The mark — "Compass Aperture"

A four-point **compass sparkle** (Explore) with an **aperture / eye** at its centre and an **orbital
ring** carrying a single accent dot (Agent Watch — the app watching an AI agent move through the
filesystem). It reads as *navigate + observe*, and its symmetry nods to *cross-platform*. It is
purely geometric, so it stays crisp from 16 px to a billboard and is trademark-safe.

| Asset | File | Use |
|-------|------|-----|
| App icon (master) | [`icon.svg`](icon.svg) | The 1024² tile → all platform icons (`tauri icon`) |
| Horizontal logo | [`logo.svg`](logo.svg) | Mark + wordmark for READMEs, sites, headers |
| Monochrome mark | [`mark-mono.svg`](mark-mono.svg) | One-colour stamp/favicon/watermark (`currentColor`) |

## Palette

| Role | Name | Hex |
|------|------|-----|
| Primary | Indigo | `#4F46E5` |
| Accent | Cyan | `#06B6D4` |
| Ink (text on light) | Ink | `#0B1020` |
| Paper (bg / text on dark) | Paper | `#F8FAFC` |

The tile background is a diagonal Indigo→Cyan gradient. Keep the mark's interior white on that
gradient; on flat surfaces use the monochrome mark in Indigo (light bg) or Paper (dark bg).

## Typography

Wordmark + UI: a humanist sans — **Segoe UI / system-ui / Arial** fallback stack (no bespoke or
licensed font is required or embedded). "Cross-Platform" bold in Ink; "EXPLORER" regular, letter-
spaced, in Indigo. For print/export, convert the wordmark text to outlines (see `LICENSE.md` note).

## Usage

- **Do**: give the mark clear space (≥ 12% of its width on all sides); use the gradient tile for app
  icons; use the monochrome mark where colour is unavailable; scale proportionally.
- **Don't**: recolour outside the palette, add drop-shadows/bevels to the flat mark, stretch it,
  rotate it, or place the coloured mark on a low-contrast background.

## Regenerating platform icons

From the repo root, with a rasteriser available (ImageMagick `convert`, `rsvg-convert`, or Inkscape):

```bash
# 1. master SVG → 1024² PNG
convert -background none brand/icon.svg -resize 1024x1024 brand/icon-1024.png
# 2. all platform sizes/formats from the PNG
npm run tauri icon brand/icon-1024.png
```

`tauri icon` writes `src-tauri/icons/*` (ico/icns/png + Android/iOS). Commit those alongside any
brand change so installers pick up the new icon on the next release.
