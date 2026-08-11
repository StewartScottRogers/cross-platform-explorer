---
id: CPE-1632
title: "White text on solid --danger fails the 3:1 contrast floor app-wide — Delete buttons, \"removed\" badges, drive-full bars"
type: Bug
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
created: 2026-08-11
closed:
---

## Why
Measured by the Visual Critic in real Chrome while re-checking CPE-1616 (PR #822). **Pre-existing** — not
introduced by that PR — but nobody had ever measured it, because it is invisible to the test suite: the
existing contrast guard tests cover text-on-tinted-background patterns, not the solid-fill pattern.

## The gap
Several surfaces render **white text on a solid `var(--danger)` background**:
- primary destructive buttons in `ConfirmDialog`, `ShredConfirmDialog`, `CheckpointDialog`, `BatchMediaDialog`
- `.agent-badge.removed` / `.tl-badge.removed` pills
- `Sidebar`'s `.drive-bar-fill.full`

Measured in dark theme, white on solid `--danger`:
- `#ff6659` (the value before CPE-1616) → **2.88:1**
- `#ff7b6f` (the value CPE-1616 briefly set, since reverted) → **2.53:1**

WCAG's floor for UI components and large text is **3:1**; normal-size text needs **4.5:1**. Both values fail,
so this surface has been below the bar for as long as it has existed. It is legible at the sizes used, but it
is the app's *destructive-action* colour — the one place a user most needs an unambiguous read.

## Why it wasn't caught
The dark-theme contrast guard (`app.css.dark-contrast.test.ts`) checks token pairs used as
**foreground-on-tinted-background** (e.g. `color-mix(in srgb, var(--danger) 8%, var(--surface))`). Nothing
asserts the **solid-fill** pairing of `#fff` on raw `var(--danger)`, so the failure never surfaced.

## Fix
- Decide the right treatment for solid destructive fills. Options worth weighing: darken the solid-fill
  background specifically (a dedicated `--danger-solid` token rather than reusing `--danger`), or keep the
  fill and switch the foreground off pure white, or restyle these as outline/tinted buttons in line with
  `docs/design/MENUS.md`'s "colour comes from theme variables, never a hard-coded destructive red".
- Whatever is chosen must clear **3:1** minimum in BOTH themes, and any new token must be defined in BOTH
  the light and dark blocks.
- **Extend the contrast guard to cover the solid-fill pattern**, so this cannot silently regress or recur.
  That guard extension is arguably the most valuable part of this ticket.
- Check `docs/design/MENUS.md` and `TABS.md` for anything this contradicts, and update if needed.

## Acceptance criteria
- Every white-on-solid-`--danger` surface listed above clears 3:1 in both themes; measured ratios recorded
  in the work log.
- A guard test covers the solid-fill pairing and fails against today's values (negative control).
- Verified **by looking in a real browser in both themes** — jsdom cannot see colour, and this defect existed
  precisely because only the numbers nobody measured were wrong.

**Conflict surface:** `src/app.css`, the contrast guard tests, and the components listed above. Touches global
theme tokens — do not run in parallel with other theming work (notably CPE-1631, the missing highlight.js
theme).
