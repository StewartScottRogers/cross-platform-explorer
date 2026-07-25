---
id: CPE-941
title: Secure-delete pass planner (overwrite schedule + honest caveats)
type: feature
component: Backend
priority: low
tags: ready
epic: CPE-738
created: 2026-07-23
closed: 2026-07-23
status: Done
---

## Summary
First headless slice of secure delete & vaults (CPE-738) — the shredding half. `cpe_server::secure_delete`:
- `ShredScheme { Zero, Random, Dod3, Gutmann }` → `passes(scheme) -> Vec<PassPattern>` (the ordered
  overwrite patterns: Zeros/Ones/Random/Byte).
- `plan_shred(path, size, scheme, on_ssd, copy_on_write) -> ShredPlan { passes, total_write_bytes,
  caveats }` — the pass schedule PLUS **honest, platform-aware caveats**: on SSD/flash (wear-levelling) or
  copy-on-write filesystems, in-place overwrite can't guarantee erasure — so it says so and points at the
  real remedies (full-disk encryption, TRIM, encrypted vaults). Pure; the engine executes the passes.

## Acceptance Criteria
- [x] Each scheme yields the right ordered passes; total write bytes = size × passes.
- [x] SSD / copy-on-write / plain-disk each get the correct honest caveat. 6 unit tests; clippy clean.

## Work Log
- 2026-07-23 (dayshift) — Activated CPE-738 with the shred-pass planner + caveats. The actual overwrite
  engine, and the encrypted-vault half (passphrase/key + transparent mount), are the remaining children.
