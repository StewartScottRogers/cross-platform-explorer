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
