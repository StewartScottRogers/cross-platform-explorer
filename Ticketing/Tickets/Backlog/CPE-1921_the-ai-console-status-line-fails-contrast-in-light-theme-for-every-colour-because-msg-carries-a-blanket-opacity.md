---
id: CPE-1921
title: the AI Console status line fails contrast in light theme for **every** colour, because `#msg` carries a blanket `opacity: .85`
type: bug
priority: Medium
status: Open
tags: ready
estimate: S
created: 2026-08-27
---

## Summary

`sidecar/ai-console/src/launcher.html` line ~110 sets `#msg { opacity: .85 }`, which knocks **every**
status colour down regardless of hue. Measured from actually-rendered pixels (not assumed CSS
values) by the independent UAT tester on PR #1040:

| state | light theme | dark theme |
|---|---|---|
| amber `#d08a1a` @ .85 | rgb(214,155,59) on white → **2.44:1 — fails AA** | rgb(179,120,24) → 5.01:1 — passes |
| green `#3a9d4a` @ .85 | rgb(87,171,100) on white → **2.83:1 — fails AA** | rgb(52,136,65) → 4.23:1 — borderline |

WCAG AA wants 4.5:1 for normal text. **Light theme fails for both colours**, and the red is worth
measuring too.

## This is pre-existing, not a CPE-1911 regression

CPE-1911 (PR #1040) added the amber and was checked for exactly this. The amber lands in the same
range the **green has always been in**, because the cause is the shared `opacity` rule, not any one
colour. The UAT tester explicitly cleared #1040 on that basis and asked for this to be its own
ticket rather than a hold — correctly.

## Acceptance criteria

- [ ] Fix the cause, not the symptom: either drop the blanket `opacity: .85` on `#msg` and bake the
      intended softness into the colour values, or pick per-theme colours that clear 4.5:1 *after*
      the opacity is applied. Do not fix one colour and leave the others.
- [ ] Measure **all three** states (green / amber / red) in **both** themes, from rendered pixels
      rather than from the CSS source — the opacity is exactly why source values lie here. The UAT
      run on #1040 documents the method (`chrome.exe --headless=new --screenshot` against the real
      `launcher.html`, sample the painted pixels).
- [ ] Keep the three states visually distinct from each other after the fix. A palette where every
      state clears contrast but amber now reads as red trades one defect for another.
- [ ] Pin it. There is a WCAG guard test in this repo for the main app's palette; establish whether
      it covers the sidecar's `launcher.html` at all (it likely does not — this is a separate HTML
      file in a sidecar crate), and either extend it or add an equivalent guard. Note CPE-1919 found
      the main app's guard missing a 3.70:1 body-text pairing, so "there is a guard" is not the same
      as "it would catch this".

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1040's independent UAT measurements. Related:
**CPE-1911** (the amber that occasioned the measurement), **CPE-1919** (the main app's dark-theme
JSON string values at 3.70:1, and its guard's blind spot).
