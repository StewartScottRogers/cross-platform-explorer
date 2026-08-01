---
id: CPE-1214
title: "Expose spotlight search + frecency over Tauri commands + specta bindings"
type: feature
component: Backend
priority: medium
status: Doing
tags: ready
created: 2026-08-01
epic: CPE-704
---

## Summary
Backbone for CPE-704. The fuzzy/ranking/frecency cores exist + are tested (`spotlight.rs`,
`spotlight_results.rs`, `spotlight_frecency.rs`) but are UNEXPOSED (no command, no binding). Wire them to the
frontend.

## Build
- Thin `#[tauri::command] spotlight_search(query, sources: Vec<(ResultKind, Vec<String>)>, per_kind_cap)`
  dispatching into `spotlight_results::aggregate`; `spotlight_frecent(visits, now_s, limit)` into
  `spotlight_frecency::rank_frecent`. One-line dispatchers (SERVER-ARCHITECTURE); register in `generate_handler!`
  + `collect_commands!`. Regenerate `bindings.gen.ts` (`SpotResult`/`SpotSection`/`Visit` first cross the
  boundary — drift guard).

## Acceptance Criteria
- [ ] `cargo test -p cpe-server` green; a Rust integration test invokes the command path → grouped/highlighted
      output; clippy both modes clean; `npm run check` green; bindings-drift guard green.

## Work Log
- 2026-08-01 — Filed by Foreman (workshift, epic CPE-704). Backbone; build first. 1216-1218 depend on the shape.
