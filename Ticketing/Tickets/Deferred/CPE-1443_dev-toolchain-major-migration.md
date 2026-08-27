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

## Update 2026-08-27 — CPE-1926 took the non-major half; the majors are now the *only* thing left

CPE-1926 (Dependency Steward pass) landed the four transitive, non-major fixes — `brace-expansion`
1.1.16→1.1.18, `nanoid` 3.3.15→3.3.18, `postcss` 8.5.16→8.5.26, `undici` 7.28.0→7.29.0 — lockfile-only,
no `package.json` change. That moved full `npm audit` from **14 (8 moderate / 5 high / 1 critical)** to
**10 (8 moderate / 1 high / 1 critical)**. `npm audit --omit=dev` was 0 before and is still 0.

**Every one of the remaining 10 is gated on a semver-major bump of a direct devDependency**, i.e. on this
ticket. There is no longer any non-major residue to pick off — the next `npm audit` number only moves when
CPE-1443 is done:

| direct dep | needs | clears |
|---|---|---|
| `vitest` ^2 → **4.1.11** | vitest 2→4 migration (4,692 tests / 339 files) | `vitest` (**critical**, UI server arbitrary file read+exec), `@vitest/mocker`, `vite-node` |
| `vite` ^5 → **8.2.2** | vite 5→8 | `vite` (**high**, path traversal in optimized-deps `.map`; `server.fs.deny` bypass on Windows; launch-editor NTLMv2 hash disclosure via UNC on Windows), `esbuild` |
| `svelte` ^4 → **5.56.10** | svelte 4→5 runes migration | `svelte` (6 moderate XSS/SSR), `svelte-hmr` |
| `@sveltejs/vite-plugin-svelte` ^3 → **7.3.0** | rides the vite/svelte majors | plugin + `-inspector` + `vitefu` |

**The security argument, not just the maintenance one:** these are dev-only, so nothing reaches users — but
they run on this machine and on CI runners **holding repo credentials**. The vite advisories in particular are
Windows-specific (`server.fs.deny` bypass, NTLMv2 hash disclosure via UNC), and this is a Windows dev box. The
vitest critical is arbitrary file read *and execute* when the Vitest UI server is listening. "Dev-only" bounds
the blast radius to the development environment; it does not make the advisories theoretical.

Baseline for the next Steward pass to diff against — **root project only**, i.e. `npm audit` run at the repo
root against the top-level `package.json` / `package-lock.json`: **`{"moderate":8,"high":1,"critical":1,"total":10}`**
on npm 10.9.8 / node v22.22.3.

**That number is not the repo's whole npm position.** `git ls-files '*package-lock.json'` enumerates **two** npm
projects: this root one and **`gui-smoke/`**, which carries its own `package.json` + `package-lock.json`, its own
advisories, and its own CI job (`gui-smoke.yml`). A Steward pass must
**enumerate** the lockfiles rather than assume "the npm audit" means the root one; CPE-1932 is the standing
lesson here. This ticket's scope is unchanged either way: CPE-1443 owns the **root** dev-toolchain majors only.

Related: **CPE-1926** (the non-major half, done), **CPE-1904** (package-lock drift backstop).
