---
id: CPE-394
title: "AI Console: launch-readiness — warn when a provider needs a key and none is set"
type: Feature
status: Done
priority: High
component: Frontend
tags: [ready]
created: 2026-07-14
closed: 2026-07-14
---

## Summary

The commonest dead-end: pick a paid provider (OpenRouter/Anthropic) with no saved key, click Launch,
get a confusing failure. Detect it up front and guide the fix — and prefer a no-key provider as the
default for a first-timer so they can launch immediately.

## Acceptance Criteria
- [x] `providerNeedsKey()` (not native / not a local provider) + `haveKeyFor()` (saved credential or
      typed key).
- [x] On provider change / load: if it needs a key and none is set, a clear inline hint with a
      "Add a key" action (or "pick a built-in-login provider").
- [x] Default provider prefers one that needs no key when the user has no keys yet.
- [x] Harness tests: hint shows/hides correctly; no-key provider chosen as default when keyless.

## Work Log
2026-07-14 — Filed: inexperienced-user goal, part 3 (no dead-ends).
