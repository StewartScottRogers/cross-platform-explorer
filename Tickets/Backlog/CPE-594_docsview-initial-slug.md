---
id: CPE-594
title: "DocsView takes an optional initial slug (select that doc on mount)"
type: Feature
status: Open
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-579
estimate: 30m
created: 2026-07-17
---

## Summary
Give `DocsView.svelte` one optional input — the doc slug to open — so a caller can land it on a specific
page. The viewer stays a dumb, reusable panel; *which* slug to pass lives in the caller/registry
([[CPE-595]]).

## Acceptance Criteria
- [ ] `DocsView` accepts an optional `initialSlug` prop; on mount it selects that doc if it exists in
      `DOCS`, else falls back to `DOCS[0]` (today's behaviour). Scrolls to top.
- [ ] `App.svelte`'s `showDocs` state carries an optional slug and passes it to `<DocsView>`; existing
      callers that pass nothing are unchanged.
- [ ] Unit test: `initialSlug` selects the matching doc; an unknown/absent slug falls back to the default.
- [ ] `npm run check` clean.

## Notes
Prereq for the contextual-open work. Keep the prop the *only* new viewer concept (design principle:
orthogonal to the viewer).
