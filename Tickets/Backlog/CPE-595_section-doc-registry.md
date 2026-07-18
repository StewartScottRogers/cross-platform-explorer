---
id: CPE-595
title: "Section→doc registry (one source of truth) + exhaustiveness guard test"
type: Feature
status: Open
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-579
estimate: 1h
created: 2026-07-17
---

## Summary
A single pure module mapping every user-facing section/mode id to a doc slug, driving both the
contextual open ([[CPE-596]]) and the "is every section documented?" check. One source of truth — no
slug strings scattered across components.

## Decisions (from activation)
- **Keying:** reuse the app's existing mode/view id enum as the registry key (no parallel section id).
- **Coverage:** every surface — all modes **and** the base surfaces (Home, Explorer).

## Acceptance Criteria
- [ ] A pure `sectionDocs` module maps each mode/view id → a doc slug from `src/docs/`:
      AI Console→`04-ai-console`, Workbench→`07-workbench`, Board→`06-agent-board`, Grid→`05-agent-grid`,
      Swarms→`09-swarms`, Repositories→`08-repositories`, Explorer→`03-explorer`, Home→`01-overview`
      (adjust slugs to the actual `DOCS` ids).
- [ ] A resolver `docSlugForSection(id) -> slug` returns the mapped slug, falling back to the default
      when unmapped (graceful in prod).
- [ ] **Guard test** (alongside `docs.test.ts`): asserts every mode/view id has a registry entry
      (exhaustive over sections) **and** every mapped slug exists in `DOCS` (no dangling slugs) — adding a
      section without its doc, or a typo'd slug, fails CI.
- [ ] `npm run check` + the full suite green.

## Notes
The exhaustiveness test is the enforcement mechanism behind [[CPE-597]]'s self-maintaining rule.
