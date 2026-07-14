---
id: CPE-384
title: "Verify CPE-296 consent gate + correct the stale threat-model sign-off record"
type: Task
status: Done
priority: High
component: Docs
tags: [ready]
estimate: 30m
created: 2026-07-14
closed: 2026-07-14
---

## Summary

While assessing the CPE-382 ship-gate, found the threat model still marked CPE-296 (capability
consent UX) as ⛔/"not built" in 6 places — but CPE-296 has been Done since 2026-07-13 (consent
sheet + per-cap grant/deny + re-prompt + revoke via CPE-274; broker-enforced, unit-tested). The
security doc misrepresented the platform's readiness. Verified the reality and corrected the record.

## What was done

- [x] Verified enforcement: host consent tests (8) + `decide_grants` deny-secrets test pass;
      ConsentSheet/SidecarManager present; `npm run check` 0/0.
- [x] `docs/security/threat-model.md`: consent-integrity, §4 EoP, the "no unconsented code
      execution" invariant, the header sign-off note, and §10 gaps all corrected to ✅ / DONE.
      Sign-off gate reduced to **CPE-322** (cross-OS keychains) + a final review.
- [x] Updated CPE-382 (ship-gate) + CPE-304 (sign-off milestone) notes; noted verification on CPE-296.

## Notes
No code change — CPE-296 was already complete; this corrects documentation that would otherwise keep
the platform looking blocked on an axis that's actually closed. Remaining sign-off blocker: CPE-322.

## Work Log
2026-07-14 — Verified + corrected.
