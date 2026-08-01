---
id: CPE-1204
title: "Wire the SimHash text + Jaccard folder near-duplicate cores (stretch)"
type: feature
component: Backend
priority: low
status: Backlog
tags: ready
created: 2026-08-01
epic: CPE-997
---

## Summary
Stretch for CPE-997 (beyond the image target). The `simhash`/`near_duplicate_docs` (`simhash.rs`) and
`folder_similarity` (`folder_similarity.rs`) pure cores are also built-but-unwired. Add scan adapters (walk+read
text / walk+hash-set-per-folder) + Tauri commands + bindings for near-duplicate DOCUMENTS and near-identical
FOLDERS.

## Acceptance Criteria
- [ ] cargo-tested adapters; commands in `bindings.gen.ts`; `npm run check` clean; clippy clean.

## Notes
- Genuinely parallel to the image spine (different cores) but shares `lib.rs`/`bindings.gen.ts` with CPE-1201 —
  **serialize the command-registration step**. Not on the DoD-critical path (epic's stated target is images).

## Work Log
- 2026-08-01 — Filed by Foreman (workshift, epic CPE-997). Stretch; pick up after the image spine lands.
