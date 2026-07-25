---
id: CPE-873
title: Document the Integrity Guard in the in-app docs
type: docs
component: Frontend
priority: low
status: Done
tags: ready
epic: CPE-737
created: 2026-07-21
closed: 2026-07-21
---

## Summary
The integrity guard (CPE-737: baseline, verify, bitrot classification, verify-all, verify-on-startup) had
no in-app docs page. Add one so users understand baselining, the five result classes, silent-corruption
detection, and the opt-in monitoring.

## Acceptance Criteria
- [x] `src/docs/14-integrity.md` covers baseline/verify/rebaseline, the intact/edited/corrupted/missing/new
      classes, the bitrot heuristic, verify-all, and opt-in verify-on-startup; auto-included via the glob.
- [x] `npm run check` + docs guard tests green.

## Work Log
- 2026-07-21 (autonomous) — Added the doc page (category "Explorer"). Self-maintaining-docs rule.
