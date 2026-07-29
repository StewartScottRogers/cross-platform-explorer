---
id: CPE-638
title: Show tag chips + a label tint on file rows
type: feature
component: Frontend
priority: medium
status: Done
tags: ready
estimate: 1.5h
created: 2026-07-18
closed: 2026-07-18
epic: CPE-614
---

# CPE-638 — Show tag chips + a label tint on file rows

## Summary

Surface the tags/labels attached in CPE-637 directly in the file list. Each entry renders its tags as
small chips and, when it carries a colour label, a colour dot before its name plus a soft accent bar in
the label's colour. Purely additive — untagged rows look exactly as before, and Agent Watch's own row
accents keep precedence. Frontend only.

## Acceptance Criteria

- [x] `FileList.svelte` reads each entry's tags from the `tags` store via `entryFor($tags, entry.path)`.
- [x] Tags render as chips: a reflow container (`display:flex; flex-wrap:wrap; gap`) with each chip
      `white-space:nowrap; flex:0 0 auto` and ellipsis — chip text never overflows its background.
- [x] A labelled entry shows a colour dot before its name and a soft left accent bar in the label colour.
- [x] The label tint yields to Agent Watch (`agent-active` / `agent-inside`) so a live change is never
      masked.
- [x] Untagged rows are visually unchanged; the details / list / icons layouts and image thumbnails are
      not regressed.
- [x] Chips/dot are suppressed while a row is being inline-renamed.
- [x] `npm run check` clean; full `npx vitest run` green.

## Resolution

- `FileList.svelte` imports `{ tags, entryFor, labelColor }` and computes `{@const tagEntry =
  entryFor($tags, entry.path)}` per row.
- The row gets `class:tagged` + an inline `--label-color` var; a `.row.tagged:not(.agent-active):not(
  .agent-inside)` rule paints the left accent bar so Agent Watch wins when both apply.
- Inside the name cell: a `.label-dot` (when a label is set) before the name, and a `.tag-chips`
  reflow row of `.tag-chip`s after it — both hidden during inline rename.
- Made `entryFor` and the `tags.ts` store setters null-safe (`store?.[path]`, `store.set(s ?? {})`)
  so the row renderer never crashes before the store has loaded / when a mock returns null; added a
  regression test in `tags.test.ts`.
- Documented tagging in `src/docs/03-explorer.md` (Files section).

## Work Log

- Studied `FileList.svelte` (row markup, agent-activity accents), `app.css` (`.row` / `.cell.name`),
  and the tick-tacks reflow convention.
- Added the chips/dot/tint, the null-safety guards, the `tags.test.ts` case, and the docs bullet.
- Verified: `npm run check` → 0 errors / 0 warnings; `npx vitest run` → 69 files / 652 tests green.
