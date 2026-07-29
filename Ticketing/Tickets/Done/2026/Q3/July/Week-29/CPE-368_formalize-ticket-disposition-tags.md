---
id: CPE-368
title: "Formalize ticket disposition tags (vocabulary + always-apply + display)"
type: Task
status: Done
priority: Medium
component: Docs
estimate: 1h
created: 2026-07-14
closed: 2026-07-14
tags: [ready]
---

## Summary

Ad-hoc labels ("big-design", "resource-blocked", "needs Mac/Linux") were used when triaging why a
ticket isn't being worked. Formalize them into a controlled `tags:` vocabulary carried in ticket
frontmatter, applied to every ticket, defined in the workflow docs, and shown as a column whenever
tickets are listed.

## Acceptance Criteria

- [x] `tags:` frontmatter field defined in `Tickets/wiki.md` with a fixed vocabulary + rules.
- [x] `/ticketing-new` writes a `tags:` line and assigns a disposition tag at creation.
- [x] `/ticketing-list` and the CLAUDE.md "Showing open tickets" contract render a **Tags** column
      for Backlog, Blocked, and Doing.
- [x] All Backlog + Blocked tickets back-tagged.
- [x] A memory records the standard so it's applied every session.

## Work Log
2026-07-14 — Filed after the user asked to formalize the ad-hoc triage labels.
