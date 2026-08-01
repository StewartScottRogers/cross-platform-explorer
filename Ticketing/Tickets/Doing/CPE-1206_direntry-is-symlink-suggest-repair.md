---
id: CPE-1206
title: "Backend: is_symlink on DirEntry (no extra syscall) + suggest_repair command"
type: feature
component: Backend
priority: medium
status: Doing
tags: ready
created: 2026-08-01
epic: CPE-715
---

## Summary
Part of CPE-715 (foundation). Link CREATE (symlink/hardlink) + `link_status` already ship. The listing doesn't
flag links, and `links::suggest_repair` (broken-link basename search) is a pure fn with no command. Add both.

## Build
- Add `is_symlink: bool` to `DirEntry` (`crates/server/src/model.rs`), sourced from the `file_type()` ALREADY
  read during `list_dir` — **no extra syscall per entry** (critical for the "no measurable listing cost when a
  folder has no links" DoD). Regen `bindings.gen.ts`.
- Add a thin `suggest_repair` Tauri command (dispatcher over `links::suggest_repair`) + binding.
- Target resolution stays LAZY (per CPE-1208, on badge render) — do NOT add link-target resolution to the hot
  `list_dir` path.

## Acceptance Criteria
- [ ] `cargo test -p cpe-server`: a listed symlinked entry has `is_symlink=true`, a plain file false (Windows-
      unprivileged skip pattern); `suggest_repair` returns the found path. Async + spawn_blocking.
- [ ] clippy clean both modes; bindings regenerated (drift guard green); `npm run check` green.

## Work Log
- 2026-08-01 — Filed by Foreman (workshift, epic CPE-715). Foundation; land first (1208/1209 depend on it).
