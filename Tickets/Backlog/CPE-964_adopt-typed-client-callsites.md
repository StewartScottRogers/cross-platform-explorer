---
id: CPE-964
title: Adopt the generated typed client across prod invoke call sites
type: refactor
component: Frontend
priority: low
status: Open
tags: ready
created: 2026-07-23
epic: CPE-810
estimate: 4h+
---

## Summary
Split from **CPE-813** (which landed the type dedup + drift-guard CI). Repoint the remaining prod
`invoke("name")` / `rawInvoke("name")` call sites at the generated, typed `commands.*` — there are **~96**
(across ~115 commands), not the 9 the original ticket estimated. Done incrementally to limit regression
risk: each site changes from stringly-typed `invoke` to a typed call, and `Result`-returning commands need
their `{status,data|error}` wrapper unwrapped (error handling changes per site).

## Acceptance Criteria
- [ ] Prod `invoke("name")`/`rawInvoke("name")` call sites replaced with `commands.*` (batch by area:
      navigation, tags, board, backup, preview, …); streaming keeps `rawInvoke`/`createChannel`.
- [ ] `npm run check` + vitest green after each batch; no behavioural change (busy cursor + streaming intact).
- [ ] GUI-verified per batch.

## Notes
Foundation done in CPE-953/957/813. Exemplars already migrated (CPE-958): `list_dir`, `board_cards`,
`can_restore_from_trash`. The typed client routes through the busy-cursor `invoke` (CPE-547). The drift
guard (CPE-813) keeps the generated types honest while this proceeds.

- 2026-07-23 — Increment 1: migrated the **tags** module (7 sites) to the typed `commands.*` via a new
  `unwrap` helper (restores throw-on-error). npm check 0/0; vitest 930. ~89 prod sites remain.
- 2026-07-23 — Increment 2: migrated the **settings** module (read_settings/write_settings) to the typed
  `commands.*`. npm check 0/0; settings tests 19.
- 2026-07-23 — Increment 3: migrated the **BoardView** module — all 8 sites: `board_move`/`board_review`
  (Result-returning, via `unwrap` to preserve throw-on-error) + `board_epics`/`board_archived`/
  `find_project_root` (plain returns, direct). BoardView now has zero raw `invoke` calls. Updated the
  test's `../invoke` mock to also export `unwrap` (the typed client routes through the same mocked
  `invoke`, so the existing assertions hold). npm check 0/0; full suite 930 green.
- 2026-07-23 — Increment 4: migrated the **RepoBrowser** module — all 8 forge_* sites (`forge_browse`,
  `forge_get_token`, `forge_set_token`, `forge_delete_token`, `forge_clone`, `forge_generic_remote`,
  `forge_admit_host`, `forge_clone_url`) to `commands.*` + `unwrap` (all Result-returning). Dropped the
  3 explicit `withBusy(() => invoke(...))` wrappers — redundant now, since the typed client routes through
  the busy-tracking `invoke` which already wraps in `withBusy`; removed the now-unused `withBusy` import.
  RepoBrowser now has zero raw `invoke`. npm check 0/0; forge tests 7; full suite 930 green.
- 2026-07-23 — Increment 5: migrated the **folderWatch** module (Agent Watch driver) — all 6 sites:
  `move_exact`/`delete_permanent`/`run_watch_actions`/`folder_watch_stop` (plain returns, direct) +
  `folder_watch_start`/`entry_info` (Result, via `unwrap`). The injected `stat`/`run` deps of
  `handleFolderBatch` now call `commands.entryInfo(path).then(unwrap)` / `commands.runWatchActions(...)`;
  generated `EntryInfo`/`OpResult`/`WatchAction` are structurally compatible with the deps' types.
  folderWatch now has zero raw `invoke`. npm check 0/0; folderWatch tests 8; full suite 930 green.
- 2026-07-23 — Increment 6: migrated the **sidecar** module (the sidecar-platform client) — all 14 sites:
  `sidecar_registry_ids`/`agent_watch_stop` (plain) + the 12 Result-returning
  (`sidecar_start_ai_console`/`_agent_board`, `agent_watch_start`, `sidecar_consent_state`/`_set_consent`/
  `_revoke_capability`/`_details`/`_repair`/`_stop`/`_set_enabled`/`_diagnostics`) via `unwrap`. This module
  is the graceful-degradation client (every fn try/catches to a safe "off" fallback); that's preserved —
  `unwrap` throws on a Result error and plain-command rejections propagate, both caught by the existing
  try/catch. Verified the 6 generated types (`Capability`, `ConsentState`, `SidecarInfo`, `SidecarRepair`,
  `SidecarDiagnostics`, `DiagLogLine`) are structurally identical to the local hand-declared ones. sidecar
  now has zero raw `invoke`. npm check 0/0; sidecar tests 17; full suite 930 green.
