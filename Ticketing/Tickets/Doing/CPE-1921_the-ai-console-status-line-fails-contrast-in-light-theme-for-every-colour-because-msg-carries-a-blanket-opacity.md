---
id: CPE-1921
title: the AI Console status line fails contrast in light theme for **every** colour, because `#msg` carries a blanket `opacity: .85`
type: bug
priority: Medium
status: In Progress
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

## Work Log

### 2026-08-27 — measured, fixed, pinned

**Method.** Every ratio below was *computed*, never estimated. Headless Chrome
(`chrome --headless=new --screenshot`) renders the **real** `launcher.html` `<style>` blocks and the
**real** `<body>` markup; the **real** `setMsg()` is lifted out of the file and executed against the
real `#msg` element; each state's element is flooded with its own computed colour (element
`opacity` composites a background fill exactly as it composites glyph cores, so this reads the
composited colour without antialiasing noise); the PNG is decoded and the centre pixel sampled by a
throwaway node script; WCAG 2.1 relative-luminance / contrast-ratio math is applied to the sampled
bytes. An independent in-page model (engine-resolved `getComputedStyle` + spec alpha compositing)
agreed with the sampled pixels to within 0.03.

**The ground, established at run time — not from the nearest CSS rule.** `#msg` is a direct child of
`body` (sibling of `.toolbar`) and paints no background of its own. Walking up from `#msg` at run
time, the first ancestor with a non-transparent background is `body`, whose `background: Canvas` the
engine resolves to **`rgb(255,255,255)`** in light and **`rgb(18,18,18)`** in dark — each confirmed
by sampling the painted body pixel in the same screenshot. The launcher is **not** themed by the
app's `data-theme` palette at all: it declares `color-scheme: light dark` and paints on CSS *system*
colours, and `console.rs::launcher_html()` substitutes only the xterm CSS into it. That is why the
light/dark split here had to come from `prefers-color-scheme` rather than the app's theme blocks.

**Bar.** `#msg` is `font-size: 12px` at weight 400 — WCAG 2.1 SC 1.4.3 "normal" text, so **4.5:1**,
not 3:1. (`#help-body h3`, below, is 11px/700; "large" starts at 18.66px bold, so 4.5:1 there too.)

**Before / after — painted pixels, all six status states:**

| state | light before | light after | dark before | dark after |
|---|---|---|---|---|
| ok (green)   | 2.83:1 **fail** | **5.08:1** | 4.23:1 **fail** | **7.46:1** |
| warn (amber) | 2.44:1 **fail** | **5.93:1** | 5.01:1 pass     | **7.33:1** |
| err (red)    | 3.30:1 **fail** | **5.66:1** | 3.59:1 **fail** | **6.57:1** |

The ticket's own numbers (2.44 / 2.83 light, 5.01 / 4.23 dark) reproduced exactly, which is what
validates the harness. The red was the unmeasured third state — it fails in **both** schemes.

**Cause fixed at the cause.** `#msg { opacity: .85 }` is gone; the softness is baked into the
values. Each state now picks a **class**, and the class picks a **token** with a
`@media (prefers-color-scheme: dark)` override. An inline hex can carry only one value, so it could
never be right in both schemes — and it is invisible to any stylesheet-based guard.

**Distinctness kept.** Amber↔red hue separation is 34° (light) / 35° (dark), against 37° before the
fix; green sits 92–98° from both. Pinned by a test, so a future "fix" that makes amber read as red
goes red itself.

**Enumeration of the neighbouring surfaces** — derived at run time, not recalled (CPE-1932). The
sweep walks every text-bearing element of the real page in both schemes and reports an effective
ratio for each. Three more failures surfaced, all the same defect class (a colour never checked
against the ground it lands on):

- `#keys-msg` (Keys panel status line) — the same two inline hexes on the same `Canvas` ground:
  light **3.44:1** (ok) and **4.08:1** (err). **Fixed**, onto the same tokens → 5.08 / 5.66.
- `#help-body h3` (help-panel section headings, 11px/700) — `var(--accent)` = `#2f6fed`, **4.12:1**
  on the dark ground. `--accent` is a **multi-role** token: it also backs `button.primary`'s *fill*
  under literal white text (4.55:1, fine, and identical in both schemes). Pinning both roles at the
  fill's value is precisely CPE-1919's trap, so the foreground role was split out as `--accent-text`
  (light `#2f6fed` 4.55:1; dark `#4f86ff` **5.52:1**) and `--accent` keeps its fill value untouched.
- Everything else the sweep found clears its bar in both schemes. Post-fix the sweep reports **zero**
  failures.

**Deliberately left:** the launcher has no `hc-light` / `hc-dark`. Those are the main app's
`data-theme` values, and nothing themes this page (no `data-theme` in the file, no theme injection
from the host). The OS-level equivalent here is `forced-colors: active`, under which the UA replaces
every author colour with the system palette — the tokens cannot fail there, and the fact that the
three states stop being colour-distinguishable under forced colours is inherent to that mode and
unchanged by this diff.

**The guard.** `src/lib/aiConsoleLauncher.contrast.test.ts`. No app-side guard covered this file —
verified **by deletion**, not by reading: with the new test set aside and `launcher.html` +
`ai-console-launcher.test.ts` restored to their pre-fix state (`opacity: .85` and the inline hexes
confirmed present by grep), the whole suite — 345 files, 4,932 tests — still ran green. The new
guard *derives* its checked set from the stylesheet (every `color: var(--token)` rule), so a new
token-coloured foreground is covered the day it is written; and a token with no dark override
resolves to its light value in both schemes exactly as the cascade does, so "missing from the dark
block" surfaces as a named failure instead of passing silently. The two engine ground constants
cannot be derived from any source, so they are recorded **as measured** and then **anchored**: the
test re-reads `body { background: Canvas }` and refuses to trust them if the ground moves.

**Red-proofed five ways** — each run by hand, each failing exactly one test and naming the culprit:

1. `--msg-ok` back to `#3a9d4a` → *"`#msg.ok … -> #3a9d4a on #ffffff = 3.44:1, below the 4.5:1 bar
   (font-size 12px / weight 400)`"*.
2. delete the dark `--msg-err` override → *"`-> #c42b1c on #121212 = 3.31:1 … give --msg-err a value
   for this scheme in the @media (prefers-color-scheme: dark) block`"*.
3. re-add `#msg { opacity: .85 }` → *"declares an opacity — … exactly the CPE-1921 defect"*.
4. `body { background: Field }` → *"the measured light/dark ground constants … are stale"*.
5. `setMsg` back to `el.style.color = "#d08a1a"` → the inline-hex tripwire fires **and the contrast
   tests stay green** — which is the point: that tripwire is load-bearing rather than shadowed by
   the measurement (CPE-1929), because an inline colour is invisible to a stylesheet sweep.

**GUI verification.** Not performed and not attempted. The AI Console is served by the sidecar host,
so a launcher change is only visible in a real app once the **host is rebuilt with sidecar config**
and installed — a launcher swap is not a host swap. Nothing here touches Rust, so that rebuild is
mechanical; the headless measurement above is against the same bytes `console.rs` `include_str!`s.

**Housekeeping.** `npm run check` clean. `npm test` 346 files / 4,942 passing.
`node scripts/ratchet-baselines.mjs compare origin/main` raises no baseline — the hard-coded-hex
ratchet walks `.svelte` files under `src/` only, so `launcher.html` sits outside it either way, and
this diff removes five inline hexes from that file regardless.
