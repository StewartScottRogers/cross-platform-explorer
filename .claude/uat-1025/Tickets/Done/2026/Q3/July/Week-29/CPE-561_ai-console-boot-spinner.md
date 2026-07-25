---
id: CPE-561
title: "AI Console — replace the ugly boot progress-cursor with a pretty animated loader"
type: Feature
status: Done
priority: Medium
component: Frontend
tags: [ready]
estimate: 45m
created: 2026-07-17
closed: 2026-07-17
---

## Summary
User feedback (2026-07-17): the OS `progress` cursor added for the AI Console boot (CPE-552) "looks bad."
Replace it with a polished, on-brand **animated loading indicator** that clearly demonstrates liveness and
looks nice — a smooth spinner + label overlay shown from first paint until the launcher is interactive,
fading out when ready.

## Acceptance Criteria
- [x] A centered boot overlay with a smooth animated spinner (brand indigo→cyan) + a "Starting…" label
      shows from first paint until the launcher `load()` resolves.
- [x] It fades out (not a hard cut) on ready — success or failure — via `endBoot()`.
- [x] The old `body.booting` progress-cursor rule is removed (the overlay is the indicator now); a normal
      cursor over the overlay.
- [x] Respects `prefers-reduced-motion` (slower, non-jarring).
- [x] Launcher jsdom harness asserts the overlay + spinner ship and that `endBoot()` dismisses it.

## Resolution
Replaced the CPE-552 `body.booting` progress-cursor with a polished boot overlay in `launcher.html`: a
full-screen `#boot-overlay` (theme `Canvas` bg, `cursor:default`) centering a **conic-gradient ring
spinner** (brand `#4F46E5`→`#06B6D4`, radial-masked to a ring, `boot-spin` 0.9s linear) + a softly-pulsing
"Starting the AI Console…" label. Shows from first paint (static markup); `endBoot()` adds `.done`
(opacity→0, `.4s` fade) on ready — success or failure — then removes it after 450ms so it never intercepts
clicks. Honors `prefers-reduced-motion` (slow ring, no label pulse). CSP-safe (self-contained CSS, no
assets). Removed the old progress-cursor rule. Launcher jsdom harness updated (58 pass): asserts the
overlay/spinner/label ship, the old cursor rule is gone, and `endBoot()` fades+dismisses. `npm run check`
0/0.

## Notes (visual — needs a live look)
Being a pretty/visual change it can only be truly judged in the running app; unit tests cover the wiring.
A fresh sidecar release + install will show it (batched with the other nightshift frontend fixes).

## Notes
Launcher-only (`launcher.html`), self-contained CSS animation (CSP-safe, no assets). Reuses the brand
gradient already in the header logo. Supersedes CPE-552's cursor approach.