- 2026-07-23 — Increment 7 (Nightshift): migrated the bulk of **App.svelte** — ~29 sites across ~30
  commands: file ops (`move_entries`/`move_exact`/`copy_entries`/`delete_to_trash`/`delete_permanent`/
  `restore_from_trash`/`rename_entry`/`create_dir`/`create_file`), preview loaders (`read_file_text`/
  `read_archive_entries`/`read_preview_info`/`read_image_data_url` via `.then(unwrap)`), archive
  (`extract_archive_entry`/`extract_archive`/`compress_to_zip`), external (`open_external`×4/`open_terminal`/
  `run_as_admin`), disk/nav (`disk_space`/`entries_for_paths`/`parent_dir`/`special_folders`/`list_drives`/
  `home_dir`), sync (`forge_sync`), sidecar (`sidecar_stop`/`sidecar_close_session`), write/misc
  (`write_file_text`×4/`files_identical`/`same_volume`/`git_remote_url`). Dropped 2 redundant `withBusy`
  wrappers + the import; handled a dynamic command name (`permanent ? delete_permanent : delete_to_trash`)
  by splitting into two typed calls; `same_volume`'s `.catch(()=>false)` preserved (plain return).
  **Deferred to increment 8** (local-type friction): `verify_all_baselines` (Record vs Partial + local
  IntegrityReport) and `forge_repo_status` ×2 (RepoSyncStatus vs local gitStatus null/undefined) — 3 sites
  still on raw `invoke`. npm check 0/0; full suite 930 green.
- 2026-07-23 — Increment 8 (Nightshift): migrated App.svelte's final 3 type-friction sites, completing the
  file (**App.svelte now has zero raw `invoke`**; dead `invoke` import removed). Confirmed local
  `ChecksumEntry`/`IntegrityReport` (lib/integrity) are structurally identical to the generated types, so
  `verify_all_baselines` needed only one `as Record<…>` cast to narrow the generated `Partial<{[k]:…}>`
  return (the arg `Record` passes as-is; every key read was just sent in). `forge_repo_status` ×2 cast the
  generated `RepoSyncStatus` to the local structural `gitStatus` type (`loadSyncPolicy`'s `OnDiverge` union
  is assignable to `string|null`); both are plain-return commands that reject on the plain build → identical
  try/catch → `null` fallback as before. npm check 0/0; full suite 930 green.
- 2026-07-23 — Increment 9 (Nightshift): **preview & inspection cluster** — 11 sites across 3 files:
  `preview/loaders.ts` (read_file_text/read_archive_entries/read_preview_info/read_image_data_url via
  `.then(unwrap)`; savePreviewText keeps its `Promise<void>` via `.then((r)=>void unwrap(r))`),
  `HexView.svelte` (read_file_range), `PropertiesDialog.svelte` (text_stats/hash_file/entry_info/image_meta/
  dir_size). Confirmed local `Stats`≡`TextStats`, `Info`≡`EntryInfo`, `ImageMeta`≡`ImageMeta` structurally,
  so no casts needed. **Deferred**: DataBrowser (`Page.total` number vs number|null) + CompareDialog
  (`CompareNode` vs generated `TreeNode`) — real type friction, next increment. npm check 0/0; suite 930.
- 2026-07-23 — Increment 10 (Nightshift): the two friction files — **DataBrowser** (data_browser_sources/
  query/page; widened local `Page.total` to `number|null` to match generated, then no casts needed;
  `Column` already ≡) + **CompareDialog** (read_file_range/read_file_text clean; scan_tree ×2 cast
  `.then(unwrap) as Promise<CompareNode[]>` — local `CompareNode` is assignable to generated `TreeNode`, so
  the cast is sound and behaviour-identical to the old `invoke<CompareNode[]>` generic). npm check 0/0;
  suite 930 green.
- 2026-07-23 — Increment 11 (Nightshift): **clean dialogs & modules cluster** — 8 sites across 7 files:
  IntegrityDialog (checksum_folder/verify_folder), Sidebar (list_dir), DuplicatesDialog (delete_to_trash),
  BackupDashboard (scan_tree → CompareNode cast), WorkbenchView (workbench_diff — now passes the required
  2nd arg `null` the generated sig added), RunCommandConfirm (run_command; **import aliased to `api`** — the
  component already has a `commands: string[]` prop), driveScheduler.ts (list_drives, plain). Two test/type
  fixes: WorkbenchView.test's `../invoke` mock now also exports `unwrap`; confirmed CommandOutput≡CmdOut,
  WorkbenchDiff≡ exactly, AuditEvent differs only in `detail` null/undefined (cast-safe). npm check 0/0;
  suite 930 green.
  - **Discovered:** `forge_conflict_abort` is called by ConflictDialog via raw `invoke` but is NOT in the
    typed bindings (only `forge_conflict_continue` is) — it's missing from `collect_commands!`. Filed as a
    follow-up; ConflictDialog's migration is blocked on it. Also still on raw invoke: SyncDialog (Status
    cast), transfers.ts, SessionHistoryDialog, and AttributesDialog (2 platform-excluded commands).
- 2026-07-23 — Increment 12 (Nightshift): **ConflictDialog** fully migrated, after fixing [[CPE-968]] (added
  the registered-but-unlisted `forge_conflict_abort` to `collect_commands!` + regenerated bindings). All 5
  sites: forge_conflict_state / forge_conflict_versions (both **plain** returns, direct await — no unwrap) +
  forge_resolve_file / forge_conflict_continue / forge_conflict_abort (Result, via unwrap; the old dynamic
  `cmd` string ternary is now a typed-call ternary). Confirmed ConflictState/ConflictFile/ConflictVersions
  are structurally identical to the local types. npm check 0/0; suite 930 green. Remaining on raw invoke:
  SyncDialog (Status cast), transfers.ts, SessionHistoryDialog, AttributesDialog.
