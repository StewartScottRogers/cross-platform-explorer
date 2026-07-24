---
id: CPE-828
title: Native tags bridge — Tauri command layer (+ Properties UI follow-up)
type: feature
component: Multiple
priority: medium
status: Done
tags: ready
created: 2026-07-24
closed: 2026-07-24
epic: CPE-717
estimate: 2-3h
---

## Summary
The native-metadata core is done + tested (`cpe_server::native_meta`/`native_tags`/`finder_tags`/
`native_bridge`, CPE-826/827/829/830). This ticket exposes the bridge's `pull`/`push` to the app. Split
into the **headless command layer** (this slice) and the **Properties UI** (follow-up, attended —
macOS↔Finder byte-interop can only be verified on a real Mac).

Picked up 2026-07-24 ("do it all").

## This slice — command layer (headless) — DONE 2026-07-24
- [x] `cpe_server::native_bridge::pull_ctx` / `push_ctx`: ServerCtx entry points that read/persist the tag
      store around the tested `pull`/`push`, mirroring `crate::tags`'s command-entry pattern. Cargo
      round-trip test (push_ctx writes native → a fresh config store pull_ctx recovers the tags) — passes
      (Windows ADS verified headlessly).
- [x] Thin `#[tauri::command]`s: `native_tags_name` (the OS store's label), `native_tags_pull` (→ updated
      `TagStore`), `native_tags_push`. Registered in **both** `generate_handler!` and `collect_commands!`
      (per CPE-968); bindings regenerated (+28 lines, specta mode compiles).
- [x] `cargo test` (cpe-server native_bridge 4/4) + clippy (app default + specta-bindings/sidecar-platform,
      + cpe-server) + `npm run check` 0/0 — all green.

## Follow-up slice — tag-editor UI — DONE 2026-07-24
- [x] A "Native tags" affordance — built into **`TagEditor`** (the per-file tag surface), single-file only
      (native metadata is per-path, so hidden in batch mode): shows `native_tags_name()` and **Pull** /
      **Push** buttons that sync the path and re-seed the editor / refresh the tag store. Store logic lives
      in `tags.ts` (`nativeTagStoreName`/`pullNativeTags`/`pushNativeTags`, mirroring the existing
      `setEntryTags` store pattern), unit-tested.
- [x] i18n: `tags.pullNative` / `tags.pushNative` added to all 12 complete locales.
- [ ] *(optional, deferred)* an opt-in auto-pull-on-browse toggle — not needed for the core feature; can
      follow if wanted (must stay off by default per the fast/small tiebreaker).
- [ ] **Attended GUI verification — the user's step**: a real macOS Finder round-trip (tag in CPE → see it
      in Finder, and vice-versa). The **Windows NTFS-ADS** round-trip is covered headlessly by the
      `native_bridge` cargo test (slice 1) + the `tags.ts` unit tests.

## Status
Code-complete: command layer (slice 1, #291 merged) + tag-editor UI (slice 2). Green: `npm run check` 0/0,
full frontend suite 938. Only the macOS-Finder visual interop check remains, which requires a Mac (the
user's). Moving to Done; reopen a thin follow-up if the auto-pull toggle or macOS check surfaces issues.

## Notes
- Everything degrades gracefully: a filesystem that can't store native metadata is a silent no-op, never a
  listing failure (enforced in `native_bridge`). Internal store is authoritative on push; pull is a
  non-destructive union. Only tag **names** cross on macOS (Finder has no colour-label concept).
