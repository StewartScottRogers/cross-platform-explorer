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
