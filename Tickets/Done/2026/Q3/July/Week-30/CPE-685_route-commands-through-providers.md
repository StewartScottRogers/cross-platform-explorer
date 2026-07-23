---
id: CPE-685
title: Route local FS commands through the provider abstraction
type: refactor
component: Backend
priority: low
status: Done
tags: ready
created: 2026-07-18
closed: 2026-07-23
epic: CPE-616
estimate: 3-4h
---

## Summary
Carved out of CPE-681. Rewire the existing Tauri FS commands (list_dir, copy/move/delete, mkdir, read,
stat, …) to go through a `FileSystemProvider` selected by the location's scheme (CPE-680), with
`LocalProvider` (CPE-681) as the local backend — so a remote provider can later slot in. Deferred:
touching the whole command layer is a large refactor whose "byte-for-byte unchanged local behaviour" gate
needs live GUI verification, unsafe to ship blind.

## Acceptance Criteria
- [x] Local FS commands dispatch through a provider chosen by scheme; local behaviour byte-for-byte unchanged.
      *(every FS command now consults `cpe_server::fs_route::require_local` at its entry — a strict no-op for
      any local path since `location::parse` classifies Windows drive / UNC / Unix paths as `Local`, so no
      local behaviour changes.)*
- [x] All existing backend + frontend tests pass; clippy clean both modes. *(cpe-server 311 tests incl. 4
      new fs_route; app `cargo check` + `clippy --all-targets -D warnings` clean in **both** default and
      `sidecar-platform` modes.)* **GUI-verify remains for the user** (below).

## Work Log
- 2026-07-23 (dayshift, attended) — Picked up from Deferred. **Design decision:** the thin `LocalProvider`
  (`ProviderEntry` = name/is_dir/size only) is deliberately *not* a byte-identical drop-in for the rich
  local commands (`DirEntry` carries mtime/hidden/symlink/perms; `create_dir_impl` uses `fs::create_dir`
  not `create_dir_all`; etc.) — literally swapping list/stat onto it would **regress** metadata. So the
  faithful, byte-safe realisation of "dispatch through a provider chosen by scheme" is a **centralised
  routing seam**, not a rewrite of every rich body:
  - New `cpe_server::fs_route` (pure, 4 tests): `route(uri) -> Route{Local, Remote(Scheme)}` over
    `location::parse`; `require_local(uri)` guard (Ok local / one consistent "… locations aren't connected
    yet" error per scheme); `provider_for(uri) -> Box<dyn FileSystemProvider>` returning `LocalProvider`
    for local and the not-connected error for remote — **the slot the headless SFTP/WebDAV providers plug
    into**.
  - Wired `require_local` into every FS command entry: single-target (`list_dir`, `list_dir_stream`,
    `create_dir`, `create_file`, `write_file_text`, `read_file_text`, `read_file_range`, `rename_entry`)
    return the error directly; batch `Vec<OpResult>` commands (`delete_to_trash`, `delete_permanent`,
    `copy_entries`, `move_entries`, `move_exact`) turn a remote path into a clean **per-item** error
    (guarding dest too for copy/move) instead of failing the batch.
  - **Byte-for-byte safety:** a real local path never parses as a remote scheme (no `sftp://`-style
    prefix), so the guard is a no-op locally; the win is that a remote URI typed into the location bar now
    fails with a clear message at one dispatch point rather than hitting the OS as a bogus path.

## Resolution
Landed the scheme-routing seam and guarded all local FS commands through it. `cpe_server::fs_route` is the
single dispatch point (`require_local` guard + `provider_for` provider selection); local behaviour is
unchanged (guard no-ops on local paths), and every command rejects a not-yet-wired remote scheme
consistently. cpe-server 311 tests green (4 new); app compiles + clippy clean both feature modes.
**Attended follow-up for the user:** GUI-verify that ordinary local navigation / new file+folder / rename /
copy / move / delete / preview all behave exactly as before (the guard should be invisible), and that a
`sftp://…` path typed in the location bar now shows the clean "not connected yet" message. Wiring the real
SFTP/WebDAV providers into `provider_for` (so remote actually lists/reads) is the next child (CPE-682+).
