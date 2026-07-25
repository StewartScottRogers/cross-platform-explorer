---
id: CPE-239
title: 100 folder-content recognizers with "open in external app" actions
type: Feature
status: Done
priority: Medium
component: Frontend
estimate: 3-4h
created: 2026-07-12
closed: 2026-07-12
---

## Summary

Extend the folder-context system (CPE-235) to ~100 recognizers covering common
project/folder types — IDEs, build systems, language ecosystems, web frameworks,
game engines, containers/DevOps, VCS, docs/data. Each detects its marker file(s)
cheaply from the folder listing and offers a one-click action to open the folder
in the appropriate external application (via the OS default handler / associated
app). Multiple contexts still aggregate into the context bar.

## Acceptance Criteria
- [ ] Data-driven registry (~100 entries) replacing the hand-written providers.
- [ ] Each entry: id, label, icon, marker match (files/exts/dirs), open action.
- [ ] Matching is cheap (name lookups + one ext pass), no deep scan.
- [ ] Multiple matches aggregate; Git keeps its "Open on GitHub" action.
- [ ] `npm run check` + tests pass.

## Resolution

*(Agent writes this when closing)*

## Work Log

*(Agent appends dated entries here)*

## Notes
"Open in external app" = openPath on the marker file (or the folder), which the OS
routes to the associated app (e.g. .sln → Visual Studio, .uproject → Unreal).
Some app associations only exist where that app is installed; unknown types fall
back to the default handler.

### filled
Rewrote folderContext.ts as a data-driven registry of ~129 recognizers (VCS,
IDEs, build systems, language ecosystems, JS monorepo tooling, web frameworks,
docs generators, game engines, containers/DevOps, CI, mobile, data/ML/hardware).
Each: marker match (files/exts/dirs, with `unless` guard) + an "Open in <app>"
action (open-path on the marker, or open-github for Git). detectContexts builds
name/dir maps once then matches; aggregates all hits. Added folderContext.test.ts
(6 tests). check 0/0; 237 tests pass. Ships in 0.10.2.
