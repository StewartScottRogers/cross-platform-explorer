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
