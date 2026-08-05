---
id: CPE-1315
title: File-Health panel — slice 1 (panel shell + dangling-links streaming tab)
type: feature
component: Frontend
priority: high
tags: ready
created: 2026-08-04
epic: CPE-1002
estimate: 3-4h
---

## Summary
First slice of the File-Health panel surfacing the built-but-unsurfaced file-inspection-safety scans (epic
CPE-1002). Slice 1 = the tabbed dialog shell + ONE scan wired end-to-end: **dangling/cyclic links**
(streaming). This proves the never-before-used FRONTEND streaming-consumer path against real Rust (the 3
`_stream` commands have Rust unit tests but ZERO frontend consumers today — this is their first UI).

## Scope (from the vetted scope plan)
- New `src/lib/components/FileHealthDialog.svelte`: a modal with a per-scan **tab shell** (TABS.md: accent
  top-bar + recessed inactive chips, reuse `.tab`/`.tab.active`) — only the **Dangling links** tab wired this
  slice (leave clearly-marked placeholders/tabs for type-mismatch, orphan-sidecars, empty-dirs to be filled by
  later slices, OR render only the one tab this slice — worker's call, but keep the shell extensible).
- Dangling-links tab: model on `NearDuplicatesDialog.svelte` (read-only reveal). Stream via **STREAMING.md**
  frontend shape (see `SimilarImagesDialog.svelte`): `rawInvoke` + `createChannel<DanglingLink[]>()` from
  `src/lib/invoke.ts`, call `find_dangling_links_stream(root, excludes, streamId, onLink)`, append batches,
  flip `loading` off on the first batch, fire `cancel_dangling_links_stream(prevStreamId)` on rescan/close
  supersede (generation token). Row = path (name + parent dir) + a `reason` badge (`Missing`/`Cyclic`) using
  the existing `link-broken` Icon glyph. Footer shows `scanned` + a "capped" note when `truncated`.
  Click a row → dispatch `navigate` with the path + `close`.
- Wire entry points (append, don't reflow — shared with 5 existing scan features): Tools menu in
  `MenuBar.svelte` (pick an EXISTING Icon glyph — check `Icon.svelte`, don't invent one), `App.svelte`
  `fileHealthOpen` state + render block + menu-select `case "file-health"` + Command-Palette entry.
- i18n: all new strings across **all 12 locales** (repo enforces 100% coverage — `i18n.test.ts`).
- Docs (CI-guarded): add `"file-health"` to the `Section` union + `SECTION_DOC` map in `src/lib/sectionDocs.ts`
  mapped to `"22-file-health"`, and create `src/docs/22-file-health.md` (frontmatter title/order:22/category:
  Explorer). `sectionDocs.test.ts` auto-checks it.

## Acceptance Criteria
- [ ] Dialog opens from Tools menu + Command Palette; light-theme-only palette; visible dialog border; tabs
      per TABS.md; reason badge reflows (flex-wrap container, nowrap pill) per the tick-tack rule.
- [ ] Dangling-links scan streams: first rows paint before the walk completes (liveness); batches append;
      rescan supersedes + cancels the prior stream; row-click navigates + closes.
- [ ] `FileHealthDialog.test.ts` (jsdom, mock invoke + a Channel stub with settable onmessage): asserts
      command+args, batch-append across multiple onmessage calls, loading flips false on first batch,
      cancel_*_stream called with the prior streamId on supersede, navigate+close on row click, error path
      surfaces without a stuck spinner. Falsifiable.
- [ ] `npm run check` clean; `npm run test:unit` green (incl. the sectionDocs guard). Production `invoke` from
      `src/lib/invoke.ts` (busy-cursor) for the collect path; `rawInvoke` + allowlist for the streamed path.
- [ ] A `gui-smoke/specs/file-health.smoke.ts` render spec sketch (seed a fixture with a dangling symlink; open
      Tools → File Health; assert the dangling tab renders a row). NOTE: real streaming-Channel end-to-end
      verification defers to the Foreman's build→run + Visual-Critic pass (the risk flagged in the scope).

## Work Log
2026-08-04 (workshift run 2) — Filed by the Foreman from the vetted File-Health scope. Slice 1 of 4.
Dispatched to a worker. Real streaming-consumer wiring to be GUI-verified by the Foreman before Done.
