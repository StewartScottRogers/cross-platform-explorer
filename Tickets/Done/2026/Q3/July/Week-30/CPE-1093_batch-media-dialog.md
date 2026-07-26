---
id: CPE-1093
title: "Batch-Media dialog — ordered ops, live plan preview, streamed progress (GUI #2)"
type: feature
component: Frontend
priority: high
status: Done
tags: ready
created: 2026-07-26
epic: CPE-723
depends-on: CPE-1092
---

## Summary
GUI #2. A dialog to apply an ordered list of media ops (resize / convert / rotate / flip / rename /
strip-metadata) to the multi-file selection, with a live plan preview and streamed progress. Modelled on the
existing **BatchRenameDialog**. Depends on CPE-1092 (the `batchMediaPlan` + `batchMediaExecuteStream`
bindings). Frontend only.

## Context (verified — file:line)
- Template to mirror: `src/lib/components/BatchRenameDialog.svelte` + `src/lib/batchRename.ts`. Opened by
  `App.svelte:beginBatchRename()` (1559-1562, gates `selectionCount>=2`), rendered ~`App.svelte:3487-3493`,
  dumb dialog (string props in, `apply`/`cancel` events out), reactive pure planner `$: items=...`,
  scrollable `from→to` preview, parent `applyBatchRename()` (1566-1592) does the work + `reportResults`
  (1535-1550) + undo + `loadPath` refresh.
- Context menu: `ContextMenu.svelte:132-136` `{#if selectionCount>1}` batch-rename row; dispatched in the
  `App.svelte` action switch (~2243). Add a "Batch media…" row + `case "batch-media": beginBatchMedia()`.
- Streamed progress precedent: `TransferPanel.svelte` `.bar`/`.fill` percent-width; channel via
  `createChannel<OpResult[]>()` (`src/lib/invoke.ts:98-100`) + `rawInvoke` (NOT the busy-cursor `invoke`,
  since the dialog shows its own progress).
- Dialog styling convention: `.dialog { background:var(--surface); border:1px solid var(--border-strong);
  border-radius:10px; box-shadow:0 20px 50px rgba(0,0,0,.25) }` (BatchRenameDialog.svelte:141-149) — visible
  border, theme vars only ([[dialogs-need-a-visible-border]]).
- New bindings (from CPE-1092): `batchMediaPlan(job, inputs)`, `batchMediaExecuteStream(items, job,
  onResult)`, types `BatchJob {ops:MediaOp[], nonDestructive:boolean}`, `MediaOp` (tagged union `op`),
  `PlannedItem {input,output,summary}`, `BatchReport`, `OpResult`.

## Design (buildable)
1. **Open path** — `beginBatchMedia()` in `App.svelte`, gated on `selectionCount>=2` AND the selection being
   image-capable; **pre-filter** unsupported/non-image extensions out of the input set (reuse the
   preview/thumbnail extension sniffing already in the app) and show a "N of M files aren't images and will be
   skipped" notice. Stash the eligible paths, render `<BatchMediaDialog paths={...} />` conditionally like
   BatchRenameDialog. Add the context-menu row (MENUS.md: `.row`, `var(--text)`, an `Icon size={15}`, i18n
   label) + the switch case.
2. **Ordered op builder** — instead of BatchRename's single-mode radios, an **op pill list** (order matters):
   an "add op" row (dropdown Resize/Convert/Rotate/Flip/Rename/Strip-metadata + the op's param field + "Add")
   appends to `ops: MediaOp[]`; render the chosen ops as **reflowing pills** (`display:flex; flex-wrap:wrap;
   gap`; each pill `white-space:nowrap; flex:0 0 auto`, `max-width`+ellipsis) each showing its param inline
   (`Resize 1024px ✕`, `Convert → webp ✕`) with an `×` to remove ([[tick-tacks-reflow]]). A
   **non-destructive** checkbox bound to `BatchJob.non_destructive` (default checked).
3. **Live plan preview** — reactively (debounced ~200ms; ops/param change) call `batchMediaPlan(job, paths)`
   and render `PlannedItem[]` as `input → output — summary` rows, reusing the `.preview`/row styling. Show
   `validate` `Err(String)` inline near Apply (e.g. "rotate must be 90/180/270"); disable Apply while invalid
   or `ops.length===0`. Generation-token the async preview so a stale plan can't overwrite a newer one. Cap
   the rendered preview list for very large selections (paginate/"showing first N").
4. **Apply with streamed progress** — open `const ch = createChannel<OpResult[]>()`, subscribe to accumulate
   results into a live `done/total` + `failed` counter and a progress bar (reuse TransferPanel `.bar/.fill`),
   call `rawInvoke("batch_media_execute_stream", { items, job, onResult: ch })`. On completion:
   `reportResults`-style summary ("N converted, M skipped: first reason…"), refresh the folder
   (`loadPath(currentPath)`), close. Keep the dialog open showing progress until done.
5. **Theme/border** — identical `.dialog` block to BatchRenameDialog; all colours via CSS variables; visible
   border. Path/destination: v1 writes non-destructive siblings (planner handles it) — no hand-typed path
   field; if a destination picker is ever added, use the native `openFolderDialog` ([[path-inputs-need-a-picker]]).

## ⚠ Notes / guardrails
- No new deps. Theme-variable colours only; pills reflow, text never wraps inside a pill. Progress via
  `rawInvoke` + `createChannel` (dialog shows its own progress) — do NOT use the busy-cursor `invoke` for the
  streamed execute; DO use the normal typed binding for `batchMediaPlan`.
- Division-safe progress (total 0 → no NaN width). Debounce + generation-token the preview. Remove the channel
  subscription on destroy/cancel (no leaks).
- Any pure helper (op→pill-label, op-list→BatchJob, eligible-extension filter) goes in a small testable module
  with vitest cases (empty ops, unsupported-ext filtering, label formatting).

## Acceptance Criteria
- [ ] Selecting ≥2 image files → "Batch media…" context entry opens the dialog with the eligible files;
      non-image files are filtered with a skip notice.
- [ ] Adding ops builds a reflowing ordered pill list; the preview shows `input → output — summary` per file
      and updates as ops change; `validate` errors show inline and block Apply.
- [ ] Apply streams progress (live done/total + failed + bar), writes outputs, shows a completion summary,
      refreshes the listing; non-destructive toggle respected.
- [ ] Pills reflow (no overflow); colours from theme vars (identical light/dark); dialog has a visible border;
      no `@tauri-apps/api/core` raw import except the sanctioned `rawInvoke`/`createChannel` for the stream.
- [ ] `npm run check` clean; vitest green (incl. new helper tests); no new deps.

## Work Log
2026-07-26 (workshift, GUI) — Filed by the Foreman as GUI #2's dialog, on top of the CPE-1092 command
enablement. Design from the Library brief
(`.claude/research-library/entries/batch-media-dialog-backend-surface.md`).
