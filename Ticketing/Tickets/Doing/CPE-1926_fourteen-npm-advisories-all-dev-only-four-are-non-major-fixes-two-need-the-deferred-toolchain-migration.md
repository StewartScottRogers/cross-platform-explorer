---
id: CPE-1926
title: fourteen npm advisories (1 critical, 5 high) — all dev-only; four are non-major fixes worth taking now, two need the deferred toolchain migration
type: task
priority: Medium
status: In Progress
tags: ready
estimate: S
created: 2026-08-27
---

## Summary

Dependency Steward pass, 2026-08-27, run by the sprint Foreman.

    npm audit --omit=dev  →  found 0 vulnerabilities
    npm audit (dev+prod)  →  {"moderate":8,"high":5,"critical":1,"total":14}

**Nothing ships to users.** Every advisory is in the dev/test toolchain. That is the reason this is
Medium and not High — but a dev-tool RCE-adjacent path still runs on this machine and on CI runners
with repo credentials, so "dev-only" is not "harmless".

## The high/critical six, split by what fixing them costs

**Take now — transitive, non-major fix available (`fixAvailable: true`):**

| pkg | advisory |
|---|---|
| `brace-expansion` | DoS via unbounded expansion length → out-of-memory process crash |
| `nanoid` | non-secure generators can loop indefinitely with a negative size |
| `postcss` | incomplete fix of GHSA-6g55-p6wh-862q — attacker-controlled `sourceMappingURL` reads arbitrary `.map` files when `from` is unset |
| `undici` | downstream response desynchronization via the retry interceptor |

**Do NOT take here — both need a semver-major bump of a direct dependency:**

| pkg | current fix | note |
|---|---|---|
| `vite` | → **8.2.2**, `isSemVerMajor: true` | path traversal in optimized-deps `.map` handling |
| `vitest` | → **4.1.11**, `isSemVerMajor: true` | critical, via `@vitest/mocker` |

Those two belong to **CPE-1443** (dev-toolchain major migration), already sitting in
`Ticketing/Tickets/Deferred/`. A vitest 3→4 jump moves ~4,600 tests across 335 files and a Vite major
moves the whole build; that is its own piece of work with its own risk, not a security patch.

## Acceptance criteria

- [x] Take the four transitive non-major fixes. Confirm afterwards that `npm audit` reports them
      resolved and that the remaining count is exactly the two majors plus whatever moderates the
      bumps do not clear — state the new numbers.
- [x] **Regenerate and commit `package-lock.json` properly.** CLAUDE.md records that lockfile drift
      here is the guardrail that gets missed, because nothing fails when it drifts — it surfaces later
      as a dirty tree that reads as unrelated noise. CPE-1904 is open on adding a build-time backstop;
      do not rely on it existing yet.
- [x] Run the full `npx vitest run` and `npm run check` after the bump. A transitive `postcss` or
      `vite`-adjacent change can move build output even when the version jump is a patch.
- [x] Re-point CPE-1443 at this ticket for the two majors, so the deferred item carries the security
      reason and not only the maintenance one — that is a stronger argument for eventually doing it.
- [x] Record the audit numbers in the ticket so the next Steward pass has a baseline to diff against
      rather than re-deriving one.

## Notes

Filed 2026-08-27 by the sprint Foreman during a Dependency Steward pass. `cargo audit` was
deliberately **not** run in the same pass — the machine was at 83–99% CPU with six concurrent build
agents and a fresh audit build would have slowed every one of them. Rust advisories are tracked
separately: **CPE-1820** (bump russh off 0.54 for two high-severity advisories) is in
`Ticketing/Tickets/Blocked/`, and **CPE-1442** tracks the RSA Marvin attack.

Related: **CPE-1443** (deferred toolchain majors — owns vite 8 and vitest 4), **CPE-1904**
(package-lock drift backstop), **CPE-1820**, **CPE-1442**.

## Work Log

**2026-08-27** — Took the four non-major transitive fixes. `npm 10.9.8` / `node v22.22.3` (the same npm
that generated the existing lockfile — the diff is surgical, not a whole-file rewrite).

