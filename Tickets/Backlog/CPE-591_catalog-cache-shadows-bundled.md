---
id: CPE-591
title: "Auto-updated agent catalog shadows the newer bundled catalog (stale manifests)"
type: Bug
status: Open
priority: Medium
component: Sidecar
tags: [ready]
epic: CPE-528
estimate: 2-3h
created: 2026-07-17
---

## Summary
The sidecar loads a downloaded catalog from `AppData/Roaming/…/ai-console-catalog/` that takes
precedence over the **bundled** catalog shipped in the installer. When the app ships newer agent
manifests than the last published catalog release (as with CPE-583's `swarm` recipe), the stale
download **shadows** them — features silently regress to the older data. Surfaced by [[CPE-590]].

## Fix options
- Version the catalog and prefer whichever of {bundled, downloaded} is newer.
- Invalidate / refresh the downloaded cache on app-version change (installer or first run).
- Merge manifests field-wise (bundled fills gaps the download lacks).

## Acceptance Criteria
- [ ] After installing a build whose bundled catalog is newer than the cached download, the sidecar uses
      the newer manifests (e.g. the `swarm` recipe + any new providers), without manual cache deletion.
- [ ] A genuinely newer *downloaded* catalog still wins over an older bundled one (auto-update intact).
