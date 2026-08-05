---
id: CPE-1316
title: File-Health panel — slice 2 (type-mismatch + orphan-sidecars streaming tabs)
type: feature
component: Frontend
priority: high
tags: ready
created: 2026-08-04
epic: CPE-1002
estimate: 3h
---

## Summary
Slice 2 of the File-Health panel (CPE-1315 shipped the shell + dangling-links tab). Add two more streaming
scan tabs to the SAME `FileHealthDialog.svelte`, reusing slice 1's proven `rawInvoke`+Channel+cancel-prior-gen
pattern (copy the dangling tab's wiring). These two DO stream progressively (unlike dangling-links, which
batches at walk-end), so liveness is genuinely visible.

## Backend commands (exist + specta-bound)
- **Type mismatch**: `find_type_mismatches_stream(root, excludes, streamId, onHit: Channel<MismatchHit[]>)` +
  `cancel_type_mismatches_stream(streamId)`. `MismatchHit { path, claimed_ext, detected_label, detected_ext }`.
  Row: file name + location, and "claims `.jpg` → looks like <detected_label>" (claimed_ext → detected_label/ext).
- **Orphan sidecars**: `find_orphan_sidecars_stream(root, recursive, excludes, streamId, onOrphan: Channel<string[]>)`
  + `cancel_orphan_sidecars_stream(streamId)`. Result rows are plain `string[]` paths (a sidecar file whose
  main file is missing).

## Acceptance Criteria
- [ ] Two new tabs in `FileHealthDialog.svelte` (`mismatch`, `orphan`) via the existing extensible TABS array +
      per-tab body, each streaming via the slice-1 pattern (createChannel/append/loading-flip/finally-clear/
      cancel-prior-streamId/late-batch-drop). Reuse the dangling tab's structure — don't reinvent.
- [ ] Row rendering per the real struct fields (mismatch: claimed→detected; orphan: path name+location).
      Reuse the row/badge styles; pill rows reflow.
- [ ] Entry points: add Tools-menu + Command-Palette entries the same way slice 1 added dangling
      (`find-type-mismatches`, `find-orphan-sidecars`), each opening the panel to its tab (pass an initial-tab
      prop to `FileHealthDialog`). Pick EXISTING Icon glyphs (check Icon.svelte). Append, don't reflow.
- [ ] i18n: new keys across ALL 12 locales. Extend `src/docs/22-file-health.md` to describe the 3 scans.
- [ ] jsdom tests for both tabs (mirror the dangling tests: append, cancel-prior-streamId, navigate+close,
      error, empty). `npm run check` clean + full `npm run test:unit` green (incl. i18n + sectionDocs guards).

## Notes
Real streamed-Channel + visual verification is batched by the Foreman across the File-Health slices (one real
build → gui-smoke → Visual-Critic pass) — jsdom green is the merge gate here, same as slice 1.

## Work Log
2026-08-04 (workshift run 2) — Filed by the Foreman. Slice 2 of 4. Reuses slice-1's proven streaming pattern.
