---
id: CPE-1494
title: "EPIC: Native accent-color adoption (Windows / macOS / Linux) — the 'feels native' signal"
type: Task
status: Proposed
priority: Medium
component: Multiple
tags: [epic]
created: 2026-08-08
closed:
---

> **Filed 2026-08-08 (sprint PM, theme-engine research pass).** Dormant brief — decompose on
> `/ticketing-epic activate CPE-1494`. **Depends on CPE-1492 + CPE-1493. Epic #3 of 5.**

## Why
Adopting the **OS accent color** is the single highest-leverage, lowest-controversy "this app respects my
Mac/Windows" signal. No Tauri/webview API surfaces accent today (Tauri #8590 open) — this is the **one piece of
genuinely custom per-OS Rust** in the whole theme program, but it's isolated behind a single command.

## Scope
- A Rust command (`src-tauri`) that reads the OS accent color per platform (`#[cfg]`-gated), pushed to the
  frontend + kept live where the OS supports change events:
  - **Windows**: `UISettings::GetColorValue(UIColorType::Accent)` via the `windows` crate (correctly resolves
    "Automatic Accent Color"); `DwmGetColorizationColor` is the simpler fallback.
  - **macOS**: `NSColor.controlAccentColor` via `objc2-app-kit` (the objc2 dependency pattern already exists in
    the repo's transparent-titlebar recipe).
  - **Linux**: the `org.freedesktop.appearance` portal `accent-color` key over D-Bus (subscribable live; unset
    when out-of-range → fall back to the palette default).
- Frontend consumer: patch `--accent` / `--accent-hover` / `--selection` at runtime from the returned color;
  the user can still override with a picked built-in accent (ties into CPE-1496).

## Verify
Rust unit tests where feasible per platform (mock/`#[cfg]`); manual verify the accent tracks a live OS accent
change on each OS (attended, cross-OS — a QA-burndown item). `cargo clippy --all-targets -D warnings`.

## Notes
Backend-heavy (3 platform reads + 1 D-Bus subscription), isolated behind one command — does NOT touch the
fast-path explorer code. Highest native-feel payoff per unit effort. Ship docs per CPE-579.
