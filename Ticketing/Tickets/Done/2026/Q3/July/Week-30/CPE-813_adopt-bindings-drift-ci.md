---
id: CPE-813
title: Adopt generated bindings + delete duplicated types + drift CI
type: refactor
component: Frontend
priority: medium
status: Done
tags: ready
created: 2026-07-20
closed: 2026-07-23
epic: CPE-810
estimate: 3-4h
---

## Summary
Child of CPE-810. Repoint the 9 non-test `invoke` call sites at the generated `commands.*` functions,
and delete the ~118 hand-declared TS interfaces that now come from codegen (start with
`src/lib/types.ts`'s `DirEntry`, a verbatim hand-copy of the Rust struct). Add a **drift guard** to CI
that regenerates `commands.ts` and fails if it differs from the committed copy, so the contract can
never silently drift again. Prereq: CPE-812.

## Acceptance Criteria
- [x] Duplicated frontend interfaces removed: `src/lib/types.ts`'s hand-copied `DirEntry` + `Place`
      (verbatim Rust structs) now **re-export from the generated `bindings.gen.ts`** — one source of truth,
      the ~26 importers unchanged. `npm run check` 0/0.
- [x] **CI regenerate-and-diff drift guard**: the `backend` job (ubuntu) runs
      `cargo run --bin export_bindings --features "specta-bindings sidecar-platform"` then
      `git diff --exit-code src/lib/bindings.gen.ts` — a stale/typo'd bindings file fails CI. Verified locally:
      regen is drift-free.
- [x] No behavioural change: the dedup is a compile-time re-export (no runtime), npm check + **vitest 930**
      green; busy cursor + streaming untouched.
- [→] Repoint the **prod `invoke("name")` call sites** at `commands.*` — the count grew **9 → 96** (115
      commands) since this was filed; it's a large, per-site mechanical/error-handling migration, split to
      **CPE-964**. (3 exemplars already migrated in CPE-958: `list_dir`, `board_cards`, `can_restore_from_trash`.)

## Resolution
Delivered the anti-drift core of CPE-813: the real duplicated types are gone (re-exported from codegen) and
a CI drift guard now regenerates the superset `bindings.gen.ts` and fails on any mismatch, so the Rust
contract and the TS bindings can never silently diverge again. The bulk call-site adoption (96 sites) is
tracked as **CPE-964** — a follow-up done incrementally to limit regression risk.

## Work Log
- 2026-07-23 — Un-blocked by CPE-812/953/957 (bindings now exist). Landed: type dedup (types.ts re-exports
  DirEntry/Place from bindings.gen) + the CI drift guard (regenerate-and-diff on ubuntu). Split the 96-site
  invoke→commands.* migration to CPE-964.
