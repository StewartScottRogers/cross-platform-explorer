---
title: "Can the `trash` crate v5 list/restore/empty (not just delete), and how to build a browsable in-app Trash?"
slug: trash-bin-crate-v5-2026-08-10
date: 2026-08-10
status: current
tags: [trash, recycle-bin, cpe-1486, trash-crate, os_limited, adapter, macos-descope, streaming, pm-reference]
---

## Answer
**`trash 5.2.6`** (pinned `trash = "5"`) exposes list/restore/purge via `trash::os_limited`, but that module is
**`cfg`-gated to Windows + Linux/Freedesktop Unix ONLY — macOS/iOS/Android are structurally excluded at compile
time** (not a doc gap). So an in-app browsable Trash is Win+Linux; macOS descopes to a "open Finder Trash" message.

## API facts (v5.2.6)
- `os_limited::list() -> Result<Vec<TrashItem>>`, `metadata(&TrashItem) -> TrashItemMetadata { size: Bytes|Entries }`,
  `restore_all<I: IntoIterator<Item=TrashItem>>(items)`, `purge_all<I>(items)`, `trash_folders()` (unix-non-mac).
- `TrashItem { id: OsString, name: OsString, original_parent: PathBuf, time_deleted: i64 }`; `item.original_path() = original_parent.join(name)`.
- **`restore_all`/`purge_all` are all-or-nothing per call** — abort on first `Error::RestoreCollision { path, remaining_items }`
  or `RestoreTwins { path, items }`. For honest per-item outcomes, loop item-by-item (as existing `restore_from_trash_impl` does).

## Not a green field — reuse existing plumbing
`src-tauri/src/lib.rs` already wraps the crate for **undo-last-delete**: `delete_to_trash`, `can_restore_from_trash`
(`cfg!(any(windows, linux))` capability probe already exposed to the frontend), `restore_from_trash`
(`#[cfg(any(windows, linux))]`, matches items by `original_parent.join(name)`, refuses if target exists, calls
`restore_all`). Tests use a `trash_roundtrip_available()` skip-don't-fail guard for CI runners with no Recycle Bin (CPE-1268).

## Architecture correction (important)
`docs/design/SERVER-ARCHITECTURE.md` explicitly lists "recycle-bin delete (`trash`)" under **"stays in the Tauri
adapter"**. So the new list/restore/empty commands belong in `src-tauri/src/lib.rs` next to the existing trash
commands — NOT a new `cpe-server` module. Only the serializable `TrashEntry` DTO goes in `cpe-server::model`
(mirroring `Place`/`OpResult`) so it flows through the specta-bindings pipeline; keep the `trash` crate out of
`cpe-server`'s dep graph.

## Build plan (3 slices → CPE-1558/1559/1560)
1. `TrashEntry` DTO in `cpe-server::model` + `list_trash`(streamed, skip-on-error)/`restore_trash_items`(per-item
   loop, surface RestoreCollision/Twins)/`empty_trash`(None=all) in `src-tauri`, `#[cfg(any(windows,linux))]`, tests
   with the CPE-1268 skip guard. Solo-safe, headless.
2. `generate_handler!` registration (both call sites) + capabilities verify (likely no new grant) + regen
   `bindings.gen.ts` + BOTH Cargo.locks. Serialize after 1; drift-guard test gates it.
3. Frontend: Trash sidebar section (own section like Smart Folders, not a `Place`) gated on a `can_browse_trash`
   probe (macOS → Finder message) + `TrashView.svelte` (reuse FileList plumbing, cols: name/original-path/deleted-date)
   + Restore/Empty menu (MENUS.md: text `var(--text)`, Empty routes through ConfirmDialog, red only on the dialog's
   primary button) + `src/docs` page + `sectionDocs.ts` entry (guard test fails CI without it). Serialize after 2;
   touches hot Sidebar.svelte/App.svelte — watch [[parallel-pr-duplicate-import-trap]].

## Traps
macOS gate must be `#[cfg]` not runtime-`if` (the module doesn't exist off Win/Linux → compile error). Streaming
per STREAMING.md (trash can grow unbounded). Bindings drift needs both Cargo.locks ([[regen-specta-bindings-on-struct-change]],
[[multiple-independent-cargo-locks]]). CPE-1268 Recycle-Bin-less CI runner → skip-don't-fail.
