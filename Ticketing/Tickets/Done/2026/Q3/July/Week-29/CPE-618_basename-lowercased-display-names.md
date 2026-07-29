---
id: CPE-618
title: "Display names were lowercased (recent-locations palette + agent-activity chips)"
type: Bug
component: Frontend
priority: low
status: Done
tags: ready
estimate: 15m
created: 2026-07-18
closed: 2026-07-18
---

## Summary
`App.svelte`'s `baseNameOf` derived a display name via `normalizePath(p)` — but that helper (from
`agentSessions.ts`) **lowercases** paths (correct for Agent-Watch case-insensitive matching, wrong for
display). So two user-visible spots rendered names in all-lowercase:
- the command palette's **Recent locations** labels (CPE-604) — "Documents" showed as "documents";
- the **Agent Watch activity chips** (CPE-401) — "App.svelte" showed as "app.svelte".

## Acceptance Criteria
- [x] Recent-location and agent-activity names render with their real case.
- [x] Uses the existing case-preserving `baseName` (contentSearch.ts) instead of the lowercasing helper.
- [x] A regression test asserts `baseName` preserves case.
- [x] `npm run check` clean; frontend suite green.

## Resolution
Removed the local lowercasing `baseNameOf` and used the tested, case-preserving `baseName` at both call
sites (`normalizePath` is still imported — it's legitimately used for case-insensitive cwd comparison).
Added a case-preservation assertion to `contentSearch.test.ts`.

## Work Log
2026-07-18 (Nightshift) — Found by auditing path-handling helpers; the lowercasing normalizePath was
being reused for display. Latent for the agent chips; I'd propagated it into CPE-604's palette labels.
