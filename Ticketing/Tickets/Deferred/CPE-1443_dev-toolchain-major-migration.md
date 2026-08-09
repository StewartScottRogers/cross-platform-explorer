---
id: CPE-1443
title: "Dev-toolchain major migration: svelte 4→5 / vite 5→8 / vitest 2→4 (clears 14 dev-only npm advisories)"
type: Chore
status: Deferred
priority: Low
component: Frontend
tags: [big-design, deferred-internal]
epic: CPE-534
created: 2026-08-07
---
## Context (found by the shift-1 dependency audit)
Full `npm audit` (incl. dev) reports **15 findings** (9 moderate / 5 high / 1 critical) — ALL cascade from three
aging dev-toolchain majors: `svelte ^4` (latest 5.x), `vite ^5` (latest 8.x), `vitest ^2` (latest 4.x). Worst is
`vitest <3.2.6` critical GHSA-5xrq-8626-4rwp.

## Why LOW urgency / Deferred (not a shipped-binary risk)
- svelte/vite/vitest/svelte-check are all **devDependencies**. `vite build` compiles to a static `dist/` bundle
  → **none ship in the redistributed binary**. The advisories are dev-server / build-time only.
- Several svelte GHSAs are **SSR-specific**; this is a client-only Tauri app (no SSR) → not applicable.
- `npm audit fix` can't clear them without `--force` (major bumps) — svelte 4→5 is a real migration (runes,
  event syntax, component API) touching the whole frontend; a big-design change, not a quick bump.

## Scope (when picked up — big-design, do as its own epic slice)
Migrate svelte 4→5 (runes/`$state`/`$derived`, `on:`→`onclick`, slot→snippet where needed), vite 5→8, vitest
2→4; update `svelte-check`, `@sveltejs/vite-plugin-svelte`, testing-library adapters. Land incrementally behind
green CI (231 test files must stay green). Re-run `npm audit` to confirm the 15 findings clear.

## Notes
Dependency Steward finding, shift-1 audit 2026-08-07. Deferred by our choice (dev-only, big migration) — pickable
anytime as a dedicated effort, not sprint filler. Track the vitest critical as the priority driver.
