---
id: CPE-393
title: "AI Console: actionable first-run guide (do the steps, don't just list them)"
type: Feature
status: Done
priority: High
component: Frontend
tags: [ready]
created: 2026-07-14
closed: 2026-07-14
---

## Summary

The first-run overlay lists steps but they aren't actionable, and the language assumes knowledge.
Turn it into a plain-language guide with buttons that DO the steps: **Add an API key** (opens Keys…)
or use a tool's built-in login, then pick a tool + Launch. Tailor to state (no keys yet → emphasise
adding one or a built-in login).

## Acceptance Criteria
- [x] Overlay copy is plain-language + accurate to the new labels (tool/provider/key/setup).
- [x] An "Add an API key" action opens the Keys panel directly from the guide.
- [x] A "use a built-in login instead" path for agents that support native login (no key).
- [x] Shown once (persists via /api/onboarded); a "?" reopens the full Help. Harness test the wiring.

## Work Log
2026-07-14 — Filed: inexperienced-user goal, part 2 (guided start).
