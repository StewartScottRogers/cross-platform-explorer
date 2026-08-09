---
id: CPE-1204
title: "Wire the SimHash text + Jaccard folder near-duplicate cores (stretch)"
type: feature
component: Backend
priority: low
status: In Progress
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
- [x] cargo-tested adapters; commands in `bindings.gen.ts`; `npm run check` clean; clippy clean.

## Notes
- Genuinely parallel to the image spine (different cores) but shares `lib.rs`/`bindings.gen.ts` with CPE-1201 —
  **serialize the command-registration step**. Not on the DoD-critical path (epic's stated target is images).

## Work Log
- 2026-08-01 — Filed by Foreman (sprint, epic CPE-997). Stretch; pick up after the image spine lands.
- 2026-08-01 — Wired end-to-end: `cpe_server::document_similarity` (walk + SimHash adapter over
  `simhash::near_duplicate_docs`) and `cpe_server::folder_similarity_scan` (walk + per-folder hash-set
  adapter over `folder_similarity::cluster_similar_folders`), each with cargo tests. Thin dispatchers
  `find_similar_documents` / `find_similar_folders` registered in `generate_handler!`/`collect_commands!`;
  `bindings.gen.ts` regenerated. Minimal frontend: `NearDuplicatesDialog.svelte` (read-only, no
  delete/cleanup — the ticket's AC is the scan wiring, not a removal workflow), reached via Tools menu +
  Command Palette ("Find similar documents…" / "Find near-identical folders…"), with Svelte tests.
  `cargo test -p cpe-server` (1173 passed), `cargo test` in `src-tauri` (85 passed, incl. the
  `bindings.gen.ts` drift guard), `cargo clippy --all-targets -D warnings` clean in both crates (plain +
  `specta`/`specta-bindings,sidecar-platform` feature modes), `npm run check` clean, `npm test` clean
  (1629 passed). PR opened.
