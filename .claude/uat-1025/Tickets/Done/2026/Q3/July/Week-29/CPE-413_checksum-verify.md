---
id: CPE-413
title: "Verify a file checksum against an expected value"
type: Feature
status: Done
priority: Medium
component: Frontend
tags: [ready]
estimate: 30m
created: 2026-07-15
closed: 2026-07-15
---

## Summary

Computing a SHA-256 (CPE-412) is only half the workflow — the real goal is **verifying** a download
against a published hash. Add an "expected" input to the Properties checksum row: paste the hash from
a website and get a live **✓ Match / ✗ No match** verdict (case-insensitive, whitespace-tolerant), so
a user can confirm a file's integrity without eyeballing 64 hex chars.

Nightshift research loop 2. Builds directly on CPE-412; the comparison is pure, headless-testable
logic.

## Acceptance Criteria

- [x] After computing the SHA-256, an input lets the user paste an expected digest; the verdict
      updates live.
- [x] Comparison is case-insensitive and ignores surrounding whitespace; an empty expected value
      shows no verdict (neutral).
- [x] Match shows a clear ✓; mismatch a clear ✗ — distinguishable without relying on colour alone.
- [x] Pure `checksumMatches()` helper is unit-tested; `npm run check` clean, JS suite green.

## Work Log
2026-07-15 — Nightshift loop 2. Estimate: 30m. Plan: a pure `checksumMatches(computed, expected)`
helper (normalize: trim/lowercase/strip inner whitespace) + test, wired into `PropertiesDialog` under
the computed digest. Verify headlessly; GUI left for the user.

2026-07-15 — Done. `src/lib/checksum.ts` (`normalizeDigest` + `checksumMatches`, neutral-on-empty) + test; wired an expected-hash input + live ✓ Match / ✗ No match verdict (glyphs, not colour-only) into `PropertiesDialog`. Verified: checksum 4 + properties 3 tests, `npm run check` 0/0, `npm test` **399 passed**.

## Resolution
Completes the checksum workflow: verify a file against a published SHA-256 (case-insensitive, whitespace-tolerant incl. space-grouped digests). Pure comparison helper is unit-tested; UI is a small additive row in the dialog from CPE-412.
