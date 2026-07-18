---
id: CPE-597
title: "Self-maintaining docs discipline: new feature ⇒ doc page + registry entry (CLAUDE.md rule)"
type: Task
status: Open
priority: Medium
component: Docs
tags: [ready]
epic: CPE-579
estimate: 30m
created: 2026-07-17
---

## Summary
The standing ask: contextual help must never drift behind the app. Fold "every new feature that earns a
section ships its doc page **and** registers its section→doc mapping" into the project rules, backed by
the exhaustiveness guard test ([[CPE-595]]).

## Acceptance Criteria
- [ ] `CLAUDE.md` gains a rule (near the docs-library guidance): a feature that adds a user-facing
      section must add/update its `src/docs/` page **and** its entry in the section→doc registry; the
      guard test enforces it.
- [ ] The in-app **Documents** library is updated so its own pages describe contextual help (open a
      section's page from its "?" / F1) — per [[maintain-in-app-docs-library]].
- [ ] Cross-reference the registry + guard test from the docs-library rule so the enforcement is
      discoverable.

## Notes
Closes the loop that makes CPE-579 self-maintaining rather than a one-off wiring.
