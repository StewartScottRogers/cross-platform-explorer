---
title: "How to implement drag files OUT of the app into other OS apps (CPE-672/674) in Tauri v2?"
date: 2026-08-02
tags: [drag-out, cpe-672, cpe-674, tauri-plugin-drag, dnd, capabilities, archive-extract, coexistence, attended-verify]
status: current
---

## Question
CPE-672 (drag files OUT into Explorer/Finder/other apps) + CPE-674 (extract-archive-entry on drag-out). How, in Tauri v2?

## Findings / decision
- **No first-party Tauri v2 drag-OUT API** — native `onDragDropEvent` is drag-IN only. Plain HTML5 webview drag
  CANNOT expose real filesystem paths cross-platform (only in-page DataTransfer strings). So use the de-facto
  plugin: **`tauri-plugin-drag` (crate) + `@crabnebula/tauri-plugin-drag` (npm), v2.1.1, MIT/Apache** — a small,
  justified, single-purpose dep (lean-core OK; the CPE-672 spike already vetted it).
- **API:** `startDrag({ item: string[] /*abs paths*/, icon: string /*REQUIRED preview img path*/, mode?: "copy"|"move" }, onEvent?)`.
  `icon` is mandatory — a missing icon is the classic first-bug (bundle a small PNG or gen a temp badge). `onEvent`
  reports `{result:"Dropped"|"Cancelled", cursorPos}` over a Channel (use for temp cleanup / undo hook).
- **Wiring:** `tauri-plugin-drag = "2"` in src-tauri/Cargo.toml; `.plugin(tauri_plugin_drag::init())` in `run()`
  (src-tauri/src/lib.rs); `@crabnebula/tauri-plugin-drag` in package.json; add **`"drag:default"`** to the
  `permissions` array in `src-tauri/capabilities/default.json` (else `plugin:drag|start_drag` is denied at runtime).
- **Cross-platform:** Win + macOS full; Linux via GTK (Tauri uses wry/GTK, so OK — the winit caveat doesn't apply). Gate gracefully anyway.
- **CPE-674 is 90% built:** archive rows carry synthetic in-zip paths → need a real file on disk first.
  `cpe_server::archive::extract_archive_entry_any(path, inner)` (crates/server/src/archive.rs:256) already extracts
  one entry (zip/tar/tar.gz/7z) to `%TEMP%/cpe-archive/<basename>` (path-traversal guarded) and is ALREADY a
  registered command (src-tauri/src/lib.rs:5185) + binding (bindings.gen.ts:1842). Flow: archive dragstart →
  `await extractArchiveEntryAny(zip, inner)` → `startDrag({item:[tempPath], icon})` → leave temp in the shared
  cpe-archive scratch (do NOT delete on Dropped — OS copy may still be reading); session/periodic cleanup.

## The coexistence crux (UX call reserved for attended verify)
`startDrag` in `dragstart` launches a NATIVE OS drag that pre-empts the existing HTML5 internal drag
(`FileList.svelte:214` onDragStart + `dnd.ts`) that internal folder/sidebar drops rely on — can't run both in one
gesture. **Recommended:** unify on the native drag — drops outside the window → OS; drops back inside → Tauri's
native `onDragDropEvent`, hit-tested against the folder row/sidebar under `cursorPos` → existing drop handler
(ExplorerPane.svelte:524). Fallback: HTML5 internal + native only on a modifier / leave-window threshold. Present
BOTH options at the attended session — it's a UX decision a human must feel.

## Slices (A → B → C)
- **A (headless, parallel-safe)** — plumbing: both deps, plugin register, `drag:default`, new `src/lib/dragOut.ts`
  wrapper (param mapping, icon resolution, platform gate) + jsdom unit tests. Not wired to any row yet. Surface:
  src-tauri/Cargo.toml, src-tauri/src/lib.rs, capabilities/default.json, package.json, src/lib/dragOut.ts(+test).
- **B (attended verify) = CPE-672 proper** — call wrapper from FileList/Sidebar dragstart for on-disk selections;
  implement chosen coexistence; platform gate. Surface: FileList.svelte, Sidebar.svelte, dnd.ts, ExplorerPane.svelte.
- **C (mostly headless, attended final) = CPE-674** — re-enable `canDrag` for archive rows (drag-out only);
  extractArchiveEntryAny → temp → startDrag; temp cleanup on onEvent. Reuses existing backend. Shares FileList.svelte
  with B → sequence after B.

## Headless vs attended
Headless-buildable+testable: deps/plugin/capability wiring (compiles+clippy), dragOut.ts wrapper + unit tests,
archive staging (already covered), re-enabling canDrag. Attended (human drag+drop): real drop into Explorer/Finder,
the coexistence decision, archive-entry-onto-real-folder. Foreman builds headless parts, skip-and-notes the verify.

Sources: github.com/crabnebula-dev/drag-rs, crates.io/crates/tauri-plugin-drag, tauri issue #2593.
