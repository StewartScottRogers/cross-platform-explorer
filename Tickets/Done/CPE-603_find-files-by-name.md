---
id: CPE-603
title: Recursive "Find files by name" search
type: feature
component: search
priority: medium
status: Done
tags: ready
estimate: 2-3h
created: 2026-07-17
closed: 2026-07-17
epic:
sprint:
---

## Summary

The explorer can filter the current folder (Ctrl+F) and search *inside* files (Ctrl+Shift+F,
content search), but there is no way to find a file by **name** anywhere below the current folder.
Add a recursive filename search: type a name (or a `*`/`?` glob) and get every matching file/folder
under the current tree, click to reveal it. Mirrors the existing content-search engine.

## Acceptance Criteria

- [x] A Rust `find_files_by_name(root, query)` command walks `root` recursively and returns every
      entry whose name matches, with bounded work (match + dir caps, dot-dir skip) like
      `search_file_contents`; unreadable dirs are skipped, never failing the whole search.
- [x] Matching mirrors the in-folder filter: plain query = case-insensitive substring; a query with
      `*`/`?` = anchored glob over the whole name. A pure matcher is unit-tested in cargo.
- [x] A `FileNameSearchDialog` overlay runs the search against the current folder, lists hits
      (folders first), and reveals the chosen entry (navigate + select), reusing `revealFileInApp`.
- [x] Reachable from the Command Palette ("Find files by name…") and listed in the shortcuts sheet.
- [x] `npm run check` clean; cargo tests for the matcher pass; frontend vitest suite green.

## Work Log
2026-07-17 (Nightshift Loop 2) — Picked up. Estimate 2-3h. Researched: Ctrl+F filters the current
folder only, Ctrl+Shift+F searches file *contents*; no recursive name search exists — the clear gap.
Plan: mirror `search_file_contents` for a name-only walk + pure glob matcher (cargo-tested), a
dialog cloned from ContentSearchDialog, wired via the new Command Palette.

## Resolution
Added a recursive "Find files by name" search (Ctrl+P — the VS Code "Go to File" pairing with the
Ctrl+Shift+P command palette):
- `src-tauri/src/lib.rs` — `find_files_by_name(root, query)` walks the tree with an explicit stack
  (bounded: 2000-match / 50k-dir caps, dot-dir skip, skip-on-error like `list_dir`), returning
  `{path,name,is_dir}` hits. Pure helpers `name_matches` + `glob_is_match` mirror the frontend
  `matchesQuery` (substring, or an anchored `*`/`?` glob). Two cargo tests (matcher + walk) pass;
  registered in `generate_handler!`.
- `src/lib/fileNameSearch.ts` (+test) — result types + a pure `sortNameMatches` (folders first,
  then name, then path). `FileNameSearchDialog.svelte` — a sibling of ContentSearchDialog listing
  hits and revealing the chosen one via `revealFileInApp`.
- `App.svelte` — Ctrl+P opens it (in a folder), plus a "Find files by name…" palette command.
- `shortcuts.ts` — Ctrl+P added to the Navigation group of the cheat sheet.

Verified: cargo tests + clippy (default and sidecar-platform) clean; `npm run check` 0/0; frontend
67 files / 635 tests green.

## Work Log
2026-07-17 (Nightshift Loop 2) — Built and verified. Chose Ctrl+P (Go-to-File) as the idiomatic
shortcut; it was free. Matcher reuses the same substring/glob rules as the in-folder filter so
behaviour is consistent. GUI verification pending the next Nightshift build.
