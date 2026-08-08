---
id: CPE-1441
title: "Security: bump dompurify 3.4.12→3.4.13 (GHSA-55q2-fjhq-7xh7, moderate XSS)"
type: Bug
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-534
created: 2026-08-07
---
## Problem (found by the shift-1 dependency audit)
`dompurify@3.4.12` — **GHSA-55q2-fjhq-7xh7** (moderate): an `IN_PLACE` hook removal leaves a detached subtree
executable → XSS. `npm audit --production` flags it (dompurify ships in the bundle — it sanitizes rendered
markdown/HTML). Fix is available and **non-breaking**: 3.4.13 already satisfies `package.json`'s `^3.4.12`.

## Fix
Update dompurify to 3.4.13 (`npm update dompurify` or bump the lockfile entry) and commit the updated
`package-lock.json`. Since `^3.4.12` already allows 3.4.13, this is a lockfile bump only — no `package.json`
range change needed.

## Acceptance
- `package-lock.json` resolves dompurify ≥3.4.13; `npm audit --production` no longer reports GHSA-55q2-fjhq-7xh7.
- Wherever dompurify is used (grep `dompurify` / `DOMPurify` — markdown/HTML sanitization path) still works;
  `npm run check` + `npx vitest run` green.

## Notes
Dependency Steward finding, shift-1 audit 2026-08-07. Trivial non-breaking patch bump. Good candidate to batch
with CPE-1440 in one "dependency security bumps" PR.
