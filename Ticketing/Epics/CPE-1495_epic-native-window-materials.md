---
id: CPE-1495
title: "EPIC: Native window materials + title-bar theming (Mica / vibrancy) — opt-in, default OFF"
type: Task
status: Proposed
priority: Low
component: Multiple
tags: [epic]
created: 2026-08-08
closed:
---

> **Filed 2026-08-08 (sprint PM, theme-engine research pass).** Dormant brief — decompose on
> `/ticketing-epic activate CPE-1495`. **Depends on CPE-1492 + CPE-1493. Epic #4 of 5.**

## Why + the PURPOSE.md tension (read before scoping)
Native window materials (Windows **Mica/Acrylic**, macOS **vibrancy**, immersive dark title bar) are the most
"looks native" polish — but they carry **real compositing overhead** and are the clearest "adds capability over
stays small" item in the theme program. Per PURPOSE.md's fast/small/predictable tiebreaker, ship this
**opt-in, default OFF**, and treat it as **the first epic to cut** if the crew is time-boxed. This is Low
priority for that reason.

## Scope
- `window-vibrancy` crate (tauri-apps): `apply_mica`/`apply_acrylic` (Win10/11), `apply_vibrancy` (macOS
  10.10+). Requires `"windows": [{ "transparent": true }]` + `macOSPrivateApi: true` in `tauri.conf.json` and
  `html, body { background: transparent }` — behind a **Settings toggle, off by default**.
- Immersive dark title bar on Win11 (`DWMWA_USE_IMMERSIVE_DARK_MODE` — may follow `set_theme(Dark)`
  automatically; verify empirically); transparent/overlay titlebar on macOS (`TitleBarStyle` — first-class in
  Tauri v2).
- Linux blur/vibrancy is compositor-controlled → **out of scope** for a first pass.

## Verify
Per-OS attended visual check (materials render, no perf regression when ON; app is unchanged when OFF — the
default). Confirm the transparent-background requirement doesn't break any existing surface. Performance Guard
should measure cold-start + memory delta with materials ON vs OFF.

## Notes
Backend-heavy (`window-vibrancy` + `#[cfg]` + tauri.conf transparency flags). The scrutiny epic of the five —
keep it strictly opt-in. Ship docs per CPE-579.
