---
id: CPE-1279
title: "Release polish: fix multi-draft race in release-sidecar + fill the eject glyph"
type: chore
component: build
priority: low
status: Backlog
tags: ready
created: 2026-08-03
---

## Summary
Two minor items observed 2026-08-03:
1. **Multi-draft release race:** `release-sidecar.yml`'s 3 matrix legs (win/linux/mac) each ran tauri-action
   concurrently and created SEPARATE draft releases for the same tag (v0.57.47-sidecar had TWO drafts: one macOS-only,
   one with Linux+Windows). `gh release download`/`view` by tag then hit the wrong draft. Fix: coordinate the legs onto
   ONE release (e.g. a pre-create-release job the legs upload to, or serialize/`needs:` the release creation, or a
   post-job that merges assets into a single draft). Intermittent (prior builds landed on one release).
2. **Eject glyph cosmetic (from CPE-1278 review):** the new `eject` glyph in Icon.svelte is an outline (parent svg
   fill="none") among the fixed-color icons; render it as the conventional solid eject symbol.

## Acceptance criteria
- A release-sidecar run produces exactly ONE draft with all OS assets.
- Eject glyph renders solid/conventional.
