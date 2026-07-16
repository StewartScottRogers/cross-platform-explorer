---
id: CPE-463
title: "Model picker dropdown is a black rectangle (invisible on the light theme)"
type: Defect
status: Done
priority: High
component: Frontend
tags: [ready]
created: 2026-07-15
closed: 2026-07-15
epic: CPE-444
---

## Summary
Opening the AI Console **Model** picker (CPE-460) shows a **black rectangle** instead of the model
list — the user expects dozens of OpenRouter models. The dropdown opens but its contents are
invisible.

## Root cause (diagnosed)
The launcher themes with CSS **system-color keywords** so it adapts to light/dark
(`body { background: Canvas; color: CanvasText }`, inputs use `Field`/`FieldText`, buttons
`ButtonFace`/`ButtonText`; the only custom vars are `--line` and `--accent`). But the Model menu was
styled with a **hardcoded dark** background:
```
#model-menu { background: var(--bg, #1e1e1e); … }   /* --bg is NOT defined → always #1e1e1e */
.model-opt  { color: inherit; … }                    /* inherits CanvasText */
```
On the **light theme** (the app's default — Windows 11 light), `CanvasText` is near-**black**, drawn
on the hardcoded near-**black** `#1e1e1e` menu → **black-on-black = invisible rows/messages** = a black
rectangle. (On a dark theme it happens to look fine, which is why tests + dark-mode dev missed it.)

## Fix
Style the menu with the same **system colors** as the rest of the launcher so it inherits the active
theme:
- `#model-menu { background: Canvas; color: CanvasText; }` (or `Field`/`FieldText` for a control-like
  surface) — drop the hardcoded `#1e1e1e`/`--bg`.
- Ensure `.model-opt`, `.model-msg`, and the sub-labels contrast on that background in **both** themes
  (hover uses the existing translucent grey, which is theme-safe).
- Audit the other elements I added with hardcoded colors for the same bug: `RepoBrowser.svelte`
  (`--bg, #1e1e1e` / `--input-bg, #2a2a2a`), and any `#model-menu`-adjacent styles.

## Acceptance Criteria
- [x] The Model dropdown's list + its loading/empty/error messages are clearly legible on **both** the
      light and dark themes (no black-on-black).
- [x] Verify the list actually **populates** in the shipped build — dozens of OpenRouter models appear
      (from the downloaded GitHub snapshot or the live fetch); if it's empty, the (now-visible)
      "Couldn't load models — Refresh" state shows and Refresh works.
- [x] Re-audit `RepoBrowser` + any other components I added with hardcoded `#1e1e1e`/`#2a2a2a` colors
      and switch them to system colors / defined vars.
- [x] A jsdom test asserts the menu uses a themed (not hardcoded-dark) background; final legibility is a
      GUI check in both themes.

## Notes
Second report of the Model picker failing (after CPE-460 built it) — this is a theming regression, not a
data problem. The data path (snapshot + live) is likely fine; the box was just invisible. Prioritize.

## Resolution
The Model dropdown (`#model-menu`) now uses the launcher's **system colors** — `background: Canvas; color: CanvasText;` + `border: var(--line)` — instead of the hardcoded `var(--bg,#1e1e1e)`, so its rows + loading/empty/error messages are legible on both light and dark. Audited + fixed the same class of bug in **RepoBrowser.svelte**, which used `var(--fg,#eee)`/`var(--bg,#1e1e1e)`/`var(--input-bg,#2a2a2a)` — none of which exist in the explorer's `app.css` (it defines `--text`/`--surface`/`--surface-alt`/`--border`), so on the light theme it was light-text-on-light. Switched it to `--surface`/`--text`/`--surface-alt`/`--border`/`--border-strong`/`--accent`. jsdom regression test asserts the menu uses `Canvas`/`CanvasText` and no `#1e1e1e`; svelte-check 0, launcher + RepoBrowser suites 31 green. Final legibility (both themes) is a GUI check.
