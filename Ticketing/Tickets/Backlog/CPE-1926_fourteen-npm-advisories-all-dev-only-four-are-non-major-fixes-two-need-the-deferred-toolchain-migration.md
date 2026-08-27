---
id: CPE-1926
title: fourteen npm advisories (1 critical, 5 high) — all dev-only; four are non-major fixes worth taking now, two need the deferred toolchain migration
type: task
priority: Medium
status: Open
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

- [ ] Take the four transitive non-major fixes. Confirm afterwards that `npm audit` reports them
      resolved and that the remaining count is exactly the two majors plus whatever moderates the
      bumps do not clear — state the new numbers.
- [ ] **Regenerate and commit `package-lock.json` properly.** CLAUDE.md records that lockfile drift
      here is the guardrail that gets missed, because nothing fails when it drifts — it surfaces later
      as a dirty tree that reads as unrelated noise. CPE-1904 is open on adding a build-time backstop;
      do not rely on it existing yet.
- [ ] Run the full `npx vitest run` and `npm run check` after the bump. A transitive `postcss` or
      `vite`-adjacent change can move build output even when the version jump is a patch.
- [ ] Re-point CPE-1443 at this ticket for the two majors, so the deferred item carries the security
      reason and not only the maintenance one — that is a stronger argument for eventually doing it.
- [ ] Record the audit numbers in the ticket so the next Steward pass has a baseline to diff against
      rather than re-deriving one.

## Notes

Filed 2026-08-27 by the sprint Foreman during a Dependency Steward pass. `cargo audit` was
deliberately **not** run in the same pass — the machine was at 83–99% CPU with six concurrent build
agents and a fresh audit build would have slowed every one of them. Rust advisories are tracked
separately: **CPE-1820** (bump russh off 0.54 for two high-severity advisories) is in
`Ticketing/Tickets/Blocked/`, and **CPE-1442** tracks the RSA Marvin attack.

Related: **CPE-1443** (deferred toolchain majors — owns vite 8 and vitest 4), **CPE-1904**
(package-lock drift backstop), **CPE-1820**, **CPE-1442**.
