---
id: CPE-492
title: "Audit every menu against the design standard and apply it everywhere"
type: Task
status: Done
priority: Medium
component: Multiple
tags: [ready]
estimate: 1h
created: 2026-07-16
closed: 2026-07-16
---

## Summary
After writing the menu standard ([[CPE-491]], `docs/design/MENUS.md`), sweep **every** menu in the app
and the AI Console for violations, fix them, and fold any discoveries back into the standard so it's
globally accurate.

## Acceptance Criteria
- [x] Every popup menu audited against `docs/design/MENUS.md` (context menus + dropdowns, Svelte app +
      `launcher.html`).
- [x] Any violation fixed; any hard-coded / red menu-text colour removed.
- [x] Discoveries folded into `docs/design/MENUS.md` (kept the single source of truth accurate).

## Resolution
**Audited:** `ContextMenu`, `AgentMenu`, `TabMenu`, `PreviewPane` (`.text-ctx`), `MenuBar`, `CommandBar`,
and the console's `launcher.html` (`.menu`, `#model-menu`).

**Result — already compliant** apart from the one fixed in CPE-491 (the `AgentMenu` red). All app
menus use theme variables for colour, inherit `var(--text)` for item text, and hover via the global
`button:hover → var(--hover)`. No other red / hard-coded menu-text colour exists (`PreviewPane`'s
`#c42b1c` is a form *error label*, not a menu; confirm-dialog primary buttons are the sanctioned place
for red).

**Discoveries (folded into `docs/design/MENUS.md`):**
- The app already has **two** shared, conforming implementations: the **global `.menu`/`.menu-sep`/
  `.menu .check`** in `src/app.css` for **dropdowns** (used by `CommandBar` + `MenuBar`), and the
  per-component **`.ctx`/`.row`** pattern for **context menus**. Documented both; noted the `.ctx`
  copies are a candidate for extraction into shared globals (follow-up, not needed for compliance).
- **Active-value marking** is a rule now: a menu tracking a current choice marks it with a ✓ in
  `var(--accent)` (`.menu .check`) or `var(--selection)` on the row (MenuBar's locale picker does this).

**Fix applied:** the console's `.menu` used a raw `rgba(128,128,128,.55)` border while its sibling
`#model-menu` used `var(--line)` — aligned `.menu` to `var(--line)` so the two console menus match.
Both console menus correctly use CSS **system colors** (`Canvas`/`CanvasText`) per the standard.

`npm run check` clean. Files: `docs/design/MENUS.md`, `sidecar/ai-console/src/launcher.html`.

## Notes
Follow-up idea (unscheduled): extract the duplicated `.ctx`/`.row` context-menu CSS into shared global
classes (like `.menu`), so the four copies collapse to one implementation.