`npm audit fix` **without** `--force`, so npm was structurally unable to accept a semver-major bump.
`package.json` is **unchanged**; the whole change is `package-lock.json`, **13 insertions / 13 deletions**
(`git diff --numstat` → `13 13 package-lock.json`). No Rust lockfile touched.

| pkg | before | after | advisory |
|---|---|---|---|
| `brace-expansion` | 1.1.16 | **1.1.18** | DoS via unbounded expansion length → OOM crash (+ the CVE-2026-14257 mitigation bypass) |
| `nanoid` | 3.3.15 | **3.3.18** | non-secure generators loop indefinitely with negative size (+ zero size) |
| `postcss` | 8.5.16 | **8.5.26** | GHSA-fxqj-rqcc-2cmp — attacker-controlled `sourceMappingURL` reads arbitrary `.map` files when `from` is unset |
| `undici` | 7.28.0 | **7.29.0** | GHSA-8xcm-r25x-g524 downstream response desync via retry interceptor (+ 4 more) |

### Audit numbers (root project only — the baseline the next Steward pass should diff against)

```
BEFORE  npm audit --json .metadata.vulnerabilities
        {"info":0,"low":0,"moderate":8,"high":5,"critical":1,"total":14}
AFTER   {"info":0,"low":0,"moderate":8,"high":1,"critical":1,"total":10}

npm audit --omit=dev   BEFORE: found 0 vulnerabilities
                       AFTER:  found 0 vulnerabilities
```

All four targeted advisories **actually cleared** — verified by name, not inferred from the count moving:
`brace-expansion`, `nanoid`, `postcss`, `undici` are all gone from `npm audit --json .vulnerabilities`.
High 5→1 is exactly those four; the surviving high is `vite`. Moderate stayed 8 and critical stayed 1
because every one of those is semver-major-gated — see below.

### What is NOT fixed (still open after this lands)

All **10** remaining findings carry `fixAvailable.isSemVerMajor: true`. There is no non-major residue
left; the number only moves again when the toolchain majors land under **CPE-1443**.

- `vitest` ^2 → **4.1.11** — clears the **critical** (`vitest` UI-server arbitrary file read+exec) plus
  `@vitest/mocker`, `vite-node`.
- `vite` ^5 → **8.2.2** — clears the surviving **high** (`vite` optimized-deps `.map` path traversal,
  `server.fs.deny` Windows bypass, launch-editor NTLMv2 hash disclosure via UNC) plus `esbuild`.
- `svelte` ^4 → **5.56.10** — clears 6 moderate SSR/XSS + `svelte-hmr`. *(Not named in the original
  ticket body, which counted only the high/critical six; it is a third major, and CPE-1443 already
  owned it.)*
- `@sveltejs/vite-plugin-svelte` ^3 → **7.3.0** — clears the plugin, `-inspector`, `vitefu`; rides the
  vite/svelte majors.

**Scope limit on this pass, worth carrying forward:** every number here is the **root** `npm audit` only.
`git ls-files '*package-lock.json'` enumerates **two** npm projects — this one and **`gui-smoke/`**, which has
its own manifest, its own advisories and its own CI job. `gui-smoke/` was **not** audited in this pass, and its
findings include a non-major-fixable `brace-expansion` high — the same package fixed here at the root, still
live in the second lockfile. Filed separately by the Foreman. The lesson is the CPE-1932 one: **enumerate the
lockfiles, do not recall them** — "the npm audit" silently meant "the root npm audit".

CPE-1443 has been updated with this table, the security framing (dev-only bounds the blast radius to the
dev environment — it does not make the advisories theoretical, and two of the vite ones are
Windows-specific on a Windows dev box), and the `total:10` baseline.

### Verification

- `npm run check` — svelte-check **0 errors, 0 warnings**
- `npm test` (`vitest run`) — **339 files / 4692 tests passed**
- `npm run build` (real `vite build`) — **✓ built in 13.87s**; only the pre-existing >500 kB chunk-size
  advisory, no new warnings. Run because `postcss` and `nanoid` sit under the bundler, so a regression
  had to surface here rather than at release time.
