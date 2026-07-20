---
id: CPE-813
title: Adopt generated bindings + delete duplicated types + drift CI
type: refactor
component: Frontend
priority: medium
status: Open
tags: needs-prereq
created: 2026-07-20
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
- [ ] All 9 invoke sites use generated, typed `commands.*`; no stringly-typed `invoke("name")` left in prod.
- [ ] Duplicated frontend interfaces removed; `npm run check` passes against generated types.
- [ ] CI regenerate-and-diff guard fails on a stale/typo'd `commands.ts`.
- [ ] GUI-verified: no behavioural change; busy cursor + streaming still work.

## Work Log
