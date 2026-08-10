---
id: CPE-1570
title: "Preview action-bar groundwork: declarative per-provider actions + generic action bar in PreviewPane"
type: Task
status: Doing
priority: High
component: Frontend
epic: CPE-1568
tags: [ready]
created: 2026-08-10
closed:
---

## Why
Epic CPE-1568 (custom per-file-type right pane), slice 1 — the unblocker for every other slice. The preview
registry (`src/lib/preview/provider.ts`, 20 providers) has no way to declare per-type ACTIONS today; actions are
wired ad hoc. This adds the declarative mechanism + a generic action bar so subsequent slices (JSON tree, image
actions, archive actions, fonts, notebook…) just declare `actions` on their provider.

## Scope
- Add to `src/lib/preview/provider.ts`:
  - `PreviewAction` interface: `{ id: string; labelKey: string; icon: string; enabled?(ctx): boolean; run(ctx): void | Promise<void> }`.
  - `PreviewActionCtx`: `{ entry, text, selectionText, ...the loader/invoke helpers PreviewPane already has }` — actions
    must call through `src/lib/invoke.ts` (busy-cursor convention), not a new IPC path.
  - Optional `actions?: PreviewAction[]` on `PreviewProvider`.
- Add a generic **action bar** render block to `src/lib/components/PreviewPane.svelte`, styled like the existing
  `.preview-edit-bar`, rendering `provider.actions` (filtered by `enabled(ctx)`) as buttons with `Icon` glyphs + `$t(labelKey)` labels.
- **Worked example (prove the mechanism):** migrate the existing hard-coded copy buttons in `JwtPreview.svelte` and
  `CertPreview.svelte`, plus the Edit/Wrap buttons, onto the new `actions` mechanism (or at least JWT/Cert copy — pick
  the cleanest to demonstrate declaration→render→run end to end). Keep behavior identical.

## Acceptance criteria
- A provider can declare `actions`; the pane renders them in the action bar; clicking runs `action.run(ctx)`; disabled
  when `enabled(ctx)` is false.
- Labels go through `$t()` (i18n) and use `Icon` glyphs — per MENUS.md (text `var(--text)`, theme-only colors).
- The migrated JWT/Cert (and/or Edit/Wrap) buttons behave exactly as before.
- Unit tests for the action-filtering/enablement logic + a component test rendering an action bar and firing `run`.
- `npm run check` clean; vitest green.

## Notes
Do NOT unify with `ContextMenu.svelte`'s boolean-prop pattern (separate, bigger untangle — out of scope). Frontend-only;
disjoint from the trash sidebar work. See Library `filetype-right-pane-coverage-2026-08-10`. Model: sonnet.
